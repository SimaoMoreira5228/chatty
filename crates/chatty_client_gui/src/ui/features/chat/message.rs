use chatty_domain::Platform;
use iced::widget::{column, container, image, mouse_area, row, rule, stack, svg, text};
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

		let mut inline_widgets: Vec<Element<'a, Message>> = Vec::new();

		if self.model.show_platform_badge
			&& let Some(icon) = platform_icon(self.model.platform)
		{
			inline_widgets.push(svg(svg_handle(icon)).width(14).height(14).into());
		}

		if !m.badge_ids.is_empty() {
			for bid in &m.badge_ids {
				if let Some(badge) = self.model.badges_map.get(bid.as_str())
					&& let Some(img) = badge.pick_image(AssetScaleUi::Two)
				{
					let badge_element = self.render_inline_image(&img.url, 18, 18);
					inline_widgets.push(badge_element);
				}
			}
		}

		let has_emotes = self.message_has_emotes();
		
		if !has_emotes {
			return self.view_text_only(inline_widgets, name_color, text_color, is_deleted);
		}

		self.view_with_inline_emotes(inline_widgets, name_color, text_color, is_deleted)
	}

	fn message_has_emotes(&self) -> bool {
		let m = self.model.message;
		let inline_emote = |token: &str| m.emotes.iter().find(|emote| emote.name == token);

		for part in m.token_parts.iter() {
			let token_str = part.token.as_str();
			if inline_emote(token_str).or_else(|| self.model.emotes_map.get(token_str)).is_some() {
				return true;
			}

			if part.has_word {
				let core_emote = inline_emote(part.core.as_str())
					.or_else(|| self.model.emotes_map.get(part.core.as_str()));
				if core_emote.is_some() {
					return true;
				}
			}
		}
		false
	}

	fn view_text_only(
		self,
		mut inline_widgets: Vec<Element<'a, Message>>,
		name_color: iced::Color,
		text_color: iced::Color,
		is_deleted: bool,
	) -> Element<'a, Message> {
		let m = self.model.message;

		let mut spans: Vec<iced::widget::text::Span<'a, (), iced::Font>> = Vec::new();

		spans.push(iced::widget::text::Span::new(m.display_name.as_str()).color(name_color));
		spans.push(iced::widget::text::Span::new(": ").color(text_color));

		for (i, part) in m.token_parts.iter().enumerate() {
			if i > 0 {
				spans.push(iced::widget::text::Span::new(" ").color(text_color));
			}

			let prefix = part.prefix.as_str();
			let core = part.core.as_str();
			let suffix = part.suffix.as_str();

			if part.has_word {
				if !prefix.is_empty() {
					spans.push(iced::widget::text::Span::new(prefix).color(text_color));
				}
				spans.push(iced::widget::text::Span::new(core).color(text_color));
				if !suffix.is_empty() {
					spans.push(iced::widget::text::Span::new(suffix).color(text_color));
				}
			} else {
				spans.push(iced::widget::text::Span::new(core).color(text_color));
			}
		}

		inline_widgets.push(
			iced::widget::rich_text(spans)
				.width(Length::Fill)
				.into()
		);

		if self.model.is_pending {
			inline_widgets.push(svg(svg_handle("spinner.svg")).width(14).height(14).into());
		}

		self.finalize_inline(inline_widgets, is_deleted, false)
	}

	fn view_with_inline_emotes(
		self,
		mut inline_widgets: Vec<Element<'a, Message>>,
		name_color: iced::Color,
		text_color: iced::Color,
		is_deleted: bool,
	) -> Element<'a, Message> {
		let m = self.model.message;

		enum Segment<'b> {
			Text(&'b str),
			Emote { url: String, name: String, width: u32, height: u32 },
		}

		let mut segments: Vec<Segment<'a>> = Vec::new();
		let inline_emote = |token: &str| m.emotes.iter().find(|emote| emote.name == token);

		for (i, part) in m.token_parts.iter().enumerate() {
			if i > 0 {
				segments.push(Segment::Text(" "));
			}

			let token_str = part.token.as_str();
			let exact_emote = inline_emote(token_str).or_else(|| self.model.emotes_map.get(token_str));

			if let Some(emote) = exact_emote
				&& let Some(img) = emote.pick_image(AssetScaleUi::Two)
			{
				let (width, height) = self.emote_size(img, 32);
				segments.push(Segment::Emote {
					url: img.url.clone(),
					name: emote.name.clone(),
					width,
					height,
				});
				continue;
			}

			let core = part.core.as_str();
			let prefix = part.prefix.as_str();
			let suffix = part.suffix.as_str();

			if part.has_word {
				let core_emote = inline_emote(core).or_else(|| self.model.emotes_map.get(core));

				if let Some(emote) = core_emote
					&& let Some(img) = emote.pick_image(AssetScaleUi::Two)
				{
					let (width, height) = self.emote_size(img, 20);
					if !prefix.is_empty() {
						segments.push(Segment::Text(prefix));
					}
					segments.push(Segment::Emote {
						url: img.url.clone(),
						name: emote.name.clone(),
						width,
						height,
					});
					if !suffix.is_empty() {
						segments.push(Segment::Text(suffix));
					}
					continue;
				}
			}

			if part.has_word {
				if !prefix.is_empty() {
					segments.push(Segment::Text(prefix));
				}
				segments.push(Segment::Text(core));
				if !suffix.is_empty() {
					segments.push(Segment::Text(suffix));
				}
			} else {
				segments.push(Segment::Text(core));
			}
		}

		let mut all_text = String::new();
		for seg in &segments {
			if let Segment::Text(t) = seg {
				all_text.push_str(t);
			}
		}

		if all_text.contains('\n') || all_text.len() > 200 {
			let mut spans: Vec<iced::widget::text::Span<'a, (), iced::Font>> = Vec::new();
			spans.push(iced::widget::text::Span::new(m.display_name.as_str()).color(name_color));
			spans.push(iced::widget::text::Span::new(": ").color(text_color));

			let mut text_buffer = String::new();
			let mut pending_emotes: Vec<(String, String, u32, u32)> = Vec::new();

			for segment in segments {
				match segment {
					Segment::Text(t) => {
						text_buffer.push_str(t);
					}
					Segment::Emote { url, name, width, height } => {
						text_buffer.push(' ');
						pending_emotes.push((url, name, width, height));
					}
				}
			}

			if !text_buffer.is_empty() {
				spans.push(iced::widget::text::Span::new(text_buffer).color(text_color));
			}

			inline_widgets.push(
				iced::widget::rich_text(spans)
					.width(Length::Fill)
					.into()
			);

			for (url, name, width, height) in pending_emotes {
				inline_widgets.push(self.render_image(&url, width, height, Some(&name)));
			}
		} else {
			inline_widgets.push(
				text(format!("{}:", m.display_name.as_str()))
					.color(name_color)
					.into()
			);

			let mut text_buffer = String::new();

			for segment in segments {
				match segment {
					Segment::Text(t) => {
						text_buffer.push_str(t);
					}
					Segment::Emote { url, name, width, height } => {
						if !text_buffer.is_empty() {
							let text_content = std::mem::take(&mut text_buffer);
							inline_widgets.push(text(text_content).color(text_color).width(Length::Shrink).into());
						}
						inline_widgets.push(self.render_image(&url, width, height, Some(&name)));
					}
				}
			}

			if !text_buffer.is_empty() {
				inline_widgets.push(text(text_buffer).color(text_color).width(Length::Fill).into());
			}
		}

		if self.model.is_pending {
			inline_widgets.push(svg(svg_handle("spinner.svg")).width(14).height(14).into());
		}

		self.finalize_inline(inline_widgets, is_deleted, true)
	}

	fn finalize_inline(
		self,
		inline_widgets: Vec<Element<'a, Message>>,
		is_deleted: bool,
		use_wrap: bool,
	) -> Element<'a, Message> {
		let m = self.model.message;

		let mut inline_row = row![].spacing(4).align_y(Alignment::Center);
		for widget in inline_widgets {
			inline_row = inline_row.push(widget);
		}

		let content: Element<'a, Message> = if use_wrap {
			inline_row.width(Length::Fill).wrap().into()
		} else {
			inline_row.width(Length::Fill).into()
		};

		let message_row: Element<'a, Message> = mouse_area(content)
			.on_right_press(Message::Chat(crate::app::message::ChatMessage::MessageActionButtonPressed(
				m.room.clone(),
				m.server_message_id.as_deref().map(|v| v.to_string()),
				m.platform_message_id.as_deref().map(|v| v.to_string()),
				m.author_id.as_deref().map(|v| v.to_string()),
			)))
			.into();

		let message_row: Element<'a, Message> = if is_deleted {
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

		let spans: Vec<iced::widget::text::Span<'a, (), iced::Font>> = vec![
			iced::widget::text::Span::new("replying to ").color(palette.text_dim),
			iced::widget::text::Span::new(format!("@{}", reply.display_name.as_str()))
				.color(if reply.is_own { palette.text } else { palette.text_dim }),
			iced::widget::text::Span::new(format!(": {}", reply.message.as_str())).color(palette.text_dim),
		];

		let reply_text = iced::widget::rich_text(spans)
			.size(12)
			.width(Length::Fill);

		if reply.is_own {
			container(reply_text)
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
			reply_text.into()
		}
	}

	fn render_inline_image(&self, url: &str, width: u32, height: u32) -> Element<'a, Message> {
		if let Some(handle) = self.assets.image_cache.get(url) {
			image(handle.clone()).width(width).height(height).into()
		} else if let Some(handle) = self.assets.svg_cache.get(url) {
			svg(handle.clone()).width(width).height(height).into()
		} else {
			let loading = self.assets.image_loading.contains(url);
			if loading {
				svg(svg_handle("spinner.svg")).width(width).height(height).into()
			} else {
				let _ = self.assets.image_fetch_sender.try_send(url.to_string());
				text("").width(width as f32).height(height as f32).into()
			}
		}
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

			iced::widget::tooltip(base, tooltip_container, iced::widget::tooltip::Position::Top).into()
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
