"""Bounded real child; append one native acknowledgement per caught HUP."""
import os
from pathlib import Path
import signal
import sys
import time
root = Path(sys.argv[1])
parent = os.getppid()
fd = os.open(root / "hup-acks", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
def hup(_signal, _frame):
    os.write(fd, b"HUP\n")
signal.signal(signal.SIGHUP, hup)
ready = root / "child.ready.tmp"
ready.write_text(f"{os.getpid()} {parent} {os.getpgrp()} {os.getsid(0)}\n")
ready.replace(root / "child.ready")
deadline = time.monotonic() + 30
while not (root / "go").exists():
    if os.getppid() != parent or time.monotonic() >= deadline:
        sys.exit(0)
    time.sleep(.01)
while time.monotonic() < deadline:
    time.sleep(.01)
