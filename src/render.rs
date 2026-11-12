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

/// Draws white text centered on the key. Words wrap onto several lines, and the text
/// shrinks until it fits; at the minimum size, lines that still do not fit are cut.
// Pixel math mixes f32 font coordinates with u32 image coordinates; values are tiny (< 100).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn draw_text(image: &mut RgbImage, text: &str) {
    let font = FontRef::try_from_slice(FONT).expect("bundled font is valid");
    let room = KEY_SIZE as f32 - 2.0 * MARGIN;

    let mut size = MAX_TEXT_SIZE;
    let mut lines = wrap(&font, size, text, room);
    while size > MIN_TEXT_SIZE && !fits(&font, size, &lines, room) {
        size -= 1.0;
        lines = wrap(&font, size, text, room);
    }
    let line_height = font.as_scaled(PxScale::from(size)).height();
    lines.truncate(((room / line_height) as usize).max(1));

    // Center the block of lines vertically.
    let top = (KEY_SIZE as f32 - lines.len() as f32 * line_height) / 2.0;
    for (i, line) in lines.iter().enumerate() {
        draw_line(image, &font, size, line, top + i as f32 * line_height);
    }
}

/// Draws one line of text, centered horizontally, starting at `top`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn draw_line(image: &mut RgbImage, font: &FontRef, size: f32, line: &str, top: f32) {
    let scaled = font.as_scaled(PxScale::from(size));
    let key = KEY_SIZE as f32;
    let mut x = (key - text_width(font, size, line)) / 2.0;
    let baseline = top + scaled.ascent();

    for c in line.chars() {
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

/// Splits `text` into lines no wider than `room`, breaking between words.
/// A single word wider than `room` stays on its own line.
fn wrap(font: &FontRef, size: f32, text: &str, room: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if text_width(font, size, &format!("{line} {word}")) <= room => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

#[allow(clippy::cast_precision_loss)]
fn fits(font: &FontRef, size: f32, lines: &[String], room: f32) -> bool {
    let height = lines.len() as f32 * font.as_scaled(PxScale::from(size)).height();
    height <= room && lines.iter().all(|l| text_width(font, size, l) <= room)
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
    fn long_text_wraps_on_words() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let lines = wrap(&font, 9.0, "Bohemian Rhapsody Remastered", 66.0);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), "Bohemian Rhapsody Remastered");
        assert!(lines.iter().all(|l| text_width(&font, 9.0, l) <= 66.0));
    }

    #[test]
    fn very_long_text_still_renders() {
        let text = "a very long title that can never fit on such a small key at all".repeat(3);
        let face = KeyFace {
            text: Some(text),
            ..KeyFace::default()
        };
        render(&face).unwrap();
    }
}
