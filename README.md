# vialctl

Set RGB color on a VialRGB keyboard. Works with split keyboards.

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
