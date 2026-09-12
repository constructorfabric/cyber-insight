-- depends_on: {{ ref('claude_team__bronze_promoted') }}
{{ config(
    materialized='table',
    engine='ReplacingMergeTree',
    order_by=['unique_key'],
    settings={'allow_nullable_key': 1},
    schema='staging',
    tags=['claude-team']
) }}

-- Flattens the `account` JSON column of bronze_claude_team.claude_team_members.
-- The snapshot macro hashes named columns and fields_history tracks them by
-- name, so neither can reach a JSON subfield; the identity chain needs them as
-- ordinary columns. FINAL dedups the promoted ReplacingMergeTree source before
-- the snapshot compares versions (ADR-0001).

SELECT
    tenant_id,
    source_id,
    unique_key,
    toString(account.uuid)          AS account_uuid,
    toString(account.email_address) AS email_address,
    toString(account.full_name)     AS full_name
FROM {{ source('bronze_claude_team', 'claude_team_members') }} FINAL
