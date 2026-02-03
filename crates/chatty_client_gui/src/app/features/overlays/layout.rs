#![forbid(unsafe_code)]

use std::path::PathBuf;

use iced::Task;

use crate::app::features::layout::UiRootState;
use crate::app::message::Message;
use crate::app::model::Chatty;

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
}
