//! The gateway's date-times.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::PrimitiveDateTime;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

/// A date and time as the gateway reports it: a wall-clock time with no offset.
///
/// The gateway runs on US Central time. Event tracking documents its times as
/// Central; settlement dates are dates at midnight. No conversion is made, as the
/// offset is not part of the data.
///
/// Read from either format the gateway uses: ISO 8601 without an offset
/// (`2023-12-01T00:00:00`, with optional fractional seconds) in JSON responses, and
/// `06/17/2022 05:00:59` in event tracking. Written as ISO 8601.
///
/// ```
/// use achq::types::LocalDateTime;
///
/// let iso: LocalDateTime = "2023-12-01T00:00:00".parse().unwrap();
/// let us: LocalDateTime = "12/01/2023 00:00:00".parse().unwrap();
/// assert_eq!(iso, us);
/// assert_eq!(us.to_string(), "2023-12-01T00:00:00");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDateTime(PrimitiveDateTime);

/// The text is not a date-time in a format the gateway uses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid date-time {0:?}: expected YYYY-MM-DDTHH:MM:SS or MM/DD/YYYY HH:MM:SS")]
pub struct DateTimeError(String);

const ISO: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][optional [.[subsecond]]]");
const US: &[BorrowedFormatItem<'_>] =
    format_description!("[month]/[day]/[year] [hour]:[minute]:[second]");
const ISO_OUT: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
const ISO_OUT_FRACTION: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]");

impl LocalDateTime {
    /// The date-time.
    pub const fn as_primitive(&self) -> PrimitiveDateTime {
        self.0
    }
}

impl From<PrimitiveDateTime> for LocalDateTime {
    fn from(value: PrimitiveDateTime) -> Self {
        Self(value)
    }
}

impl From<LocalDateTime> for PrimitiveDateTime {
    fn from(value: LocalDateTime) -> Self {
        value.0
    }
}

impl FromStr for LocalDateTime {
    type Err = DateTimeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();
        PrimitiveDateTime::parse(trimmed, ISO)
            .or_else(|_| PrimitiveDateTime::parse(trimmed, US))
            .map(Self)
            .map_err(|_| DateTimeError(text.into()))
    }
}

/// ISO 8601 without an offset, with fractional seconds only when there are any.
impl fmt::Display for LocalDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = if self.0.nanosecond() == 0 {
            ISO_OUT
        } else {
            ISO_OUT_FRACTION
        };
        let text = self.0.format(description).map_err(|_| fmt::Error)?;
        f.write_str(&text)
    }
}

impl Serialize for LocalDateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LocalDateTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

crate::validate::no_constraints!(LocalDateTime);

/// Serializes a date as `MM/DD/YYYY`, as the onboarding API's dates are written.
#[cfg(feature = "onboarding")]
pub(crate) fn serialize_mdy_slash<S: Serializer>(
    date: &time::Date,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&crate::form::with::mdy_slash(date))
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    fn parse(text: &str) -> LocalDateTime {
        text.parse().unwrap()
    }

    #[test]
    fn reads_iso_without_an_offset() {
        assert_eq!(
            parse("2023-12-01T00:00:00").as_primitive(),
            datetime!(2023-12-01 00:00:00)
        );
        assert_eq!(
            parse("2023-11-20T13:05:09.25").as_primitive(),
            datetime!(2023-11-20 13:05:09.25)
        );
    }

    #[test]
    fn reads_event_tracking_times() {
        assert_eq!(
            parse("06/17/2022 05:00:59").as_primitive(),
            datetime!(2022-06-17 05:00:59)
        );
        assert_eq!(
            parse("06/17/2022 22:52:12").as_primitive(),
            datetime!(2022-06-17 22:52:12)
        );
    }

    #[test]
    fn rejects_other_text() {
        for bad in [
            "",
            "2023-12-01",
            "2023-12-01T00:00:00Z",
            "17/06/2022 05:00:59",
            "06/17/2022",
            "yesterday",
        ] {
            assert!(bad.parse::<LocalDateTime>().is_err(), "{bad}");
        }
        assert_eq!(
            "x".parse::<LocalDateTime>().unwrap_err().to_string(),
            "invalid date-time \"x\": expected YYYY-MM-DDTHH:MM:SS or MM/DD/YYYY HH:MM:SS"
        );
    }

    #[test]
    fn writes_iso() {
        assert_eq!(
            parse("06/17/2022 05:00:59").to_string(),
            "2022-06-17T05:00:59"
        );
        assert_eq!(
            parse("2023-11-20T13:05:09.25").to_string(),
            "2023-11-20T13:05:09.25"
        );
    }

    #[test]
    fn serde_round_trips() {
        let value: LocalDateTime = serde_json::from_str("\"2023-12-01T00:00:00\"").unwrap();
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            "\"2023-12-01T00:00:00\""
        );
        assert!(serde_json::from_str::<LocalDateTime>("\"nope\"").is_err());
    }
}
