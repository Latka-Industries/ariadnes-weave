//! Decode print-IR images into PDF-ready sample streams.

use image::{ColorType, DynamicImage, GenericImageView, ImageFormat};
use miniz_oxide::deflate::{CompressionLevel, compress_to_vec_zlib};
use pdf_writer::Filter;

use crate::error::WeaveError;
use crate::ir::PrintImage;

/// Image samples ready for an Image XObject.
#[derive(Debug, Clone)]
pub struct PreparedImage {
    /// Encoded sample bytes (possibly filtered).
    pub samples: Vec<u8>,
    /// Optional soft-mask (alpha) samples.
    pub mask: Option<Vec<u8>>,
    /// PDF filter for [`Self::samples`] (and mask, when present).
    pub filter: Filter,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
}

impl PreparedImage {
    /// Display size in PDF points, fitting within `max_w` while preserving aspect.
    #[must_use]
    pub fn fit_width(&self, max_w: f32) -> (f32, f32) {
        let w = self.width as f32;
        let h = self.height as f32;
        if w <= max_w {
            (w, h)
        } else {
            let scale = max_w / w;
            (max_w, h * scale)
        }
    }
}

/// Prepare a [`PrintImage`] for embedding.
///
/// # Errors
///
/// Returns [`WeaveError::BadImage`] for unsupported formats or decode failures.
pub fn prepare_image(image: &PrintImage) -> Result<PreparedImage, WeaveError> {
    let format = guess_format(image)?;
    let dynamic =
        image::load_from_memory(&image.bytes).map_err(|e| WeaveError::BadImage(e.to_string()))?;

    match format {
        ImageFormat::Jpeg => prepare_jpeg(image.bytes.clone(), &dynamic),
        ImageFormat::Png => prepare_png(&dynamic),
        other => Err(WeaveError::BadImage(format!(
            "unsupported image format: {other:?}"
        ))),
    }
}

fn guess_format(image: &PrintImage) -> Result<ImageFormat, WeaveError> {
    if let Ok(fmt) = image::guess_format(&image.bytes) {
        return Ok(fmt);
    }
    match image.media_type.as_str() {
        "image/jpeg" | "image/jpg" => Ok(ImageFormat::Jpeg),
        "image/png" => Ok(ImageFormat::Png),
        other => Err(WeaveError::BadImage(format!(
            "cannot determine image format ({other})"
        ))),
    }
}

fn prepare_jpeg(data: Vec<u8>, dynamic: &DynamicImage) -> Result<PreparedImage, WeaveError> {
    // DCT path expects 8-bit RGB JPEG samples as-is.
    if dynamic.color() != ColorType::Rgb8 && dynamic.color() != ColorType::L8 {
        // Re-encode path: flatten to RGB PNG-style flate instead of failing hard.
        return prepare_png(dynamic);
    }
    Ok(PreparedImage {
        samples: data,
        mask: None,
        filter: Filter::DctDecode,
        width: dynamic.width(),
        height: dynamic.height(),
    })
}

fn prepare_png(dynamic: &DynamicImage) -> Result<PreparedImage, WeaveError> {
    let level = CompressionLevel::DefaultLevel as u8;
    let rgb = dynamic.to_rgb8();
    let samples = compress_to_vec_zlib(rgb.as_raw(), level);
    let mask = dynamic.color().has_alpha().then(|| {
        let alphas: Vec<u8> = dynamic.pixels().map(|p| p.2.0[3]).collect();
        compress_to_vec_zlib(&alphas, level)
    });
    Ok(PreparedImage {
        samples,
        mask,
        filter: Filter::FlateDecode,
        width: dynamic.width(),
        height: dynamic.height(),
    })
}
