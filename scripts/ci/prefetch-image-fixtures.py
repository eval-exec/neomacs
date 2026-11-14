#!/usr/bin/env python3
"""Prepare image pins for archive tests without building a Rust executable."""

import argparse
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import re
import sys
import tempfile
import time
from urllib.request import urlopen
from urllib.parse import urlsplit

PIN_SOURCE = Path("crates/neomacs-infra/src/image_fixtures.rs")
MAX_BYTES = 32 * 1024 * 1024
IO_TIMEOUT_SECONDS = 30
DOWNLOAD_BUDGET_SECONDS = 120
FIELDS = {"NYAN_MODE_NAME", "NYAN_MODE_COMMIT", "NYAN_MODE_URL", "NYAN_MODE_SHA256"}
DECLARATION = re.compile(
    r'\s*pub const ([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*"([^"\\\r\n]*)"\s*;'
)


@dataclass(frozen=True)
class Pin:
    name: str
    commit: str
    url: str
    sha256: str


def parse_pin(source: str) -> Pin:
    """Accept only comment/blank lines and ordinary literal Rust &str constants.

    Deliberately reject escapes, macros, aliases, extra items and duplicates:
    a source change outside this small grammar requires an explicit update.
    """
    text = "\n".join(
        line for line in source.splitlines() if not line.lstrip().startswith("//")
    )
    values = {}
    position = 0
    while text[position:].strip():
        match = DECLARATION.match(text, position)
        if match is None:
            raise ValueError(
                "unsupported image pin declaration; expected ordinary pub const NAME: &str = literal"
            )
        name, value = match.groups()
        if name not in FIELDS or name in values:
            raise ValueError(f"unexpected or duplicate image pin declaration: {name}")
        values[name] = value
        position = match.end()
    if values.keys() != FIELDS:
        raise ValueError(
            f"missing image pin declarations: {', '.join(sorted(FIELDS - values.keys()))}"
        )
    pin = Pin(
        *(
            values[name]
            for name in (
                "NYAN_MODE_NAME",
                "NYAN_MODE_COMMIT",
                "NYAN_MODE_URL",
                "NYAN_MODE_SHA256",
            )
        )
    )
    if (
        not pin.name
        or pin.name in (".", "..")
        or Path(pin.name).name != pin.name
        or "\\" in pin.name
    ):
        raise ValueError("image pin filename must be a single cache filename")
    if re.fullmatch(r"[0-9a-f]{40}", pin.commit) is None:
        raise ValueError("image pin commit must be a lowercase 40-digit hash")
    if re.fullmatch(r"[0-9a-f]{64}", pin.sha256) is None:
        raise ValueError("image pin SHA-256 must be a lowercase 64-digit hash")
    url = urlsplit(pin.url)
    if url.scheme != "https" or not url.netloc or pin.commit not in url.path.split("/"):
        raise ValueError("image pin URL must use HTTPS and contain the pinned commit")
    return pin


def read_pin(workspace: Path) -> Pin:
    return parse_pin((workspace / PIN_SOURCE).read_text(encoding="utf-8"))


def fetch_bytes(url: str) -> bytes:
    deadline = time.monotonic() + DOWNLOAD_BUDGET_SECONDS
    with urlopen(url, timeout=IO_TIMEOUT_SECONDS) as response:
        data = bytearray()
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    f"image download exceeded {DOWNLOAD_BUDGET_SECONDS}s budget"
                )
            # read1 returns available bytes rather than waiting to fill a block.
            chunk = response.read1(min(64 * 1024, MAX_BYTES + 1 - len(data)))
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    f"image download exceeded {DOWNLOAD_BUDGET_SECONDS}s budget"
                )
            if not chunk:
                return bytes(data)
            data.extend(chunk)
            if len(data) > MAX_BYTES:
                raise ValueError(f"image download exceeds {MAX_BYTES}-byte limit")


def prefetch(workspace: Path, pin: Pin, fetch=fetch_bytes) -> Path:
    destination = workspace / "tmp/pinned-fixtures" / pin.name
    try:
        cached = destination.read_bytes()
    except FileNotFoundError:
        cached = None
    if cached is not None and hashlib.sha256(cached).hexdigest() == pin.sha256:
        return destination

    try:
        data = fetch(pin.url)
    except (OSError, ValueError) as error:
        raise ValueError(f"url={pin.url} sha256={pin.sha256}: {error}") from error
    if len(data) > MAX_BYTES:
        raise ValueError(
            f"url={pin.url} sha256={pin.sha256}: exceeds {MAX_BYTES}-byte limit"
        )
    actual = hashlib.sha256(data).hexdigest()
    if actual != pin.sha256:
        raise ValueError(f"url={pin.url} SHA-256 {actual}, expected {pin.sha256}")

    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            dir=destination.parent,
            prefix=f".{pin.name}.",
            suffix=".partial",
            delete=False,
        ) as stream:
            temporary = Path(stream.name)
            stream.write(data)
        os.replace(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return destination


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workspace",
        type=Path,
        required=True,
        help="live nextest --workspace-remap directory",
    )
    args = parser.parse_args(argv)
    workspace = args.workspace.resolve()
    try:
        pin = read_pin(workspace)
        destination = prefetch(workspace, pin)
    except (OSError, ValueError) as error:
        print(f"FIXTURE-FETCH: workspace={workspace}: {error}", file=sys.stderr)
        return 1
    print(f"verified {destination} url={pin.url} sha256={pin.sha256}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
