# Bundled tools

- [FFmpeg / ffprobe, Gyan essentials 8.1.2](https://github.com/GyanD/codexffmpeg/releases/tag/8.1.2): GPL build with libx264. See `tools/ffmpeg/LICENSE` and `README.txt`; upstream [FFmpeg source](https://github.com/FFmpeg/FFmpeg/tree/n8.1.2). Review corresponding source and licensing obligations before distributing the codec binaries.
- [ImageMagick 7.1.2-35 portable Q16 x64](https://download.imagemagick.org/archive/binaries/ImageMagick-7.1.2-35-portable-Q16-x64.7z): see `tools/imagemagick/LICENSE.txt` and `NOTICE.txt`. Uses built-in libheif, libjpeg-turbo and other image codecs.
- `vcomp140.dll`: Microsoft Visual C++ OpenMP redistributable from MSVC Build Tools, loaded beside `magick.exe` without a global install.
- Rust dependencies and versions are pinned in `Cargo.lock`.
