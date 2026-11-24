//! Icons, drawn in code (no image files).
//!
//! The app icon: comprimo's dark rounded tile, with two "windows" made of the same 3×3 block grid,
//! sitting flush side by side with their top edges lined up. A dimmed copy marks the tray icon
//! while allineo is paused.
//!
//! Geometry is laid out in whole pixels for each size, so small icons stay crisp.
//! This file is also compiled by `build.rs` (to embed the icon in the exe), so it must not use
//! anything from the rest of the crate.

#![allow(dead_code)]

/// Signed distance to an axis-aligned rounded rectangle (negative inside).
fn rect_sdf(x: f32, y: f32, (x0, y0, x1, y1): (f32, f32, f32, f32), r: f32) -> f32 {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (hx, hy) = ((x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let r = r.min(hx).min(hy);
    let qx = (x - cx).abs() - (hx - r);
    let qy = (y - cy).abs() - (hy - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Rasterizes `shade(x, y) -> (gray, alpha)` (pixel coordinates) into straight-alpha RGBA,
/// supersampled for smooth edges.
fn raster(size: u32, shade: impl Fn(f32, f32) -> (f32, f32)) -> Vec<u8> {
    let ss = if size <= 48 { 8 } else { 4 };
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut premul, mut alpha) = (0.0, 0.0);
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = px as f32 + (sx as f32 + 0.5) / ss as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / ss as f32;
                    let (g, a) = shade(x, y);
                    premul += g * a;
                    alpha += a;
                }
            }
            let a = alpha / (ss * ss) as f32;
            let g = if alpha > 0.0 { premul / alpha } else { 0.0 };
            let g = (g * 255.0).round() as u8;
            out.extend_from_slice(&[g, g, g, (a * 255.0).round() as u8]);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// App icon
// ---------------------------------------------------------------------------------------------

/// Two windows (title bar + body, in units of the icon size) snapped side by side with their
/// top edges lined up: a taller one on the left and a shorter one on the right.
/// ((x0, x1), (title top, body top, body bottom))
type Window = ((f32, f32), (f32, f32, f32));

const WINDOWS: [Window; 2] = [
    // (x0, x1), (title top, body top, body bottom)
    ((0.195, 0.475), (0.25, 0.345, 0.75)),
    ((0.515, 0.805), (0.25, 0.345, 0.60)),
];
/// Height of the title bar, in units of the icon size.
const TITLE: f32 = 0.07;

fn blocks(size: u32) -> Vec<(f32, f32, f32, f32)> {
    let s = size as f32;
    let px = |v: f32| (v * s).round();
    let mut out = Vec::new();
    let mut prev_right = f32::MIN;
    for ((x0, x1), (top, body, bottom)) in WINDOWS {
        // Keep at least a 1 px seam between the two windows at small sizes.
        let (x0, x1) = (px(x0).max(prev_right + 1.0), px(x1));
        prev_right = x1;
        if size < 28 {
            // Too small for a separate title bar: one block per window.
            out.push((x0, px(top), x1, px(bottom)));
        } else {
            let title_bottom = px(top) + px(TITLE).max(2.0);
            out.push((x0, px(top), x1, title_bottom));
            out.push((x0, px(body).max(title_bottom + 1.0), x1, px(bottom)));
        }
    }
    out
}

fn render(size: u32, ink: f32) -> Vec<u8> {
    let s = size as f32;
    let tile_r = s * 0.23;
    let block_r = if size >= 32 { s * 0.022 } else { 0.0 };
    let blocks = blocks(size);
    raster(size, |x, y| {
        let tile = rect_sdf(x, y, (0.0, 0.0, s, s), tile_r);
        if tile > 0.0 {
            return (0.0, 0.0);
        }
        let v = y / s;
        if blocks.iter().any(|&b| rect_sdf(x, y, b, block_r) <= 0.0) {
            // Blocks: white with the faintest top-to-bottom falloff.
            return (ink - 0.05 * v, 1.0);
        }
        // Tile: soft top-to-bottom gradient with a faint highlight along the top edge.
        let base = 0.20 - 0.11 * v;
        let rim = if tile > -(s * 0.018).max(1.0) && v < 0.5 { 0.07 * (1.0 - v * 2.0) } else { 0.0 };
        (base + rim, 1.0)
    })
}

/// Straight-alpha RGBA pixels of the app icon.
pub fn rgba(size: u32) -> Vec<u8> {
    render(size, 0.97)
}

/// The tray icon while paused: same shape, blocks dimmed.
pub fn paused_rgba(size: u32) -> Vec<u8> {
    render(size, 0.48)
}

/// A multi-size .ico of the app icon, for Explorer, Apps & Features and the exe itself.
pub fn ico_bytes() -> Vec<u8> {
    ico(&[16, 20, 24, 32, 40, 48, 64, 256], rgba)
}

pub fn paused_ico_bytes() -> Vec<u8> {
    ico(&[16, 20, 24, 32, 40, 48, 64], paused_rgba)
}

// ---------------------------------------------------------------------------------------------
// .ico encoding
// ---------------------------------------------------------------------------------------------

fn ico(sizes: &[u32], render: impl Fn(u32) -> Vec<u8>) -> Vec<u8> {
    // Large frames are stored as PNG (supported since Vista), small ones as BMP for compatibility.
    let images: Vec<Vec<u8>> =
        sizes.iter().map(|&s| if s >= 128 { png(s, &render(s)) } else { bmp_entry(s, &render(s)) }).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

fn bmp_entry(size: u32, px: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    // BITMAPINFOHEADER; height is doubled to account for the AND mask.
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}

/// Minimal RGBA PNG encoder.
fn png(size: u32, px: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(px.len() + size as usize);
    for row in px.chunks(size as usize * 4) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in
        [(b"IHDR", ihdr), (b"IDAT", miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9)), (b"IEND", Vec::new())]
    {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(&data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A PNG of the app icon (for the README).
pub fn png_bytes(size: u32) -> Vec<u8> {
    png(size, &rgba(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_render_at_every_size() {
        for s in [16, 20, 24, 32, 48, 256] {
            let px = rgba(s);
            assert_eq!(px.len(), (s * s * 4) as usize);
            assert!(px.chunks(4).any(|p| p[0] > 200 && p[3] == 255), "no white blocks at {s}px");
        }
    }

    /// Writes the icons to %TEMP%\allineo-icons for a look:
    /// `cargo test -p allineo write_icon_previews -- --ignored`
    #[test]
    #[ignore]
    fn write_icon_previews() {
        let dir = std::env::temp_dir().join("allineo-icons");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.ico"), ico_bytes()).unwrap();
        std::fs::write(dir.join("paused.ico"), paused_ico_bytes()).unwrap();
        std::fs::write(dir.join("app-256.png"), png_bytes(256)).unwrap();
        // Small sizes, blown up (nearest neighbour) so they can be inspected.
        for (s, k) in [(16u32, 12u32), (20, 10), (24, 8), (32, 6), (48, 4)] {
            for (name, px) in [("app", rgba(s)), ("paused", paused_rgba(s))] {
                let big = s * k;
                let mut out = Vec::with_capacity((big * big * 4) as usize);
                for y in 0..big {
                    for x in 0..big {
                        let i = (((y / k) * s + x / k) * 4) as usize;
                        out.extend_from_slice(&px[i..i + 4]);
                    }
                }
                std::fs::write(dir.join(format!("{name}-{s}.png")), png(big, &out)).unwrap();
            }
        }
    }
}
