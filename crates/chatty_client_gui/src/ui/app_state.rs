#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::time::SystemTime;

use crate::ui::settings;
use crate::ui::settings::GuiSettings;

use chatty_domain::{Platform, RoomId, RoomKey, RoomTopic};
use std::str::FromStr;

/// Tab identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u64);

/// Window identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

/// Group identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(pub u64);

/// Content target for a tab.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TabTarget {
	Room(RoomKey),
	Group(GroupId),
}

/// Client-side group definition.
#[derive(Debug, Clone)]
pub struct GroupDef {
	pub id: GroupId,
	pub name: String,
	pub rooms: Vec<RoomKey>,
}

/// In-memory chat log for a tab target.
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

/// UI chat item.
#[derive(Debug, Clone)]
pub enum ChatItem {
	ChatMessage(ChatMessageUi),
	SystemNotice(SystemNoticeUi),
	Lagged(LaggedUi),
}

/// UI chat message.
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

	/// Optional platform message id.
	pub platform_message_id: Option<String>,
}

/// Per-room permission snapshot.
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

/// Asset reference for emotes/badges.
#[derive(Debug, Clone)]
pub struct AssetRefUi {
	pub id: String,
	pub name: String,
	pub image_url: String,
	pub image_format: String,
	pub width: u32,
	pub height: u32,
}

/// Cached asset bundle (emotes/badges) for a channel or global scope.
#[derive(Debug, Clone)]
pub struct AssetBundleUi {
	pub cache_key: String,
	pub etag: Option<String>,
	pub provider: i32,
	pub scope: i32,
	pub emotes: Vec<AssetRefUi>,
	pub badges: Vec<AssetRefUi>,
}

/// System notice.
#[derive(Debug, Clone)]
pub struct SystemNoticeUi {
	pub time: SystemTime,
	pub text: String,
}

/// Backpressure marker.
#[derive(Debug, Clone)]
pub struct LaggedUi {
	pub time: SystemTime,
	pub dropped: u64,
	pub detail: Option<String>,
}

/// Tab model.
#[derive(Debug, Clone)]
pub struct TabModel {
	pub id: TabId,
	pub title: String,
	pub target: TabTarget,

	/// In-memory chat log for what this tab displays.
	pub log: ChatLog,

	/// Optional: per-tab UI state (filters, highlights, scroll lock) later.
	pub pinned: bool,
}

#[derive(Debug, Clone)]
pub struct WindowModel {
	pub id: WindowId,
	pub title: String,

	/// Tabs in this window, in visual order.
	pub tabs: Vec<TabId>,

	/// Active tab.
	pub active_tab: Option<TabId>,
}

/// Join request from UI.
#[derive(Debug, Clone)]
pub struct JoinRequest {
	pub raw: String,
}

/// Connection status for UI.
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

/// Toast notification kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiNotificationKind {
	Info,
	Success,
	Warning,
	Error,
}

/// Queued notification.
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

/// Root application state.
#[derive(Debug)]
pub struct AppState {
	/// Windows and their tabs.
	pub windows: HashMap<WindowId, WindowModel>,

	/// All tabs.
	pub tabs: HashMap<TabId, TabModel>,

	/// Groups (client-side).
	pub groups: HashMap<GroupId, GroupDef>,

	/// Connection status.
	pub connection: ConnectionStatus,

	/// Default platform for bare channel names.
	pub default_platform: Platform,

	/// Last focused target (join convenience).
	pub last_focused_target: Option<TabTarget>,

	/// Cached GUI settings.
	pub settings: GuiSettings,

	/// Per-room permissions.
	pub room_permissions: HashMap<RoomKey, RoomPermissions>,

	/// Cached asset bundles by cache_key.
	pub asset_bundles: HashMap<String, AssetBundleUi>,

	/// Per-room mapping to asset cache keys.
	pub room_asset_cache_keys: HashMap<RoomKey, Vec<String>>,

	/// Global asset cache keys (scope=global).
	pub global_asset_cache_keys: Vec<String>,

	/// Pending notifications.
	pub notifications: Vec<UiNotification>,

	// Id counters.
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
		// Initialize from persisted settings.
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

	/// Get an immutable reference to the current GUI settings.
	pub fn gui_settings(&self) -> &GuiSettings {
		&self.settings
	}

	/// Replace GUI settings and persist them to disk (best-effort).
	pub fn set_gui_settings(&mut self, cfg: GuiSettings) {
		self.default_platform = cfg.default_platform;
		self.settings = cfg.clone();
		crate::ui::settings::set_and_persist(cfg);
	}

	pub fn set_room_permissions(&mut self, room: RoomKey, perms: RoomPermissions) {
		self.room_permissions.insert(room, perms);
	}

	pub fn upsert_asset_bundle(&mut self, room: Option<RoomKey>, bundle: AssetBundleUi) {
		let cache_key = bundle.cache_key.clone();
		let should_update = match self.asset_bundles.get(&cache_key) {
			Some(existing) => match (&existing.etag, &bundle.etag) {
				(Some(prev), Some(next)) if prev == next => false,
				_ => true,
			},
			None => true,
		};

		if should_update {
			self.asset_bundles.insert(cache_key.clone(), bundle);
		}

		match room {
			Some(room) => {
				let keys = self.room_asset_cache_keys.entry(room).or_default();
				if !keys.iter().any(|k| k == &cache_key) {
					keys.push(cache_key);
				}
			}
			None => {
				if !self.global_asset_cache_keys.iter().any(|k| k == &cache_key) {
					self.global_asset_cache_keys.push(cache_key);
				}
			}
		}
	}

	pub fn create_window(&mut self, title: impl Into<String>) -> WindowId {
		let id = WindowId(self.next_window_id);
		self.next_window_id += 1;

		self.windows.insert(
			id,
			WindowModel {
				id,
				title: title.into(),
				tabs: Vec::new(),
				active_tab: None,
			},
		);

		id
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

	pub fn create_group(&mut self, name: impl Into<String>, rooms: Vec<RoomKey>) -> GroupId {
		let id = GroupId(self.next_group_id);
		self.next_group_id += 1;

		self.groups.insert(
			id,
			GroupDef {
				id,
				name: name.into(),
				rooms,
			},
		);

		id
	}

	pub fn upsert_group(&mut self, group: GroupDef) {
		self.next_group_id = self.next_group_id.max(group.id.0.saturating_add(1));
		self.groups.insert(group.id, group);
	}

	pub fn remove_group(&mut self, group_id: GroupId) {
		self.groups.remove(&group_id);
		let tabs_to_close: Vec<TabId> = self
			.tabs
			.iter()
			.filter_map(|(id, tab)| match tab.target {
				TabTarget::Group(gid) if gid == group_id => Some(*id),
				_ => None,
			})
			.collect();
		for tab_id in tabs_to_close {
			self.close_tab(tab_id);
		}
	}

	pub fn move_tab_in_window(&mut self, window_id: WindowId, from: TabId, to: TabId) {
		let Some(win) = self.windows.get_mut(&window_id) else {
			return;
		};
		let Some(from_idx) = win.tabs.iter().position(|id| *id == from) else {
			return;
		};
		let Some(to_idx) = win.tabs.iter().position(|id| *id == to) else {
			return;
		};
		if from_idx == to_idx {
			return;
		}
		let tab = win.tabs.remove(from_idx);
		let insert_idx = if to_idx >= win.tabs.len() { win.tabs.len() } else { to_idx };
		win.tabs.insert(insert_idx, tab);
	}

	pub fn create_tab_for_group(&mut self, title: impl Into<String>, group_id: GroupId) -> TabId {
		let id = TabId(self.next_tab_id);
		self.next_tab_id += 1;

		self.tabs.insert(
			id,
			TabModel {
				id,
				title: title.into(),
				target: TabTarget::Group(group_id),
				log: ChatLog::new(self.settings.max_log_items),
				pinned: false,
			},
		);

		id
	}

	pub fn restore_tab(&mut self, ut: settings::UiTab) -> TabId {
		let id = TabId(ut.id.parse().unwrap_or(0));
		if id.0 == 0 {
			return id;
		}

		self.next_tab_id = self.next_tab_id.max(id.0 + 1);

		let target = if let Some(room) = ut.room {
			TabTarget::Room(room)
		} else if let Some(gid) = ut.group_id {
			TabTarget::Group(GroupId(gid))
		} else {
			return id;
		};

		self.tabs.entry(id).or_insert_with(|| TabModel {
			id,
			title: ut.title,
			target,
			log: ChatLog::new(self.settings.max_log_items),
			pinned: ut.pinned,
		});

		id
	}

	pub fn add_tab_to_window(&mut self, window_id: WindowId, tab_id: TabId) {
		let Some(w) = self.windows.get_mut(&window_id) else {
			return;
		};

		if !w.tabs.contains(&tab_id) {
			w.tabs.push(tab_id);
		}

		w.active_tab = Some(tab_id);

		if let Some(tab) = self.tabs.get(&tab_id) {
			self.last_focused_target = Some(tab.target.clone());
		}
	}

	pub fn set_active_tab(&mut self, window_id: WindowId, tab_id: TabId) {
		let Some(w) = self.windows.get_mut(&window_id) else {
			return;
		};

		if w.tabs.contains(&tab_id) {
			w.active_tab = Some(tab_id);
			if let Some(tab) = self.tabs.get(&tab_id) {
				self.last_focused_target = Some(tab.target.clone());
			}
		}
	}

	pub fn remove_tab_from_window(&mut self, window_id: WindowId, tab_id: TabId) {
		let Some(w) = self.windows.get_mut(&window_id) else {
			return;
		};

		w.tabs.retain(|t| *t != tab_id);

		if w.active_tab == Some(tab_id) {
			w.active_tab = w.tabs.last().copied();
		}
	}

	/// Move tab from one window to another (pop-out or drag).
	pub fn move_tab(&mut self, from: WindowId, to: WindowId, tab: TabId) {
		if from == to {
			return;
		}
		self.remove_tab_from_window(from, tab);
		self.add_tab_to_window(to, tab);
	}

	/// Remove a tab completely. (If it was hosted in a window, caller should detach it first.)
	pub fn close_tab(&mut self, tab_id: TabId) {
		self.tabs.remove(&tab_id);

		for w in self.windows.values_mut() {
			w.tabs.retain(|t| *t != tab_id);
			if w.active_tab == Some(tab_id) {
				w.active_tab = w.tabs.last().copied();
			}
		}
	}

	/// Parse a join input into a `RoomKey` when possible.
	pub fn parse_join_room(&self, req: &JoinRequest) -> Option<RoomKey> {
		let s = req.raw.trim();
		if s.is_empty() {
			return None;
		}

		// `room:<platform>/<id>` form
		if s.starts_with(RoomTopic::PREFIX) {
			return RoomTopic::parse(s).ok();
		}

		// `<platform>:<id>` form
		if let Some((platform_s, room_s)) = s.split_once(':') {
			let platform = Platform::from_str(platform_s).ok()?;
			let room_id = RoomId::new(room_s.to_string()).ok()?;
			return Some(RoomKey::new(platform, room_id));
		}

		let room_id = RoomId::new(s.to_string()).ok()?;
		let default_platform = settings::get_cloned().default_platform;
		Some(RoomKey::new(default_platform, room_id))
	}

	/// Append a chat item to tabs that target the given room.
	pub fn push_chat_item_for_room(&mut self, room: &RoomKey, item: ChatItem) {
		let matching_tabs: Vec<TabId> = self
			.tabs
			.iter()
			.filter_map(|(id, tab)| match &tab.target {
				TabTarget::Room(rk) if rk == room => Some(*id),
				TabTarget::Group(gid) => {
					let Some(g) = self.groups.get(gid) else {
						return None;
					};
					if g.rooms.iter().any(|r| r == room) { Some(*id) } else { None }
				}
				_ => None,
			})
			.collect();

		for tid in matching_tabs {
			if let Some(tab) = self.tabs.get_mut(&tid) {
				tab.log.push(item.clone());
			}
		}
	}

	/// Convenience: push a normal chat message.
	pub fn push_message(&mut self, msg: ChatMessageUi) {
		let room = msg.room.clone();
		self.push_chat_item_for_room(&room, ChatItem::ChatMessage(msg));
	}

	/// Convenience: push a lagged event (messages skipped).
	pub fn push_lagged(&mut self, room: &RoomKey, dropped: u64, detail: Option<String>) {
		self.push_chat_item_for_room(
			room,
			ChatItem::Lagged(LaggedUi {
				time: SystemTime::now(),
				dropped,
				detail,
			}),
		);
	}

	pub fn set_connection_status(&mut self, st: ConnectionStatus) {
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

	pub fn window(&self, id: WindowId) -> Option<&WindowModel> {
		self.windows.get(&id)
	}

	pub fn tab(&self, id: TabId) -> Option<&TabModel> {
		self.tabs.get(&id)
	}

	pub fn tab_mut(&mut self, id: TabId) -> Option<&mut TabModel> {
		self.tabs.get_mut(&id)
	}

	pub fn group(&self, id: GroupId) -> Option<&GroupDef> {
		self.groups.get(&id)
	}

	/// Return the list of rooms that a tab implies subscribing to.
	pub fn rooms_for_tab(&self, tab_id: TabId) -> Vec<RoomKey> {
		let Some(tab) = self.tabs.get(&tab_id) else {
			return vec![];
		};

		match &tab.target {
			TabTarget::Room(rk) => vec![rk.clone()],
			TabTarget::Group(gid) => self.groups.get(gid).map(|g| g.rooms.clone()).unwrap_or_default(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chatty_domain::{Platform, RoomId, RoomKey};

	#[test]
	fn create_tabs_and_rooms_for_tab() {
		let mut st = AppState::new();

		let win = st.create_window("test");
		let r1 = RoomKey::new(Platform::Twitch, RoomId::new("room1".to_string()).unwrap());
		let r2 = RoomKey::new(Platform::Twitch, RoomId::new("room2".to_string()).unwrap());

		let t1 = st.create_tab_for_room("room1", r1.clone());
		let t2 = st.create_tab_for_room("room2", r2.clone());

		st.add_tab_to_window(win, t1);
		st.add_tab_to_window(win, t2);

		assert_eq!(st.rooms_for_tab(t1), vec![r1.clone()]);
		assert_eq!(st.rooms_for_tab(t2), vec![r2.clone()]);

		st.close_tab(t1);
		assert!(st.tab(t1).is_none());

		let w = st.window(win).unwrap();
		assert!(!w.tabs.contains(&t1));
		assert!(w.tabs.contains(&t2));
	}

	#[test]
	fn rooms_for_group_tab_returns_all_rooms() {
		let mut st = AppState::new();

		let r1 = RoomKey::new(Platform::Twitch, RoomId::new("groom1".to_string()).unwrap());
		let r2 = RoomKey::new(Platform::Twitch, RoomId::new("groom2".to_string()).unwrap());
		let rooms = vec![r1.clone(), r2.clone()];

		let gid = st.create_group("mygroup", rooms.clone());
		let tab = st.create_tab_for_group("mygroup", gid);

		let found = st.rooms_for_tab(tab);
		assert_eq!(found.len(), 2);
		assert!(found.contains(&r1));
		assert!(found.contains(&r2));
	}

	#[test]
	fn chat_log_is_bounded_by_settings() {
		let mut st = AppState::new();
		st.settings.max_log_items = 5;

		let win = st.create_window("test");
		let room = RoomKey::new(Platform::Twitch, RoomId::new("room".to_string()).unwrap());
		let tab = st.create_tab_for_room("room", room.clone());
		st.add_tab_to_window(win, tab);

		for idx in 0..10 {
			st.push_message(ChatMessageUi {
				time: SystemTime::now(),
				platform: Platform::Twitch,
				room: room.clone(),
				server_message_id: None,
				author_id: None,
				user_login: format!("user{idx}"),
				user_display: None,
				text: format!("msg-{idx}"),
				badge_ids: Vec::new(),
				platform_message_id: None,
			});
		}

		let tab_model = st.tab(tab).expect("tab exists");
		assert_eq!(tab_model.log.items.len(), 5);
		let last = tab_model.log.items.back().expect("last item");
		match last {
			ChatItem::ChatMessage(msg) => assert_eq!(msg.text, "msg-9"),
			other => panic!("expected ChatMessage, got {other:?}"),
		}
	}
}
