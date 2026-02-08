# vialctl

Set RGB color on keyboards running Vial firmware with RGB support. Works with split keyboards.

Developed for use with [Omarchy](https://github.com/basecamp/omarchy) keyboard RGB theme settings.

> **Note:** This utility currently supports setting solid colors. More features (effects, per-key control, etc.) will be added in future releases.

## Usage

```
vialctl ff00ff
vialctl '#00ff00'
vialctl ff0000 --brightness 80
vialctl ff00ff --white-point 200,255,230
```

## Color correction

Set a white point to compensate for LED color bias. Use `--white-point` or add to `~/.config/vialctl/config.toml`:

```toml
white_point = [255, 255, 230]
```

## Build

```
cargo build --release
```

Requires `libhidapi-dev` (or equivalent) installed on your system.
