//! Colours as the ABI carries them.

/// An sRGB colour with alpha, not premultiplied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// A colour from hue (0–1, wrapping), saturation, value and alpha (each 0–1).
    pub fn hsva(hue: f32, saturation: f32, value: f32, alpha: f32) -> Self {
        let h = hue.rem_euclid(1.0) * 6.0;
        let s = saturation.clamp(0.0, 1.0);
        let v = value.clamp(0.0, 1.0);
        let chroma = v * s;
        let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (chroma, x, 0.0),
            1 => (x, chroma, 0.0),
            2 => (0.0, chroma, x),
            3 => (0.0, x, chroma),
            4 => (x, 0.0, chroma),
            _ => (chroma, 0.0, x),
        };
        let m = v - chroma;
        Self { r: byte(r + m), g: byte(g + m), b: byte(b + m), a: byte(alpha) }
    }

    /// The same colour with its alpha multiplied by `factor` (0–1).
    pub fn faded(self, factor: f32) -> Self {
        Self { a: byte(self.a as f32 / 255.0 * factor), ..self }
    }

    /// The ABI's `u32`: red, green, blue, alpha bytes in memory order.
    pub const fn to_bits(self) -> u32 {
        u32::from_le_bytes([self.r, self.g, self.b, self.a])
    }
}

fn byte(unit: f32) -> u8 {
    (unit.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hues_land_on_primaries() {
        assert_eq!(Color::hsva(0.0, 1.0, 1.0, 1.0), Color::rgb(255, 0, 0));
        assert_eq!(Color::hsva(1.0 / 3.0, 1.0, 1.0, 1.0), Color::rgb(0, 255, 0));
        assert_eq!(Color::hsva(2.0 / 3.0, 1.0, 1.0, 1.0), Color::rgb(0, 0, 255));
        assert_eq!(Color::hsva(-1.0, 0.0, 1.0, 0.5), Color::rgba(255, 255, 255, 128));
    }

    #[test]
    fn bits_are_rgba_in_memory_order() {
        assert_eq!(Color::rgba(1, 2, 3, 4).to_bits().to_le_bytes(), [1, 2, 3, 4]);
    }
}
