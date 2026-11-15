#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

[[ "$(uname -s)" == Darwin ]] || { echo "Run this build on macOS." >&2; exit 1; }
arch="$(uname -m)"
[[ "$arch" == arm64 || "$arch" == x86_64 ]] || exit 1
cargo build --locked --release
version="$(target/release/compress --version)"
output="$PWD/dist/macos-$arch"
stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT
mkdir -p "$output"
app="$stage/Easy Media Compress.app"
contents="$app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
cp target/release/{EasyMediaCompress,compress} "$contents/MacOS/"
cp packaging/Info.plist "$contents/"
cp icon/Compress.icns README.md THIRD_PARTY.md "$contents/Resources/"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$contents/Info.plist"
ln -sfn /Applications "$stage/Applications"

codesign --force --sign - "$contents/MacOS/compress"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"

dmg="$output/EasyMediaCompress-macos-$arch.dmg"
hdiutil create -ov -format UDZO -volname 'Easy Media Compress' -srcfolder "$stage" "$dmg"
echo "$dmg"
