#![forbid(unsafe_code)]

pub mod assets;
mod identities;
pub mod images;
mod message;
mod model;
mod net;
mod run;
mod split;
pub mod state;
mod subscription;
pub(crate) mod types;
mod update;

pub use message::Message;
pub use model::Chatty;
pub use run::run;
pub use types::{InsertTarget, Page, PendingCommand, PlatformChoice, ShortcutKeyChoice, SplitLayoutChoice, ThemeChoice};
