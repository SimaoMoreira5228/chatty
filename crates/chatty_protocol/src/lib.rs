#![forbid(unsafe_code)]

pub mod framing;

pub use framing::{
	DEFAULT_MAX_FRAME_SIZE, FramingError, decode_frame, encode_frame, encode_frame_default, encode_frame_into,
	frame_len_from_payload_len, try_decode_frame_from_buffer,
};

/// Generated protobuf types (`chatty.v1`).
#[allow(clippy::large_enum_variant)]
pub mod pb {
	include!(concat!(env!("OUT_DIR"), "/chatty.v1.rs"));
}

/// Protocol version constants.
pub mod version {
	/// Current protocol major version (v1).
	pub const PROTOCOL_MAJOR: u32 = 1;
	/// Current protocol minor version.
	pub const PROTOCOL_MINOR: u32 = 0;

	/// Compact representation useful for logs/metrics.
	pub const PROTOCOL_VERSION_U32: u32 = (PROTOCOL_MAJOR << 16) | PROTOCOL_MINOR;
}
