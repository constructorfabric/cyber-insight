//! The metric, widget and dashboard operations, over the stores they need.

use serde_json::Value;
use thiserror::Error;

use crate::definitions::{DefinitionKind, DefinitionName, DefinitionStoreError, Definitions};
use crate::metric_query::{MetricQuery, MetricQueryError, MetricRunError, MetricRunner, RunResult};
use crate::widget::{Widget, WidgetError};

#[cfg(test)]
mod tests;

#[derive(Debug, Error)]
pub(crate) enum CustomError {
    #[error("{} `{name}` was not found", kind.singular())]
    NotFound { kind: DefinitionKind, name: String },
    #[error("still in use by {}", used_by.join(", "))]
    InUse { used_by: Vec<String> },
    #[error(transparent)]
    Widget(WidgetError),
    #[error("definition body is not valid: {0}")]
    Body(serde_json::Error),
    #[error(transparent)]
    Compile(MetricQueryError),
    #[error(transparent)]
    Run(MetricRunError),
    #[error(transparent)]
    Store(DefinitionStoreError),
}

#[derive(Debug)]
pub(crate) struct Surfaces<'a> {
    definitions: &'a dyn Definitions,
    metrics: &'a MetricRunner,
}

impl<'a> Surfaces<'a> {
    pub(crate) fn new(definitions: &'a dyn Definitions, metrics: &'a MetricRunner) -> Self {
        Self {
            definitions,
            metrics,
        }
    }

    pub(crate) async fn list(&self, kind: DefinitionKind) -> Result<Vec<String>, CustomError> {
        self.definitions
            .list(kind)
            .await
            .map_err(CustomError::Store)
    }

    pub(crate) async fn get(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<Value, CustomError> {
        self.definitions
            .get(kind, name)
            .await
            .map_err(CustomError::Store)?
            .ok_or_else(|| CustomError::NotFound {
                kind,
                name: name.as_str().to_owned(),
            })
    }

    pub(crate) async fn put(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
        body: &Value,
    ) -> Result<(), CustomError> {
        if kind == DefinitionKind::Widget {
            self.check_widget(body).await?;
        }

        self.definitions
            .put(kind, name, body)
            .await
            .map_err(CustomError::Store)
    }

    pub(crate) async fn delete(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<(), CustomError> {
        let used_by = self.dependents_of(kind, name).await?;
        if !used_by.is_empty() {
            return Err(CustomError::InUse { used_by });
        }

        let removed = self
            .definitions
            .delete(kind, name)
            .await
            .map_err(CustomError::Store)?;

        if removed {
            return Ok(());
        }

        Err(CustomError::NotFound {
            kind,
            name: name.as_str().to_owned(),
        })
    }

    pub(crate) async fn run_metric(&self, name: &DefinitionName) -> Result<RunResult, CustomError> {
        let body = self.get(DefinitionKind::Metric, name).await?;

        let metric: MetricQuery = serde_json::from_value(body).map_err(CustomError::Body)?;
        let compiled = metric.compile().map_err(CustomError::Compile)?;

        self.metrics.run(&compiled).await.map_err(CustomError::Run)
    }

    pub(crate) async fn check_widget(&self, body: &Value) -> Result<(), CustomError> {
        let widget: Widget = serde_json::from_value(body.clone())
            .map_err(|error| CustomError::Widget(error.into()))?;

        let metric_name = DefinitionName::parse(widget.metric())
            .map_err(|_| CustomError::Widget(WidgetError::NoMetric(widget.metric().to_owned())))?;

        let stored = self
            .definitions
            .get(DefinitionKind::Metric, &metric_name)
            .await
            .map_err(CustomError::Store)?
            .ok_or_else(|| {
                CustomError::Widget(WidgetError::NoMetric(widget.metric().to_owned()))
            })?;

        let metric: MetricQuery =
            serde_json::from_value(stored).map_err(|error| CustomError::Widget(error.into()))?;

        widget.check_against(&metric).map_err(CustomError::Widget)
    }

    async fn dependents_of(
        &self,
        kind: DefinitionKind,
        name: &DefinitionName,
    ) -> Result<Vec<String>, CustomError> {
        let (holder, needle) = match kind {
            DefinitionKind::Metric => (DefinitionKind::Widget, "metric"),
            DefinitionKind::Widget => (DefinitionKind::Dashboard, "widgets"),
            DefinitionKind::Dashboard => return Ok(Vec::new()),
        };

        let mut used_by = Vec::new();
        for holder_name in self.list(holder).await? {
            let Ok(parsed) = DefinitionName::parse(&holder_name) else {
                continue;
            };
            let Some(body) = self
                .definitions
                .get(holder, &parsed)
                .await
                .map_err(CustomError::Store)?
            else {
                continue;
            };

            let names = match body.get(needle) {
                Some(Value::String(one)) => vec![one.as_str()],
                Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            if names.contains(&name.as_str()) {
                used_by.push(holder_name);
            }
        }

        Ok(used_by)
    }
}
