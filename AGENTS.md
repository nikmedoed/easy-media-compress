# AGENTS.md

Easy Media Compress is a Rust GUI and CLI. Keep entry points small and codec handling in `src/media.rs`.

- `src/gui.rs`: drag and drop, native file picker, queue and cancellation.
- `src/jobs.rs`: background scans, worker scheduling and output reservations.
- `src/cli.rs`: mixed media, image and legacy `5` modes.
- `src/settings.rs`: GUI preferences and legacy settings import.
- `build.ps1`, `scripts/setup.ps1`, `build-macos.sh`: packaging.
- `dev.ps1`: launch directly from source on Windows.
- `scripts/smoke.ps1`: integration checks against packaged codecs.

Preserve GUI launch with no arguments and `--gui <paths>` for Explorer. Process supported folders recursively. Keep originals and save results beside them with `_compressed`, `_converted`, or `_smaller` suffixes. Default video uses x264 CRF 30, slow, yuv420p and AAC. Lossy JPEG keeps resolution, applies orientation and uses quality 60. Original JPEG uses quality 95 and preserves EXIF/ICC where supported.

Release tags must match Cargo's version. The root Windows release asset named `EasyMediaCompress.exe` must be a self-contained migration bridge for the old EXE updater. The actual GUI in the installer and portable package requires its bundled tools.
