mod cli;
mod color;
mod config;
mod hid;

use anyhow::{Context, Result};
use clap::Parser;
use hidapi::HidApi;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let config = config::load();

    let white_point = cli.white_point.or(config.white_point);

    let (r, g, b) = color::parse_hex_rgb(&cli.color)?;
    let (r, g, b) = match white_point {
        Some(wp) => {
            let corrected = wp.apply(r, g, b);
            println!(
                "Color correction: white_point = {:?}, RGB({r},{g},{b}) -> RGB({},{},{})",
                wp.0, corrected.0, corrected.1, corrected.2
            );
            corrected
        }
        None => (r, g, b),
    };

    let (hue, sat, mut val) = color::rgb_to_hsv(r, g, b);
    if let Some(brightness) = cli.brightness {
        if brightness == 0 {
            eprintln!("warning: brightness 0 will turn the LEDs off");
        }
        val = brightness;
    }

    let api = HidApi::new().context("failed to initialize HID API")?;
    let info = hid::find_device(&api).context("no Vial RGB device found")?;

    println!(
        "Found: {} {}",
        info.manufacturer_string().unwrap_or("?"),
        info.product_string().unwrap_or("?"),
    );

    let dev = info.open_device(&api).context("failed to open device")?;
    let persist = !cli.no_save;
    let color_hex = cli.color.trim_start_matches('#');

    println!("Setting color to #{color_hex} (HSV: {hue}, {sat}, {val})");

    hid::set_solid_color(&dev, hue, sat, val, persist)?;

    if persist {
        println!("Done! (saved to EEPROM)");
    } else {
        println!("Done! (not saved to EEPROM)");
    }

    Ok(())
}
