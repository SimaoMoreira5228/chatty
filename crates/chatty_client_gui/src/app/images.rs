use std::time::Duration;

use iced::widget::image::Handle as ImageHandle;
#[cfg(not(test))]
use image::AnimationDecoder;
#[cfg(not(test))]
use image::codecs::gif::GifDecoder;
#[cfg(not(test))]
use image::codecs::webp::WebPDecoder;

#[derive(Debug, Clone)]
pub struct AnimatedImage {
	pub frames: Vec<ImageHandle>,
	pub delays: Vec<Duration>,
	pub total: Duration,
}

impl AnimatedImage {
	pub fn frame_at(&self, elapsed: Duration) -> Option<&ImageHandle> {
		if self.frames.is_empty() {
			return None;
		}

		let total_ms = self.total.as_millis() as u64;
		let mut t_ms = if total_ms == 0 {
			0
		} else {
			(elapsed.as_millis() as u64) % total_ms
		};

		for (idx, delay) in self.delays.iter().enumerate() {
			let delay_ms = delay.as_millis() as u64;
			if t_ms <= delay_ms {
				return self.frames.get(idx);
			}
			t_ms = t_ms.saturating_sub(delay_ms);
		}

		self.frames.last()
	}
}

#[allow(dead_code)]
pub fn decode_animated_image(bytes: &[u8]) -> Option<AnimatedImage> {
	#[cfg(not(test))]
	{
		if bytes.len() < 12 {
			return None;
		}

		let is_gif = bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a");
		let is_webp = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");

		if is_gif {
			return decode_gif(bytes);
		}
		if is_webp {
			return decode_webp(bytes);
		}

		None
	}

	#[cfg(test)]
	{
		let _ = bytes;
		None
	}
}

#[cfg(not(test))]
fn decode_gif(bytes: &[u8]) -> Option<AnimatedImage> {
	use std::io::Cursor;

	let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
	let frames = decoder.into_frames().collect_frames().ok()?;
	if frames.len() < 2 {
		return None;
	}

	let mut handles = Vec::with_capacity(frames.len());
	let mut delays = Vec::with_capacity(frames.len());
	let mut total = Duration::ZERO;

	for frame in frames {
		let delay = frame.delay();
		let ms = delay.numer_denom_ms().0.max(20u32);
		let d = Duration::from_millis(ms as u64);
		total += d;
		delays.push(d);
		let buffer = frame.into_buffer();
		let width = buffer.width();
		let height = buffer.height();
		handles.push(ImageHandle::from_rgba(width, height, buffer.into_raw()));
	}

	Some(AnimatedImage {
		frames: handles,
		delays,
		total,
	})
}

#[cfg(not(test))]
fn decode_webp(bytes: &[u8]) -> Option<AnimatedImage> {
	use std::io::Cursor;

	let decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
	let frames = decoder.into_frames().collect_frames().ok()?;
	if frames.len() < 2 {
		return None;
	}

	let mut handles = Vec::with_capacity(frames.len());
	let mut delays = Vec::with_capacity(frames.len());
	let mut total = Duration::ZERO;

	for frame in frames {
		let delay = frame.delay();
		let ms = delay.numer_denom_ms().0.max(20u32);
		let d = Duration::from_millis(ms as u64);
		total += d;
		delays.push(d);
		let buffer = frame.into_buffer();
		let width = buffer.width();
		let height = buffer.height();
		handles.push(ImageHandle::from_rgba(width, height, buffer.into_raw()));
	}

	Some(AnimatedImage {
		frames: handles,
		delays,
		total,
	})
}
