use std::collections::BTreeSet;
use std::thread;
use std::time::Duration;

use anyhow::{bail, ensure, Result};
use hidapi::{DeviceInfo, HidApi, HidDevice};

const MSG_LEN: usize = 32;

const VIAL_SERIAL_NUMBER_MAGIC: &str = "vial:f64c2b3c";

const VIALRGB_EFFECT_SOLID_COLOR: u16 = 2;

const CMD_VIA_LIGHTING_SET_VALUE: u8 = 0x07;
const CMD_VIA_LIGHTING_GET_VALUE: u8 = 0x08;
const CMD_VIA_LIGHTING_SAVE: u8 = 0x09;

const VIALRGB_GET_INFO: u8 = 0x40;
const VIALRGB_SET_MODE: u8 = 0x41;
const VIALRGB_GET_SUPPORTED: u8 = 0x42;

const DEFAULT_EFFECT_SPEED: u8 = 128;
const MAX_EFFECT_QUERY_ROUNDS: usize = 100;

fn hid_send(dev: &HidDevice, msg: &[u8], attempts: u32) -> Result<[u8; MSG_LEN]> {
    ensure!(msg.len() <= MSG_LEN, "message must be <= {MSG_LEN} bytes");

    let mut buf = [0u8; MSG_LEN + 1];
    buf[1..=msg.len()].copy_from_slice(msg);

    let mut last_err = None;
    let mut data = [0u8; MSG_LEN];

    for attempt in 0..attempts {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(500));
        }
        if let Err(e) = dev.write(&buf) {
            last_err = Some(e);
            continue;
        }
        match dev.read_timeout(&mut data, 1000) {
            Ok(n) if n > 0 => return Ok(data),
            Ok(_) => { /* timeout or zero-length read */ }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    match last_err {
        Some(e) => bail!("failed to communicate with device after {attempts} attempts: {e}"),
        None => bail!("failed to communicate with device after {attempts} attempts"),
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

pub fn find_device(api: &HidApi) -> Option<&DeviceInfo> {
    api.device_list().find(|info| {
        let serial = info.serial_number().unwrap_or("");
        serial.contains(VIAL_SERIAL_NUMBER_MAGIC) && is_rawhid(api, info) && is_vialrgb(api, info)
    })
}

fn get_modes(dev: &HidDevice) -> Result<BTreeSet<u16>> {
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
        let data = hid_send(dev, &msg, 3)?;

        for i in (2..MSG_LEN).step_by(2) {
            let value = u16::from_le_bytes([data[i], data[i + 1]]);
            if value == 0xFFFF {
                max_effect = 0xFFFF;
                break; // remaining slots are padding
            }
            effects.insert(value);
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

fn set_mode(dev: &HidDevice, mode: u16, speed: u8, h: u8, s: u8, v: u8) -> Result<()> {
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

fn save(dev: &HidDevice) -> Result<()> {
    hid_send(dev, &[CMD_VIA_LIGHTING_SAVE], 20)?;
    Ok(())
}

pub fn set_solid_color(dev: &HidDevice, h: u8, s: u8, v: u8, persist: bool) -> Result<()> {
    let modes = get_modes(dev)?;
    ensure!(
        modes.contains(&VIALRGB_EFFECT_SOLID_COLOR),
        "keyboard doesn't support solid color effect"
    );

    set_mode(
        dev,
        VIALRGB_EFFECT_SOLID_COLOR,
        DEFAULT_EFFECT_SPEED,
        h,
        s,
        v,
    )?;

    if persist {
        save(dev)?;
    }

    Ok(())
}
