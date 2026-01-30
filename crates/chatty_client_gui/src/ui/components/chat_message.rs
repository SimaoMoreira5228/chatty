use std::sync::Arc;
use std::time::SystemTime;

use chatty_domain::{Platform, RoomKey};
use iced::widget::{container, image, mouse_area, row, stack, svg, text, text_editor};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::assets::AssetManager;
use crate::app::{Chatty, Message, PendingCommand};
use crate::assets::svg_handle;
use crate::theme::Palette;

#[derive(Debug, Clone)]
#[allow(dead_code)]
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

impl ChatMessageUi {
	pub fn key(&self) -> String {
		let time = self
			.time
			.duration_since(std::time::UNIX_EPOCH)
			.map(|d| d.as_millis())
			.unwrap_or(0);
		format!(
			"{}:{}:{}:{}",
			self.room,
			self.server_message_id.as_deref().unwrap_or(""),
			self.platform_message_id.as_deref().unwrap_or(""),
			time
		)
	}

	pub fn selection_text(text: &str) -> String {
		let mut out = String::with_capacity(text.len());
		let mut chars = text.chars().peekable();
		while let Some(ch) = chars.next() {
			if is_emoji_base(ch) {
				out.push('□');
				consume_emoji_suffix(&mut chars);
				continue;
			}

			if is_emoji_modifier(ch) || is_variation_selector(ch) || is_zwj(ch) {
				continue;
			}
			out.push(ch);
		}
		out
	}
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SystemNoticeUi {
	pub time: SystemTime,
	pub text: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LaggedUi {
	pub time: SystemTime,
	pub dropped: u64,
	pub detail: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AssetRefUi {
	pub id: String,
	pub name: String,
	pub image_url: String,
	pub image_format: String,
	pub width: u32,
	pub height: u32,
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

pub struct ChatMessageView<'a> {
	message: &'a ChatMessageUi,
	palette: Palette,
	is_focused: bool,
	is_pending: bool,
	anim_elapsed: std::time::Duration,
	emotes_map: Arc<std::collections::HashMap<String, AssetRefUi>>,
	badges_map: Arc<std::collections::HashMap<String, AssetRefUi>>,
	text_content: Option<&'a iced::widget::text_editor::Content>,
	assets: &'a AssetManager,
}

impl<'a> ChatMessageView<'a> {
	pub fn new(
		app: &'a Chatty,
		message: &'a ChatMessageUi,
		palette: Palette,
		is_focused: bool,
		emotes_map: Arc<std::collections::HashMap<String, AssetRefUi>>,
		badges_map: Arc<std::collections::HashMap<String, AssetRefUi>>,
	) -> Self {
		let anim_elapsed = app.state.ui.animation_clock.duration_since(app.state.ui.animation_start);
		let is_pending = app.pending_commands.iter().any(|pc| match pc {
			PendingCommand::Delete {
				room: r,
				server_message_id: s,
				platform_message_id: p,
			} => {
				(r == &message.room)
					&& s.as_ref() == message.server_message_id.as_ref()
					&& p.as_ref() == message.platform_message_id.as_ref()
			}
			_ => false,
		});

		let message_key = message.key();
		let text_content = app.message_text_editors.get(&message_key);

		Self {
			message,
			palette,
			is_focused,
			is_pending,
			anim_elapsed,
			emotes_map,
			badges_map,
			text_content,
			assets: &app.assets,
		}
	}

	pub fn view(self) -> Element<'a, Message> {
		let m = self.message;
		let palette = self.palette;
		let is_focused = self.is_focused;

		let name = m.user_display.clone().unwrap_or_else(|| m.user_login.clone());

		let mut msg_row = row![].spacing(6).align_y(Alignment::Center);

		if !m.badge_ids.is_empty() {
			for bid in &m.badge_ids {
				if let Some(badge) = self.badges_map.get(bid.as_str()) {
					let badge_element = self.render_image(&badge.image_url, 18, 18, Some(&badge.name));
					msg_row = msg_row.push(badge_element);
				}
			}
		}

		let name_txt = text(name).color(palette.chat_nick);
		let mut content_row = row![].spacing(4).align_y(Alignment::Center);

		let inline_emote = |token: &str| m.emotes.iter().find(|emote| emote.name == token);
		let tokens: Vec<&str> = m.text.split_whitespace().collect();

		for (i, token) in tokens.iter().enumerate() {
			if i > 0 {
				content_row = content_row.push(text(" ").color(if is_focused { palette.text } else { palette.text_dim }));
			}

			let found_emote = inline_emote(token).cloned().or_else(|| self.emotes_map.get(*token).cloned());

			let token_el: Element<'_, Message> = if let Some(emote) = found_emote {
				self.render_image(&emote.image_url, 20, 20, Some(&emote.name))
			} else {
				text(*token)
					.color(if is_focused { palette.text } else { palette.text_dim })
					.into()
			};

			content_row = content_row.push(token_el);
		}

		let content_row = content_row.width(Length::Fill).wrap();

		let message_key = m.key();
		let content_block: Element<'_, Message> = if let Some(content) = self.text_content {
			let key = message_key.clone();
			let overlay = text_editor(content)
				.on_action(move |action| Message::MessageTextEdit(key.clone(), action))
				.style(move |_theme, _status| text_editor::Style {
					background: Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.0)),
					border: Border::default(),
					placeholder: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
					value: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
					selection: Color::from_rgba(0.3, 0.6, 1.0, 0.35),
				});

			let overlay = container(overlay).width(Length::Fill).height(Length::Shrink);
			stack([content_row.into(), overlay.into()]).width(Length::Fill).into()
		} else {
			content_row.into()
		};

		msg_row = msg_row
			.push(name_txt)
			.push(text(": ").color(if is_focused { palette.text } else { palette.text_dim }))
			.push(content_block)
			.width(Length::Fill);

		if self.is_pending {
			msg_row = msg_row.push(svg(svg_handle("spinner.svg")).width(14).height(14));
		}

		mouse_area(msg_row)
			.on_right_press(Message::MessageActionButtonPressed(
				m.room.clone(),
				m.server_message_id.clone(),
				m.platform_message_id.clone(),
				m.author_id.clone(),
			))
			.into()
	}

	fn render_image(&self, url: &str, width: u32, height: u32, alt_text: Option<&str>) -> Element<'a, Message> {
		let animated = self
			.assets
			.animated_cache
			.get(url)
			.and_then(|anim| anim.frame_at(self.anim_elapsed).cloned());

		if let Some(handle) = animated {
			image(handle).width(width).height(height).into()
		} else if let Some(handle) = self.assets.image_cache.get(url) {
			image(handle).width(width).height(height).into()
		} else if let Some(handle) = self.assets.svg_cache.get(url) {
			svg(handle.clone()).width(width).height(height).into()
		} else {
			let loading = self.assets.image_loading.contains(url);
			let failed = self.assets.image_failed.contains(url);

			if loading {
				svg(svg_handle("spinner.svg")).width(width).height(height).into()
			} else if failed {
				if let Some(alt) = alt_text {
					text(format!("[{}]", alt)).color(self.palette.system_text).into()
				} else {
					svg(svg_handle("close.svg")).width(width).height(height).into()
				}
			} else {
				let _ = self.assets.image_fetch_sender.try_send(url.to_string());
				if let Some(alt) = alt_text {
					text(format!("[{}]", alt)).color(self.palette.text_dim).into()
				} else {
					svg(svg_handle("spinner.svg")).width(width).height(height).into()
				}
			}
		}
	}
}

pub fn is_emoji_base(ch: char) -> bool {
	let code = ch as u32;
	(0x1F300..=0x1FAFF).contains(&code) || (0x2600..=0x27BF).contains(&code) || (0x1F1E6..=0x1F1FF).contains(&code)
}

pub fn is_emoji_modifier(ch: char) -> bool {
	matches!(ch as u32, 0x1F3FB..=0x1F3FF)
}

pub fn is_variation_selector(ch: char) -> bool {
	matches!(ch, '\u{FE0E}' | '\u{FE0F}')
}

pub fn is_zwj(ch: char) -> bool {
	ch == '\u{200D}'
}

pub fn consume_emoji_suffix<I>(chars: &mut std::iter::Peekable<I>)
where
	I: Iterator<Item = char>,
{
	loop {
		match chars.peek().copied() {
			Some(next) if is_variation_selector(next) || is_emoji_modifier(next) => {
				chars.next();
			}
			Some(next) if is_zwj(next) => {
				chars.next();
				if let Some(after) = chars.next()
					&& is_emoji_base(after)
				{
					continue;
				}
			}
			Some(next) if (next as u32) >= 0x1F1E6 && (next as u32) <= 0x1F1FF => {
				chars.next();
			}
			_ => break,
		}
	}
}
