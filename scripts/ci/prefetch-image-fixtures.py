#!/usr/bin/env python3
"""Prepare image pins for archive tests without building a Rust executable."""

from __future__ import annotations

import argparse
import contextlib
from dataclasses import dataclass
import errno
import hashlib
from http.client import IncompleteRead, RemoteDisconnected
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import ssl
import stat
import sys
import time
from urllib.request import urlopen
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit

try:
    import fcntl
except ImportError:
    fcntl = None

try:
    import msvcrt
except ImportError:
    msvcrt = None

PIN_SOURCE = Path("crates/neomacs-infra/src/image_fixtures.rs")
MAX_BYTES = 32 * 1024 * 1024
IO_TIMEOUT_SECONDS = 30
DOWNLOAD_BUDGET_SECONDS = 120
FETCH_RETRY_BACKOFF = (2, 5, 15)
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


class DownloadBudgetExceeded(TimeoutError):
    """The shared download budget is exhausted, so a retry cannot help."""


def transient_fetch_error(error: BaseException | str) -> bool:
    # HTTPError is also a URLError: status takes precedence over reason text.
    # All 4xx, including 408/429, are permanent for these immutable pins.
    if isinstance(error, HTTPError):
        return 500 <= error.code < 600
    if isinstance(error, URLError):
        return transient_fetch_error(error.reason)
    if isinstance(
        error, (ssl.SSLCertVerificationError, PermissionError, DownloadBudgetExceeded)
    ):
        return False
    message = str(error).lower()
    if any(
        marker in message
        for marker in (
            "certificate verify failed",
            "certificate verification failed",
            "certificate is not trusted",
            "ssl certificate problem",
        )
    ):
        return False
    if isinstance(
        error,
        (
            TimeoutError,
            ConnectionResetError,
            ConnectionRefusedError,
            ssl.SSLEOFError,
            IncompleteRead,
            RemoteDisconnected,
        ),
    ):
        return True
    if isinstance(error, socket.gaierror):
        return error.errno in (socket.EAI_AGAIN, socket.EAI_NONAME)
    if isinstance(error, OSError) and error.errno in (
        errno.ETIMEDOUT,
        errno.ECONNRESET,
        errno.ECONNREFUSED,
        errno.ENETUNREACH,
        errno.EHOSTUNREACH,
    ):
        return True
    return any(
        marker in message
        for marker in (
            "unexpected eof while reading",
            "temporary failure in name resolution",
            "could not resolve host",
            "timed out",
            "connection timeout",
            "timeout was reached",
            "connection reset",
            "connection refused",
            "network is unreachable",
            "no route to host",
        )
    )


def download_once(url: str, deadline: float) -> bytes:
    with urlopen(url, timeout=IO_TIMEOUT_SECONDS) as response:
        data = bytearray()
        while True:
            if time.monotonic() >= deadline:
                raise DownloadBudgetExceeded(
                    f"image download exceeded {DOWNLOAD_BUDGET_SECONDS}s budget"
                )
            # read1 returns available bytes rather than waiting to fill a block.
            chunk = response.read1(min(64 * 1024, MAX_BYTES + 1 - len(data)))
            if time.monotonic() >= deadline:
                raise DownloadBudgetExceeded(
                    f"image download exceeded {DOWNLOAD_BUDGET_SECONDS}s budget"
                )
            if not chunk:
                # HTTPResponse.read1 can return EOF without IncompleteRead even
                # when a non-chunked Content-Length still has bytes outstanding.
                remaining = getattr(response, "length", None)
                if remaining is not None and remaining > 0:
                    raise IncompleteRead(bytes(data), remaining)
                return bytes(data)
            data.extend(chunk)
            if len(data) > MAX_BYTES:
                raise ValueError(f"image download exceeds {MAX_BYTES}-byte limit")


def fetch_bytes(url: str) -> bytes:
    """Retry only transport/5xx failures, sharing the original time/size budget."""
    deadline = time.monotonic() + DOWNLOAD_BUDGET_SECONDS
    failures = []
    for attempt, delay in enumerate((*FETCH_RETRY_BACKOFF, None), 1):
        if attempt > 1 and time.monotonic() >= deadline:
            raise DownloadBudgetExceeded(
                f"image download exceeded {DOWNLOAD_BUDGET_SECONDS}s budget"
            )
        try:
            return download_once(url, deadline)
        except (OSError, IncompleteRead) as error:
            if not transient_fetch_error(error):
                raise
            if isinstance(error, HTTPError) and error.fp is not None:
                error.close()
            failures.append(f"attempt {attempt}: {error}")
            if delay is None:
                raise OSError(
                    f"image fetch failed after {attempt} attempts:\n"
                    + "\n".join(failures)
                ) from error
            print(
                f"FIXTURE-FETCH: {url}: transient attempt {attempt}: {error}; "
                f"retrying in {delay}s",
                file=sys.stderr,
            )
            time.sleep(delay)


def cache_root(workspace: Path) -> Path:
    """Same order as pinned_fixture_cache_root: env, XDG, HOME, workspace tmp."""
    explicit = os.environ.get("NEOMACS_PINNED_FIXTURE_CACHE", "")
    if explicit:
        return Path(explicit)
    xdg = os.environ.get("XDG_CACHE_HOME", "")
    if xdg and Path(xdg).is_absolute():
        return Path(xdg) / "neomacs" / "pinned-fixtures"
    home = os.environ.get("HOME", "")
    if home:
        return Path(home) / ".cache" / "neomacs" / "pinned-fixtures"
    return workspace / "tmp" / "pinned-fixtures"


def entry_path(root: Path, pin: Pin) -> Path:
    return root / pin.sha256 / pin.name


def lstat_mode(path: Path) -> int | None:
    try:
        return os.lstat(path).st_mode
    except (FileNotFoundError, NotADirectoryError):
        return None


def is_real_dir(path: Path) -> bool:
    """True only when `path` is a real directory, via a single lstat."""
    mode = lstat_mode(path)
    return mode is not None and stat.S_ISDIR(mode)


def is_real_file(path: Path) -> bool:
    """True only when `path` is a real regular file, via a single lstat."""
    mode = lstat_mode(path)
    return mode is not None and stat.S_ISREG(mode)


def entry_matches(destination: Path, sha256: str) -> bool:
    try:
        if not is_real_dir(destination.parent) or not is_real_file(destination):
            return False
        cached = destination.read_bytes()
    except OSError:
        return False
    return hashlib.sha256(cached).hexdigest() == sha256


def create_stage_dir(root: Path, sha256: str) -> Path:
    """Create a unique staging directory respecting the process umask."""
    prefix = f".stage-{sha256[:12]}-"
    for _ in range(64):
        candidate = root / f"{prefix}{secrets.token_hex(8)}"
        try:
            os.mkdir(candidate, 0o777)
            return candidate
        except FileExistsError:
            continue
    raise OSError(
        errno.EEXIST, "could not allocate unique staging directory", str(root)
    )


def remove_stray_non_dir(path: Path) -> bool:
    """Remove a stray file or symlink occupying `<cache_root>/<sha256>`."""
    mode = lstat_mode(path)
    if mode is None or stat.S_ISDIR(mode):
        return False
    try:
        os.unlink(path)
    except FileNotFoundError:
        return True
    except PermissionError:
        # On Windows, directory symlinks require rmdir() rather than unlink().
        if os.name == "nt" and stat.S_ISLNK(mode):
            try:
                os.rmdir(path)
            except FileNotFoundError:
                return True
        else:
            raise
    return True


@contextlib.contextmanager
def pin_lock(root: Path, sha256: str):
    """Acquire the same per-digest lock file used by Rust neomacs_infra::pinned."""
    root.mkdir(parents=True, exist_ok=True)
    lock_path = root / f".{sha256}.lock"
    # Open without truncating so concurrent lock holders on Windows do not hit a
    # sharing violation.
    fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o666)
    stream = os.fdopen(fd, "r+b")
    try:
        if fcntl is not None:
            fcntl.flock(stream.fileno(), fcntl.LOCK_EX)
        elif msvcrt is not None:
            stream.seek(0)
            msvcrt.locking(stream.fileno(), msvcrt.LK_LOCK, 1)
        try:
            yield
        finally:
            if fcntl is not None:
                fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
            elif msvcrt is not None:
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
    finally:
        stream.close()


def prefetch(workspace: Path, pin: Pin, fetch=fetch_bytes) -> Path:
    root = cache_root(workspace)
    destination = entry_path(root, pin)
    if entry_matches(destination, pin.sha256):
        return destination

    with pin_lock(root, pin.sha256):
        if entry_matches(destination, pin.sha256):
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

        stage_dir = None
        try:
            stage_dir = create_stage_dir(root, pin.sha256)
            staged_file = stage_dir / pin.name
            staged_file.write_bytes(data)
            target_dir = destination.parent
            remove_stray_non_dir(target_dir)
            if not is_real_dir(target_dir):
                try:
                    os.rename(stage_dir, target_dir)
                    stage_dir = None
                except OSError:
                    if is_real_dir(target_dir):
                        os.replace(staged_file, destination)
                    elif remove_stray_non_dir(target_dir):
                        os.rename(stage_dir, target_dir)
                        stage_dir = None
                    else:
                        raise
            else:
                os.replace(staged_file, destination)
        finally:
            if stage_dir is not None:
                shutil.rmtree(stage_dir, ignore_errors=True)
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
