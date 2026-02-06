use chatty_domain::Platform;
use iced::widget::{column, container, image, mouse_area, row, rule, stack, svg, text, tooltip};
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
		let is_deleted = self.model.is_deleted;
		let text_color = if is_deleted {
			palette.text_dim
		} else if is_focused {
			palette.text
		} else {
			palette.text_dim
		};
		let name_color = if is_deleted { palette.text_dim } else { palette.chat_nick };

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

		let name_txt = text(m.display_name.as_str()).color(name_color);
		let mut content_row = row![].spacing(4).align_y(Alignment::Start);

		let inline_emote = |token: &str| m.emotes.iter().find(|emote| emote.name == token);

		for (i, part) in m.token_parts.iter().enumerate() {
			let token_str = part.token.as_str();
			if i > 0 {
				content_row = content_row.push(text(" ").color(text_color));
			}

			let exact_emote = inline_emote(token_str).or_else(|| self.model.emotes_map.get(token_str));
			if let Some(emote) = exact_emote
				&& let Some(img) = emote.pick_image(AssetScaleUi::Two)
			{
				let (width, height) = self.emote_size(img, 32);
				content_row = content_row.push(self.render_image(&img.url, width, height, Some(emote.name.as_str())));
				continue;
			}

			let core = part.core.as_str();
			let prefix = part.prefix.as_str();
			let suffix = part.suffix.as_str();
			let has_word = part.has_word;

			let mut token_row = row![].spacing(0).align_y(Alignment::Start);
			if has_word {
				if !prefix.is_empty() {
					token_row = token_row.push(text(prefix).color(text_color));
				}

				let found_emote = inline_emote(core).or_else(|| self.model.emotes_map.get(core));

				let core_el: Element<'_, Message> = if let Some(emote) = found_emote {
					if let Some(img) = emote.pick_image(AssetScaleUi::Two) {
						let (width, height) = self.emote_size(img, 20);
						self.render_image(&img.url, width, height, Some(emote.name.as_str()))
					} else {
						text(core).color(text_color).into()
					}
				} else {
					text(core).color(text_color).into()
				};

				token_row = token_row.push(core_el);
				if !suffix.is_empty() {
					token_row = token_row.push(text(suffix).color(text_color));
				}
			} else {
				token_row = token_row.push(text(core).color(text_color));
			}

			content_row = content_row.push(token_row);
		}

		let content_block: Element<'_, Message> = content_row.width(Length::Fill).wrap().into();

		msg_row = msg_row
			.push(name_txt)
			.push(text(": ").color(text_color))
			.push(content_block)
			.width(Length::Fill)
			.height(Length::Shrink);

		if self.model.is_pending {
			msg_row = msg_row.push(svg(svg_handle("spinner.svg")).width(14).height(14));
		}

		let message_row: Element<'_, Message> = mouse_area(msg_row)
			.on_right_press(Message::Chat(crate::app::message::ChatMessage::MessageActionButtonPressed(
				m.room.clone(),
				m.server_message_id.as_deref().map(|v| v.to_string()),
				m.platform_message_id.as_deref().map(|v| v.to_string()),
				m.author_id.as_deref().map(|v| v.to_string()),
			)))
			.into();

		let message_row: Element<'_, Message> = if is_deleted {
			let strike = container(rule::horizontal(1))
				.width(Length::Fill)
				.height(Length::Fill)
				.center_y(Length::Fill);
			stack(vec![message_row, strike.into()]).width(Length::Fill).into()
		} else {
			message_row
		};

		if let Some(reply) = &self.model.reply {
			column![self.render_reply(reply), message_row].spacing(2).into()
		} else {
			message_row
		}
	}

	fn render_reply(&self, reply: &crate::app::view_models::ReplyPreviewUi) -> Element<'a, Message> {
		let palette = self.model.palette;
		let mut reply_row = row![].spacing(4).align_y(Alignment::Center);

		reply_row = reply_row.push(text("replying to ").size(12).color(palette.text_dim));

		let name_text = text(format!("@{}", reply.display_name.as_str()))
			.size(12)
			.color(if reply.is_own { palette.text } else { palette.text_dim });

		let name_el: Element<'_, Message> = if reply.is_own {
			container(name_text)
				.padding([1, 4])
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.accent_blue)),
					border: Border {
						color: palette.border,
						width: 1.0,
						radius: 4.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				})
				.into()
		} else {
			name_text.color(palette.text_dim).into()
		};

		reply_row = reply_row.push(name_el);
		reply_row = reply_row.push(text(format!(": {}", reply.message.as_str())).size(12).color(palette.text_dim));

		reply_row.into()
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
