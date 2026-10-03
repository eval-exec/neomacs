# Timer readers and quit recovery

A timer callback can enter a minibuffer or another key reader while the outer
reader is suspended. Preserve the outer accumulator, echo and published keys
across the timer's `inhibit-quit` binding, callback and unbinding, including Lisp
variable watchers and nonlocal exits. The saved values remain GC roots.

Observing timers still see the pending prefix. Input-method callbacks instead
start with empty published command/raw keys, as before. Neither boundary owns
future input events or completion receipts.

A timer Signal is reported and normal execution resumes. Restore its exact prior
`inhibit-quit` value before that continuation, even if an UNLET watcher signals;
do not replay the watcher or clear deliberately enabled inhibition. Throws retain
their existing propagation. Command/startup error recovery is different: only a
normally returning error reporter clears its reporting inhibition and quit flag.

## Focused tests

From the repository root (no display or GPU required), first generate the two
charset inputs and CL autoloads used by the native source-bootstrap helper.
Charset generation uses the same GNU recipes as `cargo xtask fresh-build`;
GNU Emacs is needed only to generate `cl-loaddefs.el`, not as the test evaluator.

```sh
awk -f admin/charsets/cp51932.awk < etc/charsets/CP932-2BYTE.map \
  > lisp/international/cp51932.el
gzip -dc admin/charsets/glibc/EUC-JP-MS.gz \
  | awk -f admin/charsets/eucjp-ms.awk > lisp/international/eucjp-ms.el
emacs -Q --batch -l lisp/emacs-lisp/loaddefs-gen.el \
  --eval '(loaddefs-generate "lisp/emacs-lisp" "lisp/emacs-lisp/.loaddefs-test.el")'
cargo nextest run --locked -p neovm-core --lib --test-threads 1 -E 'test(keyboard::tests::timer_reader)'
cargo nextest run --locked -p neovm-core --lib --test-threads 1 -E 'test(emacs_core::timer::tests::unlet_inhibition) or test(emacs_core::timer::tests::watcher_boundary)'
cargo nextest run --locked -p neovm-core --lib --test-threads 1 -E 'test(quit_recovery)'
```

These tests use the repository's native runtime-startup helper. They exercise
prefix observation, lone ESC through `read-key`, recursive minibuffer quit and
acceptance, normal/Signal/Throw watcher boundaries, heap roots, future input and
quit inhibition. The Cargo build generates required Unicode Lisp inputs.

## GNU compatibility boundary

GNU keeps global command/raw-key chronology across recursive reads. Neomacs
restores the suspended reader's published vectors after a timer callback. Real
prefix completion and `read-key` consumers remain supported, but code which
inspects the full historical vectors (for example transient-map/prefix-help
logic) can distinguish them. This is not a claim of blanket GNU publication
parity, nor a general unwinder redesign.
