#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use chatty_client_core::ClientConfigV1;
use chatty_domain::{Platform, RoomKey};
use iced::keyboard;
use iced::widget::pane_grid;
use tracing::{debug, info};

use crate::app::types::{JoinTarget, Page, SettingsCategory};
use crate::settings;
use crate::settings::GuiSettings;
pub use crate::ui::components::chat_message::ChatMessageUi;
use crate::ui::components::chat_pane::ChatPane;
pub use crate::ui::components::room::{JoinRequest, RoomPermissions, RoomStateUi};
pub use crate::ui::components::tab::{ChatItem, ChatLog, TabId, TabModel, TabTarget};
pub use crate::ui::components::toaster::{UiNotification, UiNotificationKind};
pub use crate::ui::components::window::{WindowId, WindowModel};

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
	Disconnected {
		reason: Option<String>,
	},
	Connecting,
	Reconnecting {
		attempt: u32,
		next_retry_in_ms: u64,
	},
	Connected {
		server: String,
	},
}

#[derive(Debug, Clone)]
pub struct UiState {
	pub page: Page,
	pub spiral_dir: u8,
	pub masonry_flip: bool,
	pub modifiers: keyboard::Modifiers,
	pub window_size: Option<(f32, f32)>,
	pub last_cursor_pos: Option<(f32, f32)>,
	pub toaster: crate::ui::components::toaster::Toaster,
	pub animation_start: std::time::Instant,
	pub animation_clock: std::time::Instant,
	pub follow_end: bool,
	pub last_focus: Option<std::time::Instant>,
	pub vim: crate::ui::vim::VimState,
	pub server_endpoint_quic: String,
	pub server_auth_token: String,
	pub max_log_items_raw: String,
	pub users_view: crate::ui::users_view::UsersView,
	pub active_overlay: Option<crate::ui::modals::ActiveOverlay>,
	pub overlay_dismissed: bool,
	pub main_window_id: Option<iced::window::Id>,
	pub settings_view: crate::ui::settings::SettingsView,
	pub pending_auto_connect_cfg: Option<ClientConfigV1>,
	pub pending_join_target: Option<JoinTarget>,
}

impl Default for UiState {
	fn default() -> Self {
		Self {
			page: Page::Main,
			spiral_dir: 0,
			masonry_flip: false,
			modifiers: keyboard::Modifiers::default(),
			window_size: None,
			last_cursor_pos: None,
			toaster: crate::ui::components::toaster::Toaster::new(),
			animation_start: Instant::now(),
			animation_clock: Instant::now(),
			follow_end: true,
			last_focus: None,
			vim: crate::ui::vim::VimState::default(),
			server_endpoint_quic: String::new(),
			server_auth_token: String::new(),
			max_log_items_raw: String::new(),
			users_view: crate::ui::users_view::UsersView::new(),
			active_overlay: None,
			overlay_dismissed: false,
			main_window_id: None,
			settings_view: crate::ui::settings::SettingsView::new(SettingsCategory::General),
			pending_auto_connect_cfg: None,
			pending_join_target: None,
		}
	}
}

impl Default for ConnectionStatus {
	fn default() -> Self {
		Self::Disconnected { reason: None }
	}
}

#[derive(Debug)]
pub struct AppState {
	pub windows: HashMap<WindowId, WindowModel>,
	pub tabs: HashMap<TabId, TabModel>,
	pub connection: ConnectionStatus,
	pub ui: UiState,
	pub default_platform: Platform,
	pub last_focused_target: Option<TabTarget>,
	pub settings: GuiSettings,
	pub room_permissions: HashMap<RoomKey, RoomPermissions>,
	pub room_states: HashMap<RoomKey, RoomStateUi>,
	pub asset_bundles: HashMap<String, crate::ui::components::chat_message::AssetBundleUi>,
	pub selected_tab_id: Option<TabId>,
	pub tab_order: Vec<TabId>,
	pub room_asset_cache_keys: HashMap<RoomKey, Vec<String>>,
	pub global_asset_cache_keys: Vec<String>,
	pub notifications: Vec<UiNotification>,
	pub popped_windows: HashMap<iced::window::Id, WindowModel>,
	pub pending_popped_tabs: VecDeque<TabId>,
	pub pending_restore_windows: Vec<WindowModel>,
	pub main_window_geometry: crate::ui::layout::WindowGeometry,
	pub custom_themes: HashMap<String, crate::theme::Palette>,
	next_window_id: u64,
	next_tab_id: u64,
}

impl Default for AppState {
	fn default() -> Self {
		Self::new()
	}
}

impl AppState {
	pub fn new() -> Self {
		let settings = settings::get_cloned();

		Self {
			windows: HashMap::new(),
			tabs: HashMap::new(),
			connection: ConnectionStatus::default(),
			ui: UiState::default(),
			default_platform: settings.default_platform,
			last_focused_target: None,
			settings,
			notifications: Vec::new(),
			room_permissions: HashMap::new(),
			room_states: HashMap::new(),
			asset_bundles: HashMap::new(),
			selected_tab_id: None,
			tab_order: Vec::new(),
			room_asset_cache_keys: HashMap::new(),
			global_asset_cache_keys: Vec::new(),
			next_window_id: 1,
			next_tab_id: 1,
			popped_windows: HashMap::new(),
			pending_popped_tabs: VecDeque::new(),
			pending_restore_windows: Vec::new(),
			main_window_geometry: crate::ui::layout::WindowGeometry {
				width: 800,
				height: 600,
				x: -1,
				y: -1,
			},
			custom_themes: HashMap::new(),
		}
		.load_custom_themes()
	}

	pub fn load_custom_themes(mut self) -> Self {
		let Some(p) = dirs::config_dir() else {
			return self;
		};
		let theme_dir = p.join("chatty").join("themes");
		if !theme_dir.exists() {
			let _ = std::fs::create_dir_all(&theme_dir);
			return self;
		}

		if let Ok(entries) = std::fs::read_dir(theme_dir) {
			for entry in entries.flatten() {
				let path = entry.path();
				if path.extension().and_then(|s| s.to_str()) == Some("json")
					&& let Ok(content) = std::fs::read_to_string(&path)
					&& let Ok(palette) = serde_json::from_str::<crate::theme::Palette>(&content)
					&& let Some(name) = path.file_stem().and_then(|s| s.to_str())
				{
					self.custom_themes.insert(name.to_string(), palette);
				}
			}
		}

		self
	}

	pub fn gui_settings(&self) -> &GuiSettings {
		&self.settings
	}

	pub fn set_gui_settings(&mut self, cfg: GuiSettings) {
		info!(
			"set_gui_settings: auto_connect={} vim_nav={}",
			cfg.auto_connect_on_startup, cfg.keybinds.vim_nav
		);
		self.default_platform = cfg.default_platform;
		self.settings = cfg.clone();
		settings::set_and_persist(cfg);
	}

	pub fn parse_join_rooms(&self, req: &JoinRequest) -> Vec<RoomKey> {
		req.parse_rooms()
	}

	pub fn parse_join_room(&self, req: &JoinRequest) -> Option<RoomKey> {
		req.parse_first()
	}

	pub fn create_tab_for_rooms(&mut self, title: impl Into<String>, rooms: Vec<RoomKey>) -> TabId {
		let id = TabId(self.next_tab_id);
		self.next_tab_id += 1;

		let (panes, root) = pane_grid::State::new(ChatPane::new(Some(id)));

		self.tabs.insert(
			id,
			TabModel {
				id,
				title: title.into(),
				target: TabTarget(rooms),
				log: ChatLog::new(self.settings.max_log_items),
				user_counts: HashMap::new(),
				pinned: false,
				panes,
				focused_pane: Some(root),
			},
		);
		self.tab_order.push(id);

		id
	}

	pub fn pop_tab(&mut self, id: TabId) -> Option<TabModel> {
		if let Some(index) = self.tab_order.iter().position(|&t| t == id) {
			self.tab_order.remove(index);
		}

		if self.selected_tab_id == Some(id) {
			self.selected_tab_id = self.tab_order.first().cloned();
		}

		self.tabs.get(&id).cloned()
	}

	pub fn push_chat_item_for_room(&mut self, room: &RoomKey, item: ChatItem) -> Vec<TabId> {
		debug!(room = %room, item_kind = ?item, "push chat item for room");
		let matching_tabs: Vec<TabId> = self
			.tabs
			.iter()
			.filter_map(|(id, tab)| if tab.target.0.contains(room) { Some(*id) } else { None })
			.collect();

		for tid in &matching_tabs {
			if let Some(tab) = self.tabs.get_mut(tid) {
				let removed = tab.log.push(item.clone());
				for removed_item in removed {
					if let ChatItem::ChatMessage(m) = removed_item
						&& let Some(count) = tab.user_counts.get_mut(&m.user_login)
					{
						*count = count.saturating_sub(1);
						if *count == 0 {
							tab.user_counts.remove(&m.user_login);
						}
					}
				}

				if let ChatItem::ChatMessage(m) = &item {
					*tab.user_counts.entry(m.user_login.clone()).or_insert(0) += 1;
				}
			}
		}

		matching_tabs
	}

	pub fn push_message(&mut self, msg: ChatMessageUi) -> Vec<TabId> {
		let room = msg.room.clone();
		self.push_chat_item_for_room(&room, ChatItem::ChatMessage(Box::new(msg)))
	}

	pub fn remove_message(&mut self, room: &RoomKey, server_message_id: Option<&str>, platform_message_id: Option<&str>) {
		for (_tid, tab) in self.tabs.iter_mut() {
			let mut new_items = std::collections::VecDeque::with_capacity(tab.log.items.len());
			for item in tab.log.items.drain(..) {
				match item {
					ChatItem::ChatMessage(m) => {
						if &m.room == room {
							let mut should_remove = false;
							if let Some(sid) = server_message_id
								&& let Some(ref msg_sid) = m.server_message_id
								&& msg_sid == sid
							{
								should_remove = true;
							}
							if let Some(pid) = platform_message_id
								&& let Some(ref msg_pid) = m.platform_message_id
								&& msg_pid == pid
							{
								should_remove = true;
							}
							if should_remove {
								if let Some(count) = tab.user_counts.get_mut(&m.user_login) {
									*count = count.saturating_sub(1);
									if *count == 0 {
										tab.user_counts.remove(&m.user_login);
									}
								}
							} else {
								new_items.push_back(ChatItem::ChatMessage(m));
							}
						} else {
							new_items.push_back(ChatItem::ChatMessage(m));
						}
					}
					other => new_items.push_back(other),
				}
			}
			tab.log.items = new_items;
		}
	}

	pub fn set_connection_status(&mut self, st: ConnectionStatus) {
		info!(?st, "connection status changed");
		self.connection = st;
	}

	pub fn push_notification(&mut self, kind: UiNotificationKind, message: impl Into<String>) {
		self.notifications.push(UiNotification {
			kind,
			message: message.into(),
		});
	}

	pub fn take_notifications(&mut self) -> Vec<UiNotification> {
		std::mem::take(&mut self.notifications)
	}
}
