#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::time::SystemTime;

use chatty_domain::{Platform, RoomId, RoomKey, RoomTopic};
use tracing::{debug, info};

use crate::settings;
use crate::settings::GuiSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TabTarget {
	Room(RoomKey),
	Group(GroupId),
}

#[derive(Debug, Clone)]
pub struct GroupDef {
	pub id: GroupId,
	pub name: String,
	pub rooms: Vec<RoomKey>,
}

#[derive(Debug, Clone)]
pub struct ChatLog {
	pub items: VecDeque<ChatItem>,
	pub max_items: usize,
}

impl ChatLog {
	pub fn new(max_items: usize) -> Self {
		Self {
			items: VecDeque::new(),
			max_items,
		}
	}

	pub fn push(&mut self, item: ChatItem) {
		self.items.push_back(item);
		while self.items.len() > self.max_items {
			self.items.pop_front();
		}
	}
}

#[derive(Debug, Clone)]
pub enum ChatItem {
	ChatMessage(ChatMessageUi),
	SystemNotice(SystemNoticeUi),
	Lagged(LaggedUi),
}

#[derive(Debug, Clone)]
pub struct ChatMessageUi {
	pub time: SystemTime,
	pub platform: Platform,
	pub room: RoomKey,
	pub server_message_id: Option<String>,
	pub author_id: Option<String>,
	pub user_login: String,
	pub user_display: Option<String>,
	pub text: String,
	pub badge_ids: Vec<String>,
	pub emotes: Vec<AssetRefUi>,
	pub platform_message_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RoomPermissions {
	pub can_send: bool,
	pub can_reply: bool,
	pub can_delete: bool,
	pub can_timeout: bool,
	pub can_ban: bool,
	pub is_moderator: bool,
	pub is_broadcaster: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RoomStateUi {
	pub emote_only: Option<bool>,
	pub subscribers_only: Option<bool>,
	pub unique_chat: Option<bool>,
	pub slow_mode: Option<bool>,
	pub slow_mode_wait_time_seconds: Option<u64>,
	pub followers_only: Option<bool>,
	pub followers_only_duration_minutes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AssetRefUi {
	pub id: String,
	pub name: String,
	pub image_url: String,
	pub image_format: String,
	pub width: u32,
	pub height: u32,
}

#[derive(Debug, Clone)]
pub struct AssetBundleUi {
	pub cache_key: String,
	pub etag: Option<String>,
	pub provider: i32,
	pub scope: i32,
	pub emotes: Vec<AssetRefUi>,
	pub badges: Vec<AssetRefUi>,
}

#[derive(Debug, Clone)]
pub struct SystemNoticeUi {
	pub time: SystemTime,
	pub text: String,
}

#[derive(Debug, Clone)]
pub struct LaggedUi {
	pub time: SystemTime,
	pub dropped: u64,
	pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TabModel {
	pub id: TabId,
	pub title: String,
	pub target: TabTarget,
	pub log: ChatLog,
	pub pinned: bool,
}

#[derive(Debug, Clone)]
pub struct WindowModel {
	pub id: WindowId,
	pub title: String,
	pub tabs: Vec<TabId>,
	pub active_tab: Option<TabId>,
}

#[derive(Debug, Clone)]
pub struct JoinRequest {
	pub raw: String,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiNotificationKind {
	Info,
	Success,
	Warning,
	Error,
}

#[derive(Debug, Clone)]
pub struct UiNotification {
	pub kind: UiNotificationKind,
	pub message: String,
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
	pub groups: HashMap<GroupId, GroupDef>,
	pub connection: ConnectionStatus,
	pub default_platform: Platform,
	pub last_focused_target: Option<TabTarget>,
	pub settings: GuiSettings,
	pub room_permissions: HashMap<RoomKey, RoomPermissions>,
	pub room_states: HashMap<RoomKey, RoomStateUi>,
	pub asset_bundles: HashMap<String, AssetBundleUi>,
	pub room_asset_cache_keys: HashMap<RoomKey, Vec<String>>,
	pub global_asset_cache_keys: Vec<String>,
	pub notifications: Vec<UiNotification>,
	next_window_id: u64,
	next_tab_id: u64,
	next_group_id: u64,
}

impl Default for AppState {
	fn default() -> Self {
		Self::new()
	}
}

impl AppState {
	pub fn new() -> Self {
		let settings = settings::get_cloned();
		let mut state = Self {
			windows: HashMap::new(),
			tabs: HashMap::new(),
			groups: HashMap::new(),
			connection: ConnectionStatus::default(),
			default_platform: settings.default_platform,
			last_focused_target: None,
			settings,
			notifications: Vec::new(),
			room_permissions: HashMap::new(),
			room_states: HashMap::new(),
			asset_bundles: HashMap::new(),
			room_asset_cache_keys: HashMap::new(),
			global_asset_cache_keys: Vec::new(),
			next_window_id: 1,
			next_tab_id: 1,
			next_group_id: 1,
		};
		state.sync_groups_from_settings();
		state
	}

	pub fn sync_groups_from_settings(&mut self) {
		self.groups.clear();
		self.next_group_id = 1;
		for group in &self.settings.groups {
			let id = GroupId(group.id);
			self.groups.insert(
				id,
				GroupDef {
					id,
					name: group.name.clone(),
					rooms: group.rooms.clone(),
				},
			);
			self.next_group_id = self.next_group_id.max(group.id.saturating_add(1));
		}
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

	pub fn parse_join_room(&self, req: &JoinRequest) -> Option<RoomKey> {
		let s = req.raw.trim();
		if s.is_empty() {
			return None;
		}

		if s.starts_with(RoomTopic::PREFIX) {
			return RoomTopic::parse(s).ok();
		}

		if let Some((platform_s, room_s)) = s.split_once(':') {
			let platform = Platform::from_str(platform_s).ok()?;
			let room_id = RoomId::new(room_s.to_string()).ok()?;
			return Some(RoomKey::new(platform, room_id));
		}

		let room_id = RoomId::new(s.to_string()).ok()?;
		let default_platform = settings::get_cloned().default_platform;
		Some(RoomKey::new(default_platform, room_id))
	}

	pub fn create_tab_for_room(&mut self, title: impl Into<String>, room: RoomKey) -> TabId {
		let id = TabId(self.next_tab_id);
		self.next_tab_id += 1;

		self.tabs.insert(
			id,
			TabModel {
				id,
				title: title.into(),
				target: TabTarget::Room(room),
				log: ChatLog::new(self.settings.max_log_items),
				pinned: false,
			},
		);

		id
	}

	pub fn push_chat_item_for_room(&mut self, room: &RoomKey, item: ChatItem) -> Vec<TabId> {
		debug!(room = %room, item_kind = ?item, "push chat item for room");
		let matching_tabs: Vec<TabId> = self
			.tabs
			.iter()
			.filter_map(|(id, tab)| match &tab.target {
				TabTarget::Room(rk) if rk == room => Some(*id),
				TabTarget::Group(gid) => {
					let g = self.groups.get(gid)?;
					if g.rooms.iter().any(|r| r == room) { Some(*id) } else { None }
				}
				_ => None,
			})
			.collect();

		for tid in &matching_tabs {
			if let Some(tab) = self.tabs.get_mut(tid) {
				tab.log.push(item.clone());
			}
		}

		matching_tabs
	}

	pub fn push_message(&mut self, msg: ChatMessageUi) -> Vec<TabId> {
		let room = msg.room.clone();
		self.push_chat_item_for_room(&room, ChatItem::ChatMessage(msg))
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
							if !should_remove {
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

	pub fn push_lagged(&mut self, room: &RoomKey, dropped: u64, detail: Option<String>) -> Vec<TabId> {
		self.push_chat_item_for_room(
			room,
			ChatItem::Lagged(LaggedUi {
				time: SystemTime::now(),
				dropped,
				detail,
			}),
		)
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
