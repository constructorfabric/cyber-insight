//! Canonical on-disk form of a service's `OpenAPI` document, and the drift
//! check that pins the committed copy under `docs/components/backend/` to it.
//!
//! Regenerate a committed document with
//! `(cd src/backend && cargo run -p <service> -- openapi) > docs/components/backend/<service>/openapi.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use utoipa::openapi::OpenApi;

/// Sorted keys, two-space indent, trailing newline — the `jq -S .` form, so a
/// committed document diffs by content only, never by emitter key order.
///
/// # Errors
/// Serialization failure of the document.
pub fn canonical_json(document: &OpenApi) -> serde_json::Result<String> {
    // INVARIANT: key sorting relies on `serde_json::Value` staying
    // `BTreeMap`-backed, i.e. no `preserve_order` feature in the workspace.
    let value = serde_json::to_value(document)?;
    let mut text = serde_json::to_string_pretty(&value)?;
    text.push('\n');
    Ok(text)
}

#[derive(Debug, thiserror::Error)]
pub enum DriftError {
    #[error("serializing the OpenAPI document: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("reading {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error(
        "{path} is stale at line {line}:\n  committed: {committed}\n  generated: {generated}\nRegenerate: {regenerate}"
    )]
    Stale {
        path: PathBuf,
        line: usize,
        committed: String,
        generated: String,
        regenerate: String,
    },
}

/// Fails when the committed document of `service` differs from the canonical
/// form of `document`. `manifest_dir` is the service crate's `CARGO_MANIFEST_DIR`.
///
/// # Errors
/// [`DriftError::Stale`] on drift, [`DriftError::Read`] when the committed
/// file is unreadable, [`DriftError::Serialize`] when the document is not.
pub fn check_committed(
    document: &OpenApi,
    manifest_dir: &str,
    service: &str,
) -> Result<(), DriftError> {
    let repo_path = committed_document_repo_path(service);
    let path = committed_document_path(manifest_dir, &repo_path);
    let committed =
        fs::read_to_string(&path).map_err(|source| DriftError::Read { path, source })?;
    let generated = canonical_json(document)?;

    match first_difference(&committed, &generated) {
        None => Ok(()),
        Some(difference) => Err(DriftError::Stale {
            path: repo_path,
            line: difference.line,
            committed: difference.committed,
            generated: difference.generated,
            regenerate: regenerate_command(service),
        }),
    }
}

fn committed_document_repo_path(service: &str) -> PathBuf {
    Path::new("docs/components/backend")
        .join(service)
        .join("openapi.json")
}

fn committed_document_path(manifest_dir: &str, repo_path: &Path) -> PathBuf {
    // INVARIANT: service crates live at src/backend/services/<service>, four
    // levels below the repository root.
    Path::new(manifest_dir).join("../../../..").join(repo_path)
}

fn regenerate_command(service: &str) -> String {
    format!(
        "(cd src/backend && cargo run -p {service} -- openapi) > docs/components/backend/{service}/openapi.json"
    )
}

#[derive(Debug, PartialEq, Eq)]
struct Difference {
    line: usize,
    committed: String,
    generated: String,
}

/// The first line where the two texts diverge, `None` when they are identical.
/// A missing line is reported as `<end of file>`.
fn first_difference(committed: &str, generated: &str) -> Option<Difference> {
    const END: &str = "<end of file>";

    let mut committed_lines = committed.lines();
    let mut generated_lines = generated.lines();
    let mut line = 0;
    loop {
        line += 1;
        match (committed_lines.next(), generated_lines.next()) {
            (None, None) => return None,
            (Some(c), Some(g)) if c == g => {}
            (c, g) => {
                return Some(Difference {
                    line,
                    committed: c.unwrap_or(END).to_owned(),
                    generated: g.unwrap_or(END).to_owned(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
