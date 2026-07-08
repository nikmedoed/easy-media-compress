import time
import unittest

from compress_tool import updater


class UpdaterTests(unittest.TestCase):
    def test_version_comparison_accepts_v_tags(self) -> None:
        self.assertTrue(updater.is_newer_version("v0.1.1", "0.1.0"))
        self.assertTrue(updater.is_newer_version("1.2", "1.1.9"))
        self.assertFalse(updater.is_newer_version("v0.1.0", "0.1.0"))
        self.assertFalse(updater.is_newer_version("not-a-version", "0.1.0"))

    def test_release_payload_selects_easy_media_compress_exe(self) -> None:
        release = updater.release_from_payload(
            {
                "tag_name": "v0.2.0",
                "html_url": "https://github.com/nikmedoed/easy-media-compress/releases/tag/v0.2.0",
                "draft": False,
                "prerelease": False,
                "assets": [
                    {
                        "name": "notes.txt",
                        "browser_download_url": "https://example.invalid/notes.txt",
                    },
                    {
                        "name": "EasyMediaCompress-v0.2.0-windows-x64.exe",
                        "browser_download_url": "https://example.invalid/EasyMediaCompress.exe",
                        "size": 10,
                    },
                ],
            },
            "0.1.0",
        )

        self.assertIsNotNone(release)
        assert release is not None
        self.assertEqual(release.version, "0.2.0")
        self.assertEqual(release.tag_name, "v0.2.0")
        self.assertEqual(release.asset_name, "EasyMediaCompress-v0.2.0-windows-x64.exe")

    def test_release_payload_ignores_old_draft_or_prerelease_versions(self) -> None:
        base = {
            "tag_name": "v0.1.0",
            "assets": [
                {
                    "name": "EasyMediaCompress.exe",
                    "browser_download_url": "https://example.invalid/app.exe",
                }
            ],
        }
        self.assertIsNone(updater.release_from_payload({**base, "draft": False}, "0.1.0"))
        self.assertIsNone(updater.release_from_payload({**base, "tag_name": "v0.2.0", "draft": True}, "0.1.0"))
        self.assertIsNone(updater.release_from_payload({**base, "tag_name": "v0.2.0", "prerelease": True}, "0.1.0"))

    def test_check_interval_uses_error_backoff(self) -> None:
        now = time.time()
        original_read_json = updater._read_json
        try:
            updater._read_json = lambda _path: {"last_check_at": now - 120, "last_error_at": now - 60}
            self.assertFalse(updater.should_check_for_update(now))
            updater._read_json = lambda _path: {"last_check_at": now - 3700, "last_error_at": now - 3600}
            self.assertTrue(updater.should_check_for_update(now))
        finally:
            updater._read_json = original_read_json


if __name__ == "__main__":
    unittest.main()
