//! A query bound to its dataset, with every field reference already resolved.
//!
//! INVARIANT: a plan exists only for a query [`super::validation`] accepted, so
//! the compiler never re-asks whether anything is declared.

use chrono::NaiveDate;

use super::contract::dto::{AnswerColumn, Direction, Grain, ScalarDto};
use crate::domain::datasets::declaration::{Dataset, Dimension, Measurable, TimeField};
pub use crate::domain::datasets::label_column_name;

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPlan<'d> {
    pub dataset: &'d Dataset,
    pub filters: Vec<PlannedFilter<'d>>,
    pub group_by: Vec<PlannedAxis<'d>>,
    pub aggregates: Vec<PlannedAggregate<'d>>,
    pub time: PlannedTime<'d>,
    pub order: Vec<PlannedOrder>,
    pub limit: u32,
    pub columns: Vec<PlannedColumn<'d>>,
}

/// One column of the answer and what the statement selects for it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedColumn<'d> {
    pub answer: AnswerColumn,
    pub source: ColumnSource<'d>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnSource<'d> {
    Dimension(&'d Dimension),
    /// The declared label of a dimension the answer also carries the value of.
    Label(&'d Dimension),
    Bucket,
    /// Index into [`QueryPlan::aggregates`].
    Aggregate(usize),
}

impl PlannedColumn<'_> {
    /// Whether the statement groups by this column.
    pub fn groups(&self) -> bool {
        match self.source {
            ColumnSource::Dimension(_) | ColumnSource::Bucket => true,
            ColumnSource::Label(_) | ColumnSource::Aggregate(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedAxis<'d> {
    Dimension(&'d Dimension),
    Time,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFilter<'d> {
    pub target: FilterTarget<'d>,
    pub test: PlannedTest,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlannedTest {
    Eq(ScalarDto),
    /// INVARIANT: validation admits between one value and the contract's cap.
    In(Vec<ScalarDto>),
    Compare(CompareOp, ScalarDto),
    Between {
        low: ScalarDto,
        high: ScalarDto,
    },
    NotNull,
    /// INVARIANT: validation admits a pattern only over a dimension and within the length cap.
    Like(String),
    /// INVARIANT: validation compiled the expression, so the warehouse will too.
    Matches(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
}

impl PlannedTest {
    pub fn operands(&self) -> Vec<(String, &ScalarDto)> {
        match self {
            Self::Eq(value) | Self::Compare(_, value) => vec![("value".to_owned(), value)],
            Self::In(values) => values
                .iter()
                .enumerate()
                .map(|(index, value)| (format!("values[{index}]"), value))
                .collect(),
            Self::Between { low, high } => {
                vec![("low".to_owned(), low), ("high".to_owned(), high)]
            }
            Self::NotNull | Self::Like(_) | Self::Matches(_) => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterTarget<'d> {
    /// Compared against the value the answer reports, never the raw column.
    Dimension(&'d Dimension),
    /// Compared against the numeric column itself.
    Measurable(&'d Measurable),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAggregate<'d> {
    /// Position in the request's `aggregates`, which a refusal names.
    pub index: usize,
    pub name: String,
    pub fold: PlannedFold<'d>,
    pub filter: Option<PlannedFilter<'d>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedFold<'d> {
    Rows,
    Values {
        function: FoldFn,
        measurable: &'d Measurable,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldFn {
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTime<'d> {
    pub field: &'d TimeField,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Present exactly when the query carries a time axis.
    pub grain: Option<Grain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedOrder {
    /// Index into [`QueryPlan::columns`].
    pub column: usize,
    pub direction: Direction,
}

/// The name the bucket column answers under, and an order term reaches it by.
pub const BUCKET_COLUMN: &str = "time";

// SAFETY: aliases are positional and engine-owned, so no caller-supplied string
// reaches the SQL text, and no alias can shadow the column it reads.
pub fn column_alias(index: usize) -> String {
    format!("c{index}")
}
