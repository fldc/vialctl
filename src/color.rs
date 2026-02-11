use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhitePoint(pub [u8; 3]);

impl WhitePoint {
    pub fn new(rgb: [u8; 3]) -> Option<Self> {
        if rgb.contains(&0) {
            None
        } else {
            Some(Self(rgb))
        }
    }

    pub fn apply(self, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        let [wr, wg, wb] = self.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (
            (f64::from(r) * f64::from(wr) / 255.0).round() as u8,
            (f64::from(g) * f64::from(wg) / 255.0).round() as u8,
            (f64::from(b) * f64::from(wb) / 255.0).round() as u8,
        )
    }
}

pub fn parse_white_point(s: &str) -> Result<WhitePoint, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err("expected 3 comma-separated values, e.g. 200,255,230".into());
    }

    let r = parts[0]
        .trim()
        .parse::<u8>()
        .map_err(|e| format!("red: {e}"))?;
    let g = parts[1]
        .trim()
        .parse::<u8>()
        .map_err(|e| format!("green: {e}"))?;
    let b = parts[2]
        .trim()
        .parse::<u8>()
        .map_err(|e| format!("blue: {e}"))?;

    WhitePoint::new([r, g, b]).ok_or_else(|| "white point channels must be 1-255".into())
}

pub fn parse_hex_rgb(hex: &str) -> Result<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.chars().count() != 6 {
        bail!("color must be 6 hex characters (0-9, a-f), e.g. ff00ff");
    }

    let r = u8::from_str_radix(&hex[0..2], 16).context("invalid red component")?;
    let g = u8::from_str_radix(&hex[2..4], 16).context("invalid green component")?;
    let b = u8::from_str_radix(&hex[4..6], 16).context("invalid blue component")?;

    Ok((r, g, b))
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let red = f64::from(r) / 255.0;
    let green = f64::from(g) / 255.0;
    let blue = f64::from(b) / 255.0;

    let max = red.max(green).max(blue);
    let delta = max - red.min(green).min(blue);

    let hue = if delta < f64::EPSILON {
        0.0
    } else if (max - red).abs() < f64::EPSILON {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if (max - green).abs() < f64::EPSILON {
        60.0 * (((blue - red) / delta) + 2.0)
    } else {
        60.0 * (((red - green) / delta) + 4.0)
    };

    let sat = if max < f64::EPSILON { 0.0 } else { delta / max };

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (
        (hue / 360.0 * 255.0).round() as u8,
        (sat * 255.0).round() as u8,
        (max * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_rgb_basic() {
        assert_eq!(parse_hex_rgb("ff00ff").unwrap(), (255, 0, 255));
    }

    #[test]
    fn parse_hex_rgb_with_hash() {
        assert_eq!(parse_hex_rgb("#00ff00").unwrap(), (0, 255, 0));
    }

    #[test]
    fn parse_hex_rgb_black_and_white() {
        assert_eq!(parse_hex_rgb("000000").unwrap(), (0, 0, 0));
        assert_eq!(parse_hex_rgb("ffffff").unwrap(), (255, 255, 255));
    }

    #[test]
    fn parse_hex_rgb_uppercase() {
        assert_eq!(parse_hex_rgb("FF8800").unwrap(), (255, 136, 0));
    }

    #[test]
    fn parse_hex_rgb_rejects_short() {
        assert!(parse_hex_rgb("fff").is_err());
    }

    #[test]
    fn parse_hex_rgb_rejects_invalid_chars() {
        assert!(parse_hex_rgb("gghhii").is_err());
    }

    #[test]
    fn rgb_to_hsv_pure_red() {
        let (h, s, v) = rgb_to_hsv(255, 0, 0);
        assert_eq!(h, 0);
        assert_eq!(s, 255);
        assert_eq!(v, 255);
    }

    #[test]
    fn rgb_to_hsv_pure_green() {
        let (h, s, v) = rgb_to_hsv(0, 255, 0);
        // Green = 120 degrees = 120/360 * 255 ≈ 85
        assert_eq!(h, 85);
        assert_eq!(s, 255);
        assert_eq!(v, 255);
    }

    #[test]
    fn rgb_to_hsv_pure_blue() {
        let (h, s, v) = rgb_to_hsv(0, 0, 255);
        // Blue = 240 degrees = 240/360 * 255 ≈ 170
        assert_eq!(h, 170);
        assert_eq!(s, 255);
        assert_eq!(v, 255);
    }

    #[test]
    fn rgb_to_hsv_black() {
        assert_eq!(rgb_to_hsv(0, 0, 0), (0, 0, 0));
    }

    #[test]
    fn rgb_to_hsv_white() {
        assert_eq!(rgb_to_hsv(255, 255, 255), (0, 0, 255));
    }

    #[test]
    fn rgb_to_hsv_gray() {
        let (h, s, v) = rgb_to_hsv(128, 128, 128);
        assert_eq!(h, 0);
        assert_eq!(s, 0);
        assert_eq!(v, 128);
    }

    #[test]
    fn white_point_rejects_zero_channel() {
        assert!(WhitePoint::new([0, 255, 255]).is_none());
        assert!(WhitePoint::new([255, 0, 255]).is_none());
        assert!(WhitePoint::new([255, 255, 0]).is_none());
    }

    #[test]
    fn white_point_accepts_valid() {
        assert!(WhitePoint::new([1, 1, 1]).is_some());
        assert!(WhitePoint::new([200, 255, 230]).is_some());
    }

    #[test]
    fn white_point_identity() {
        let wp = WhitePoint::new([255, 255, 255]).unwrap();
        assert_eq!(wp.apply(100, 200, 50), (100, 200, 50));
    }

    #[test]
    fn white_point_scales_down() {
        let wp = WhitePoint::new([128, 255, 255]).unwrap();
        let (r, _g, _b) = wp.apply(255, 255, 255);
        // 255 * 128 / 255 = 128
        assert_eq!(r, 128);
    }

    #[test]
    fn parse_white_point_valid() {
        let wp = parse_white_point("200,255,230").unwrap();
        assert_eq!(wp.0, [200, 255, 230]);
    }

    #[test]
    fn parse_white_point_with_spaces() {
        let wp = parse_white_point("200, 255, 230").unwrap();
        assert_eq!(wp.0, [200, 255, 230]);
    }

    #[test]
    fn parse_white_point_rejects_zero() {
        assert!(parse_white_point("0,255,255").is_err());
    }

    #[test]
    fn parse_white_point_rejects_wrong_count() {
        assert!(parse_white_point("200,255").is_err());
        assert!(parse_white_point("200,255,230,100").is_err());
    }

    #[test]
    fn parse_white_point_rejects_overflow() {
        assert!(parse_white_point("256,255,255").is_err());
    }
}
