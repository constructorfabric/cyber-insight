//! Binds a query to its dataset, or refuses it.
//!
//! INVARIANT: the whole query is checked before anything is refused, so a caller
//! sees every problem at once.

use std::collections::HashSet;

use chrono::NaiveDate;

use super::contract::dto::{
    AggregateDto, AnswerColumn, ColumnKind, ColumnType, DEFAULT_ROW_LIMIT, FilterDto, GroupAxisDto,
    MAX_AGGREGATES, MAX_FILTER_VALUES, MAX_FILTERS, MAX_GROUP_AXES, MAX_NAME_CHARS,
    MAX_ORDER_TERMS, MAX_PATTERN_CHARS, MAX_ROW_LIMIT, MAX_WINDOW_DAYS, OrderDto, QueryRequest,
    ScalarDto,
};
use super::plan::{
    BUCKET_COLUMN, ColumnSource, CompareOp, FilterTarget, FoldFn, PlannedAggregate, PlannedAxis,
    PlannedColumn, PlannedFilter, PlannedFold, PlannedOrder, PlannedTest, PlannedTime, QueryPlan,
    label_column_name,
};
use super::violation::{Reason, Violation};
use crate::domain::datasets;
use crate::domain::datasets::declaration::Dataset;
use crate::domain::datasets::validate::DeclarationError;

/// Why a request got no plan: the request's fault, or this build's.
#[derive(Debug)]
pub enum PlanError {
    Refused(Vec<Violation>),
    /// The shipped declarations failed to load, so no query can be answered.
    DeclarationsUnusable(&'static DeclarationError),
}

pub fn plan(request: &QueryRequest) -> Result<QueryPlan<'static>, PlanError> {
    let declared = datasets::product_datasets().map_err(PlanError::DeclarationsUnusable)?;

    let dataset = declared
        .iter()
        .find(|dataset| dataset.key.as_str() == request.dataset)
        .ok_or_else(|| {
            PlanError::Refused(vec![Violation::unknown(
                "dataset",
                &request.dataset,
                &datasets::declared_keys(),
            )])
        })?;

    plan_against(request, dataset).map_err(PlanError::Refused)
}

pub fn plan_against<'d>(
    request: &QueryRequest,
    dataset: &'d Dataset,
) -> Result<QueryPlan<'d>, Vec<Violation>> {
    let mut violations = Vec::new();

    let filters = plan_filters(&request.filters, dataset, &mut violations);
    let group_by = plan_axes(&request.group_by, dataset, &mut violations);
    let aggregates = plan_aggregates(&request.aggregates, dataset, &mut violations);
    let time = plan_time(request, &group_by, dataset, &mut violations);
    let limit = plan_limit(request.limit, &mut violations);

    let columns = answer_columns(&group_by, &aggregates, &mut violations);
    let order = plan_order(&request.order, &columns, &mut violations);

    let Some(time) = time else {
        return Err(violations);
    };
    if !violations.is_empty() {
        return Err(violations);
    }

    Ok(QueryPlan {
        dataset,
        filters,
        group_by,
        aggregates,
        time,
        order,
        limit,
        columns,
    })
}

fn plan_filters<'d>(
    filters: &[FilterDto],
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Vec<PlannedFilter<'d>> {
    if filters.len() > MAX_FILTERS {
        violations.push(Violation::new(
            "filters",
            Reason::OutOfRange,
            format!("a query carries at most {MAX_FILTERS} filters"),
        ));
    }

    filters
        .iter()
        .enumerate()
        .filter_map(|(index, filter)| {
            plan_filter(filter, &format!("filters[{index}]"), dataset, violations)
        })
        .collect()
}

fn plan_filter<'d>(
    filter: &FilterDto,
    path: &str,
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Option<PlannedFilter<'d>> {
    let target = resolve_filter_target(filter, path, dataset, violations)?;
    let test = planned_test(filter, path, violations)?;
    check_operands(&test, &target, path, violations);

    Some(PlannedFilter { target, test })
}

// INVARIANT: every operator but `in` has its arity fixed by its shape.
fn planned_test(
    filter: &FilterDto,
    path: &str,
    violations: &mut Vec<Violation>,
) -> Option<PlannedTest> {
    let test = match filter {
        FilterDto::Eq { value, .. } => PlannedTest::Eq(value.clone()),
        FilterDto::In { values, .. } => {
            let complaint = match values.len() {
                0 => Some("takes at least one value".to_owned()),
                n if n > MAX_FILTER_VALUES => {
                    Some(format!("takes at most {MAX_FILTER_VALUES} values"))
                }
                _ => None,
            };
            if let Some(complaint) = complaint {
                violations.push(Violation::new(
                    format!("{path}.values"),
                    Reason::OutOfRange,
                    format!("`in` {complaint}, and {} were given", values.len()),
                ));
                return None;
            }
            PlannedTest::In(values.clone())
        }
        FilterDto::Gt { value, .. } => PlannedTest::Compare(CompareOp::Gt, value.clone()),
        FilterDto::Gte { value, .. } => PlannedTest::Compare(CompareOp::Gte, value.clone()),
        FilterDto::Lt { value, .. } => PlannedTest::Compare(CompareOp::Lt, value.clone()),
        FilterDto::Lte { value, .. } => PlannedTest::Compare(CompareOp::Lte, value.clone()),
        FilterDto::Between { low, high, .. } => PlannedTest::Between {
            low: low.clone(),
            high: high.clone(),
        },
        FilterDto::NotNull { .. } => PlannedTest::NotNull,
        FilterDto::Like { value, .. } => {
            check_pattern_length(value, path, violations)?;
            PlannedTest::Like(value.clone())
        }
        FilterDto::Matches { value, .. } => {
            check_pattern_length(value, path, violations)?;
            if let Err(error) = regex_lite::Regex::new(value) {
                violations.push(Violation::new(
                    format!("{path}.value"),
                    Reason::Malformed,
                    format!("the pattern is not a regular expression: {error}"),
                ));
                return None;
            }
            PlannedTest::Matches(value.clone())
        }
    };

    Some(test)
}

fn check_pattern_length(pattern: &str, path: &str, violations: &mut Vec<Violation>) -> Option<()> {
    if pattern.chars().count() <= MAX_PATTERN_CHARS {
        return Some(());
    }
    violations.push(Violation::new(
        format!("{path}.value"),
        Reason::OutOfRange,
        format!("a pattern is at most {MAX_PATTERN_CHARS} characters"),
    ));
    None
}

fn resolve_filter_target<'d>(
    filter: &FilterDto,
    path: &str,
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Option<FilterTarget<'d>> {
    if let Some(dimension) = dataset.dimension(filter.field()) {
        return Some(FilterTarget::Dimension(dimension));
    }
    if let Some(measurable) = dataset.measurable(filter.field()) {
        return Some(FilterTarget::Measurable(measurable));
    }

    let mut admissible = dataset.dimension_names();
    admissible.extend(dataset.measurable_names());
    violations.push(Violation::unknown(
        format!("{path}.field"),
        filter.field(),
        &admissible,
    ));
    None
}

// INVARIANT: a dimension compares as text, so an ordered test over it would
// order lexicographically; a measurable compares against the numeric column, so
// a pattern cannot apply and only a number can be on the other side.
fn check_operands(
    test: &PlannedTest,
    target: &FilterTarget<'_>,
    path: &str,
    violations: &mut Vec<Violation>,
) {
    let measurable = match target {
        FilterTarget::Dimension(dimension) => {
            if matches!(test, PlannedTest::Compare(..) | PlannedTest::Between { .. }) {
                violations.push(Violation::new(
                    format!("{path}.op"),
                    Reason::Unexpected,
                    format!(
                        "`{}` is a dimension and takes eq, in, not_null, like or match",
                        dimension.field
                    ),
                ));
            }
            return;
        }
        FilterTarget::Measurable(measurable) => measurable,
    };

    if matches!(test, PlannedTest::Like(_) | PlannedTest::Matches(_)) {
        violations.push(Violation::new(
            format!("{path}.op"),
            Reason::Unexpected,
            format!(
                "`{}` is numeric and takes eq, in, not_null, the comparisons or between",
                measurable.field
            ),
        ));
        return;
    }

    for (operand, value) in test.operands() {
        if matches!(value, ScalarDto::Number(_)) {
            continue;
        }
        violations.push(Violation::new(
            format!("{path}.{operand}"),
            Reason::TypeMismatch,
            format!(
                "`{}` is numeric and compares against numbers",
                measurable.field
            ),
        ));
    }
}

fn plan_axes<'d>(
    axes: &[GroupAxisDto],
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Vec<PlannedAxis<'d>> {
    if axes.len() > MAX_GROUP_AXES {
        violations.push(Violation::new(
            "group_by",
            Reason::OutOfRange,
            format!("a query groups by at most {MAX_GROUP_AXES} axes"),
        ));
    }

    let mut seen_dimensions = HashSet::new();
    let mut seen_time = false;
    let mut planned = Vec::with_capacity(axes.len());

    for (index, axis) in axes.iter().enumerate() {
        match axis {
            GroupAxisDto::Dimension { field: named } => {
                let Some(dimension) = dataset.dimension(named) else {
                    violations.push(Violation::unknown(
                        format!("group_by[{index}].field"),
                        named,
                        &dataset.dimension_names(),
                    ));
                    continue;
                };
                if !seen_dimensions.insert(dimension.field.as_str()) {
                    violations.push(Violation::new(
                        format!("group_by[{index}].field"),
                        Reason::Duplicate,
                        format!("`{}` is already a group axis", dimension.field),
                    ));
                    continue;
                }
                planned.push(PlannedAxis::Dimension(dimension));
            }
            GroupAxisDto::Time {} => {
                if seen_time {
                    violations.push(Violation::new(
                        format!("group_by[{index}].axis"),
                        Reason::Duplicate,
                        "a query carries one time axis",
                    ));
                    continue;
                }
                seen_time = true;
                planned.push(PlannedAxis::Time);
            }
        }
    }

    planned
}

fn plan_aggregates<'d>(
    aggregates: &[AggregateDto],
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Vec<PlannedAggregate<'d>> {
    if aggregates.is_empty() {
        violations.push(Violation::new(
            "aggregates",
            Reason::Missing,
            "a query computes at least one aggregate",
        ));
    }
    if aggregates.len() > MAX_AGGREGATES {
        violations.push(Violation::new(
            "aggregates",
            Reason::OutOfRange,
            format!("a query carries at most {MAX_AGGREGATES} aggregates"),
        ));
    }

    let mut seen = HashSet::new();
    let mut planned = Vec::with_capacity(aggregates.len());

    for (index, aggregate) in aggregates.iter().enumerate() {
        let path = format!("aggregates[{index}]");
        let name = aggregate.name();
        if !is_answer_name(name) {
            violations.push(Violation::new(
                format!("{path}.name"),
                Reason::Malformed,
                format!(
                    "an aggregate name is lowercase snake_case of at most {MAX_NAME_CHARS} characters"
                ),
            ));
            continue;
        }
        if !seen.insert(name) {
            violations.push(Violation::new(
                format!("{path}.name"),
                Reason::Duplicate,
                format!("`{name}` names two aggregates"),
            ));
            continue;
        }

        let Some(fold) = plan_fold(aggregate, &path, dataset, violations) else {
            continue;
        };
        let declared_filter = aggregate.filter();
        let filter = declared_filter
            .and_then(|filter| plan_filter(filter, &format!("{path}.filter"), dataset, violations));
        if declared_filter.is_some() && filter.is_none() {
            continue;
        }

        planned.push(PlannedAggregate {
            index,
            name: name.to_owned(),
            fold,
            filter,
        });
    }

    planned
}

fn plan_fold<'d>(
    aggregate: &AggregateDto,
    path: &str,
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Option<PlannedFold<'d>> {
    let (function, field) = match aggregate {
        AggregateDto::Count { .. } => return Some(PlannedFold::Rows),
        AggregateDto::Sum { field, .. } => (FoldFn::Sum, field),
        AggregateDto::Avg { field, .. } => (FoldFn::Avg, field),
        AggregateDto::Min { field, .. } => (FoldFn::Min, field),
        AggregateDto::Max { field, .. } => (FoldFn::Max, field),
    };

    let Some(measurable) = dataset.measurable(field) else {
        violations.push(Violation::unknown(
            format!("{path}.field"),
            field,
            &dataset.measurable_names(),
        ));
        return None;
    };

    Some(PlannedFold::Values {
        function,
        measurable,
    })
}

fn plan_time<'d>(
    request: &QueryRequest,
    group_by: &[PlannedAxis<'d>],
    dataset: &'d Dataset,
    violations: &mut Vec<Violation>,
) -> Option<PlannedTime<'d>> {
    let time = &request.time;
    if time.from > time.to {
        violations.push(Violation::new(
            "time.from",
            Reason::OutOfRange,
            "the window starts after it ends",
        ));
    } else if (time.to - time.from).num_days() >= MAX_WINDOW_DAYS {
        violations.push(Violation::new(
            "time.to",
            Reason::OutOfRange,
            format!("a window spans at most {MAX_WINDOW_DAYS} days"),
        ));
    }
    check_window_days_representable(time.from, time.to, violations);

    let buckets = group_by
        .iter()
        .any(|axis| matches!(axis, PlannedAxis::Time));
    match (buckets, time.grain) {
        (true, None) => violations.push(Violation::new(
            "time.grain",
            Reason::Missing,
            "a time axis needs a bucket width",
        )),
        (false, Some(_)) => violations.push(Violation::new(
            "time.grain",
            Reason::Unexpected,
            "a bucket width buckets nothing without an `{\"axis\": \"time\"}` axis",
        )),
        (true, Some(_)) | (false, None) => {}
    }

    let field = match time.field.as_deref() {
        None => dataset.default_time_field(),
        Some(named) => {
            let found = dataset.time_field(named);
            if found.is_none() {
                violations.push(Violation::unknown(
                    "time.field",
                    named,
                    &dataset.time_field_names(),
                ));
            }
            found
        }
    }?;

    Some(PlannedTime {
        field,
        from: time.from,
        to: time.to,
        grain: time.grain,
    })
}

// SAFETY: the compiler renders every bound day through `toDate`, which clamps a
// day outside the warehouse's `Date` range instead of refusing it — so a window
// the caller never asked for would match rows.
fn check_window_days_representable(
    from: NaiveDate,
    to: NaiveDate,
    violations: &mut Vec<Violation>,
) {
    let (earliest, latest) = representable_window_days();
    for (field, day) in [("time.from", from), ("time.to", to)] {
        if day < earliest || day > latest {
            violations.push(Violation::new(
                field,
                Reason::OutOfRange,
                format!("a window day lies between {earliest} and {latest}"),
            ));
        }
    }
}

/// The range of ClickHouse's `Date`.
fn representable_window_days() -> (NaiveDate, NaiveDate) {
    let earliest = NaiveDate::from_ymd_opt(1970, 1, 1);
    let latest = NaiveDate::from_ymd_opt(2149, 6, 6);
    match (earliest, latest) {
        (Some(earliest), Some(latest)) => (earliest, latest),
        (None, _) | (_, None) => unreachable!("the Date range is a valid calendar range"),
    }
}

fn plan_limit(limit: Option<u32>, violations: &mut Vec<Violation>) -> u32 {
    match limit {
        None => DEFAULT_ROW_LIMIT,
        Some(0) => {
            violations.push(Violation::new(
                "limit",
                Reason::OutOfRange,
                "a limit of zero asks for no rows",
            ));
            DEFAULT_ROW_LIMIT
        }
        Some(requested) if requested > MAX_ROW_LIMIT => {
            violations.push(Violation::new(
                "limit",
                Reason::OutOfRange,
                format!("an answer carries at most {MAX_ROW_LIMIT} rows"),
            ));
            DEFAULT_ROW_LIMIT
        }
        Some(requested) => requested,
    }
}

// INVARIANT: no two answer columns share a name, or an order term could not say
// which one it means.
fn answer_columns<'d>(
    group_by: &[PlannedAxis<'d>],
    aggregates: &[PlannedAggregate<'d>],
    violations: &mut Vec<Violation>,
) -> Vec<PlannedColumn<'d>> {
    let mut columns: Vec<PlannedColumn<'d>> =
        Vec::with_capacity(2 * group_by.len() + aggregates.len());

    for axis in group_by {
        match axis {
            PlannedAxis::Dimension(dimension) => {
                columns.push(PlannedColumn {
                    answer: AnswerColumn {
                        name: dimension.field.clone(),
                        kind: ColumnKind::Dimension,
                        value_type: ColumnType::Text,
                    },
                    source: ColumnSource::Dimension(dimension),
                });
                if dimension.label_field.is_some() {
                    columns.push(PlannedColumn {
                        answer: AnswerColumn {
                            name: label_column_name(dimension),
                            kind: ColumnKind::Label,
                            value_type: ColumnType::Text,
                        },
                        source: ColumnSource::Label(dimension),
                    });
                }
            }
            PlannedAxis::Time => columns.push(PlannedColumn {
                answer: AnswerColumn {
                    name: BUCKET_COLUMN.to_owned(),
                    kind: ColumnKind::Bucket,
                    value_type: ColumnType::Date,
                },
                source: ColumnSource::Bucket,
            }),
        }
    }

    for (position, aggregate) in aggregates.iter().enumerate() {
        if columns
            .iter()
            .any(|column| column.answer.name == aggregate.name)
        {
            violations.push(Violation::new(
                format!("aggregates[{}].name", aggregate.index),
                Reason::Duplicate,
                format!("`{}` is already a column of this answer", aggregate.name),
            ));
            continue;
        }
        columns.push(PlannedColumn {
            answer: AnswerColumn {
                name: aggregate.name.clone(),
                kind: ColumnKind::Aggregate,
                value_type: ColumnType::Number,
            },
            source: ColumnSource::Aggregate(position),
        });
    }

    columns
}

fn plan_order(
    order: &[OrderDto],
    columns: &[PlannedColumn<'_>],
    violations: &mut Vec<Violation>,
) -> Vec<PlannedOrder> {
    if order.len() > MAX_ORDER_TERMS {
        violations.push(Violation::new(
            "order",
            Reason::OutOfRange,
            format!("a query orders by at most {MAX_ORDER_TERMS} columns"),
        ));
    }

    let names: Vec<&str> = columns
        .iter()
        .map(|column| column.answer.name.as_str())
        .collect();
    let mut seen = HashSet::new();
    let mut planned = Vec::with_capacity(order.len());

    for (index, term) in order.iter().enumerate() {
        let Some(column) = columns
            .iter()
            .position(|column| column.answer.name == term.by)
        else {
            violations.push(Violation::unknown(
                format!("order[{index}].by"),
                &term.by,
                &names,
            ));
            continue;
        };
        if !seen.insert(column) {
            violations.push(Violation::new(
                format!("order[{index}].by"),
                Reason::Duplicate,
                format!("`{}` is already an ordering term", term.by),
            ));
            continue;
        }
        planned.push(PlannedOrder {
            column,
            direction: term.dir,
        });
    }

    planned
}

fn is_answer_name(name: &str) -> bool {
    if name.chars().count() > MAX_NAME_CHARS {
        return false;
    }
    let mut characters = name.chars();
    match characters.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        Some(_) | None => return false,
    }
    characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::query::contract::dto::{Direction, Grain};
    use crate::domain::query::fixtures;

    fn refuse(json: &str) -> Vec<Violation> {
        let dataset = fixtures::commits();
        plan_against(&fixtures::query(json), &dataset).expect_err("the query must be refused")
    }

    fn only(json: &str) -> Violation {
        let mut violations = refuse(json);
        assert_eq!(violations.len(), 1, "{violations:?}");
        violations.remove(0)
    }

    fn accept(json: &str) -> (Dataset, QueryRequest) {
        let dataset = fixtures::commits();
        let request = fixtures::query(json);
        plan_against(&request, &dataset).expect("the query must be admissible");
        (dataset, request)
    }

    const COUNT_BY_WEEK: &str = r#"{
      "dataset": "git_commits",
      "group_by": [{"axis": "dimension", "field": "repository"}, {"axis": "time"}],
      "aggregates": [{"name": "commits", "fn": "count"}],
      "time": {"from": "2026-01-01", "to": "2026-03-31", "grain": "week"},
      "order": [{"by": "commits", "dir": "desc"}],
      "limit": 50
    }"#;

    #[test]
    fn a_grouped_bucketed_query_binds_every_reference_it_names() {
        let dataset = fixtures::commits();
        let plan = plan_against(&fixtures::query(COUNT_BY_WEEK), &dataset).expect("admissible");

        assert_eq!(plan.limit, 50);
        assert!(plan.group_by.contains(&PlannedAxis::Time));
        assert_eq!(plan.time.grain, Some(Grain::Week));
        assert_eq!(plan.time.field.field, "authored_at");
        assert_eq!(
            plan.columns
                .iter()
                .map(|column| {
                    (
                        column.answer.name.as_str(),
                        column.answer.kind,
                        column.answer.value_type,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("repository", ColumnKind::Dimension, ColumnType::Text),
                ("repository_label", ColumnKind::Label, ColumnType::Text),
                ("time", ColumnKind::Bucket, ColumnType::Date),
                ("commits", ColumnKind::Aggregate, ColumnType::Number),
            ]
        );
        assert_eq!(
            plan.order,
            vec![PlannedOrder {
                column: 3,
                direction: Direction::Desc,
            }]
        );
    }

    #[test]
    fn a_dimension_without_a_declared_label_answers_its_value_alone() {
        let dataset = fixtures::commits();
        let plan = plan_against(
            &fixtures::query(
                r#"{"dataset": "git_commits",
                     "group_by": [{"axis": "dimension", "field": "author_email"}],
                     "aggregates": [{"name": "commits", "fn": "count"}],
                     "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
            ),
            &dataset,
        )
        .expect("admissible");

        let names: Vec<&str> = plan
            .columns
            .iter()
            .map(|column| column.answer.name.as_str())
            .collect();
        assert_eq!(names, ["author_email", "commits"]);
    }

    #[test]
    fn an_aggregate_colliding_with_a_group_column_is_refused_at_its_own_request_index() {
        let violations = refuse(
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "source"}],
                 "aggregates": [{"name": "Bad", "fn": "count"}, {"name": "source", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        let fields: Vec<&str> = violations
            .iter()
            .map(|violation| violation.field.as_str())
            .collect();
        assert_eq!(fields, ["aggregates[0].name", "aggregates[1].name"]);
        assert_eq!(violations[1].reason, Reason::Duplicate);
    }

    #[test]
    fn a_window_day_the_warehouse_cannot_represent_is_refused_rather_than_clamped() {
        for (json, field) in [
            (
                r#"{"dataset": "git_commits",
                     "aggregates": [{"name": "commits", "fn": "count"}],
                     "time": {"from": "1960-01-01", "to": "1960-01-31"}}"#,
                vec!["time.from", "time.to"],
            ),
            (
                r#"{"dataset": "git_commits",
                     "aggregates": [{"name": "commits", "fn": "count"}],
                     "time": {"from": "2149-06-01", "to": "2149-06-07"}}"#,
                vec!["time.to"],
            ),
        ] {
            let violations = refuse(json);
            let fields: Vec<&str> = violations
                .iter()
                .map(|violation| violation.field.as_str())
                .collect();
            assert_eq!(fields, field, "{json}");
            assert!(violations.iter().all(|v| v.reason == Reason::OutOfRange));
        }
    }

    #[test]
    fn a_query_naming_no_limit_takes_the_default_ceiling() {
        let dataset = fixtures::commits();
        let plan = plan_against(
            &fixtures::query(
                r#"{"dataset": "git_commits",
                     "aggregates": [{"name": "commits", "fn": "count"}],
                     "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
            ),
            &dataset,
        )
        .expect("admissible");

        assert_eq!(plan.limit, DEFAULT_ROW_LIMIT);
        assert_eq!(plan.time.grain, None);
        assert!(!plan.group_by.contains(&PlannedAxis::Time));
    }

    #[test]
    fn a_query_over_the_row_ceiling_is_refused_rather_than_clipped() {
        let violation = only(&format!(
            r#"{{"dataset": "git_commits",
                 "aggregates": [{{"name": "commits", "fn": "count"}}],
                 "time": {{"from": "2026-01-01", "to": "2026-01-31"}},
                 "limit": {}}}"#,
            MAX_ROW_LIMIT + 1
        ));

        assert_eq!(violation.field, "limit");
        assert_eq!(violation.reason, Reason::OutOfRange);
    }

    #[test]
    fn a_group_axis_the_dataset_does_not_declare_is_refused_with_the_admissible_set() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "branch"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        assert_eq!(violation.field, "group_by[0].field");
        assert_eq!(violation.reason, Reason::Unknown);
        assert!(violation.detail.contains("repository"), "{violation:?}");
    }

    #[test]
    fn a_time_axis_and_a_bucket_width_each_require_the_other() {
        let missing = only(
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "time"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(missing.field, "time.grain");
        assert_eq!(missing.reason, Reason::Missing);

        let unexpected = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31", "grain": "day"}}"#,
        );
        assert_eq!(unexpected.field, "time.grain");
        assert_eq!(unexpected.reason, Reason::Unexpected);
    }

    #[test]
    fn a_repeated_axis_is_refused() {
        for (json, field) in [
            (
                r#"{"dataset": "git_commits",
                     "group_by": [{"axis": "dimension", "field": "repository"}, {"axis": "dimension", "field": "repository"}],
                     "aggregates": [{"name": "commits", "fn": "count"}],
                     "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
                "group_by[1].field",
            ),
            (
                r#"{"dataset": "git_commits",
                     "group_by": [{"axis": "time"}, {"axis": "time"}],
                     "aggregates": [{"name": "commits", "fn": "count"}],
                     "time": {"from": "2026-01-01", "to": "2026-01-31", "grain": "day"}}"#,
                "group_by[1].axis",
            ),
        ] {
            let violation = only(json);
            assert_eq!(violation.field, field);
            assert_eq!(violation.reason, Reason::Duplicate);
        }
    }

    #[test]
    fn a_window_that_ends_before_it_starts_is_refused() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-03-31", "to": "2026-01-01"}}"#,
        );
        assert_eq!(violation.field, "time.from");
        assert_eq!(violation.reason, Reason::OutOfRange);
    }

    #[test]
    fn a_window_wider_than_the_cap_is_refused_and_one_at_the_cap_is_not() {
        let at_cap = fixtures::query(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2024-01-01", "to": "2025-12-31"}}"#,
        );
        plan_against(&at_cap, &fixtures::commits()).expect("731 days inclusive is the cap");

        let violation = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2024-01-01", "to": "2026-01-01"}}"#,
        );
        assert_eq!(violation.field, "time.to");
        assert_eq!(violation.reason, Reason::OutOfRange);
    }

    #[test]
    fn a_time_field_the_dataset_does_not_declare_is_refused() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"field": "ingested_at", "from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "time.field");
        assert_eq!(violation.reason, Reason::Unknown);
        assert!(violation.detail.contains("authored_at"), "{violation:?}");
    }

    #[test]
    fn a_query_computing_nothing_is_refused() {
        let violation = only(
            r#"{"dataset": "git_commits", "aggregates": [],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "aggregates");
        assert_eq!(violation.reason, Reason::Missing);
    }

    #[test]
    fn an_aggregate_over_a_column_that_is_not_a_measurable_is_refused() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "added", "fn": "sum", "field": "repository"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "aggregates[0].field");
        assert_eq!(violation.reason, Reason::Unknown);
        assert!(violation.detail.contains("lines_added"), "{violation:?}");
    }

    #[test]
    fn two_answer_columns_may_not_share_a_name() {
        let repeated = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"},
                                {"name": "commits", "fn": "sum", "field": "lines_added"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(repeated.field, "aggregates[1].name");
        assert_eq!(repeated.reason, Reason::Duplicate);

        let shadowing = only(
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "repository"}],
                 "aggregates": [{"name": "repository", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(shadowing.field, "aggregates[0].name");
        assert_eq!(shadowing.reason, Reason::Duplicate);
    }

    #[test]
    fn an_aggregate_name_outside_the_answer_name_shape_is_refused() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "Commits Merged", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "aggregates[0].name");
        assert_eq!(violation.reason, Reason::Malformed);
    }

    #[test]
    fn an_in_filter_takes_between_one_value_and_the_cap() {
        let over = format!(
            "[{}]",
            (0..=MAX_FILTER_VALUES)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        for values in ["[]", over.as_str()] {
            let violation = only(&format!(
                r#"{{"dataset": "git_commits",
                     "filters": [{{"field": "lines_added", "op": "in", "values": {values}}}],
                     "aggregates": [{{"name": "commits", "fn": "count"}}],
                     "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
            ));
            assert_eq!(violation.field, "filters[0].values");
            assert_eq!(violation.reason, Reason::OutOfRange);
        }
    }
    #[test]
    fn a_measurable_filter_compares_against_numbers_only() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "filters": [{"field": "lines_added", "op": "gte", "value": "500"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "filters[0].value");
        assert_eq!(violation.reason, Reason::TypeMismatch);
    }

    #[test]
    fn an_ordered_test_over_a_dimension_is_refused_rather_than_compared_as_text() {
        let filters = [
            r#"{"field": "repository", "op": "gt", "value": "b"}"#,
            r#"{"field": "repository", "op": "lte", "value": "b"}"#,
            r#"{"field": "repository", "op": "between", "low": "a", "high": "b"}"#,
        ];
        for filter in filters {
            let violation = only(&format!(
                r#"{{"dataset": "git_commits",
                     "filters": [{filter}],
                     "aggregates": [{{"name": "commits", "fn": "count"}}],
                     "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
            ));
            assert_eq!(violation.field, "filters[0].op", "{filter}");
            assert_eq!(violation.reason, Reason::Unexpected, "{filter}");
        }
    }

    #[test]
    fn a_pattern_test_is_admitted_over_a_dimension_and_refused_over_a_measurable() {
        let dataset = fixtures::commits();
        for op in ["like", "match"] {
            let request = fixtures::query(&format!(
                r#"{{"dataset": "git_commits",
                     "filters": [{{"field": "repository", "op": "{op}", "value": "example/%"}}],
                     "aggregates": [{{"name": "commits", "fn": "count"}}],
                     "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
            ));
            plan_against(&request, &dataset).unwrap_or_else(|v| panic!("{op}: {v:?}"));

            let violation = only(&format!(
                r#"{{"dataset": "git_commits",
                     "filters": [{{"field": "lines_added", "op": "{op}", "value": "1%"}}],
                     "aggregates": [{{"name": "commits", "fn": "count"}}],
                     "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
            ));
            assert_eq!(violation.field, "filters[0].op", "{op}");
            assert_eq!(violation.reason, Reason::Unexpected, "{op}");
        }
    }

    #[test]
    fn a_pattern_that_is_not_a_regular_expression_or_is_too_long_is_refused() {
        let malformed = only(
            r#"{"dataset": "git_commits",
                 "filters": [{"field": "repository", "op": "match", "value": "("}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(malformed.field, "filters[0].value");
        assert_eq!(malformed.reason, Reason::Malformed);

        let long = "%".repeat(MAX_PATTERN_CHARS + 1);
        let too_long = only(&format!(
            r#"{{"dataset": "git_commits",
                 "filters": [{{"field": "repository", "op": "like", "value": "{long}"}}],
                 "aggregates": [{{"name": "commits", "fn": "count"}}],
                 "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
        ));
        assert_eq!(too_long.field, "filters[0].value");
        assert_eq!(too_long.reason, Reason::OutOfRange);
    }

    #[test]
    fn a_filter_over_a_column_that_is_neither_a_dimension_nor_a_measurable_is_refused() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "filters": [{"field": "message", "op": "eq", "value": "x"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "filters[0].field");
        assert_eq!(violation.reason, Reason::Unknown);
        assert!(violation.detail.contains("lines_added"), "{violation:?}");
    }

    #[test]
    fn a_conditional_aggregate_filter_is_refused_on_its_own_path() {
        let violation = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count",
                                 "filter": {"field": "branch", "op": "eq", "value": "main"}}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        assert_eq!(violation.field, "aggregates[0].filter.field");
        assert_eq!(violation.reason, Reason::Unknown);
    }

    #[test]
    fn an_order_term_names_a_column_the_answer_reports() {
        let unknown = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"},
                 "order": [{"by": "repository"}]}"#,
        );
        assert_eq!(unknown.field, "order[0].by");
        assert_eq!(unknown.reason, Reason::Unknown);
        assert!(unknown.detail.contains("commits"), "{unknown:?}");

        let repeated = only(
            r#"{"dataset": "git_commits",
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"},
                 "order": [{"by": "commits"}, {"by": "commits", "dir": "desc"}]}"#,
        );
        assert_eq!(repeated.field, "order[1].by");
        assert_eq!(repeated.reason, Reason::Duplicate);
    }

    #[test]
    fn every_problem_in_one_query_is_reported_at_once() {
        let violations = refuse(
            r#"{"dataset": "git_commits",
                 "group_by": [{"axis": "dimension", "field": "branch"}],
                 "aggregates": [{"name": "added", "fn": "sum", "field": "branch_scope"}],
                 "time": {"from": "2026-03-31", "to": "2026-01-01"},
                 "limit": 0}"#,
        );

        let fields: Vec<&str> = violations
            .iter()
            .map(|violation| violation.field.as_str())
            .collect();
        assert_eq!(
            fields,
            vec![
                "group_by[0].field",
                "aggregates[0].field",
                "time.from",
                "limit"
            ]
        );
    }

    fn refused_by_the_build(json: &str) -> Vec<Violation> {
        match plan(&fixtures::query(json)) {
            Err(PlanError::Refused(violations)) => violations,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_dataset_this_build_does_not_declare_is_refused_with_the_declared_keys() {
        let violations = refused_by_the_build(
            r#"{"dataset": "git_tags",
                 "aggregates": [{"name": "tags", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].field, "dataset");
        assert!(violations[0].detail.contains("git_commits"));
    }

    #[test]
    fn a_dimension_another_dataset_declares_is_still_refused_here() {
        let axis = r#""group_by": [{"axis": "dimension", "field": "file_extension"}]"#;
        let query = |dataset: &str| {
            format!(
                r#"{{"dataset": "{dataset}", {axis},
                     "aggregates": [{{"name": "changes", "fn": "count"}}],
                     "time": {{"from": "2026-01-01", "to": "2026-01-31"}}}}"#
            )
        };

        plan(&fixtures::query(&query("git_file_changes")))
            .expect("the file-change dataset groups by it");

        let violations = refused_by_the_build(&query("git_commits"));
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].field, "group_by[0].field");
        assert_eq!(violations[0].reason, Reason::Unknown);
        let (_, admissible) = violations[0]
            .detail
            .split_once("declared: ")
            .expect("the refusal reports what would have been admissible");
        assert!(
            !admissible.contains("file_extension"),
            "the admissible set must be the commit dataset's own: {admissible:?}"
        );
        assert!(admissible.contains("repository"), "{admissible:?}");
    }

    #[test]
    fn the_shipped_dataset_answers_the_same_query_the_fixture_does() {
        plan(&fixtures::query(COUNT_BY_WEEK)).expect("the shipped declaration admits it");
    }

    #[test]
    fn a_filter_over_a_nullable_dimension_binds_to_the_dimension_not_the_column() {
        let (dataset, request) = accept(
            r#"{"dataset": "git_commits",
                 "filters": [{"field": "source_id", "op": "eq", "value": "__unknown__"}],
                 "aggregates": [{"name": "commits", "fn": "count"}],
                 "time": {"from": "2026-01-01", "to": "2026-01-31"}}"#,
        );
        let plan = plan_against(&request, &dataset).expect("admissible");

        assert!(matches!(
            plan.filters[0].target,
            FilterTarget::Dimension(dimension) if dimension.absent_value.is_some()
        ));
    }
}
