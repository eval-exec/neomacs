"""Hermetic contracts for the display-stack nine-file font cache."""

import hashlib
import importlib.util
import io
import os
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock
from urllib.error import HTTPError, URLError

WORKSPACE = Path(__file__).resolve().parents[3]
SCRIPT = WORKSPACE / "scripts/ci/prefetch-display-font-fixtures.py"
SPEC = importlib.util.spec_from_file_location("prefetch_display_font_fixtures", SCRIPT)
provisioner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(provisioner)

# cired2 #7284. A lib.rs edit that moves one of these hashes fails this test.
ARABIC_SHA = "63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e"
UBUNTU_ARCHIVE_SHA = "a3583a3d112b3a19957189474507ec13431187a582f484d600a46a34150233e8"
DEJAVU_ARCHIVE_SHA = "7576310b219e04159d35ff61dd4a4ec4cdba4f35c00e002a136f00e96a908b0a"
DESKTOP_SHA = {
    "UbuntuMono[wght].ttf": "fbf1e748836994f730e602f7dcf2525564d6d78aa336080cbb73af909d0e08ee",
    "UbuntuMono-Italic[wght].ttf": "4148a3a8e6a702da461992047bcc07179f1eed12a3020384be754d015240678a",
    "Ubuntu[wdth,wght].ttf": "28c4c189a44803b1986fd16074187034dc6d94ad35f5e87de13dd0e786b70b73",
    "Ubuntu-Italic[wdth,wght].ttf": "f5689f6ae7f16f7ede2012702abd605ac1afa03c950713523b9953d600937975",
    "DejaVuSansMono.ttf": "b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582",
    "DejaVuSansMono-Bold.ttf": "bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769",
    "DejaVuSans.ttf": "7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954",
    "DejaVuSans-Bold.ttf": "e6476c1b80502924294eed40894c5b18e06c181444ca953e5334262df9c27724",
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def snippet(ubuntu_faces, dejavu_faces, arabic_name, arabic_sha, ubuntu_bytes, dejavu_bytes) -> str:
    def pairs(faces):
        return ",\n".join(f'("{name}", "{digest(data)}")' for name, data in faces)
    return f"""
const NOTO_SANS_ARABIC_VARIABLE_NAME: &str = "{arabic_name}";
const NOTO_SANS_ARABIC_VARIABLE_URL: &str = "https://fonts.example/arabic.ttf";
const NOTO_SANS_ARABIC_VARIABLE_SHA256: &str = "{arabic_sha}";
const UBUNTU_FONTS_RELEASE: &str = "ubuntu-release";
const UBUNTU_FONTS_ARCHIVE_URL: &str = "https://fonts.example/ubuntu.tar.gz";
const UBUNTU_FONTS_ARCHIVE_SHA256: &str = "{digest(ubuntu_bytes)}";
const UBUNTU_FONTS: [(&str, &str); 4] = [
{pairs(ubuntu_faces)}
];
const DEJAVU_FONTS_RELEASE: &str = "dejavu-release";
const DEJAVU_FONTS_ARCHIVE_URL: &str = "https://fonts.example/dejavu.zip";
const DEJAVU_FONTS_ARCHIVE_SHA256: &str = "{digest(dejavu_bytes)}";
const DEJAVU_FONTS: [(&str, &str); 4] = [
{pairs(dejavu_faces)}
];
"""


def gzip_tar(members: dict) -> bytes:
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w:gz") as archive:
        for name, data in members.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return stream.getvalue()


def zip_bytes(members: dict) -> bytes:
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        for name, data in members.items():
            archive.writestr(name, data)
    return stream.getvalue()


class DisplayFontFixtureTests(unittest.TestCase):
    def test_real_pins_are_the_nine_file_set(self):
        pins = provisioner.load_pins(WORKSPACE)
        files = dict(provisioner.output_files(pins))
        self.assertEqual(
            set(files),
            {f"desktop-fonts/{name}" for name in DESKTOP_SHA}
            | {"NotoSansArabic-variable.ttf"},
        )
        for name, expected in DESKTOP_SHA.items():
            self.assertEqual(files[f"desktop-fonts/{name}"], expected)
        self.assertEqual(files["NotoSansArabic-variable.ttf"], ARABIC_SHA)
        self.assertEqual(pins["UBUNTU_FONTS_ARCHIVE_SHA256"], UBUNTU_ARCHIVE_SHA)
        self.assertEqual(pins["DEJAVU_FONTS_ARCHIVE_SHA256"], DEJAVU_ARCHIVE_SHA)
        self.assertEqual(len(provisioner.cache_key(pins)), 64)

    def test_partial_desktop_directory_is_not_a_hit(self):
        ubuntu_faces = [(f"u{i}.ttf", f"u{i}".encode()) for i in range(4)]
        dejavu_faces = [(f"d{i}.ttf", f"d{i}".encode()) for i in range(4)]
        arabic = b"arabic-bytes"
        ubuntu = gzip_tar(
            {f"ubuntu-release/{name}": data for name, data in ubuntu_faces}
        )
        dejavu = zip_bytes(
            {f"dejavu-release/ttf/{name}": data for name, data in dejavu_faces}
        )
        text = snippet(
            ubuntu_faces,
            dejavu_faces,
            "Arabic.ttf",
            digest(arabic),
            ubuntu,
            dejavu,
        )
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        workspace = root / "ws"
        source = workspace / provisioner.PIN_SOURCE
        source.parent.mkdir(parents=True)
        source.write_text(text)
        pins = provisioner.load_pins(workspace)
        cache = root / "cache"
        os.environ["NEOMACS_FONT_FIXTURE_CACHE"] = str(cache)
        self.addCleanup(os.environ.pop, "NEOMACS_FONT_FIXTURE_CACHE", None)
        (cache / "desktop-fonts").mkdir(parents=True)
        # Seven faces match. The eighth is absent, so the set is not complete.
        for name, data in ubuntu_faces[:3] + dejavu_faces:
            destination = cache / "desktop-fonts" / name
            destination.write_bytes(data)
        (cache / "Arabic.ttf").write_bytes(arabic)
        self.assertFalse(provisioner.all_match(cache, pins))
        calls = []

        def fetch(url):
            calls.append(url)
            if url.endswith("ubuntu.tar.gz"):
                return ubuntu
            if url.endswith("dejavu.zip"):
                return dejavu
            if url.endswith("arabic.ttf"):
                return arabic
            raise AssertionError(url)

        provisioner.provision(workspace, pins, fetch)
        self.assertEqual(len(calls), 3)
        self.assertTrue(provisioner.all_match(cache, pins))

    def test_complete_cache_does_not_download(self):
        ubuntu_faces = [(f"u{i}.ttf", f"u{i}".encode()) for i in range(4)]
        dejavu_faces = [(f"d{i}.ttf", f"d{i}".encode()) for i in range(4)]
        arabic = b"arabic-bytes"
        ubuntu = gzip_tar({f"ubuntu-release/{n}": d for n, d in ubuntu_faces})
        dejavu = zip_bytes({f"dejavu-release/ttf/{n}": d for n, d in dejavu_faces})
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        workspace = root / "ws"
        source = workspace / provisioner.PIN_SOURCE
        source.parent.mkdir(parents=True)
        source.write_text(
            snippet(ubuntu_faces, dejavu_faces, "Arabic.ttf", digest(arabic), ubuntu, dejavu)
        )
        pins = provisioner.load_pins(workspace)
        cache = root / "cache"
        os.environ["NEOMACS_FONT_FIXTURE_CACHE"] = str(cache)
        self.addCleanup(os.environ.pop, "NEOMACS_FONT_FIXTURE_CACHE", None)
        for name, data in ubuntu_faces + dejavu_faces:
            destination = cache / "desktop-fonts" / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        (cache / "Arabic.ttf").write_bytes(arabic)

        def fetch(_url):
            raise AssertionError("complete cache must not download")

        provisioner.provision(workspace, pins, fetch)

    def test_fetch_error_names_the_url(self):
        url = "https://archive.ubuntu.com/ubuntu/pool/font.tar.gz"
        with mock.patch.object(
            provisioner,
            "urlopen",
            side_effect=HTTPError(url, 403, "Forbidden", hdrs=None, fp=None),
        ):
            with self.assertRaises(provisioner.PinError) as caught:
                provisioner.fetch_bytes(url, 64)
        self.assertIn(f"url={url}", str(caught.exception))
        self.assertIn("403", str(caught.exception))
        with mock.patch.object(
            provisioner,
            "urlopen",
            side_effect=URLError("timed out"),
        ):
            with self.assertRaises(provisioner.PinError) as caught:
                provisioner.fetch_bytes(url, 64)
        self.assertIn(f"url={url}", str(caught.exception))

    def test_fetch_sends_a_non_urllib_user_agent(self):
        seen = []

        class Response:
            def read(self, _limit):
                return b"font"
            def __enter__(self):
                return self
            def __exit__(self, *_args):
                return False

        def urlopen(request, timeout=None):
            seen.append((request, timeout))
            return Response()

        with mock.patch.object(provisioner, "urlopen", urlopen):
            data = provisioner.fetch_bytes("https://example.invalid/font.tar.gz", 64)
        self.assertEqual(data, b"font")
        agent = seen[0][0].get_header("User-agent")
        self.assertEqual(agent, "neomacs-font-fixtures")
        self.assertNotIn("Python-urllib", agent)

    def test_workflow_provisions_before_display_tests_and_can_dispatch(self):
        workflow = (WORKSPACE / ".github/workflows/test-suite.yml").read_text()
        self.assertIn("\n          - neomacs-display-tests\n", workflow)
        display = workflow.split("  neomacs-display-tests:", 1)[1].split(
            "  neomacs-prefix-face-tui-parity:", 1
        )[0]
        prepare = "python3 scripts/ci/prefetch-display-font-fixtures.py --workspace"
        self.assertLess(display.index(prepare), display.index("- name: Run display stack tests"))
        self.assertIn("NEOMACS_FONT_FIXTURE_CACHE:", display)
        self.assertIn("display-font-fixtures-${{ steps.display-font-pins.outputs.key }}", display)
        self.assertIn(
            "github.event.inputs.job == 'neomacs-display-tests'",
            workflow.split("  neomacs-test-runtime:", 1)[1].split("  neomacs-display-tests:", 1)[0],
        )
        archive = workflow.split("  neomacs-workspace-test-archive:", 1)[1].split(
            "  neomacs-test-runtime:", 1
        )[0]
        self.assertIn("github.event.inputs.job == 'neomacs-display-tests'", archive)


if __name__ == "__main__":
    unittest.main()
