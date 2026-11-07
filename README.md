# Easy Media Compress — Rust

Easy Media Compress is a Rust app for drag-and-drop video compression and photo conversion. Windows and macOS packages include their codecs.

On Windows, use [Windows Setup](https://github.com/nikmedoed/easy-media-compress/releases/latest/download/EasyMediaCompress-Setup-x64.exe) for a per-user install. Setup creates a Start menu shortcut and offers an Explorer action and a `compress` command on the user PATH. In Explorer, right-click a media file or folder and choose **Easy Media Compress**. On Windows 11 this action is under **Show more options**. It opens the GUI, adds the selection to the queue and uses the saved profiles. Remove the installed app from **Settings → Apps → Installed apps**.

For portable use, extract the entire Windows ZIP and open its `EasyMediaCompress.exe`. Keep the `tools` folder beside it.

On macOS, open the disk image and drag **Easy Media Compress** to **Applications**. The CLI is in the app under `Contents/MacOS/compress`.

The Linux archive requires `ffmpeg`, `ffprobe` and ImageMagick 7 (`magick` with HEIC support) on PATH.

Drop files or folders onto the window, or use **Add Media** to open the system file picker. Processing starts automatically and results appear beside the inputs. On Windows, subsequent Explorer calls add files to the open queue. Profiles and window size are saved in `%LOCALAPPDATA%\EasyMediaCompressRust\settings.json`. On first launch the app reads the Python version's settings if available.

From the portable folder, use the console executable for scripting:

```powershell
.\compress.exe 'D:\Media'
.\compress.exe 5 'D:\Videos'
.\compress.exe 'D:\Videos\clip.mp4' -crf 28 -preset slow --video-profile crf720
.\compress.exe image 'D:\Photos' --mode original --format webp
.\compress.exe --help
```

No arguments, `gui`, or `--gui` opens the window. Paths after `--gui` go directly into the visible queue. Folders are searched recursively. Double-click a filename to open its folder. **<>** in the **Alt** column runs the alternate profile.

**Stop** cancels the active and waiting jobs. When closing the window during conversion, choose **Stop and close** or **Keep open**. Completed outputs remain on disk.

- Video: MP4 (x264 CRF 30, preset slow, yuv420p, AAC), 480p/720p on the short side without upscaling, approximate 5 MB target (original 4.5 MiB one-pass budget), and GIFs at 15 fps with 120/180/240/480 px short sides.
- Photo: JPEG/JFIF, PNG, BMP, TIFF, WebP, HEIC/HEIF and AVIF inputs; JPEG, PNG and WebP outputs. The GUI uses JPEG. No resizing. EXIF orientation is applied, JPEG transparency is flattened onto white, and original JPEG mode uses quality 95 with full chroma and preserves EXIF/ICC where supported. Lossy JPEG uses quality 60. Different codec libraries can produce different bytes from Pillow.
- Outputs end in `_compressed`, `_converted`, or `_smaller`. If a name is taken, a numbered variant is created (e.g. `photo (2)_compressed.jpg`). Sources and previous results are kept.

The portable-only `win_install.ps1` registers a shortcut and an Explorer action that point at the current folder. Use `-Uninstall` to remove those registrations. The Setup executable is the standard Windows installation route.

## Upgrade from 0.2.x

The old Windows standalone EXE can receive the release asset named `EasyMediaCompress.exe` through its existing updater. That asset is a migration launcher with Setup embedded. On first run it installs the Rust app and opens it. Later launches through the same path open the installed app, so an existing shortcut to the old standalone EXE keeps working. Running Windows Setup directly also works.

The Rust installer upgrades the existing installation in place. Close the app before running a new Setup. Installed Windows builds check for a newer release in the background and offer a link to its installer. Portable users can extract the new ZIP into a separate folder and macOS users can replace the app from the new DMG.

The new app imports legacy GUI preferences on first launch. For Python installs made from a source checkout, switch to the new Start menu shortcut and remove any PowerShell profile function that still calls the old `compress.py` or `compress.bat`.

## Run from source

On Windows, run `.\dev.ps1`. It prepares the portable codecs on the first run and opens a debug GUI using Cargo. With a custom Visual Studio installation, pass `-VcVars 'D:\path\to\vcvars64.bat'`. Rerun after editing source to see changes.

On macOS or Linux, put FFmpeg, ffprobe and ImageMagick 7 on PATH and run `cargo run --locked`. To try the CLI, use `cargo run --locked --bin compress -- --help`. The `MEDIA_COMPRESS_FFMPEG_DIR` and `MEDIA_COMPRESS_MAGICK_DIR` environment variables override tool resolution.

## Build

Use Rust stable, Visual Studio C++ Build Tools with a Windows SDK, `curl.exe`, and `tar.exe`. These are build-time requirements only.

```powershell
.\build.ps1
# For a nonstandard Visual Studio install, supply the x64 environment script:
.\build.ps1 -VcVars 'D:\path\to\vcvars64.bat'
.\scripts\smoke.ps1
.\scripts\setup.ps1  # requires Inno Setup 6.3+ on the build machine
```

The build script verifies pinned codec archives and stages the Windows x64 portable folder under `dist\EasyMediaCompress`. It locates the OpenMP DLL in the Build Tools redistributable files, or accepts a path via `-OpenMpDll`. Build downloads are cached in `.tools`.

For macOS, run this on the target architecture with Xcode command-line tools and Rust stable:

```sh
brew install pkgconf dylibbundler ffmpeg libheif jpeg-turbo libpng libtiff webp little-cms2
bash build-macos.sh
```

The script packages the app, CLI, and codecs for the build machine's architecture, tests the bundled tools, and creates a disk image in `dist/macos-<arch>`. Homebrew is used on the build machine. The disk image uses an ad hoc signature. Public distribution through Gatekeeper requires a Developer ID signature and notarization.

To release, set the version in `Cargo.toml`, update `Cargo.lock`, then tag the matching commit with `v<version>`. The release workflow checks the version and builds the Windows Setup, portable ZIP and migration EXE, the two macOS disk images and the Linux archive. It publishes only after all builds succeed.
