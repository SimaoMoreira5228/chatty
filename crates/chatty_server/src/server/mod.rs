#![forbid(unsafe_code)]

pub mod adapter_manager;
pub mod audit;
pub mod auth;
pub mod connection;
pub mod health;
pub mod replay;
pub mod room_hub;
pub mod router;
pub mod state;

#[cfg(test)]
mod adapter_manager_tests;

#[cfg(test)]
mod quic_demo_adapter_tests;

#[cfg(test)]
mod room_hub_tests;
