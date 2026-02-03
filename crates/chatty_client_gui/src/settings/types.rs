use chatty_domain::{Platform, RoomKey};
use serde::{Deserialize, Serialize};

use crate::theme::ThemeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ShortcutKey {
	#[default]
	Alt,
	Control,
	Shift,
	Logo,
	Always,
	None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybinds {
	pub drag_modifier: ShortcutKey,
	pub close_key: String,
	pub new_key: String,
	pub reconnect_key: String,
	pub vim_nav: bool,
	pub vim_left_key: String,
	pub vim_down_key: String,
	pub vim_up_key: String,
	pub vim_right_key: String,
}

impl Default for Keybinds {
	fn default() -> Self {
		Self {
			drag_modifier: ShortcutKey::Alt,
			close_key: "q".to_string(),
			new_key: "n".to_string(),
			reconnect_key: "r".to_string(),
			vim_nav: false,
			vim_left_key: "h".to_string(),
			vim_down_key: "j".to_string(),
			vim_up_key: "k".to_string(),
			vim_right_key: "l".to_string(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SplitLayoutKind {
	Spiral,
	#[default]
	Masonry,
	Linear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
	pub id: String,
	pub display_name: String,
	pub platform: Platform,
	pub username: String,
	pub user_id: String,
	pub oauth_token: String,
	pub refresh_token: String,
	pub client_id: String,
	pub enabled: bool,
}

impl Default for Identity {
	fn default() -> Self {
		Self {
			id: String::new(),
			display_name: String::new(),
			platform: Platform::Twitch,
			username: String::new(),
			user_id: String::new(),
			oauth_token: String::new(),
			refresh_token: String::new(),
			client_id: String::new(),
			enabled: true,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GroupSettings {
	pub id: u64,
	pub name: String,
	pub rooms: Vec<RoomKey>,
}

pub const CURRENT_SETTINGS_VERSION: u32 = 1;

pub fn default_settings_version() -> u32 {
	CURRENT_SETTINGS_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
	pub settings_version: u32,

	pub theme: ThemeKind,
	pub default_platform: Platform,
	pub max_log_items: usize,
	pub split_layout: SplitLayoutKind,
	pub identities: Vec<Identity>,
	pub active_identity: Option<String>,
	pub groups: Vec<GroupSettings>,
	pub server_endpoint_quic: String,
	pub server_auth_token: String,
	pub auto_connect_on_startup: bool,
	pub locale: String,
	pub keybinds: Keybinds,
}

impl Default for GuiSettings {
	fn default() -> Self {
		Self {
			settings_version: default_settings_version(),
			theme: ThemeKind::DarkAmethyst,
			default_platform: Platform::Twitch,
			max_log_items: 2000,
			split_layout: SplitLayoutKind::Masonry,
			identities: Vec::new(),
			active_identity: None,
			groups: Vec::new(),
			server_endpoint_quic: String::new(),
			server_auth_token: String::new(),
			auto_connect_on_startup: true,
			locale: "en-US".to_string(),
			keybinds: Keybinds::default(),
		}
	}
}
