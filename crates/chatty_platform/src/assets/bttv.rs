#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Context;
use parking_lot::Mutex;
use serde::Deserialize;
use tracing::{info, warn};

use super::common::{CachedBundle, compute_bundle_etag, prune_map_cache, prune_optional_cache};
use crate::{AssetBundle, AssetProvider, AssetRef, AssetScope};

const BTTV_BASE_URL: &str = "https://api.betterttv.net/3";
const BTTV_EMOTES_TTL: Duration = Duration::from_secs(300);
const BTTV_BADGES_TTL: Duration = Duration::from_secs(600);
const BTTV_GLOBAL_EMOTES_TTL: Duration = Duration::from_secs(600);

static BTTV_EMOTES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static BTTV_BADGES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static BTTV_GLOBAL_EMOTES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();

pub async fn fetch_bttv_bundle(provider: &str, provider_id: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_bttv_emotes(provider, provider_id) {
		return Ok(bundle);
	}

	let url = format!("{BTTV_BASE_URL}/cached/users/{provider}/{provider_id}");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("bttv user request")?
		.error_for_status()
		.context("bttv user status")?;

	let body: BttvUserResponse = resp.json().await.context("bttv user json")?;
	let mut dedupe: HashMap<String, AssetRef> = HashMap::new();
	for emote in body.channel_emotes.iter().chain(body.shared_emotes.iter()) {
		if let Some(asset) = bttv_emote_to_asset(emote) {
			dedupe.entry(asset.id.clone()).or_insert(asset);
		}
	}

	let emotes: Vec<AssetRef> = dedupe.into_values().collect();
	let etag = compute_bundle_etag(&emotes, &[]);
	let bundle = AssetBundle {
		provider: AssetProvider::Bttv,
		scope: AssetScope::Channel,
		cache_key: format!("bttv:user:{provider}:{provider_id}"),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	if bundle.emotes.is_empty() {
		warn!(provider=%provider, provider_id=%provider_id, "bttv channel emote bundle empty");
	} else {
		info!(provider=%provider, provider_id=%provider_id, emote_count = bundle.emotes.len(), "bttv channel emote bundle fetched");
	}

	set_cached_bttv_emotes(provider, provider_id, bundle.clone());
	Ok(bundle)
}

pub async fn fetch_bttv_global_emotes_bundle() -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_bttv_global_emotes() {
		return Ok(bundle);
	}

	let url = format!("{BTTV_BASE_URL}/cached/emotes/global");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("bttv global emotes request")?
		.error_for_status()
		.context("bttv global emotes status")?;

	let body: Vec<BttvEmote> = resp.json().await.context("bttv global emotes json")?;
	let emotes: Vec<AssetRef> = body.iter().filter_map(bttv_emote_to_asset).collect();
	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::Bttv,
		scope: AssetScope::Global,
		cache_key: "bttv:emotes:global".to_string(),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	if bundle.emotes.is_empty() {
		warn!("bttv global emotes bundle empty");
	} else {
		info!(emote_count = bundle.emotes.len(), "bttv global emotes bundle fetched");
	}

	set_cached_bttv_global_emotes(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_bttv_badges_bundle(provider: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_bttv_badges(provider) {
		return Ok(bundle);
	}

	let url = format!("{BTTV_BASE_URL}/cached/badges/{provider}");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("bttv badges request")?
		.error_for_status()
		.context("bttv badges status")?;

	let body: Vec<BttvBadgeEntry> = resp.json().await.context("bttv badges json")?;
	let mut dedupe: HashMap<i32, AssetRef> = HashMap::new();
	for entry in body {
		if let Some(asset) = bttv_badge_to_asset(&entry.badge) {
			dedupe.entry(entry.badge.badge_type).or_insert(asset);
		}
	}

	let badges: Vec<AssetRef> = dedupe.into_values().collect();
	let etag = compute_bundle_etag(&[], &badges);
	let bundle = AssetBundle {
		provider: AssetProvider::Bttv,
		scope: AssetScope::Global,
		cache_key: format!("bttv:badges:{provider}"),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	if bundle.badges.is_empty() {
		warn!(provider=%provider, "bttv badges bundle empty");
	} else {
		info!(provider=%provider, badge_count = bundle.badges.len(), "bttv badges bundle fetched");
	}

	set_cached_bttv_badges(provider, bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = BTTV_EMOTES_CACHE.get() {
		prune_map_cache(cache, BTTV_EMOTES_TTL);
	}
	if let Some(cache) = BTTV_BADGES_CACHE.get() {
		prune_map_cache(cache, BTTV_BADGES_TTL);
	}
	if let Some(cache) = BTTV_GLOBAL_EMOTES_CACHE.get() {
		prune_optional_cache(cache, BTTV_GLOBAL_EMOTES_TTL);
	}
}

fn bttv_emote_to_asset(emote: &BttvEmote) -> Option<AssetRef> {
	let format = emote
		.image_type
		.clone()
		.or_else(|| if emote.animated { Some("gif".to_string()) } else { None })
		.unwrap_or_else(|| "png".to_string());

	let url = format!("https://cdn.betterttv.net/emote/{}/1x", emote.id);

	Some(AssetRef {
		id: emote.id.clone(),
		name: emote.code.clone(),
		image_url: url,
		image_format: format,
		width: 0,
		height: 0,
	})
}

fn bttv_badge_to_asset(badge: &BttvBadge) -> Option<AssetRef> {
	if badge.svg.is_empty() {
		return None;
	}

	Some(AssetRef {
		id: format!("bttv:badge:{}", badge.badge_type),
		name: badge.description.clone(),
		image_url: badge.svg.clone(),
		image_format: "svg".to_string(),
		width: 0,
		height: 0,
	})
}

fn get_cached_bttv_emotes(provider: &str, provider_id: &str) -> Option<AssetBundle> {
	let cache = BTTV_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let key = format!("{provider}:{provider_id}");
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(&key) {
		if entry.fetched_at.elapsed() <= BTTV_EMOTES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(&key);
			None
		}
	} else {
		None
	}
}

fn set_cached_bttv_emotes(provider: &str, provider_id: &str, bundle: AssetBundle) {
	let cache = BTTV_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	let key = format!("{provider}:{provider_id}");
	guard.insert(
		key,
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

fn get_cached_bttv_global_emotes() -> Option<AssetBundle> {
	let cache = BTTV_GLOBAL_EMOTES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= BTTV_GLOBAL_EMOTES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_bttv_global_emotes(bundle: AssetBundle) {
	let cache = BTTV_GLOBAL_EMOTES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_bttv_badges(provider: &str) -> Option<AssetBundle> {
	let cache = BTTV_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(provider) {
		if entry.fetched_at.elapsed() <= BTTV_BADGES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(provider);
			None
		}
	} else {
		None
	}
}

fn set_cached_bttv_badges(provider: &str, bundle: AssetBundle) {
	let cache = BTTV_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		provider.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug, Deserialize)]
struct BttvUserResponse {
	#[serde(default, rename = "channelEmotes")]
	channel_emotes: Vec<BttvEmote>,
	#[serde(default, rename = "sharedEmotes")]
	shared_emotes: Vec<BttvEmote>,
}

#[derive(Debug, Deserialize)]
struct BttvEmote {
	id: String,
	code: String,
	#[serde(default, rename = "imageType")]
	image_type: Option<String>,
	#[serde(default)]
	animated: bool,
}

#[derive(Debug, Deserialize)]
struct BttvBadgeEntry {
	#[serde(default)]
	badge: BttvBadge,
}

#[derive(Debug, Deserialize, Default)]
struct BttvBadge {
	#[serde(default)]
	description: String,
	#[serde(default)]
	svg: String,
	#[serde(default, rename = "type")]
	badge_type: i32,
}
