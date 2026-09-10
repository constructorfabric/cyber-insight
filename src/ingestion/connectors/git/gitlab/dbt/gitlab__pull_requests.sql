-- depends_on: {{ ref('gitlab__bronze_promoted') }}
{{ config(
    materialized='incremental',
    unique_key='unique_key',
    order_by=['unique_key'],
    settings={'allow_nullable_key': 1},
    schema='staging',
    tags=['gitlab', 'silver:class_git_pull_requests']
) }}

-- GitLab merge request -> class_git_pull_requests. pr_id and pr_number both use
-- the per-project iid, because the merge-request child streams (commits, notes,
-- approvals) are keyed by iid and must join back on it. state is normalised to
-- the class's uppercase vocabulary (opened -> OPEN). files_changed / lines are
-- not collected by the merge_requests stream (emitted as 0).
WITH proj AS (
    SELECT
        tenant_id,
        source_id,
        id AS project_id,
        COALESCE(namespace_full_path, '') AS project_key,
        COALESCE(path, '') AS repo_slug
    FROM {{ source('bronze_gitlab', 'projects') }} FINAL
),
usr AS (
    SELECT
        tenant_id,
        source_id,
        username,
        lower(COALESCE(NULLIF(email, ''), NULLIF(public_email, ''))) AS email
    FROM {{ source('bronze_gitlab', 'users') }} FINAL
    WHERE COALESCE(username, '') != ''
)
SELECT
    mr.tenant_id AS tenant_id,
    mr.source_id AS source_id,
    mr.unique_key AS unique_key,
    COALESCE(p.project_key, '') AS project_key,
    COALESCE(p.repo_slug, '') AS repo_slug,
    COALESCE(mr.iid, 0) AS pr_id,
    COALESCE(mr.iid, 0) AS pr_number,
    COALESCE(mr.title, '') AS title,
    COALESCE(mr.description, '') AS description,
    multiIf(
        mr.state = 'opened', 'OPEN',
        mr.state = 'closed', 'CLOSED',
        mr.state = 'merged', 'MERGED',
        mr.state = 'locked', 'LOCKED',
        upper(COALESCE(mr.state, ''))
    ) AS state,
    COALESCE(mr.author_username, '') AS author_name,
    COALESCE(u.email, '') AS author_email,
    -- The numeric GitLab user id, stringified — the same key
    -- gitlab__identity_inputs binds accounts on, so attribution survives an
    -- author GitLab exposes no address for. '' when the source reports no
    -- author, which leaves resolution on author_email.
    if(COALESCE(mr.author_id, 0) > 0, toString(COALESCE(mr.author_id, 0)), '') AS author_account_id,
    COALESCE(mr.source_branch, '') AS source_branch,
    COALESCE(mr.target_branch, '') AS destination_branch,
    parseDateTimeBestEffortOrNull(mr.created_at) AS created_on,
    parseDateTimeBestEffortOrNull(mr.updated_at) AS updated_on,
    -- A request that merged closed when it merged. `closed_at` describes one
    -- closed WITHOUT merging, and a request closed, reopened and then merged
    -- keeps that earlier close — taking it would end the interval before the
    -- merge and date the merge to the wrong day.
    parseDateTimeBestEffortOrNull(
        COALESCE(nullIf(mr.merged_at, ''), nullIf(mr.closed_at, ''))
    ) AS closed_on,
    -- GitLab reports the close time itself, so there is nothing to derive.
    parseDateTimeBestEffortOrNull(
        COALESCE(nullIf(mr.merged_at, ''), nullIf(mr.closed_at, ''))
    ) AS closed_on_reported,
    COALESCE(mr.merge_commit_sha, '') AS merge_commit_hash,
    -- GitLab exposes no diff-stat stream for a merge request, so these were
    -- never collected. NULL says that; 0 would claim the request changed
    -- nothing, and the class columns are nullable to keep the two apart.
    CAST(NULL AS Nullable(Int64)) AS files_changed,
    CAST(NULL AS Nullable(Int64)) AS lines_added,
    CAST(NULL AS Nullable(Int64)) AS lines_removed,
    'insight_gitlab' AS data_source,
    toUnixTimestamp64Milli(now64()) AS _version,
    mr._airbyte_extracted_at
-- FINAL: collapse RMT bronze to one row per merge request before staging, so
-- a transient pre-merge duplicate cannot tie the class dedup and let stale MR
-- state (e.g. an old OPEN row) win over the latest.
FROM {{ source('bronze_gitlab', 'merge_requests') }} AS mr FINAL
LEFT JOIN proj AS p
    ON p.project_id = mr.project_id
    AND p.tenant_id = mr.tenant_id
    AND p.source_id = mr.source_id
LEFT JOIN usr AS u
    ON u.username = mr.author_username
    AND u.tenant_id = mr.tenant_id
    AND u.source_id = mr.source_id
{% if is_incremental() %}
WHERE mr._airbyte_extracted_at > (SELECT max(_airbyte_extracted_at) FROM {{ this }})
{% endif %}
