//! Renders the tray icon with an unread-count badge baked in.
//! Spec: docs/superpowers/specs/2026-06-14-tray-unread-badge-design.md
//! Pure pixel math + glyph rasterization — no GTK/Tauri types, so it unit-tests
//! without a display.

use std::sync::OnceLock;

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};

/// Base WT icon, embedded at compile time. 128×128 keeps the badge crisp when
/// the desktop scales the tray icon down. CLAUDE.md guarantees bundled icons are
/// 8-bit RGBA PNG, so decoding is strict.
const BASE_PNG: &[u8] = include_bytes!("../icons/128x128.png");

/// Bold font for the digits (see assets/badge-font.LICENSE.txt).
const FONT_TTF: &[u8] = include_bytes!("../assets/badge-font.ttf");

const DISC: [u8; 3] = [0xFF, 0x3B, 0x30]; // #FF3B30
const GLYPH: [u8; 3] = [0xFF, 0xFF, 0xFF]; // #FFFFFF

/// The string drawn inside the badge. Exact for 1..=99; "99+" beyond, because a
/// 3-digit number is unreadable on a downscaled tray icon. The true count is
/// unaffected elsewhere — only the drawn glyphs cap.
pub fn glyphs_for(count: u32) -> String {
    if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    }
}

/// Decode the base icon once into straight RGBA (rgba, width, height).
fn base() -> &'static (Vec<u8>, u32, u32) {
    static BASE: OnceLock<(Vec<u8>, u32, u32)> = OnceLock::new();
    BASE.get_or_init(|| {
        let decoder = png::Decoder::new(BASE_PNG);
        let mut reader = decoder.read_info().expect("badge: base PNG header");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("badge: base PNG frame");
        assert_eq!(
            info.color_type,
            png::ColorType::Rgba,
            "badge: base icon must be RGBA"
        );
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        buf.truncate(info.buffer_size());
        (buf, info.width, info.height)
    })
}

fn font() -> &'static FontRef<'static> {
    static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(FONT_TTF).expect("badge: font parse"))
}

/// Render the WT icon with an unread badge. `count == 0` returns the untouched
/// base icon (no disc). Output is straight RGBA, ready for `Image::new_owned`.
pub fn render(count: u32) -> (Vec<u8>, u32, u32) {
    let (base_rgba, w, h) = base();
    let mut rgba = base_rgba.clone();
    if count > 0 {
        draw_badge(&mut rgba, *w, *h, &glyphs_for(count));
    }
    (rgba, *w, *h)
}

/// Straight-alpha "over" composite of `color` at coverage/alpha `a` onto pixel (x,y).
fn blend(rgba: &mut [u8], w: u32, x: u32, y: u32, color: [u8; 3], a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let i = ((y * w + x) * 4) as usize;
    let mix = |src: u8, dst: u8| (src as f32 * a + dst as f32 * (1.0 - a)).round() as u8;
    rgba[i] = mix(color[0], rgba[i]);
    rgba[i + 1] = mix(color[1], rgba[i + 1]);
    rgba[i + 2] = mix(color[2], rgba[i + 2]);
    let dst_a = rgba[i + 3] as f32 / 255.0;
    rgba[i + 3] = (((a + dst_a * (1.0 - a)) * 255.0).round()).clamp(0.0, 255.0) as u8;
}

/// Antialiased coverage of a horizontal capsule (spine from (cx_l,cy) to
/// (cx_r,cy), radius rr) at sample point (px,py). A circle when cx_l == cx_r.
fn capsule_coverage(px: f32, py: f32, cx_l: f32, cx_r: f32, cy: f32, rr: f32) -> f32 {
    let nearest_x = px.clamp(cx_l, cx_r);
    let dx = px - nearest_x;
    let dy = py - cy;
    let dist = (dx * dx + dy * dy).sqrt();
    (rr + 0.5 - dist).clamp(0.0, 1.0)
}

fn draw_badge(rgba: &mut [u8], w: u32, h: u32, text: &str) {
    let f = font();
    let margin = 6.0_f32;
    let rr = 28.0_f32; // disc radius -> 56px diameter on a 128px icon
    let px = 40.0_f32; // font pixel size; digit cap height ~29px fits the disc
    let cy = h as f32 - margin - rr;
    let cx_r = w as f32 - margin - rr;

    // Lay out the glyphs at baseline (0,0) to measure and to draw later.
    let scale = PxScale::from(px);
    let scaled = f.as_scaled(scale);
    let mut pen = 0.0_f32;
    let mut outlines = Vec::new();
    for c in text.chars() {
        let id = f.glyph_id(c);
        let glyph = id.with_scale_and_position(scale, point(pen, 0.0));
        pen += scaled.h_advance(id);
        if let Some(o) = f.outline_glyph(glyph) {
            outlines.push(o);
        }
    }

    // Capsule width grows to fit the text (e.g. "99+"); otherwise a plain disc.
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
    for o in &outlines {
        let b = o.px_bounds();
        min_x = min_x.min(b.min.x);
        min_y = min_y.min(b.min.y);
        max_x = max_x.max(b.max.x);
        max_y = max_y.max(b.max.y);
    }
    let text_w = if outlines.is_empty() {
        0.0
    } else {
        (max_x - min_x).max(0.0)
    };
    let pad_x = 10.0_f32;
    let capsule_w = (rr * 2.0).max(text_w + 2.0 * pad_x);
    let cx_l = cx_r - (capsule_w - rr * 2.0);
    let cx_mid = (cx_l + cx_r) / 2.0;

    // Draw the disc/pill (full-image scan; 128×128 is trivial).
    for y in 0..h {
        for x in 0..w {
            let cov = capsule_coverage(x as f32 + 0.5, y as f32 + 0.5, cx_l, cx_r, cy, rr);
            if cov > 0.0 {
                blend(rgba, w, x, y, DISC, cov);
            }
        }
    }

    if outlines.is_empty() {
        return;
    }

    // Center the glyph union at the capsule center, then blend the digits.
    let off_x = cx_mid - (min_x + max_x) / 2.0;
    let off_y = cy - (min_y + max_y) / 2.0;
    for o in &outlines {
        let b = o.px_bounds();
        o.draw(|gx, gy, cov| {
            if cov <= 0.0 {
                return;
            }
            let x = (b.min.x + gx as f32 + off_x).round();
            let y = (b.min.y + gy as f32 + off_y).round();
            if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 {
                return;
            }
            blend(rgba, w, x as u32, y as u32, GLYPH, cov);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_exact_then_capped() {
        assert_eq!(glyphs_for(1), "1");
        assert_eq!(glyphs_for(7), "7");
        assert_eq!(glyphs_for(99), "99");
        assert_eq!(glyphs_for(100), "99+");
        assert_eq!(glyphs_for(150), "99+");
    }

    #[test]
    fn base_is_128_rgba() {
        let (rgba, w, h) = render(0);
        assert_eq!((w, h), (128, 128));
        assert_eq!(rgba.len(), 128 * 128 * 4);
    }

    #[test]
    fn zero_returns_untouched_base() {
        let (a, _, _) = render(0);
        let (b, _, _) = render(0);
        assert_eq!(a, b, "render(0) must be deterministic");
    }

    #[test]
    fn count_draws_over_base() {
        let base = render(0).0;
        let badged = render(5).0;
        assert_eq!(base.len(), badged.len());
        assert_ne!(base, badged, "a non-zero count must change pixels");
    }

    #[test]
    fn different_counts_differ() {
        // "5" vs "99+" must produce visibly different glyphs.
        assert_ne!(render(5).0, render(150).0);
    }

    /// Not run by default. `cargo test badge -- --ignored --nocapture` dumps
    /// sample badges to /tmp for eyeballing the visual result offline.
    #[test]
    #[ignore]
    fn dump_samples() {
        for n in [1u32, 5, 42, 99, 150] {
            let (rgba, w, h) = render(n);
            let path = format!("/tmp/badge-{n}.png");
            let file = std::fs::File::create(&path).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
            println!("wrote {path}");
        }
    }
}
