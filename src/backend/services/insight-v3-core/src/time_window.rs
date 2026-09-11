use chrono::{DateTime, Datelike as _, Days, Months, NaiveDate, TimeZone as _, Utc};
use chrono_tz::Tz;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Grain {
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Bounds {
    Unbounded,
    Empty,
    Finite {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
}

/// The window a run reads over.
///
/// A caller who named no range gets [`Window::Unwindowed`], which is not the
/// same as an unbounded one they did name: `inf` buckets its rows and leaves
/// out the undated, and a maximum range refuses it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Window {
    Unwindowed,
    Requested {
        bounds: Bounds,
        grain: Option<Grain>,
        timezone: RequestedTimeZone,
    },
}

impl Window {
    pub(crate) fn legacy() -> Self {
        Self::Unwindowed
    }

    pub(crate) fn unbucketed(self) -> Self {
        match self {
            Self::Unwindowed => Self::Unwindowed,
            Self::Requested {
                bounds, timezone, ..
            } => Self::Requested {
                bounds,
                grain: None,
                timezone,
            },
        }
    }

    pub(crate) fn grain(&self) -> Option<Grain> {
        match self {
            Self::Unwindowed => None,
            Self::Requested { grain, .. } => *grain,
        }
    }

    pub(crate) fn timezone(&self) -> &str {
        match self {
            Self::Unwindowed => RequestedTimeZone::UTC_NAME,
            Self::Requested { timezone, .. } => timezone.as_str(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RequestedTimeZone(Tz);

impl RequestedTimeZone {
    pub(crate) const UTC_NAME: &'static str = "UTC";

    pub(crate) fn parse(value: &str) -> Result<Self, WindowError> {
        value
            .parse::<Tz>()
            .map(Self)
            .map_err(|_| WindowError::Timezone(value.to_owned()))
    }

    fn as_str(&self) -> &str {
        self.0.name()
    }
}

impl Default for RequestedTimeZone {
    fn default() -> Self {
        Self(chrono_tz::UTC)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestedRange {
    PreviousDay,
    RollingDays { days: u64, grain: Grain },
    PreviousMonth,
    PreviousQuarter,
    AllTime,
    Interval { from: NaiveDate, to: NaiveDate },
}

impl RequestedRange {
    pub(crate) fn parse(value: &str) -> Result<Self, WindowError> {
        match value {
            "PDC" => Ok(Self::PreviousDay),
            "P7D" => Ok(Self::RollingDays {
                days: 7,
                grain: Grain::Day,
            }),
            "P30D" => Ok(Self::RollingDays {
                days: 30,
                grain: Grain::Day,
            }),
            "PMC" => Ok(Self::PreviousMonth),
            "PQC" => Ok(Self::PreviousQuarter),
            "P1Y" => Ok(Self::RollingDays {
                days: 365,
                grain: Grain::Month,
            }),
            "inf" => Ok(Self::AllTime),
            _ => Self::parse_interval(value),
        }
    }

    fn parse_interval(value: &str) -> Result<Self, WindowError> {
        let Some((from, to)) = value.split_once('/') else {
            return Err(WindowError::Range(value.to_owned()));
        };
        if to.contains('/') {
            return Err(WindowError::Range(value.to_owned()));
        }

        let from = canonical_date(from).ok_or_else(|| WindowError::Range(value.to_owned()))?;
        let to = canonical_date(to).ok_or_else(|| WindowError::Range(value.to_owned()))?;
        if from >= to {
            return Err(WindowError::Range(value.to_owned()));
        }

        Ok(Self::Interval { from, to })
    }

    pub(crate) fn resolve(
        &self,
        anchor: Option<DateTime<Utc>>,
        timezone: &RequestedTimeZone,
    ) -> Result<Window, WindowError> {
        Ok(Window::Requested {
            bounds: self.bounds(anchor, timezone)?,
            grain: Some(self.grain()),
            timezone: timezone.clone(),
        })
    }

    fn bounds(
        &self,
        anchor: Option<DateTime<Utc>>,
        timezone: &RequestedTimeZone,
    ) -> Result<Bounds, WindowError> {
        if *self == Self::AllTime {
            return Ok(Bounds::Unbounded);
        }

        if let Self::Interval { from, to } = self {
            return Ok(Bounds::Finite {
                from: local_midnight(*from, timezone.0)?,
                to: local_midnight(*to, timezone.0)?,
            });
        }

        // A relative window counts back from the newest row, so a source
        // with no dated row selects nothing.
        let Some(anchor) = anchor else {
            return Ok(Bounds::Empty);
        };
        let local_anchor = anchor.with_timezone(&timezone.0);
        let (from, to) = match *self {
            Self::PreviousDay => {
                let to = local_anchor.date_naive();
                let from = to
                    .checked_sub_days(Days::new(1))
                    .ok_or(WindowError::Overflow)?;
                (
                    local_midnight(from, timezone.0)?,
                    local_midnight(to, timezone.0)?,
                )
            }
            Self::RollingDays { days, .. } => {
                let from_naive = local_anchor
                    .naive_local()
                    .checked_sub_days(Days::new(days))
                    .ok_or(WindowError::Overflow)?;
                let from = local_datetime(from_naive, timezone.0)?;
                (from, anchor)
            }
            Self::PreviousMonth => {
                let this_month =
                    NaiveDate::from_ymd_opt(local_anchor.year(), local_anchor.month(), 1)
                        .ok_or(WindowError::Overflow)?;
                let previous = this_month
                    .checked_sub_months(Months::new(1))
                    .ok_or(WindowError::Overflow)?;
                (
                    local_midnight(previous, timezone.0)?,
                    local_midnight(this_month, timezone.0)?,
                )
            }
            Self::PreviousQuarter => {
                let quarter_month = ((local_anchor.month() - 1) / 3) * 3 + 1;
                let this_quarter = NaiveDate::from_ymd_opt(local_anchor.year(), quarter_month, 1)
                    .ok_or(WindowError::Overflow)?;
                let previous = this_quarter
                    .checked_sub_months(Months::new(3))
                    .ok_or(WindowError::Overflow)?;
                (
                    local_midnight(previous, timezone.0)?,
                    local_midnight(this_quarter, timezone.0)?,
                )
            }
            // Both answered above, before an anchor was needed.
            Self::AllTime | Self::Interval { .. } => return Ok(Bounds::Unbounded),
        };

        Ok(Bounds::Finite { from, to })
    }

    fn grain(self) -> Grain {
        match self {
            Self::PreviousDay => Grain::Hour,
            Self::RollingDays { grain, .. } => grain,
            Self::PreviousMonth => Grain::Day,
            Self::PreviousQuarter => Grain::Week,
            Self::AllTime => Grain::Month,
            Self::Interval { from, to } => grain_for_days((to - from).num_days()),
        }
    }
}

fn canonical_date(value: &str) -> Option<NaiveDate> {
    if value.len() != 10 {
        return None;
    }
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    (parsed.format("%Y-%m-%d").to_string() == value).then_some(parsed)
}

fn grain_for_days(days: i64) -> Grain {
    match days {
        1 => Grain::Hour,
        2..=31 => Grain::Day,
        32..=92 => Grain::Week,
        _ => Grain::Month,
    }
}

fn local_midnight(date: NaiveDate, timezone: Tz) -> Result<DateTime<Utc>, WindowError> {
    local_datetime(
        date.and_hms_opt(0, 0, 0).ok_or(WindowError::Overflow)?,
        timezone,
    )
}

fn local_datetime(
    value: chrono::NaiveDateTime,
    timezone: Tz,
) -> Result<DateTime<Utc>, WindowError> {
    timezone
        .from_local_datetime(&value)
        .earliest()
        .map(|value| value.with_timezone(&Utc))
        .ok_or(WindowError::LocalTime)
}

/// What a caller asked a run for, before any data is read. Naming no
/// range asks for the legacy window: unbounded, unbucketed and UTC.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowRequest {
    range: Option<RequestedRange>,
    timezone: RequestedTimeZone,
    bucketed: bool,
}

impl WindowRequest {
    pub(crate) fn parse(
        range: Option<&str>,
        timezone: Option<&str>,
        bucketed: Option<bool>,
    ) -> Result<Self, WindowError> {
        Ok(Self {
            range: range.map(RequestedRange::parse).transpose()?,
            timezone: timezone
                .map(RequestedTimeZone::parse)
                .transpose()?
                .unwrap_or_default(),
            bucketed: bucketed.unwrap_or(true),
        })
    }

    pub(crate) fn is_ranged(&self) -> bool {
        self.range.is_some()
    }

    pub(crate) fn resolve(&self, anchor: Option<DateTime<Utc>>) -> Result<Window, WindowError> {
        let Some(range) = self.range else {
            return Ok(Window::legacy());
        };

        let window = range.resolve(anchor, &self.timezone)?;

        Ok(if self.bucketed {
            window
        } else {
            window.unbucketed()
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaximumRange {
    Days(u32),
    Months(u32),
    Years(u32),
}

impl MaximumRange {
    pub(crate) fn parse(value: &str) -> Result<Self, WindowError> {
        let Some(body) = value.strip_prefix('P') else {
            return Err(WindowError::Maximum(value.to_owned()));
        };
        let Some((digits, unit)) = body.split_at_checked(body.len().saturating_sub(1)) else {
            return Err(WindowError::Maximum(value.to_owned()));
        };
        let amount = digits
            .parse::<u32>()
            .ok()
            .filter(|amount| *amount > 0)
            .ok_or_else(|| WindowError::Maximum(value.to_owned()))?;

        match unit {
            "D" => Ok(Self::Days(amount)),
            "M" => Ok(Self::Months(amount)),
            "Y" => Ok(Self::Years(amount)),
            _ => Err(WindowError::Maximum(value.to_owned())),
        }
    }

    /// Whether this cap admits the window. A run that named no range is
    /// not capped.
    pub(crate) fn allows(self, window: &Window) -> bool {
        let Window::Requested {
            bounds, timezone, ..
        } = window
        else {
            return true;
        };
        let (from, to) = match *bounds {
            Bounds::Unbounded => return false,
            Bounds::Empty => return true,
            Bounds::Finite { from, to } => (from, to),
        };

        let from = from.with_timezone(&timezone.0).date_naive();
        let to = to.with_timezone(&timezone.0).date_naive();
        let earliest = match self {
            Self::Days(days) => to.checked_sub_days(Days::new(u64::from(days))),
            Self::Months(months) => to.checked_sub_months(Months::new(months)),
            Self::Years(years) => years
                .checked_mul(12)
                .and_then(|months| to.checked_sub_months(Months::new(months))),
        };

        earliest.is_some_and(|earliest| from >= earliest)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(crate) enum WindowError {
    #[error("unsupported or malformed time range `{0}`")]
    Range(String),
    #[error("invalid timezone `{0}`")]
    Timezone(String),
    #[error("maximum range `{0}` must be a positive day, month or year duration")]
    Maximum(String),
    #[error("time range arithmetic overflowed")]
    Overflow,
    #[error("a requested local time does not exist")]
    LocalTime,
}

#[cfg(test)]
#[path = "time_window/tests.rs"]
mod tests;
