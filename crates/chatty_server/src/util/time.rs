#![forbid(unsafe_code)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Current Unix time in milliseconds.
#[inline]
pub fn unix_ms_now() -> i64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or(Duration::from_secs(0))
		.as_millis() as i64
}
