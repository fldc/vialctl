use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::color::WhitePoint;

#[derive(Deserialize, Default)]
struct RawConfig {
    white_point: Option<[u8; 3]>,
}

#[derive(Debug, Default)]
pub struct Config {
    pub white_point: Option<WhitePoint>,
}

pub fn path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vialctl")
        .join("config.toml")
}

pub fn load() -> Config {
    let path = path();

    let Ok(contents) = fs::read_to_string(&path) else {
        return Config::default();
    };

    let raw: RawConfig = match toml::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: ignoring invalid config {}: {e}", path.display());
            return Config::default();
        }
    };

    let white_point = raw.white_point.and_then(|rgb| {
        let wp = WhitePoint::new(rgb);
        if wp.is_none() {
            eprintln!(
                "warning: ignoring white_point in {}: channels must be 1-255",
                path.display()
            );
        }
        wp
    });

    Config { white_point }
}
