"""Bounded owned fixture: survive editor exit only after supervisor GO."""
import os
from pathlib import Path
import signal
import sys
import time

root = Path(sys.argv[1])
parent = os.getppid()
signal.signal(signal.SIGHUP, signal.SIG_IGN)
ready = root / "child.ready.tmp"
ready.write_text(f"{os.getpid()} {parent} {os.getpgrp()} {os.getsid(0)}\n")
ready.replace(root / "child.ready")
deadline = time.monotonic() + 30
# Before GO, a failed editor/setup must not leave this fixture running.
# Check GO first so an editor exit immediately after GO cannot hide it.
while not (root / "go").exists():
    if os.getppid() != parent or time.monotonic() >= deadline:
        sys.exit(0)
    time.sleep(0.01)
# After GO, survival across editor exit is the actual observation.
while time.monotonic() < deadline:
    time.sleep(0.02)
