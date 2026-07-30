#!/usr/bin/env python3
import sys


def _enable_console_for_cli() -> None:
    """Attach to the caller's terminal, or create one for CLI-only launches."""
    if sys.platform != "win32":
        return
    args = sys.argv[1:]
    if not args or args[0] in {"gui", "--gui"}:
        return
    try:
        import ctypes

        kernel32 = ctypes.windll.kernel32
        if not kernel32.GetConsoleWindow() and not kernel32.AttachConsole(-1):
            kernel32.AllocConsole()

        # A windowed executable has no standard streams until it attaches to a
        # console. Reopen them before the CLI (and Rich) is imported.
        if sys.stdout is None:
            sys.stdout = open("CONOUT$", "w", encoding="utf-8", errors="replace")
        if sys.stderr is None:
            sys.stderr = open("CONOUT$", "w", encoding="utf-8", errors="replace")
        if sys.stdin is None:
            sys.stdin = open("CONIN$", "r", encoding="utf-8", errors="replace")
    except Exception:
        pass


if __name__ == "__main__":
    _enable_console_for_cli()
    from compress_tool.cli import main

    main()
