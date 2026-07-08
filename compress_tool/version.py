import subprocess
from pathlib import Path

DEFAULT_VERSION = "0.1.0"


def _git_version() -> str | None:
    root = Path(__file__).resolve().parent.parent
    try:
        result = subprocess.run(
            ["git", "describe", "--tags", "--dirty", "--always"],
            cwd=root,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    version = result.stdout.strip()
    return version or None


try:
    from ._build_version import __version__
except ImportError:
    __version__ = _git_version() or DEFAULT_VERSION
