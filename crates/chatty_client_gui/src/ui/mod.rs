#![forbid(unsafe_code)]

pub mod app_state;
pub mod badges;
pub mod components;
pub mod main_window;
pub mod net;
pub mod pages;
pub mod reducer;
pub mod settings;
pub mod theme;

pub use main_window::open_all_windows;
