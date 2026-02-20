use iced::widget::{button, column, container, pane_grid, pick_list, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use super::message::ChatMessageView;
use crate::app::features::chat::{ChatPane, ChatPaneMessage};
use crate::app::message::Message;
use crate::app::view_models::{ChatPaneLogItem, ChatPaneViewModel};
use crate::theme::Palette;

const MIN_WIDTH_FOR_SELECTOR: f32 = 450.0;
const PLATFORM_OPTIONS: [chatty_domain::Platform; 2] = [chatty_domain::Platform::Twitch, chatty_domain::Platform::Kick];

impl ChatPane {
	pub fn view<'a>(
		&'a self,
		vm: ChatPaneViewModel<'a>,
		assets: &'a crate::app::assets::AssetManager,
		palette: Palette,
	) -> pane_grid::Content<'a, Message> {
		let title_color = if vm.is_focused { palette.text } else { palette.text_dim };
		let title_bar = pane_grid::TitleBar::new(text(vm.title.clone()).color(title_color)).padding(6);

		let body: Element<'a, Message> = if vm.is_subscribed {
			self.view_subscribed_pane(vm, assets, palette)
		} else {
			self.view_unsubscribed_pane(vm, palette)
		};

		let pane_body = container(body)
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.chat_bg)),
				border: Border {
					color: palette.border,
					width: 1.0,
					radius: 8.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			});

		pane_grid::Content::new(pane_body).title_bar(title_bar)
	}

	fn view_subscribed_pane<'a>(
		&'a self,
		vm: ChatPaneViewModel<'a>,
		assets: &'a crate::app::assets::AssetManager,
		palette: Palette,
	) -> Element<'a, Message> {
		let mut col = column![].spacing(4);

		for warning in vm.warnings {
			col = col.push(
				container(text(warning).color(palette.warning_text))
					.padding(8)
					.style(move |_theme| container::Style {
						background: Some(Background::Color(palette.warning_bg)),
						border: Border {
							radius: 4.0.into(),
							..Default::default()
						},
						..Default::default()
					}),
			);
		}

		for item in vm.log_items {
			match item {
				ChatPaneLogItem::ChatMessage(model) => {
					col = col.push(ChatMessageView::new(*model, assets).view());
				}
				ChatPaneLogItem::SystemNotice(text_value) => {
					col = col.push(text(format!("{} {}", t!("log.system_label"), text_value)).color(palette.system_text));
				}
			}
		}

		let end_marker = container(text("")).id(format!("end-{:?}", vm.pane));
		let col = col.push(end_marker);

		let log_id = format!("log-{:?}", vm.pane);
		let log = scrollable(col)
			.id(log_id)
			.on_scroll(move |viewport| {
				Message::Layout(crate::app::message::LayoutMessage::ChatLogScrolled(vm.pane, viewport))
			})
			.height(Length::Fill)
			.width(Length::Fill);

		let mut input = text_input(&vm.placeholder, vm.composer_text)
			.on_input(move |v| Message::PaneMessage(vm.pane, ChatPaneMessage::ComposerChanged(v)))
			.width(Length::Fill)
			.id(format!("composer-{:?}", vm.pane));
		if vm.can_compose {
			input = input.on_submit(Message::PaneMessage(vm.pane, ChatPaneMessage::SendPressed));
		}

		let send_btn = if vm.can_compose {
			button(text(t!("main.send_label"))).on_press(Message::PaneMessage(vm.pane, ChatPaneMessage::SendPressed))
		} else {
			button(text(t!("main.send_label")).color(palette.text_muted))
		};

		let show_selector =
			vm.show_platform_selector && vm.window_width.map(|w| w >= MIN_WIDTH_FOR_SELECTOR).unwrap_or(true);

		let input_and_caret =
			container(row![input].spacing(4).align_y(Alignment::Center)).style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(if vm.composer_active {
					palette.panel_bg_2
				} else {
					palette.chat_bg
				})),
				border: Border {
					color: if vm.composer_active {
						palette.accent_blue
					} else {
						palette.border
					},
					width: 1.0,
					radius: 6.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			});

		let composer = if show_selector {
			let platform_selector = pick_list(&PLATFORM_OPTIONS[..], vm.selected_platform, move |p| {
				Message::PaneMessage(vm.pane, ChatPaneMessage::PlatformSelected(p))
			})
			.width(Length::Shrink);

			row![platform_selector, input_and_caret, send_btn]
				.spacing(8)
				.align_y(Alignment::Center)
		} else {
			row![input_and_caret, send_btn].spacing(8).align_y(Alignment::Center)
		};

		column![log, rule::horizontal(1), composer].spacing(8).padding(8).into()
	}

	fn view_unsubscribed_pane<'a>(&'a self, vm: ChatPaneViewModel<'a>, palette: Palette) -> Element<'a, Message> {
		let input = text_input(&vm.placeholder, vm.composer_text)
			.on_input(move |v| Message::PaneMessage(vm.pane, ChatPaneMessage::ComposerChanged(v)))
			.width(Length::Fill)
			.id(format!("composer-{:?}", vm.pane));
		let send_btn = button(text(t!("main.send_label")).color(palette.text_muted));
		let composer = row![input, send_btn].spacing(8).align_y(Alignment::Center);

		let info = column![
			text(t!("main.info_join_begin")).color(palette.text_dim),
			text(t!("main.info_use_join_field")).color(palette.text_muted),
			text(t!("main.info_split_button")).color(palette.text_muted),
		]
		.spacing(8)
		.padding(12);

		let end_marker = container(text("")).id(format!("end-{:?}", vm.pane));
		let col = info.push(end_marker);
		let log_id = format!("log-{:?}", vm.pane);
		let log = scrollable(col)
			.id(log_id)
			.on_scroll(move |viewport| {
				Message::Layout(crate::app::message::LayoutMessage::ChatLogScrolled(vm.pane, viewport))
			})
			.height(Length::Fill)
			.width(Length::Fill);
		column![log, rule::horizontal(1), composer].spacing(8).padding(8).into()
	}
}
