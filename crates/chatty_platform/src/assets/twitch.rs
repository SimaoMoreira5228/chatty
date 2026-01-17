#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Context;
use parking_lot::Mutex;
use serde::Deserialize;

use crate::{AssetBundle, AssetProvider, AssetRef, AssetScope};

use super::common::{CachedBundle, compute_bundle_etag, guess_format, prune_map_cache, prune_optional_cache};

const TWITCH_BADGES_TTL: Duration = Duration::from_secs(600);

static TWITCH_BADGES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();
static TWITCH_CHANNEL_BADGES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();

pub async fn fetch_twitch_badges_bundle(client_id: &str, bearer_token: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_twitch_badges() {
		return Ok(bundle);
	}

	let url = "https://api.twitch.tv/helix/chat/badges/global";
	let resp = reqwest::Client::new()
		.get(url)
		.header("Client-Id", client_id)
		.header("Authorization", format!("Bearer {bearer_token}"))
		.send()
		.await
		.context("twitch badges request")?
		.error_for_status()
		.context("twitch badges status")?;

	let body: TwitchBadgesResponse = resp.json().await.context("twitch badges json")?;
	let mut badges = Vec::new();
	for set in body.data {
		let set_id = set.set_id;
		for ver in set.versions {
			if let Some(asset) = twitch_badge_to_asset(&set_id, ver) {
				badges.push(asset);
			}
		}
	}

	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Twitch,
		scope: AssetScope::Global,
		cache_key: "twitch:badges:global".to_string(),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_twitch_badges(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_twitch_channel_badges_bundle(
	client_id: &str,
	bearer_token: &str,
	broadcaster_id: &str,
) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_twitch_channel_badges(broadcaster_id) {
		return Ok(bundle);
	}

	let url = format!("https://api.twitch.tv/helix/chat/badges?broadcaster_id={broadcaster_id}");
	let resp = reqwest::Client::new()
		.get(url)
		.header("Client-Id", client_id)
		.header("Authorization", format!("Bearer {bearer_token}"))
		.send()
		.await
		.context("twitch channel badges request")?
		.error_for_status()
		.context("twitch channel badges status")?;

	let body: TwitchBadgesResponse = resp.json().await.context("twitch channel badges json")?;
	let mut badges = Vec::new();
	for set in body.data {
		let set_id = set.set_id;
		for ver in set.versions {
			if let Some(asset) = twitch_badge_to_asset(&set_id, ver) {
				badges.push(asset);
			}
		}
	}
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Twitch,
		scope: AssetScope::Channel,
		cache_key: format!("twitch:badges:channel:{broadcaster_id}"),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_twitch_channel_badges(broadcaster_id, bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = TWITCH_BADGES_CACHE.get() {
		prune_optional_cache(cache, TWITCH_BADGES_TTL);
	}
	if let Some(cache) = TWITCH_CHANNEL_BADGES_CACHE.get() {
		prune_map_cache(cache, TWITCH_BADGES_TTL);
	}
}

fn twitch_badge_to_asset(set_id: &str, badge: TwitchBadgeVersion) -> Option<AssetRef> {
	let url = badge.image_url_1x.clone();
	Some(AssetRef {
		id: format!("twitch:{set_id}:{}", badge.id),
		name: badge.title.unwrap_or_else(|| format!("{set_id}:{}", badge.id)),
		image_url: url.clone(),
		image_format: guess_format(&url),
		width: 0,
		height: 0,
	})
}

fn get_cached_twitch_badges() -> Option<AssetBundle> {
	let cache = TWITCH_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= TWITCH_BADGES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_twitch_badges(bundle: AssetBundle) {
	let cache = TWITCH_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_twitch_channel_badges(broadcaster_id: &str) -> Option<AssetBundle> {
	let cache = TWITCH_CHANNEL_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(broadcaster_id) {
		if entry.fetched_at.elapsed() <= TWITCH_BADGES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(broadcaster_id);
			None
		}
	} else {
		None
	}
}

fn set_cached_twitch_channel_badges(broadcaster_id: &str, bundle: AssetBundle) {
	let cache = TWITCH_CHANNEL_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		broadcaster_id.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug, Deserialize)]
struct TwitchBadgesResponse {
	data: Vec<TwitchBadgeSet>,
}

#[derive(Debug, Deserialize)]
struct TwitchBadgeSet {
	#[serde(rename = "set_id")]
	set_id: String,
	versions: Vec<TwitchBadgeVersion>,
}

#[derive(Debug, Deserialize)]
struct TwitchBadgeVersion {
	id: String,
	#[serde(default)]
	title: Option<String>,
	#[serde(rename = "image_url_1x")]
	image_url_1x: String,
}
