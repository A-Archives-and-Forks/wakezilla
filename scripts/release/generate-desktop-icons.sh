#!/usr/bin/env sh
set -eu

LC_ALL=C
export LC_ALL
umask 022

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
ASSET_DIR="$REPO_ROOT/assets/desktop"
MASTER="$ASSET_DIR/wakezilla-1024.png"
SIPS=/usr/bin/sips
ICONUTIL=/usr/bin/iconutil

if [ ! -f "$MASTER" ]; then
  printf 'master icon not found: %s\n' "$MASTER" >&2
  exit 1
fi
if [ ! -x "$SIPS" ]; then
  printf 'required macOS tool not found: %s\n' "$SIPS" >&2
  exit 1
fi
if [ ! -x "$ICONUTIL" ]; then
  printf 'required macOS tool not found: %s\n' "$ICONUTIL" >&2
  exit 1
fi
if ! PYTHON3=$(command -v python3); then
  printf 'required command not found: python3\n' >&2
  exit 1
fi

WORK_DIR=$(mktemp -d "$ASSET_DIR/.generate-desktop-icons.XXXXXX")
trap 'rm -rf "$WORK_DIR"' 0 HUP INT TERM

PNG_DIR="$WORK_DIR/png"
ICONSET_DIR="$WORK_DIR/Wakezilla.iconset"
OUTPUT_DIR="$WORK_DIR/output"
mkdir -p "$PNG_DIR" "$ICONSET_DIR" "$OUTPUT_DIR"

for size in 16 32 48 64 128 256 512 1024; do
  "$SIPS" --resampleHeightWidth "$size" "$size" "$MASTER" \
    --out "$PNG_DIR/$size.png" >/dev/null
done

cp "$PNG_DIR/16.png" "$ICONSET_DIR/icon_16x16.png"
cp "$PNG_DIR/32.png" "$ICONSET_DIR/icon_16x16@2x.png"
cp "$PNG_DIR/32.png" "$ICONSET_DIR/icon_32x32.png"
cp "$PNG_DIR/64.png" "$ICONSET_DIR/icon_32x32@2x.png"
cp "$PNG_DIR/128.png" "$ICONSET_DIR/icon_128x128.png"
cp "$PNG_DIR/256.png" "$ICONSET_DIR/icon_128x128@2x.png"
cp "$PNG_DIR/256.png" "$ICONSET_DIR/icon_256x256.png"
cp "$PNG_DIR/512.png" "$ICONSET_DIR/icon_256x256@2x.png"
cp "$PNG_DIR/512.png" "$ICONSET_DIR/icon_512x512.png"
cp "$PNG_DIR/1024.png" "$ICONSET_DIR/icon_512x512@2x.png"

"$ICONUTIL" --convert icns --output "$OUTPUT_DIR/Wakezilla.icns" "$ICONSET_DIR"

"$PYTHON3" - \
  "$OUTPUT_DIR/wakezilla.ico" \
  "$PNG_DIR/16.png" \
  "$PNG_DIR/32.png" \
  "$PNG_DIR/48.png" \
  "$PNG_DIR/256.png" <<'PYTHON'
from pathlib import Path
import struct
import sys


output = Path(sys.argv[1])
expected_sizes = (16, 32, 48, 256)
inputs = [Path(value) for value in sys.argv[2:]]
if len(inputs) != len(expected_sizes):
    raise SystemExit("expected exactly four ICO input images")

images = []
for expected_size, path in zip(expected_sizes, inputs):
    payload = path.read_bytes()
    if len(payload) < 24 or payload[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"ICO input is not a PNG: {path}")
    if payload[12:16] != b"IHDR":
        raise SystemExit(f"ICO input has no leading IHDR chunk: {path}")
    width, height = struct.unpack(">II", payload[16:24])
    if (width, height) != (expected_size, expected_size):
        raise SystemExit(
            f"ICO input {path} is {width}x{height}, expected "
            f"{expected_size}x{expected_size}"
        )
    images.append((expected_size, payload))

directory_offset = 6 + 16 * len(images)
directory = bytearray()
payloads = bytearray()
for size, payload in images:
    encoded_size = 0 if size == 256 else size
    directory.extend(
        struct.pack(
            "<BBBBHHII",
            encoded_size,
            encoded_size,
            0,
            0,
            1,
            32,
            len(payload),
            directory_offset + len(payloads),
        )
    )
    payloads.extend(payload)

ico = bytearray(struct.pack("<HHH", 0, 1, len(images)))
ico.extend(directory)
ico.extend(payloads)
output.write_bytes(ico)
PYTHON

for size in 48 128 256; do
  destination="$ASSET_DIR/hicolor/${size}x${size}/apps"
  mkdir -p "$destination"
  mv "$PNG_DIR/$size.png" "$destination/dev.wakezilla.Wakezilla.png"
done

chmod 0644 "$OUTPUT_DIR/Wakezilla.icns" "$OUTPUT_DIR/wakezilla.ico"
mv "$OUTPUT_DIR/Wakezilla.icns" "$ASSET_DIR/Wakezilla.icns"
mv "$OUTPUT_DIR/wakezilla.ico" "$ASSET_DIR/wakezilla.ico"

printf 'Generated desktop icons from %s\n' "$MASTER"
