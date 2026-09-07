//! What a widget draws, and whether its metric can supply it.
//!
//! A widget names columns by the `as_name` its metric gives them. Nothing
//! checked that, so a widget could name a column the metric never produces —
//! `y: "lines"` against a metric whose column is `total_lines`. The chart then
//! rendered its axes and no line at all, which reads as missing data rather
//! than as a broken definition.

use serde::Deserialize;
use thiserror::Error;

use crate::metric_query::MetricQuery;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Widget {
    Table {
        metric: String,
        #[serde(default)]
        columns: Vec<String>,
    },
    Line {
        metric: String,
        x: String,
        y: String,
    },
}

impl Widget {
    /// The metric this widget draws.
    pub(crate) fn metric(&self) -> &str {
        match self {
            Self::Table { metric, .. } | Self::Line { metric, .. } => metric,
        }
    }

    /// The columns it reads out of that metric's result.
    fn columns(&self) -> Vec<&str> {
        match self {
            Self::Table { columns, .. } => columns.iter().map(String::as_str).collect(),
            Self::Line { x, y, .. } => vec![x.as_str(), y.as_str()],
        }
    }

    /// Refuses a widget whose metric cannot supply what it draws.
    pub(crate) fn check_against(&self, metric: &MetricQuery) -> Result<(), WidgetError> {
        let available = metric.column_names();

        for column in self.columns() {
            if !available.iter().any(|name| name == column) {
                return Err(WidgetError::UnknownColumn {
                    column: column.to_owned(),
                    metric: self.metric().to_owned(),
                    available: available.join(", "),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum WidgetError {
    #[error("a widget needs a type of `table` or `line`, a metric, and the columns it draws")]
    Shape(#[from] serde_json::Error),
    #[error("`{column}` is not a column of metric `{metric}`; its columns are: {available}")]
    UnknownColumn {
        column: String,
        metric: String,
        available: String,
    },
    #[error("there is no metric named `{0}`")]
    NoMetric(String),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn metric() -> MetricQuery {
        serde_json::from_value(json!({
            "table": "events",
            "fields": [
                { "json": "day", "type": "string", "as_name": "day" },
                { "json": "lines", "type": "int", "agg": "sum", "as_name": "total_lines" }
            ],
            "group_by": ["day"],
            "filters": []
        }))
        .unwrap_or_else(|error| panic!("the fixture parses: {error}"))
    }

    fn widget(value: serde_json::Value) -> Widget {
        serde_json::from_value(value).unwrap_or_else(|error| panic!("the fixture parses: {error}"))
    }

    #[test]
    fn a_line_naming_the_metrics_own_columns_is_accepted() {
        let widget = widget(json!({
            "type": "line", "metric": "lines_per_day", "x": "day", "y": "total_lines"
        }));

        assert!(widget.check_against(&metric()).is_ok());
    }

    #[test]
    fn a_line_naming_the_raw_field_instead_of_the_alias_is_refused() {
        // Seen live: the chart drew its axes and no line, which reads as
        // missing data rather than as a widget naming a column that is not
        // there.
        let widget = widget(json!({
            "type": "line", "metric": "lines_per_day", "x": "day", "y": "lines"
        }));

        let Err(error) = widget.check_against(&metric()) else {
            panic!("a column the metric does not produce must be refused");
        };

        let message = error.to_string();
        assert!(message.contains("`lines` is not a column"), "{message}");
        // The message carries what IS available, so the repair round can fix it.
        assert!(message.contains("day, total_lines"), "{message}");
    }

    #[test]
    fn a_table_column_the_metric_does_not_produce_is_refused() {
        let widget = widget(json!({
            "type": "table", "metric": "lines_per_day", "columns": ["day", "author"]
        }));

        assert!(widget.check_against(&metric()).is_err());
    }

    #[test]
    fn a_table_drawing_every_column_it_names_is_accepted() {
        let widget = widget(json!({
            "type": "table", "metric": "lines_per_day", "columns": ["day", "total_lines"]
        }));

        assert!(widget.check_against(&metric()).is_ok());
    }
}
