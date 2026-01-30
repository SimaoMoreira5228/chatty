#![forbid(unsafe_code)]

use chatty_domain::{Platform, RoomKey};
use rust_i18n::t;

use crate::settings::{ShortcutKey, SplitLayoutKind};
use crate::theme::ThemeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardTarget {
	Twitch,
	Kick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeChoice(pub ThemeKind);

impl ThemeChoice {
	pub fn all() -> Vec<ThemeChoice> {
		vec![
			ThemeChoice(ThemeKind::DarkAmethyst),
			ThemeChoice(ThemeKind::Dark),
			ThemeChoice(ThemeKind::Light),
			ThemeChoice(ThemeKind::Solarized),
			ThemeChoice(ThemeKind::HighContrast),
			ThemeChoice(ThemeKind::Ocean),
			ThemeChoice(ThemeKind::Dracula),
			ThemeChoice(ThemeKind::Gruvbox),
			ThemeChoice(ThemeKind::Nord),
			ThemeChoice(ThemeKind::Synthwave),
		]
	}
}

impl std::fmt::Display for ThemeChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.0 {
			ThemeKind::Dark => t!("theme.dark"),
			ThemeKind::Light => t!("theme.light"),
			ThemeKind::Solarized => t!("theme.solarized"),
			ThemeKind::HighContrast => t!("theme.high_contrast"),
			ThemeKind::Ocean => t!("theme.ocean"),
			ThemeKind::Dracula => t!("theme.dracula"),
			ThemeKind::Gruvbox => t!("theme.gruvbox"),
			ThemeKind::Nord => t!("theme.nord"),
			ThemeKind::Synthwave => t!("theme.synthwave"),
			ThemeKind::DarkAmethyst => t!("theme.dark_amethyst"),
			ThemeKind::Custom(ref name) => name.clone().into(),
		};
		write!(f, "{}", name)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitLayoutChoice(pub SplitLayoutKind);

impl SplitLayoutChoice {
	pub const ALL: [SplitLayoutChoice; 3] = [
		SplitLayoutChoice(SplitLayoutKind::Masonry),
		SplitLayoutChoice(SplitLayoutKind::Spiral),
		SplitLayoutChoice(SplitLayoutKind::Linear),
	];
}

impl std::fmt::Display for SplitLayoutChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.0 {
			SplitLayoutKind::Spiral => t!("split.spiral"),
			SplitLayoutKind::Masonry => t!("split.masonry"),
			SplitLayoutKind::Linear => t!("split.linear"),
		};

		write!(f, "{}", name)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutKeyChoice(pub ShortcutKey);

impl ShortcutKeyChoice {
	pub const ALL: [ShortcutKeyChoice; 6] = [
		ShortcutKeyChoice(ShortcutKey::Alt),
		ShortcutKeyChoice(ShortcutKey::Control),
		ShortcutKeyChoice(ShortcutKey::Shift),
		ShortcutKeyChoice(ShortcutKey::Logo),
		ShortcutKeyChoice(ShortcutKey::Always),
		ShortcutKeyChoice(ShortcutKey::None),
	];
}

impl std::fmt::Display for ShortcutKeyChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.0 {
			ShortcutKey::Alt => t!("shortcut.alt"),
			ShortcutKey::Control => t!("shortcut.control"),
			ShortcutKey::Shift => t!("shortcut.shift"),
			ShortcutKey::Logo => t!("shortcut.logo"),
			ShortcutKey::Always => t!("shortcut.always"),
			ShortcutKey::None => t!("shortcut.none"),
		};
		write!(f, "{}", name)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
	Main,
	Settings,
	Users,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
	General,
	Server,
	Accounts,
	Keybinds,
	Diagnostics,
}

impl SettingsCategory {
	pub const ALL: [SettingsCategory; 5] = [
		SettingsCategory::General,
		SettingsCategory::Server,
		SettingsCategory::Accounts,
		SettingsCategory::Keybinds,
		SettingsCategory::Diagnostics,
	];

	pub fn label_key(self) -> &'static str {
		match self {
			SettingsCategory::General => "settings.general",
			SettingsCategory::Server => "settings.server",
			SettingsCategory::Accounts => "settings.accounts",
			SettingsCategory::Keybinds => "settings.keybinds",
			SettingsCategory::Diagnostics => "settings.diagnostics",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformChoice(pub Platform);

impl PlatformChoice {
	pub const ALL: [PlatformChoice; 2] = [PlatformChoice(Platform::Twitch), PlatformChoice(Platform::Kick)];
}

impl std::fmt::Display for PlatformChoice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let label = match self.0 {
			Platform::Twitch => t!("platform.twitch"),
			Platform::Kick => t!("platform.kick"),
			_ => t!("platform.unknown"),
		};
		write!(f, "{}", label)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertTarget {
	Composer,
	Join,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PendingCommand {
	Delete {
		room: RoomKey,
		server_message_id: Option<String>,
		platform_message_id: Option<String>,
	},
	Timeout {
		room: RoomKey,
		user_id: String,
	},
	Ban {
		room: RoomKey,
		user_id: String,
	},
}
