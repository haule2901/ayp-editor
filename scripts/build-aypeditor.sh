#!/usr/bin/env bash
# Build AYPEditor.app (release) into $AYP_STAGE (default ~/Library/Caches/aypeditor); --install copies it to ~/Applications.
set -euo pipefail
cd "$(dirname "$0")/.."
# cargo from PATH; Homebrew's rustup only when none is on it
command -v cargo >/dev/null || export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
export CARGO_INCREMENTAL=0
# The repo's target/ unless CARGO_TARGET_DIR is set. To build on an external drive (the app is still
# assembled and signed on the internal disk, see below):
#   CARGO_TARGET_DIR="/Volumes/<drive>/aypeditor-target" scripts/build-aypeditor.sh --install
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

cargo build --release -p filmcraft --features whisper,vieneu-voices

# Assemble on the internal disk: codesign fails on exFAT (AppleDouble ._ files) when the target dir is an external SSD.
STAGE="${AYP_STAGE:-$HOME/Library/Caches/aypeditor}"
mkdir -p "$STAGE"
APP="$STAGE/AYPEditor.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$CARGO_TARGET_DIR/release/filmcraft" "$APP/Contents/MacOS/FilmCraft"
cp assets/app-icon/filmcraft.icns "$APP/Contents/Resources/FilmCraft.icns"
V=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys;print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="filmcraft"))')
sed -e "s/@VERSION@/$V/g" -e "s/@SHORT_VERSION@/$V/g" -e "s/@BUILD_SHA@/$(git rev-parse --short HEAD)/g" \
  packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"
P="$APP/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string vn.ayp.editor "$P"
plutil -replace CFBundleDisplayName -string AYPEditor "$P"
plutil -replace CFBundleName -string AYPEditor "$P"
plutil -insert LSEnvironment -json '{"FILMCRAFT_CONTROL_PORT":"9879","AYP_EDITOR":"1"}' "$P"
printf 'APPL????' > "$APP/Contents/PkgInfo"
codesign --force --deep -s - "$APP"
echo "Built $APP"

if [ "${1:-}" = "--install" ]; then
  DEST="$HOME/Applications/AYPEditor.app"
  mkdir -p "$HOME/Applications"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
  echo "Installed $DEST"
fi
