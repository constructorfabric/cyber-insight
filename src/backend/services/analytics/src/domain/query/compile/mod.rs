//! Turns a plan into one bounded ClickHouse statement and the values to bind
//! against it. Nothing here touches a connection.
//!
//! SAFETY: only engine-owned strings reach the statement text; every
//! caller-supplied value binds.

pub mod aggregates;
pub mod filters;
pub mod group;
pub mod order;
pub mod params;
pub mod scan;
pub mod time;

use std::fmt::Write;

use super::plan::{ColumnSource, QueryPlan, column_alias};

use params::QueryParam;

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledQuery {
    pub sql: String,
    pub params: Vec<QueryParam>,
}

// INVARIANT: placeholders bind by position — folds, then scope predicates,
// then the row ceiling. The ceiling is one over the limit: the extra row is
// how the answer knows it was truncated, and assembly drops it.
pub fn compile(plan: &QueryPlan<'_>, tenant_id: &str) -> CompiledQuery {
    let mut params = Vec::new();

    let selects = select_terms(plan, &mut params);
    let predicates = scan::predicates(plan, tenant_id, &mut params);
    params.push(QueryParam::UInt(u64::from(plan.limit) + 1));

    let mut sql = String::from("SELECT\n");
    let _ = writeln!(sql, "    {}", selects.join(",\n    "));
    let _ = writeln!(sql, "FROM {}", scan::from_clause(plan));
    let _ = writeln!(sql, "WHERE {}", predicates.join("\n  AND "));

    let group_aliases = group::group_aliases(plan);
    if !group_aliases.is_empty() {
        let _ = writeln!(sql, "GROUP BY {}", group_aliases.join(", "));
    }

    let order_terms = order::terms(plan);
    if !order_terms.is_empty() {
        let _ = writeln!(sql, "ORDER BY {}", order_terms.join(", "));
    }
    let _ = write!(sql, "LIMIT ?");

    CompiledQuery { sql, params }
}

fn select_terms(plan: &QueryPlan<'_>, params: &mut Vec<QueryParam>) -> Vec<String> {
    plan.columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let expr = match column.source {
                ColumnSource::Dimension(dimension) => group::dimension_expr(dimension),
                ColumnSource::Label(dimension) => group::label_expr(dimension),
                // INVARIANT: validation admits a time axis only beside a grain.
                ColumnSource::Bucket => plan
                    .time
                    .grain
                    .map(|grain| time::bucket_expr(plan.time.field, grain))
                    .unwrap_or_default(),
                ColumnSource::Aggregate(position) => {
                    aggregates::fold_expr(&plan.aggregates[position], params)
                }
            };
            format!("{expr} AS {}", column_alias(index))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::datasets::declaration::Dataset;
    use crate::domain::query::contract::dto::MAX_ROW_LIMIT;
    use crate::domain::query::fixtures;
    use crate::domain::query::validation::plan_against;

    const TENANT: &str = "acme-tenant";

    fn compiled(dataset: &Dataset, json: &str) -> CompiledQuery {
        let request = fixtures::query(json);
        let plan = plan_against(&request, dataset).expect("the query is admissible");
        compile(&plan, TENANT)
    }

    fn text(value: &str) -> QueryParam {
        QueryParam::Text(value.to_owned())
    }

    fn lines(expected: &[&str]) -> String {
        expected.join("\n")
    }

    #[test]
    fn a_grouped_bucketed_filtered_query_renders_one_bounded_scan() {
        let dataset = fixtures::commits();
        let compiled = compiled(
            &dataset,
            r#"{
              "dataset": "git_commits",
              "filters": [
                {"field": "source", "op": "in", "values": ["github", "gitlab"]},
                {"field": "lines_added", "op": "gte", "value": 500}
              ],
              "group_by": [{"axis": "dimension", "field": "repository"}, {"axis": "time"}],
              "aggregates": [
                {"name": "commits", "fn": "count"},
                {"name": "added_on_default", "fn": "sum", "field": "lines_added",
                 "filter": {"field": "branch_scope", "op": "eq", "value": "default"}}
              ],
              "time": {"from": "2026-01-01", "to": "2026-03-31", "grain": "week"},
              "order": [{"by": "commits", "dir": "desc"}],
              "limit": 100
            }"#,
        );

        assert_eq!(
            compiled.sql,
            lines(&[
                "SELECT",
                "    toString(repository) AS c0,",
                "    any(repository_label) AS c1,",
                "    toStartOfWeek(toDate(authored_at, 'UTC'), 1) AS c2,",
                "    count() AS c3,",
                "    sumIfOrNull(lines_added, coalesce(toString(branch_scope) = ?, 0)) AS c4",
                "FROM insight.git_commits",
                "WHERE tenant_id = ?",
                "  AND toDate(authored_at, 'UTC') >= toDate(?, 'UTC')",
                "  AND toDate(authored_at, 'UTC') <= toDate(?, 'UTC')",
                "  AND toString(source) IN (?, ?)",
                "  AND lines_added >= ?",
                "GROUP BY c0, c2",
                "ORDER BY c3 DESC, c0 ASC, c2 ASC",
                "LIMIT ?",
            ])
        );
        assert_eq!(
            compiled.params,
            vec![
                text("default"),
                text(TENANT),
                text("2026-01-01"),
                text("2026-03-31"),
                text("github"),
                text("gitlab"),
                QueryParam::Int(500),
                QueryParam::UInt(101),
            ]
        );
    }

    #[test]
    fn a_query_grouping_by_nothing_folds_the_window_into_one_row() {
        let dataset = fixtures::commits();
        let compiled = compiled(
            &dataset,
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        assert_eq!(
            compiled.sql,
            lines(&[
                "SELECT",
                "    count() AS c0",
                "FROM insight.git_commits",
                "WHERE tenant_id = ?",
                "  AND toDate(authored_at, 'UTC') >= toDate(?, 'UTC')",
                "  AND toDate(authored_at, 'UTC') <= toDate(?, 'UTC')",
                "LIMIT ?",
            ])
        );
    }

    #[test]
    fn every_grain_renders_its_own_bucket() {
        let dataset = fixtures::commits();
        let cases = [
            ("day", "toDate(authored_at, 'UTC') AS c0"),
            ("week", "toStartOfWeek(toDate(authored_at, 'UTC'), 1) AS c0"),
            ("month", "toStartOfMonth(toDate(authored_at, 'UTC')) AS c0"),
        ];

        for (grain, expected) in cases {
            let compiled = compiled(
                &dataset,
                &format!(
                    r#"{{"dataset": "git_commits",
                         "group_by": [{{"axis": "time"}}],
                         "aggregates": [{{"name": "commits", "fn": "count"}}],
                         "time": {{"from": "2026-01-01", "to": "2026-01-31", "grain": "{grain}"}}}}"#
                ),
            );
            assert!(compiled.sql.contains(expected), "{grain}: {}", compiled.sql);
        }
    }

    #[test]
    fn every_fold_renders_the_function_its_empty_case_needs() {
        let dataset = fixtures::commits();
        let cases = [
            ("count", None, "count() AS c0"),
            ("sum", Some("lines_added"), "sumOrNull(lines_added) AS c0"),
            ("avg", Some("lines_added"), "avgOrNull(lines_added) AS c0"),
            (
                "min",
                Some("lines_removed"),
                "minOrNull(lines_removed) AS c0",
            ),
            (
                "max",
                Some("lines_removed"),
                "maxOrNull(lines_removed) AS c0",
            ),
        ];

        for (function, field, expected) in cases {
            let field = field.map_or(String::new(), |name| format!(r#", "field": "{name}""#));
            let compiled = compiled(
                &dataset,
                &format!(
                    r#"{{"dataset": "git_commits",
                         "aggregates": [{{"name": "value", "fn": "{function}"{field}}}],
                         "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
                ),
            );
            assert!(
                compiled.sql.contains(expected),
                "{function}: {}",
                compiled.sql
            );
        }
    }

    #[test]
    fn every_filter_operator_renders_its_own_predicate() {
        let dataset = fixtures::commits();
        let cases = [
            (
                r#"{"field": "source", "op": "eq", "value": "github"}"#,
                "toString(source) = ?",
            ),
            (
                r#"{"field": "source", "op": "in", "values": ["github"]}"#,
                "toString(source) IN (?)",
            ),
            (
                r#"{"field": "lines_added", "op": "gt", "value": 1}"#,
                "lines_added > ?",
            ),
            (
                r#"{"field": "lines_added", "op": "gte", "value": 1}"#,
                "lines_added >= ?",
            ),
            (
                r#"{"field": "lines_added", "op": "lt", "value": 1}"#,
                "lines_added < ?",
            ),
            (
                r#"{"field": "lines_added", "op": "lte", "value": 1}"#,
                "lines_added <= ?",
            ),
            (
                r#"{"field": "lines_added", "op": "between", "low": 1, "high": 9}"#,
                "(lines_added >= ? AND lines_added <= ?)",
            ),
            (
                r#"{"field": "source_id", "op": "not_null"}"#,
                "source_id IS NOT NULL",
            ),
            (
                r#"{"field": "repository", "op": "like", "value": "example/%"}"#,
                "toString(repository) LIKE ?",
            ),
            (
                r#"{"field": "repository", "op": "match", "value": "^example/"}"#,
                "match(toString(repository), ?)",
            ),
        ];

        for (filter, expected) in cases {
            let compiled = compiled(
                &dataset,
                &format!(
                    r#"{{"dataset": "git_commits", "filters": [{filter}],
                         "aggregates": [{{"name": "commits", "fn": "count"}}],
                         "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
                ),
            );
            assert!(
                compiled.sql.contains(expected),
                "{filter}: {}",
                compiled.sql
            );
        }
    }

    #[test]
    fn a_nullable_dimension_groups_its_absent_rows_under_the_declared_value() {
        let dataset = fixtures::commits();
        let compiled = compiled(
            &dataset,
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "source_id"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        assert!(
            compiled
                .sql
                .contains("coalesce(toString(source_id), '__unknown__') AS c0"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn tenancy_leads_every_scan_and_binds_from_the_session() {
        let dataset = fixtures::commits();
        let compiled = compiled(
            &dataset,
            r#"{"dataset": "git_commits",
                 "filters": [{"field": "source", "op": "eq", "value": "github"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        assert!(compiled.sql.contains("WHERE tenant_id = ?\n  AND"));
        assert_eq!(compiled.params[0], text(TENANT));
    }

    #[test]
    fn a_query_over_a_relation_that_must_be_read_collapsed_scans_it_final() {
        let mut dataset = fixtures::commits();
        dataset.read_discipline = crate::domain::field_catalog::model::ReadDiscipline::Final;
        let compiled = compiled(
            &dataset,
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        assert!(compiled.sql.contains("FROM insight.git_commits FINAL"));
    }

    #[test]
    fn the_row_ceiling_binds_last_and_one_over_the_limit() {
        let dataset = fixtures::commits();
        let compiled = compiled(
            &dataset,
            &format!(
                r#"{{"dataset": "git_commits",
                     "aggregates": [{{"name": "commits", "fn": "count"}}],
                     "time": {{"from": "2026-01-01", "to": "2026-01-31"}},
                     "limit": {MAX_ROW_LIMIT}}}"#
            ),
        );

        assert!(compiled.sql.ends_with("LIMIT ?"));
        assert_eq!(
            compiled.params.last(),
            Some(&QueryParam::UInt(u64::from(MAX_ROW_LIMIT) + 1))
        );
    }

    #[test]
    fn a_grouped_answer_orders_by_every_group_column_the_query_did_not_name() {
        let dataset = fixtures::commits();
        let compiled = compiled(
            &dataset,
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "repository"}, {"axis": "dimension", "field": "source"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"},
                 "order": [{"by": "source", "dir": "desc"}]}"#,
        );

        assert!(
            compiled.sql.contains("ORDER BY c2 DESC, c0 ASC"),
            "{}",
            compiled.sql
        );
    }

    #[test]
    fn every_rendered_statement_binds_one_parameter_per_placeholder() {
        let dataset = fixtures::commits();
        let queries = [
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "author_email"}, {"axis": "time"}],
                 "filters": [{"field": "source_id", "op": "in", "values": ["a", "b", "c"]},
                             {"field": "lines_removed", "op": "between", "low": 0, "high": 100},
                             {"field": "repository", "op": "not_null"}],
                 "aggregates": [{"name": "commits", "fn": "count",
                                 "filter": {"field": "source", "op": "eq", "value": "github"}},
                                {"name": "added", "fn": "sum", "field": "lines_added"},
                                {"name": "biggest", "fn": "max", "field": "lines_added",
                                 "filter": {"field": "lines_added", "op": "gt", "value": 10}}],
                 "time": {"field": "authored_date", "from": "2026-01-01", "to": "2026-12-31",
                          "grain": "month"},
                 "order": [{"by": "added", "dir": "desc"}],
                 "limit": 250}"#,
        ];

        for query in queries {
            let compiled = compiled(&dataset, query);
            assert_eq!(
                compiled.sql.matches('?').count(),
                compiled.params.len(),
                "{}",
                compiled.sql
            );
        }
    }

    #[test]
    fn a_query_over_the_file_change_dataset_scans_its_own_relation() {
        let request = fixtures::query(
            r#"{"dataset": "git_file_changes",
                 "group_by": [{"axis": "dimension", "field": "file_extension"}, {"axis": "time"}],
                 "aggregates": [
                   {"name": "changes", "fn": "count"},
                   {"name": "added_to_tests", "fn": "sum", "field": "lines_added",
                    "filter": {"field": "category", "op": "eq", "value": "test"}}
                 ],
                 "time": {"from": "2026-01-01", "to": "2026-03-31", "grain": "month"},
                 "order": [{"by": "changes", "dir": "desc"}],
                 "limit": 500}"#,
        );
        let plan = crate::domain::query::validation::plan(&request).expect("admissible");
        let compiled = compile(&plan, TENANT);

        assert_eq!(
            compiled.sql,
            lines(&[
                "SELECT",
                "    toString(file_extension) AS c0,",
                "    any(file_extension_label) AS c1,",
                "    toStartOfMonth(toDate(authored_at, 'UTC')) AS c2,",
                "    count() AS c3,",
                "    sumIfOrNull(lines_added, coalesce(toString(category) = ?, 0)) AS c4",
                "FROM insight.git_file_changes",
                "WHERE tenant_id = ?",
                "  AND toDate(authored_at, 'UTC') >= toDate(?, 'UTC')",
                "  AND toDate(authored_at, 'UTC') <= toDate(?, 'UTC')",
                "GROUP BY c0, c2",
                "ORDER BY c3 DESC, c0 ASC, c2 ASC",
                "LIMIT ?",
            ])
        );
        assert_eq!(
            compiled.params,
            vec![
                text("test"),
                text(TENANT),
                text("2026-01-01"),
                text("2026-03-31"),
                QueryParam::UInt(501),
            ]
        );
        assert_eq!(compiled.sql.matches('?').count(), compiled.params.len());
    }

    #[test]
    fn the_shipped_dataset_compiles_the_same_query_the_fixture_does() {
        let request = fixtures::query(
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "repository"}, {"axis": "time"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-03-31", "grain": "week"}}"#,
        );
        let plan = crate::domain::query::validation::plan(&request).expect("admissible");
        let compiled = compile(&plan, TENANT);

        assert!(compiled.sql.contains("FROM insight.git_commits"));
        assert_eq!(compiled.sql.matches('?').count(), compiled.params.len());
    }
}
