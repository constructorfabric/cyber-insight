//! Turns what the warehouse returned into the answer's typed table.
//!
//! INVARIANT: the statement aliases every column by position, so the decode is
//! a lookup by alias and never a guess.

use serde_json::{Map, Value};

use super::contract::dto::{AnswerFlags, QueryAnswer};
use super::plan::{QueryPlan, column_alias};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnswerError {
    #[error("a returned row is not an object")]
    RowShape,
    #[error("a returned row reports no `{alias}` for column `{column}`")]
    ColumnMissing { alias: String, column: String },
}

// INVARIANT: the statement asked for one row over the limit; a returned set
// larger than the limit is the truncation signal, and the extra row is dropped.
pub fn assemble(plan: &QueryPlan<'_>, returned: Vec<Value>) -> Result<QueryAnswer, AnswerError> {
    let aliases: Vec<String> = (0..plan.columns.len()).map(column_alias).collect();
    let limit = plan.limit as usize;
    let truncated = returned.len() > limit;

    let mut rows = Vec::with_capacity(returned.len().min(limit));
    for row in returned.into_iter().take(limit) {
        let Value::Object(row) = row else {
            return Err(AnswerError::RowShape);
        };
        rows.push(decode_row(plan, &aliases, &row)?);
    }

    Ok(QueryAnswer {
        columns: plan
            .columns
            .iter()
            .map(|column| column.answer.clone())
            .collect(),
        rows,
        flags: AnswerFlags { truncated },
    })
}

fn decode_row(
    plan: &QueryPlan<'_>,
    aliases: &[String],
    row: &Map<String, Value>,
) -> Result<Vec<Value>, AnswerError> {
    let mut values = Vec::with_capacity(aliases.len());
    for (alias, column) in aliases.iter().zip(&plan.columns) {
        let value = row
            .get(alias)
            .ok_or_else(|| AnswerError::ColumnMissing {
                alias: alias.clone(),
                column: column.answer.name.clone(),
            })?
            .clone();

        // INVARIANT: a grouped column is never NULL; an aggregate may be, and
        // NULL is the answer rather than a zero to fill in.
        values.push(value);
    }

    Ok(values)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::query::contract::dto::{ColumnKind, ColumnType};
    use crate::domain::query::fixtures;
    use crate::domain::query::validation::plan_against;

    const GROUPED: &str = r#"{
      "dataset": "git_commits",
      "group_by": [{"axis": "dimension", "field": "repository"}, {"axis": "time"}],
      "aggregates": [{"name": "commits", "fn": "count"},
                     {"name": "added", "fn": "sum", "field": "lines_added"}],
      "time": {"from": "2026-01-01", "to": "2026-01-31", "grain": "week"},
      "limit": 2
    }"#;

    fn returned(rows: &str) -> Vec<Value> {
        serde_json::from_str(rows).expect("the fixture rows parse")
    }

    #[test]
    fn an_answer_reports_its_columns_in_the_order_a_row_carries_them() {
        let dataset = fixtures::commits();
        let request = fixtures::query(GROUPED);
        let plan = plan_against(&request, &dataset).expect("admissible");

        let answer = assemble(
            &plan,
            returned(
                r#"[{"c0": "example/app", "c1": "App", "c2": "2026-01-05", "c3": 12, "c4": 340},
                    {"c0": "example/lib", "c1": "Lib", "c2": "2026-01-05", "c3": 3, "c4": null}]"#,
            ),
        )
        .expect("the rows decode");

        assert_eq!(
            answer
                .columns
                .iter()
                .map(|column| (column.name.as_str(), column.kind, column.value_type))
                .collect::<Vec<_>>(),
            vec![
                ("repository", ColumnKind::Dimension, ColumnType::Text),
                ("repository_label", ColumnKind::Label, ColumnType::Text),
                ("time", ColumnKind::Bucket, ColumnType::Date),
                ("commits", ColumnKind::Aggregate, ColumnType::Number),
                ("added", ColumnKind::Aggregate, ColumnType::Number),
            ]
        );
        assert_eq!(
            answer.rows,
            vec![
                returned(r#"["example/app", "App", "2026-01-05", 12, 340]"#),
                returned(r#"["example/lib", "Lib", "2026-01-05", 3, null]"#),
            ]
        );
        assert!(!answer.flags.truncated);
    }

    #[test]
    fn a_result_one_row_over_the_limit_is_reported_truncated_and_clipped_to_the_limit() {
        let dataset = fixtures::commits();
        let request = fixtures::query(GROUPED);
        let plan = plan_against(&request, &dataset).expect("admissible");

        let answer = assemble(
            &plan,
            returned(
                r#"[{"c0": "a", "c1": "A", "c2": "2026-01-05", "c3": 1, "c4": 1},
                    {"c0": "b", "c1": "B", "c2": "2026-01-05", "c3": 1, "c4": 1},
                    {"c0": "c", "c1": "C", "c2": "2026-01-05", "c3": 1, "c4": 1}]"#,
            ),
        )
        .expect("the rows decode");

        assert!(answer.flags.truncated);
        assert_eq!(answer.rows.len(), 2);
    }

    #[test]
    fn a_fold_over_nothing_observed_stays_null_rather_than_becoming_zero() {
        let dataset = fixtures::commits();
        let request = fixtures::query(GROUPED);
        let plan = plan_against(&request, &dataset).expect("admissible");

        let answer = assemble(
            &plan,
            returned(
                r#"[{"c0": "example/app", "c1": "App", "c2": "2026-01-05", "c3": 0, "c4": null}]"#,
            ),
        )
        .expect("the rows decode");

        assert_eq!(answer.rows[0][4], Value::Null);
    }

    #[test]
    fn an_answer_over_no_rows_still_reports_its_columns() {
        let dataset = fixtures::commits();
        let request = fixtures::query(GROUPED);
        let plan = plan_against(&request, &dataset).expect("admissible");

        let answer = assemble(&plan, Vec::new()).expect("an empty result decodes");

        assert_eq!(answer.columns.len(), 5);
        assert!(answer.rows.is_empty());
        assert!(!answer.flags.truncated);
    }

    #[test]
    fn a_row_missing_a_column_the_statement_asked_for_is_an_error_not_a_hole() {
        let dataset = fixtures::commits();
        let request = fixtures::query(GROUPED);
        let plan = plan_against(&request, &dataset).expect("admissible");

        let error = assemble(
            &plan,
            returned(r#"[{"c0": "example/app", "c1": "App", "c2": "2026-01-05", "c3": 12}]"#),
        )
        .expect_err("the row is short");

        assert_eq!(
            error,
            AnswerError::ColumnMissing {
                alias: "c4".to_owned(),
                column: "added".to_owned(),
            }
        );
    }

    #[test]
    fn a_returned_value_that_is_not_a_row_is_an_error() {
        let dataset = fixtures::commits();
        let request = fixtures::query(GROUPED);
        let plan = plan_against(&request, &dataset).expect("admissible");

        assert_eq!(
            assemble(&plan, returned("[42]")).expect_err("not a row"),
            AnswerError::RowShape
        );
    }
}
