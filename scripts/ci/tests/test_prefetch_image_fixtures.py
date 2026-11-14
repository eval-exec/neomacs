"""Hermetic cache/provisioning contracts; network access is always mocked."""

import contextlib
import errno
import hashlib
from http.client import HTTPResponse, IncompleteRead, RemoteDisconnected
import importlib.util
import io
from pathlib import Path
import socket
import ssl
import sys
import tempfile
import unittest
from unittest import mock
from urllib.error import HTTPError, URLError

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
        sleep_guard = mock.patch.object(
            provisioner.time,
            "sleep",
            side_effect=AssertionError("real unit-test backoff forbidden"),
        )
        sleep_guard.start()
        self.addCleanup(sleep_guard.stop)

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

    def test_transient_network_failures_recover_on_fourth_attempt_with_backoff(self):
        failures = [
            ssl.SSLEOFError(8, "unexpected eof while reading"),
            ConnectionResetError("connection reset by peer"),
            TimeoutError("connection timed out"),
        ]
        with (
            mock.patch.object(
                provisioner, "urlopen", side_effect=[*failures, io.BytesIO(self.data)]
            ) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            self.assertEqual(provisioner.fetch_bytes(self.pin.url), self.data)
        self.assertEqual(opened.call_count, 4)
        self.assertEqual(
            sleep.call_args_list, [mock.call(2), mock.call(5), mock.call(15)]
        )

    def test_http_5xx_and_wrapped_transport_errors_retry(self):
        error_body = io.BytesIO(b"server error")
        failures = [
            HTTPError(self.pin.url, 500, "server failed", None, error_body),
            HTTPError(self.pin.url, 503, "service unavailable", None, None),
            URLError(ssl.SSLEOFError(8, "unexpected eof while reading")),
            URLError("TLS connect error: unexpected eof while reading"),
            URLError(TimeoutError("timed out")),
            URLError(OSError(errno.ENETUNREACH, "network unavailable")),
            URLError(socket.gaierror(socket.EAI_AGAIN, "DNS temporarily unavailable")),
            RemoteDisconnected("remote end closed connection without response"),
        ]
        for failure in failures:
            with (
                self.subTest(failure=failure),
                mock.patch.object(
                    provisioner, "urlopen", side_effect=[failure, io.BytesIO(self.data)]
                ) as opened,
                mock.patch.object(provisioner.time, "sleep") as sleep,
                contextlib.redirect_stderr(io.StringIO()),
            ):
                self.assertEqual(provisioner.fetch_bytes(self.pin.url), self.data)
                self.assertEqual(opened.call_count, 2)
                sleep.assert_called_once_with(2)
        self.assertTrue(error_body.closed)

    def test_all_http_4xx_certificates_and_unknown_failures_stop_without_retry(self):
        failures = [
            HTTPError(self.pin.url, status, "connection reset timed out", None, None)
            for status in (400, 401, 403, 404, 408, 429, 499)
        ] + [
            URLError(URLError(ssl.SSLCertVerificationError(1, "timed out"))),
            URLError("certificate verify failed; connection reset"),
            ssl.SSLError("unknown TLS protocol failure"),
            OSError("unknown failure"),
            PermissionError("permission denied"),
        ]
        for failure in failures:
            with (
                self.subTest(failure=failure),
                mock.patch.object(
                    provisioner, "urlopen", side_effect=failure
                ) as opened,
                mock.patch.object(provisioner.time, "sleep") as sleep,
            ):
                with self.assertRaises(type(failure)) as error:
                    provisioner.fetch_bytes(self.pin.url)
                self.assertIs(error.exception, failure)
                opened.assert_called_once()
                sleep.assert_not_called()

    def test_retry_exhaustion_preserves_four_causes_and_existing_cache(self):
        self.seed(b"existing corrupt cache")
        failures = [
            ConnectionResetError(f"connection reset cause {n}") for n in range(4)
        ]
        with (
            mock.patch.object(provisioner, "urlopen", side_effect=failures) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            with self.assertRaises(ValueError) as error:
                provisioner.prefetch(self.workspace, self.pin)
        diagnostic = str(error.exception)
        self.assertIn("failed after 4 attempts", diagnostic)
        for n in range(4):
            self.assertIn(f"attempt {n + 1}: connection reset cause {n}", diagnostic)
        self.assertIn(self.pin.url, diagnostic)
        self.assertIn(self.pin.sha256, diagnostic)
        self.assertEqual(opened.call_count, 4)
        self.assertEqual(
            sleep.call_args_list, [mock.call(2), mock.call(5), mock.call(15)]
        )
        self.assertEqual(self.destination.read_bytes(), b"existing corrupt cache")
        self.assertEqual(list(self.cache.iterdir()), [self.destination])

    def test_permanent_failure_after_transient_does_not_spend_remaining_attempts(self):
        permanent = HTTPError(self.pin.url, 404, "not found", None, None)
        with (
            mock.patch.object(
                provisioner,
                "urlopen",
                side_effect=[TimeoutError("timed out"), permanent],
            ) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            with self.assertRaises(HTTPError) as error:
                provisioner.fetch_bytes(self.pin.url)
        self.assertIs(error.exception, permanent)
        self.assertEqual(opened.call_count, 2)
        sleep.assert_called_once_with(2)

    def test_partial_response_is_closed_and_discarded_before_retry(self):
        response = mock.MagicMock()
        response.__enter__.return_value = response
        response.read1.side_effect = [
            b"discard partial bytes",
            IncompleteRead(b"x", 10),
        ]
        with (
            mock.patch.object(
                provisioner, "urlopen", side_effect=[response, io.BytesIO(self.data)]
            ) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            self.assertEqual(
                provisioner.prefetch(self.workspace, self.pin), self.destination
            )
        self.assertEqual(self.destination.read_bytes(), self.data)
        response.__exit__.assert_called_once()
        self.assertEqual(opened.call_count, 2)
        sleep.assert_called_once_with(2)

    def test_truncated_content_length_response_retries_instead_of_hash_failure(self):
        socket = mock.Mock()
        socket.makefile.return_value = io.BytesIO(
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort"
        )
        response = HTTPResponse(socket)
        response.begin()
        with (
            mock.patch.object(
                provisioner, "urlopen", side_effect=[response, io.BytesIO(self.data)]
            ) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            self.assertEqual(
                provisioner.prefetch(self.workspace, self.pin), self.destination
            )
        self.assertTrue(response.isclosed())
        self.assertEqual(self.destination.read_bytes(), self.data)
        self.assertEqual(opened.call_count, 2)
        sleep.assert_called_once_with(2)

    def test_hash_mismatch_is_not_retried_and_cache_hit_never_fetches_or_sleeps(self):
        socket = mock.Mock()
        socket.makefile.return_value = io.BytesIO(
            b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nwrong image"
        )
        complete_response = HTTPResponse(socket)
        complete_response.begin()
        with (
            mock.patch.object(
                provisioner, "urlopen", return_value=complete_response
            ) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
        ):
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                provisioner.prefetch(self.workspace, self.pin)
            opened.assert_called_once()
            sleep.assert_not_called()
            self.assertFalse(self.cache.exists())
        self.seed(self.data)
        with (
            mock.patch.object(provisioner, "urlopen") as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
        ):
            self.assertEqual(
                provisioner.prefetch(self.workspace, self.pin), self.destination
            )
            opened.assert_not_called()
            sleep.assert_not_called()

    def test_retry_attempts_share_original_budget_including_backoff(self):
        with (
            mock.patch.object(
                provisioner, "urlopen", side_effect=TimeoutError("timed out")
            ) as opened,
            mock.patch.object(provisioner.time, "sleep") as sleep,
            mock.patch.object(
                provisioner.time,
                "monotonic",
                side_effect=[0, provisioner.DOWNLOAD_BUDGET_SECONDS + 1],
            ),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            with self.assertRaisesRegex(TimeoutError, "budget"):
                provisioner.fetch_bytes(self.pin.url)
        opened.assert_called_once()
        sleep.assert_called_once_with(2)

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
