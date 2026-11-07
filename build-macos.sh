#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

[[ "$(uname -s)" == Darwin ]] || { echo "Run this build on macOS." >&2; exit 1; }
arch="$(uname -m)"
[[ "$arch" == arm64 || "$arch" == x86_64 ]] || exit 1
for tool in brew cargo dylibbundler pkg-config; do
    command -v "$tool" >/dev/null || { echo "Missing build tool: $tool" >&2; exit 1; }
done

cargo build --locked --release
version="$(target/release/compress --version)"
output="$PWD/dist/macos-$arch"
stage="$output/stage"
app="$stage/Easy Media Compress.app"
contents="$app/Contents"
tools="$contents/Resources/tools"
mkdir -p "$contents/MacOS" "$tools/ffmpeg" "$tools/imagemagick"
cp target/release/{EasyMediaCompress,compress} "$contents/MacOS/"
cp packaging/Info.plist "$contents/"
cp icon/Compress.icns README.md THIRD_PARTY.md "$contents/Resources/"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$contents/Info.plist"
ln -sfn /Applications "$stage/Applications"

ffmpeg="$(brew --prefix ffmpeg)"
cp -L "$ffmpeg/bin/ffmpeg" "$ffmpeg/bin/ffprobe" "$tools/ffmpeg/"
if [[ -f "$ffmpeg/COPYING.GPLv3" ]]; then
    cp "$ffmpeg/COPYING.GPLv3" "$tools/ffmpeg/"
fi

# Build ImageMagick with built-in coders so the app can be relocated without
# a Homebrew module directory. Its codec dylibs are bundled below.
im_version=7.1.2-35
im_sha=1d95f7d0ed11e91a3ee9e3042a8e248cb88c666dd9256ab14cc871b1f73138fd
cache="$PWD/.tools"
mkdir -p "$cache"
archive="$cache/ImageMagick-$im_version.7z"
if [[ ! -f "$archive" ]]; then
    curl -fL --retry 2 "https://github.com/ImageMagick/ImageMagick/releases/download/$im_version/ImageMagick-$im_version.7z" -o "$archive.part"
    echo "$im_sha  $archive.part" | shasum -a 256 -c -
    mv "$archive.part" "$archive"
fi
echo "$im_sha  $archive" | shasum -a 256 -c -
tar -xf "$archive" -C "$cache"
im_source="$cache/ImageMagick-$im_version"
(
    cd "$im_source"
    for formula in libheif jpeg-turbo libpng libtiff webp little-cms2; do
        export PKG_CONFIG_PATH="$(brew --prefix "$formula")/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    done
    ./configure --disable-shared --enable-static --without-modules --disable-openmp \
        --without-x --without-magick-plus-plus --without-perl \
        --with-heic=yes --with-jpeg=yes --with-png=yes --with-tiff=yes --with-webp=yes --with-lcms=yes
    make -j "$(sysctl -n hw.logicalcpu)" utilities/magick
)
cp "$im_source/utilities/magick" "$im_source/LICENSE" "$im_source/NOTICE" "$tools/imagemagick/"
cp packaging/policy.xml "$tools/imagemagick/"

# All tool executables sit one level below tools, alongside the dylib folder.
# The same install name also resolves correctly from inside tools/lib.
dylibbundler -ns -od -b \
    -x "$tools/ffmpeg/ffmpeg" -x "$tools/ffmpeg/ffprobe" -x "$tools/imagemagick/magick" \
    -d "$tools/lib" -p '@loader_path/../lib/'
while IFS= read -r -d '' binary; do
    if file "$binary" | grep -q 'Mach-O'; then
        codesign --force --sign - "$binary"
        codesign --verify --strict "$binary"
    fi
done < <(find "$tools" -type f -print0)
codesign --force --sign - "$contents/MacOS/compress"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"

# Exercise the packaged codecs and verify their dependencies stay in the app.
while IFS= read -r -d '' binary; do
    if file "$binary" | grep -q 'Mach-O'; then
        if otool -L "$binary" | tail -n +2 | grep -E '^[[:space:]]+(/opt/homebrew/|/usr/local/|@rpath/)'; then
            echo "Unbundled dependency in $binary" >&2
            exit 1
        fi
    fi
done < <(find "$tools" -type f -print0)
smoke="$(mktemp -d)"
trap 'rm -rf -- "$smoke"' EXIT
cp tests/fixtures/sample.heic "$smoke/"
"$tools/ffmpeg/ffmpeg" -hide_banner -loglevel error -f lavfi \
    -i 'testsrc2=size=128x96:rate=15:duration=1' -c:v libx264 "$smoke/clip.mp4"
env -u MEDIA_COMPRESS_MAGICK_DIR -u MEDIA_COMPRESS_FFMPEG_DIR PATH=/usr/bin:/bin \
    "$contents/MacOS/compress" "$smoke"
test -s "$smoke/clip_compressed.mp4"
test -s "$smoke/sample_compressed.jpg"

dmg="$output/EasyMediaCompress-macos-$arch.dmg"
hdiutil create -ov -format UDZO -volname 'Easy Media Compress' -srcfolder "$stage" "$dmg"
echo "$dmg"
