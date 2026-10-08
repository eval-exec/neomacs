"""Deterministic pty driver for TTY benchmarks and reproducers.

WHY THIS EXISTS: driving a TTY neomacs with `script -qec` is ~50% flaky --
the identical fixture completed on one run and timed out on the next, which
made every TTY timing meaningless. Waiting on a sentinel FILE rather than on
terminal output makes it deterministic (5/5 runs).

  SENTINEL=path PTY_TIMEOUT=secs TERM=xterm-256color \
    python3 tools/bench/pty-run.py ./target/release/neomacs -nw -Q -l fixture.el

PTY_PERF_RECORD and PTY_PERF_STAT optionally observe only the app inside this
private PTY. The runner remains outside the capture, so terminal draining does
not pollute app attribution.

Known limit: it drains terminal output in a read loop, so a fixture that
redraws thousands of times is bottlenecked by pty I/O rather than by the
application. Keep iteration counts modest, or the app blocks on write.

Original docstring: run a TTY neomacs, wait for a sentinel FILE
(not terminal output), report status. `script` proved flaky as a driver --
the same fixture completed on one run and timed out on the next -- so this
removes `script` from the equation to see whether it was the cause."""
import fcntl, os, pty, select, struct, sys, termios, time, signal
from enum import Enum


class CounterThreadScope(Enum):
    PROCESS = "process"
    MAIN_THREAD = "main-thread"


argv = sys.argv[1:]
cpu = os.environ.get("PTY_CPU", "")
if cpu:
    argv = ["taskset", "-c", cpu] + argv
sentinel = os.environ.get("SENTINEL", "")
output_path = os.environ.get("PTY_OUTPUT", "")
timeout = float(os.environ.get("PTY_TIMEOUT", "120"))
rows = int(os.environ.get("PTY_ROWS", "40"))
cols = int(os.environ.get("PTY_COLS", "120"))
perf_record = os.environ.get("PTY_PERF_RECORD", "")
perf_stat = os.environ.get("PTY_PERF_STAT", "")
if perf_stat:
    perf_argv = [
        "perf", "stat", "--no-big-num", "--field-separator", ",",
        "--output", perf_stat,
        "--event", os.environ.get("PTY_PERF_EVENTS", "cycles:u,instructions:u"),
    ]
    thread_scope = CounterThreadScope(os.environ.get("PTY_PERF_THREAD_SCOPE", "process"))
    if thread_scope is CounterThreadScope.MAIN_THREAD:
        perf_argv.append("--no-inherit")
    perf_control = os.environ.get("PTY_PERF_CONTROL", "")
    if perf_control:
        perf_argv += ["--delay=-1", "--control=" + perf_control]
    argv = perf_argv + ["--"] + argv
elif perf_record:
    perf_argv = [
        "perf", "record", "--quiet", "--no-buildid-cache",
        "--event", os.environ.get("PTY_PERF_EVENT", "cycles:u"),
        "--call-graph", os.environ.get("PTY_PERF_CALL_GRAPH", "lbr"),
    ]
    perf_period = os.environ.get("PTY_PERF_PERIOD", "")
    if perf_period:
        perf_argv += ["--count", perf_period]
    else:
        perf_argv += ["--freq", os.environ.get("PTY_PERF_FREQUENCY", "999")]
    perf_control = os.environ.get("PTY_PERF_CONTROL", "")
    if perf_control:
        perf_argv += ["--delay=-1", "--control=" + perf_control]
    argv = perf_argv + ["--output", perf_record, "--"] + argv
if sentinel and os.path.exists(sentinel):
    os.unlink(sentinel)
pid, fd = pty.fork()
if pid == 0:
    os.environ.setdefault("TERM", "xterm-256color")
    os.execvp(argv[0], argv)
# pty.fork() leaves the terminal at 0x0; a TTY app that sizes its frame from
# the winsize then initializes a zero-area display and never gets going --
# every run timed out, and before the sentinel rework this was the "script
# is ~50% flaky" mystery. Give the child a real terminal size, like
# script(1) does.
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
deadline = time.time() + timeout
out = bytearray()
child_status = None
pty_open = True
while time.time() < deadline:
    # select() with the REMAINING time, so the deadline is actually
    # enforced. A bare blocking os.read() ignores it -- the runner then waits
    # for the child no matter what PTY_TIMEOUT says, which made the timeout
    # decorative and the completion check below unreachable.
    remaining = deadline - time.time()
    if remaining <= 0:
        break
    ready, _, _ = select.select([fd] if pty_open else [], [], [],
                                min(remaining, 0.25))
    if ready:
        try:
            r = os.read(fd, 65536)
            if not r:
                pty_open = False
            else:
                out.extend(r)
        except OSError:
            pty_open = False
    done, status = os.waitpid(pid, os.WNOHANG)
    if done:
        child_status = status
        break

timed_out = child_status is None
if timed_out:
    try:
        # pty.fork makes the child a new session and process-group leader. Kill
        # that complete group only on timeout; after the fixture sentinel, perf
        # still needs to finalize its mmap-backed data file before it exits.
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        _, child_status = os.waitpid(pid, 0)
    except ChildProcessError:
        child_status = None
if output_path:
    with open(output_path, "wb") as output_file:
        output_file.write(out)
# A benchmark that dies partway is the dangerous case: it still produces a
# plausible instruction count, just for less work than asked. Treat "did not
# finish" as an ERROR (non-zero exit), never as a quiet result -- a silently
# truncated baseline once looked like a convincing +16% regression that did
# not exist.
exit_code = (os.waitstatus_to_exitcode(child_status)
             if child_status is not None else None)
ok = (bool(sentinel) and os.path.exists(sentinel)
      and not timed_out and exit_code == 0)
if not ok:
    print("PTY-RUN-INCOMPLETE: sentinel=%r present=%s timed_out=%s exit_code=%r"
          % (sentinel, os.path.exists(sentinel), timed_out, exit_code),
          file=sys.stderr)
    sys.exit(2)
print("SENTINEL_WRITTEN")
