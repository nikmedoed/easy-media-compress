import argparse
import os
import shutil
import subprocess
import sys
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD_VERSION_FILE = ROOT / "compress_tool" / "_build_version.py"
DEFAULT_VERSION = "0.1.0"


def data_arg(source: Path, dest: str) -> str:
    separator = ";" if os.name == "nt" else ":"
    return f"{source}{separator}{dest}"


def run(cmd: list[str]) -> None:
    print(" ".join(str(part) for part in cmd))
    subprocess.run(cmd, check=True, cwd=ROOT)


def output(cmd: list[str]) -> str | None:
    try:
        result = subprocess.run(
            cmd,
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip() or None


def normalize_version(version: str) -> str:
    return version.strip().lstrip("vV")


def resolve_build_version(explicit_version: str | None = None) -> str:
    if explicit_version:
        return normalize_version(explicit_version)

    github_ref = os.environ.get("GITHUB_REF_NAME")
    if github_ref and github_ref.startswith(("v", "V")):
        return normalize_version(github_ref)

    git_tag = output(["git", "describe", "--tags", "--abbrev=0"])
    if git_tag:
        return normalize_version(git_tag)

    return DEFAULT_VERSION


@contextmanager
def embedded_build_version(version: str):
    old_content = BUILD_VERSION_FILE.read_text(encoding="utf-8") if BUILD_VERSION_FILE.exists() else None
    BUILD_VERSION_FILE.write_text(f'__version__ = "{version}"\n', encoding="utf-8")
    try:
        yield
    finally:
        if old_content is None:
            BUILD_VERSION_FILE.unlink(missing_ok=True)
        else:
            BUILD_VERSION_FILE.write_text(old_content, encoding="utf-8")


def icon_path() -> Path | None:
    if sys.platform.startswith("win"):
        return ROOT / "icon" / "icon.ico"
    if sys.platform == "darwin":
        return ROOT / "icon" / "Compress.icns"
    return None


def build_target(name: str, entry: Path, *, onedir: bool) -> None:
    mode = "--onedir" if onedir else "--onefile"
    pyinstaller_args = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--noconfirm",
        "--clean",
        mode,
        "--name",
        name,
        "--windowed",
        "--add-data",
        data_arg(ROOT / "icon", "icon"),
        "--hidden-import",
        "pillow_heif",
        "--hidden-import",
        "tkinterdnd2",
        "--collect-all",
        "pillow_heif",
        "--collect-data",
        "tkinterdnd2",
    ]

    icon = icon_path()
    if icon and icon.exists():
        pyinstaller_args.extend(["--icon", str(icon)])

    pyinstaller_args.append(str(entry))
    run(pyinstaller_args)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--name", default="EasyMediaCompress")
    parser.add_argument("--onedir", action="store_true")
    parser.add_argument("--install-deps", action="store_true")
    parser.add_argument("--version", help="Version to embed in the packaged app.")
    args = parser.parse_args()
    version = resolve_build_version(args.version)

    if args.install_deps:
        run([sys.executable, "-m", "pip", "install", "--upgrade", "pip"])
        run(
            [
                sys.executable,
                "-m",
                "pip",
                "install",
                "-r",
                "requirements.txt",
                "-r",
                "requirements-build.txt",
            ]
        )

    run([sys.executable, "-c", "from compress_tool.ui import ensure_icon_ico; ensure_icon_ico()"])

    dist_dir = ROOT / "dist"
    if dist_dir.exists():
        try:
            shutil.rmtree(dist_dir)
        except PermissionError as exc:
            raise SystemExit(
                "Could not clean dist/. Close any running EasyMediaCompress executable, "
                "then run the build again."
            ) from exc

    print(f"Embedding version: {version}")
    with embedded_build_version(version):
        build_target(args.name, ROOT / "compress.py", onedir=args.onedir)

    print(f"Build complete: {ROOT / 'dist'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
