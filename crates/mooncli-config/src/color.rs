//! 颜色解析和 WCAG 对比度计算（和 tools/check_contrast.py 同一个公式）。

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub fn parse(s: &str) -> crate::Result<Self> {
        let h = s.strip_prefix('#').ok_or_else(|| crate::Error::BadColor(s.into()))?;
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| crate::Error::BadColor(s.into()));
        match h.len() {
            6 => Ok(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 255 }),
            8 => Ok(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
            _ => Err(crate::Error::BadColor(s.into())),
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    pub fn with_alpha(self, a: f64) -> Self {
        Self { a: (a.clamp(0.0, 1.0) * 255.0).round() as u8, ..self }
    }

    /// 叠在 `bg` 上之后看到的颜色（去掉透明度）。
    pub fn over(self, bg: Rgba) -> Rgba {
        let a = self.a as f64 / 255.0;
        let mix = |f: u8, b: u8| (f as f64 * a + b as f64 * (1.0 - a)).round() as u8;
        Rgba { r: mix(self.r, bg.r), g: mix(self.g, bg.g), b: mix(self.b, bg.b), a: 255 }
    }

    /// HSL 色相，0–360 度（和 Python colorsys 的 HLS 色相一致）
    pub fn hue(self) -> f64 {
        let (r, g, b) = (self.r as f64 / 255.0, self.g as f64 / 255.0, self.b as f64 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        if d == 0.0 {
            return 0.0;
        }
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        h * 60.0
    }

    pub fn luminance(self) -> f64 {
        let ch = |c: u8| {
            let c = c as f64 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(self.r) + 0.7152 * ch(self.g) + 0.0722 * ch(self.b)
    }
}

pub fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_contrast() {
        let w = Rgba::parse("#FFFFFF").unwrap();
        let k = Rgba::parse("#000000").unwrap();
        assert!((contrast(w, k) - 21.0).abs() < 1e-9);
        assert_eq!(Rgba::parse("#11223380").unwrap().a, 0x80);
        assert!(Rgba::parse("red").is_err());
        assert!(Rgba::parse("#12345").is_err());
        assert!((Rgba::parse("#FF0000").unwrap().hue() - 0.0).abs() < 1e-9);
        assert!((Rgba::parse("#0000FF").unwrap().hue() - 240.0).abs() < 1e-9);
        // 0.5 存成 alpha 128（=0.502），叠在白底上是 127
        assert_eq!(k.with_alpha(0.5).over(w).to_hex(), "#7F7F7F");
    }
}
