use std::collections::BTreeSet;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use hidapi::{DeviceInfo, HidApi, HidDevice};

const MSG_LEN: usize = 32;
const VIAL_SERIAL_NUMBER_MAGIC: &str = "vial:f64c2b3c";

const VIALRGB_EFFECT_SOLID_COLOR: u16 = 2;

const CMD_VIA_LIGHTING_SET_VALUE: u8 = 0x07;
const CMD_VIA_LIGHTING_GET_VALUE: u8 = 0x08;

const VIALRGB_GET_INFO: u8 = 0x40;
const VIALRGB_GET_SUPPORTED: u8 = 0x42;
const VIALRGB_SET_MODE: u8 = 0x41;

#[derive(Parser)]
#[command(
    about = "Set RGB color on a VialRGB keyboard",
    after_help = "Examples:\n  vialctl ff00ff\n  vialctl '#00ff00'\n  vialctl ff0000 --brightness 80"
)]
struct Cli {
    /// Hex color code, e.g. ff00ff or #ff00ff
    #[arg(value_name = "HEX_COLOR")]
    color: String,

    /// Override brightness/value (0-255). If not set, uses the color's own brightness.
    #[arg(short, long)]
    brightness: Option<u8>,
}

fn hid_send(dev: &HidDevice, msg: &[u8], retries: u32) -> Result<[u8; MSG_LEN]> {
    ensure!(msg.len() <= MSG_LEN, "message must be <= {MSG_LEN} bytes");

    // Pad message to MSG_LEN, with a leading 0x00 report ID byte
    let mut buf = [0u8; MSG_LEN + 1];
    buf[1..=msg.len()].copy_from_slice(msg);

    let mut data = [0u8; MSG_LEN];
    let mut last_err = None;

    for attempt in 0..retries {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(500));
        }
        match dev.write(&buf) {
            Ok(_) => {}
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
        match dev.read_timeout(&mut data, 1000) {
            Ok(n) if n > 0 => return Ok(data),
            Ok(_) => {}
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    match last_err {
        Some(e) => bail!("failed to communicate with device: {e}"),
        None => bail!("failed to communicate with device: no response"),
    }
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
        bail!("unsupported VialRGB protocol ({rgb_version})");
    }

    let mut effects = BTreeSet::from([0u16]);
    let mut max_effect: u16 = 0;

    loop {
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

#[allow(clippy::many_single_char_names)]
fn hex_to_hsv(hex: &str) -> Result<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.is_ascii() {
        bail!("color must be 6 hex characters (0-9, a-f), e.g. ff00ff");
    }

    let r = u8::from_str_radix(&hex[0..2], 16).context("invalid red component")?;
    let g = u8::from_str_radix(&hex[2..4], 16).context("invalid green component")?;
    let b = u8::from_str_radix(&hex[4..6], 16).context("invalid blue component")?;

    let rf = f64::from(r) / 255.0;
    let gf = f64::from(g) / 255.0;
    let bf = f64::from(b) / 255.0;

    let max = rf.max(gf).max(bf);
    let delta = max - rf.min(gf).min(bf);

    let hue = if delta < f64::EPSILON {
        0.0
    } else if (max - rf).abs() < f64::EPSILON {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if (max - gf).abs() < f64::EPSILON {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };

    let sat = if max < f64::EPSILON { 0.0 } else { delta / max };

    // Values are guaranteed to be in 0.0..=255.0 by the HSV math above
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (h, s, v) = (
        (hue / 360.0 * 255.0).round() as u8,
        (sat * 255.0).round() as u8,
        (max * 255.0).round() as u8,
    );

    Ok((h, s, v))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let (h, s, mut v) = hex_to_hsv(&cli.color)?;

    if let Some(brightness) = cli.brightness {
        v = brightness;
    }

    let api = HidApi::new().context("failed to initialize HID API")?;

    let info = find_vial_device(&api).context("no VialRGB device found")?;

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
    println!("Setting color to #{color_hex} (HSV: {h}, {s}, {v})");

    vialrgb_set_mode(&dev, VIALRGB_EFFECT_SOLID_COLOR, 128, h, s, v)?;

    println!("Done!");
    Ok(())
}
