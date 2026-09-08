//! How a group axis becomes a column of the answer.
//!
//! INVARIANT: the answer and a filter both read a dimension's rendered value,
//! never the raw column.

use crate::domain::datasets::declaration::Dimension;
use crate::domain::query::plan::{QueryPlan, column_alias};

/// What the answer reports for one dimension; a group is never NULL.
pub fn dimension_expr(dimension: &Dimension) -> String {
    let value = format!("toString({})", dimension.field);
    match &dimension.absent_value {
        // SAFETY: the sentinel comes from the declaration, not from a caller.
        Some(absent) => format!("coalesce({value}, '{}')", escape_literal(absent)),
        None => value,
    }
}

/// The declared label beside a dimension's value; one per group, so any row's.
pub fn label_expr(dimension: &Dimension) -> String {
    let label = dimension.label_field.as_deref().unwrap_or(&dimension.field);
    format!("any({label})")
}

pub fn group_aliases(plan: &QueryPlan<'_>) -> Vec<String> {
    plan.columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.groups())
        .map(|(index, _)| column_alias(index))
        .collect()
}

fn escape_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn dimension(absent_value: Option<&str>) -> Dimension {
        Dimension {
            field: "repository".to_owned(),
            label_field: None,
            absent_value: absent_value.map(str::to_owned),
        }
    }

    #[test]
    fn a_dimension_over_a_non_nullable_column_reports_the_column_as_text() {
        assert_eq!(dimension_expr(&dimension(None)), "toString(repository)");
    }

    #[test]
    fn a_nullable_dimension_reports_its_absent_rows_under_the_declared_value() {
        assert_eq!(
            dimension_expr(&dimension(Some("__unknown__"))),
            "coalesce(toString(repository), '__unknown__')"
        );
    }

    #[test]
    fn a_label_column_reads_the_declared_label_field() {
        let dimension = Dimension {
            field: "repository".to_owned(),
            label_field: Some("repository_label".to_owned()),
            absent_value: None,
        };
        assert_eq!(label_expr(&dimension), "any(repository_label)");
    }

    #[test]
    fn a_quote_in_a_declared_value_cannot_end_the_literal() {
        assert_eq!(
            dimension_expr(&dimension(Some("it's"))),
            r"coalesce(toString(repository), 'it\'s')"
        );
    }
}
