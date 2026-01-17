#![forbid(unsafe_code)]

pub mod chat_input;
pub mod join_dialog;
pub mod message_list;
pub mod rename_dialog;
pub mod resize_handle;
pub mod split_controls;
pub mod split_header;
pub mod split_messages;
pub mod split_resize;
pub mod split_sizing;
pub mod tab_strip;
pub mod topbar;

pub use chat_input::render_chat_input;
pub use join_dialog::open_join_dialog;
pub use rename_dialog::open_rename_dialog;
pub use resize_handle::render_resize_handle;
pub use split_controls::render_split_controls;
pub use split_header::render_split_header;
pub use split_messages::render_split_messages;
pub use split_resize::{ResizeDrag, begin_resize_drag, default_min_split_width, end_resize_drag, update_resize_drag};
pub use split_sizing::{ensure_split_proportions, rebalance_split_proportions, split_content_width};
pub use tab_strip::{TabItem, render_tab_strip};
pub use topbar::{StatusChip, TopbarButton, render_topbar};
