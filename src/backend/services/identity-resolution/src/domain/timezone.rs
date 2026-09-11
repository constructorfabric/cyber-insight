//! The zone a person reads their dashboards in.

use std::fmt;

use thiserror::Error;

/// Longer than the longest name in the IANA database.
const MAX_TIMEZONE_CHARS: usize = 64;

/// An IANA zone name, spelled the way the database spells it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timezone(chrono_tz::Tz);

impl Timezone {
    /// # Errors
    ///
    /// Returns [`TimezoneError`] when the name is not an IANA zone, or is
    /// long enough that it cannot be one.
    pub fn parse(value: &str) -> Result<Self, TimezoneError> {
        if value.is_empty() || value.chars().count() > MAX_TIMEZONE_CHARS {
            return Err(TimezoneError::Unknown);
        }

        let parsed: chrono_tz::Tz = value.parse().map_err(|_| TimezoneError::Unknown)?;
        if parsed.name() != value {
            return Err(TimezoneError::Unknown);
        }

        Ok(Self(parsed))
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        self.0.name()
    }
}

impl Default for Timezone {
    fn default() -> Self {
        Self(chrono_tz::UTC)
    }
}

impl fmt::Display for Timezone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TimezoneError {
    #[error("not a known IANA timezone name")]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_iana_zone_is_kept_exactly_as_it_was_written() {
        for name in ["UTC", "Europe/Belgrade", "America/Argentina/Ushuaia"] {
            let parsed =
                Timezone::parse(name).unwrap_or_else(|error| panic!("`{name}` is a zone: {error}"));

            assert_eq!(parsed.as_str(), name);
        }
    }

    #[test]
    fn anything_that_is_not_a_zone_is_refused() {
        for name in [
            "",
            "   ",
            "utc",
            "Mars/Olympus",
            "UTC' OR 1=1",
            "../../etc/passwd",
            "+02:00",
        ] {
            assert!(Timezone::parse(name).is_err(), "should reject {name:?}");
        }
    }

    #[test]
    fn a_name_longer_than_any_zone_is_refused_before_it_is_looked_up() {
        let long = "A".repeat(MAX_TIMEZONE_CHARS + 1);

        assert!(Timezone::parse(&long).is_err());
    }

    #[test]
    fn no_preference_reads_as_utc() {
        assert_eq!(Timezone::default().as_str(), "UTC");
    }
}
