#![forbid(unsafe_code)]

use iced::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
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
	#[default]
	DarkAmethyst,
	Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Palette {
	#[serde(with = "color_hex")]
	pub app_bg: Color,
	#[serde(with = "color_hex")]
	pub panel_bg: Color,
	#[serde(with = "color_hex")]
	pub panel_bg_2: Color,
	#[serde(with = "color_hex")]
	pub surface_bg: Color,
	#[serde(with = "color_hex")]
	pub surface_hover_bg: Color,
	#[serde(with = "color_hex")]
	pub border: Color,
	#[serde(with = "color_hex")]
	pub text: Color,
	#[serde(with = "color_hex")]
	pub text_dim: Color,
	#[serde(with = "color_hex")]
	pub text_muted: Color,
	#[serde(with = "color_hex")]
	pub accent_green: Color,
	#[serde(with = "color_hex")]
	pub accent_blue: Color,
	#[serde(with = "color_hex")]
	pub chat_bg: Color,
	#[serde(with = "color_hex")]
	pub chat_row_bg: Color,
	#[serde(with = "color_hex")]
	pub chat_row_hover_bg: Color,
	#[serde(with = "color_hex")]
	pub chat_nick: Color,
	#[serde(with = "color_hex")]
	pub system_text: Color,
	#[serde(with = "color_hex")]
	pub tooltip_bg: Color,
	#[serde(with = "color_hex")]
	pub button_bg: Color,
	#[serde(with = "color_hex")]
	pub button_hover_bg: Color,
	#[serde(with = "color_hex")]
	pub button_text: Color,
	#[serde(with = "color_hex")]
	pub icon_button_bg: Color,
	#[serde(with = "color_hex")]
	pub icon_button_hover_bg: Color,
	#[serde(with = "color_hex")]
	pub warning_text: Color,
	#[serde(with = "color_hex")]
	pub warning_bg: Color,
}

mod color_hex {
	use iced::Color;
	use serde::{Deserialize, Deserializer, Serializer};

	pub fn serialize<S>(color: &Color, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let r = (color.r * 255.0) as u32;
		let g = (color.g * 255.0) as u32;
		let b = (color.b * 255.0) as u32;
		let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
		serializer.serialize_str(&hex)
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Color, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		let s = s.trim_start_matches('#');
		if let Ok(hex) = u32::from_str_radix(s, 16) {
			let r = ((hex >> 16) & 0xff) as f32 / 255.0;
			let g = ((hex >> 8) & 0xff) as f32 / 255.0;
			let b = (hex & 0xff) as f32 / 255.0;
			Ok(Color::from_rgb(r, g, b))
		} else {
			Err(serde::de::Error::custom("invalid hex color"))
		}
	}
}

fn rgb(hex: u32) -> Color {
	let r = ((hex >> 16) & 0xff) as f32 / 255.0;
	let g = ((hex >> 8) & 0xff) as f32 / 255.0;
	let b = (hex & 0xff) as f32 / 255.0;
	Color::from_rgb(r, g, b)
}

pub fn palette(kind: &ThemeKind, custom_palettes: &std::collections::HashMap<String, Palette>) -> Palette {
	match kind {
		ThemeKind::Custom(name) => custom_palettes
			.get(name)
			.cloned()
			.unwrap_or_else(|| palette(&ThemeKind::DarkAmethyst, custom_palettes)),
		ThemeKind::Dark => Palette {
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
			warning_text: rgb(0xfbbf24),
			warning_bg: rgb(0x422006),
		},
		ThemeKind::Light => Palette {
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
			warning_text: rgb(0x92400e),
			warning_bg: rgb(0xfef3c7),
		},
		ThemeKind::Solarized => Palette {
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
			warning_text: rgb(0xb58900),
			warning_bg: rgb(0x073642),
		},
		ThemeKind::HighContrast => Palette {
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
			warning_text: rgb(0xffff00),
			warning_bg: rgb(0x111111),
		},
		ThemeKind::Ocean => Palette {
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
			warning_text: rgb(0xfbbf24),
			warning_bg: rgb(0x123244),
		},
		ThemeKind::Dracula => Palette {
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
			warning_text: rgb(0xffb86c),
			warning_bg: rgb(0x44475a),
		},
		ThemeKind::Gruvbox => Palette {
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
			warning_text: rgb(0xfabd2f),
			warning_bg: rgb(0x3c3836),
		},
		ThemeKind::Nord => Palette {
			app_bg: rgb(0x2e3440),
			panel_bg: rgb(0x3b4252),
			panel_bg_2: rgb(0x434c5e),
			surface_bg: rgb(0x2e3440),
			surface_hover_bg: rgb(0x434c5e),
			border: rgb(0x4c566a),
			text: rgb(0xeceff4),
			text_dim: rgb(0xd8dee9),
			text_muted: rgb(0x81a1c1),
			accent_green: rgb(0xa3be8c),
			accent_blue: rgb(0x88c0d0),
			chat_bg: rgb(0x2e3440),
			chat_row_bg: rgb(0x2e3440),
			chat_row_hover_bg: rgb(0x3b4252),
			chat_nick: rgb(0x88c0d0),
			system_text: rgb(0x81a1c1),
			tooltip_bg: rgb(0x2e3440),
			button_bg: rgb(0x3b4252),
			button_hover_bg: rgb(0x4c566a),
			button_text: rgb(0xeceff4),
			icon_button_bg: rgb(0x3b4252),
			icon_button_hover_bg: rgb(0x4c566a),
			warning_text: rgb(0xebcb8b),
			warning_bg: rgb(0x3b4252),
		},
		ThemeKind::Synthwave => Palette {
			app_bg: rgb(0x241b2f),
			panel_bg: rgb(0x1e1627),
			panel_bg_2: rgb(0x2d243a),
			surface_bg: rgb(0x241b2f),
			surface_hover_bg: rgb(0x362c49),
			border: rgb(0x3a2f4f),
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
			warning_text: rgb(0xf0c674),
			warning_bg: rgb(0x362c49),
		},
		ThemeKind::DarkAmethyst => Palette {
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
			warning_text: rgb(0xfbbf24),
			warning_bg: rgb(0x1f2937),
		},
	}
}
