use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;

use super::common::{CachedBundle, compute_bundle_etag, prune_map_cache};
use crate::{AssetBundle, AssetImage, AssetProvider, AssetRef, AssetScale, AssetScope};

const KICK_BADGES_TTL: Duration = Duration::from_secs(600);
const KICK_EMOTES_TTL: Duration = Duration::from_secs(300);

static KICK_BADGES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static KICK_EMOTES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();

pub async fn fetch_kick_badge_bundle(room_id: &str) -> Option<AssetBundle> {
	if let Some(bundle) = get_cached_kick_badges(room_id) {
		return Some(bundle);
	}

	let subscriber_badge = AssetRef {
		id: format!("kick:subscriber:{}", room_id),
		name: "Subscriber".to_string(),
		images: vec![AssetImage {
			scale: AssetScale::One,
			url: format!("https://files.kick.com/channel_subscriber_badges/{}/original", room_id),
			format: "png".to_string(),
			width: 0,
			height: 0,
		}],
	};

	// TODO: Add more badge types if patterns are discovered (e.g., moderator, vip)
	let badges = vec![subscriber_badge];
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Kick,
		scope: AssetScope::Channel,
		cache_key: format!("kick:badges:channel:{}", room_id),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_kick_badges(room_id, bundle.clone());
	Some(bundle)
}

pub async fn fetch_kick_emote_bundle(room_id: &str, emote_ids: &[String]) -> Option<AssetBundle> {
	if let Some(bundle) = get_cached_kick_emotes(room_id) {
		return Some(bundle);
	}

	let emotes: Vec<AssetRef> = emote_ids
		.iter()
		.map(|id| AssetRef {
			id: format!("kick:emote:{}", id),
			name: id.clone(),
			images: vec![AssetImage {
				scale: AssetScale::One,
				url: format!("https://files.kick.com/emotes/{}/fullsize", id),
				format: "png".to_string(),
				width: 0,
				height: 0,
			}],
		})
		.collect();

	if emotes.is_empty() {
		return None;
	}

	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::Kick,
		scope: AssetScope::Channel,
		cache_key: format!("kick:emotes:channel:{}", room_id),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	set_cached_kick_emotes(room_id, bundle.clone());
	Some(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = KICK_BADGES_CACHE.get() {
		prune_map_cache(cache, KICK_BADGES_TTL);
	}
	if let Some(cache) = KICK_EMOTES_CACHE.get() {
		prune_map_cache(cache, KICK_EMOTES_TTL);
	}
}

fn get_cached_kick_badges(room_id: &str) -> Option<AssetBundle> {
	let cache = KICK_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(room_id) {
		if entry.fetched_at.elapsed() <= KICK_BADGES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(room_id);
			None
		}
	} else {
		None
	}
}

fn set_cached_kick_badges(room_id: &str, bundle: AssetBundle) {
	let cache = KICK_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		room_id.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

fn get_cached_kick_emotes(room_id: &str) -> Option<AssetBundle> {
	let cache = KICK_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(room_id) {
		if entry.fetched_at.elapsed() <= KICK_EMOTES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(room_id);
			None
		}
	} else {
		None
	}
}

fn set_cached_kick_emotes(room_id: &str, bundle: AssetBundle) {
	let cache = KICK_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		room_id.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}
