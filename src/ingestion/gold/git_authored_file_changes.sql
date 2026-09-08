{{ config(
    materialized='table',
    engine='MergeTree',
    order_by=['tenant_id', 'data_source', 'commit_hash'],
    schema=var('gold_database'),
    settings={'allow_nullable_key': 1},
    tags=['gold'],
    query_settings=metric_serving_query_settings(
        query_settings_overrides={
            'max_memory_usage': 3221225472,
            'max_bytes_before_external_sort': 805306368
        }
    )
) }}

-- One row per change CONTENT, not per commit that carries it. The same content
-- entering a repository on two lines of history — a branch whose copy of a
-- tree also landed on the default branch, a cherry-pick, a squash that
-- re-applies its branch's whole span, a reverted-then-restored file — is one
-- authored change, and summing every carrier's diff would count those lines
-- more than once. `git_file_content_identity` is what "same content" means.
--
-- Earliest commit wins, so the value lands in the period the content was first
-- authored and does not move when a later commit repeats it.
--
-- The commit_hash tie-breaker keeps rows whose identity is UNKNOWN (a source
-- that reports no oid, or a row collected before the proxy did) distinct per
-- commit: without it every such row for one path would collapse into one,
-- because LIMIT 1 BY reads their NULL keys as equal.
--
-- The superseded set is the case content identity alone cannot reach: a squash
-- whose branch was only PARTLY collected carries the collected commits' work
-- under a span no single commit made, so nothing folds it. See
-- git_superseded_file_changes.
--
-- Materialized so the sort behind LIMIT 1 BY runs once per build in its own
-- query budget, and every reader — the metric evidence and the query datasets
-- — scans the same rows rather than re-deriving them.

SELECT
    tenant_id,
    source_id,
    project_key,
    repo_slug,
    commit_hash,
    data_source,
    file_path,
    file_extension,
    change_type,
    lines_added,
    lines_removed
FROM {{ ref('git_commit_file_changes') }}
-- SAFETY: tenant_id is Nullable, and a tuple holding NULL never matches under
-- NOT IN — the superseded row would survive and count twice.
WHERE (coalesce(tenant_id, ''), data_source, commit_hash, file_path) NOT IN (
    SELECT coalesce(tenant_id, ''), data_source, commit_hash, file_path
    FROM {{ ref('git_superseded_file_changes') }}
)
-- INVARIANT: committer_date breaks the tie, and must stay ahead of the hash.
-- observed_at is the AUTHOR date, which a rebase preserves — so the copy and
-- its original tie there, and without this the survivor (and with it the
-- repository and branch scope the lines are filed under) would be decided by
-- comparing hashes. #3153
ORDER BY observed_at, committer_date, commit_hash
LIMIT 1 BY
    tenant_id,
    data_source,
    project_key,
    repo_slug,
    file_path,
    {{ git_file_content_identity('post_image_oid', 'pre_image_oid') }},
    if(
        coalesce(pre_image_oid, '') = ''
            AND coalesce(post_image_oid, '') = '',
        commit_hash,
        ''
    )
