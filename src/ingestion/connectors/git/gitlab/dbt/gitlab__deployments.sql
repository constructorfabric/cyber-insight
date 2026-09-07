-- depends_on: {{ ref('gitlab__bronze_promoted') }}
{{ config(
    materialized='incremental',
    unique_key='unique_key',
    order_by=['unique_key'],
    settings={'allow_nullable_key': 1},
    schema='staging',
    tags=['gitlab', 'silver:class_git_deployments']
) }}

-- GitLab deployments -> the vendor-neutral deployment class. Bronze keeps one
-- row per (deployment, status); the deployment itself is the newest of them,
-- and the outcome lives in class_git_deployment_events.
--
-- is_production follows the environment tier GitLab assigns. GitLab marks no
-- environment as ephemeral — a review app is an ordinary environment that
-- gets stopped — so is_transient is 0 for every row.
WITH latest_status AS (
    SELECT
        tenant_id,
        source_id,
        project_id,
        id,
        repo_path,
        ref,
        sha,
        environment_name,
        environment_tier,
        user_username,
        created_at,
        _airbyte_extracted_at
    FROM {{ source('bronze_gitlab', 'deployments') }} FINAL
    ORDER BY parseDateTimeBestEffortOrNull(updated_at) DESC
    LIMIT 1 BY tenant_id, source_id, project_id, id
)

SELECT
    tenant_id,
    source_id,
    concat(COALESCE(tenant_id, ''), ':', COALESCE(source_id, ''), ':', toString(COALESCE(project_id, 0)), ':', toString(COALESCE(id, 0))) AS unique_key,
    COALESCE(repo_path, '') AS repo_full_name,
    toString(COALESCE(id, 0)) AS deployment_id,
    COALESCE(environment_name, '') AS environment,
    if(COALESCE(environment_tier, '') = 'production', 1, 0) AS is_production,
    0 AS is_transient,
    COALESCE(ref, '') AS ref,
    COALESCE(sha, '') AS commit_sha,
    '' AS task,
    COALESCE(user_username, '') AS creator_login,
    parseDateTimeBestEffortOrNull(created_at) AS created_at,
    'insight_gitlab' AS data_source,
    toUnixTimestamp64Milli(now64()) AS _version,
    _airbyte_extracted_at
FROM latest_status
{% if is_incremental() %}
WHERE _airbyte_extracted_at > (SELECT max(_airbyte_extracted_at) FROM {{ this }})
{% endif %}
