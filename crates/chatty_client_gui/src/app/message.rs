use std::time::Instant;

use chatty_domain::RoomKey;
use iced::keyboard;
use iced::widget::{pane_grid, text_editor};

use crate::app::state::TabId;
use crate::app::types::Page;
use crate::ui::components::chat_pane::ChatPaneMessage;
use crate::ui::settings::SettingsMessage;
use crate::ui::users_view::UsersViewMessage;

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Message {
	Navigate(Page),
	ToasterMessage(crate::ui::components::toaster::ToasterMessage),
	OverlayMessage(crate::ui::modals::OverlayMessage),
	UsersViewMessage(UsersViewMessage),
	CursorMoved(f32, f32),
	UserScrolled,
	AnimationTick(Instant),

	CharPressed(char, keyboard::Modifiers),
	NamedKeyPressed(iced::keyboard::key::Named),

	ModalDismissed,
	ConnectPressed,
	DisconnectPressed,
	ConnectFinished(Result<(), String>),

	PaneMessage(pane_grid::Pane, ChatPaneMessage),
	SettingsMessage(SettingsMessage),
	PaneSubscribed(pane_grid::Pane, Result<(), String>),
	TabUnsubscribed(RoomKey, Result<(), String>),
	Sent(Result<(), String>),
	MessageTextEdit(String, text_editor::Action),

	MessageActionButtonPressed(chatty_domain::RoomKey, Option<String>, Option<String>, Option<String>),
	ReplyToMessage(chatty_domain::RoomKey, Option<String>, Option<String>),
	DeleteMessage(chatty_domain::RoomKey, Option<String>, Option<String>),
	TimeoutUser(chatty_domain::RoomKey, String),
	BanUser(chatty_domain::RoomKey, String),

	ClipboardRead(crate::app::types::ClipboardTarget, Option<String>),

	PaneClicked(pane_grid::Pane),
	PaneResized(pane_grid::ResizeEvent),
	PaneDragged(pane_grid::DragEvent),
	SplitSpiral,
	SplitMasonry,
	SplitPressed,
	CloseFocused,
	DismissToast,
	ModifiersChanged(keyboard::Modifiers),

	NetPolled(Option<crate::net::UiEvent>),
	AutoJoinCompleted(Vec<(RoomKey, Result<(), String>)>),
	NavigatePaneLeft,
	NavigatePaneDown,
	NavigatePaneUp,
	NavigatePaneRight,
	TabSelected(TabId),
	AddTabPressed,
	CloseTabPressed(TabId),
	PopTab(TabId),
	WindowClosed(iced::window::Id),
	WindowOpened(iced::window::Id),
	WindowResized(iced::window::Id, u32, u32),
	WindowMoved(iced::window::Id, i32, i32),
}
