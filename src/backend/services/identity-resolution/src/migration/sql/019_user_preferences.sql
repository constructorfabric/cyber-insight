CREATE TABLE IF NOT EXISTS user_preferences (
    insight_tenant_id BINARY(16) NOT NULL,
    person_id BINARY(16) NOT NULL,
    timezone VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    PRIMARY KEY (insight_tenant_id, person_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
