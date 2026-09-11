//! Where a relative window counts back from, read off the data itself.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// The newest clock a metric's rows carry, and how many carry none.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Anchor {
    newest: Option<DateTime<Utc>>,
    undated: u64,
}

/// `ClickHouse`'s `JSON` format writes 64-bit integers as strings, so both
/// columns arrive either way round.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Number {
    Text(String),
    Value(i64),
}

impl Number {
    fn value(&self) -> Option<i64> {
        match self {
            Self::Text(text) => text.parse().ok(),
            Self::Value(value) => Some(*value),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnchorRow {
    newest: Option<Number>,
    undated: Option<Number>,
}

#[derive(Debug, Deserialize)]
struct AnchorAnswer {
    data: Vec<AnchorRow>,
}

impl Anchor {
    pub(crate) fn parse(body: &[u8]) -> Result<Self, serde_json::Error> {
        let answer: AnchorAnswer = serde_json::from_slice(body)?;
        let Some(row) = answer.data.first() else {
            return Ok(Self::default());
        };

        Ok(Self {
            newest: row
                .newest
                .as_ref()
                .and_then(Number::value)
                .and_then(DateTime::from_timestamp_millis),
            undated: row
                .undated
                .as_ref()
                .and_then(Number::value)
                .unwrap_or_default()
                .unsigned_abs(),
        })
    }

    pub(crate) fn newest(self) -> Option<DateTime<Utc>> {
        self.newest
    }

    pub(crate) fn undated(self) -> u64 {
        self.undated
    }
}

#[cfg(test)]
#[path = "anchor/tests.rs"]
mod tests;
