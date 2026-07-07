import subprocess
from pathlib import Path

from rich.progress import Progress

from .constants import (
    AUDIO_BR,
    COMMON_AARGS,
    COMMON_VARGS,
    CREATE_NO_WINDOW,
    SHORT_THRESHOLD,
    SIZE_AARGS,
    TARGET_MB,
    VIDEO_EXTS,
    DEFAULT_VIDEO_PROFILE,
    VIDEO_PROFILE_480,
    VIDEO_PROFILE_720,
    VIDEO_PROFILE_CRF,
    VIDEO_PROFILE_GIF,
    VIDEO_PROFILE_GIF_120,
    VIDEO_PROFILE_GIF_180,
    VIDEO_PROFILE_GIF_480,
    VIDEO_PROFILE_IDS,
    VIDEO_PROFILE_LABELS,
    VIDEO_PROFILE_SIZE,
)
from .ffmpeg import ffmpeg_cmd, ffprobe_cmd
from .ui import console


def get_duration(path: Path) -> float:
    out = subprocess.run(
        [
            ffprobe_cmd(),
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(path),
        ],
        stdout=subprocess.PIPE,
        text=True,
        check=True,
        creationflags=CREATE_NO_WINDOW,
    ).stdout.strip()
    try:
        return float(out)
    except ValueError:
        return 0.0


def probe_video(path: Path) -> tuple[int, int]:
    out = subprocess.run(
        [
            ffprobe_cmd(),
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(path),
        ],
        stdout=subprocess.PIPE,
        text=True,
        check=True,
        creationflags=CREATE_NO_WINDOW,
    ).stdout.strip().splitlines()
    w, h = map(int, out)
    return w, h


def find_all_videos(inputs: list[str], *, quiet: bool = False) -> list[Path]:
    videos: list[Path] = []
    for s in inputs:
        p = Path(s)
        if p.is_file() and p.suffix.lower() in VIDEO_EXTS:
            videos.append(p)
        elif p.is_dir():
            videos += [f for f in p.rglob("*") if f.suffix.lower() in VIDEO_EXTS]
        elif not quiet:
            console.log(f"[yellow]Skipping unsupported video input: {s}[/]")
    return videos


def parse_ffmpeg_progress_seconds(line: str) -> float | None:
    line = line.strip()
    if not line:
        return None

    for key in ("out_time_ms=", "out_time_us="):
        if line.startswith(key):
            try:
                return int(line.split("=", 1)[1].strip()) / 1_000_000
            except ValueError:
                return None

    if line.startswith("out_time="):
        try:
            h, m, s = line.split("=", 1)[1].strip().split(":")
            return int(h) * 3600 + int(m) * 60 + float(s)
        except ValueError:
            return None

    return None


def run_ffmpeg_with_progress(cmd: list[str], duration: float, update):
    if duration <= SHORT_THRESHOLD:
        subprocess.run(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=True,
            creationflags=CREATE_NO_WINDOW,
        )
        update(duration)
        return

    proc = subprocess.Popen(
        cmd[:-1] + ["-progress", "pipe:1", "-nostats", cmd[-1]],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        encoding="utf-8",
        errors="replace",
        creationflags=CREATE_NO_WINDOW,
        bufsize=1,
    )
    if proc.stdout is not None:
        for line in proc.stdout:
            sec = parse_ffmpeg_progress_seconds(line)
            if sec is not None:
                update(sec)
            elif line.strip() == "progress=end":
                break

    proc.wait()
    if proc.returncode:
        raise RuntimeError(f"ffmpeg exited with code {proc.returncode}")
    update(duration)


def run_with_progress(cmd: list[str], duration: float, task, progress: Progress):
    run_ffmpeg_with_progress(cmd, duration, lambda sec: progress.update(task, completed=sec))


def normalize_video_profile(profile: str | None) -> str:
    if not profile:
        return DEFAULT_VIDEO_PROFILE
    aliases = {
        "compress": VIDEO_PROFILE_CRF,
        "5": VIDEO_PROFILE_SIZE,
        "5mb": VIDEO_PROFILE_SIZE,
        "480": VIDEO_PROFILE_480,
        "720": VIDEO_PROFILE_720,
        "gif120": VIDEO_PROFILE_GIF_120,
        "gif180": VIDEO_PROFILE_GIF_180,
        "gif240": VIDEO_PROFILE_GIF,
    }
    profile = aliases.get(profile.lower(), profile.lower())
    return profile if profile in VIDEO_PROFILE_IDS else DEFAULT_VIDEO_PROFILE


def video_profile_label(profile: str) -> str:
    return VIDEO_PROFILE_LABELS[normalize_video_profile(profile)]


def video_profile_choices() -> tuple[str, ...]:
    return tuple(VIDEO_PROFILE_LABELS.keys())


def video_profile_labels() -> tuple[str, ...]:
    return tuple(VIDEO_PROFILE_LABELS.values())


def video_profile_from_label(label: str) -> str:
    for profile, profile_label in VIDEO_PROFILE_LABELS.items():
        if label == profile_label:
            return profile
    return normalize_video_profile(label)


def alternate_video_profile(profile: str) -> str:
    profile = normalize_video_profile(profile)
    return VIDEO_PROFILE_CRF if profile != VIDEO_PROFILE_CRF else VIDEO_PROFILE_SIZE


def gif_profile_target(profile: str) -> int | None:
    return {
        VIDEO_PROFILE_GIF_120: 120,
        VIDEO_PROFILE_GIF_180: 180,
        VIDEO_PROFILE_GIF: 240,
        VIDEO_PROFILE_GIF_480: 480,
    }.get(normalize_video_profile(profile))


def video_output_path(path: Path, profile: str) -> Path:
    profile = normalize_video_profile(profile)
    if profile == VIDEO_PROFILE_SIZE:
        return path.with_name(f"{path.stem}_smaller.mp4")
    if gif_profile_target(profile) is not None:
        return path.with_name(f"{path.stem}_converted.gif")
    return path.with_name(f"{path.stem}_compressed.mp4")


def _even_dimension(value: float) -> int:
    return max(2, int(value) // 2 * 2)


def short_side_scale_filter(path: Path, target: int, *, no_upscale: bool = True) -> str | None:
    width, height = probe_video(path)
    short_side = min(width, height)
    if short_side <= 0:
        return None
    scale = target / short_side
    if no_upscale and scale >= 1:
        return None
    width2 = _even_dimension(width * scale)
    height2 = _even_dimension(height * scale)
    return f"scale={width2}:{height2}"


def gif_scale_filter(target: int) -> str:
    return (
        f"fps=15,"
        f"scale=if(lt(iw\\,ih)\\,{target}\\,-2):"
        f"if(lt(iw\\,ih)\\,-2\\,{target}):flags=lanczos"
    )


def build_video_args(path: Path, output: Path, mode: str, crf: int, preset: str) -> list[str]:
    mode = normalize_video_profile(mode)
    dur = get_duration(path)

    gif_target = gif_profile_target(mode)
    if gif_target is not None:
        return [
            ffmpeg_cmd(),
            "-y",
            "-i",
            str(path),
            "-vf",
            gif_scale_filter(gif_target),
            "-loop",
            "0",
            str(output),
        ]

    base = [ffmpeg_cmd(), "-y", "-i", str(path)] + COMMON_VARGS

    if mode == VIDEO_PROFILE_CRF:
        return base + ["-preset", preset, "-crf", str(crf)] + COMMON_AARGS + [str(output)]

    if mode in {VIDEO_PROFILE_480, VIDEO_PROFILE_720}:
        target = 480 if mode == VIDEO_PROFILE_480 else 720
        vf = short_side_scale_filter(path, target, no_upscale=True)
        args = base
        if vf:
            args += ["-vf", vf]
        return args + ["-preset", preset, "-crf", str(crf)] + COMMON_AARGS + [str(output)]

    w, h = probe_video(path)
    target_b = int(TARGET_MB * 1024 * 1024)
    scale = 1.0
    while True:
        vbr = (target_b * 8 - AUDIO_BR * dur) / dur
        if vbr < w * h * scale * scale * 0.1:
            scale *= 0.9
        else:
            w2, h2 = int(w * scale), int(h * scale)
            break
    return base + ["-vf", f"scale={w2}:{h2}", "-b:v", str(int(vbr))] + SIZE_AARGS + [str(output)]


def compress_video(path: Path, output: Path, mode: str, crf: int, preset: str, progress: Progress):
    mode = normalize_video_profile(mode)
    dur = get_duration(path)
    task = progress.add_task(path.name, total=dur)
    console.log(f"Starting video {video_profile_label(mode)}: {path.name}")

    try:
        args = build_video_args(path, output, mode, crf, preset)
        run_with_progress(args, dur, task, progress)
        if mode == VIDEO_PROFILE_SIZE or gif_profile_target(mode) is not None:
            size_mb = output.stat().st_size / (1024 * 1024)
            console.log(f"Completed: {path.name} -> {size_mb:.2f} MB")
        else:
            console.log(f"Completed: {path.name}")
    except Exception as e:
        console.log(f"[red]Error {path.name}: {e}[/]")


def get_video_info(path: Path) -> tuple[float, str, int]:
    dur = get_duration(path)
    out = subprocess.run(
        [
            ffprobe_cmd(),
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(path),
        ],
        stdout=subprocess.PIPE,
        text=True,
        check=True,
        creationflags=CREATE_NO_WINDOW,
    ).stdout.strip().splitlines()
    codec = out[0] if out else ""
    try:
        br = int(out[1]) if len(out) > 1 else 0
    except ValueError:
        br = 0
    return dur, codec, br


def compress_video_gui(path: Path, output: Path, mode: str, update):
    dur = get_duration(path)
    args = build_video_args(path, output, mode, crf=30, preset="slow")
    run_ffmpeg_with_progress(args, dur, update)
