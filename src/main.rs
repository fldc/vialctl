use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use hidapi::{DeviceInfo, HidApi, HidDevice};
use serde::Deserialize;

const MSG_LEN: usize = 32;
const VIAL_SERIAL_NUMBER_MAGIC: &str = "vial:f64c2b3c";

const VIALRGB_EFFECT_SOLID_COLOR: u16 = 2;

const CMD_VIA_LIGHTING_SET_VALUE: u8 = 0x07;
const CMD_VIA_LIGHTING_GET_VALUE: u8 = 0x08;
const CMD_VIA_LIGHTING_SAVE: u8 = 0x09;

const VIALRGB_GET_INFO: u8 = 0x40;
const VIALRGB_GET_SUPPORTED: u8 = 0x42;
const VIALRGB_SET_MODE: u8 = 0x41;

const DEFAULT_EFFECT_SPEED: u8 = 128;

const MAX_EFFECT_QUERY_ROUNDS: usize = 100;

#[derive(Deserialize, Default)]
struct Config {
    white_point: Option<[u8; 3]>,
}

fn load_config() -> Config {
    let Some(config_dir) = dirs::config_dir() else {
        return Config::default();
    };
    let path = config_dir.join("vialctl").join("config.toml");
    let Ok(contents) = fs::read_to_string(&path) else {
        return Config::default();
    };
    match toml::from_str(&contents) {
        Ok(config) => {
            let config: Config = config;
            if let Some([r, g, b]) = config.white_point
                && (r == 0 || g == 0 || b == 0)
            {
                eprintln!(
                    "warning: ignoring white_point in {}: channels must be 1-255",
                    path.display()
                );
                return Config { white_point: None };
            }
            config
        }
        Err(e) => {
            eprintln!("warning: ignoring invalid config {}: {e}", path.display());
            Config::default()
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vialctl")
        .join("config.toml")
}

#[derive(Parser)]
#[command(
    version,
    about = "Set RGB color on keyboards running Vial firmware with RGB support",
    after_help = format!(
        "Examples:\n  vialctl ff00ff\n  vialctl '#00ff00'\n  vialctl ff0000 --brightness 80\n\n\
         Config: {}\n  Example:\n    white_point = [200, 255, 230]",
        config_path().display()
    )
)]
struct Cli {
    #[arg(value_name = "HEX_COLOR")]
    color: String,

    #[arg(short, long)]
    brightness: Option<u8>,

    #[arg(long)]
    no_save: bool,

    #[arg(long, value_name = "R,G,B", value_parser = parse_white_point)]
    white_point: Option<[u8; 3]>,
}

fn parse_white_point(s: &str) -> Result<[u8; 3], String> {
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
    if r == 0 || g == 0 || b == 0 {
        return Err("white point channels must be 1-255".into());
    }
    Ok([r, g, b])
}

fn hid_send(dev: &HidDevice, msg: &[u8], retries: u32) -> Result<[u8; MSG_LEN]> {
    ensure!(msg.len() <= MSG_LEN, "message must be <= {MSG_LEN} bytes");

    let mut buf = [0u8; MSG_LEN + 1];
    buf[1..=msg.len()].copy_from_slice(msg);

    let mut data = [0u8; MSG_LEN];

    for attempt in 0..retries {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(500));
        }
        if let Err(_e) = dev.write(&buf) {
            continue;
        }
        match dev.read_timeout(&mut data, 1000) {
            Ok(n) if n > 0 => return Ok(data),
            _ => {}
        }
    }

    bail!("failed to communicate with device after {retries} attempts");
}

fn is_rawhid(api: &HidApi, info: &DeviceInfo) -> bool {
    if info.usage_page() != 0xFF60 || info.usage() != 0x61 {
        return false;
    }
    let Ok(dev) = info.open_device(api) else {
        return false;
    };
    let Ok(data) = hid_send(&dev, &[0x01], 3) else {
        return false;
    };
    data[0..3] == [0x01, 0x00, 0x09]
}

fn is_vialrgb(api: &HidApi, info: &DeviceInfo) -> bool {
    let Ok(dev) = info.open_device(api) else {
        return false;
    };
    let Ok(data) = hid_send(&dev, &[0xFE, 0x00], 3) else {
        return false;
    };

    let vial_protocol = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let flags = data[12];

    vial_protocol >= 4 && (flags & 1) == 1
}

fn find_vial_device(api: &HidApi) -> Option<&DeviceInfo> {
    api.device_list().find(|info| {
        let serial = info.serial_number().unwrap_or("");
        serial.contains(VIAL_SERIAL_NUMBER_MAGIC) && is_rawhid(api, info) && is_vialrgb(api, info)
    })
}

fn vialrgb_get_modes(dev: &HidDevice) -> Result<BTreeSet<u16>> {
    let data = hid_send(dev, &[CMD_VIA_LIGHTING_GET_VALUE, VIALRGB_GET_INFO], 20)?;
    let rgb_version = u16::from_le_bytes([data[2], data[3]]);
    if rgb_version != 1 {
        bail!("unsupported Vial RGB protocol ({rgb_version})");
    }

    let mut effects = BTreeSet::from([0u16]);
    let mut max_effect: u16 = 0;

    for _ in 0..MAX_EFFECT_QUERY_ROUNDS {
        let mut msg = [0u8; MSG_LEN];
        msg[0] = CMD_VIA_LIGHTING_GET_VALUE;
        msg[1] = VIALRGB_GET_SUPPORTED;
        msg[2..4].copy_from_slice(&max_effect.to_le_bytes());
        let data = hid_send(dev, &msg, 1)?;

        for i in (2..MSG_LEN).step_by(2) {
            let value = u16::from_le_bytes([data[i], data[i + 1]]);
            if value != 0xFFFF {
                effects.insert(value);
            }
            max_effect = max_effect.max(value);
        }

        if max_effect == 0xFFFF {
            break;
        }
    }

    ensure!(
        max_effect == 0xFFFF,
        "device reported too many effects (>{MAX_EFFECT_QUERY_ROUNDS} rounds without terminator)"
    );

    Ok(effects)
}

fn vialrgb_set_mode(dev: &HidDevice, mode: u16, speed: u8, h: u8, s: u8, v: u8) -> Result<()> {
    let mut msg = [0u8; 8];
    msg[0] = CMD_VIA_LIGHTING_SET_VALUE;
    msg[1] = VIALRGB_SET_MODE;
    msg[2..4].copy_from_slice(&mode.to_le_bytes());
    msg[4] = speed;
    msg[5] = h;
    msg[6] = s;
    msg[7] = v;
    hid_send(dev, &msg, 20)?;
    Ok(())
}

fn vialrgb_save(dev: &HidDevice) -> Result<()> {
    hid_send(dev, &[CMD_VIA_LIGHTING_SAVE], 20)?;
    Ok(())
}

fn parse_hex_rgb(hex: &str) -> Result<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.is_ascii() {
        bail!("color must be 6 hex characters (0-9, a-f), e.g. ff00ff");
    }

    let r = u8::from_str_radix(&hex[0..2], 16).context("invalid red component")?;
    let g = u8::from_str_radix(&hex[2..4], 16).context("invalid green component")?;
    let b = u8::from_str_radix(&hex[4..6], 16).context("invalid blue component")?;

    Ok((r, g, b))
}

fn apply_white_point(r: u8, g: u8, b: u8, white_point: [u8; 3]) -> (u8, u8, u8) {
    let [wr, wg, wb] = white_point;
    // Scale each channel down proportionally: if white_point blue is 200, blue gets reduced to 200/255
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (
        (f64::from(r) * f64::from(wr) / 255.0).round() as u8,
        (f64::from(g) * f64::from(wg) / 255.0).round() as u8,
        (f64::from(b) * f64::from(wb) / 255.0).round() as u8,
    )
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
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

    // Values are guaranteed to be in 0.0..=255.0 by the HSV math above
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (
        (hue / 360.0 * 255.0).round() as u8,
        (sat * 255.0).round() as u8,
        (max * 255.0).round() as u8,
    )
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config();

    let white_point = cli.white_point.or(config.white_point);
    let (r, g, b) = parse_hex_rgb(&cli.color)?;
    let (r, g, b) = match white_point {
        Some(wp) => apply_white_point(r, g, b, wp),
        None => (r, g, b),
    };
    let (hue, sat, mut val) = rgb_to_hsv(r, g, b);

    if let Some(wp) = white_point {
        println!("Color correction: white_point = {wp:?}");
    }

    if let Some(brightness) = cli.brightness {
        val = brightness;
    }

    let api = HidApi::new().context("failed to initialize HID API")?;

    let info = find_vial_device(&api).context("no Vial RGB device found")?;

    println!(
        "Found: {} {}",
        info.manufacturer_string().unwrap_or("?"),
        info.product_string().unwrap_or("?"),
    );

    let dev = info.open_device(&api).context("failed to open device")?;

    let modes = vialrgb_get_modes(&dev)?;
    ensure!(
        modes.contains(&VIALRGB_EFFECT_SOLID_COLOR),
        "keyboard doesn't support solid color effect"
    );

    let color_hex = cli.color.trim_start_matches('#');
    println!("Setting color to #{color_hex} (HSV: {hue}, {sat}, {val})");

    vialrgb_set_mode(
        &dev,
        VIALRGB_EFFECT_SOLID_COLOR,
        DEFAULT_EFFECT_SPEED,
        hue,
        sat,
        val,
    )?;

    if cli.no_save {
        println!("Done! (not saved to EEPROM)");
    } else {
        vialrgb_save(&dev)?;
        println!("Done! (saved to EEPROM)");
    }

    Ok(())
}
