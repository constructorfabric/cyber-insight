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
const ANSWER_TOOL: &str = "answer";
const CREATE_TOOL: &str = "create";
/// Definition names: what `DefinitionName::parse` accepts.
const NAME_PATTERN: &str = "^[A-Za-z0-9_-]{1,128}$";

/// A table data has been ingested into, and what is in it.
#[derive(Debug, Clone)]
pub(crate) struct KnownTable {
    pub(crate) name: String,
    /// `day (string), lines (int)`, empty until something has landed.
    pub(crate) fields: String,
}

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
    /// [`Proposal::parse`], then refuse a table the reader does not have.
    ///
    /// Asked what data exists, the model reaches for `information_schema` and
    /// friends; the charset check passes such a name and the query then fails
    /// in the database, which surfaced as an internal error. The refusal goes
    /// back through the repair round, so the model gets the real table list.
    /// With no known tables at all the check stands aside — refusing
    /// everything would be worse than the guess.
    pub(crate) fn checked(reply: &str, known: &[KnownTable]) -> Result<Self, ChatError> {
        let proposal = Self::parse(reply)?;

        if known.is_empty() {
            return Ok(proposal);
        }

        match proposal.table() {
            Some(table) if !known.iter().any(|entry| entry.name == table) => {
                Err(ChatError::UnknownTable {
                    table: table.to_owned(),
                    known: known
                        .iter()
                        .map(|entry| entry.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                })
            }
            _ => Ok(proposal),
        }
    }

    /// The table this proposal reads, when it names one.
    fn table(&self) -> Option<&str> {
        match self {
            Self::Answer { query, .. } => Some(query.table()),
            Self::Create { metric, .. } => metric
                .as_ref()
                .and_then(|(_, body)| body.get("table").and_then(Value::as_str)),
        }
    }

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
            } => {
                if metric.is_none() && widgets.is_empty() && dashboard.is_none() {
                    return Err(ChatError::EmptyCreate);
                }

                Self::Create {
                    reply,
                    metric: metric.map(compile_named_metric).transpose()?,
                    widgets: widgets.into_iter().map(NamedBody::into_pair).collect(),
                    dashboard: dashboard.map(NamedBody::into_pair),
                }
            }
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

impl ChatError {
    /// What the repair round tells the model. `Display` on the JSON variant
    /// names a category; the serde message names the offending field.
    fn feedback(&self) -> String {
        match self {
            Self::Json(error) => error.to_string(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum ChatError {
    #[error("the model reply was not valid JSON")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Metric(#[from] MetricQueryError),
    #[error("there is no table named `{table}`; the tables are: {known}")]
    UnknownTable { table: String, known: String },
    #[error("a create must carry at least one metric, widget or dashboard")]
    EmptyCreate,
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
        tables: &[KnownTable],
    ) -> Result<Proposal, ChatError> {
        match &self.backend {
            ChatBackend::Canned => Ok(canned_proposal(message)),
            #[cfg(test)]
            ChatBackend::Scripted(build) => Ok(build()),
            ChatBackend::Live { http, token, model } => {
                let system = system_prompt(tables);
                let first = call_model(http, token, model, &system, message).await?;

                match Proposal::checked(&first, tables) {
                    Ok(proposal) => Ok(proposal),
                    // One repair round: hand the model its own rejection and
                    // let it correct itself. The schema stops malformed
                    // arguments; this catches what only our own validation
                    // knows — an unknown table, a field that is not there.
                    Err(rejection) => {
                        let detail = rejection.feedback();
                        tracing::info!(rejection = %detail, "asking the model to correct its proposal");
                        let retry = format!(
                            "{message}\n\nYour previous proposal was rejected: {detail}\nIt was:\n{first}\nReturn a corrected proposal."
                        );
                        let second = call_model(http, token, model, &system, &retry).await?;
                        Proposal::checked(&second, tables)
                    }
                }
            }
        }
    }
}

/// One forced tool call, returning the proposal JSON the model produced.
async fn call_model(
    http: &reqwest::Client,
    token: &SecretString,
    model: &str,
    system: &str,
    message: &str,
) -> Result<String, ChatError> {
    let body = MessagesRequest {
        model,
        max_tokens: CHAT_MAX_TOKENS,
        system,
        messages: vec![Message {
            role: "user",
            content: message,
        }],
        tools: proposal_tools(),
        tool_choice: json!({ "type": "any" }),
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
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
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

    Ok(parsed.proposal_json())
}

fn transport_error(error: &reqwest::Error) -> ChatError {
    if error.is_timeout() {
        return ChatError::Timeout;
    }
    tracing::error!(error = %error, "the model could not be reached");
    ChatError::Failed
}

fn system_prompt(tables: &[KnownTable]) -> String {
    let mut prompt = String::from(
        "You are the Insight v3 chat assistant. Answer by calling exactly one tool.\n\n\
         - Call `answer` to answer a question: it runs one query and stores nothing.\n\
         - Call `create` to build definitions to store. Pass the metric, the widgets and the dashboard as {\"name\":<string>,\"body\":<object>}, where the name is the identifier and the body is the definition. A create that carries none of the three is refused, and a dashboard needs the metric and widgets it draws.\n\n\
         A MetricQuery is {\"table\":<string>,\"fields\":[{\"json\":<string>,\"type\":\"string\"|\"int\"|\"float\",\"agg\":\"count\"|\"sum\"|\"avg\"|\"min\"|\"max\"|null,\"as_name\":<string>}],\"group_by\":[<string>],\"filters\":[{\"json\":<string>,\"type\":<field type>,\"op\":\"eq\"|\"ne\"|\"gt\"|\"gte\"|\"lt\"|\"lte\",\"value\":<value>}],\"limit\":<int>|null}.\n\
         Every group_by entry must be spelled exactly like the as_name of a field in the same query.\n\
         A table widget is {\"type\":\"table\",\"metric\":<metric name>,\"columns\":[<string>]}. A line widget is {\"type\":\"line\",\"metric\":<metric name>,\"x\":<string>,\"y\":<string>}. A dashboard is {\"title\":<string>,\"widgets\":[<widget name>]}.\n",
    );

    if tables.is_empty() {
        prompt.push_str("\nNo tables are known yet.\n");
    } else {
        prompt.push_str("\nKnown tables:\n");
        for table in tables {
            prompt.push_str("- ");
            prompt.push_str(&table.name);
            if !table.fields.is_empty() {
                prompt.push_str(": ");
                prompt.push_str(&table.fields);
            }
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
    tools: Vec<Value>,
    tool_choice: Value,
}

/// The structured query a metric carries. Shared by both tools: the
/// answer tool runs one, the create tool stores one.
fn metric_query_schema() -> Value {
    let plain = json!({ "type": "string" });
    let field_type = json!({ "enum": ["string", "int", "float"] });

    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["table", "fields", "group_by", "filters"],
        "properties": {
            "table": plain,
            "fields": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["json", "type", "as_name"],
                    "properties": {
                        "json": plain,
                        "type": field_type,
                        "agg": { "enum": ["count", "sum", "avg", "min", "max"] },
                        "as_name": plain,
                    },
                },
            },
            "group_by": { "type": "array", "items": plain },
            "filters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["json", "type", "op", "value"],
                    "properties": {
                        "json": plain,
                        "type": field_type,
                        "op": { "enum": ["eq", "ne", "gt", "gte", "lt", "lte"] },
                        "value": { "type": ["string", "number", "boolean"] },
                    },
                },
            },
            "limit": { "type": "integer" },
        },
    })
}

/// The two tools the model may call. The tool it picks IS the intent, so a
/// question cannot be mistaken for a creation. The schemas guide the shape and
/// document the name charset. `strict` is deliberately NOT set: the nested
/// [`MetricQuery`] shape exceeds the API's compiled-grammar budget and a strict
/// request is refused outright ("the compiled grammar is too large"). What the
/// schema cannot enforce, our own validation refuses and the repair round fixes.
fn proposal_tools() -> Vec<Value> {
    let plain = json!({ "type": "string" });
    let name = json!({
        "type": "string",
        "pattern": NAME_PATTERN,
        "description": "letters, digits, underscore and dash only - never a space",
    });
    let metric_query = metric_query_schema();

    let widget = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["type", "metric"],
        "properties": {
            "type": { "enum": ["table", "line"] },
            "metric": name,
            "columns": { "type": "array", "items": plain },
            "x": plain,
            "y": plain,
        },
    });
    let dashboard = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "widgets"],
        "properties": {
            "title": { "type": "string" },
            "widgets": { "type": "array", "items": name },
        },
    });
    let named = |body: Value| {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["name", "body"],
            "properties": { "name": name, "body": body },
        })
    };

    vec![
        json!({
            "name": ANSWER_TOOL,
            "description": "Answer a question about the data by running one query. Stores nothing.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["reply", "query"],
                "properties": { "reply": { "type": "string" }, "query": metric_query.clone() },
            },
        }),
        json!({
            "name": CREATE_TOOL,
            "description": "Build metric, widget and dashboard definitions to store. Use only when asked to build or save something. Carry every definition the request needs: a dashboard request means the metric, the widgets that draw it, and the dashboard holding them.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["reply"],
                "properties": {
                    "reply": { "type": "string" },
                    "metric": named(metric_query),
                    "widgets": { "type": "array", "items": named(widget) },
                    "dashboard": named(dashboard),
                },
            },
        }),
    ]
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

    /// The forced tool call's arguments, tagged with the intent the chosen
    /// tool implies, in the shape `Proposal::parse` reads. Falls back to the
    /// text blocks when a reply arrives without a tool call at all.
    fn proposal_json(&self) -> String {
        for block in &self.content {
            if block.kind != "tool_use" {
                continue;
            }
            let intent = match block.name.as_str() {
                ANSWER_TOOL => "answer",
                CREATE_TOOL => "create",
                _ => continue,
            };
            if let Some(Value::Object(fields)) = block.input.clone() {
                let mut tagged = fields;
                tagged.insert("intent".to_owned(), Value::String(intent.to_owned()));
                return Value::Object(tagged).to_string();
            }
        }

        self.text()
    }
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    input: Option<Value>,
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
    fn the_prompt_names_every_table_with_its_fields() {
        let prompt = system_prompt(&[
            KnownTable {
                name: "events".to_owned(),
                fields: "day (string), lines (int)".to_owned(),
            },
            KnownTable {
                name: "empty_yet".to_owned(),
                fields: String::new(),
            },
        ]);

        assert!(
            prompt.contains("- events: day (string), lines (int)"),
            "{prompt}"
        );
        // A table nothing has landed in yet is still a table it may read.
        assert!(prompt.contains("- empty_yet\n"), "{prompt}");
        assert!(!prompt.contains("No tables are known yet"), "{prompt}");
    }

    #[test]
    fn an_answer_naming_a_table_the_reader_does_not_have_is_refused() {
        let known = [KnownTable {
            name: "events".to_owned(),
            fields: "day (string)".to_owned(),
        }];
        let reply = json!({
            "intent": "answer",
            "reply": "here",
            "query": {
                "table": "information_schema_tables",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
                "group_by": [],
                "filters": []
            }
        })
        .to_string();

        let Err(rejection) = Proposal::checked(&reply, &known) else {
            panic!("a table that does not exist must not reach the database");
        };

        // The message is what the repair round hands back, so it has to name
        // the tables that DO exist.
        let feedback = rejection.feedback();
        assert!(feedback.contains("information_schema_tables"), "{feedback}");
        assert!(feedback.contains("events"), "{feedback}");
    }

    #[test]
    fn a_known_table_passes_the_check() {
        let known = [KnownTable {
            name: "events".to_owned(),
            fields: String::new(),
        }];
        let reply = json!({
            "intent": "answer",
            "reply": "here",
            "query": {
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
                "group_by": [],
                "filters": []
            }
        })
        .to_string();

        assert!(matches!(
            Proposal::checked(&reply, &known),
            Ok(Proposal::Answer { .. })
        ));
    }

    #[test]
    fn with_nothing_ingested_the_check_stands_aside() {
        // Refusing every table when we know of none would block the chat
        // outright on a stand whose listing failed.
        let reply = json!({
            "intent": "answer",
            "reply": "here",
            "query": {
                "table": "events",
                "fields": [{ "json": "day", "type": "string", "as_name": "day" }],
                "group_by": [],
                "filters": []
            }
        })
        .to_string();

        assert!(matches!(
            Proposal::checked(&reply, &[]),
            Ok(Proposal::Answer { .. })
        ));
    }

    #[test]
    fn one_tool_per_intent_carries_the_name_charset() {
        let tools = proposal_tools();

        assert_eq!(tools.len(), 2, "one tool per intent");
        assert_eq!(tools[0]["name"], json!(ANSWER_TOOL));
        assert_eq!(tools[1]["name"], json!(CREATE_TOOL));

        // Not strict on purpose: the nested MetricQuery exceeds the API's
        // compiled-grammar budget and a strict request is refused outright.
        // Verified by hand against the live API before this was written.
        for tool in &tools {
            assert!(tool.get("strict").is_none(), "strict must stay off");
            assert_eq!(tool["input_schema"]["additionalProperties"], json!(false));
        }

        // Every name the model invents carries the charset DefinitionName
        // enforces, stated where the model reads it.
        let create = &tools[1]["input_schema"]["properties"];
        for path in [
            &create["metric"]["properties"]["name"],
            &create["dashboard"]["properties"]["name"],
            &create["widgets"]["items"]["properties"]["name"],
        ] {
            assert_eq!(path["pattern"], json!(NAME_PATTERN), "missing name pattern");
        }
    }

    #[test]
    fn the_chosen_tool_becomes_the_intent() {
        let answered: MessagesResponse = serde_json::from_value(json!({
            "content": [
                { "type": "text", "text": "looking that up" },
                { "type": "tool_use", "name": ANSWER_TOOL,
                  "input": { "reply": "here", "query": {} } },
            ],
        }))
        .unwrap_or_else(|error| panic!("the fixture deserializes: {error}"));

        let json = answered.proposal_json();
        assert!(json.contains("\"intent\":\"answer\""), "got {json}");
        assert!(!json.contains("looking that up"), "text leaked in");

        let created: MessagesResponse = serde_json::from_value(json!({
            "content": [
                { "type": "tool_use", "name": CREATE_TOOL,
                  "input": { "reply": "made it", "widgets": [] } },
            ],
        }))
        .unwrap_or_else(|error| panic!("the fixture deserializes: {error}"));

        assert!(created.proposal_json().contains("\"intent\":\"create\""));
    }

    #[test]
    fn prose_around_the_json_is_tolerated() {
        let reply = "Sure!\n```json\n{\"intent\":\"create\",\"reply\":\"ok\",\"widgets\":[],\"dashboard\":{\"name\":\"lines\",\"body\":{\"title\":\"Lines\",\"widgets\":[]}}}\n```";

        assert!(matches!(
            Proposal::parse(reply),
            Ok(Proposal::Create { .. })
        ));
    }

    #[test]
    fn a_create_that_stores_nothing_is_refused() {
        let reply = json!({ "intent": "create", "reply": "done", "widgets": [] }).to_string();

        assert!(matches!(
            Proposal::parse(&reply),
            Err(ChatError::EmptyCreate)
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
            Err(ChatError::Metric(MetricQueryError::GroupBy(_)))
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
