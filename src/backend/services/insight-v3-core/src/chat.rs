//! The chat client: turns a message into either a one-time answer or a set
//! of metric/widget/dashboard definitions to store.

use std::time::Duration;

use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::metric_query::{MetricQuery, MetricQueryError};

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const CHAT_TIMEOUT_SECS: u64 = 30;
const CHAT_MAX_TOKENS: u32 = 2048;

/// One of the two things the model can propose in reply to a chat message.
#[derive(Debug)]
pub(crate) enum Proposal {
    /// A one-time question. The service runs `query` and answers; nothing is stored.
    Answer { reply: String, query: MetricQuery },
    /// A metric/widget/dashboard to store. Any of the three may be absent.
    Create {
        reply: String,
        metric: Option<(String, Value)>,
        widgets: Vec<(String, Value)>,
        dashboard: Option<(String, Value)>,
    },
}

impl Proposal {
    /// Strips any prose or code fence around the JSON object, deserializes on
    /// `intent`, and compiles every query and every proposed metric with
    /// [`MetricQuery::compile`] — a refusal is [`ChatError::Metric`], and
    /// nothing runs or is stored.
    pub(crate) fn parse(reply: &str) -> Result<Self, ChatError> {
        let wire: ProposalWire = serde_json::from_str(extract_json_object(reply))?;

        Ok(match wire {
            ProposalWire::Answer { reply, query } => {
                query.compile()?;
                Self::Answer { reply, query }
            }
            ProposalWire::Create {
                reply,
                metric,
                widgets,
                dashboard,
            } => Self::Create {
                reply,
                metric: metric.map(compile_named_metric).transpose()?,
                widgets: widgets.into_iter().map(NamedBody::into_pair).collect(),
                dashboard: dashboard.map(NamedBody::into_pair),
            },
        })
    }
}

fn compile_named_metric(named: NamedBody) -> Result<(String, Value), ChatError> {
    let metric: MetricQuery = serde_json::from_value(named.body.clone())?;
    metric.compile()?;
    Ok(named.into_pair())
}

fn extract_json_object(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end >= start => &text[start..=end],
        _ => text,
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "intent", rename_all = "lowercase")]
enum ProposalWire {
    Answer {
        reply: String,
        query: MetricQuery,
    },
    Create {
        reply: String,
        #[serde(default)]
        metric: Option<NamedBody>,
        #[serde(default)]
        widgets: Vec<NamedBody>,
        #[serde(default)]
        dashboard: Option<NamedBody>,
    },
}

#[derive(Debug, Deserialize)]
struct NamedBody {
    name: String,
    body: Value,
}

impl NamedBody {
    fn into_pair(self) -> (String, Value) {
        (self.name, self.body)
    }
}

#[derive(Debug, Error)]
pub(crate) enum ChatError {
    #[error("the model reply was not valid JSON")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Metric(#[from] MetricQueryError),
    #[error("the key was rejected upstream")]
    TokenRejected,
    #[error("the model is unavailable right now")]
    Unavailable,
    #[error("the model did not answer in time")]
    Timeout,
    #[error("the model call failed")]
    Failed,
}

/// Proposes an answer or a creation from a chat message. `canned()` never
/// makes a network call — it is used when `chat_mode: canned` is configured,
/// so recordings and tests never depend on a live model.
#[derive(Debug, Clone)]
pub(crate) struct ChatClient {
    backend: ChatBackend,
}

#[derive(Debug, Clone)]
enum ChatBackend {
    Live {
        http: reqwest::Client,
        token: SecretString,
        model: String,
    },
    Canned,
    #[cfg(test)]
    Scripted(fn() -> Proposal),
}

impl ChatClient {
    pub(crate) fn new(token: &SecretString, model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(CHAT_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|error| panic!("the HTTP client must build: {error}"));

        Self {
            backend: ChatBackend::Live {
                http,
                token: token.clone(),
                model,
            },
        }
    }

    pub(crate) fn canned() -> Self {
        Self {
            backend: ChatBackend::Canned,
        }
    }

    /// Always answers with a fixed [`Proposal`] built by `build`, making no
    /// network call — lets a test drive `handle_chat` with a proposal shape
    /// of its own choosing, independent of `canned()`'s fixed shape.
    #[cfg(test)]
    pub(crate) fn scripted(build: fn() -> Proposal) -> Self {
        Self {
            backend: ChatBackend::Scripted(build),
        }
    }

    /// # Errors
    ///
    /// Returns [`ChatError`] describing what the upstream did, or why the
    /// reply could not be turned into a [`Proposal`].
    pub(crate) async fn propose(
        &self,
        message: &str,
        tables: &[String],
    ) -> Result<Proposal, ChatError> {
        match &self.backend {
            ChatBackend::Canned => Ok(canned_proposal(message)),
            #[cfg(test)]
            ChatBackend::Scripted(build) => Ok(build()),
            ChatBackend::Live { http, token, model } => {
                let system = system_prompt(tables);
                let body = MessagesRequest {
                    model,
                    max_tokens: CHAT_MAX_TOKENS,
                    system: &system,
                    messages: vec![Message {
                        role: "user",
                        content: message,
                    }],
                };

                let response = http
                    .post(format!("{ANTHROPIC_API_BASE}/v1/messages"))
                    .header("x-api-key", token.expose_secret())
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|error| transport_error(&error))?;

                let status = response.status();
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    return Err(ChatError::TokenRejected);
                }
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                    tracing::warn!(status = %status, "the model call was refused upstream");
                    return Err(ChatError::Unavailable);
                }
                if !status.is_success() {
                    tracing::error!(status = %status, "the model call failed upstream");
                    return Err(ChatError::Failed);
                }

                let parsed: MessagesResponse = response.json().await.map_err(|error| {
                    tracing::error!(error = %error, "the model answer could not be read");
                    ChatError::Failed
                })?;

                Proposal::parse(&parsed.text())
            }
        }
    }
}

fn transport_error(error: &reqwest::Error) -> ChatError {
    if error.is_timeout() {
        return ChatError::Timeout;
    }
    tracing::error!(error = %error, "the model could not be reached");
    ChatError::Failed
}

fn system_prompt(tables: &[String]) -> String {
    let mut prompt = String::from(
        "You are the Insight v3 chat assistant. Reply with exactly one JSON object and nothing else.\n\n\
         Two intents are possible:\n\
         - {\"intent\":\"answer\",\"reply\":<string>,\"query\":<MetricQuery>} answers a one-time question by running a query. Nothing is stored.\n\
         - {\"intent\":\"create\",\"reply\":<string>,\"metric\":{\"name\":<string>,\"body\":<MetricQuery>}|null,\"widgets\":[{\"name\":<string>,\"body\":<widget>}],\"dashboard\":{\"name\":<string>,\"body\":<dashboard>}|null} builds definitions to store.\n\n\
         A MetricQuery is {\"table\":<string>,\"fields\":[{\"json\":<string>,\"type\":\"string\"|\"int\"|\"float\",\"agg\":\"count\"|\"sum\"|\"avg\"|\"min\"|\"max\"|null,\"as_name\":<string>}],\"group_by\":[<string>],\"filters\":[{\"json\":<string>,\"type\":<field type>,\"op\":\"eq\"|\"ne\"|\"gt\"|\"gte\"|\"lt\"|\"lte\",\"value\":<value>}],\"limit\":<int>|null}.\n\
         A table widget is {\"type\":\"table\",\"metric\":<metric name>,\"columns\":[<string>]}. A line widget is {\"type\":\"line\",\"metric\":<metric name>,\"x\":<string>,\"y\":<string>}. A dashboard is {\"title\":<string>,\"widgets\":[<widget name>]}.\n",
    );

    if tables.is_empty() {
        prompt.push_str("\nNo tables are known yet.\n");
    } else {
        prompt.push_str("\nKnown tables and their fields:\n");
        for table in tables {
            prompt.push_str("- ");
            prompt.push_str(table);
            prompt.push('\n');
        }
    }

    prompt
}

fn canned_proposal(message: &str) -> Proposal {
    let word = message.split_whitespace().next().unwrap_or("chat");
    let metric_name = format!("{word}_metric");
    let line_widget_name = format!("{word}_line");
    let dashboard_name = format!("{word}_dashboard");

    let metric_body = json!({
        "table": "events",
        "fields": [
            { "json": "day", "type": "string", "as_name": "day" },
            { "json": "lines", "type": "int", "agg": "sum", "as_name": "lines" }
        ],
        "group_by": ["day"],
        "filters": []
    });
    let table_widget_body = json!({
        "type": "table",
        "metric": metric_name,
        "columns": ["day", "lines"]
    });
    let line_widget_body = json!({
        "type": "line",
        "metric": metric_name,
        "x": "day",
        "y": "lines"
    });
    let dashboard_body = json!({
        "title": word,
        "widgets": [word, line_widget_name]
    });

    Proposal::Create {
        reply: format!("Here's a starter dashboard for \"{word}\"."),
        metric: Some((metric_name, metric_body)),
        widgets: vec![
            (word.to_owned(), table_widget_body),
            (line_widget_name, line_widget_body),
        ],
        dashboard: Some((dashboard_name, dashboard_body)),
    }
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

impl MessagesResponse {
    fn text(&self) -> String {
        self.content
            .iter()
            .filter(|block| block.kind == "text")
            .map(|block| block.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_answer_intent_carries_a_query_and_stores_nothing() {
        let reply = r#"{"intent":"answer","reply":"About 59 lines on the first day","query":{"table":"events","fields":[{"json":"day","type":"string","as_name":"day"},{"json":"lines","type":"int","agg":"sum","as_name":"lines"}],"group_by":["day"],"filters":[]}}"#;

        match Proposal::parse(reply).unwrap_or_else(|error| panic!("parses: {error}")) {
            Proposal::Answer { reply, query } => {
                assert_eq!(reply, "About 59 lines on the first day");
                query
                    .compile()
                    .unwrap_or_else(|error| panic!("the query compiles: {error}"));
            }
            Proposal::Create { .. } => panic!("expected an answer"),
        }
    }

    #[test]
    fn a_create_intent_is_read_out_of_the_model_reply() {
        let reply = r#"{"intent":"create","reply":"Here you go","metric":{"name":"commits_per_day","body":{"table":"events","fields":[{"json":"day","type":"string","as_name":"day"}],"group_by":["day"],"filters":[]}},"widgets":[{"name":"commits_table","body":{"type":"table","metric":"commits_per_day","columns":["day"]}}],"dashboard":{"name":"engineering","body":{"title":"Engineering","widgets":["commits_table"]}}}"#;

        match Proposal::parse(reply).unwrap_or_else(|error| panic!("parses: {error}")) {
            Proposal::Create { reply, widgets, .. } => {
                assert_eq!(reply, "Here you go");
                assert_eq!(widgets.len(), 1);
            }
            Proposal::Answer { .. } => panic!("expected a creation"),
        }
    }

    #[test]
    fn a_metric_the_compiler_refuses_is_not_stored() {
        let reply = r#"{"intent":"create","reply":"x","metric":{"name":"bad","body":{"table":"events`--","fields":[],"group_by":[],"filters":[]}},"widgets":[],"dashboard":null}"#;

        assert!(matches!(Proposal::parse(reply), Err(ChatError::Metric(_))));
    }

    #[test]
    fn prose_around_the_json_is_tolerated() {
        let reply = "Sure!\n```json\n{\"intent\":\"create\",\"reply\":\"ok\",\"widgets\":[],\"dashboard\":null}\n```";

        assert!(matches!(
            Proposal::parse(reply),
            Ok(Proposal::Create { .. })
        ));
    }

    #[test]
    fn an_answer_with_a_field_outside_a_selected_group_by_is_a_metric_refusal() {
        let reply = json!({
            "intent": "answer",
            "reply": "x",
            "query": {
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
                "group_by": ["not_selected"],
                "filters": []
            }
        })
        .to_string();

        assert!(matches!(
            Proposal::parse(&reply),
            Err(ChatError::Metric(MetricQueryError::Identifier(_)))
        ));
    }

    #[tokio::test]
    async fn canned_mode_names_everything_after_the_messages_first_word() {
        let client = ChatClient::canned();

        let proposal = client
            .propose("commits_table", &[])
            .await
            .unwrap_or_else(|error| panic!("canned mode never fails: {error}"));

        match proposal {
            Proposal::Create {
                metric,
                widgets,
                dashboard,
                ..
            } => {
                assert_eq!(
                    metric.map(|(name, _)| name),
                    Some("commits_table_metric".to_owned())
                );
                assert_eq!(widgets.len(), 2);
                assert_eq!(widgets[0].0, "commits_table");
                assert_eq!(
                    dashboard.map(|(name, _)| name),
                    Some("commits_table_dashboard".to_owned())
                );
            }
            Proposal::Answer { .. } => panic!("canned mode always creates"),
        }
    }
}
