"""Shared Chrome configuration for Neomacs browser integration tests."""

from __future__ import annotations

import os
import shutil

from selenium.webdriver.chrome.options import Options


def chrome_binary(explicit: str | None) -> str | None:
    if explicit:
        return explicit
    configured = os.environ.get("NEOMACS_WASM_CHROME")
    if configured:
        return configured
    for candidate in (
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
    ):
        if resolved := shutil.which(candidate):
            return resolved
    return None


def chrome_options(explicit_binary: str | None, headless: bool) -> Options:
    options = Options()
    if binary := chrome_binary(explicit_binary):
        options.binary_location = binary
    if headless or os.environ.get("NEOMACS_WASM_HEADLESS", "0") != "0":
        options.add_argument("--headless=new")
    options.add_argument("--no-sandbox")
    options.add_argument("--disable-dev-shm-usage")
    return options
