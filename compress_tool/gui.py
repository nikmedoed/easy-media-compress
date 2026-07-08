import os
import queue
import subprocess
import sys
import time
import webbrowser
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from tkinter import StringVar, filedialog, ttk

import tkinter as tk
from tkinter import messagebox
from tkinterdnd2 import DND_FILES, TkinterDnD

from .constants import APP_NAME, CREATE_NO_WINDOW, IMAGE_EXTS, MAX_WORKERS, VIDEO_EXTS, WINDOWS_APP_ID
from .image import convert_image, get_image_info, image_output_path
from .settings import GuiSettings, load_gui_settings, save_gui_settings
from .ui import console, ensure_icon_ico
from .updater import current_version, start_background_updater
from .video import (
    alternate_video_profile,
    compress_video_gui,
    get_video_info,
    video_output_path,
    video_profile_from_label,
    video_profile_label,
    video_profile_labels,
)

AUTHOR_URL = "https://nikmedoed.com/"


def is_dark_mode() -> bool:
    """Return True if the system appears to be using a dark theme."""
    if sys.platform == "darwin":
        try:
            out = subprocess.run(
                ["defaults", "read", "-g", "AppleInterfaceStyle"],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                creationflags=CREATE_NO_WINDOW,
            )
            return out.stdout.strip().lower() == "dark"
        except Exception:
            return False
    return False


def format_duration(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    h, m = divmod(m, 60)
    if h:
        return f"{h:d}:{m:02d}:{s:02d}"
    return f"{m:02d}:{s:02d}"


def progress_bar_text(percent: float, width: int = 16) -> str:
    percent = max(0.0, min(100.0, percent))
    text = f" {percent:3.0f}% "
    inner = width - 2
    bar = [" "] * inner
    fill_len = int(inner * percent / 100)
    for i in range(fill_len):
        bar[i] = "#"
    start = (inner - len(text)) // 2
    bar[start : start + len(text)] = list(text)
    return "[" + "".join(bar) + "]"


def open_in_folder(path: Path):
    folder = path.parent
    if sys.platform.startswith("win"):
        os.startfile(folder)
    elif sys.platform == "darwin":
        subprocess.Popen(["open", folder], creationflags=CREATE_NO_WINDOW)
    else:
        subprocess.Popen(["xdg-open", folder], creationflags=CREATE_NO_WINDOW)


def run_gui():
    if sys.platform.startswith("win"):
        try:
            import ctypes

            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(WINDOWS_APP_ID)
        except Exception:
            pass

    root = TkinterDnD.Tk()
    root.title(APP_NAME)
    saved_settings = load_gui_settings()
    if saved_settings.window_geometry:
        try:
            root.geometry(saved_settings.window_geometry)
        except tk.TclError:
            pass
    try:
        icon_dir = ensure_icon_ico()
        if sys.platform.startswith("win"):
            root.iconbitmap(icon_dir / "icon.ico")
        img = tk.PhotoImage(file=icon_dir / "icon.png")
        root._icon_image = img
        root.iconphoto(True, img)
    except Exception:
        pass

    style = ttk.Style(root)
    dark = is_dark_mode()
    if dark:
        style.theme_use("clam")
        style.configure(
            "Treeview",
            background="#333333",
            fieldbackground="#333333",
            foreground="white",
        )
        style.map(
            "Treeview",
            background=[("selected", "#446688")],
            foreground=[("selected", "white")],
        )
        fg = "white"
        waiting_bg = "#333333"
        prog_bg = "#454545"
        completed_bg = "#355e35"
        error_bg = "#553333"
    else:
        fg = "black"
        waiting_bg = "white"
        prog_bg = "#ffe5b4"
        completed_bg = "#d4f7d4"
        error_bg = "#f7d4d4"

    top = ttk.Frame(root)
    top.pack(fill="x")

    def current_settings() -> GuiSettings:
        return GuiSettings(
            video_profile=video_profile_from_label(video_profile_var.get()),
            image_mode=image_mode_var.get(),
            window_geometry=root.geometry(),
        )

    def persist_current_settings(*_args):
        save_gui_settings(current_settings())

    video_profile_var = StringVar(value=video_profile_label(saved_settings.video_profile))
    ttk.Label(top, text="Video").pack(side="left", padx=(5, 3), pady=5)
    video_profile = ttk.Combobox(
        top,
        textvariable=video_profile_var,
        values=video_profile_labels(),
        state="readonly",
        width=10,
    )
    video_profile.pack(side="left", padx=3, pady=5)
    video_profile.bind("<<ComboboxSelected>>", persist_current_settings)

    image_mode_var = StringVar(value=saved_settings.image_mode)
    ttk.Label(top, text="Photo").pack(side="left", padx=(10, 3), pady=5)
    image_mode = ttk.Combobox(
        top,
        textvariable=image_mode_var,
        values=("lossy", "original"),
        state="readonly",
        width=9,
    )
    image_mode.pack(side="left", padx=3, pady=5)
    image_mode.bind("<<ComboboxSelected>>", persist_current_settings)

    btn = ttk.Button(top, text="Add Media")
    btn.pack(side="left", padx=5, pady=5)

    columns = (
        "kind",
        "file",
        "codec",
        "details",
        "size",
        "result",
        "mode",
        "alt",
        "progress",
    )
    bottom = ttk.Frame(root)
    bottom.pack(side="bottom", fill="x", padx=5, pady=(0, 5))
    ttk.Separator(bottom, orient="horizontal").pack(fill="x", pady=(0, 4))
    ttk.Label(bottom, text=f"Version {current_version()} | Author: Muromtsev Nikita | ").pack(side="left")
    author_link = ttk.Label(
        bottom,
        text=AUTHOR_URL,
        foreground="#0b63ce",
        cursor="hand2",
    )
    author_link.pack(side="left")
    author_link.bind("<Button-1>", lambda _event: webbrowser.open(AUTHOR_URL))

    table_frame = ttk.Frame(root)
    table_frame.pack(side="top", fill="both", expand=True, padx=5, pady=(0, 5))

    tree = ttk.Treeview(table_frame, columns=columns, show="headings")
    tree.tag_configure("waiting", background=waiting_bg, foreground=fg)
    tree.tag_configure("in_progress", background=prog_bg, foreground=fg)
    tree.tag_configure("completed", background=completed_bg, foreground=fg)
    tree.tag_configure("error", background=error_bg, foreground=fg)
    widths = {
        "kind": 60,
        "file": 220,
        "codec": 70,
        "details": 95,
        "size": 80,
        "result": 80,
        "mode": 80,
        "alt": 40,
        "progress": 150,
    }
    for c in columns:
        tree.heading(c, text=c.title())
        tree.column(c, width=widths[c], anchor="center")
    tree.column("file", anchor="w")

    vsb = ttk.Scrollbar(table_frame, orient="vertical")
    vsb.pack(side="right", fill="y")

    auto_scroll = True
    scrolling_programmatically = False
    last_action = time.time()

    def user_action(event=None):
        nonlocal auto_scroll, last_action
        auto_scroll = False
        last_action = time.time()

    def yview(*args):
        tree.yview(*args)
        if not scrolling_programmatically:
            user_action()

    vsb.config(command=yview)
    tree.configure(yscrollcommand=vsb.set)
    tree.pack(side="left", fill="both", expand=True)

    root.bind_all("<MouseWheel>", user_action, add="+")
    root.bind_all("<Button-4>", user_action, add="+")
    root.bind_all("<Button-5>", user_action, add="+")
    root.bind_all("<ButtonPress>", user_action, add="+")
    root.bind_all("<ButtonRelease>", user_action, add="+")
    root.bind_all("<B1-Motion>", user_action, add="+")
    root.bind_all("<Key>", user_action, add="+")
    progress_vals: dict[str, float] = {}
    info: dict[str, dict[str, object]] = {}

    last_idx = -1
    scroll_scheduled = False

    def _do_scroll():
        nonlocal scroll_scheduled, last_idx, scrolling_programmatically
        scroll_scheduled = False
        if not auto_scroll:
            return
        items = tree.get_children()
        for it in items:
            if (
                it in progress_vals
                and not info.get(it, {}).get("done")
                and progress_vals.get(it, 0) < 100
            ):
                idx = items.index(it)
                if idx != last_idx:
                    scrolling_programmatically = True
                    tree.yview_moveto(idx / len(items))
                    scrolling_programmatically = False
                    last_idx = idx
                break

    def scroll_to_current():
        nonlocal scroll_scheduled
        if scroll_scheduled:
            return
        scroll_scheduled = True
        root.after(100, _do_scroll)

    def check_idle():
        nonlocal auto_scroll, last_action
        if not auto_scroll and time.time() - last_action >= 5:
            auto_scroll = True
            scroll_to_current()
        root.after(1000, check_idle)

    total = 0
    done = 0

    overall_bar = ttk.Progressbar(top, length=200)
    overall_bar.pack(side="left", fill="x", expand=True, padx=5)
    overall_label = ttk.Label(top, text="0/0")
    overall_label.pack(side="left", padx=5)

    executor = ThreadPoolExecutor(max_workers=MAX_WORKERS)
    gui_queue: "queue.Queue[tuple]" = queue.Queue()
    shown_errors: set[str] = set()

    def update_overall():
        if total:
            overall_bar["value"] = done * 100 / total
            overall_label.config(text=f"{done}/{total}")
        else:
            overall_bar["value"] = 0
            overall_label.config(text="0/0")
        root.update_idletasks()

    def process_gui_queue():
        nonlocal done
        try:
            while True:
                msg = gui_queue.get_nowait()
                kind = msg[0]
                if kind == "begin":
                    row = msg[1]
                    tree.item(row, tags=("in_progress",))
                    tree.update_idletasks()
                    scroll_to_current()
                elif kind == "progress":
                    row, p = msg[1], msg[2]
                    progress_vals[row] = p
                    tree.set(row, "progress", progress_bar_text(p))
                    tree.item(row, tags=("completed",) if p >= 100 else ("in_progress",))
                    tree.update_idletasks()
                elif kind == "finish":
                    row, size_mb = msg[1], msg[2]
                    progress_vals[row] = 100
                    tree.set(row, "progress", progress_bar_text(100))
                    tree.item(row, tags=("completed",))
                    tree.set(row, "result", f"{size_mb:.1f} MB")
                    info[row]["done"] = True
                    tree.update_idletasks()
                    scroll_to_current()
                    done += 1
                    update_overall()
                elif kind == "error":
                    row = msg[1]
                    error_text = str(msg[2]) if len(msg) > 2 else "Unknown error"
                    short_error = error_text.splitlines()[0][:80] or "error"
                    tree.set(row, "result", short_error)
                    progress_vals[row] = 0
                    info[row]["done"] = True
                    tree.item(row, tags=("error",))
                    tree.update_idletasks()
                    scroll_to_current()
                    done += 1
                    update_overall()
                    if error_text not in shown_errors:
                        shown_errors.add(error_text)
                        messagebox.showerror(APP_NAME, error_text)
        except queue.Empty:
            pass
        root.after(100, process_gui_queue)

    def add_file(path: Path, mode_override=None):
        nonlocal total
        suffix = path.suffix.lower()
        if suffix in VIDEO_EXTS:
            dur, codec, _br = get_video_info(path)
            details = format_duration(dur)
            mode = (
                video_profile_from_label(mode_override)
                if mode_override
                else video_profile_from_label(video_profile_var.get())
            )
            kind = "video"
        elif suffix in IMAGE_EXTS:
            image_info = get_image_info(path)
            dur = 1.0
            codec = image_info.format_name
            details = f"{image_info.width}x{image_info.height}"
            mode = mode_override or image_mode_var.get()
            kind = "image"
        else:
            return

        size_mb = path.stat().st_size / (1024 * 1024)
        row = tree.insert(
            "",
            "end",
            values=(
                kind,
                path.name,
                codec,
                details,
                f"{size_mb:.1f} MB",
                "",
                video_profile_label(str(mode)) if kind == "video" else mode,
                "<>",
                progress_bar_text(0),
            ),
        )
        tree.item(row, tags=("waiting",))
        progress_vals[row] = 0.0
        info[row] = {
            "path": path,
            "duration": dur,
            "mode": mode,
            "kind": kind,
            "done": False,
        }
        tree.update_idletasks()
        scroll_to_current()
        total += 1
        update_overall()
        executor.submit(process_row, row)

    def add_files(paths, mode_override=None):
        for p in paths:
            add_file(Path(p), mode_override)
        root.update_idletasks()

    def select_files():
        patterns = " ".join(f"*{ext}" for ext in sorted(VIDEO_EXTS | IMAGE_EXTS))
        files = filedialog.askopenfilenames(filetypes=[("Media", patterns)])
        add_files(root.splitlist(files))

    btn.config(command=select_files)

    def drop(event):
        add_files(root.splitlist(event.data))

    tree.drop_target_register(DND_FILES)
    tree.dnd_bind("<<Drop>>", drop)

    def on_click(event):
        user_action(event)
        row = tree.identify_row(event.y)
        col = tree.identify_column(event.x)
        alt_col = f"#{columns.index('alt') + 1}"
        if not row or col != alt_col or tree.set(row, "alt") != "<>":
            return
        current = info[row]["mode"]
        if info[row]["kind"] == "video":
            mode = alternate_video_profile(str(current))
        else:
            mode = "original" if current == "lossy" else "lossy"
        add_files([str(info[row]["path"])], mode)
        tree.set(row, "alt", "OK")

    tree.bind("<Button-1>", on_click)

    def on_double(event):
        user_action(event)
        item = tree.identify_row(event.y)
        if item:
            open_in_folder(info[item]["path"])

    tree.bind("<Double-1>", on_double)

    def process_row(row):
        path = info[row]["path"]
        mode = info[row]["mode"]
        kind = info[row]["kind"]
        if kind == "video":
            out = video_output_path(path, str(mode))
        else:
            out = image_output_path(path, str(mode), "jpg")

        gui_queue.put(("begin", row))

        def update(sec):
            duration = float(info[row]["duration"])
            if duration <= 0:
                return
            percent = min(100, sec * 100 / duration)
            gui_queue.put(("progress", row, percent))

        try:
            if kind == "video":
                compress_video_gui(path, out, str(mode), update)
            else:
                gui_queue.put(("progress", row, 10))
                convert_image(path, out, str(mode), "jpg")
                gui_queue.put(("progress", row, 100))
            size_mb = out.stat().st_size / (1024 * 1024)
            gui_queue.put(("finish", row, size_mb))
        except Exception as e:
            console.log(f"[red]Error {path.name}: {e}[/]")
            gui_queue.put(("error", row, str(e)))

    def on_close():
        persist_current_settings()
        root.destroy()

    root.protocol("WM_DELETE_WINDOW", on_close)
    root.after(100, scroll_to_current)
    root.after(100, process_gui_queue)
    root.after(1000, check_idle)
    root.after(1500, lambda: start_background_updater(current_version()))
    root.mainloop()
