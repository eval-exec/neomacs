"""Keep HUP pending so shutdown ordering is observable without scheduling races."""
import os
from pathlib import Path
import signal
import sys
import time

root = Path(sys.argv[1])
signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGHUP})
parent = os.getppid()
ready = root / "child.ready.tmp"
ready.write_text(str(os.getpid()) + "\n")
ready.replace(root / "child.ready")
# The editor normally exits immediately.  Bound the helper independently of
# the test supervisor, and never survive a departed parent.
deadline = time.monotonic() + 30
while os.getppid() == parent and time.monotonic() < deadline:
    time.sleep(0.01)
