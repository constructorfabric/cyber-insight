use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use sha2::{Digest as _, Sha256};

use super::metrics::{self, ErrorClass, QueryKind, QueryOutcome};

const QUERY_FETCH_TIMEOUT: Duration = Duration::from_mins(1);
/// The most an answer may weigh before decoding: a row limit bounds rows, not
/// the strings in them.
pub(crate) const MAX_ANSWER_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub(crate) enum QueryFetchError {
    #[error("query submission failed: {0}")]
    Submit(String),
    #[error("query fetch timed out")]
    Timeout,
    #[error("query fetch failed: {0}")]
    Fetch(String),
    #[error("query result parsing failed: {0}")]
    Parse(String),
}

impl QueryFetchError {
    fn class(&self) -> ErrorClass {
        match self {
            Self::Submit(message) | Self::Fetch(message) => ErrorClass::classify(message),
            Self::Timeout => ErrorClass::Timeout,
            Self::Parse(_) => ErrorClass::ParseFailed,
        }
    }
}

/// What a bounded fetch can fail with: anything a fetch can, or the ceiling.
#[derive(Debug, thiserror::Error)]
pub(crate) enum BoundedFetchError {
    #[error(transparent)]
    Fetch(#[from] QueryFetchError),
    #[error("query result exceeded {MAX_ANSWER_BYTES} bytes")]
    TooLarge,
}

impl BoundedFetchError {
    fn class(&self) -> ErrorClass {
        match self {
            Self::Fetch(error) => error.class(),
            Self::TooLarge => ErrorClass::ResourceExhausted,
        }
    }
}

// WORKAROUND: ClickHouse quotes every 64-bit integer by default, which would
// make an answer's number column arrive as a string.
pub(crate) async fn fetch_bound_rows(
    client: &insight_clickhouse::Client,
    sql: &str,
    params: &[serde_json::Value],
    kind: QueryKind,
    log_comment: &str,
) -> Result<Vec<serde_json::Value>, BoundedFetchError> {
    let started = Instant::now();
    let result = fetch_bound_rows_inner(client, sql, params, log_comment).await;

    match &result {
        Ok(_) => metrics::record_query(kind, QueryOutcome::Success, started.elapsed()),
        Err(error) => {
            metrics::record_query(kind, QueryOutcome::Error, started.elapsed());
            metrics::record_clickhouse_error(kind, error.class());
        }
    }
    result
}

async fn fetch_bound_rows_inner(
    client: &insight_clickhouse::Client,
    sql: &str,
    params: &[serde_json::Value],
    log_comment: &str,
) -> Result<Vec<serde_json::Value>, BoundedFetchError> {
    let mut query = client
        .query(sql)
        .with_setting("log_comment", log_comment)
        .with_setting("output_format_json_quote_64bit_integers", "0");
    for param in params {
        query = query.bind(param);
    }

    let mut cursor = query.fetch_bytes("JSONEachRow").map_err(|error| {
        log_query_failure(
            &error.to_string(),
            log_comment,
            sql,
            "ClickHouse query failed",
        );
        QueryFetchError::Submit(error.to_string())
    })?;

    let raw_bytes = tokio::time::timeout(QUERY_FETCH_TIMEOUT, collect_bounded(&mut cursor))
        .await
        .map_err(|_| {
            log_query_failure(
                "timeout",
                log_comment,
                sql,
                "ClickHouse query fetch timed out",
            );
            QueryFetchError::Timeout
        })?
        .map_err(|error| match error {
            ChunkError::TooLarge => {
                tracing::warn!(
                    comment = log_comment,
                    "ClickHouse query result exceeded the byte ceiling"
                );
                BoundedFetchError::TooLarge
            }
            ChunkError::Fetch(message) => {
                log_query_failure(&message, log_comment, sql, "ClickHouse query fetch failed");
                BoundedFetchError::Fetch(QueryFetchError::Fetch(message))
            }
        })?;

    let parsed = tokio::task::spawn_blocking(move || parse_json_rows(&raw_bytes))
        .await
        .map_err(|error| {
            log_query_failure(
                &error.to_string(),
                log_comment,
                sql,
                "ClickHouse row parsing task failed",
            );
            QueryFetchError::Parse(error.to_string())
        })?;

    parsed.map_err(|error| {
        log_query_failure(
            &error.to_string(),
            log_comment,
            sql,
            "failed to parse ClickHouse query rows",
        );
        BoundedFetchError::Fetch(QueryFetchError::Parse(error.to_string()))
    })
}

fn parse_json_rows(raw_bytes: &[u8]) -> Result<Vec<serde_json::Value>, serde_json::Error> {
    raw_bytes
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice)
        .collect()
}

enum ChunkError {
    TooLarge,
    Fetch(String),
}

// SAFETY: the ceiling is enforced chunk by chunk while receiving, so an
// oversized result is abandoned rather than buffered and then measured.
async fn collect_bounded(
    cursor: &mut clickhouse::query::BytesCursor,
) -> Result<Vec<u8>, ChunkError> {
    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = cursor
        .next()
        .await
        .map_err(|error| ChunkError::Fetch(error.to_string()))?
    {
        if buffer.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Err(ChunkError::TooLarge);
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(buffer)
}

pub(crate) async fn fetch_json_rows<T>(
    client: &insight_clickhouse::Client,
    sql: &str,
    params: &[String],
    kind: QueryKind,
    log_comment: &str,
) -> Result<Vec<T>, QueryFetchError>
where
    T: DeserializeOwned,
{
    let started = Instant::now();
    let result = fetch_json_rows_inner(client, sql, params, log_comment).await;

    match &result {
        Ok(_) => metrics::record_query(kind, QueryOutcome::Success, started.elapsed()),
        Err(error) => {
            metrics::record_query(kind, QueryOutcome::Error, started.elapsed());
            metrics::record_clickhouse_error(kind, error.class());
        }
    }
    result
}

async fn fetch_json_rows_inner<T>(
    client: &insight_clickhouse::Client,
    sql: &str,
    params: &[String],
    log_comment: &str,
) -> Result<Vec<T>, QueryFetchError>
where
    T: DeserializeOwned,
{
    let mut query = client.query(sql).with_setting("log_comment", log_comment);
    for param in params {
        query = query.bind(param.as_str());
    }

    let mut cursor = query.fetch_bytes("JSONEachRow").map_err(|error| {
        log_query_failure(
            &error.to_string(),
            log_comment,
            sql,
            "ClickHouse query failed",
        );
        QueryFetchError::Submit(error.to_string())
    })?;
    let raw_bytes = tokio::time::timeout(QUERY_FETCH_TIMEOUT, cursor.collect())
        .await
        .map_err(|_| {
            log_query_failure(
                "timeout",
                log_comment,
                sql,
                "ClickHouse query fetch timed out",
            );
            QueryFetchError::Timeout
        })?
        .map_err(|error| {
            log_query_failure(
                &error.to_string(),
                log_comment,
                sql,
                "ClickHouse query fetch failed",
            );
            QueryFetchError::Fetch(error.to_string())
        })?;
    if raw_bytes.is_empty() {
        return Ok(Vec::new());
    }

    raw_bytes
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            log_query_failure(
                &error.to_string(),
                log_comment,
                sql,
                "failed to parse ClickHouse query rows",
            );
            QueryFetchError::Parse(error.to_string())
        })
}

/// One failure line per stage. The SQL never reaches the line — a drilldown or
/// saved-query predicate can embed a person's name or address (insight#2488
/// AC-4) — so the line carries its SHA-256 instead, joinable against the
/// ClickHouse query log via `log_comment`. The server echoes the query into
/// exception text too (syntax and analyzer errors quote it), so the error is
/// scrubbed of any echo before it is logged.
pub(crate) fn log_query_failure(error: &str, log_comment: &str, sql: &str, message: &str) {
    let sql_hash = hex::encode(Sha256::digest(sql.as_bytes()));
    let error = scrub_query_echo(error, sql);
    tracing::error!(error = %error, comment = log_comment, sql_hash, "{message}");
}

/// Cut an error message before any point where the server starts quoting the
/// query: a literal echo of the SQL, or the ClickHouse exception phrases that
/// precede one (`failed at position …`, `… while processing query: …`,
/// `In query: …`). Everything before the earliest marker is diagnosis
/// (exception code and kind) and stays.
fn scrub_query_echo<'a>(error: &'a str, sql: &str) -> std::borrow::Cow<'a, str> {
    const ECHO_MARKERS: [&str; 3] = ["failed at position", "while processing query", "in query:"];

    // SAFETY: byte-offset search; a match starts with an ASCII byte, so
    // slicing `error` at the returned index stays on a char boundary.
    let find_ascii_ci = |haystack: &str, needle: &str| {
        haystack
            .as_bytes()
            .windows(needle.len())
            .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
    };

    let mut cut = error.len();
    for marker in ECHO_MARKERS {
        if let Some(index) = find_ascii_ci(error, marker) {
            cut = cut.min(index);
        }
    }
    if !sql.is_empty()
        && let Some(index) = error.find(sql)
    {
        cut = cut.min(index);
    }

    if cut == error.len() {
        return std::borrow::Cow::Borrowed(error);
    }
    let kept = error[..cut].trim_end();
    std::borrow::Cow::Owned(format!("{kept} <query echo scrubbed>"))
}
