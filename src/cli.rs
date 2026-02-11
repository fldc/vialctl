use clap::Parser;

use crate::color::{parse_white_point, WhitePoint};
use crate::config;

#[derive(Parser)]
#[command(
    version,
    about = "Set RGB color on keyboards running Vial firmware with RGB support",
    after_help = format!(
        "Examples:\n  vialctl ff00ff\n  vialctl '#00ff00'\n  vialctl ff0000 --brightness 80\n\n\
         Config: {}\n  Example:\n    white_point = [200, 255, 230]",
        config::path().display()
    )
)]
pub struct Cli {
    #[arg(value_name = "HEX_COLOR")]
    pub color: String,

    #[arg(short, long)]
    pub brightness: Option<u8>,

    #[arg(long)]
    pub no_save: bool,

    #[arg(long, value_name = "R,G,B", value_parser = parse_white_point)]
    pub white_point: Option<WhitePoint>,
}
