use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Deserialize;

use super::common::{CachedBundle, compute_bundle_etag, prune_map_cache};
use crate::{AssetBundle, AssetImage, AssetProvider, AssetRef, AssetScale, AssetScope};

const KICK_BADGES_TTL: Duration = Duration::from_secs(600);
const KICK_EMOTES_TTL: Duration = Duration::from_secs(300);
const KICK_GLOBAL_EMOTES_CACHE_KEY: &str = "__global__";

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

pub async fn fetch_kick_emote_bundles(room_slug: &str) -> Vec<AssetBundle> {
	let mut bundles = Vec::new();
	let cached_channel = get_cached_kick_emotes(room_slug);
	let cached_global = get_cached_kick_emotes(KICK_GLOBAL_EMOTES_CACHE_KEY);
	if let Some(bundle) = cached_channel {
		bundles.push(bundle);
	}
	if let Some(bundle) = cached_global {
		bundles.push(bundle);
	}
	if !bundles.is_empty() {
		return bundles;
	}

	let url = format!("https://kick.com/emotes/{}", urlencoding::encode(room_slug));
	let resp = match reqwest::Client::new()
		.get(url)
		.header("Accept", "application/json")
		.header("User-Agent", "chatty-server/0.1")
		.send()
		.await
	{
		Ok(resp) => resp,
		Err(_) => return bundles,
	};
	if !resp.status().is_success() {
		return bundles;
	}

	let groups: Vec<KickEmoteGroup> = match resp.json().await {
		Ok(groups) => groups,
		Err(_) => return bundles,
	};
	let mut channel_emotes = Vec::new();
	let mut global_emotes = Vec::new();

	for group in groups {
		let slug_match = group
			.slug
			.as_deref()
			.map(|slug| slug.eq_ignore_ascii_case(room_slug))
			.unwrap_or(false);
		let is_global = group
			.name
			.as_deref()
			.map(|name| matches!(name, "Global" | "Emoji"))
			.unwrap_or(false)
			|| group.id.as_deref() == Some("Global")
			|| group.id.as_deref() == Some("Emoji");

		let target = if slug_match {
			Some(&mut channel_emotes)
		} else if is_global {
			Some(&mut global_emotes)
		} else {
			None
		};

		if let Some(target) = target {
			for emote in group.emotes {
				target.push(AssetRef {
					id: format!("kick:emote:{}", emote.id),
					name: emote.name,
					images: vec![AssetImage {
						scale: AssetScale::One,
						url: format!("https://files.kick.com/emotes/{}/fullsize", emote.id),
						format: "png".to_string(),
						width: 0,
						height: 0,
					}],
				});
			}
		}
	}

	if !global_emotes.is_empty() {
		let etag = compute_bundle_etag(&global_emotes, &[]);
		let bundle = AssetBundle {
			provider: AssetProvider::Kick,
			scope: AssetScope::Global,
			cache_key: "kick:emotes:global".to_string(),
			etag: Some(etag),
			emotes: global_emotes,
			badges: Vec::new(),
		};
		set_cached_kick_emotes(KICK_GLOBAL_EMOTES_CACHE_KEY, bundle.clone());
		bundles.push(bundle);
	}

	if !channel_emotes.is_empty() {
		let etag = compute_bundle_etag(&channel_emotes, &[]);
		let bundle = AssetBundle {
			provider: AssetProvider::Kick,
			scope: AssetScope::Channel,
			cache_key: format!("kick:emotes:channel:{}", room_slug),
			etag: Some(etag),
			emotes: channel_emotes,
			badges: Vec::new(),
		};
		set_cached_kick_emotes(room_slug, bundle.clone());
		bundles.push(bundle);
	}

	bundles
}

#[derive(Debug, Deserialize)]
struct KickEmoteGroup {
	#[serde(default)]
	name: Option<String>,
	#[serde(default)]
	id: Option<String>,
	#[serde(default)]
	slug: Option<String>,
	#[serde(default)]
	emotes: Vec<KickEmote>,
}

#[derive(Debug, Deserialize)]
struct KickEmote {
	id: u64,
	name: String,
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
