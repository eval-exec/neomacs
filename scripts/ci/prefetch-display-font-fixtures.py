#!/usr/bin/env python3
"""Materialize the display-stack cached font set without building Rust.

The five otf_fixture_tests read only desktop_fonts_cached() and
noto_sans_arabic_variable_cached(). desktop_font_files_match requires all
eight desktop faces, so a partial desktop-fonts directory still fails.
This writes the same cache paths those two functions write:

  <cache>/desktop-fonts/<eight faces>
  <cache>/NotoSansArabic-variable.ttf

Pins are read from neomacs-test-fonts. A file whose SHA-256 already matches
is left in place. The printed cache key is the SHA-256 of those nine
name/hash lines, so the CI cache rolls when a pin changes and not when an
unrelated fixture does.
"""

import argparse
import hashlib
import io
import os
from pathlib import Path
import re
import sys
import tarfile
import tempfile
import zipfile
from urllib.request import urlopen

PIN_SOURCE = Path("crates/neomacs-test-fonts/src/lib.rs")
ARCHIVE_LIMIT = 64 * 1024 * 1024
FILE_LIMIT = 32 * 1024 * 1024
IO_TIMEOUT_SECONDS = 60

STRING_CONST = re.compile(
    r"""\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*"([^"\\\r\n]*)"\s*;""",
    re.DOTALL,
)
PAIR_ARRAY = re.compile(
    r"""\bconst\s+(UBUNTU_FONTS|DEJAVU_FONTS)\s*:\s*\[\(\s*&str\s*,\s*&str\s*\)\s*;\s*4\]\s*=\s*\[(.*?)\]\s*;""",
    re.DOTALL,
)
PAIR = re.compile(r"""\(\s*"([^"\\\r\n]*)"\s*,\s*"([0-9a-f]{64})"\s*,?\s*\)""")
SHA256 = re.compile(r"^[0-9a-f]{64}$")

REQUIRED_STRINGS = (
    "NOTO_SANS_ARABIC_VARIABLE_NAME",
    "NOTO_SANS_ARABIC_VARIABLE_URL",
    "NOTO_SANS_ARABIC_VARIABLE_SHA256",
    "UBUNTU_FONTS_RELEASE",
    "UBUNTU_FONTS_ARCHIVE_URL",
    "UBUNTU_FONTS_ARCHIVE_SHA256",
    "DEJAVU_FONTS_RELEASE",
    "DEJAVU_FONTS_ARCHIVE_URL",
    "DEJAVU_FONTS_ARCHIVE_SHA256",
)


class PinError(ValueError):
    """The font pin source or a downloaded byte string does not match."""


def strip_rust_comments(source: str) -> str:
    """Drop full-line // comments. String lines that contain https:// stay."""
    return "\n".join(
        line for line in source.splitlines() if not line.lstrip().startswith("//")
    )


def parse_string_consts(source: str) -> dict:
    values = {}
    for name, value in STRING_CONST.findall(source):
        if name not in REQUIRED_STRINGS:
            continue
        if name in values:
            raise PinError(f"duplicate font pin const {name}")
        values[name] = value
    missing = [name for name in REQUIRED_STRINGS if name not in values]
    if missing:
        raise PinError("missing font pin consts: " + ", ".join(missing))
    return values


def parse_face_array(source: str, name: str) -> tuple:
    found = [body for array_name, body in PAIR_ARRAY.findall(source) if array_name == name]
    if len(found) != 1:
        raise PinError(f"{name} must be one 4-element (&str, &str) array")
    pairs = tuple(PAIR.findall(found[0]))
    if len(pairs) != 4:
        raise PinError(f"{name} must list exactly four faces")
    for face_name, digest in pairs:
        if not face_name or face_name in (".", "..") or Path(face_name).name != face_name:
            raise PinError(f"{name} face must be a single filename")
        if SHA256.fullmatch(digest) is None:
            raise PinError(f"{name} face {face_name} has a bad SHA-256")
    return pairs


def validate_strings(values: dict) -> None:
    if SHA256.fullmatch(values["NOTO_SANS_ARABIC_VARIABLE_SHA256"]) is None:
        raise PinError("Noto Sans Arabic SHA-256 is not 64 hex digits")
    if SHA256.fullmatch(values["UBUNTU_FONTS_ARCHIVE_SHA256"]) is None:
        raise PinError("Ubuntu archive SHA-256 is not 64 hex digits")
    if SHA256.fullmatch(values["DEJAVU_FONTS_ARCHIVE_SHA256"]) is None:
        raise PinError("DejaVu archive SHA-256 is not 64 hex digits")
    arabic = values["NOTO_SANS_ARABIC_VARIABLE_NAME"]
    if not arabic or arabic in (".", "..") or Path(arabic).name != arabic:
        raise PinError("Noto Sans Arabic name must be a single filename")
    for key in ("NOTO_SANS_ARABIC_VARIABLE_URL", "UBUNTU_FONTS_ARCHIVE_URL", "DEJAVU_FONTS_ARCHIVE_URL"):
        url = values[key]
        if not url.startswith("https://") or " " in url:
            raise PinError(f"{key} must be an https URL")


def load_pins(workspace: Path) -> dict:
    text = strip_rust_comments((workspace / PIN_SOURCE).read_text(encoding="utf-8"))
    values = parse_string_consts(text)
    validate_strings(values)
    values["UBUNTU_FONTS"] = parse_face_array(text, "UBUNTU_FONTS")
    values["DEJAVU_FONTS"] = parse_face_array(text, "DEJAVU_FONTS")
    return values


def output_files(pins: dict) -> tuple:
    """Nine cache-relative paths and their SHA-256, desktop faces first."""
    files = tuple(
        (f"desktop-fonts/{name}", digest) for name, digest in pins["UBUNTU_FONTS"]
    ) + tuple(
        (f"desktop-fonts/{name}", digest) for name, digest in pins["DEJAVU_FONTS"]
    )
    files = files + (
        (
            pins["NOTO_SANS_ARABIC_VARIABLE_NAME"],
            pins["NOTO_SANS_ARABIC_VARIABLE_SHA256"],
        ),
    )
    if len(files) != 9:
        raise PinError(f"display font set must be nine files, found {len(files)}")
    return files


def cache_key(pins: dict) -> str:
    lines = "".join(f"{name} {digest}\n" for name, digest in sorted(output_files(pins)))
    return hashlib.sha256(lines.encode("utf-8")).hexdigest()


def cache_root(workspace: Path) -> Path:
    """Same order as font_fixture_cache_root: env, XDG, HOME, workspace tmp."""
    explicit = os.environ.get("NEOMACS_FONT_FIXTURE_CACHE", "")
    if explicit:
        return Path(explicit)
    xdg = os.environ.get("XDG_CACHE_HOME", "")
    if xdg:
        return Path(xdg) / "neomacs" / "font-fixtures"
    home = os.environ.get("HOME", "")
    if home:
        return Path(home) / ".cache" / "neomacs" / "font-fixtures"
    return workspace / "tmp" / "font-fixtures"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_matches(path: Path, digest: str) -> bool:
    try:
        data = path.read_bytes()
    except FileNotFoundError:
        return False
    return sha256_bytes(data) == digest


def all_match(root: Path, pins: dict) -> bool:
    return all(file_matches(root / name, digest) for name, digest in output_files(pins))


def fetch_bytes(url: str, limit: int, fetch=None) -> bytes:
    if fetch is not None:
        data = fetch(url)
    else:
        with urlopen(url, timeout=IO_TIMEOUT_SECONDS) as response:
            data = response.read(limit + 1)
    if not isinstance(data, (bytes, bytearray)):
        raise PinError(f"url={url}: download did not return bytes")
    data = bytes(data)
    if len(data) > limit:
        raise PinError(f"url={url}: download exceeds {limit}-byte limit")
    return data


def require_sha(data: bytes, digest: str, label: str) -> None:
    actual = sha256_bytes(data)
    if actual != digest:
        raise PinError(f"{label}: SHA-256 {actual}, expected {digest}")


def write_verified(destination: Path, data: bytes, digest: str) -> None:
    require_sha(data, digest, str(destination))
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            dir=destination.parent,
            prefix=f".{destination.name}.",
            suffix=".partial",
            delete=False,
        ) as stream:
            temporary = Path(stream.name)
            stream.write(data)
        os.replace(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def tar_members(data: bytes) -> dict:
    members = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for entry in archive:
            if not entry.isfile():
                continue
            extracted = archive.extractfile(entry)
            if extracted is None:
                continue
            payload = extracted.read(FILE_LIMIT + 1)
            members[entry.name] = payload
    return members


def zip_members(data: bytes) -> dict:
    members = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for info in archive.infolist():
            if info.is_dir():
                continue
            with archive.open(info) as entry:
                members[info.filename] = entry.read(FILE_LIMIT + 1)
    return members


def extract_faces(members: dict, release: str, faces: tuple, prefix: str) -> dict:
    """Return filename -> bytes for the four faces, rejecting oversize members."""
    found = {}
    for name, digest in faces:
        member = f"{release}/{prefix}{name}" if prefix else f"{release}/{name}"
        payload = members.get(member)
        if payload is None:
            raise PinError(f"archive is missing {member}")
        if len(payload) > FILE_LIMIT:
            raise PinError(f"{member} exceeds {FILE_LIMIT}-byte limit")
        require_sha(payload, digest, member)
        found[name] = payload
    return found


def provision(workspace: Path, pins: dict, fetch=None) -> Path:
    root = cache_root(workspace)
    if all_match(root, pins):
        return root
    ubuntu = fetch_bytes(
        pins["UBUNTU_FONTS_ARCHIVE_URL"], ARCHIVE_LIMIT, fetch
    )
    require_sha(ubuntu, pins["UBUNTU_FONTS_ARCHIVE_SHA256"], "ubuntu archive")
    dejavu = fetch_bytes(
        pins["DEJAVU_FONTS_ARCHIVE_URL"], ARCHIVE_LIMIT, fetch
    )
    require_sha(dejavu, pins["DEJAVU_FONTS_ARCHIVE_SHA256"], "dejavu archive")
    arabic = fetch_bytes(
        pins["NOTO_SANS_ARABIC_VARIABLE_URL"], FILE_LIMIT, fetch
    )
    require_sha(arabic, pins["NOTO_SANS_ARABIC_VARIABLE_SHA256"], "noto sans arabic")
    ubuntu_faces = extract_faces(
        tar_members(ubuntu),
        pins["UBUNTU_FONTS_RELEASE"],
        pins["UBUNTU_FONTS"],
        "",
    )
    dejavu_faces = extract_faces(
        zip_members(dejavu),
        pins["DEJAVU_FONTS_RELEASE"],
        pins["DEJAVU_FONTS"],
        "ttf/",
    )
    for name, digest in pins["UBUNTU_FONTS"]:
        write_verified(root / "desktop-fonts" / name, ubuntu_faces[name], digest)
    for name, digest in pins["DEJAVU_FONTS"]:
        write_verified(root / "desktop-fonts" / name, dejavu_faces[name], digest)
    write_verified(
        root / pins["NOTO_SANS_ARABIC_VARIABLE_NAME"],
        arabic,
        pins["NOTO_SANS_ARABIC_VARIABLE_SHA256"],
    )
    if not all_match(root, pins):
        raise PinError(f"display font cache at {root} is still incomplete")
    return root


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument(
        "--print-cache-key",
        action="store_true",
        help="print the nine-file pin key and do not download",
    )
    args = parser.parse_args(argv)
    workspace = args.workspace.resolve()
    try:
        pins = load_pins(workspace)
        if args.print_cache_key:
            print(cache_key(pins))
            return 0
        root = provision(workspace, pins)
    except (OSError, PinError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"FIXTURE-FETCH: workspace={workspace}: {error}", file=sys.stderr)
        return 1
    print(f"verified display font cache {root} key={cache_key(pins)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
