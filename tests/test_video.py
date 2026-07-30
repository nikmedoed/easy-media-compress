import json
from pathlib import Path
import unittest
from unittest.mock import patch

from compress_tool.video import probe_video, short_side_scale_filter


def _ffprobe_result(data):
    class Result:
        stdout = json.dumps(data)

    return Result()


class VideoTests(unittest.TestCase):
    @patch("compress_tool.video.subprocess.run")
    def test_probe_video_uses_rotation_side_data(self, run):
        run.return_value = _ffprobe_result(
            {"streams": [{"width": 1920, "height": 1080, "side_data_list": [{"rotation": -90}]}]}
        )

        self.assertEqual(probe_video(Path("vertical.mp4")), (1080, 1920))

    @patch("compress_tool.video.probe_video", return_value=(1080, 1920))
    def test_short_side_scaling_keeps_vertical_orientation(self, probe):
        self.assertEqual(
            short_side_scale_filter(Path("vertical.mp4"), 480),
            "scale=480:852",
        )


if __name__ == "__main__":
    unittest.main()
