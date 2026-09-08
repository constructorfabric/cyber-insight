{{ config(
    materialized='table',
    engine='MergeTree',
    order_by=['tenant_id', 'author_email', 'authored_at'],
    partition_by='toYYYYMM(authored_date)',
    schema=var('gold_database'),
    settings={'allow_nullable_key': 1},
    tags=['gold'],
    query_settings=metric_serving_query_settings()
) }}

-- Authored file changes: one row per path and change type a commit touched.

WITH
-- INVARIANT: one row per (tenant, source, commit, path, lower-cased change
-- type) is the dataset's row identity. Two content identities under one such
-- key fold into one row carrying the sum of their lines, so every line total
-- agrees with the metric evidence.
folded_file_changes AS (
    SELECT
        tenant_id,
        data_source,
        commit_hash,
        file_path,
        lower(change_type) AS change_kind,
        min(lower(file_extension)) AS extension,
        sum(lines_added) AS lines_added,
        sum(lines_removed) AS lines_removed
    FROM {{ ref('git_authored_file_changes') }}
    GROUP BY tenant_id, data_source, commit_hash, file_path, lower(change_type)
)

SELECT
    commits.tenant_id AS tenant_id,
    commits.source_id AS source_id,
    commits.project_key AS project_key,
    commits.repo_slug AS repo_slug,
    commits.commit_hash AS commit_hash,
    file_changes.file_path AS file_path,
    commits.entity_id AS author_email,
    commits.author_name AS author_name,
    -- SAFETY: the commit stage admits no row without a date, and neither a
    -- partition key nor a sort key may be nullable.
    assumeNotNull(commits.observed_at) AS authored_at,
    assumeNotNull(commits.metric_date) AS authored_date,
    {{ git_file_category('file_changes.file_path') }} AS category,
    {{ git_file_category_label('category') }} AS category_label,
    if(file_changes.extension = '', '__unknown__', file_changes.extension) AS file_extension,
    if(file_changes.extension = '', 'Unknown', file_changes.extension) AS file_extension_label,
    if(file_changes.change_kind = '', '__unknown__', file_changes.change_kind) AS change_type,
    multiIf(
        file_changes.change_kind = '', 'Unknown',
        file_changes.change_kind = 'added', 'Added',
        file_changes.change_kind = 'modified', 'Modified',
        file_changes.change_kind = 'renamed', 'Renamed',
        file_changes.change_kind = 'deleted', 'Deleted',
        file_changes.change_kind
    ) AS change_type_label,
    -- INVARIANT: inherited, not recomputed — lines belong to the bucket their
    -- commit belongs to.
    commits.branch_scope_value AS branch_scope,
    commits.branch_scope_label AS branch_scope_label,
    commits.repository_value AS repository,
    commits.repository_label AS repository_label,
    commits.project_value AS project,
    commits.project_label AS project_label,
    commits.source_value AS source,
    commits.source_label AS source_label,
    file_changes.lines_added AS lines_added,
    file_changes.lines_removed AS lines_removed
FROM folded_file_changes AS file_changes
-- SAFETY: tenant_id is Nullable on the class, and a plain `=` never matches
-- NULL to NULL — which would drop a whole tenant's file changes.
INNER JOIN {{ ref('git_authored_commits') }} AS commits
    ON commits.tenant_id IS NOT DISTINCT FROM file_changes.tenant_id
    AND commits.data_source = file_changes.data_source
    AND commits.commit_hash = file_changes.commit_hash
