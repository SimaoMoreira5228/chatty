#![forbid(unsafe_code)]

use std::cmp::Ordering;

pub fn badge_priority(id: &str) -> u8 {
	let lower = id.to_ascii_lowercase();
	if lower.contains("broadcaster") || lower.contains("owner") {
		0
	} else if lower.contains("moderator") || lower.contains("mod") {
		1
	} else if lower.contains("vip") {
		2
	} else if lower.contains("subscriber") || lower.contains("sub") {
		3
	} else {
		10
	}
}

pub fn cmp_badge_ids(a: &str, b: &str) -> Ordering {
	let pa = badge_priority(a);
	let pb = badge_priority(b);
	pa.cmp(&pb).then_with(|| a.cmp(b))
}
