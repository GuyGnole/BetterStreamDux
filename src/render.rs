//! Turns a `KeyFace` into a 72x72 image: background (color or image), then the symbol
//! (white unless the button sets `symbol_color`) and/or text in white.
//!
//! Layout: symbol alone -> big and centered; text alone -> centered, wrapped;
//! both -> symbol in the top part, text (smaller) underneath.
//! A pressed key is drawn smaller on black, so it looks pushed in.

use ab_glyph::{Font, FontRef, OutlinedGlyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result};
use image::{DynamicImage, Rgb, RgbImage, imageops, imageops::FilterType};

use crate::icons;
use crate::screen::KeyFace;

const WHITE: [u8; 3] = [255, 255, 255];

/// Pixel size of one key on the MK.2.
pub const KEY_SIZE: u32 = 72;
const KEY: f32 = 72.0;

/// Bundled in the binary so rendering does not depend on installed fonts.
const FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");
const MIN_TEXT_SIZE: f32 = 8.0;
/// Space kept free on each side of the text.
const MARGIN: f32 = 3.0;

/// A horizontal band of the key where text is drawn.
struct Area {
    top: f32,
    height: f32,
    max_size: f32,
}

const FULL: Area = Area {
    top: MARGIN,
    height: KEY - 2.0 * MARGIN,
    max_size: 22.0,
};
const UNDER_SYMBOL: Area = Area {
    top: 46.0,
    height: 24.0,
    max_size: 13.0,
};
const SYMBOL_ALONE: f32 = 46.0;
const SYMBOL_ABOVE_TEXT: f32 = 32.0;
/// Side of a pressed key's picture, in pixels (the key is 72).
const PRESSED_SIZE: u32 = 60;

pub fn render(face: &KeyFace) -> Result<DynamicImage> {
    let mut image = match (&face.icon, face.color) {
        (Some(path), _) => image::open(path)
            .with_context(|| format!("cannot open icon {path}"))?
            .resize_to_fill(KEY_SIZE, KEY_SIZE, FilterType::Triangle)
            .to_rgb8(),
        (None, Some(rgb)) => RgbImage::from_pixel(KEY_SIZE, KEY_SIZE, Rgb(rgb)),
        (None, None) => RgbImage::new(KEY_SIZE, KEY_SIZE), // black
    };
    let symbol_color = face.symbol_color.unwrap_or(WHITE);
    match (face.symbol, &face.text) {
        (Some(symbol), None) => {
            draw_symbol(&mut image, symbol, symbol_color, SYMBOL_ALONE, KEY / 2.0);
        }
        (Some(symbol), Some(text)) => {
            draw_symbol(&mut image, symbol, symbol_color, SYMBOL_ABOVE_TEXT, 25.0);
            draw_text(&mut image, text, &UNDER_SYMBOL);
        }
        (None, Some(text)) => draw_text(&mut image, text, &FULL),
        (None, None) => {}
    }
    if face.pressed {
        image = shrink(&image);
    }
    Ok(DynamicImage::ImageRgb8(image))
}

/// The whole picture scaled down to `PRESSED_SIZE`, centered on black.
fn shrink(image: &RgbImage) -> RgbImage {
    let small = imageops::resize(image, PRESSED_SIZE, PRESSED_SIZE, FilterType::Triangle);
    let mut out = RgbImage::new(KEY_SIZE, KEY_SIZE);
    let offset = i64::from((KEY_SIZE - PRESSED_SIZE) / 2);
    imageops::replace(&mut out, &small, offset, offset);
    out
}

/// Draws an icon in `color`, centered horizontally around `center_y`.
fn draw_symbol(image: &mut RgbImage, symbol: char, color: [u8; 3], size: f32, center_y: f32) {
    let font = icons::font();
    let glyph = font
        .glyph_id(symbol)
        .with_scale_and_position(size, point(0.0, 0.0));
    let Some(outline) = font.outline_glyph(glyph) else {
        return;
    };
    // Center the drawn shape itself (glyph metrics vary a lot between emoji).
    let bounds = outline.px_bounds();
    let dx = KEY / 2.0 - f32::midpoint(bounds.min.x, bounds.max.x);
    let dy = center_y - f32::midpoint(bounds.min.y, bounds.max.y);
    paint(image, &outline, color, dx, dy);
}

/// Draws white text centered in `area`. Words wrap onto several lines, and the text
/// shrinks until it fits; at the minimum size, lines that still do not fit are cut.
// Pixel math mixes f32 font coordinates with usize line counts; values are tiny (< 100).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn draw_text(image: &mut RgbImage, text: &str, area: &Area) {
    let font = FontRef::try_from_slice(FONT).expect("bundled font is valid");
    let width = KEY - 2.0 * MARGIN;

    let mut size = area.max_size;
    let mut lines = wrap(&font, size, text, width);
    while size > MIN_TEXT_SIZE && !fits(&font, size, &lines, width, area.height) {
        size -= 1.0;
        lines = wrap(&font, size, text, width);
    }
    let line_height = font.as_scaled(PxScale::from(size)).height();
    lines.truncate(((area.height / line_height) as usize).max(1));

    // Center the block of lines vertically in the area.
    let top = area.top + (area.height - lines.len() as f32 * line_height) / 2.0;
    for (i, line) in lines.iter().enumerate() {
        draw_line(image, &font, size, line, top + i as f32 * line_height);
    }
}

/// Draws one line of text, centered horizontally, starting at `top`.
fn draw_line(image: &mut RgbImage, font: &FontRef, size: f32, line: &str, top: f32) {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut x = (KEY - text_width(font, size, line)) / 2.0;
    let baseline = top + scaled.ascent();

    for c in line.chars() {
        let id = scaled.glyph_id(c);
        let glyph = id.with_scale_and_position(size, point(x, baseline));
        x += scaled.h_advance(id);
        if let Some(outline) = font.outline_glyph(glyph) {
            paint(image, &outline, WHITE, 0.0, 0.0);
        } // spaces have no outline
    }
}

/// Blends a glyph in `color` over the image, shifted by (dx, dy), clipped to the key.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn paint(image: &mut RgbImage, outline: &OutlinedGlyph, color: [u8; 3], dx: f32, dy: f32) {
    let bounds = outline.px_bounds();
    outline.draw(|gx, gy, coverage| {
        let px = bounds.min.x + gx as f32 + dx;
        let py = bounds.min.y + gy as f32 + dy;
        if px < 0.0 || py < 0.0 || px >= KEY || py >= KEY {
            return; // outside the key
        }
        // Weighted by glyph coverage: smooth (anti-aliased) edges.
        let pixel = image.get_pixel_mut(px as u32, py as u32);
        for (channel, target) in pixel.0.iter_mut().zip(color) {
            *channel =
                (f32::from(*channel) * (1.0 - coverage) + f32::from(target) * coverage) as u8;
        }
    });
}

/// Splits `text` into lines no wider than `width`, breaking between words.
/// A single word wider than `width` stays on its own line.
fn wrap(font: &FontRef, size: f32, text: &str, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if text_width(font, size, &format!("{line} {word}")) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

#[allow(clippy::cast_precision_loss)]
fn fits(font: &FontRef, size: f32, lines: &[String], width: f32, height: f32) -> bool {
    let total = lines.len() as f32 * font.as_scaled(PxScale::from(size)).height();
    total <= height && lines.iter().all(|l| text_width(font, size, l) <= width)
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

    fn white_rows(face: &KeyFace) -> Vec<u32> {
        let image = render(face).unwrap().to_rgb8();
        (0..KEY_SIZE)
            .filter(|&y| (0..KEY_SIZE).any(|x| image.get_pixel(x, y).0 == [255, 255, 255]))
            .collect()
    }

    #[test]
    fn text_is_drawn_on_background() {
        let face = KeyFace {
            color: Some([0, 0, 0]),
            text: Some("12:34".into()),
            ..KeyFace::default()
        };
        let image = render(&face).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), (KEY_SIZE, KEY_SIZE));
        assert!(!white_rows(&face).is_empty(), "some white text pixels");
        assert_eq!(
            image.get_pixel(0, 0).0,
            [0, 0, 0],
            "corner keeps background"
        );
    }

    #[test]
    fn symbol_goes_above_the_text() {
        let symbol_only = white_rows(&KeyFace {
            symbol: Some('▶'),
            ..KeyFace::default()
        });
        assert!(!symbol_only.is_empty(), "symbol drawn");
        let both = white_rows(&KeyFace {
            symbol: Some('▶'),
            text: Some("Play".into()),
            ..KeyFace::default()
        });
        assert!(both.iter().any(|&y| y < 36), "symbol in the top half");
        assert!(both.iter().any(|&y| y > 50), "text at the bottom");
    }

    #[test]
    fn pressed_key_is_smaller() {
        let face = KeyFace {
            color: Some([255, 0, 0]),
            pressed: true,
            ..KeyFace::default()
        };
        let image = render(&face).unwrap().to_rgb8();
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0], "black border");
        assert_eq!(
            image.get_pixel(36, 36).0,
            [255, 0, 0],
            "color in the middle"
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
            symbol: Some('🎵'),
            ..KeyFace::default()
        };
        render(&face).unwrap();
    }
}
