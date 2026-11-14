"""Hermetic cache/provisioning contracts; network access is always mocked."""

import contextlib
import hashlib
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

WORKSPACE = Path(__file__).resolve().parents[3]
SCRIPT = WORKSPACE / "scripts/ci/prefetch-image-fixtures.py"
SPEC = importlib.util.spec_from_file_location("prefetch_image_fixtures", SCRIPT)
provisioner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = provisioner
SPEC.loader.exec_module(provisioner)


class ImageFixtureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.workspace = Path(self.temporary.name)
        self.data = b"mock pinned image bytes"
        self.pin = provisioner.Pin(
            "mock-image.xpm",
            "1" * 40,
            "https://fixtures.invalid/" + "1" * 40 + "/image.xpm",
            hashlib.sha256(self.data).hexdigest(),
        )
        self.source = (
            "//! Ordinary Rust pin declarations, including rustfmt's wrapped digest.\n"
            f'pub const NYAN_MODE_NAME: &str = "{self.pin.name}";\n'
            f'pub const NYAN_MODE_COMMIT: &str = "{self.pin.commit}";\n'
            f'pub const NYAN_MODE_URL: &str = "{self.pin.url}";\n'
            f'pub const NYAN_MODE_SHA256: &str =\n    "{self.pin.sha256}";\n'
        )
        self.cache = self.workspace / "tmp/pinned-fixtures"
        self.destination = self.cache / self.pin.name
        guard = mock.patch.object(
            provisioner,
            "urlopen",
            side_effect=AssertionError("live unit-test network forbidden"),
        )
        guard.start()
        self.addCleanup(guard.stop)

    def seed(self, data):
        self.cache.mkdir(parents=True)
        self.destination.write_bytes(data)

    def test_parser_reads_actual_shared_rust_pin_without_python_or_yaml_copy(self):
        pin = provisioner.read_pin(WORKSPACE)
        self.assertEqual(len(pin.sha256), 64)
        self.assertIn(pin.commit, pin.url)
        for path in [SCRIPT, WORKSPACE / ".github/workflows/test-suite.yml"]:
            self.assertNotIn(pin.url, path.read_text())
            self.assertNotIn(pin.sha256, path.read_text())
        self.assertEqual(provisioner.parse_pin(self.source), self.pin)
        self.assertEqual(
            provisioner.parse_pin(
                '// pub const NYAN_MODE_NAME: &str = "fake";\n' + self.source
            ),
            self.pin,
        )

    def test_parser_rejects_missing_duplicate_or_nonliteral_declarations(self):
        bad_sources = [
            self.source.replace(
                f'pub const NYAN_MODE_NAME: &str = "{self.pin.name}";', ""
            ),
            self.source + f'pub const NYAN_MODE_NAME: &str = "{self.pin.name}";',
            self.source.replace(f'"{self.pin.name}"', 'concat!("mock", ".xpm")'),
            self.source.replace(f'"{self.pin.name}"', '"../outside.xpm"'),
            self.source.replace(self.pin.sha256, "not-a-sha256"),
            self.source + 'pub const EXTRA: &str = "unrecognized";',
            "/*\n" + self.source + "\n*/",
        ]
        for source in bad_sources:
            with self.subTest(source=source), self.assertRaises(ValueError):
                provisioner.parse_pin(source)

    def test_verified_hit_needs_no_fetch_or_replacement(self):
        self.seed(self.data)
        fetch = mock.Mock(side_effect=AssertionError("cache hit must not fetch"))
        with mock.patch.object(provisioner.os, "replace") as replace:
            self.assertEqual(
                provisioner.prefetch(self.workspace, self.pin, fetch), self.destination
            )
        fetch.assert_not_called()
        replace.assert_not_called()
        self.assertEqual(list(self.cache.iterdir()), [self.destination])

    def test_missing_entry_is_fetched_verified_and_published_in_live_workspace(self):
        fetch = mock.Mock(return_value=self.data)
        self.assertEqual(
            provisioner.prefetch(self.workspace, self.pin, fetch), self.destination
        )
        fetch.assert_called_once_with(self.pin.url)
        self.assertEqual(self.destination.read_bytes(), self.data)
        self.assertEqual(list(self.cache.iterdir()), [self.destination])

    def test_corrupt_entry_is_replaced_only_after_verified_temporary_write(self):
        self.seed(b"corrupt")
        real_replace = provisioner.os.replace

        def observe(source, destination):
            self.assertEqual(self.destination.read_bytes(), b"corrupt")
            self.assertEqual(Path(source).parent, self.cache)
            self.assertEqual(Path(source).read_bytes(), self.data)
            real_replace(source, destination)

        with mock.patch.object(
            provisioner.os, "replace", side_effect=observe
        ) as replace:
            provisioner.prefetch(
                self.workspace, self.pin, mock.Mock(return_value=self.data)
            )
        replace.assert_called_once()
        self.assertEqual(self.destination.read_bytes(), self.data)
        self.assertEqual(list(self.cache.iterdir()), [self.destination])

    def test_hash_failure_never_publishes_or_damages_existing_cache(self):
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            provisioner.prefetch(
                self.workspace, self.pin, mock.Mock(return_value=b"wrong response")
            )
        self.assertFalse(self.cache.exists())
        self.seed(b"existing corrupt cache")
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            provisioner.prefetch(
                self.workspace, self.pin, mock.Mock(return_value=b"wrong response")
            )
        self.assertEqual(self.destination.read_bytes(), b"existing corrupt cache")
        self.assertEqual(list(self.cache.iterdir()), [self.destination])

    def test_replace_failure_keeps_destination_and_cleans_temporary_file(self):
        self.seed(b"existing corrupt cache")
        with mock.patch.object(
            provisioner.os, "replace", side_effect=OSError("replace refused")
        ):
            with self.assertRaisesRegex(OSError, "replace refused"):
                provisioner.prefetch(
                    self.workspace, self.pin, mock.Mock(return_value=self.data)
                )
        self.assertEqual(self.destination.read_bytes(), b"existing corrupt cache")
        self.assertEqual(list(self.cache.iterdir()), [self.destination])

    def test_fetch_failure_keeps_full_diagnostic_and_existing_bytes(self):
        self.seed(b"existing corrupt cache")
        diagnostic = "TLS read failed: detailed upstream cause"
        with self.assertRaises(ValueError) as error:
            provisioner.prefetch(
                self.workspace, self.pin, mock.Mock(side_effect=OSError(diagnostic))
            )
        self.assertIn(diagnostic, str(error.exception))
        self.assertIn(self.pin.url, str(error.exception))
        self.assertIn(self.pin.sha256, str(error.exception))
        self.assertEqual(self.destination.read_bytes(), b"existing corrupt cache")

    def test_download_and_injected_fetch_enforce_size_limit(self):
        with mock.patch.object(provisioner, "MAX_BYTES", 4):
            with mock.patch.object(
                provisioner, "urlopen", return_value=io.BytesIO(b"12345")
            ) as opened:
                with self.assertRaisesRegex(ValueError, "byte limit"):
                    provisioner.fetch_bytes(self.pin.url)
                opened.assert_called_once_with(
                    self.pin.url, timeout=provisioner.IO_TIMEOUT_SECONDS
                )
            with self.assertRaisesRegex(ValueError, "byte limit"):
                provisioner.prefetch(
                    self.workspace, self.pin, mock.Mock(return_value=b"12345")
                )
        self.assertFalse(self.cache.exists())

    def test_download_reads_complete_verified_candidate_bytes(self):
        with mock.patch.object(
            provisioner, "urlopen", return_value=io.BytesIO(self.data)
        ) as opened:
            self.assertEqual(provisioner.fetch_bytes(self.pin.url), self.data)
        opened.assert_called_once_with(
            self.pin.url, timeout=provisioner.IO_TIMEOUT_SECONDS
        )

    def test_download_budget_is_checked_without_real_network_or_waiting(self):
        with mock.patch.object(
            provisioner, "urlopen", return_value=io.BytesIO(self.data)
        ):
            with mock.patch.object(
                provisioner.time,
                "monotonic",
                side_effect=[0, provisioner.DOWNLOAD_BUDGET_SECONDS + 1],
            ):
                with self.assertRaisesRegex(TimeoutError, "budget"):
                    provisioner.fetch_bytes(self.pin.url)

    def test_download_budget_also_rejects_a_read_that_finishes_late(self):
        with mock.patch.object(
            provisioner, "urlopen", return_value=io.BytesIO(self.data)
        ):
            with mock.patch.object(
                provisioner.time,
                "monotonic",
                side_effect=[0, 0, provisioner.DOWNLOAD_BUDGET_SECONDS + 1],
            ):
                with self.assertRaisesRegex(TimeoutError, "budget"):
                    provisioner.fetch_bytes(self.pin.url)

    def test_workspace_option_selects_pin_source_and_cache(self):
        source_path = self.workspace / provisioner.PIN_SOURCE
        source_path.parent.mkdir(parents=True)
        source_path.write_text(self.source)
        self.seed(self.data)
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(provisioner.main(["--workspace", str(self.workspace)]), 0)
        self.assertIn(str(self.destination), output.getvalue())
        self.assertIn(self.pin.sha256, output.getvalue())

    def test_display_archive_prepares_live_remap_before_nextest(self):
        workflow = (WORKSPACE / ".github/workflows/test-suite.yml").read_text()
        display = workflow.split("  neomacs-display-tests:", 1)[1].split(
            "  neomacs-prefix-face-tui-parity:", 1
        )[0]
        preparation = 'python3 scripts/ci/prefetch-image-fixtures.py --workspace "$GITHUB_WORKSPACE"'
        self.assertLess(
            display.index(preparation), display.index("- name: Run display stack tests")
        )
        self.assertIn('--workspace-remap "$GITHUB_WORKSPACE"', display)
        self.assertIn('--extract-to "$TMPDIR/nextest"', display)


if __name__ == "__main__":
    unittest.main()
