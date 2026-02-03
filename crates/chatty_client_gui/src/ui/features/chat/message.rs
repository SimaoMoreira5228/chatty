use chatty_domain::Platform;
use iced::widget::{column, container, image, mouse_area, row, svg, text, tooltip};
use iced::{Alignment, Background, Border, Element, Length, Shadow};

use crate::app::assets::AssetManager;
use crate::app::message::Message;
use crate::app::view_models::{AssetImageUi, AssetScaleUi, ChatMessageViewModel};
use crate::assets::svg_handle;
use crate::theme::Palette;

pub struct ChatMessageView<'a> {
	model: ChatMessageViewModel<'a>,
	assets: &'a AssetManager,
}

impl<'a> ChatMessageView<'a> {
	pub fn new(model: ChatMessageViewModel<'a>, assets: &'a AssetManager) -> Self {
		Self { model, assets }
	}

	pub fn view(self) -> Element<'a, Message> {
		let m = self.model.message;
		let palette = self.model.palette;
		let is_focused = self.model.is_focused;

		let mut msg_row = row![].spacing(6).align_y(Alignment::Start);

		if self.model.show_platform_badge
			&& let Some(icon) = platform_icon(self.model.platform)
		{
			msg_row = msg_row.push(svg(svg_handle(icon)).width(14).height(14));
		}

		if !m.badge_ids.is_empty() {
			for bid in &m.badge_ids {
				if let Some(badge) = self.model.badges_map.get(bid.as_str())
					&& let Some(img) = badge.pick_image(AssetScaleUi::Two)
				{
					let badge_element = self.render_image(&img.url, 18, 18, Some(&badge.name));
					msg_row = msg_row.push(badge_element);
				}
			}
		}

		let name_txt = text(m.display_name.as_str()).color(palette.chat_nick);
		let mut content_row = row![].spacing(4).align_y(Alignment::Start);

		let inline_emote = |token: &str| m.emotes.iter().find(|emote| emote.name == token);
		let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';

		for (i, token) in m.tokens.iter().enumerate() {
			if i > 0 {
				content_row = content_row.push(text(" ").color(if is_focused { palette.text } else { palette.text_dim }));
			}

			let exact_emote = inline_emote(token.as_str())
				.cloned()
				.or_else(|| self.model.emotes_map.get(token.as_str()).cloned());
			if let Some(emote) = exact_emote
				&& let Some(img) = emote.pick_image(AssetScaleUi::Two)
			{
				let (width, height) = self.emote_size(img, 32);
				content_row = content_row.push(self.render_image(&img.url, width, height, Some(&emote.name)));
				continue;
			}

			let (core, prefix, suffix, has_word) = {
				let trimmed = token.trim_matches(|ch: char| ch.is_ascii_punctuation());
				if !trimmed.is_empty() && trimmed != token {
					if let Some(start) = token.find(trimmed) {
						let end = start + trimmed.len();
						let prefix = &token[..start];
						let suffix = &token[end..];
						(trimmed, prefix, suffix, true)
					} else {
						(token.as_str(), "", "", false)
					}
				} else {
					let mut start = None;
					let mut end = None;
					for (idx, ch) in token.char_indices() {
						if is_word_char(ch) {
							start = Some(idx);
							break;
						}
					}
					for (idx, ch) in token.char_indices().rev() {
						if is_word_char(ch) {
							end = Some(idx + ch.len_utf8());
							break;
						}
					}
					if let (Some(start), Some(end)) = (start, end) {
						let core = &token[start..end];
						let prefix = &token[..start];
						let suffix = &token[end..];
						(core, prefix, suffix, true)
					} else {
						(token.as_str(), "", "", false)
					}
				}
			};

			let mut token_row = row![].spacing(0).align_y(Alignment::Start);
			if has_word {
				if !prefix.is_empty() {
					token_row = token_row.push(text(prefix).color(if is_focused { palette.text } else { palette.text_dim }));
				}

				let found_emote = inline_emote(core)
					.cloned()
					.or_else(|| self.model.emotes_map.get(core).cloned());

				let core_el: Element<'_, Message> = if let Some(emote) = found_emote {
					if let Some(img) = emote.pick_image(AssetScaleUi::Two) {
						let (width, height) = self.emote_size(img, 20);
						self.render_image(&img.url, width, height, Some(&emote.name))
					} else {
						text(core)
							.color(if is_focused { palette.text } else { palette.text_dim })
							.into()
					}
				} else {
					text(core)
						.color(if is_focused { palette.text } else { palette.text_dim })
						.into()
				};

				token_row = token_row.push(core_el);
				if !suffix.is_empty() {
					token_row = token_row.push(text(suffix).color(if is_focused { palette.text } else { palette.text_dim }));
				}
			} else {
				token_row = token_row.push(text(core).color(if is_focused { palette.text } else { palette.text_dim }));
			}

			content_row = content_row.push(token_row);
		}

		let content_block: Element<'_, Message> = content_row.width(Length::Fill).wrap().into();

		msg_row = msg_row
			.push(name_txt)
			.push(text(": ").color(if is_focused { palette.text } else { palette.text_dim }))
			.push(content_block)
			.width(Length::Fill)
			.height(Length::Shrink);

		if self.model.is_pending {
			msg_row = msg_row.push(svg(svg_handle("spinner.svg")).width(14).height(14));
		}

		mouse_area(msg_row)
			.on_right_press(Message::Chat(crate::app::message::ChatMessage::MessageActionButtonPressed(
				m.room.clone(),
				m.server_message_id.clone(),
				m.platform_message_id.clone(),
				m.author_id.clone(),
			)))
			.into()
	}

	fn render_image(&self, url: &str, width: u32, height: u32, alt_text: Option<&str>) -> Element<'a, Message> {
		fn wrap_tooltip<'b>(
			base: Element<'b, Message>,
			large: Element<'b, Message>,
			label: String,
			palette: Palette,
		) -> Element<'b, Message> {
			let tooltip_body = column![large, text(label).size(12).color(palette.text_dim)]
				.spacing(4)
				.align_x(Alignment::Center);

			let tooltip_container = container(tooltip_body).padding(6).style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.tooltip_bg)),
				border: Border {
					color: palette.border,
					width: 1.0,
					radius: 6.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			});

			tooltip(base, tooltip_container, tooltip::Position::Top).into()
		}

		let tooltip_label = alt_text.map(|label| label.to_string());

		let animated = self
			.assets
			.animated_cache
			.get(url)
			.and_then(|anim| anim.frame_at(self.model.anim_elapsed).cloned());

		if let Some(handle) = animated {
			let base = image(handle.clone()).width(width).height(height).into();
			if let Some(label) = tooltip_label {
				let large = image(handle).width(48).height(48).into();
				wrap_tooltip(base, large, label, self.model.palette)
			} else {
				base
			}
		} else if let Some(handle) = self.assets.image_cache.get(url) {
			let base = image(handle.clone()).width(width).height(height).into();
			if let Some(label) = tooltip_label {
				let large = image(handle).width(48).height(48).into();
				wrap_tooltip(base, large, label, self.model.palette)
			} else {
				base
			}
		} else if let Some(handle) = self.assets.svg_cache.get(url) {
			let base = svg(handle.clone()).width(width).height(height).into();
			if let Some(label) = tooltip_label {
				let large = svg(handle).width(48).height(48).into();
				wrap_tooltip(base, large, label, self.model.palette)
			} else {
				base
			}
		} else {
			let loading = self.assets.image_loading.contains(url);
			let failed = self.assets.image_failed.contains(url);

			if loading {
				svg(svg_handle("spinner.svg")).width(width).height(height).into()
			} else if failed {
				if let Some(alt) = alt_text {
					text(format!("[{}]", alt)).color(self.model.palette.system_text).into()
				} else {
					svg(svg_handle("close.svg")).width(width).height(height).into()
				}
			} else {
				let _ = self.assets.image_fetch_sender.try_send(url.to_string());
				if let Some(alt) = alt_text {
					text(format!("[{}]", alt)).color(self.model.palette.text_dim).into()
				} else {
					svg(svg_handle("spinner.svg")).width(width).height(height).into()
				}
			}
		}
	}

	fn emote_size(&self, img: &AssetImageUi, fallback: u32) -> (u32, u32) {
		let scale = img.scale.as_u8().max(1) as f32;
		let width = if img.width == 0 {
			fallback
		} else {
			((img.width as f32) / scale).round().max(1.0) as u32
		};

		let height = if img.height == 0 {
			fallback
		} else {
			((img.height as f32) / scale).round().max(1.0) as u32
		};

		(width, height)
	}
}

fn platform_icon(platform: Platform) -> Option<&'static str> {
	match platform {
		Platform::Twitch => Some("platform-icons/twitch.svg"),
		Platform::Kick => Some("platform-icons/kick.svg"),
		Platform::YouTube => Some("platform-icons/youtube.svg"),
	}
}
