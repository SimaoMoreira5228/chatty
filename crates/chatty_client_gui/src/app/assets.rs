use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashSet;
use iced::widget::image::Handle as ImageHandle;
use iced::widget::svg;
use tokio::sync::mpsc;

use crate::app::features::tabs::TabTarget;
use crate::app::images::AnimatedImage;
use crate::app::state;
use crate::app::view_models::{AssetBundleUi, AssetRefUi, AssetScaleUi};

fn merge_asset_refs(existing: &mut Vec<AssetRefUi>, incoming: Vec<AssetRefUi>) {
	if incoming.is_empty() {
		return;
	}

	let mut seen: HashSet<String> = existing
		.iter()
		.map(|asset| {
			if asset.id.trim().is_empty() {
				asset.name.clone()
			} else {
				asset.id.clone()
			}
		})
		.collect();

	for asset in incoming {
		let key = if asset.id.trim().is_empty() {
			asset.name.clone()
		} else {
			asset.id.clone()
		};
		if seen.insert(key) {
			existing.push(asset);
		}
	}
}

pub struct AssetManager {
	pub image_cache: moka::sync::Cache<String, ImageHandle>,
	pub animated_cache: moka::sync::Cache<String, AnimatedImage>,
	pub image_loading: Arc<DashSet<String>>,
	pub image_failed: Arc<DashSet<String>>,
	pub svg_cache: moka::sync::Cache<String, svg::Handle>,
	pub image_fetch_sender: mpsc::Sender<String>,
}

#[derive(Debug, Default, Clone)]
pub struct AssetCatalog {
	bundles: HashMap<String, AssetBundleUi>,
	global_keys: Vec<String>,
	room_keys: HashMap<chatty_domain::RoomKey, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct RoomProviderAssetCounts {
	pub room: chatty_domain::RoomKey,
	pub emotes: usize,
	pub badges: usize,
}

impl AssetManager {
	pub fn new(image_fetch_sender: mpsc::Sender<String>) -> Self {
		Self {
			image_cache: moka::sync::Cache::new(512),
			animated_cache: moka::sync::Cache::new(256),
			image_loading: Arc::new(DashSet::new()),
			image_failed: Arc::new(DashSet::new()),
			svg_cache: moka::sync::Cache::new(256),
			image_fetch_sender,
		}
	}

	pub fn get_emotes_for_target(
		&self,
		state: &state::AppState,
		target: &crate::app::features::tabs::TabTarget,
	) -> Arc<HashMap<String, AssetRefUi>> {
		state.asset_catalog.emotes_for_target(&TabTarget(target.0.clone()))
	}

	pub fn get_badges_for_target(
		&self,
		state: &state::AppState,
		target: &crate::app::features::tabs::TabTarget,
	) -> Arc<HashMap<String, AssetRefUi>> {
		state.asset_catalog.badges_for_target(&TabTarget(target.0.clone()))
	}

	pub fn prefetch_bundle(&self, bundle: &AssetBundleUi, max_emotes: usize) {
		let img_cache = self.image_cache.clone();
		let sender = self.image_fetch_sender.clone();

		let mut queued = 0usize;
		for em in bundle.emotes.iter().take(max_emotes) {
			if let Some(img) = em.pick_image(AssetScaleUi::Two) {
				let url = img.url.clone();
				if img_cache.contains_key(&url) {
					continue;
				}

				if sender.try_send(url).is_err() {
					break;
				}
				queued += 1;
			}
		}

		if queued > 0 {
			tracing::debug!(cache_key = %bundle.cache_key, queued, "prefetched emote images");
		}

		for bd in &bundle.badges {
			if let Some(img) = bd.pick_image(AssetScaleUi::Two) {
				let url = img.url.clone();
				if img_cache.contains_key(&url) {
					continue;
				}

				if sender.try_send(url).is_err() {
					break;
				}
			}
		}
	}
}

impl AssetCatalog {
	pub fn new() -> Self {
		Self::default()
	}

	fn lookup_room_keys(&self, room: &chatty_domain::RoomKey) -> Option<&Vec<String>> {
		if let Some(keys) = self.room_keys.get(room) {
			return Some(keys);
		}

		let lower = room.room_id.as_str().to_ascii_lowercase();
		if lower == room.room_id.as_str() {
			return None;
		}

		let Ok(room_id) = chatty_domain::RoomId::new(lower) else {
			return None;
		};
		let normalized = chatty_domain::RoomKey::new(room.platform, room_id);
		self.room_keys.get(&normalized)
	}

	pub fn register_bundle(&mut self, bundle: AssetBundleUi, scope: i32, room: Option<chatty_domain::RoomKey>) -> bool {
		let ck = bundle.cache_key.clone();
		let is_new = !self.bundles.contains_key(&ck);
		if let Some(existing) = self.bundles.get_mut(&ck) {
			merge_asset_refs(&mut existing.emotes, bundle.emotes);
			merge_asset_refs(&mut existing.badges, bundle.badges);
			if bundle.etag.is_some() {
				existing.etag = bundle.etag;
			}
		} else {
			self.bundles.insert(ck.clone(), bundle);
		}

		if scope == chatty_protocol::pb::AssetScope::Global as i32 {
			if !self.global_keys.contains(&ck) {
				self.global_keys.push(ck);
			}
		} else if let Some(room) = room {
			let keys = self.room_keys.entry(room).or_default();
			if !keys.contains(&ck) {
				keys.push(ck);
			}
		}

		is_new
	}

	#[allow(dead_code)]
	pub fn bundle(&self, cache_key: &str) -> Option<&AssetBundleUi> {
		self.bundles.get(cache_key)
	}

	pub fn emotes_for_target(&self, target: &TabTarget) -> Arc<HashMap<String, AssetRefUi>> {
		let mut map = HashMap::new();

		for ck in &self.global_keys {
			if let Some(bundle) = self.bundles.get(ck) {
				for emote in &bundle.emotes {
					map.insert(emote.name.clone(), emote.clone());
				}
			}
		}

		for room in &target.0 {
			if let Some(keys) = self.lookup_room_keys(room) {
				for ck in keys {
					if let Some(bundle) = self.bundles.get(ck) {
						for emote in &bundle.emotes {
							map.insert(emote.name.clone(), emote.clone());
						}
					}
				}
			}
		}

		Arc::new(map)
	}

	pub fn badges_for_target(&self, target: &TabTarget) -> Arc<HashMap<String, AssetRefUi>> {
		let mut map = HashMap::new();

		for ck in &self.global_keys {
			if let Some(bundle) = self.bundles.get(ck) {
				for badge in &bundle.badges {
					map.insert(badge.id.clone(), badge.clone());
				}
			}
		}

		for room in &target.0 {
			if let Some(keys) = self.lookup_room_keys(room) {
				for ck in keys {
					if let Some(bundle) = self.bundles.get(ck) {
						for badge in &bundle.badges {
							map.insert(badge.id.clone(), badge.clone());
						}
					}
				}
			}
		}

		Arc::new(map)
	}

	pub fn room_provider_asset_counts(&self, provider: i32) -> Vec<RoomProviderAssetCounts> {
		let mut rows = Vec::new();
		for (room, keys) in &self.room_keys {
			let mut emotes = 0usize;
			let mut badges = 0usize;
			for ck in keys {
				if let Some(bundle) = self.bundles.get(ck)
					&& bundle.provider == provider
				{
					emotes += bundle.emotes.len();
					badges += bundle.badges.len();
				}
			}
			rows.push(RoomProviderAssetCounts {
				room: room.clone(),
				emotes,
				badges,
			});
		}

		rows.sort_by(|a, b| {
			a.room
				.platform
				.as_str()
				.cmp(b.room.platform.as_str())
				.then_with(|| a.room.room_id.as_str().cmp(b.room.room_id.as_str()))
		});

		rows
	}
}

#[cfg(test)]
mod tests {
	use chatty_domain::{Platform, RoomId, RoomKey};
	use chatty_protocol::pb::AssetScope;

	use super::*;

	fn make_ref(id: &str, name: &str) -> AssetRefUi {
		AssetRefUi {
			id: id.to_string(),
			name: name.to_string(),
			images: Vec::new(),
		}
	}

	fn make_bundle(cache_key: &str, scope: i32, emotes: Vec<AssetRefUi>, badges: Vec<AssetRefUi>) -> AssetBundleUi {
		AssetBundleUi {
			cache_key: cache_key.to_string(),
			etag: None,
			provider: 1,
			scope,
			emotes,
			badges,
		}
	}

	fn room_key(id: &str) -> RoomKey {
		RoomKey::new(Platform::Twitch, RoomId::new(id).expect("room id"))
	}

	#[test]
	fn register_bundle_returns_true_once() {
		let mut catalog = AssetCatalog::new();
		let bundle = make_bundle("g1", AssetScope::Global as i32, Vec::new(), Vec::new());

		assert!(catalog.register_bundle(bundle.clone(), bundle.scope, None));
		assert!(!catalog.register_bundle(bundle, AssetScope::Global as i32, None));
	}

	#[test]
	fn emotes_for_target_includes_global_and_room() {
		let mut catalog = AssetCatalog::new();
		let global = make_bundle(
			"g1",
			AssetScope::Global as i32,
			vec![make_ref("e1", "GlobalEmote")],
			Vec::new(),
		);
		let room = room_key("room1");
		let room_bundle = make_bundle(
			"r1",
			AssetScope::Channel as i32,
			vec![make_ref("e2", "RoomEmote")],
			Vec::new(),
		);

		catalog.register_bundle(global, AssetScope::Global as i32, None);
		catalog.register_bundle(room_bundle, AssetScope::Channel as i32, Some(room.clone()));

		let empty = TabTarget(vec![]);
		let empty_emotes = catalog.emotes_for_target(&empty);
		assert!(empty_emotes.contains_key("GlobalEmote"));
		assert!(!empty_emotes.contains_key("RoomEmote"));

		let target = TabTarget(vec![room]);
		let emotes = catalog.emotes_for_target(&target);
		assert!(emotes.contains_key("GlobalEmote"));
		assert!(emotes.contains_key("RoomEmote"));
	}

	#[test]
	fn badges_for_target_includes_global_and_room() {
		let mut catalog = AssetCatalog::new();
		let global = make_bundle(
			"g1",
			AssetScope::Global as i32,
			Vec::new(),
			vec![make_ref("b1", "GlobalBadge")],
		);
		let room = room_key("room2");
		let room_bundle = make_bundle(
			"r2",
			AssetScope::Channel as i32,
			Vec::new(),
			vec![make_ref("b2", "RoomBadge")],
		);

		catalog.register_bundle(global, AssetScope::Global as i32, None);
		catalog.register_bundle(room_bundle, AssetScope::Channel as i32, Some(room.clone()));

		let target = TabTarget(vec![room]);
		let badges = catalog.badges_for_target(&target);
		assert!(badges.contains_key("b1"));
		assert!(badges.contains_key("b2"));
	}
}
