from __future__ import annotations

import ctypes
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from . import __version__
from .constants import APP_DIR_NAME, CREATE_NO_WINDOW

GITHUB_API_LATEST_RELEASE = (
    "https://api.github.com/repos/nikmedoed/easy-media-compress/releases/latest"
)
CHECK_INTERVAL_SECONDS = 12 * 60 * 60
FAILED_CHECK_INTERVAL_SECONDS = 60 * 60
PARENT_WAIT_TIMEOUT_SECONDS = 6 * 60 * 60
REPLACE_RETRY_SECONDS = 10 * 60
LOCK_STALE_SECONDS = PARENT_WAIT_TIMEOUT_SECONDS + REPLACE_RETRY_SECONDS + 60
HELPER_NAME = "EasyMediaCompressUpdater.exe"
USER_AGENT = f"{APP_DIR_NAME}-updater/{__version__}"


class UpdateError(RuntimeError):
    """Raised when update checks or installs fail."""


@dataclass(frozen=True)
class UpdateInfo:
    version: str
    tag_name: str
    page_url: str
    asset_name: str
    asset_url: str
    asset_size: int = 0
    asset_digest: str = ""


@dataclass(frozen=True)
class StagedUpdate:
    version: str
    path: Path


class UpdateLock:
    def __init__(self, path: Path | None = None) -> None:
        self.path = path or _update_dir() / "updater.lock"
        self._owned = False

    def acquire(self) -> bool:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        try:
            stale = self.path.exists() and time.time() - self.path.stat().st_mtime > LOCK_STALE_SECONDS
        except OSError:
            stale = False
        if stale:
            try:
                self.path.unlink()
            except OSError:
                return False
        try:
            fd = os.open(str(self.path), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            return False
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(str(os.getpid()))
        self._owned = True
        return True

    def release(self) -> None:
        if not self._owned:
            return
        try:
            self.path.unlink()
        except OSError:
            pass
        self._owned = False


def current_version() -> str:
    return __version__


def is_frozen_app() -> bool:
    return bool(getattr(sys, "frozen", False)) and Path(sys.executable).exists()


def start_background_updater(current: str | None = None) -> None:
    if sys.platform != "win32" or not is_frozen_app():
        return

    target = Path(sys.executable).resolve()
    if target.name.casefold() == HELPER_NAME.casefold() or not target.exists():
        return

    update_dir = _update_dir()
    update_dir.mkdir(parents=True, exist_ok=True)
    helper = update_dir / HELPER_NAME
    if not _prepare_helper(target, helper):
        return

    creationflags = 0x00000008 | 0x00000200 | CREATE_NO_WINDOW
    args = [
        str(helper),
        "--background-update",
        "--parent-pid",
        str(os.getpid()),
        "--target-exe",
        str(target),
        "--current-version",
        current or __version__,
    ]
    try:
        subprocess.Popen(
            args,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
            creationflags=creationflags,
        )
    except OSError:
        return


def run_background_update(parent_pid: int, target_exe: str, current: str) -> int:
    target = Path(target_exe).resolve()
    if sys.platform != "win32" or not target.name.lower().endswith(".exe"):
        return 0

    lock = UpdateLock()
    if not lock.acquire():
        return 0

    try:
        staged = find_staged_update(current)
        if staged is None and should_check_for_update():
            staged = _check_and_download(current)
        if staged is not None:
            _wait_for_process_exit(parent_pid, PARENT_WAIT_TIMEOUT_SECONDS)
            if _replace_with_retry(target, staged.path):
                _mark_applied(staged.version)
    finally:
        lock.release()
    return 0


def should_check_for_update(now: float | None = None) -> bool:
    now = time.time() if now is None else now
    state = _read_json(_state_file())
    last_check = float(state.get("last_check_at", 0) or 0)
    last_error = float(state.get("last_error_at", 0) or 0)
    interval = FAILED_CHECK_INTERVAL_SECONDS if last_error >= last_check else CHECK_INTERVAL_SECONDS
    return now - last_check >= interval


def check_for_update() -> UpdateInfo | None:
    if not is_frozen_app():
        return None
    return fetch_latest_release(__version__)


def fetch_latest_release(current: str, *, timeout: int = 6) -> UpdateInfo | None:
    request = urllib.request.Request(
        GITHUB_API_LATEST_RELEASE,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": USER_AGENT,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except (OSError, urllib.error.URLError, json.JSONDecodeError) as exc:
        raise UpdateError(f"Could not check for updates: {exc}") from exc
    return release_from_payload(payload, current)


def release_from_payload(payload: dict[str, Any], current: str) -> UpdateInfo | None:
    if payload.get("draft") or payload.get("prerelease"):
        return None
    tag_name = str(payload.get("tag_name") or payload.get("name") or "").strip()
    if not tag_name or not is_newer_version(tag_name, current):
        return None

    asset = select_release_asset(payload.get("assets", []))
    if asset is None:
        return None

    return UpdateInfo(
        version=tag_name.lstrip("vV"),
        tag_name=tag_name,
        page_url=str(payload.get("html_url") or ""),
        asset_name=str(asset.get("name") or ""),
        asset_url=str(asset.get("browser_download_url") or ""),
        asset_size=int(asset.get("size") or 0),
        asset_digest=str(asset.get("digest") or ""),
    )


def select_release_asset(assets: list[dict[str, Any]]) -> dict[str, Any] | None:
    candidates: list[dict[str, Any]] = []
    for asset in assets:
        name = str(asset.get("name") or "")
        url = str(asset.get("browser_download_url") or "")
        if name.lower().endswith(".exe") and url:
            candidates.append(asset)
    if not candidates:
        return None

    def score(asset: dict[str, Any]) -> tuple[int, int]:
        lowered = str(asset.get("name") or "").casefold()
        exact = 2 if lowered == "easymediacompress.exe" else 0
        named = 1 if "easymediacompress" in lowered or "media" in lowered else 0
        return exact, named

    return sorted(candidates, key=score, reverse=True)[0]


def is_newer_version(remote: str, current: str) -> bool:
    remote_parts = _parse_version(remote)
    current_parts = _parse_version(current)
    if not remote_parts or not current_parts:
        return False
    size = max(len(remote_parts), len(current_parts))
    remote_norm = remote_parts + (0,) * (size - len(remote_parts))
    current_norm = current_parts + (0,) * (size - len(current_parts))
    return remote_norm > current_norm


def find_staged_update(current: str) -> StagedUpdate | None:
    metadata = _read_json(_metadata_file())
    version = str(metadata.get("version") or "")
    file_name = str(metadata.get("file") or "")
    if not version or not file_name or not is_newer_version(version, current):
        return None
    path = _update_dir() / file_name
    if path.exists():
        return StagedUpdate(version=version, path=path)
    return None


def download_and_install_update(
    update: UpdateInfo,
    *,
    relaunch_args: list[str] | None = None,
    progress: Callable[[int, int | None], None] | None = None,
) -> None:
    if not is_frozen_app():
        raise UpdateError("Updates are available only in packaged builds.")
    staged = _download_asset(update, progress=progress)
    _wait_for_process_exit(os.getpid(), 0)
    target = Path(sys.executable).resolve()
    if sys.platform == "win32" and staged.path.suffix.lower() == ".exe":
        _start_windows_replace(staged.path, target, relaunch_args or [])
        return
    raise UpdateError("Automatic replacement is currently supported only for Windows .exe builds.")


def _check_and_download(current: str) -> StagedUpdate | None:
    _write_state(last_check_at=time.time())
    try:
        release = fetch_latest_release(current)
        if release is None:
            _write_state(last_error_at=None)
            return None
        state = _read_json(_state_file())
        if state.get("installed_version") == release.tag_name:
            _write_state(last_error_at=None)
            return None
        staged = _download_asset(release)
        _write_state(latest_version=release.tag_name, update_pending=True, last_error_at=None)
        return staged
    except Exception as exc:
        _write_state(last_error_at=time.time(), last_error=str(exc)[:500])
        return None


def _download_asset(
    update: UpdateInfo,
    *,
    progress: Callable[[int, int | None], None] | None = None,
    timeout: int = 12,
) -> StagedUpdate:
    update_dir = _update_dir()
    update_dir.mkdir(parents=True, exist_ok=True)
    safe_version = re.sub(r"[^A-Za-z0-9_.-]+", "_", update.tag_name).strip("._") or "latest"
    final = update_dir / f"{APP_DIR_NAME}-{safe_version}.exe"
    temp = final.with_suffix(".download")
    sha256 = hashlib.sha256()
    downloaded = 0
    first = b""

    request = urllib.request.Request(update.asset_url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response, temp.open("wb") as handle:
            total_header = response.headers.get("Content-Length")
            total = int(total_header) if total_header and total_header.isdigit() else update.asset_size or None
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                if not first:
                    first = chunk[:2]
                sha256.update(chunk)
                downloaded += len(chunk)
                handle.write(chunk)
                if progress:
                    progress(downloaded, total)
    except (OSError, urllib.error.URLError) as exc:
        temp.unlink(missing_ok=True)
        raise UpdateError(f"Could not download update: {exc}") from exc

    if first != b"MZ":
        temp.unlink(missing_ok=True)
        raise UpdateError("Downloaded asset is not a Windows executable.")
    if update.asset_size and downloaded != update.asset_size:
        temp.unlink(missing_ok=True)
        raise UpdateError("Downloaded asset size does not match release metadata.")
    digest = update.asset_digest
    if digest.startswith("sha256:") and digest.removeprefix("sha256:").casefold() != sha256.hexdigest().casefold():
        temp.unlink(missing_ok=True)
        raise UpdateError("Downloaded asset checksum does not match release metadata.")

    os.replace(temp, final)
    _metadata_file().write_text(
        json.dumps(
            {
                "version": update.tag_name,
                "file": final.name,
                "sha256": sha256.hexdigest(),
                "release_url": update.page_url,
                "downloaded_at": time.time(),
            },
            indent=2,
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )
    return StagedUpdate(version=update.tag_name, path=final)


def _prepare_helper(target: Path, helper: Path) -> bool:
    try:
        if helper.exists() and helper.stat().st_size == target.stat().st_size:
            return True
        temp = helper.with_suffix(".tmp")
        shutil.copy2(target, temp)
        os.replace(temp, helper)
        return True
    except OSError:
        return helper.exists()


def _replace_with_retry(target: Path, staged: Path) -> bool:
    deadline = time.time() + REPLACE_RETRY_SECONDS
    while True:
        try:
            _replace_executable(target, staged)
            return True
        except OSError as exc:
            _write_state(last_apply_error=str(exc)[:500], last_apply_error_at=time.time())
            if time.time() >= deadline:
                return False
            time.sleep(3)


def _replace_executable(target: Path, staged: Path) -> None:
    update_dir = _update_dir()
    update_dir.mkdir(parents=True, exist_ok=True)
    previous = update_dir / f"{target.stem}.previous.exe"
    temp_previous = update_dir / f"{target.stem}.previous.tmp"
    temp_previous.unlink(missing_ok=True)
    previous.unlink(missing_ok=True)
    os.replace(target, temp_previous)
    try:
        os.replace(staged, target)
    except OSError:
        os.replace(temp_previous, target)
        raise
    os.replace(temp_previous, previous)


def _start_windows_replace(staged: Path, target: Path, relaunch_args: list[str]) -> None:
    quoted_args = " ".join(f'"{arg}"' for arg in relaunch_args)
    ps_source = str(staged).replace("'", "''")
    ps_target = str(target).replace("'", "''")
    ps_args = quoted_args.replace("'", "''")
    script = staged.with_suffix(".ps1")
    script.write_text(
        "\n".join(
            [
                "$ErrorActionPreference = 'Stop'",
                f"$pidToWait = {os.getpid()}",
                f"$source = '{ps_source}'",
                f"$target = '{ps_target}'",
                "Wait-Process -Id $pidToWait -ErrorAction SilentlyContinue",
                "Start-Sleep -Milliseconds 500",
                "$backup = \"$target.old\"",
                "Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue",
                "try {",
                "    Copy-Item -LiteralPath $target -Destination $backup -Force",
                "    Copy-Item -LiteralPath $source -Destination $target -Force",
                "    Remove-Item -LiteralPath $source -Force -ErrorAction SilentlyContinue",
                f"    Start-Process -FilePath $target -ArgumentList '{ps_args}' -WindowStyle Hidden",
                "    Start-Sleep -Seconds 3",
                "    Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue",
                "} catch {",
                "    if (Test-Path -LiteralPath $backup) {",
                "        Copy-Item -LiteralPath $backup -Destination $target -Force -ErrorAction SilentlyContinue",
                "    }",
                "    $log = Join-Path ([IO.Path]::GetTempPath()) 'EasyMediaCompress-update.log'",
                "    Add-Content -LiteralPath $log -Value $_.Exception.Message",
                "}",
                "Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue",
                "",
            ]
        ),
        encoding="utf-8-sig",
    )
    subprocess.Popen(
        [
            "powershell",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            str(script),
        ],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=CREATE_NO_WINDOW,
        close_fds=True,
    )


def _mark_applied(version: str) -> None:
    _metadata_file().unlink(missing_ok=True)
    _write_state(
        installed_version=version,
        update_pending=False,
        last_applied_at=time.time(),
        last_apply_error=None,
        last_apply_error_at=None,
    )


def _wait_for_process_exit(pid: int, timeout: int) -> None:
    if pid <= 0:
        return
    deadline = time.time() + timeout
    while time.time() < deadline and _process_exists(pid):
        time.sleep(2)


def _process_exists(pid: int) -> bool:
    if pid <= 0:
        return False
    if sys.platform == "win32":
        handle = ctypes.windll.kernel32.OpenProcess(0x1000, False, int(pid))
        if not handle:
            return False
        ctypes.windll.kernel32.CloseHandle(handle)
        return True
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def _parse_version(value: str) -> tuple[int, ...]:
    match = re.search(r"\d+(?:\.\d+){0,3}", value)
    if not match:
        return ()
    return tuple(int(part) for part in match.group(0).split("."))


def _update_dir() -> Path:
    if sys.platform.startswith("win"):
        base = os.getenv("LOCALAPPDATA") or os.getenv("APPDATA")
        if base:
            return Path(base) / APP_DIR_NAME / "updates"
    return Path.home() / ".cache" / "easy-media-compress" / "updates"


def _state_file() -> Path:
    return _update_dir() / "state.json"


def _metadata_file() -> Path:
    return _update_dir() / "staged.json"


def _read_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}


def _write_state(**updates: Any) -> None:
    path = _state_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    state = _read_json(path)
    for key, value in updates.items():
        if value is None:
            state.pop(key, None)
        else:
            state[key] = value
    path.write_text(json.dumps(state, indent=2, ensure_ascii=False), encoding="utf-8")
