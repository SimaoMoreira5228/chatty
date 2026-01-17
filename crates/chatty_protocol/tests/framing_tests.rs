use bytes::BytesMut;
use chatty_protocol::{
	DEFAULT_MAX_FRAME_SIZE, FramingError, decode_frame, encode_frame, encode_frame_default, encode_frame_into,
	frame_len_from_payload_len, try_decode_frame_from_buffer,
};
use prost::Message;

/// Minimal prost message for integration testing
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

	let frame = encode_frame(&msg, DEFAULT_MAX_FRAME_SIZE).expect("encode_frame");
	let (decoded, consumed) = decode_frame::<TestMsg>(&frame, DEFAULT_MAX_FRAME_SIZE).expect("decode_frame");

	assert_eq!(consumed, frame.len());
	assert_eq!(decoded, msg);
}

#[test]
fn encode_frame_default_matches_explicit_default_limit() {
	let msg = TestMsg {
		s: "abc".to_string(),
		n: 7,
	};

	let a = encode_frame_default(&msg).expect("encode_frame_default");
	let b = encode_frame(&msg, DEFAULT_MAX_FRAME_SIZE).expect("encode_frame");

	assert_eq!(a, b);
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
fn encode_into_appends_and_respects_existing_data() {
	let msg1 = TestMsg {
		s: "one".to_string(),
		n: 1,
	};
	let msg2 = TestMsg {
		s: "two".to_string(),
		n: 2,
	};

	let mut buf = BytesMut::new();
	buf.extend_from_slice(b"prefix-");

	encode_frame_into(&mut buf, &msg1, DEFAULT_MAX_FRAME_SIZE).expect("encode_frame_into msg1");
	encode_frame_into(&mut buf, &msg2, DEFAULT_MAX_FRAME_SIZE).expect("encode_frame_into msg2");

	let total = buf.to_vec();
	let framed = &total[b"prefix-".len()..];

	let (d1, used1) = decode_frame::<TestMsg>(framed, DEFAULT_MAX_FRAME_SIZE).expect("decode msg1");
	assert_eq!(d1, msg1);

	let (d2, used2) = decode_frame::<TestMsg>(&framed[used1..], DEFAULT_MAX_FRAME_SIZE).expect("decode msg2");
	assert_eq!(d2, msg2);

	assert_eq!(used1 + used2, framed.len());
}

#[test]
fn frame_len_helper_is_correct() {
	let msg = TestMsg {
		s: "hello".to_string(),
		n: 123,
	};

	let payload_len = msg.encoded_len();
	let frame = encode_frame_default(&msg).expect("encode");

	assert_eq!(frame_len_from_payload_len(payload_len), frame.len());
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
