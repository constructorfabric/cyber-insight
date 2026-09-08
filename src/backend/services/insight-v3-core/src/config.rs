use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use thiserror::Error;

const DEFAULT_CLICKHOUSE_DATABASE: &str = "insight";
const DEFAULT_IDENTITY_DATABASE: &str = "identity";
pub(crate) const MIN_INGEST_TOKEN_BYTES: usize = 32;
pub(crate) const MAX_INGEST_TOKEN_BYTES: usize = 1024;
const DEFAULT_CHAT_MODEL: &str = "claude-haiku-4-5-20251001";

/// Whether the chat endpoint calls the model or returns a fixed reply.
///
/// `canned` makes no network call at all — it exists so a recording (or a
/// test) can exercise the chat endpoint without a live model or an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ChatMode {
    Live,
    Canned,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct GearConfig {
    pub(crate) clickhouse_url: String,
    pub(crate) clickhouse_database: String,
    /// The database identity materialises into.
    pub(crate) identity_database: String,
    pub(crate) clickhouse_user: Option<String>,
    pub(crate) clickhouse_password: Option<SecretString>,
    /// The read-only principal the assistant's query path connects as. Blank
    /// on a stand without one, and the query path then uses the pair above.
    pub(crate) clickhouse_query_user: Option<String>,
    pub(crate) clickhouse_query_password: Option<SecretString>,
    pub(crate) ingest_token: SecretString,
    pub(crate) anthropic_token: SecretString,
    pub(crate) chat_mode: ChatMode,
    pub(crate) chat_model: String,
    pub(crate) database_url: String,
    pub(crate) identity_url: String,
}

impl Default for GearConfig {
    fn default() -> Self {
        Self {
            clickhouse_url: String::new(),
            clickhouse_database: DEFAULT_CLICKHOUSE_DATABASE.to_owned(),
            identity_database: DEFAULT_IDENTITY_DATABASE.to_owned(),
            clickhouse_user: None,
            clickhouse_password: None,
            clickhouse_query_user: None,
            clickhouse_query_password: None,
            ingest_token: SecretString::from(String::new()),
            anthropic_token: SecretString::from(String::new()),
            chat_mode: ChatMode::Live,
            chat_model: DEFAULT_CHAT_MODEL.to_owned(),
            database_url: String::new(),
            identity_url: String::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ValidatedConfig {
    clickhouse_url: String,
    clickhouse_database: String,
    identity_database: String,
    clickhouse_user: Option<String>,
    clickhouse_password: Option<SecretString>,
    clickhouse_query_user: Option<String>,
    clickhouse_query_password: Option<SecretString>,
    ingest_token: IngestToken,
    anthropic_token: SecretString,
    chat_mode: ChatMode,
    chat_model: String,
    database_url: String,
    identity_url: String,
}

impl ValidatedConfig {
    pub(crate) fn from_app_config(
        app: &toolkit::bootstrap::AppConfig,
    ) -> Result<Self, ConfigLoadError> {
        let raw = app
            .gears
            .get("insight-v3-core")
            .and_then(|gear| gear.get("config"))
            .ok_or(ConfigLoadError::MissingSection)?;
        let config = serde_json::from_value::<GearConfig>(raw.clone())?;

        config.validate().map_err(ConfigLoadError::Invalid)
    }

    pub(crate) fn clickhouse_client(&self) -> insight_clickhouse::Client {
        self.client_as(
            self.clickhouse_user.as_deref(),
            self.clickhouse_password.as_ref(),
        )
    }

    pub(crate) fn clickhouse_query_client(&self) -> insight_clickhouse::Client {
        match (
            self.clickhouse_query_user.as_deref(),
            self.clickhouse_query_password.as_ref(),
        ) {
            (Some(user), Some(password)) => self.client_as(Some(user), Some(password)),
            _ => self.clickhouse_client(),
        }
    }

    fn client_as(
        &self,
        user: Option<&str>,
        password: Option<&SecretString>,
    ) -> insight_clickhouse::Client {
        let mut config =
            insight_clickhouse::Config::new(&self.clickhouse_url, &self.clickhouse_database);
        if let (Some(user), Some(password)) = (user, password) {
            config = config.with_auth(user, password.expose_secret());
        }

        insight_clickhouse::Client::new(config)
    }

    pub(crate) fn ingest_token(&self) -> &SecretString {
        self.ingest_token.as_secret()
    }

    pub(crate) fn anthropic_token(&self) -> &SecretString {
        &self.anthropic_token
    }

    pub(crate) fn chat_mode(&self) -> ChatMode {
        self.chat_mode
    }

    pub(crate) fn chat_model(&self) -> String {
        self.chat_model.clone()
    }

    pub(crate) fn database_url(&self) -> &str {
        &self.database_url
    }

    pub(crate) fn identity_url(&self) -> &str {
        &self.identity_url
    }

    /// The database gold materialises into — `dbt_project.yml` sets
    /// `gold_database` to the same one this service reads and writes.
    pub(crate) fn clickhouse_database(&self) -> String {
        self.clickhouse_database.clone()
    }

    /// The database holding the names people are known by.
    pub(crate) fn identity_database(&self) -> &str {
        &self.identity_database
    }
}

impl GearConfig {
    pub(crate) fn validate(self) -> Result<ValidatedConfig, ConfigError> {
        require_non_empty("clickhouse_url", &self.clickhouse_url)?;
        require_non_empty("clickhouse_database", &self.clickhouse_database)?;
        require_non_empty("identity_database", &self.identity_database)?;
        require_non_empty("chat_model", &self.chat_model)?;
        require_non_empty("database_url", &self.database_url)?;
        require_non_empty("identity_url", &self.identity_url)?;
        let ingest_token = IngestToken::parse(self.ingest_token)?;
        if self.chat_mode == ChatMode::Live {
            require_non_empty("anthropic_token", self.anthropic_token.expose_secret())?;
        }
        validate_credentials(
            self.clickhouse_user.as_deref(),
            self.clickhouse_password.as_ref(),
        )?;
        let clickhouse_query_user = self
            .clickhouse_query_user
            .filter(|user| !user.trim().is_empty());
        let clickhouse_query_password = self
            .clickhouse_query_password
            .filter(|password| !password.expose_secret().trim().is_empty());
        if clickhouse_query_user.is_some() != clickhouse_query_password.is_some() {
            return Err(ConfigError::IncompleteQueryCredentials);
        }

        Ok(ValidatedConfig {
            clickhouse_url: self.clickhouse_url,
            clickhouse_database: self.clickhouse_database,
            identity_database: self.identity_database,
            clickhouse_user: self.clickhouse_user,
            clickhouse_password: self.clickhouse_password,
            clickhouse_query_user,
            clickhouse_query_password,
            ingest_token,
            anthropic_token: self.anthropic_token,
            chat_mode: self.chat_mode,
            chat_model: self.chat_model,
            database_url: self.database_url,
            identity_url: self.identity_url,
        })
    }
}

#[derive(Debug)]
struct IngestToken(SecretString);

impl IngestToken {
    fn parse(value: SecretString) -> Result<Self, ConfigError> {
        let exposed = value.expose_secret();
        require_non_empty("ingest_token", exposed)?;
        if exposed.len() < MIN_INGEST_TOKEN_BYTES {
            return Err(ConfigError::IngestTokenTooShort);
        }
        if exposed.len() > MAX_INGEST_TOKEN_BYTES {
            return Err(ConfigError::IngestTokenTooLong);
        }
        if !exposed.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(ConfigError::InvalidIngestTokenCharacters);
        }

        Ok(Self(value))
    }

    fn as_secret(&self) -> &SecretString {
        &self.0
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::Empty(field));
    }

    Ok(())
}

fn validate_credentials(
    user: Option<&str>,
    password: Option<&SecretString>,
) -> Result<(), ConfigError> {
    match (user, password) {
        (None, None) => Ok(()),
        (Some(user), Some(password))
            if !user.trim().is_empty() && !password.expose_secret().is_empty() =>
        {
            Ok(())
        }
        (Some(_), Some(_)) => Err(ConfigError::EmptyCredentials),
        (Some(_), None) | (None, Some(_)) => Err(ConfigError::IncompleteCredentials),
    }
}

#[derive(Debug, Error)]
pub(crate) enum ConfigError {
    #[error("gears.insight-v3-core.config.{0} must not be empty")]
    Empty(&'static str),
    #[error("ClickHouse user and password must both be configured or both omitted")]
    IncompleteCredentials,
    #[error("ClickHouse credentials must not be empty")]
    EmptyCredentials,
    #[error("clickhouse_query_user and clickhouse_query_password must both be set or both empty")]
    IncompleteQueryCredentials,
    #[error("ingest_token must be at least {MIN_INGEST_TOKEN_BYTES} bytes")]
    IngestTokenTooShort,
    #[error("ingest_token must be at most {MAX_INGEST_TOKEN_BYTES} bytes")]
    IngestTokenTooLong,
    #[error("ingest_token must contain only non-whitespace ASCII characters")]
    InvalidIngestTokenCharacters,
}

#[derive(Debug, Error)]
pub(crate) enum ConfigLoadError {
    #[error("missing gears.insight-v3-core.config section")]
    MissingSection,
    #[error("invalid gears.insight-v3-core.config: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("invalid gears.insight-v3-core.config: {0}")]
    Invalid(#[source] ConfigError),
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::*;

    fn valid_config() -> GearConfig {
        GearConfig {
            clickhouse_url: "http://clickhouse.example.test:8123".to_owned(),
            clickhouse_database: "insight".to_owned(),
            identity_database: "identity".to_owned(),
            clickhouse_user: None,
            clickhouse_password: None,
            clickhouse_query_user: None,
            clickhouse_query_password: None,
            ingest_token: SecretString::from("test-ingest-token-0123456789abcdef"),
            anthropic_token: SecretString::from("test-anthropic-token"),
            chat_mode: ChatMode::Live,
            chat_model: "claude-haiku-4-5-20251001".to_owned(),
            database_url: "mysql://insight:secret@mariadb.example.test:3306/insight_v3".to_owned(),
            identity_url: "http://identity-resolution.example.test:8082".to_owned(),
        }
    }

    #[test]
    fn required_values_must_not_be_empty() {
        for field in ["clickhouse_url", "clickhouse_database", "ingest_token"] {
            let mut config = valid_config();
            match field {
                "clickhouse_url" => config.clickhouse_url.clear(),
                "clickhouse_database" => config.clickhouse_database.clear(),
                "ingest_token" => config.ingest_token = SecretString::from(String::new()),
                _ => unreachable!(),
            }

            assert!(config.validate().is_err(), "empty {field} must be rejected");
        }
    }

    #[test]
    fn chat_model_must_not_be_empty() {
        let mut config = valid_config();
        config.chat_model.clear();

        assert!(config.validate().is_err());
    }

    #[test]
    fn anthropic_token_is_required_only_in_live_mode() {
        let mut live = valid_config();
        live.anthropic_token = SecretString::from(String::new());
        assert!(live.validate().is_err(), "live mode needs a token");

        let mut canned = valid_config();
        canned.anthropic_token = SecretString::from(String::new());
        canned.chat_mode = ChatMode::Canned;
        assert!(
            canned.validate().is_ok(),
            "canned mode makes no model call and needs no token"
        );
    }

    #[test]
    fn secrets_are_redacted_from_debug_output() {
        let mut config = valid_config();
        config.clickhouse_user = Some("writer".to_owned());
        config.clickhouse_password = Some(SecretString::from("database-secret"));

        let rendered = format!("{config:?}");

        assert!(!rendered.contains("test-ingest-token-0123456789abcdef"));
        assert!(!rendered.contains("database-secret"));
        assert!(!rendered.contains("test-anthropic-token"));
    }

    #[test]
    fn credentials_must_be_complete() {
        let mut config = valid_config();
        config.clickhouse_user = Some("writer".to_owned());

        assert!(matches!(
            config.validate(),
            Err(ConfigError::IncompleteCredentials)
        ));
    }

    #[test]
    fn ingest_token_must_be_usable_in_the_instance_token_header() {
        let invalid_tokens = [
            "token with space".to_owned(),
            "töken".to_owned(),
            "x".repeat(1025),
        ];

        for token in invalid_tokens {
            let mut config = valid_config();
            config.ingest_token = SecretString::from(token);

            assert!(
                config.validate().is_err(),
                "configured token outside the instance-token header contract must be rejected"
            );
        }
    }

    #[test]
    fn ingest_token_accepts_the_wire_size_boundary() {
        let mut config = valid_config();
        config.ingest_token = SecretString::from("x".repeat(MAX_INGEST_TOKEN_BYTES));

        assert!(config.validate().is_ok());
    }

    #[test]
    fn ingest_token_rejects_values_shorter_than_32_bytes() {
        let mut config = valid_config();
        config.ingest_token = SecretString::from("x".repeat(31));

        assert!(config.validate().is_err());
    }

    #[test]
    fn ingest_token_accepts_the_minimum_size_boundary() {
        let mut config = valid_config();
        config.ingest_token = SecretString::from("x".repeat(32));

        assert!(config.validate().is_ok());
    }

    #[test]
    fn the_query_client_falls_back_to_the_ordinary_credentials_when_no_reader_is_configured() {
        let mut config = valid_config();
        config.clickhouse_user = Some("writer".to_owned());
        config.clickhouse_password = Some(SecretString::from("writer-secret"));

        let validated = config
            .validate()
            .unwrap_or_else(|error| panic!("config must be valid: {error}"));
        let client = validated.clickhouse_query_client();

        assert_eq!(client.config().user.as_deref(), Some("writer"));
        assert_eq!(client.config().password.as_deref(), Some("writer-secret"));
    }

    #[test]
    fn the_query_client_uses_the_reader_when_it_is_configured() {
        let mut config = valid_config();
        config.clickhouse_user = Some("writer".to_owned());
        config.clickhouse_password = Some(SecretString::from("writer-secret"));
        config.clickhouse_query_user = Some("reader".to_owned());
        config.clickhouse_query_password = Some(SecretString::from("reader-secret"));

        let validated = config
            .validate()
            .unwrap_or_else(|error| panic!("config must be valid: {error}"));
        let client = validated.clickhouse_query_client();

        assert_eq!(client.config().user.as_deref(), Some("reader"));
        assert_eq!(client.config().password.as_deref(), Some("reader-secret"));
    }

    #[test]
    fn a_blank_reader_setting_reads_as_unset_rather_than_as_a_credential() {
        let mut config = valid_config();
        config.clickhouse_user = Some("writer".to_owned());
        config.clickhouse_password = Some(SecretString::from("writer-secret"));
        config.clickhouse_query_user = Some(String::new());
        config.clickhouse_query_password = Some(SecretString::from(String::new()));

        let validated = config
            .validate()
            .unwrap_or_else(|error| panic!("config must be valid: {error}"));
        let client = validated.clickhouse_query_client();

        assert_eq!(client.config().user.as_deref(), Some("writer"));
    }

    #[test]
    fn half_a_reader_credential_is_refused_instead_of_falling_back() {
        let mut config = valid_config();
        config.clickhouse_query_user = Some("reader".to_owned());

        assert!(matches!(
            config.validate(),
            Err(ConfigError::IncompleteQueryCredentials)
        ));
    }
}
