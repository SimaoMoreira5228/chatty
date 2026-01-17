#![forbid(unsafe_code)]

use gpui::{MouseDownEvent, MouseMoveEvent, Pixels, px};

#[derive(Debug, Clone, Copy)]
pub struct ResizeDrag {
	pub handle_index: usize,
	pub start_x: Pixels,
	pub left_proportion: f32,
	pub right_proportion: f32,
	pub container_width: Pixels,
}

pub fn begin_resize_drag(
	handle_index: usize,
	ev: &MouseDownEvent,
	proportions: &[f32],
	resize_drag: &mut Option<ResizeDrag>,
	container_width: Pixels,
) -> bool {
	if handle_index + 1 >= proportions.len() {
		return false;
	}
	let left_proportion = proportions[handle_index];
	let right_proportion = proportions[handle_index + 1];

	*resize_drag = Some(ResizeDrag {
		handle_index,
		start_x: ev.position.x,
		left_proportion,
		right_proportion,
		container_width,
	});
	true
}

pub fn update_resize_drag(
	ev: &MouseMoveEvent,
	proportions: &mut [f32],
	resize_drag: &Option<ResizeDrag>,
	min_width: Pixels,
) -> bool {
	let Some(drag) = resize_drag else {
		return false;
	};
	if !ev.dragging() {
		return false;
	}
	if f32::from(drag.container_width) <= 0.0 {
		return false;
	}

	let total_prop = drag.left_proportion + drag.right_proportion;
	let pixel_delta = ev.position.x - drag.start_x;
	let prop_delta = pixel_delta / drag.container_width;

	let min_prop = min_width / drag.container_width;

	let next_left = (drag.left_proportion + prop_delta).clamp(min_prop, total_prop - min_prop);
	let next_right = total_prop - next_left;

	if let Some(prop) = proportions.get_mut(drag.handle_index) {
		*prop = next_left;
	}
	if let Some(prop) = proportions.get_mut(drag.handle_index + 1) {
		*prop = next_right;
	}
	true
}

pub fn end_resize_drag(resize_drag: &mut Option<ResizeDrag>) -> bool {
	resize_drag.take().is_some()
}

pub fn default_min_split_width() -> Pixels {
	px(220.0)
}
