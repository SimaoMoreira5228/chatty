use std::path::PathBuf;

use iced::Task;
use rust_i18n::t;

use crate::app::{Chatty, Message};
use crate::ui::modals::{LayoutModal, LayoutModalMessage};

impl Chatty {
	pub fn update_export_layout_pressed(&mut self) -> Task<Message> {
		let root = crate::ui::layout::UiRootState::from_app(self);
		self.state.ui.active_overlay = Some(crate::ui::modals::ActiveOverlay::Layout(LayoutModal::new_export(root, None)));
		Task::none()
	}

	pub fn update_reset_layout_pressed(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::ui::modals::ActiveOverlay::Layout(LayoutModal::new_reset()));
		Task::none()
	}

	pub fn update_confirm_export(&mut self) -> Task<Message> {
		if let Some(crate::ui::modals::ActiveOverlay::Layout(modal)) = &self.state.ui.active_overlay
			&& let crate::ui::modals::layout::LayoutModalKind::Export { root, path } = &modal.kind
			&& let Some(path) = path
		{
			let root = root.clone();
			let path = path.clone();
			self.state.ui.active_overlay = None;
			return Task::perform(
				async move {
					if let Ok(s) = serde_json::to_string_pretty(&root) {
						let _ = tokio::fs::write(path, s).await;
					}
				},
				|_| Message::AnimationTick(std::time::Instant::now()), // Dummy message to satisfy Task::perform
			);
		}
		Task::none()
	}

	pub fn update_cancel_export(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = None;
		Task::none()
	}

	pub fn update_confirm_reset(&mut self) -> Task<Message> {
		let root = crate::ui::layout::UiRootState::default();
		self.apply_ui_root(root.clone());
		crate::ui::layout::save_ui_layout(&root);
		self.state.ui.active_overlay = None;
		self.toast(t!("layout_reset").to_string())
	}

	pub fn update_cancel_reset(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = None;
		Task::none()
	}

	pub fn update_cancel_error(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = None;
		Task::none()
	}

	pub fn update_choose_export_path_pressed(&mut self) -> Task<Message> {
		Task::perform(
			async {
				rfd::AsyncFileDialog::new()
					.add_filter("JSON", &["json"])
					.set_file_name("ui_layout.json")
					.save_file()
					.await
					.map(|f| f.path().to_path_buf())
			},
			|res| {
				Message::OverlayMessage(crate::ui::modals::OverlayMessage::Layout(
					LayoutModalMessage::ChooseExportPathResult(res),
				))
			},
		)
	}

	pub fn update_layout_export_path_chosen(&mut self, path: Option<PathBuf>) -> Task<Message> {
		if let Some(crate::ui::modals::ActiveOverlay::Layout(modal)) = &mut self.state.ui.active_overlay
			&& let crate::ui::modals::layout::LayoutModalKind::Export { path: p, .. } = &mut modal.kind
		{
			*p = path;
		}
		Task::none()
	}

	pub fn update_import_layout_pressed(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = Some(crate::ui::modals::ActiveOverlay::Layout(LayoutModal::new_import_empty()));
		Task::none()
	}

	pub fn update_import_from_file_pressed(&mut self) -> Task<Message> {
		Task::perform(
			async {
				let path = rfd::AsyncFileDialog::new().add_filter("JSON", &["json"]).pick_file().await;
				if let Some(handle) = path {
					let bytes = handle.read().await;
					match serde_json::from_slice::<crate::ui::layout::UiRootState>(&bytes) {
						Ok(root) => Ok(root),
						Err(e) => Err(format!("failed to parse layout: {}", e)),
					}
				} else {
					Err("no file selected".to_string())
				}
			},
			|res| {
				Message::OverlayMessage(crate::ui::modals::OverlayMessage::Layout(
					LayoutModalMessage::LayoutImportFileParsed(res),
				))
			},
		)
	}

	pub fn update_layout_import_clipboard(&mut self, opt: Option<String>) -> Task<Message> {
		if let Some(txt) = opt {
			match serde_json::from_str::<crate::ui::layout::UiRootState>(&txt) {
				Ok(root) => {
					self.state.ui.active_overlay =
						Some(crate::ui::modals::ActiveOverlay::Layout(LayoutModal::new_import(root)));
					self.toast(t!("import_parsed_confirm").to_string())
				}
				Err(e) => self.toast(format!("{}: {}", t!("import_failed"), e)),
			}
		} else {
			self.toast(t!("clipboard_empty").to_string())
		}
	}

	pub fn update_layout_import_file_parsed(
		&mut self,
		res: Result<crate::ui::layout::UiRootState, String>,
	) -> Task<Message> {
		match res {
			Ok(root) => {
				self.state.ui.active_overlay = Some(crate::ui::modals::ActiveOverlay::Layout(LayoutModal::new_import(root)));
				self.toast(t!("import_parsed_confirm").to_string())
			}
			Err(e) => self.toast(format!("{}: {}", t!("import_failed"), e)),
		}
	}

	pub fn update_confirm_import(&mut self) -> Task<Message> {
		if let Some(crate::ui::modals::ActiveOverlay::Layout(modal)) = &self.state.ui.active_overlay
			&& let crate::ui::modals::layout::LayoutModalKind::Import { root } = &modal.kind
		{
			let root = root.clone();
			self.apply_ui_root(root.clone());
			crate::ui::layout::save_ui_layout(&root);
			self.state.ui.active_overlay = None;
			return self.toast(t!("imported_layout").to_string());
		}
		Task::none()
	}

	pub fn update_cancel_import(&mut self) -> Task<Message> {
		self.state.ui.active_overlay = None;
		Task::none()
	}

	pub fn update_modal_dismissed(&mut self) -> Task<Message> {
		let mut toast = Task::none();
		if let Some(crate::ui::modals::ActiveOverlay::Layout(modal)) = &self.state.ui.active_overlay
			&& let crate::ui::modals::layout::LayoutModalKind::Import { .. } = &modal.kind
		{
			toast = self.toast(t!("import_cancelled").to_string());
		}
		self.state.ui.active_overlay = None;
		toast
	}
}
