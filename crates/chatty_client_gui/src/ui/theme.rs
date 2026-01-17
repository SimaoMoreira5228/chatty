#![forbid(unsafe_code)]
#![allow(unused)]

use gpui::{App, Rgba, rgb};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Logical theme kinds you can choose from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeKind {
	Dark,
	Light,
	Solarized,
	HighContrast,
	Ocean,
	Dracula,
	Gruvbox,
	Nord,
	Synthwave,
	DarkAmethyst,
}

/// Chatty UI theme.
#[derive(Debug, Clone)]
pub struct Theme {
	// Core surfaces
	pub app_bg: Rgba,
	pub panel_bg: Rgba,
	pub panel_bg_2: Rgba,
	pub surface_bg: Rgba,
	pub surface_hover_bg: Rgba,

	// Borders
	pub border: Rgba,

	// Text
	pub text: Rgba,
	pub text_dim: Rgba,
	pub text_muted: Rgba,

	// Accents / status
	pub accent_green: Rgba,
	pub accent_blue: Rgba,

	// Chat-specific
	pub chat_bg: Rgba,
	pub chat_row_bg: Rgba,
	pub chat_row_hover_bg: Rgba,
	pub chat_nick: Rgba,
	pub system_text: Rgba,

	// Tooltip / popover
	pub tooltip_bg: Rgba,

	// Buttons
	pub button_bg: Rgba,
	pub button_hover_bg: Rgba,
	pub button_text: Rgba,

	// Icon-style buttons (used for small glyphs / compact controls)
	pub icon_button_bg: Rgba,
	pub icon_button_hover_bg: Rgba,
}

impl Theme {
	/// Default dark theme (matches the current hard-coded colors in `main_window.rs`).
	pub fn dark() -> Self {
		Self {
			app_bg: rgb(0x1e1f22),
			panel_bg: rgb(0x232428),
			panel_bg_2: rgb(0x2b2d31),
			surface_bg: rgb(0x1b1c20),
			surface_hover_bg: rgb(0x2a2b2f),
			border: rgb(0x303236),
			text: rgb(0xe3e5e8),
			text_dim: rgb(0xb1b5bd),
			text_muted: rgb(0x8a8f98),
			accent_green: rgb(0x2d7d46),
			accent_blue: rgb(0x7dd3fc),
			chat_bg: rgb(0x1b1c20),
			chat_row_bg: rgb(0x1b1c20),
			chat_row_hover_bg: rgb(0x24262b),
			chat_nick: rgb(0x7dd3fc),
			system_text: rgb(0x8a8f98),
			tooltip_bg: rgb(0x15171a),
			button_bg: rgb(0x2b2d31),
			button_hover_bg: rgb(0x35373c),
			button_text: rgb(0xe3e5e8),
			icon_button_bg: rgb(0x2b2d31),
			icon_button_hover_bg: rgb(0x35373c),
		}
	}

	/// Light / day theme suitable for bright UIs.
	pub fn light() -> Self {
		Self {
			app_bg: rgb(0xf6f7f8),
			panel_bg: rgb(0xffffff),
			panel_bg_2: rgb(0xf0f2f4),
			surface_bg: rgb(0xffffff),
			surface_hover_bg: rgb(0xf3f6f9),
			border: rgb(0xdcdfe3),
			text: rgb(0x0b0c0d),
			text_dim: rgb(0x55585b),
			text_muted: rgb(0x8a8d90),
			accent_green: rgb(0x1a9a4b),
			accent_blue: rgb(0x0077cc),
			chat_bg: rgb(0xffffff),
			chat_row_bg: rgb(0xffffff),
			chat_row_hover_bg: rgb(0xf6f7f8),
			chat_nick: rgb(0x0077cc),
			system_text: rgb(0x6b6f73),
			tooltip_bg: rgb(0x111214),
			button_bg: rgb(0xe9eef5),
			button_hover_bg: rgb(0xd6e2ff),
			button_text: rgb(0x0b0c0d),
			icon_button_bg: rgb(0xe9eef5),
			icon_button_hover_bg: rgb(0xd6e2ff),
		}
	}

	/// Solarized-inspired theme.
	pub fn solarized() -> Self {
		Self {
			app_bg: rgb(0x002b36),
			panel_bg: rgb(0x073642),
			panel_bg_2: rgb(0x002b36),
			surface_bg: rgb(0x073642),
			surface_hover_bg: rgb(0x0b3b46),
			border: rgb(0x08454f),
			text: rgb(0x93a1a1),
			text_dim: rgb(0x657b83),
			text_muted: rgb(0x586e75),
			accent_green: rgb(0x859900),
			accent_blue: rgb(0x268bd2),
			chat_bg: rgb(0x002b36),
			chat_row_bg: rgb(0x073642),
			chat_row_hover_bg: rgb(0x0b3b46),
			chat_nick: rgb(0x268bd2),
			system_text: rgb(0x586e75),
			tooltip_bg: rgb(0x002b36),
			button_bg: rgb(0x073642),
			button_hover_bg: rgb(0x0b3b46),
			button_text: rgb(0x93a1a1),
			icon_button_bg: rgb(0x073642),
			icon_button_hover_bg: rgb(0x0b3b46),
		}
	}

	/// High-contrast theme for accessibility.
	pub fn high_contrast() -> Self {
		Self {
			app_bg: rgb(0x000000),
			panel_bg: rgb(0x0a0a0a),
			panel_bg_2: rgb(0x111111),
			surface_bg: rgb(0x000000),
			surface_hover_bg: rgb(0x1a1a1a),
			border: rgb(0xffffff),
			text: rgb(0xffffff),
			text_dim: rgb(0xbfbfbf),
			text_muted: rgb(0x9f9f9f),
			accent_green: rgb(0x00ff00),
			accent_blue: rgb(0x00ffff),
			chat_bg: rgb(0x000000),
			chat_row_bg: rgb(0x000000),
			chat_row_hover_bg: rgb(0x111111),
			chat_nick: rgb(0xffff00),
			system_text: rgb(0xbfbfbf),
			tooltip_bg: rgb(0x000000),
			button_bg: rgb(0x111111),
			button_hover_bg: rgb(0x222222),
			button_text: rgb(0xffffff),
			icon_button_bg: rgb(0x111111),
			icon_button_hover_bg: rgb(0x222222),
		}
	}

	/// A cool "ocean" palette with teal accents.
	pub fn ocean() -> Self {
		Self {
			app_bg: rgb(0x071528),
			panel_bg: rgb(0x08263b),
			panel_bg_2: rgb(0x0b2f45),
			surface_bg: rgb(0x0a2433),
			surface_hover_bg: rgb(0x113245),
			border: rgb(0x123244),
			text: rgb(0xdbeefc),
			text_dim: rgb(0x95b9c9),
			text_muted: rgb(0x6f8d9a),
			accent_green: rgb(0x2bbfba),
			accent_blue: rgb(0x5aa9ff),
			chat_bg: rgb(0x071528),
			chat_row_bg: rgb(0x071528),
			chat_row_hover_bg: rgb(0x0b2f45),
			chat_nick: rgb(0x5aa9ff),
			system_text: rgb(0x95b9c9),
			tooltip_bg: rgb(0x071528),
			button_bg: rgb(0x123244),
			button_hover_bg: rgb(0x16485a),
			button_text: rgb(0xdbeefc),
			icon_button_bg: rgb(0x123244),
			icon_button_hover_bg: rgb(0x16485a),
		}
	}

	/// Dracula: Famous dark theme with vibrant purple/pink accents.
	pub fn dracula() -> Self {
		Self {
			app_bg: rgb(0x282a36),
			panel_bg: rgb(0x21222c),
			panel_bg_2: rgb(0x44475a),
			surface_bg: rgb(0x282a36),
			surface_hover_bg: rgb(0x44475a),
			border: rgb(0x6272a4),
			text: rgb(0xf8f8f2),
			text_dim: rgb(0xbd93f9),
			text_muted: rgb(0x6272a4),
			accent_green: rgb(0x50fa7b),
			accent_blue: rgb(0x8be9fd),
			chat_bg: rgb(0x282a36),
			chat_row_bg: rgb(0x282a36),
			chat_row_hover_bg: rgb(0x44475a),
			chat_nick: rgb(0xbd93f9),
			system_text: rgb(0x6272a4),
			tooltip_bg: rgb(0x191a21),
			button_bg: rgb(0x44475a),
			button_hover_bg: rgb(0x6272a4),
			button_text: rgb(0xf8f8f2),
			icon_button_bg: rgb(0x44475a),
			icon_button_hover_bg: rgb(0x6272a4),
		}
	}

	/// Gruvbox (Dark): Retro groove color scheme.
	pub fn gruvbox() -> Self {
		Self {
			app_bg: rgb(0x282828),
			panel_bg: rgb(0x1d2021),
			panel_bg_2: rgb(0x32302f),
			surface_bg: rgb(0x282828),
			surface_hover_bg: rgb(0x3c3836),
			border: rgb(0x504945),
			text: rgb(0xebdbb2),
			text_dim: rgb(0xa89984),
			text_muted: rgb(0x928374),
			accent_green: rgb(0xb8bb26),
			accent_blue: rgb(0x83a598),
			chat_bg: rgb(0x282828),
			chat_row_bg: rgb(0x282828),
			chat_row_hover_bg: rgb(0x32302f),
			chat_nick: rgb(0x83a598),
			system_text: rgb(0x928374),
			tooltip_bg: rgb(0x1d2021),
			button_bg: rgb(0x3c3836),
			button_hover_bg: rgb(0x504945),
			button_text: rgb(0xebdbb2),
			icon_button_bg: rgb(0x3c3836),
			icon_button_hover_bg: rgb(0x504945),
		}
	}

	/// Nord: An arctic, north-bluish color palette.
	pub fn nord() -> Self {
		Self {
			app_bg: rgb(0x2e3440),
			panel_bg: rgb(0x3b4252),
			panel_bg_2: rgb(0x434c5e),
			surface_bg: rgb(0x2e3440),
			surface_hover_bg: rgb(0x3b4252),
			border: rgb(0x4c566a),
			text: rgb(0xd8dee9),
			text_dim: rgb(0xe5e9f0),
			text_muted: rgb(0x4c566a),
			accent_green: rgb(0xa3be8c),
			accent_blue: rgb(0x88c0d0),
			chat_bg: rgb(0x2e3440),
			chat_row_bg: rgb(0x2e3440),
			chat_row_hover_bg: rgb(0x3b4252),
			chat_nick: rgb(0x81a1c1),
			system_text: rgb(0x4c566a),
			tooltip_bg: rgb(0x242933),
			button_bg: rgb(0x434c5e),
			button_hover_bg: rgb(0x4c566a),
			button_text: rgb(0xd8dee9),
			icon_button_bg: rgb(0x434c5e),
			icon_button_hover_bg: rgb(0x4c566a),
		}
	}

	/// Synthwave: Neon colors on a deep purple background.
	pub fn synthwave() -> Self {
		Self {
			app_bg: rgb(0x241b2f),
			panel_bg: rgb(0x2a2139),
			panel_bg_2: rgb(0x362c49),
			surface_bg: rgb(0x241b2f),
			surface_hover_bg: rgb(0x2a2139),
			border: rgb(0x49365e),
			text: rgb(0xf0f0f0),
			text_dim: rgb(0xb45bcf),
			text_muted: rgb(0x6b5382),
			accent_green: rgb(0x00ff9f),
			accent_blue: rgb(0x00e5ff),
			chat_bg: rgb(0x241b2f),
			chat_row_bg: rgb(0x241b2f),
			chat_row_hover_bg: rgb(0x362c49),
			chat_nick: rgb(0xff00cc),
			system_text: rgb(0x6b5382),
			tooltip_bg: rgb(0x191021),
			button_bg: rgb(0x362c49),
			button_hover_bg: rgb(0x49365e),
			button_text: rgb(0xff00cc),
			icon_button_bg: rgb(0x362c49),
			icon_button_hover_bg: rgb(0x49365e),
		}
	}

	/// Dark Amethyst: Dark neutral surfaces with purple accents.
	pub fn dark_amethyst() -> Self {
		Self {
			app_bg: rgb(0x030712),
			panel_bg: rgb(0x1f2937),
			panel_bg_2: rgb(0x161d27),
			surface_bg: rgb(0x1f2937),
			surface_hover_bg: rgb(0x253141),
			border: rgb(0x1f2937),
			button_bg: rgb(0x6d28d9),
			button_hover_bg: rgb(0x8952e0),
			icon_button_bg: rgb(0x6d28d9),
			icon_button_hover_bg: rgb(0x8952e0),
			text: rgb(0xf9fafb),
			text_dim: rgb(0x9ca3af),
			text_muted: rgb(0x9ca3af),
			button_text: rgb(0xf9fafb),
			system_text: rgb(0x9ca3af),
			accent_green: rgb(0x2eb88a),
			accent_blue: rgb(0x6d28d9),
			chat_nick: rgb(0x6d28d9),
			chat_bg: rgb(0x030712),
			chat_row_bg: rgb(0x030712),
			chat_row_hover_bg: rgb(0x1f2937),
			tooltip_bg: rgb(0x1f2937),
		}
	}

	/// Construct a theme from a `ThemeKind`.
	pub fn from_kind(kind: ThemeKind) -> Self {
		match kind {
			ThemeKind::Dark => Self::dark(),
			ThemeKind::Light => Self::light(),
			ThemeKind::Solarized => Self::solarized(),
			ThemeKind::HighContrast => Self::high_contrast(),
			ThemeKind::Ocean => Self::ocean(),
			ThemeKind::Dracula => Self::dracula(),
			ThemeKind::Gruvbox => Self::gruvbox(),
			ThemeKind::Nord => Self::nord(),
			ThemeKind::Synthwave => Self::synthwave(),
			ThemeKind::DarkAmethyst => Self::dark_amethyst(),
		}
	}
}

use crate::ui::settings;

/// Shorthand accessor used by most UI modules.
pub fn theme() -> Theme {
	let kind = settings::theme_kind();
	Theme::from_kind(kind)
}

/// Convenience: pick a theme by kind without persisting.
pub fn theme_with_kind(kind: ThemeKind) -> Theme {
	Theme::from_kind(kind)
}

/// Set the current theme kind and persist it via the centralized settings module.
pub fn set_theme_kind(kind: ThemeKind) {
	settings::set_theme(kind);
}

/// Sync gpui-component theme colors to the active Chatty theme.
pub fn sync_component_theme(cx: &mut App, kind: ThemeKind) {
	let t = Theme::from_kind(kind);
	let comp = gpui_component::Theme::global_mut(cx);

	comp.colors.background = t.app_bg.into();
	comp.colors.foreground = t.text.into();
	comp.colors.border = t.border.into();
	comp.colors.input = t.border.into();
	comp.colors.muted = t.panel_bg_2.into();
	comp.colors.muted_foreground = t.text_dim.into();
	comp.colors.popover = t.panel_bg.into();
	comp.colors.popover_foreground = t.text.into();
	comp.colors.list = t.panel_bg.into();
	comp.colors.list_hover = t.panel_bg_2.into();
	comp.colors.list_active = t.panel_bg_2.into();
	comp.colors.list_active_border = t.border.into();
	comp.colors.accent = t.button_bg.into();
	comp.colors.accent_foreground = t.button_text.into();
	comp.colors.secondary = t.panel_bg_2.into();
	comp.colors.secondary_foreground = t.text.into();
	comp.colors.primary = t.button_bg.into();
	comp.colors.primary_foreground = t.button_text.into();
	comp.colors.ring = t.border.into();
	comp.colors.scrollbar = t.border.into();
	comp.colors.scrollbar_thumb = t.border.into();
	comp.colors.scrollbar_thumb_hover = t.button_hover_bg.into();
}
