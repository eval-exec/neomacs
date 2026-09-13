"""Find the nesting ceiling, and say WHICH of the two stacks ended it.

A DIAGNOSTIC, not a CI gate: one rung can take minutes, and the answer is a
number to reason about rather than a pass/fail. `browser_stack_smoke.py` is
the gate. Run this when the stack budget changes, or to re-decide whether
`-z stack-size` is still the lever (as of 2026-09-13 it is not -- see
`LispDepthLimit::CURRENT`).

Three outcomes, deliberately kept apart. Conflating the middle one with the
last is the mistake that made an earlier measurement read "alive past N=6400"
when all that had been established is that the Worker did not die:

  COMPLETED  the form evaluated (or Lisp's own guard signalled) -- the goal
  SURVIVED   no answer inside the timeout, Worker still `ready` -- too slow,
             but NOT a stack breach, so the ladder must keep climbing
  DEAD       SHADOW: RuntimeError: memory access out of bounds
                     (linear-memory shadow stack; `-z stack-size` moves this)
             ENGINE: RangeError: Maximum call stack size exceeded
                     (V8's Worker stack, 500 KiB; NO linker flag moves this)

A page whose Worker answered is still healthy, so rungs that COMPLETE reuse
one browser; only a non-completion forces a fresh 88 MB load.
"""
import re
import sys
import time

sys.path.insert(0, "crates/neomacs-wasm/tests")
from browser_test_support import BrowserEditorHarness, chrome_options
from selenium import webdriver

COMPLETED, SURVIVED, DEAD = "COMPLETED", "SURVIVED", "DEAD"


def form_for(kind, depth):
    """`when` is a MACRO, so it also drives recursive macro expansion;
    `if` is a special form, which skips the expander."""
    opener = "(when t " if kind == "macro" else "(if t "
    return "(progn " + opener * depth + "42" + ")" * depth + ")"


def classify_dead(text):
    t = (text or "").lower()
    if "out of bounds" in t:
        return "SHADOW (RuntimeError: memory access out of bounds)"
    if "maximum call stack" in t or "rangeerror" in t:
        return "ENGINE (RangeError: Maximum call stack size exceeded)"
    return f"OTHER ({(text or '')[:140]})"


class Probe:
    def __init__(self, url, timeout):
        self.url, self.timeout, self.driver, self.editor = url, timeout, None, None

    def open(self):
        self.close()
        self.driver = webdriver.Chrome(options=chrome_options(None, True))
        self.editor = BrowserEditorHarness(self.driver, self.timeout)
        self.editor.install_frame_observer()
        self.driver.get(self.url)
        self.editor.wait_ready()
        # Required, not optional: `dispatch_key` targets the canvas, so input
        # sent before the first presentation is dropped and every rung then
        # reports "alive, no marker" regardless of depth.
        self.editor.wait_for_presentation()

    def close(self):
        if self.driver:
            try:
                self.driver.quit()
            except Exception:
                pass
        self.driver = self.editor = None

    def run(self, kind, depth):
        if self.driver is None:
            self.open()
        form = form_for(kind, depth)
        expr = (f'(insert "R=" (condition-case e (progn {form} "ok") '
                f'(error (symbol-name (car e)))))')
        started = time.monotonic()
        try:
            frame = self.editor.eval_expression(expr, marker="R=")
        except Exception:
            time.sleep(1.0)
            state = self.driver.execute_script(
                "const e=document.querySelector('#browser-status');"
                "return e?[e.dataset.state, e.textContent]:null")
            elapsed = time.monotonic() - started
            self.close()  # either way this page is no longer trustworthy
            if state and state[0] == "ready":
                return SURVIVED, f"no answer in {elapsed:.0f}s, Worker still ready"
            return DEAD, classify_dead(state[1] if state else None)
        elapsed = time.monotonic() - started
        if "excessive-lisp-nesting" in frame:
            return COMPLETED, f"GUARD excessive-lisp-nesting ({elapsed:.0f}s)"
        if "R=ok" in frame:
            return COMPLETED, f"evaluated ({elapsed:.0f}s)"
        m = re.search(r"R=([a-z-]+)", frame)
        return COMPLETED, f"lisp-error {m.group(1) if m else '?'} ({elapsed:.0f}s)"


def main():
    url = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:4176/"
    kind = sys.argv[2] if len(sys.argv) > 2 else "macro"
    timeout = float(sys.argv[3]) if len(sys.argv) > 3 else 60.0

    print(f"=== nesting ceiling: kind={kind} timeout={timeout:.0f}s url={url} ===", flush=True)
    probe = Probe(url, timeout)
    last_completed = None
    first_dead = None
    dead_why = None
    try:
        for depth in (400, 800, 1600, 3200, 6400, 12800, 25600, 51200):
            verdict, detail = probe.run(kind, depth)
            print(f"  depth {depth:6}: {verdict:9} -- {detail}", flush=True)
            if verdict == COMPLETED:
                last_completed = depth
            elif verdict == DEAD:
                first_dead, dead_why = depth, detail
                break
    finally:
        probe.close()

    print("", flush=True)
    print(f"  deepest COMPLETED : {last_completed}", flush=True)
    if first_dead is None:
        print("  no stack breach reached on this ladder -- the wall is above it,", flush=True)
        print("  or the form gets too slow to reach it.", flush=True)
    else:
        print(f"  first DEAD        : {first_dead}  <- {dead_why}", flush=True)


if __name__ == "__main__":
    main()
