#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use chatty_domain::{Platform, RoomKey};
use iced::widget::pane_grid::Pane;
use rust_i18n::t;

use crate::app::features::chat::ChatPane;
use crate::app::features::tabs::{ChatItem, TabModel, TabTarget};
use crate::app::model::Chatty;
use crate::app::types::InsertTarget;
use crate::theme::Palette;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatMessageUi {
	pub time: SystemTime,
	pub platform: Platform,
	pub room: RoomKey,
	pub key: String,
	pub server_message_id: Option<String>,
	pub author_id: Option<String>,
	pub user_login: String,
	pub user_display: Option<String>,
	pub display_name: String,
	pub text: String,
	pub tokens: Vec<String>,
	pub badge_ids: Vec<String>,
	pub emotes: Vec<AssetRefUi>,
	pub platform_message_id: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SystemNoticeUi {
	pub time: SystemTime,
	pub text: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AssetRefUi {
	pub id: String,
	pub name: String,
	pub images: Vec<AssetImageUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetScaleUi {
	One,
	Two,
	Three,
	Four,
}

impl AssetScaleUi {
	pub fn as_u8(self) -> u8 {
		match self {
			Self::One => 1,
			Self::Two => 2,
			Self::Three => 3,
			Self::Four => 4,
		}
	}
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AssetImageUi {
	pub scale: AssetScaleUi,
	pub url: String,
	pub format: String,
	pub width: u32,
	pub height: u32,
}

impl AssetRefUi {
	pub fn pick_image(&self, preferred: AssetScaleUi) -> Option<&AssetImageUi> {
		if self.images.is_empty() {
			return None;
		}
		if let Some(img) = self.images.iter().find(|img| img.scale == preferred) {
			return Some(img);
		}
		if let Some(img) = self.images.iter().find(|img| img.scale == AssetScaleUi::One) {
			return Some(img);
		}
		self.images.first()
	}
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AssetBundleUi {
	pub cache_key: String,
	pub etag: Option<String>,
	pub provider: i32,
	pub scope: i32,
	pub emotes: Vec<AssetRefUi>,
	pub badges: Vec<AssetRefUi>,
}

#[derive(Debug, Clone)]
pub struct ChatMessageViewModel<'a> {
	pub message: &'a ChatMessageUi,
	pub palette: Palette,
	pub is_focused: bool,
	pub is_pending: bool,
	pub anim_elapsed: std::time::Duration,
	pub emotes_map: Arc<HashMap<String, AssetRefUi>>,
	pub badges_map: Arc<HashMap<String, AssetRefUi>>,
}

#[derive(Debug, Clone)]
pub enum ChatPaneLogItem<'a> {
	ChatMessage(Box<ChatMessageViewModel<'a>>),
	SystemNotice(&'a str),
}

#[derive(Debug, Clone)]
pub struct ChatPaneViewModel<'a> {
	pub pane: Pane,
	pub title: String,
	pub is_focused: bool,
	pub is_subscribed: bool,
	pub can_compose: bool,
	pub composer_active: bool,
	pub placeholder: String,
	pub composer_text: &'a str,
	pub warnings: Vec<String>,
	pub log_items: Vec<ChatPaneLogItem<'a>>,
}

pub fn build_chat_pane_view_model<'a>(
	app: &'a Chatty,
	tab: &'a TabModel,
	pane: Pane,
	state: &'a ChatPane,
	palette: Palette,
) -> ChatPaneViewModel<'a> {
	let title = state
		.tab_id
		.and_then(|tid| app.state.tabs.get(&tid).map(|t| t.title.clone()))
		.unwrap_or_else(|| {
			if tab.title.is_empty() {
				t!("main.welcome").to_string()
			} else {
				tab.title.clone()
			}
		});

	let is_focused = Some(pane) == tab.focused_pane;

	let mut warnings = Vec::new();
	let mut log_items = Vec::new();
	let tab_ref = state
		.tab_id
		.and_then(|tid| app.state.tabs.get(&tid))
		.unwrap_or(tab);
	let rooms = tab_ref.target.0.clone();
	let can_send = rooms
		.iter()
		.any(|rk| app.state.room_permissions.get(rk).map(|p| p.can_send).unwrap_or(true));
	let connected = matches!(app.state.connection, crate::app::state::ConnectionStatus::Connected { .. });
	let can_compose = connected && !rooms.is_empty() && can_send;
	let is_subscribed = true;

	let restrictions = room_restrictions(app, &rooms);
	let placeholder = composer_placeholder(connected, &rooms, can_send, &restrictions);

	let mut platforms = HashSet::new();
	for room in &rooms {
		platforms.insert(room.platform);
	}
	for platform in platforms {
		let has_identity = app.state.gui_settings().identities.iter().any(|id| id.platform == platform);
		if !has_identity {
			let warning_text = match platform {
				chatty_domain::Platform::Twitch => t!("main.warning_no_twitch_login"),
				chatty_domain::Platform::Kick => t!("main.warning_no_kick_login"),
				_ => t!("main.warning_no_login"),
			};
			warnings.push(warning_text.to_string());
		}
	}

	let badges_map = app.assets.get_badges_for_target(&app.state, &tab_ref.target);
	let anim_elapsed = app.state.ui.animation_clock.duration_since(app.state.ui.animation_start);
	let start_index = tab_ref.log.items.len().saturating_sub(100);
	for item in tab_ref.log.items.iter().skip(start_index) {
		match item {
			ChatItem::ChatMessage(m) => {
				let room_target = TabTarget(vec![m.room.clone()]);
				let emotes_map = app.assets.get_emotes_for_target(&app.state, &room_target);
				let is_pending =
					app.is_pending_delete(&m.room, m.server_message_id.as_deref(), m.platform_message_id.as_deref());
				let model = ChatMessageViewModel {
					message: m.as_ref(),
					palette,
					is_focused,
					is_pending,
					anim_elapsed,
					emotes_map,
					badges_map: badges_map.clone(),
				};
				log_items.push(ChatPaneLogItem::ChatMessage(Box::new(model)));
			}
			ChatItem::SystemNotice(n) => {
				log_items.push(ChatPaneLogItem::SystemNotice(n.text.as_str()));
			}
		}
	}

	let composer_active = tab.focused_pane == Some(pane)
		&& app.state.ui.vim.insert_mode
		&& app.state.ui.vim.insert_target == Some(InsertTarget::Composer);

	ChatPaneViewModel {
		pane,
		title,
		is_focused,
		is_subscribed,
		can_compose,
		composer_active,
		placeholder,
		composer_text: state.composer.as_str(),
		warnings,
		log_items,
	}
}

fn room_restrictions(app: &Chatty, rooms: &[chatty_domain::RoomKey]) -> Vec<String> {
	let mut restrictions: Vec<String> = Vec::new();
	for rk in rooms {
		if let Some(state) = app.state.room_states.get(rk) {
			if state.emote_only == Some(true) {
				restrictions.push(t!("main.room_state_emote_only").to_string());
			}
			if state.subscribers_only == Some(true) {
				restrictions.push(t!("main.room_state_subscribers_only").to_string());
			}
			if state.unique_chat == Some(true) {
				restrictions.push(t!("main.room_state_unique_chat").to_string());
			}
			if state.followers_only == Some(true) {
				let label = if let Some(minutes) = state.followers_only_duration_minutes {
					format!("{} {}m", t!("main.room_state_followers_only"), minutes)
				} else {
					t!("main.room_state_followers_only").to_string()
				};
				restrictions.push(label);
			}
			if state.slow_mode == Some(true) {
				let label = if let Some(wait) = state.slow_mode_wait_time_seconds {
					format!("{} {}s", t!("main.room_state_slow_mode"), wait)
				} else {
					t!("main.room_state_slow_mode").to_string()
				};
				restrictions.push(label);
			}
		}
	}
	restrictions.sort();
	restrictions.dedup();
	restrictions
}

fn composer_placeholder(
	connected: bool,
	rooms: &[chatty_domain::RoomKey],
	can_send: bool,
	restrictions: &[String],
) -> String {
	if !connected {
		t!("main.placeholder_connect_to_send").to_string()
	} else if rooms.is_empty() {
		t!("main.placeholder_no_active_room").to_string()
	} else if !can_send {
		t!("main.placeholder_no_permission").to_string()
	} else if !restrictions.is_empty() {
		format!("{} ({})", t!("main.placeholder_message"), restrictions.join(", "))
	} else {
		t!("main.placeholder_message").to_string()
	}
}
