#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chatty_domain::RoomKey;
use iced::Task;
use iced::widget::{pane_grid, text_editor};
#[cfg(not(test))]
use tokio::sync::Semaphore;
use tokio::sync::{Mutex, mpsc};

use crate::app::assets::AssetManager;
use crate::app::features::tabs::TabId;
use crate::app::message::Message;
use crate::app::net::{UiEventReceiver, recv_next};
use crate::app::room::JoinRequest;
use crate::app::services::{FileLayoutStore, RealNetEffects, SharedClock, SharedLayoutStore, SharedNetEffects, SystemClock};
use crate::app::state::AppState;
use crate::app::types::PendingCommand;
use crate::net;
use crate::settings::ShortcutKey;

pub struct Chatty {
	pub(crate) net_effects: SharedNetEffects,
	pub(crate) layout_store: SharedLayoutStore,
	pub(crate) clock: SharedClock,
	pub(crate) net_rx: Arc<Mutex<UiEventReceiver>>,
	pub(crate) shutdown: Option<net::ShutdownHandle>,

	pub(crate) state: AppState,
	pub(crate) assets: AssetManager,
	pub(crate) pending_commands: Vec<PendingCommand>,
	pub(crate) pending_delete_keys: HashSet<PendingDeleteKey>,
	pub(crate) message_text_editors: HashMap<String, text_editor::Content>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PendingDeleteKey {
	pub(crate) room: RoomKey,
	pub(crate) server_message_id: Option<String>,
	pub(crate) platform_message_id: Option<String>,
}

impl Drop for Chatty {
	fn drop(&mut self) {
		if let Some(shutdown) = self.shutdown.take() {
			shutdown.shutdown();
		}
	}
}

pub(crate) fn first_char_lower(s: &str) -> char {
	s.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('\0')
}

impl Chatty {
	pub(crate) fn rebuild_pending_delete_keys(&mut self) {
		self.pending_delete_keys = self
			.pending_commands
			.iter()
			.filter_map(|pc| match pc {
				PendingCommand::Delete {
					room,
					server_message_id,
					platform_message_id,
				} => Some(PendingDeleteKey {
					room: room.clone(),
					server_message_id: server_message_id.clone(),
					platform_message_id: platform_message_id.clone(),
				}),
				_ => None,
			})
			.collect();
	}

	pub(crate) fn is_pending_delete(
		&self,
		room: &RoomKey,
		server_message_id: Option<&str>,
		platform_message_id: Option<&str>,
	) -> bool {
		self.pending_delete_keys.contains(&PendingDeleteKey {
			room: room.clone(),
			server_message_id: server_message_id.map(|v| v.to_string()),
			platform_message_id: platform_message_id.map(|v| v.to_string()),
		})
	}

	pub fn title(&self, _window: iced::window::Id) -> String {
		"Chatty".to_string()
	}

	pub fn theme(&self, _window: iced::window::Id) -> iced::Theme {
		iced::Theme::Dark
	}

	pub fn new() -> (Self, Task<Message>) {
		let (net, rx, shutdown) = net::start_networking();
		let net_rx = Arc::new(Mutex::new(rx));
		let mut state = AppState::new();
		let gs = state.gui_settings().clone();

		state.ui.server_endpoint_quic = gs.server_endpoint_quic.clone();
		state.ui.server_auth_token = gs.server_auth_token.clone();
		state.ui.max_log_items_raw = gs.max_log_items.to_string();

		let mut instance = Self {
			net_effects: Arc::new(RealNetEffects::new(net.clone())),
			layout_store: Arc::new(FileLayoutStore),
			clock: Arc::new(SystemClock),
			net_rx: net_rx.clone(),
			shutdown: Some(shutdown),

			state,

			assets: AssetManager::new({
				let (tx, _rx) = mpsc::channel::<String>(1);
				tx
			}),
			pending_commands: Vec::new(),
			pending_delete_keys: HashSet::new(),
			message_text_editors: HashMap::new(),
		};

		#[cfg(not(test))]
		{
			let image_cache_arc = instance.assets.image_cache.clone();
			let animated_cache_arc = instance.assets.animated_cache.clone();
			let image_loading_arc = Arc::clone(&instance.assets.image_loading);
			let image_failed_arc = Arc::clone(&instance.assets.image_failed);
			let svg_cache_arc = instance.assets.svg_cache.clone();
			let (img_tx, mut img_rx) = mpsc::channel::<String>(1000);
			let sem: std::sync::Arc<tokio::sync::Semaphore> = Arc::new(Semaphore::new(6));

			instance.assets.image_fetch_sender = img_tx.clone();

			tokio::spawn(async move {
				let client = reqwest::Client::new();
				while let Some(url) = img_rx.recv().await {
					let img_cache = image_cache_arc.clone();
					let animated_cache = animated_cache_arc.clone();
					let image_loading = Arc::clone(&image_loading_arc);
					let image_failed = Arc::clone(&image_failed_arc);
					let svg_cache = svg_cache_arc.clone();
					let sem = Arc::clone(&sem);
					let client = client.clone();
					tokio::spawn(async move {
						let Ok(_permit) = sem.acquire().await else {
							return;
						};

						if img_cache.contains_key(&url) {
							return;
						}

						if animated_cache.contains_key(&url) {
							return;
						}

						if svg_cache.contains_key(&url) {
							return;
						}

						image_loading.insert(url.clone());

						let mut succeeded = false;
						let mut attempt = 0u8;
						while attempt < 3 && !succeeded {
							attempt += 1;
							tracing::debug!(url = %url, attempt, "fetching image");
							match tokio::time::timeout(std::time::Duration::from_secs(8), client.get(&url).send()).await {
								Ok(Ok(resp)) => {
									match tokio::time::timeout(std::time::Duration::from_secs(8), resp.bytes()).await {
										Ok(Ok(bytes)) => {
											let bytes = bytes.to_vec();
											let bytes_to_decode = bytes.clone();
											let animated = tokio::task::spawn_blocking(move || {
												crate::app::images::decode_animated_image(&bytes_to_decode)
											})
											.await
											.ok()
											.flatten();

											if let Some(animated) = animated {
												animated_cache.insert(url.clone(), animated);
												tracing::debug!(url = %url, attempt, "animated image decoded");
												succeeded = true;
											} else if url.ends_with(".svg") || bytes.windows(4).any(|w| w == b"<svg") {
												let handle = iced::widget::svg::Handle::from_memory(bytes);
												svg_cache.insert(url.clone(), handle);
												tracing::debug!(url = %url, attempt, "svg image fetch succeeded");
												succeeded = true;
											} else {
												let handle = iced::widget::image::Handle::from_bytes(bytes);
												img_cache.insert(url.clone(), handle);
												tracing::debug!(url = %url, attempt, "image fetch succeeded");
												succeeded = true;
											}
										}
										_ => {
											if attempt < 3 {
												tokio::time::sleep(std::time::Duration::from_millis(300)).await;
											}
										}
									}
								}
								_ => {
									tracing::warn!(url = %url, attempt, "image fetch request failed");
									if attempt < 3 {
										tokio::time::sleep(std::time::Duration::from_millis(300)).await;
									}
								}
							}
						}

						image_loading.remove(&url);

						if !succeeded {
							image_failed.insert(url.clone());
							tracing::error!(url = %url, "image fetch final failure");
						} else {
							image_failed.remove(&url);
							tracing::debug!(url = %url, "image marked healthy");
						}
					});
				}
			});
		}

		if let Some(layout) = instance.layout_store.load() {
			instance.apply_ui_root(layout);
			instance.save_ui_layout();
		}

		let initial_task = if gs.auto_connect_on_startup {
			match crate::settings::build_client_config(&gs) {
				Ok(cfg) => {
					instance.state.ui.pending_auto_connect_cfg = Some(cfg.clone());
					instance
						.state
						.set_connection_status(crate::app::state::ConnectionStatus::Connecting);
					let net_clone = instance.net_effects.clone();
					let rx_clone = net_rx.clone();
					Task::perform(
						async move {
							let _ = net_clone.connect(cfg).await;
							recv_next(rx_clone).await
						},
						|ev| Message::Net(crate::app::message::NetMessage::NetPolled(ev)),
					)
				}
				Err(e) => {
					let _ = instance.report_error(e);
					Task::perform(recv_next(net_rx.clone()), |ev| {
						Message::Net(crate::app::message::NetMessage::NetPolled(ev))
					})
				}
			}
		} else {
			Task::perform(recv_next(net_rx), |ev| {
				Message::Net(crate::app::message::NetMessage::NetPolled(ev))
			})
		};

		let restore_tasks = Task::batch(instance.state.pending_restore_windows.drain(..).map(|win_model| {
			if let Some(tid) = win_model.tabs.first() {
				instance.state.pending_popped_tabs.push_back(*tid);
			}

			let (wid, task) = iced::window::open(iced::window::Settings {
				exit_on_close_request: false,
				size: iced::Size::new(win_model.width as f32, win_model.height as f32),
				position: if win_model.x >= 0 && win_model.y >= 0 {
					iced::window::Position::Specific(iced::Point::new(win_model.x as f32, win_model.y as f32))
				} else {
					iced::window::Position::Default
				},
				..Default::default()
			});

			instance.state.popped_windows.insert(wid, win_model);
			task.map(|id| Message::Window(crate::app::message::WindowMessage::Opened(id)))
		}));

		let geo = &instance.state.main_window_geometry;
		let (main_wid, main_window_task) = iced::window::open(iced::window::Settings {
			exit_on_close_request: false,
			size: iced::Size::new(geo.width as f32, geo.height as f32),
			position: if geo.x >= 0 && geo.y >= 0 {
				iced::window::Position::Specific(iced::Point::new(geo.x as f32, geo.y as f32))
			} else {
				iced::window::Position::Default
			},
			..Default::default()
		});
		instance.state.ui.main_window_id = Some(main_wid);
		let main_window_task = main_window_task.map(|id| Message::Window(crate::app::message::WindowMessage::Opened(id)));

		(instance, Task::batch(vec![initial_task, restore_tasks, main_window_task]))
	}

	pub fn collect_orphaned_tab(&mut self) -> Option<chatty_domain::RoomKey> {
		let mut to_remove: Vec<crate::app::features::tabs::TabId> = Vec::new();
		for (tid, tab) in &self.state.tabs {
			let referenced = tab.panes.iter().any(|(_, p)| p.tab_id == Some(*tid));
			if !referenced && !tab.pinned {
				to_remove.push(*tid);
			}
		}

		if let Some(tid) = to_remove.into_iter().next() {
			let tab = self.state.tabs.remove(&tid)?;
			tab.target.0.into_iter().next()
		} else {
			None
		}
	}

	pub fn navigate_pane(&mut self, dx: i32, dy: i32) {
		if let Some(tab) = self.selected_tab_mut()
			&& let Some(focused) = tab.focused_pane
		{
			let dir = if dx > 0 {
				pane_grid::Direction::Right
			} else if dx < 0 {
				pane_grid::Direction::Left
			} else if dy > 0 {
				pane_grid::Direction::Down
			} else {
				pane_grid::Direction::Up
			};
			if let Some(next) = tab.panes.adjacent(focused, dir) {
				tab.focused_pane = Some(next);
				self.save_ui_layout();
			}
		}
	}

	pub(crate) fn set_focused_by_cursor(&mut self, x: f32, y: f32) {
		let sz = self.state.ui.window_size;
		if let (Some(tab), Some(sz)) = (self.selected_tab_mut(), sz) {
			for (pane, region) in tab.panes.layout().pane_regions(8.0, 50.0, iced::Size::new(sz.0, sz.1)) {
				if region.contains(iced::Point::new(x, y)) {
					tab.focused_pane = Some(pane);
					break;
				}
			}
		}
	}

	pub fn selected_tab_id(&self) -> Option<TabId> {
		self.state.selected_tab_id
	}

	pub fn selected_tab(&self) -> Option<&crate::app::features::tabs::TabModel> {
		self.selected_tab_id().and_then(|id| self.state.tabs.get(&id))
	}

	pub fn selected_tab_mut(&mut self) -> Option<&mut crate::app::features::tabs::TabModel> {
		self.selected_tab_id().and_then(|id| self.state.tabs.get_mut(&id))
	}

	pub fn capture_ui_root(&self) -> crate::app::features::layout::UiRootState {
		crate::app::features::layout::UiRootState::from_app(self)
	}

	pub fn apply_ui_root(&mut self, root: crate::app::features::layout::UiRootState) {
		root.apply_to(self);
	}

	pub fn save_ui_layout(&self) {
		let root = self.capture_ui_root();
		self.layout_store.save(&root);
	}

	pub fn view(&self, window: iced::window::Id) -> iced::Element<'_, Message> {
		crate::ui::view(self, window)
	}

	pub fn pane_drag_enabled(&self) -> bool {
		let gs = self.state.gui_settings();
		match gs.keybinds.drag_modifier {
			ShortcutKey::Always => true,
			ShortcutKey::Alt => self.state.ui.modifiers.alt(),
			ShortcutKey::Control => self.state.ui.modifiers.control(),
			ShortcutKey::Shift => self.state.ui.modifiers.shift(),
			ShortcutKey::Logo => self.state.ui.modifiers.logo(),
			ShortcutKey::None => false,
		}
	}

	pub(crate) fn focused_tab_id(&self) -> Option<TabId> {
		self.selected_tab()
			.and_then(|t| t.focused_pane.and_then(|fp| t.panes.get(fp)).and_then(|p| p.tab_id))
	}

	pub(crate) fn pane_room(&self, pane: pane_grid::Pane) -> Option<RoomKey> {
		self.pane_rooms(pane).into_iter().next()
	}

	pub(crate) fn pane_rooms(&self, pane: pane_grid::Pane) -> Vec<RoomKey> {
		for tab in self.state.tabs.values() {
			if let Some(ps) = tab.panes.get(pane)
				&& let Some(tid) = ps.tab_id
				&& let Some(t) = self.state.tabs.get(&tid)
			{
				return t.target.0.clone();
			}
		}
		Vec::new()
	}

	#[allow(dead_code)]
	pub(crate) fn ensure_tab_for_rooms(&mut self, rooms: Vec<RoomKey>) -> TabId {
		if rooms.is_empty() {
			return TabId(0);
		}

		let mut sorted_rooms = rooms.clone();
		sorted_rooms.sort_by(|a, b| {
			a.platform
				.as_str()
				.cmp(b.platform.as_str())
				.then_with(|| a.room_id.as_str().cmp(b.room_id.as_str()))
		});

		if let Some((tid, _)) = self.state.tabs.iter().find(|(_, t)| {
			let mut t_rooms = t.target.0.clone();
			t_rooms.sort_by(|a, b| {
				a.platform
					.as_str()
					.cmp(b.platform.as_str())
					.then_with(|| a.room_id.as_str().cmp(b.room_id.as_str()))
			});
			t_rooms == sorted_rooms
		}) {
			return *tid;
		}

		let title = rooms.iter().map(|r| r.room_id.as_str()).collect::<Vec<_>>().join(", ");
		self.state.create_tab_for_rooms(title, rooms)
	}

	#[allow(dead_code)]
	pub(crate) fn apply_join_request(&mut self, pane: pane_grid::Pane) -> Option<(RoomKey, JoinRequest)> {
		let raw = self
			.selected_tab()
			.and_then(|t| t.panes.get(pane))
			.map(|p| p.join_raw.clone())
			.unwrap_or_default();
		let req = JoinRequest { raw };
		let room = self.state.parse_join_room(&req)?;
		Some((room, req))
	}
}

#[cfg(test)]
mod tests {
	use tempfile::tempdir;

	use super::*;

	fn roundtrip_for_layout<F>(mut make_layout: F)
	where
		F: FnMut(&mut Chatty),
	{
		let td = tempdir().expect("tempdir");

		let (mut inst, _task) = Chatty::new();
		inst.state.ui.window_size = Some((800.0, 600.0));

		let room = chatty_domain::RoomKey::new(
			chatty_domain::Platform::Twitch,
			chatty_domain::RoomId::new("room1").expect("room id"),
		);
		let tid = inst.ensure_tab_for_rooms(vec![room]);
		if let Some(tab) = inst.state.tabs.get_mut(&tid) {
			tab.title = "room1 title".to_string();
			tab.pinned = true;
		}

		for _ in 0..4 {
			make_layout(&mut inst);
		}

		if let Some(tab) = inst.selected_tab_mut() {
			let ids: Vec<pane_grid::Pane> = tab.panes.iter().map(|(id, _)| *id).collect();
			for id in ids {
				if let Some(p) = tab.panes.get_mut(id) {
					p.composer.clear();
					p.join_raw.clear();
					p.reply_to_server_message_id.clear();
					p.reply_to_platform_message_id.clear();
				}
			}
		}

		let root = inst.capture_ui_root();

		let p = td.path().join(".chatty").join("ui_layout.json");
		if let Some(parent) = p.parent() {
			let _ = std::fs::create_dir_all(parent);
		}
		let json_s = serde_json::to_string_pretty(&root).expect("serialize");
		std::fs::write(&p, json_s).expect("write");

		let loaded_s = std::fs::read_to_string(&p).expect("read");
		let loaded = serde_json::from_str::<crate::app::features::layout::UiRootState>(&loaded_s).expect("loaded");
		let a = serde_json::to_value(&root).unwrap();
		let b = serde_json::to_value(&loaded).unwrap();
		assert_eq!(a, b, "roundtrip must preserve layout");
	}

	#[test]
	fn layout_roundtrip_spiral() {
		roundtrip_for_layout(|i| i.split_spiral());
	}

	#[test]
	fn layout_roundtrip_masonry() {
		roundtrip_for_layout(|i| i.split_masonry());
	}
}
