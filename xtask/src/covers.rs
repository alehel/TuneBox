//! Album covers for the demo library, drawn procedurally: every picture is original and needs
//! no attribution.

use image::{Rgb, RgbImage};

const SIZE: u32 = 512;

/// The motif of a cover.
#[derive(Clone, Copy)]
pub enum Style {
    Sunset,
    Aurora,
    Grid,
    Stripes,
    Dots,
    Waves,
    Rings,
    Shards,
}

/// A JPEG cover in `style` using `palette` (background, main colour, accent).
pub fn cover(style: Style, palette: [[u8; 3]; 3]) -> Result<Vec<u8>, String> {
    let [bg, main, accent] = palette.map(|c| [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])]);
    let image = RgbImage::from_fn(SIZE, SIZE, |x, y| {
        let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);
        let color = match style {
            Style::Sunset => sunset(u, v, bg, main, accent),
            Style::Aurora => aurora(u, v, bg, main, accent),
            Style::Grid => grid(u, v, bg, main, accent),
            Style::Stripes => stripes(u, v, bg, main, accent),
            Style::Dots => dots(u, v, bg, main, accent),
            Style::Waves => waves(u, v, bg, main, accent),
            Style::Rings => rings(u, v, bg, main, accent),
            Style::Shards => shards(u, v, bg, main, accent),
        };
        Rgb(color.map(|c| c.clamp(0.0, 255.0) as u8))
    });
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 88)
        .encode_image(&image)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

type C = [f32; 3];

fn mix(a: C, b: C, t: f32) -> C {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn sunset(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let sky = mix(main, bg, v * 1.3);
    let sun = ((u - 0.5).powi(2) + (v - 0.55).powi(2)).sqrt();
    let horizon = 0.68 + 0.04 * (u * 9.0).sin() + 0.02 * (u * 23.0).cos();
    if v > horizon {
        mix(bg, [10.0, 10.0, 20.0], (v - horizon) * 3.0)
    } else if sun < 0.17 {
        mix(accent, [255.0, 240.0, 200.0], 1.0 - sun / 0.17)
    } else {
        sky
    }
}

fn aurora(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let base = mix(bg, [4.0, 6.0, 18.0], v);
    let curtain = (v - 0.35 - 0.12 * (u * 7.0).sin() - 0.05 * (u * 17.0).cos()).abs();
    let glow = (1.0 - curtain * 6.0).max(0.0);
    let star = if ((u * 431.0).sin() * (v * 619.0).cos()).abs() > 0.9993 { 1.0 } else { 0.0 };
    let lit = mix(base, mix(main, accent, u), glow);
    mix(lit, [255.0, 255.0, 255.0], star)
}

fn grid(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let (cx, cy) = ((u * 8.0).floor(), (v * 8.0).floor());
    let n = ((cx * 12.9898 + cy * 78.233).sin() * 43758.547).fract().abs();
    let inner = (u * 8.0).fract() > 0.08 && (v * 8.0).fract() > 0.08;
    if !inner {
        [12.0, 8.0, 24.0]
    } else if n > 0.8 {
        accent
    } else if n > 0.45 {
        mix(main, bg, v)
    } else {
        mix(bg, [12.0, 8.0, 24.0], 0.5)
    }
}

fn stripes(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let band = ((u + v) * 7.0).floor() as i32;
    let grain = ((u * 900.0).sin() * (v * 13.0).cos()) * 10.0;
    let c = match band.rem_euclid(3) {
        0 => bg,
        1 => main,
        _ => accent,
    };
    [c[0] + grain, c[1] + grain, c[2] + grain]
}

fn dots(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let (fx, fy) = ((u * 12.0).fract() - 0.5, (v * 12.0).fract() - 0.5);
    let radius = 0.12 + 0.3 * (1.0 - ((u - 0.35).powi(2) + (v - 0.4).powi(2)).sqrt());
    if (fx * fx + fy * fy).sqrt() < radius { mix(main, accent, u) } else { bg }
}

fn waves(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let wave = (v * 14.0 + (u * 6.0).sin() * 1.4).sin();
    let deep = mix(bg, [5.0, 15.0, 35.0], v);
    if wave > 0.6 { mix(main, accent, u) } else { mix(deep, main, (wave + 1.0) * 0.15) }
}

fn rings(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let d = ((u - 0.62).powi(2) + (v - 0.42).powi(2)).sqrt();
    let ring = (d * 26.0).floor() as i32;
    if d > 0.55 {
        bg
    } else if ring % 5 == 0 {
        accent
    } else {
        mix(main, bg, d * 1.6)
    }
}

fn shards(u: f32, v: f32, bg: C, main: C, accent: C) -> C {
    let a = (u * 3.1 + v * 1.7).sin() + (u * -2.3 + v * 4.1).cos();
    let b = (u * 5.3 - v * 2.9).sin();
    if a > 0.9 {
        accent
    } else if b > 0.2 {
        mix(main, bg, v)
    } else {
        mix(bg, [0.0, 0.0, 0.0], u * 0.5)
    }
}
