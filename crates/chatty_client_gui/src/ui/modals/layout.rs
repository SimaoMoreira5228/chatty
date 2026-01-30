use std::path::PathBuf;

use iced::widget::{button, column, container, row, rule, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Task};
use rust_i18n::t;

use crate::app::{Chatty, Message};
use crate::theme;
use crate::ui::layout::UiRootState;

#[derive(Debug, Clone)]
pub enum LayoutModalMessage {
	ConfirmImport,
	CancelImport,
	ConfirmExport,
	CancelExport,
	ConfirmReset,
	CancelReset,
	CancelError,
	ChooseExportPathResult(Option<PathBuf>),
	ChooseExportPathPressed,
	LayoutImportFileParsed(Result<UiRootState, String>),
}

#[derive(Debug, Clone)]
pub enum LayoutModalKind {
	Export {
		root: UiRootState,
		path: Option<PathBuf>,
	},
	Import {
		root: UiRootState,
	},
	Reset,
	Error {
		message: String,
	},
}

#[derive(Debug, Clone)]
pub struct LayoutModal {
	pub kind: LayoutModalKind,
}

impl LayoutModal {
	pub fn new_export(root: UiRootState, path: Option<PathBuf>) -> Self {
		Self {
			kind: LayoutModalKind::Export { root, path },
		}
	}

	pub fn new_import(root: UiRootState) -> Self {
		Self {
			kind: LayoutModalKind::Import { root },
		}
	}

	pub fn new_reset() -> Self {
		Self {
			kind: LayoutModalKind::Reset,
		}
	}

	pub fn new_import_empty() -> Self {
		Self {
			kind: LayoutModalKind::Import {
				root: UiRootState::default(),
			},
		}
	}

	pub fn new_error(message: String) -> Self {
		Self {
			kind: LayoutModalKind::Error { message },
		}
	}

	pub fn update(&mut self, app: &mut Chatty, message: LayoutModalMessage) -> Task<Message> {
		match message {
			LayoutModalMessage::ConfirmImport => app.update_confirm_import(),
			LayoutModalMessage::CancelImport => app.update_cancel_import(),
			LayoutModalMessage::ConfirmExport => app.update_confirm_export(),
			LayoutModalMessage::CancelExport => app.update_cancel_export(),
			LayoutModalMessage::ConfirmReset => app.update_confirm_reset(),
			LayoutModalMessage::CancelReset => app.update_cancel_reset(),
			LayoutModalMessage::CancelError => app.update_cancel_error(),
			LayoutModalMessage::ChooseExportPathPressed => app.update_choose_export_path_pressed(),
			LayoutModalMessage::ChooseExportPathResult(opt) => app.update_layout_export_path_chosen(opt),
			LayoutModalMessage::LayoutImportFileParsed(res) => app.update_layout_import_file_parsed(res),
		}
	}

	pub fn view<'a>(&'a self, _app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		let inner = match &self.kind {
			LayoutModalKind::Export { path, .. } => {
				let path_s = path
					.as_ref()
					.map(|p| p.display().to_string())
					.unwrap_or_else(|| t!("settings.no_path_chosen").to_string());
				container(column![
					text(t!("export_layout_title")).color(palette.text).size(16),
					rule::horizontal(1),
					text(format!("{} {}", t!("path_colon"), path_s)).color(palette.text_dim),
					row![
						button(text(t!("choose_path"))).on_press(Message::OverlayMessage(
							crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::ChooseExportPathPressed)
						)),
						if path.is_some() {
							button(text(t!("confirm_label"))).on_press(Message::OverlayMessage(
								crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::ConfirmExport),
							))
						} else {
							button(text(t!("confirm_label")).color(palette.text_muted))
						},
						button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(
							crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::CancelExport)
						)),
					]
					.spacing(8)
					.align_y(Alignment::Center),
				])
			}
			LayoutModalKind::Import { root } => {
				let tabs = root.tabs.len();
				fn leaf_count(node: &crate::ui::layout::UiNode) -> usize {
					match node {
						crate::ui::layout::UiNode::Leaf(_) => 1,
						crate::ui::layout::UiNode::Split { first, second, .. } => leaf_count(first) + leaf_count(second),
					}
				}
				let leaves: usize = root.tabs.iter().map(|t| leaf_count(&t.root)).sum();
				container(column![
					text(t!("import_layout_title")).color(palette.text).size(16),
					rule::horizontal(1),
					text(format!("{} {} leaves, {} tabs", t!("parsed_stats"), leaves, tabs)).color(palette.text_dim),
					row![
						button(text(t!("apply_label"))).on_press(Message::OverlayMessage(
							crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::ConfirmImport)
						)),
						button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(
							crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::CancelImport)
						))
					]
					.spacing(8)
					.align_y(Alignment::Center),
				])
			}
			LayoutModalKind::Reset => container(column![
				text(t!("reset_layout_title")).color(palette.text).size(16),
				rule::horizontal(1),
				text(t!("reset_description")).color(palette.text_dim),
				row![
					button(text(t!("reset_label"))).on_press(Message::OverlayMessage(
						crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::ConfirmReset)
					)),
					button(text(t!("cancel_label"))).on_press(Message::OverlayMessage(
						crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::CancelReset)
					))
				]
				.spacing(8)
				.align_y(Alignment::Center),
			]),
			LayoutModalKind::Error { message } => container(column![
				text(t!("error_modal_title")).color(palette.text).size(16),
				rule::horizontal(1),
				text(message.clone()).color(palette.text_dim),
				row![
					button(text(t!("retry_label"))).on_press(Message::ConnectPressed),
					button(text(t!("dismiss_label"))).on_press(Message::OverlayMessage(
						crate::ui::modals::OverlayMessage::Layout(LayoutModalMessage::CancelError)
					)),
				]
				.spacing(8)
				.align_y(Alignment::Center),
			]),
		};

		let inner = inner.padding(12).style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		});

		container(container(inner).center_x(Length::Fill).center_y(Length::Fill))
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			})
			.into()
	}
}
