#![allow(dead_code)]
use std::io::Cursor;

use image::{imageops::FilterType, DynamicImage, ImageFormat};

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CropState {
    /// Original image dimensions.
    pub original_width: u32,
    pub original_height: u32,
    /// Crop circle center, as fraction of image dimensions (0.0–1.0).
    pub center_x: f32,
    pub center_y: f32,
    /// Crop circle radius, as fraction of min(width, height) (0.0–0.5).
    pub radius: f32,
    /// Zoom factor (1.0 = no zoom, >1.0 = zoomed in).
    pub zoom: f32,
}

impl CropState {
    /// Create a new crop state centered on the image with default radius = 0.4.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            original_width: width,
            original_height: height,
            center_x: 0.5,
            center_y: 0.5,
            radius: 0.4,
            zoom: 1.0,
        }
    }

    /// Pan: move center by (dx, dy) in image-fraction units. Clamps to valid range.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.center_x = (self.center_x + dx).clamp(0.0, 1.0);
        self.center_y = (self.center_y + dy).clamp(0.0, 1.0);
    }

    /// Set zoom level. Clamps to [1.0, 4.0].
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(1.0, 4.0);
    }

    /// Set radius. Clamps to [0.1, 0.5].
    pub fn set_radius(&mut self, radius: f32) {
        self.radius = radius.clamp(0.1, 0.5);
    }

    /// Compute the crop rectangle in pixel space.
    ///
    /// Returns `(left, top, side_length)` for a square crop.
    pub fn crop_rect(&self) -> (u32, u32, u32) {
        let min_dim = self.original_width.min(self.original_height) as f32;

        let center_px = self.center_x * self.original_width as f32;
        let center_py = self.center_y * self.original_height as f32;

        let radius_px = self.radius * min_dim * self.zoom;
        let side = (radius_px * 2.0).round() as u32;

        // Clamp so the crop rect stays inside the image.
        let max_left = self.original_width.saturating_sub(side);
        let max_top = self.original_height.saturating_sub(side);

        let left = ((center_px - radius_px).max(0.0) as u32).min(max_left);
        let top = ((center_py - radius_px).max(0.0) as u32).min(max_top);

        (left, top, side)
    }
}

// ---------------------------------------------------------------------------
// Crop + encode
// ---------------------------------------------------------------------------

/// Crop and resize an image to a square avatar.
///
/// - Decodes `input` bytes (any supported format).
/// - Applies the crop described by `state`.
/// - Resizes the result to `output_size × output_size` pixels.
/// - Returns PNG bytes.
pub fn crop_to_avatar(
    input: &[u8],
    state: &CropState,
    output_size: u32,
) -> Result<Vec<u8>, String> {
    let img: DynamicImage =
        image::load_from_memory(input).map_err(|e| format!("decode error: {e}"))?;

    let (x, y, size) = state.crop_rect();

    // Guard against a zero-size crop (degenerate state).
    if size == 0 {
        return Err("crop_rect produced a zero-size region".into());
    }

    let cropped = img.crop_imm(x, y, size, size);
    let resized = cropped.resize_exact(output_size, output_size, FilterType::Lanczos3);

    let mut buf = Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| format!("encode error: {e}"))?;

    Ok(buf.into_inner())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal PNG of the given dimensions in memory.
    fn make_png(w: u32, h: u32) -> Vec<u8> {
        use image::{DynamicImage, ImageBuffer};
        let img = DynamicImage::ImageRgba8(ImageBuffer::new(w, h));
        let mut buf = std::io::Cursor::new(vec![]);
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    // 1. new() centers on the image
    #[test]
    fn new_centers_on_image() {
        let state = CropState::new(200, 100);
        assert_eq!(state.center_x, 0.5);
        assert_eq!(state.center_y, 0.5);
        assert_eq!(state.radius, 0.4);
        assert_eq!(state.zoom, 1.0);
        assert_eq!(state.original_width, 200);
        assert_eq!(state.original_height, 100);
    }

    // 2. pan clamps to [0.0, 1.0]
    #[test]
    fn pan_clamps_to_valid_range() {
        let mut state = CropState::new(100, 100);
        // Pan past the right/bottom edge.
        state.pan(10.0, 10.0);
        assert_eq!(state.center_x, 1.0);
        assert_eq!(state.center_y, 1.0);
        // Pan past the left/top edge.
        state.pan(-20.0, -20.0);
        assert_eq!(state.center_x, 0.0);
        assert_eq!(state.center_y, 0.0);
    }

    // 3. set_zoom clamps to [1.0, 4.0]
    #[test]
    fn set_zoom_clamps_to_4() {
        let mut state = CropState::new(100, 100);
        state.set_zoom(10.0);
        assert_eq!(state.zoom, 4.0);
        state.set_zoom(0.0);
        assert_eq!(state.zoom, 1.0);
    }

    // 4. set_radius clamps to [0.1, 0.5]
    #[test]
    fn set_radius_clamps_min() {
        let mut state = CropState::new(100, 100);
        state.set_radius(0.0);
        assert_eq!(state.radius, 0.1);
        state.set_radius(1.0);
        assert_eq!(state.radius, 0.5);
    }

    // 5. crop_rect stays within image bounds
    #[test]
    fn crop_rect_returns_valid_bounds() {
        let state = CropState::new(200, 150);
        let (x, y, size) = state.crop_rect();

        assert!(x + size <= state.original_width, "crop exceeds image width");
        assert!(
            y + size <= state.original_height,
            "crop exceeds image height"
        );
        assert!(size > 0, "crop size must be positive");
    }

    // 6. crop_to_avatar produces a valid PNG at the requested output size
    #[test]
    fn crop_to_avatar_produces_output_size_png() {
        let png_bytes = make_png(200, 200);
        let state = CropState::new(200, 200);
        let result = crop_to_avatar(&png_bytes, &state, 128).expect("crop_to_avatar failed");

        // Verify the output is a valid PNG and its dimensions are correct.
        let decoded = image::load_from_memory(&result).expect("output is not a valid PNG");
        assert_eq!(decoded.width(), 128);
        assert_eq!(decoded.height(), 128);
    }
}
