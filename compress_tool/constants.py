import os
import subprocess

CREATE_NO_WINDOW = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0

VIDEO_EXTS = {".mp4", ".mkv", ".avi", ".mov", ".flv", ".wmv", ".webm"}
IMAGE_EXTS = {
    ".jpg",
    ".jpeg",
    ".jfif",
    ".png",
    ".bmp",
    ".tif",
    ".tiff",
    ".webp",
    ".heic",
    ".heif",
    ".avif",
}

SHORT_THRESHOLD = 2.0
MAX_WORKERS = 4
TARGET_MB = 4.5
AUDIO_BR = 64_000

COMMON_VARGS = ["-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "faststart"]
COMMON_AARGS = ["-c:a", "aac", "-b:a", "128k"]
SIZE_AARGS = ["-c:a", "aac", "-b:a", str(AUDIO_BR)]

VIDEO_PROFILE_CRF = "crf"
VIDEO_PROFILE_SIZE = "size"
VIDEO_PROFILE_480 = "crf480"
VIDEO_PROFILE_720 = "crf720"
VIDEO_PROFILE_GIF_120 = "gif120"
VIDEO_PROFILE_GIF_180 = "gif180"
VIDEO_PROFILE_GIF = "gif"
VIDEO_PROFILE_GIF_480 = "gif480"
DEFAULT_VIDEO_PROFILE = VIDEO_PROFILE_CRF
VIDEO_PROFILE_IDS = {
    VIDEO_PROFILE_CRF,
    VIDEO_PROFILE_SIZE,
    VIDEO_PROFILE_480,
    VIDEO_PROFILE_720,
    VIDEO_PROFILE_GIF_120,
    VIDEO_PROFILE_GIF_180,
    VIDEO_PROFILE_GIF,
    VIDEO_PROFILE_GIF_480,
}
VIDEO_PROFILE_LABELS = {
    VIDEO_PROFILE_CRF: "MP4",
    VIDEO_PROFILE_SIZE: "5 MB",
    VIDEO_PROFILE_480: "MP4 480p",
    VIDEO_PROFILE_720: "MP4 720p",
    VIDEO_PROFILE_GIF_120: "GIF 120p",
    VIDEO_PROFILE_GIF_180: "GIF 180p",
    VIDEO_PROFILE_GIF: "GIF 240p",
    VIDEO_PROFILE_GIF_480: "GIF 480p",
}

IMAGE_LOSSY_QUALITY = 60
IMAGE_ORIGINAL_QUALITY = 95
IMAGE_OUTPUT_FORMATS = {"jpg", "jpeg", "png", "webp"}

APP_NAME = "Easy Media Compress"
APP_DIR_NAME = "EasyMediaCompress"
WINDOWS_APP_ID = "AlignTech.EasyMediaCompress"
