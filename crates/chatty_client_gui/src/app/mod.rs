#![forbid(unsafe_code)]

mod identities;
mod model;
mod net;
mod run;
mod split;
mod subscription;
mod update;

pub use model::{
	Chatty, ClipboardTarget, InsertTarget, Message, Page, PaneState, PendingCommand, PlatformChoice, SettingsCategory,
	ShortcutKeyChoice, SplitLayoutChoice, ThemeChoice,
};
pub use run::run;
