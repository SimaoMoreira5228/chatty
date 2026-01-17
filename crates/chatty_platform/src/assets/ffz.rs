#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, anyhow};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::{AssetBundle, AssetProvider, AssetRef, AssetScope};

use super::common::{CachedBundle, compute_bundle_etag, guess_format, prune_map_cache, prune_optional_cache};

const FFZ_BASE_URL: &str = "https://api.frankerfacez.com";
const FFZ_EMOTES_TTL: Duration = Duration::from_secs(300);
const FFZ_BADGES_TTL: Duration = Duration::from_secs(600);

static FFZ_EMOTES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static FFZ_BADGES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();

pub async fn fetch_ffz_bundle(room_login: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_ffz_emotes(room_login) {
		return Ok(bundle);
	}

	let url = format!("{FFZ_BASE_URL}/v1/room/{room_login}");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("ffz room request")?
		.error_for_status()
		.context("ffz room status")?;

	let body: FfzRoomResponse = resp.json().await.context("ffz room json")?;
	let set_id = body.room.set;
	let set = body
		.sets
		.get(&set_id.to_string())
		.ok_or_else(|| anyhow!("ffz set not found"))?;

	let emotes: Vec<AssetRef> = set.emoticons.iter().filter_map(ffz_emote_to_asset).collect();
	let badges = ffz_room_badges(&body.room);
	let etag = compute_bundle_etag(&emotes, &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Ffz,
		scope: AssetScope::Channel,
		cache_key: format!("ffz:set:{set_id}"),
		etag: Some(etag),
		emotes,
		badges,
	};

	set_cached_ffz_emotes(room_login, bundle.clone());
	Ok(bundle)
}

pub async fn fetch_ffz_badges_bundle() -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_ffz_badges() {
		return Ok(bundle);
	}

	let url = format!("{FFZ_BASE_URL}/v1/badges");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("ffz badges request")?
		.error_for_status()
		.context("ffz badges status")?;

	let body: FfzBadgesResponse = resp.json().await.context("ffz badges json")?;
	let badges: Vec<AssetRef> = body.badges.into_iter().filter_map(ffz_badge_to_asset).collect();
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Ffz,
		scope: AssetScope::Global,
		cache_key: "ffz:badges:global".to_string(),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_ffz_badges(bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = FFZ_EMOTES_CACHE.get() {
		prune_map_cache(cache, FFZ_EMOTES_TTL);
	}

	if let Some(cache) = FFZ_BADGES_CACHE.get() {
		prune_optional_cache(cache, FFZ_BADGES_TTL);
	}
}

fn ffz_emote_to_asset(emote: &FfzEmote) -> Option<AssetRef> {
	let (url, format) = if let Some(animated) = &emote.animated {
		animated
			.get("1")
			.map(|u| (u.clone(), "webp".to_string()))
			.or_else(|| emote.urls.get("1").map(|u| (u.clone(), guess_format(u))))
	} else {
		emote.urls.get("1").map(|u| (u.clone(), guess_format(u)))
	}?;

	Some(AssetRef {
		id: emote.id.to_string(),
		name: emote.name.clone(),
		image_url: url,
		image_format: format,
		width: emote.width.unwrap_or(0.0).round() as u32,
		height: emote.height.unwrap_or(0.0).round() as u32,
	})
}

fn ffz_room_badges(room: &FfzRoom) -> Vec<AssetRef> {
	let mut badges = Vec::new();

	if let Some(urls) = room.vip_badge.as_ref()
		&& let Some(badge) = ffz_badge_from_urls("ffz:vip", "VIP", urls)
	{
		badges.push(badge);
	}

	if let Some(urls) = room.mod_urls.as_ref()
		&& let Some(badge) = ffz_badge_from_urls("ffz:moderator", "Moderator", urls)
	{
		badges.push(badge);
	}

	badges
}

fn ffz_badge_from_urls(id: &str, name: &str, urls: &HashMap<String, String>) -> Option<AssetRef> {
	let url = urls.get("1").or_else(|| urls.values().next())?.clone();
	Some(AssetRef {
		id: id.to_string(),
		name: name.to_string(),
		image_url: url.clone(),
		image_format: guess_format(&url),
		width: 0,
		height: 0,
	})
}

fn ffz_badge_to_asset(badge: FfzBadge) -> Option<AssetRef> {
	let url = badge.urls.get("1").or_else(|| badge.urls.values().next()).cloned()?;

	Some(AssetRef {
		id: badge.id.to_string(),
		name: badge.name,
		image_url: url.clone(),
		image_format: guess_format(&url),
		width: 0,
		height: 0,
	})
}

fn get_cached_ffz_badges() -> Option<AssetBundle> {
	let cache = FFZ_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= FFZ_BADGES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_ffz_badges(bundle: AssetBundle) {
	let cache = FFZ_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_ffz_emotes(room_login: &str) -> Option<AssetBundle> {
	let cache = FFZ_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(room_login) {
		if entry.fetched_at.elapsed() <= FFZ_EMOTES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(room_login);
			None
		}
	} else {
		None
	}
}

fn set_cached_ffz_emotes(room_login: &str, bundle: AssetBundle) {
	let cache = FFZ_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		room_login.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug, Deserialize)]
struct FfzRoomResponse {
	room: FfzRoom,
	sets: HashMap<String, FfzEmoteSet>,
}

#[derive(Debug, Deserialize)]
struct FfzRoom {
	set: u64,
	#[serde(default)]
	vip_badge: Option<HashMap<String, String>>,
	#[serde(default)]
	mod_urls: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct FfzEmoteSet {
	#[serde(default)]
	emoticons: Vec<FfzEmote>,
}

#[derive(Debug, Deserialize)]
struct FfzEmote {
	id: u64,
	name: String,
	width: Option<f64>,
	height: Option<f64>,
	urls: HashMap<String, String>,
	animated: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct FfzBadgesResponse {
	badges: Vec<FfzBadge>,
}

#[derive(Debug, Deserialize)]
struct FfzBadge {
	id: u64,
	name: String,
	urls: HashMap<String, String>,
}
