#![forbid(unsafe_code)]

use gpui::{Bounds, Pixels, px};

pub fn ensure_split_proportions(proportions: &mut Vec<f32>, splits_len: usize) {
	if proportions.is_empty() || proportions.len() != splits_len {
		*proportions = rebalance_split_proportions(splits_len);
	}
}

pub fn rebalance_split_proportions(splits_len: usize) -> Vec<f32> {
	if splits_len == 0 {
		return Vec::new();
	}
	let count = splits_len as f32;
	vec![1.0 / count; splits_len]
}

pub fn split_content_width(
	index: usize,
	proportions: &[f32],
	split_bounds: Option<Bounds<Pixels>>,
	splits_len: usize,
) -> Pixels {
	let proportion = proportions
		.get(index)
		.cloned()
		.unwrap_or_else(|| if splits_len > 0 { 1.0 / splits_len as f32 } else { 1.0 });

	if let Some(bounds) = split_bounds {
		let total_width = bounds.size.width;
		let handles_width = px((splits_len.saturating_sub(1) * 4) as f32);
		let available_width = (total_width - handles_width).max(px(0.0));
		return available_width * proportion;
	}

	px(900.0) * proportion
}
