use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use super::overlay::wrap_overlay;
use crate::app::features::overlays::{LayoutModal, LayoutModalKind, LayoutModalMessage, OverlayMessage};
use crate::app::message::Message;
use crate::theme;

impl LayoutModal {
	pub fn view<'a>(&'a self, palette: theme::Palette) -> Element<'a, Message> {
		match &self.kind {
			LayoutModalKind::Export { path, .. } => {
				let path_label = path
					.as_ref()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_else(|| t!("settings.no_path_chosen").to_string());
				let choose_btn = button(text(t!("choose_path"))).on_press(Message::OverlayMessage(OverlayMessage::Layout(
					LayoutModalMessage::ChooseExportPathPressed,
				)));
				let confirm_btn = button(text(t!("confirm_label"))).on_press(Message::OverlayMessage(
					OverlayMessage::Layout(LayoutModalMessage::ConfirmExport),
				));
				let cancel_btn = button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(OverlayMessage::Layout(
					LayoutModalMessage::CancelExport,
				)));
				let body = column![
					text(t!("export_layout_title")).color(palette.text),
					rule::horizontal(1),
					row![
						text(t!("path_colon")).color(palette.text_dim),
						text(path_label).color(palette.text)
					]
					.spacing(8)
					.align_y(Alignment::Center),
					row![choose_btn, cancel_btn, confirm_btn]
						.spacing(8)
						.align_y(Alignment::Center),
				]
				.spacing(12)
				.padding(12);
				let content = container(body)
					.width(Length::Fill)
					.height(Length::Shrink)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.panel_bg)),
						border: Border {
							color: palette.border,
							width: 1.0,
							radius: 10.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					});
				wrap_overlay(content.into(), palette)
			}
			LayoutModalKind::Import { .. } => {
				let confirm_btn = button(text(t!("apply_label"))).on_press(Message::OverlayMessage(OverlayMessage::Layout(
					LayoutModalMessage::ConfirmImport,
				)));
				let cancel_btn = button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(OverlayMessage::Layout(
					LayoutModalMessage::CancelImport,
				)));
				let body = column![
					text(t!("import_layout_title")).color(palette.text),
					rule::horizontal(1),
					text(t!("import_parsed_confirm")).color(palette.text_dim),
					row![cancel_btn, confirm_btn].spacing(8).align_y(Alignment::Center),
				]
				.spacing(12)
				.padding(12);
				let content = container(body)
					.width(Length::Fill)
					.height(Length::Shrink)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.panel_bg)),
						border: Border {
							color: palette.border,
							width: 1.0,
							radius: 10.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					});
				wrap_overlay(content.into(), palette)
			}
			LayoutModalKind::Reset => {
				let confirm_btn = button(text(t!("reset_label"))).on_press(Message::OverlayMessage(OverlayMessage::Layout(
					LayoutModalMessage::ConfirmReset,
				)));
				let cancel_btn = button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(OverlayMessage::Layout(
					LayoutModalMessage::CancelReset,
				)));
				let body = column![
					text(t!("reset_layout_title")).color(palette.text),
					rule::horizontal(1),
					text(t!("reset_description")).color(palette.text_dim),
					row![cancel_btn, confirm_btn].spacing(8).align_y(Alignment::Center),
				]
				.spacing(12)
				.padding(12);
				let content = container(body)
					.width(Length::Fill)
					.height(Length::Shrink)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.panel_bg)),
						border: Border {
							color: palette.border,
							width: 1.0,
							radius: 10.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					});
				wrap_overlay(content.into(), palette)
			}
			LayoutModalKind::Error { message } => {
				let dismiss_btn = button(text(t!("dismiss_label"))).on_press(Message::OverlayMessage(
					OverlayMessage::Layout(LayoutModalMessage::CancelError),
				));
				let body = column![
					text(t!("error_modal_title")).color(palette.text),
					rule::horizontal(1),
					text(message.clone()).color(palette.text_dim),
					row![dismiss_btn].spacing(8).align_y(Alignment::Center),
				]
				.spacing(12)
				.padding(12);
				let content = container(body)
					.width(Length::Fill)
					.height(Length::Shrink)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.panel_bg)),
						border: Border {
							color: palette.border,
							width: 1.0,
							radius: 10.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					});
				wrap_overlay(content.into(), palette)
			}
		}
	}
}
