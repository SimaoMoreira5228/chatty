use std::time::Duration;

use rand::Rng;
use tokio::time::Instant;

pub const RECONNECT_RESET_AFTER: Duration = Duration::from_secs(60 * 5);

pub fn schedule_reconnect(attempt: u32) -> (Instant, u64) {
	let base_ms = 500u64;
	let max_ms = 30_000u64;
	let pow = 2u64.saturating_pow(attempt.saturating_sub(1).min(6));
	let delay_ms = (base_ms.saturating_mul(pow)).min(max_ms);
	let jitter_window = (delay_ms / 10).max(1);
	let mut rng = rand::rng();
	let jitter_offset = rng.random_range(0..=(jitter_window * 2));
	let final_ms = delay_ms.saturating_sub(jitter_window).saturating_add(jitter_offset);
	(Instant::now() + Duration::from_millis(final_ms), final_ms)
}
