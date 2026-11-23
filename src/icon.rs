//! Rayo's icon: a lightning bolt on a dark rounded square, rasterized in code.
//! Shared by build.rs (writes the .ico embedded in the .exe) and the app (window icon).

/// Bolt outline in unit coordinates (y down).
const BOLT: [(f32, f32); 7] = [
    (0.60, 0.10),
    (0.25, 0.55),
    (0.46, 0.55),
    (0.39, 0.90),
    (0.75, 0.43),
    (0.54, 0.43),
    (0.63, 0.10),
];
const BG: [u8; 3] = [0x12, 0x14, 0x18];
const FG: [u8; 3] = [0x7c, 0xc4, 0xe4];

fn inside_bolt(x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = BOLT.len() - 1;
    for (i, &(xi, yi)) in BOLT.iter().enumerate() {
        let (xj, yj) = BOLT[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn inside_rounded(x: f32, y: f32, r: f32) -> bool {
    let cx = x.clamp(r, 1.0 - r);
    let cy = y.clamp(r, 1.0 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// RGBA pixels, `size`×`size`, 4×4 supersampled.
pub fn rgba(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut cover, mut bolt) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if inside_rounded(x, y, 0.22) {
                        cover += 1;
                        bolt += inside_bolt(x, y) as u32;
                    }
                }
            }
            let n = (SS * SS) as f32;
            let t = if cover == 0 {
                0.0
            } else {
                bolt as f32 / cover as f32
            };
            for c in 0..3 {
                out.push((BG[c] as f32 * (1.0 - t) + FG[c] as f32 * t).round() as u8);
            }
            out.push((cover as f32 / n * 255.0).round() as u8);
        }
    }
    out
}

/// Multi-size .ico (32-bit BMP entries).
#[allow(dead_code)] // used by build.rs
pub fn ico() -> Vec<u8> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| {
            let px = rgba(s);
            let mask_row = s.div_ceil(32) * 4;
            let mut b = Vec::new();
            for v in [40u32, s, s * 2] {
                b.extend(v.to_le_bytes());
            }
            b.extend(1u16.to_le_bytes());
            b.extend(32u16.to_le_bytes());
            for v in [0u32, s * s * 4 + mask_row * s, 0, 0, 0, 0] {
                b.extend(v.to_le_bytes());
            }
            // Bottom-up BGRA rows.
            for y in (0..s).rev() {
                for x in 0..s {
                    let i = ((y * s + x) * 4) as usize;
                    b.extend([px[i + 2], px[i + 1], px[i], px[i + 3]]);
                }
            }
            b.extend(std::iter::repeat_n(0u8, (mask_row * s) as usize));
            b
        })
        .collect();
    let mut out = Vec::new();
    out.extend([0u8, 0, 1, 0]);
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&images) {
        let d = if *s >= 256 { 0 } else { *s as u8 };
        out.extend([d, d, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((img.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend(img);
    }
    out
}
