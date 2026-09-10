-- The class model owns this column but never adds it to a warm relation: the
-- DDL snapshot is IF NOT EXISTS, and the deploy hook builds tag:gold, not the
-- tag:silver model that would widen it — so gold reads it in the same run.
-- AFTER anchors the contract position the positional insert requires; MODIFY
-- converges an instance where an out-of-band ALTER placed it elsewhere.
-- Idempotent: this channel has no ledger and re-runs on every deploy.
--
-- Existing rows heal to NULL, which reads as "the source stated no close time"
-- and so drops them from the duration measures until the next sync refills the
-- column. A count still has `closed_on` and is unaffected. #3362
ALTER TABLE silver.class_git_pull_requests
    ADD COLUMN IF NOT EXISTS closed_on_reported Nullable(DateTime) AFTER closed_on;

ALTER TABLE silver.class_git_pull_requests
    MODIFY COLUMN closed_on_reported Nullable(DateTime) AFTER closed_on;
