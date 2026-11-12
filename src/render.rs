//! Turns a `KeyFace` into a 72x72 image: background color or icon, then centered text.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use anyhow::{Context, Result};
use image::{DynamicImage, Rgb, RgbImage, imageops::FilterType};

use crate::screen::KeyFace;

/// Pixel size of one key on the MK.2.
pub const KEY_SIZE: u32 = 72;

/// Bundled in the binary so rendering does not depend on installed fonts.
const FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
const MAX_TEXT_SIZE: f32 = 22.0;
const MIN_TEXT_SIZE: f32 = 8.0;
/// Space kept free on each side of the text.
const MARGIN: f32 = 3.0;

pub fn render(face: &KeyFace) -> Result<DynamicImage> {
    let mut image = match (&face.icon, face.color) {
        (Some(path), _) => image::open(path)
            .with_context(|| format!("cannot open icon {path}"))?
            .resize_to_fill(KEY_SIZE, KEY_SIZE, FilterType::Triangle)
            .to_rgb8(),
        (None, Some(rgb)) => RgbImage::from_pixel(KEY_SIZE, KEY_SIZE, Rgb(rgb)),
        (None, None) => RgbImage::new(KEY_SIZE, KEY_SIZE), // black
    };
    if let Some(text) = &face.text {
        draw_text(&mut image, text);
    }
    Ok(DynamicImage::ImageRgb8(image))
}

/// Draws white text centered on the key, shrinking it until it fits the width.
// Pixel math mixes f32 font coordinates with u32 image coordinates; values are tiny (< 100).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn draw_text(image: &mut RgbImage, text: &str) {
    let font = FontRef::try_from_slice(FONT).expect("bundled font is valid");
    let key = KEY_SIZE as f32;

    let mut size = MAX_TEXT_SIZE;
    while size > MIN_TEXT_SIZE && text_width(&font, size, text) > key - 2.0 * MARGIN {
        size -= 1.0;
    }
    let scaled = font.as_scaled(PxScale::from(size));

    // Center horizontally, and vertically between the font's ascent and descent.
    let mut x = (key - text_width(&font, size, text)) / 2.0;
    let baseline = (key + scaled.ascent() + scaled.descent()) / 2.0;

    for c in text.chars() {
        let id = scaled.glyph_id(c);
        let glyph = id.with_scale_and_position(size, point(x, baseline));
        x += scaled.h_advance(id);
        let Some(outline) = font.outline_glyph(glyph) else {
            continue; // spaces have no outline
        };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            let px = bounds.min.x + gx as f32;
            let py = bounds.min.y + gy as f32;
            if px < 0.0 || py < 0.0 || px >= key || py >= key {
                return; // outside the key
            }
            // Blend white over the background, weighted by glyph coverage (anti-aliasing).
            let pixel = image.get_pixel_mut(px as u32, py as u32);
            for channel in &mut pixel.0 {
                *channel = (f32::from(*channel) * (1.0 - coverage) + 255.0 * coverage) as u8;
            }
        });
    }
}

fn text_width(font: &FontRef, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_drawn_on_background() {
        let face = KeyFace {
            color: Some([0, 0, 0]),
            icon: None,
            text: Some("12:34".into()),
        };
        let image = render(&face).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), (KEY_SIZE, KEY_SIZE));
        assert!(
            image.pixels().any(|p| p.0 == [255, 255, 255]),
            "some white text pixels"
        );
        assert_eq!(
            image.get_pixel(0, 0).0,
            [0, 0, 0],
            "corner keeps background"
        );
    }

    #[test]
    fn long_text_fits_in_key() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let face = KeyFace {
            text: Some("a very long label".into()),
            ..KeyFace::default()
        };
        render(&face).unwrap();
        assert!(text_width(&font, MIN_TEXT_SIZE, "a very long label") < 72.0);
    }
}
