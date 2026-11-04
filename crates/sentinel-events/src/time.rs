//! Time conversions shared by health accounting and normalized events.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, SecondsFormat, Utc};

pub const FILETIME_UNIX_EPOCH_DIFF_100NS: u64 = 116_444_736_000_000_000;

pub fn filetime_to_unix_ms(filetime: u64) -> Option<u64> {
    let delta = filetime.checked_sub(FILETIME_UNIX_EPOCH_DIFF_100NS)?;
    Some(delta / 10_000)
}

pub fn unix_ms_to_rfc3339(ms: u64) -> Option<String> {
    let seconds = i64::try_from(ms / 1_000).ok()?;
    let nanos = u32::try_from((ms % 1_000) * 1_000_000).ok()?;
    DateTime::<Utc>::from_timestamp(seconds, nanos)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub fn now_unix_ms() -> Option<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_conversion_matches_unix_epoch() {
        assert_eq!(filetime_to_unix_ms(FILETIME_UNIX_EPOCH_DIFF_100NS), Some(0));
        assert_eq!(
            filetime_to_unix_ms(FILETIME_UNIX_EPOCH_DIFF_100NS + 10_000_000),
            Some(1_000)
        );
        assert_eq!(filetime_to_unix_ms(0), None);
    }

    #[test]
    fn unix_ms_format_is_utc_with_milliseconds() {
        assert_eq!(
            unix_ms_to_rfc3339(0).as_deref(),
            Some("1970-01-01T00:00:00.000Z")
        );
        assert_eq!(
            unix_ms_to_rfc3339(1_000).as_deref(),
            Some("1970-01-01T00:00:01.000Z")
        );
    }

    #[test]
    fn now_is_after_the_unix_epoch() {
        assert!(now_unix_ms().is_some_and(|value| value > 0));
    }
}
