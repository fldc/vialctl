# vialctl

Set RGB color on keyboards running Vial firmware with RGB support. Works with split keyboards.

> **Note:** This utility currently supports setting solid colors. More features (effects, per-key control, etc.) will be added in future releases.

## Usage

```
vialctl ff00ff
vialctl '#00ff00'
vialctl ff0000 --brightness 80
```

## Build

```
cargo build --release
```

Requires `libhidapi-dev` (or equivalent) installed on your system.
