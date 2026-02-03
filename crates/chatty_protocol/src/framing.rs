#![forbid(unsafe_code)]

use bytes::{BufMut, BytesMut};
use prost::Message;
use thiserror::Error;

/// Default maximum frame payload size for v1.
pub const DEFAULT_MAX_FRAME_SIZE: usize = 2 * 1024 * 1024; // 2 MiB

#[derive(Debug, Error)]
pub enum FramingError {
	#[error("frame exceeds maximum size: len={len} max={max}")]
	FrameTooLarge {
		len: usize,
		max: usize,
	},

	#[error("insufficient data: need={need} have={have}")]
	InsufficientData {
		need: usize,
		have: usize,
	},

	#[error("protobuf decode error: {0}")]
	Decode(#[from] prost::DecodeError),

	#[error("protobuf encode error: {0}")]
	Encode(#[from] prost::EncodeError),
}

/// Encode a protobuf message into a length-prefixed frame.
pub fn encode_frame<M: Message>(msg: &M, max_frame_size: usize) -> Result<Vec<u8>, FramingError> {
	let payload_len = msg.encoded_len();
	if payload_len > max_frame_size {
		return Err(FramingError::FrameTooLarge {
			len: payload_len,
			max: max_frame_size,
		});
	}

	let mut out = Vec::with_capacity(4 + payload_len);
	out.extend_from_slice(&(payload_len as u32).to_be_bytes());
	msg.encode(&mut out)?;
	Ok(out)
}

/// Encode a frame using `DEFAULT_MAX_FRAME_SIZE`.
pub fn encode_frame_default<M: Message>(msg: &M) -> Result<Vec<u8>, FramingError> {
	encode_frame(msg, DEFAULT_MAX_FRAME_SIZE)
}

/// Append an encoded frame into the provided buffer.
pub fn encode_frame_into<M: Message>(buf: &mut BytesMut, msg: &M, max_frame_size: usize) -> Result<(), FramingError> {
	let payload_len = msg.encoded_len();
	if payload_len > max_frame_size {
		return Err(FramingError::FrameTooLarge {
			len: payload_len,
			max: max_frame_size,
		});
	}

	buf.reserve(4 + payload_len);
	buf.put_u32(payload_len as u32);
	msg.encode(buf)?;
	Ok(())
}

/// Compute total frame length (prefix + payload).
#[inline]
pub fn frame_len_from_payload_len(payload_len: usize) -> usize {
	4 + payload_len
}

/// Decode a single frame from the start of `src`.
pub fn decode_frame<M: Message + Default>(src: &[u8], max_frame_size: usize) -> Result<(M, usize), FramingError> {
	if src.len() < 4 {
		return Err(FramingError::InsufficientData {
			need: 4,
			have: src.len(),
		});
	}

	let len = u32::from_be_bytes([src[0], src[1], src[2], src[3]]) as usize;
	if len > max_frame_size {
		return Err(FramingError::FrameTooLarge {
			len,
			max: max_frame_size,
		});
	}

	let need = 4 + len;
	if src.len() < need {
		return Err(FramingError::InsufficientData { need, have: src.len() });
	}

	let msg = M::decode(&src[4..4 + len])?;
	Ok((msg, need))
}

/// Try to decode a single frame from a growable buffer.
pub fn try_decode_frame_from_buffer<M: Message + Default>(
	buf: &mut BytesMut,
	max_frame_size: usize,
) -> Result<Option<M>, FramingError> {
	if buf.len() < 4 {
		return Ok(None);
	}

	let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
	if len > max_frame_size {
		return Err(FramingError::FrameTooLarge {
			len,
			max: max_frame_size,
		});
	}

	let need = 4 + len;
	if buf.len() < need {
		return Ok(None);
	}

	let frame = buf.split_to(need);
	let msg = M::decode(&frame[4..])?;
	Ok(Some(msg))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(Clone, PartialEq, ::prost::Message)]
	struct TestMsg {
		#[prost(string, tag = "1")]
		s: String,
		#[prost(uint32, tag = "2")]
		n: u32,
	}

	#[test]
	fn encode_decode_roundtrip_slice() {
		let msg = TestMsg {
			s: "hello".to_string(),
			n: 42,
		};

		let frame = encode_frame_default(&msg).expect("encode");
		let (decoded, consumed) = decode_frame::<TestMsg>(&frame, DEFAULT_MAX_FRAME_SIZE).expect("decode");
		assert_eq!(consumed, frame.len());
		assert_eq!(decoded, msg);
	}

	#[test]
	fn decode_requires_full_frame() {
		let msg = TestMsg { s: "x".repeat(10), n: 7 };
		let frame = encode_frame_default(&msg).expect("encode");

		let err = decode_frame::<TestMsg>(&frame[..4], DEFAULT_MAX_FRAME_SIZE).unwrap_err();
		match err {
			FramingError::InsufficientData { need, have } => {
				assert!(need > have);
			}
			other => panic!("unexpected error: {other:?}"),
		}
	}

	#[test]
	fn try_decode_from_buffer_incremental() {
		let msg = TestMsg {
			s: "hello".to_string(),
			n: 99,
		};
		let frame = encode_frame_default(&msg).expect("encode");

		let mut buf = BytesMut::new();

		buf.extend_from_slice(&frame[..2]);
		assert!(
			try_decode_frame_from_buffer::<TestMsg>(&mut buf, DEFAULT_MAX_FRAME_SIZE)
				.expect("ok")
				.is_none()
		);

		buf.extend_from_slice(&frame[2..8]);
		assert!(
			try_decode_frame_from_buffer::<TestMsg>(&mut buf, DEFAULT_MAX_FRAME_SIZE)
				.expect("ok")
				.is_none()
		);

		buf.extend_from_slice(&frame[8..]);
		let decoded = try_decode_frame_from_buffer::<TestMsg>(&mut buf, DEFAULT_MAX_FRAME_SIZE)
			.expect("ok")
			.expect("some");
		assert_eq!(decoded, msg);
		assert!(buf.is_empty());
	}

	#[test]
	fn encode_rejects_too_large() {
		let msg = TestMsg {
			s: "a".repeat(10_000),
			n: 1,
		};

		let err = encode_frame(&msg, 32).unwrap_err();
		match err {
			FramingError::FrameTooLarge { len, max } => {
				assert!(len > max);
			}
			other => panic!("unexpected error: {other:?}"),
		}
	}

	#[test]
	fn decode_rejects_too_large_prefix() {
		let mut buf = BytesMut::new();
		buf.extend_from_slice(&(DEFAULT_MAX_FRAME_SIZE as u32 + 1).to_be_bytes());

		let err = try_decode_frame_from_buffer::<TestMsg>(&mut buf, DEFAULT_MAX_FRAME_SIZE).unwrap_err();
		match err {
			FramingError::FrameTooLarge { .. } => {}
			other => panic!("unexpected error: {other:?}"),
		}
	}
}
