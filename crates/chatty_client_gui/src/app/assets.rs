use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashSet;
use iced::widget::image::Handle as ImageHandle;
use iced::widget::svg;
use tokio::sync::mpsc;

use crate::app::images::AnimatedImage;
use crate::app::state;
use crate::ui::components::chat_message::AssetRefUi;

pub struct AssetManager {
	pub image_cache: moka::sync::Cache<String, ImageHandle>,
	pub animated_cache: moka::sync::Cache<String, AnimatedImage>,
	pub image_loading: Arc<DashSet<String>>,
	pub image_failed: Arc<DashSet<String>>,
	pub svg_cache: moka::sync::Cache<String, svg::Handle>,
	pub emotes_cache: moka::sync::Cache<Option<state::TabTarget>, Arc<HashMap<String, AssetRefUi>>>,
	pub badges_cache: moka::sync::Cache<Option<state::TabTarget>, Arc<HashMap<String, AssetRefUi>>>,
	pub image_fetch_sender: mpsc::Sender<String>,
}

impl AssetManager {
	pub fn new(image_fetch_sender: mpsc::Sender<String>) -> Self {
		Self {
			image_cache: moka::sync::Cache::new(512),
			animated_cache: moka::sync::Cache::new(256),
			image_loading: Arc::new(DashSet::new()),
			image_failed: Arc::new(DashSet::new()),
			svg_cache: moka::sync::Cache::new(256),
			emotes_cache: moka::sync::Cache::new(256),
			badges_cache: moka::sync::Cache::new(256),
			image_fetch_sender,
		}
	}

	pub fn get_emotes_for_target(
		&self,
		state: &state::AppState,
		target: &state::TabTarget,
	) -> Arc<HashMap<String, AssetRefUi>> {
		let key = Some(target.clone());
		self.emotes_cache.get_with(key, || {
			let mut map = HashMap::new();

			for ck in &state.global_asset_cache_keys {
				if let Some(bundle) = state.asset_bundles.get(ck) {
					for emote in &bundle.emotes {
						map.insert(emote.name.clone(), emote.clone());
					}
				}
			}

			for room in &target.0 {
				if let Some(keys) = state.room_asset_cache_keys.get(room) {
					for ck in keys {
						if let Some(bundle) = state.asset_bundles.get(ck) {
							for emote in &bundle.emotes {
								map.insert(emote.name.clone(), emote.clone());
							}
						}
					}
				}
			}

			Arc::new(map)
		})
	}

	pub fn get_badges_for_target(
		&self,
		state: &state::AppState,
		target: &state::TabTarget,
	) -> Arc<HashMap<String, AssetRefUi>> {
		let key = Some(target.clone());
		self.badges_cache.get_with(key, || {
			let mut map = HashMap::new();

			for ck in &state.global_asset_cache_keys {
				if let Some(bundle) = state.asset_bundles.get(ck) {
					for badge in &bundle.badges {
						map.insert(badge.id.clone(), badge.clone());
					}
				}
			}

			for room in &target.0 {
				if let Some(keys) = state.room_asset_cache_keys.get(room) {
					for ck in keys {
						if let Some(bundle) = state.asset_bundles.get(ck) {
							for badge in &bundle.badges {
								map.insert(badge.id.clone(), badge.clone());
							}
						}
					}
				}
			}

			Arc::new(map)
		})
	}
}
