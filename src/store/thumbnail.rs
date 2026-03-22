use image::ImageFormat;
use std::io::Cursor;

/// Maximum dimension (width or height) for generated thumbnails.
pub const THUMBNAIL_MAX_DIM: u32 = 256;

#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    /// PNG bytes of the thumbnail.
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThumbnailError {
    UnsupportedFormat,
    DecodeError(String),
    EncodeError(String),
}

impl std::fmt::Display for ThumbnailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThumbnailError::UnsupportedFormat => write!(f, "Unsupported image format"),
            ThumbnailError::DecodeError(e) => write!(f, "Decode error: {e}"),
            ThumbnailError::EncodeError(e) => write!(f, "Encode error: {e}"),
        }
    }
}

/// Generate a thumbnail from raw image bytes.
///
/// - Detects format from bytes (tries PNG, JPEG, GIF, WebP in order).
/// - Resizes to fit within `THUMBNAIL_MAX_DIM × THUMBNAIL_MAX_DIM`, preserving aspect ratio.
/// - Returns PNG bytes.
pub fn generate(input: &[u8]) -> Result<Thumbnail, ThumbnailError> {
    let img = image::load_from_memory(input).map_err(|e| {
        // Distinguish truly unrecognised format from a decode failure.
        // `image` returns `UnsupportedError` when the magic bytes don't match
        // any registered format.
        let msg = e.to_string();
        if msg.contains("unsupported") || msg.contains("Unsupported") {
            ThumbnailError::UnsupportedFormat
        } else {
            ThumbnailError::DecodeError(msg)
        }
    })?;

    // Only downscale — never upscale images that already fit within the max dim.
    let (orig_w, orig_h) = (img.width(), img.height());
    let thumb = if orig_w <= THUMBNAIL_MAX_DIM && orig_h <= THUMBNAIL_MAX_DIM {
        img
    } else {
        img.thumbnail(THUMBNAIL_MAX_DIM, THUMBNAIL_MAX_DIM)
    };
    let width = thumb.width();
    let height = thumb.height();

    let mut cursor = Cursor::new(Vec::new());
    thumb
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| ThumbnailError::EncodeError(e.to_string()))?;

    Ok(Thumbnail {
        width,
        height,
        data: cursor.into_inner(),
    })
}

/// Generate a thumbnail from a file path (reads the file, calls `generate`).
pub fn generate_from_path(path: &std::path::Path) -> Result<Thumbnail, ThumbnailError> {
    let bytes =
        std::fs::read(path).map_err(|e| ThumbnailError::DecodeError(e.to_string()))?;
    generate(&bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer};

    fn make_png_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgba8(ImageBuffer::new(width, height));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn generate_returns_thumbnail_for_valid_png() {
        let bytes = make_png_bytes(100, 100);
        let result = generate(&bytes);
        assert!(result.is_ok());
        let thumb = result.unwrap();
        assert!(thumb.width > 0);
        assert!(thumb.height > 0);
        assert!(!thumb.data.is_empty());
    }

    #[test]
    fn thumbnail_fits_within_max_dim() {
        let bytes = make_png_bytes(1024, 768);
        let thumb = generate(&bytes).unwrap();
        assert!(thumb.width <= THUMBNAIL_MAX_DIM);
        assert!(thumb.height <= THUMBNAIL_MAX_DIM);
    }

    #[test]
    fn thumbnail_preserves_aspect_ratio_landscape() {
        // 800 × 400 → 2:1 ratio; thumbnail should remain 2:1.
        let bytes = make_png_bytes(800, 400);
        let thumb = generate(&bytes).unwrap();
        // width == THUMBNAIL_MAX_DIM (256), height should be 128.
        assert_eq!(thumb.width, 256);
        assert_eq!(thumb.height, 128);
    }

    #[test]
    fn thumbnail_preserves_aspect_ratio_portrait() {
        // 400 × 800 → 1:2 ratio; thumbnail should remain 1:2.
        let bytes = make_png_bytes(400, 800);
        let thumb = generate(&bytes).unwrap();
        // height == THUMBNAIL_MAX_DIM (256), width should be 128.
        assert_eq!(thumb.width, 128);
        assert_eq!(thumb.height, 256);
    }

    #[test]
    fn generate_returns_error_for_invalid_bytes() {
        let garbage = b"this is not an image at all!!";
        let result = generate(garbage);
        assert!(result.is_err());
        // Must be either UnsupportedFormat or DecodeError — not a panic.
        match result.unwrap_err() {
            ThumbnailError::UnsupportedFormat | ThumbnailError::DecodeError(_) => {}
            other => panic!("unexpected error variant: {:?}", other),
        }
    }

    #[test]
    fn small_image_not_upscaled() {
        // 50 × 30 is already within 256 × 256, so `thumbnail()` must not upscale it.
        let bytes = make_png_bytes(50, 30);
        let thumb = generate(&bytes).unwrap();
        assert_eq!(thumb.width, 50);
        assert_eq!(thumb.height, 30);
    }
}
