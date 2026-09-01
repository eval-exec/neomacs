# Traps when writing MELPA parity workflows

Things that cost someone an hour, or silently produced a wrong test, while
converting the parity suites. `tmp/neomacs-melpa-tests-standards.md` says what a
good suite is; this says how to avoid getting a bad one by accident.

The dangerous ones are marked **SILENT** — they do not fail, they produce a
passing test that asserts the wrong thing.

---

## Snapshots

**SILENT — `UPDATE_EXPECT=1` under parallel nextest loses updates.** Each test
is its own process patching the same `workflows.rs`; last writer wins, and some
expectations keep their old value while the run reports success. Use
`--test-threads=1` for the UPDATE_EXPECT pass.

**SILENT — serialising is necessary but not sufficient.** `expect-test` records
each macro's position at *compile* time, so once an earlier test's update
rewrites the file, a later test in the same run panics with "Failed to parse
macro invocation" and its snapshot is silently left at the old value, while the
run reports a failure that looks unrelated. The recovery that works: replace the
stale literal with an empty `expect![[r#""#]];` by hand and re-run. After any
UPDATE_EXPECT pass, check that every snapshot you expected to change actually
changed.

Refinement from a second encounter: a literal holding *stale text* may not clear
by re-running, but an **empty** literal does. One suite hit the cascade on 5 of 6
tests with empty literals already in place and a single further pass filled all
five. So if your expectations are still empty, just re-run; hand-editing is only
needed to clear stale text.

**SILENT — a pin behind a step that fails is an *unverified* pin, and it rots.**
A GNU-side `expect!` only asserts anything on a run that reaches it. In a TUI
workflow whose stages assert in sequence, a failure at stage 3 means every pin
from stage 4 on has never been compared to GNU — for as long as stage 3 stays
red, which can be months. Meanwhile the MELPA cache moves under the suite and
those pins quietly stop describing GNU. helm-css-scss had five such pins: two
recorded a 42/7 window split that live GNU stopped producing once
`helm-css-scss-split-window-function` began halving the frame, and nobody saw
it because the workflow was failing earlier at DIVERGENCES.md 114.

So: **when you fix the failure that was gating a workflow, treat every pin
downstream of it as unaudited** and check each against live GNU before
believing a red one is a parity gap. Conversely, a suite that runs green
end-to-end *is* the audit — every pin on the path was compared to GNU on that
run. The exception to check for is a pin inside an `if`: guard the branch on a
condition that pushes to `divergences` when false (as
`helm_gitignore_test.rs`'s `wait_for_progress` does, hard-asserting on the GNU
side and recording a divergence on the Neomacs side), so a skipped pin cannot
pass silently.

**SILENT — `#N=` back references over *strings* are flaky by construction.**
Two equal strings that happen to be the same object print as a back reference
under `print-circle`, and whether that sharing survives the oracle's normaliser
is not stable: the same workflow has produced both the shared and the unshared
form from GNU Emacs on different runs. Have every test helper `copy-sequence`
the strings it returns, so nothing can print as a back reference. This is why
"transcribe from a harness run" is necessary but not sufficient.

Sharing of *conses* is different and stable: it is structural, not incidental.
But stable is not the same as worth pinning. **Pin sharing only when the sharing
is the thing under test.** The `a` suite pins `#1=`/`#1#` markers because a.el's
"immutable" operations really do share alist tails — the aliasing *is* the
feature. A list you are merely using as a value should be `copy-sequence`d.
`custom-enabled-themes` is the cautionary case: `enable-theme` conses onto the
existing list and `disable-theme` returns the same tail, so a workflow capturing
it at four points renders three as back references into the first. That passes,
and it is stable — but it is a claim about cons identity that a theme-layering
workflow does not mean to make, and if Neomacs ever allocated a fresh list there
the red test would say nothing about themes. Copying keeps the order-and-
membership assertion, which is the part carrying the precedence meaning.

**SILENT — `print-circle` leaks into text the *package* writes for the user.**
The notes above concern sharing inside a value you capture; this is the other
direction. anaconda-mode `format`s url's status plist into `*anaconda-response*`,
a buffer the user is meant to read, and with `print-circle` on url's repeated
host string printed as `#1=`/`#1#` *inside that buffer* — reader markers in
package output, and flaky besides. Bind `print-circle` to nil while the command
runs **and while any asynchronous callback lands**, which means the helper that
types the keys and pumps the event loop should be the one binding it.

**SILENT — project-relative paths escape the sandbox normaliser.**
`xref--show-xrefs` groups results by project; the project is the Neomacs
checkout, so the heading is the per-case sandbox's random directory name spelled
*relative* to the repository root. The oracle rewrites the absolute sandbox path
only, so two runs disagree on `...-YwrfNK` vs `...-IAgVWA` and it reads as a
divergence. Any suite whose assertions pass through xref, project.el, or
`file-relative-name` must normalise the workspace-relative spelling too.

**SILENT — never let a snapshot embed a harness path, and never lean on the
oracle's normaliser to hide one.** Two failures of the same shape:

*A pinned harness path breaks later wearing the shape of a package regression.*
An alchemist expectation spelled out `tmp/melpa/package-cache/…`. Acquisition
later moved to the revision-pinned `tmp/melpa/source-install-cache/<upstream>/
<recipe>/<tools>/`, and the case began failing **in both editors** — which reads
as the package having broken, not the harness having moved. Mask it to
`[PACKAGE]`, which is what the assertion meant: the command is
`<elixir> <package>/alchemist-server/run.exs`.

*Masking in your own helper is not optional just because the oracle also masks.*
The same suite's invocation log masked the project path only in its **slashed**
form, while a process's `cwd` is recorded without the trailing slash — so three
snapshots passed only because the oracle's sandbox normaliser caught the
unslashed form on the way out. They would have failed anywhere else, and they
were not asserting what they appeared to. Mask **both** forms in the helper, and
read the recorded snapshot to confirm it says `[PROJECT]` on its own rather than
by the oracle's grace.

Found by the mutation matrix, which has now caught its author twice rather than
the package.

**SILENT — an *index into* sandbox-bearing text escapes the normaliser too, and
it escapes silently because a number does not look like a path.** The normaliser
rewrites the sandbox root inside strings; it cannot rewrite a number that was
*computed from* one. company-go recorded
`(string-match-p "c[0-9]+" <the fake gocode's recorded argv>)` to assert that a
`c<offset>` cursor argument was passed, and that argv quotes the path of the
visited file, so the pinned number was `36 + (length <path>) + 1`. It was 162
where it was written, 154 in the main checkout and 196 in a worktree of the same
commit — and the `:argv` field sitting right beside it, masked to
`[ORACLE-SANDBOX]/…`, hid the very length the number was made of. It is flaky in
place as well: `tempfile` gives each sandbox six random alphanumerics, and a
suffix that puts a `c` next to a digit (about one run in eighty) moves the first
match into the directory name and changes the value by tens.

Record the matched **argument**, not where it was found —
`(cl-find-if (lambda (a) (string-match-p "\\`c[0-9]+\\'" a)) (split-string argv "\n" t))`
pins `"c34"`, the offset the package actually computed, which is also a stronger
claim than any index. Same family as ace_link's avy labels, recorded as buffer
offsets past a quoted sandbox root and re-recorded as line and column
(DIVERGENCES.md 127 and the eleven-suites table).

Two tells. First, **both editors are red** — a Neomacs bug cannot move GNU's
side, so a red GNU pin is always the harness or a stale expectation.
Second, `NEOMACS_MELPA_AUDIT_BATCH_ISOLATION=1` already catches the class as a
side effect: it re-runs the case under a second sandbox whose *label* is a
different length, so a path-length record shows up as `is not batch-safe` with
the two numbers differing by exactly the label-length difference. When that audit
fires and both editors move together, suspect a path-length record before you go
looking for leaked batch state.

**SILENT — scrub a string as a string, before anything formats or prints it.**
A normaliser applied to the *printed* form of a value silently matches nothing,
and **fails open rather than closed**: the volatile data stays in the
expectation. assess's explanation carries a `diff -c` header with a generated
filename and a modification time; the scrubber ran after `(format "%S" …)`, where
`print-escape-control-characters` had turned the header's TAB into the two
characters `\011`, so a regexp looking for a real tab matched nothing. The test
passed on the run that created it and was red on the next one, with two
timestamps baked in. Same family as the `#N=` back-reference entry — the
printer's representation is not the value.

**Transcribe expectations from a harness run, not a raw probe.** The oracle's
normaliser breaks string identity, so probe output containing `#1=` sharing
markers never matches. Applies whenever a value repeats.

**Sometimes a probe cannot reproduce the condition at all, and then a clean
probe is not weak evidence — it is no evidence.** `dir-locals-find-file` stops
its upward walk at `abbreviated-home-dir`, computed from `$HOME` at process
start. A probe driver that sets `HOME` to its own fixture directory therefore
*cannot* see the repository's `.dir-locals.el` above the sandbox: the walk stops
first, the file gets the mode's own `fill-column`, and the trap looks dead. Under
the harness — sandbox below `tmp/melpa/`, `HOME` at `<root>/home` — the same file
gets the repository's 72. One agent nearly wrote "does not apply here" into a
commit on the strength of the clean probe.

So a probe driver must mirror the harness's directory layout and `HOME`, not
merely its package set. And when a probe says a known trap is absent, probe the
*mechanism* rather than trusting the symptom.

**And a probe driver is usually the *more forgiving* environment, which is the
wrong way round for a prototype.** A probe that calls `package-initialize` loads
every installed package, and one of them may `require` a library the harness's
bare session does not have. apiwrap's `:condition-case` config path calls
`byte-compile-warn`, which lives in `bytecomp`: it worked in the probe and
signalled `void-function` under the harness. A defect that reproduces only in the
stricter environment is invisible from the permissive one — so a probe that
passes is not evidence, and only the harness run is.

**`copy-tree` / `copy-sequence` state you capture mid-workflow.** Packages share
cons cells between their variables (amx's `amx-data` and `amx-cache` do), so a
snapshot taken early can render the *final* values under a `#1=` marker and look
self-contradictory beside something captured at the same moment.

Predicates can leak the live structure too: `custom-theme-p` returns a *tail of
`custom-enabled-themes`*, so capturing it across several load/disable cycles
renders the final list under `#N=` markers. Coerce to a boolean —
`(and (custom-theme-p theme) t)`.

## Driving the editor in batch

**`execute-kbd-macro` only reaches the buffer of the selected window.**
`set-window-buffer` first; `with-temp-buffer` alone is not enough and the keys
land in `*scratch*`.

**SILENT — `transient-mark-mode` is nil in batch.** A region set with `C-SPC` is
never active, so any command branching on `use-region-p` takes its no-region
path. adoc-mode's styling commands insert an empty `**` pair instead of
unwrapping. Enable the mode explicitly in the workflow.

**SILENT — `sit-for` / `sleep-for` do not run idle timers in batch.** There is no
command loop to notice idleness, so a workflow that types and then waits
observes nothing. Run the due entries of `timer-idle-list` instead.

**Running timers needs a captured baseline, not a function match.** The editor
has its own pending timers (`undo-auto--boundary-timer` is live in every batch
run), so "run everything in `timer-list`" runs editor internals too. Capture
`timer-list` before the trigger, run only what appeared, and pin how many
appeared. Matching on the timer's printed function is the tempting alternative
and it is editor-specific — Neomacs need not print a closure the way GNU does.

**SILENT — a timer's delay is only assertable as a delta.** Capture a timestamp
immediately before the trigger and assert
`(round (* 10 (- (float-time (timer--time timer)) start)))`, which gives tenths
and is stable. Pinning `timer--time` itself pins wall-clock: it passes locally
forever and means nothing.

**SILENT — one baseline per *call*, not per workflow.** A delta measured from a
timestamp captured before several calls absorbs the time the earlier calls took,
so a slower editor fails on arithmetic rather than behaviour. This produced
three false failures in one suite, all blamed on Neomacs, because the earlier
calls wrote a file. Take a fresh timestamp immediately before each triggering
call, and prefer whole seconds — minutes for anything read back from a file that
stores truncated timestamps.

**A truncated timestamp cannot be asserted in seconds against a sub-second
baseline** — the delta straddles a rounding boundary. Assert in minutes, or
against the in-memory record the value was written from.

**Arbitrary literal text as keys:** `(vconcat (kbd "C-c a") (string-to-vector
"some text") [?\r])`. `kbd` swallows spaces (use `SPC`) and cannot express `[`.

**Log probe output to a file with `write-region`, not `princ`.** `princ` output
is buffered and lost when a hung probe is killed, so a hang looks like "no
output at all" and you cannot tell which form blocked.

**A real Gnus summary buffer is reachable in batch**, with no server and no
network: write an mbox into the sandbox and open it as an `nndoc` ephemeral
group —
`(gnus-group-read-ephemeral-group NAME '(nndoc PATH (nndoc-address PATH) (nndoc-article-type mbox)))`
— which gives a genuine `gnus-summary-mode` buffer with real articles and
threading. Set `gnus-batch-mode`, and:

**SILENT — set `gnus-use-byte-compile` to nil.** With it on, Gnus compiles the
format spec at runtime and that compilation raises "Defining as dynamic an
already lexical var" partway through building the summary. You get a
*half-drawn* summary — first message rendered, second missing — and no error at
the point you are testing.

## Test doubles

**Check `executable-find` before building fixtures.** ac-php cost an agent an
hour because its indexer needs a PHP interpreter that does not exist here. An
environment blocker is a legitimate answer — report it rather than working
around it.

**Record what crossed the boundary, not what the tool can do.** "I ran the real
tool" is weaker than "I ran the real integration and transcribed what came back".
anaconda-mode's payloads are trustworthy because the pinned `anaconda-mode.py`
was run as a real server and real `anaconda-mode-call` was driven at it over a
real socket, so the fixtures carry the server's own serialisation rather than a
reconstruction of it. For a package that *parses a tool's output* rather than
speaking a protocol, that difference is the whole test.

**The strongest case for this is error handling, where an invented fixture
actively hides defects.** android-mode's `android-start-app` tests adb's reply
against `^Error: `. Real adb with no device attached prints
`adb: no devices/emulators found` — which does not match — so the command reports
success and tells the user the activity started. A plausible invented error
string (`Error: no devices found`) *would* have matched, the code would have
looked correct, and the defect would have been invisible. Inventing a fixture
does not merely risk being wrong; it biases the suite toward confirming that the
package's parsing works, because you write the input its parser expects. Record the tool's version
in the commit beside the package pin — a recording is only as good as the version
it came from.

**SILENT — a fixture that is source code in another language must be diffed
against what lands on disk.** ac-php's PHP fixtures live in the prelude as Elisp
string constants, and PHP namespaces use backslashes. Writing `\\\\` in a Rust raw
string where Elisp needs `\\` puts doubled backslashes in the file, and **nothing
errors**: the file is written, the buffer opens, `php-mode` starts, the indexer
runs, the index loads, and completion returns nil for every context — because
`namespace Shop\\Service;` is not the namespace any candidate belongs to. Two
attempts were lost to it; a `diff` against the recording's own input found it.
Diff the file that lands on disk against the file the expectations were derived
from, rather than trusting the constant.

**Two ways a recording goes wrong even when you ran the real tool.** Both hit
the same ameba session. *You captured the wrong process's status:* `$?` after a
pipeline is the **last** command's, so `ameba … | head` records `head`'s exit
status, not the linter's. Capture the status of the process you are recording, in
its own run. *You recorded what you expected rather than what it said:* a fixture
written to trigger a rule the tool does not actually implement produces a
recording with nothing in it for that case — real ameba 1.6.4 does not flag a
redundant `begin`. Read the recording against the fixture before building
assertions on it; a rule you were sure would fire and did not is the cheapest
possible warning that you are reasoning from documentation.

**SILENT — check what a package manager actually gave you, not that it gave you
something.** `nix shell nixpkgs#alan` resolves. It is not the Alan platform
compiler that `alan-mode` drives; it is ALAN 3.0beta8, the *interactive fiction*
language, which shares the name **and** the `.alan` file extension
(`Usage: ALAN <adventure>`). Recording from it would have been real output, from
a real tool, run for real — and completely wrong, while reading as the strongest
possible provenance in a commit message.

This is the **inverse** of the invented-fixture trap: there, fabricated input
flatters the package's parser; here, a name collision makes a wrong recording
look like a right one, and the usual defence — "I ran the real thing" — is
exactly what fails. Read the package description and `--help` output and confirm
it is the tool your package drives, not merely a tool with the right name. An
exit code of 0 proves nothing.

**A missing program is usually a dev-time problem, not a blocker.**
`nix shell nixpkgs#<tool> --command …` obtains the real tool for the recording
step without touching the dev shell, and the committed test does not depend on it
afterwards, because the suite replays the recording through a stand-in. Verified
obtainable this way, neither being on PATH: `clj-kondo` (v2025.09.22) and `php`
(8.4.20). "The binary is not on this host" is a blocker only once you have tried
this and it failed.

**Protocol → implement it. Data format → blocked only if unrecordable.** When the
boundary is a documented wire protocol, stand up the counterparty for real:
ac-sly had no Common Lisp, so the suite ran a `make-network-process :server t`
speaking slynk's framing and connected with the real `sly-connect`. When the
missing program generates the package's *own on-disk format*, standing it in
means authoring the package's data structure — the standards' "reimplementing the
package algorithm inside the expected value" anti-pattern. That is the blocked
case **only when the program cannot be obtained at all**; if it can be run once
off to the side, its real output is a recording rather than a fiction.

**comint/REPL stand-ins need two things.** The response shape the package parses
— inf-ruby does `(butlast (split-string kept "\r?\n") 2)`, so print completions,
then a result line, then the prompt — and the *initial* prompt drained before
the call, or it is already in the accumulator when the package installs its
filter, the wait loop exits immediately, and you get nil candidates with no
request ever sent. Also `inhibit-field-text-motion` for prompt-matching helpers,
and `set-process-query-on-exit-flag` nil so `kill-buffer` does not prompt.

**Make async deterministic.** Wait on the sentinel or `accept-process-output`
until the process is dead. Sort concurrently recorded requests before asserting
— activity-watch's bucket and heartbeat curl processes finish in either order.

**Capture the package's output, not the echo area.** Anything asynchronous the
package merely runs *through* — url.el's `Contacting host: …` progress reports, a
mode's own status chatter — lands in `*Messages*` at a time nobody controls, and
a suite that scoops up the whole region manufactures editor-shaped disagreements
out of scheduling. angry-police-captain was green on GNU and red on Neomacs twice
in a row on exactly this, which reads like a missing-diagnostic divergence and is
a near neighbour of entry 24. A second GNU run produced the *other* answer: GNU
disagreed with itself. Excluding foreign output is not weakening the assertion —
it makes the captured value exactly the package's own `message`.

**So run the GNU side twice before believing a Neomacs mismatch.** The standards
already say to re-run a candidate reduction before reporting it; this is the same
discipline against a different failure. A flaky *reference* is far more
convincing than a flaky subject, because the asymmetry looks like a finding.

**Fixed-duration waits are both the slow answer and the wrong one.** 200 polling
rounds per fetch took angry-police-captain's suite to 100 seconds and *still*
captured too early on one case. Waiting for the observed state to stop changing
took it under 3 seconds and made it correct. There is no duration that is both
long enough and not wasteful; wait on the condition.

**"What is safe to assert" is a separate question from "when to stop waiting",
and the same signal often answers both.** aio draws it precisely. Timer promises
resolve in a defined order, so a select race gives `(fast slow)` every run and is
safely pinnable. Process output is not: **how many chunks a filter receives is
the kernel's choice**, so chunk counts and chunk boundaries are never assertable
even when your wait is correct. Chain on every chunk if the package does, but
wait on the **sentinel** — it fires exactly once — and assert the *joined* text
plus the exit status. The sentinel is both the right thing to wait on and the
only part of the process story deterministic enough to pin.

**SILENT — with `compile`, the last writer *is* the sentinel.** So output
arriving is not completion: the sentinel appends `Compilation finished at …`
after everything the build printed. A capture taken when the expected output
appears catches GNU after that line and Neomacs before it — two byte-identical
buffers differing by one trailing line, which is *exactly* what a real divergence
looks like in the diff. Both editors write it; only the moment of capture
differed. Require `get-buffer-process` to return nil, not merely that the output
you asked for is present. Worth knowing that an agent holding the note below
still got this wrong, because "the output I asked for is here" feels like the
natural stopping condition.

**`flymake-running-backends` is not a completion signal.** It stays non-nil long
after the diagnostics have landed, so a wait loop keyed on it runs to its cap
every time and tells you nothing — slow *and* uninformative. Wait for the
diagnostic list to appear and then stop changing. Same shape as the process and
`compile` notes: the state that sounds like "still working" is not the state that
means "not finished".

**SILENT — waiting for the process to die is not waiting for its output.**
`process-live-p` goes nil while the last output and the sentinel's own line are
still queued, so a capture taken then sees a buffer that is about to change —
and when the package reuses one buffer across invocations, the sentinel lands in
whatever the *next* command put there. android-env's first logcat snapshot had an
empty first capture and a stray `Process Android Logcat finished` at the top of
the second, which reads exactly like a package bug about buffer erasure. Wait for
the buffer text to stop changing across several `accept-process-output` rounds,
not for the process to exit.

**SILENT — a stand-in that answers the same thing regardless of input is a
double for the package's own logic.** If the real tool's answer depends on its
input, the stand-in must depend on it too, or the suite pins the stand-in. Three
independent encounters:

- *Wrong answers look like package behaviour.* anakondo's first stand-in replayed
  the `inventory.core` analysis for every buffer, so completing in
  `inventory.util` returned nil — a plausible-looking assertion that was purely
  the stand-in's limitation. Fixed by recording `util.clj` separately and keying
  the stand-in on the `ns` form in the text actually piped to it.
- *Right answers stop witnessing anything.* android-env's first logcat stand-in
  printed identical text for the unfiltered, tag-filtered and crash invocations,
  so "each call kills the previous process and erases what it printed" was
  asserted by three identical strings. Making it answer per-argv turned that into
  a real observation. Same family as the alignment-fixture rule — make each case
  wrong by a different amount.
- *The defect is already in the snapshot, and nobody reads it.* This is the
  worst of the three, because the suite looks fine and the evidence is sitting
  in the committed expectation. agtags' old corpus pinned the trace line
  `global <-u> <--single-update=>` — an empty path — and passed, because its
  invented stand-in matched on `*' --single-update='*` and appended its "I was
  updated" marker whatever the argument said. The empty argument is agtags'
  save-time update **never working at all** (below), visible in the expectation
  of a green test for as long as it had existed. An argv-keyed replay makes it
  impossible to miss: the stand-in must **fail loudly on an argument vector it
  has no recording for** — write `UNRECORDED` into the trace and exit nonzero —
  because a stand-in that answers "nothing" to an unknown request is
  indistinguishable from the tool finding nothing, which is a legitimate result
  for most CLI tools and therefore reads as data.

**SILENT — `awk -v key=…` expands escape sequences, so a lookup key containing a
backslash silently misses.** POSIX awk processes `\.` in a `-v` assignment as
`.`, so a replay key for `global --result=path -P \.c$` arrived as `.c$`,
matched no row, and the stand-in printed nothing — which for a search tool is
"no matches", not "lookup failed". It was caught only by replaying every
recorded argument vector through the stand-in and diffing against the recording
itself. Pass the key through the environment and read it with
`ENVIRON["…"]`, which does no escape processing — and **diff the stand-in's
whole output against the recording before building assertions on it**, the same
discipline the ac-php entry prescribes for fixture files that land on disk.

**A batch frame has real windows — do not fake window geometry.** `--batch`
gives an 80x25 terminal frame whose windows split, resize and delete like any
other, so `split-window-horizontally`, `set-window-buffer`, `delete-window`,
`window-list` and `window-width` all work, and a package that decides what to
draw from the width it is given can be driven end to end with nothing replaced.
ascii-table's corpus faked it in every file — `ascii-table--width-limit` bound
to a constant in eleven tests, and `walk-windows`, `window-buffer` and
`window-width` replaced together by a list of invented `(name . width)` pairs in
the two tests that were about window geometry specifically — which pinned the
arithmetic exactly and never once witnessed the package's whole purpose, that
the table fits the window. Two real windows recovered three things the fakes
could not reach: that narrowing a window does **not** relayout until something
reverts the buffer (a fake width is only read at render time, so the stale-text
window between the two is invisible to it); that the narrowest of several
windows wins over the *selected* one, which is a distinction only real geometry
forces you to set up; and catalogue entry 40, a one-column error in
`window-body-width` that no invented width pair could have contained.

The reason to reach for a fake here is the opposite of the usual one: not that
the capability is missing but that the real widths are awkward. They are not
awkward, they are just not round — the frame is 80 columns and
`split-window-horizontally 50` leaves 49 usable, because the vertical bar costs
a column. Capture the width in the snapshot beside the result and the number
explains itself.

**`insert` is a stand-in for typing, and it is a bad one.** `insert` does not
run `post-self-insert-hook`, so `electric-indent-mode`, `electric-pair-mode`
and every mode's own post-insert behaviour are silently absent; a document
built with `insert` is a document that arrived by paste, not by keyboard. The
substitute is easy to miss because it is not written as a substitute — there is
no `cl-letf` to notice, just an ordinary-looking `(insert "…")`. Drive the real
path with `(let ((last-command-event ch)) (call-interactively (key-binding
(vector ch))))`, and `(kbd "RET")`/`(kbd "TAB")` for the keys, which is what the
command loop does and what the hooks hang off.

It changes results, not just provenance. astro-ts-mode's corpus builds every
document with `insert` and then calls `indent-region`, and all nine of its
indentation expectations are the nested layout that produces. Typing the same
component through `self-insert-command` with electric indent live gives a
**completely flat** document, every line at column 0 — because an element's
nesting is established by its closing tag, so at the moment `RET` reindents a
line the enclosing elements are still unterminated. Both products are real and
they are not the same product; a suite that only has the second one is pinning
formatting the user never sees while typing.

The general form: **ask whether the corpus reaches the subject the way a user
does, or the way that was convenient to write.** `insert` for typing,
`funcall` for a command, `key-binding` + `call-interactively` for a keystroke,
and a bound constant for a measurement are all the same move — each is one step
closer to the code and one step further from the behaviour, and each skips
whatever the real path would have run on the way.

## Assertions

**Read a theme's display clauses before deciding what the suite can assert.**
The two ancient-* themes are the same package category with opposite correct
suites. ancient-one-dark gates every spec on `(min-colors 89)`, so on the batch
frame it paints exactly two faces and the display facts are part of the finding.
ancient-theme writes all 236 specs as `((t …))` with no clause at all, so it
paints every one and resolved `face-attribute` is the right assertion throughout
— all 73 themed faces that exist, every token of a fontified buffer, and the
faces dired/org/flymake/pulse bring in. Getting this backwards in either
direction produces a suite that looks fine and means little: resolved assertions
on a gated theme measure a frame you do not have, and spec-only assertions on an
ungated theme leave the whole product untested.

Two corollaries from the pair. **A gated theme cannot override an ungated one on
a low-colour terminal** — loading wombat over ancient changes nothing while
manoj-dark, which is also ungated, takes over; pin both clauses beside the
resolved colours so the snapshot says *why* rather than only what, and give the
no-op case a control that really does take over. And **prefer built-in libraries
as the late-loading case**: 163 of ancient-theme's styled faces belong to
libraries that are not loaded, and requiring dired, org, flymake and pulse is a
real user opening a Dired buffer, not a synthetic `defface`. Those four also
carry attribute kinds nothing else in the theme reaches — `:strike-through` on a
broken symlink, wave underlines naming their own colour, fractional heading
scales.

**A `min-colors` clause makes a theme unassertable only if it has no fallback.**
Check for a second clause before concluding a theme cannot be read back. Where
each conditional spec carries a `(t …)` clause — the zenburn construction does —
a batch frame resolves that clause **for real**, and the fallback is precisely
what a user on a low-colour terminal sees. ample-zen-theme is the mixed case that
neither of the two previous rules covers: 412 of its 421 settings are `((t …))`,
and the other nine are `((class color) (min-colors 89))` *with* fallbacks. Those
nine are the most interesting faces in the package — below 89 colours the theme
does not fail to apply, it applies something deliberately different. `mode-line`
and `region` come out inverse video with no colours, `hl-line` bold with no
background, an added diff line `#6abd50` rather than `#6a7550`. Pin both clauses
side by side with `face-spec-set-match-display` for each, so the reason the
fallback won sits beside what it produced.

**A display that cannot satisfy a theme's clause is a finding, not an
obstacle.** The tempting fix is to redefine `display-color-cells` and
`face-spec-set-match-display` so the gated clause matches. That is worse than a
weak assertion: it manufactures a frame neither editor has, and every colour the
suite then asserts belongs to that invented display. ancient-one-dark-theme's old
corpus did exactly this, and it read as the *strongest* corpus replaced so far —
it fontified a real Elisp buffer and read the faces back — while asserting an
appearance neither editor can produce. It is the min-colors note turned inside
out: that note says you cannot assert resolved appearance on this display, and
the answer was to fake a display where you can.

The honest route, which is also more coverage rather than less:

- pin the registered specs **whole** (all 202, as ample-regexps pins its regexps);
- pin the display facts beside them — 0 colour cells, `static-gray`, gated clause
  does not match, `(t …)` clause does — so the reason is on the record;
- assert resolved `face-attribute` only for the settings that genuinely apply
  (11 of the 202 here);
- and list every themed face with the theme **on and off**, so the snapshot
  states exactly which faces the theme changes on this display. For
  ancient-one-dark that is precisely `line-number` and `line-number-current-line`
  — the theme's entire terminal effect, now recorded rather than papered over.

Generalises past themes: whenever a package's behaviour is gated on a capability
the batch editor lacks, pin the gate's *answer* and the behaviour on both sides
of it. Never synthesise the capability.

**SILENT — the repository's own `.dir-locals.el` reaches every file the suite
visits, and can never fail as a parity difference.** The sandbox lives inside the
neomacs worktree, whose top-level `.dir-locals.el` (inherited from GNU Emacs)
binds for *every* mode:

```elisp
((nil . ((tab-width . 8) (sentence-end-double-space . t) (fill-column . 72) …)))
```

Directory-local variables are applied **after** the major mode runs, so they
overwrite what the mode set for itself. asn1-mode does `(setq-local tab-width 4)`
and a visited `.asn1` file in the sandbox gets 8 — changing every indentation
result in the suite. It bites only tests that **visit a file**; `with-temp-buffer`
plus a direct mode call is unaffected, which is why a probe and a real run can
disagree and why an old corpus never saw it. And because both editors read the
same file, it can never surface as a parity failure — only as a suite that
quietly measures the wrong configuration.

Bind `enable-dir-local-variables` to nil around `find-file-noselect`. That
restores the realistic configuration rather than removing one: a user editing an
ASN.1 file in their own project has no such directory above them. Also watch that
`emacs-lisp-mode` there gets `indent-tabs-mode nil`, and c/java/objc get
`(mode . bug-reference-prog)` — so a file-visiting test can silently acquire a
minor mode it never asked for.

This is the second trap of the shape "the sandbox is inside the repository, and
the repository has opinions". Expect more.

**And read it the other way round too: a package that walks up for a marker has
a bug the sandbox will show you.** `ameba-project-root` does
`(cl-find-if fn ameba-project-root-files)` — the *list* is searched in order, so
the **nearest** marker does not win. With the default list `.git` is checked
before `shard.yml`, matches the enclosing checkout, and the linter runs from
there rather than from the shard, changing what every path in its output means.
That is a real quirk for any user whose project sits inside a larger repository,
not a sandbox artefact. When a package resolves a root, pin it under several
marker configurations rather than assuming the harness trap is the whole story.

**SILENT — the sandbox is inside a project, so "outside a project" is
unreachable.** Every sandbox lives at `<workspace>/tmp/melpa/<label>-XXXXXX`,
inside the neomacs worktree, so a fixture directory with no marker of its own is
*not* outside a project — projectile and project.el walk up and find the neomacs
repository:

```elisp
;; a plain sandbox directory: no .projectile, no .git
(list (projectile-project-p) (projectile-project-root))
;; => ("[ORACLE-WORKSPACE]/" "[ORACLE-WORKSPACE]/")
```

Three quiet consequences: a "not within a project" guard test asserts nothing,
because the case cannot arise; `projectile-current-project-files` enumerates the
whole neomacs tree instead of the fixture, which is slow and non-deterministic;
and an assertion about the project root can be the workspace rather than the
fixture and still pass. The third bites easily — reading `(projectile-project-root)`
one `let` too late, after the fixture buffer was killed, returns the workspace and
looks like a plausible value.

Any project-based fixture must plant its own marker file, and every workflow must
assert the project root, so that a fixture which failed to establish itself shows
up as a wrong root instead of a silent test against the entire repository.
`GIT_CEILING_DIRECTORIES` does not help: it stops git walking *above* the
workspace, and the workspace is the problem.

**For the "outside any project" case, use a buffer whose `default-directory` is
the filesystem root.** Nothing needs faking — `/` has no project above it — and
this is the only way to reach a guard that refuses outside a project. Omitting a
marker file does *not* reach it: the test records the guard passing and reads as
coverage. amd-mode's `registry.rs` had exactly that test, recording nil for its
"outside directory" case while distinguishing nothing. Covered for real, all
three amd-mode commands refuse with `(error "Not within a project")` and leave
the buffer and kill ring untouched. Compounds with the project-relative
path note in Snapshots — a fixture that silently resolves to the workspace and a
heading spelled relative to it will agree with each other while both are wrong.

**SILENT — check the package is not silently a no-op in batch.** Several
packages have a path that quietly does nothing on a non-graphical or
noninteractive Emacs, and a suite that does not notice asserts the no-op while
claiming to cover the feature — passing in both editors. Four found so far:

| package | gate | effect in batch |
|---|---|---|
| `all-the-icons-ibuffer` | `all-the-icons-ibuffer-display-predicate` defaults to `display-graphic-p` | icon column renders empty |
| `ada-ts-mode` | `treesit-ready-p` | falls back to a non-treesit path |
| `activity-watch-mode` | its own `noninteractive` guard | refuses to switch on |
| DDSKK (via `ac-skk`) | `(unless noninteractive …)` in `skk-save-jisyo` | no dictionary is written |

Assert the gate itself in its own workflow, both open and closed, so the suite
says which path it exercised.

**SILENT — print characters with `%c`, not `%s`, when building event names.** A
helper formatting keypad events as `(format "M-kp-%s" digit)` with `digit` bound
to `?6` produced `M-kp-54`; the package takes the last character of the symbol
name, so the code under test was "43" while the workflow was named and
documented as ASCII 65. All six tests passed self-consistently, asserting codes
nobody chose. Check one expectation against your own doc comment before trusting
the set.

**SILENT — a fixture name can shadow one of the library's own built-ins.** An
`ample-regexps` fixture used `num`, which is a built-in `rx` name, and it shadowed
silently. Check your fixture's names against the library's namespace before
concluding the package mishandled them.

**`let`-bind a variable only after its library is loaded.** Binding
`byte-compile-verbose` before bytecomp is loaded gives "Defining as dynamic an
already lexical var" — `(require 'bytecomp)` first. Same error surfaces from
inside Gnus when `gnus-use-byte-compile` is left on; see the Gnus note above.

**SILENT — a fixture must be able to tell the two answers apart.** Four buffers
under 1k made `file-size-human-readable` return the same digits as the raw size,
so the human-readable setting would have been asserted with a fixture that could
not distinguish it. A 2048-byte buffer reads `2k` against `2048`. Same principle
as making each element of an alignment fixture wrong by a different amount.

**SILENT — do not take the head of a package's list to mean "the item I just
added".** If the command re-sorts, the head is whichever entry sorts first.
`alarm-clock-set` calls `alarm-clock--list-prepare`, which sorts, so three
workflows agreed with themselves and reported the same alarm three times. Look
the record up by a field you set.

**SILENT — assert at the granularity that separates the mechanisms you are
claiming, not at the granularity of the observable.** `all-the-icons` reaches
`icon-for-file` or `icon-for-dir` depending on `file-directory-p`, and the two
return the *same* codepoint 61462 for their respective fallbacks, differing only
in `:family` and `:v-adjust`. A codepoint-only comparison therefore confirmed a
wrong causal story — whole-string regexp matching in the wrong function —
because it predicted the right observable for the wrong reason. Two agents held
that story between them until the dispatcher was instrumented. Reading each text
property separately is what would have caught it.

Granularity alone is not sufficient, because **ordering** can defeat it: in this
same case the icon was built unconditionally *before* the `member` test believed
to select it, so even a per-property assertion placed after the branch would have
agreed with the wrong story. The formulation that covers both halves: assert
something only the claimed mechanism can produce, *and* confirm the claimed
mechanism is the one that actually ran rather than inferring it from the output.

**Instrument the dispatcher rather than inferring it from the source.** Advising
both candidate entry points and reporting per input which one fires settled two
wrong attributions that reading the code had not — the second being that a guard
"short-circuits before the call", when the call is made three lines earlier and
its result discarded.

**SILENT — a capture read after the next call is a stale capture.** A helper
that resets a shared record on entry, called twice, leaves only the second
result behind; reading both captures at the end of the enclosing `let` reports
the second display twice. It passes, self-consistently, and it makes a
one-match regexp look as though it offered two entries — so the symptom points
at the *package*, not at the test, and reads as a divergence worth filing. Bind
each capture in the `let*` immediately after its own call. Same family as the
`copy-tree` note, but the mechanism is ordering rather than sharing.

**SILENT — scope a `*Warnings*` capture to a mark; the harness has already
written there before your workflow starts.** The oracle `load`s the package
source, and most MELPA packages predate the `lexical-binding` cookie, so
`*Warnings*` already holds `Warning (files): Missing ‘lexical-binding’ cookie in
"…"` before the workflow's first form runs. A whole-buffer capture therefore
records somebody else's sentence. It bites hardest in the case that can least
afford it: add-node-modules-path's "with debugging off the package says nothing"
workflow captured exactly that foreign warning, so it recorded a non-empty
string where the whole claim was emptiness and **could not have failed for the
reason it was named after**. It also baked the source-install-cache path, two
content-addressed hash directories deep, into the expectation, where any change
to the cache layout would have read as a divergence. Mark `*Warnings*` the way
`*Messages*` is conventionally marked and capture only what was added since.
The `*Messages*` half of this is already habit; the `*Warnings*` half is not,
and it is the one that starts out non-empty.

**SILENT — name the text, do not count the line.** ahk-mode's font-lock
workflow located each construct by a hand-counted line number and **four of
eleven were wrong** — `:hotstring` was reading a `MsgBox` line, `:label` a
different one. Every one was green, because a face run is a face run wherever you
take it from: the assertion cannot know it is pointed at the wrong construct, and
the snapshot's label says one thing while its recorded value describes another.
The fix is structural, not more careful counting — locate by content
(`ahk-test-faces-where "::btw::"`), so the label and the subject are the same
string and cannot drift apart. Where you can, do not compute a fixture position
at all.

**The *pairing* is what does the work, not the search.** A position found by
machinery rather than by text is equally safe as long as the text is reported
with it — ameba locates by `compilation-next-error` and returns
`(buffer-name line column line-text)` in one tuple, so a miscount shows up as
different text. Report the number and the text it names together; a number
verified once can drift, a number beside its line cannot.

**A package's compatibility fallback is the path least likely to have been run,
and a bare batch session is exactly what takes it.** Three instances in one day,
all the same mechanism: **the guard tests the *preferred* symbol's availability,
not the fallback's**, so the fallback is entered unchecked and names something no
one loaded.

| package | guard | fallback that breaks |
|---|---|---|
| `apiwrap` | config value is not a function/macro | `byte-compile-warn` — lives in `bytecomp`, unloaded |
| `ac-php-core` | `xref-push-marker-stack` not autoloaded | `find-tag-marker-ring` — only exists once `etags` loads |
| `attrap` | — | the elisp backends ship with Emacs, so the *preferred* path is the safe one |

A full user session loads these libraries incidentally, which is why the paths
survive in the wild untested. Drive them deliberately: assert the failure in a
bare session, then load the library and assert it working. Note this is also why
a probe driver that calls `package-initialize` hides the defect — see the
probe-is-more-forgiving note.

**An error far from its cause is the finding, not an obstacle.**
`ac-php-get-tags-data` treats an absent index as "not built yet", starts a
rebuild, and returns *the rebuild's* value — the symbol
`ac-php-phptags-index-process-filter`. The next caller then signals
`(wrong-type-argument listp ac-php-phptags-index-process-filter)` a long way from
anything to do with a missing file. When a probe fails somewhere implausible,
consider that the implausible error *is* what a user gets.

**Run the hook, do not read it — and in general prefer observable behaviour to
internal representation.** add-hooks' only real decision is whether it was handed
a list of functions or one function, and a lambda is both
(`(and (listp object) (not (functionp object)))`). Asserting the hook's
*contents* would mean pinning a closure's printed form — an implementation detail
where the two editors may differ for reasons that say nothing about the package,
manufacturing a divergence out of a representation choice. Asserting **how many
times something fires and in what order** is the behaviour, and it is portable.

This matters more in a parity suite than in an ordinary test suite: every
internal representation you pin is a place the oracle can go red without anyone
having a bug. Pin representation only when the representation is the subject —
the same rule as structural sharing.

**The recurring species: a repair or transform whose output is wrong in a way
the surrounding machinery cannot notice.** Three packages running, and it is the
strongest evidence this campaign has produced for why workflows assert the
*product* rather than that the operation ran:

| package | what completes normally | what is actually wrong |
|---|---|---|
| `angular-snippets` | `yas-expand` returns, snippet inserted | `<div ng-hide=""class="card">` — attributes jammed, invalid markup |
| `attrap` (footer) | repair applies, file byte-compiles | checkdoc's message carries `’`, so the file provides a symbol named `’sample`, not the feature `sample` |
| `attrap` (LaTeX) | fixer runs, buffer edited | it edits while only *listing* options, returns none, and the user is told "No fixer applies" **after** their buffer changed |

In every case the package's own control flow completes without error, so nothing
in the call, the return value, or the absence of a signal can catch it. Only the
resulting text — or the resulting *symbol* — can. When a package's job is to
produce or change something, the assertion must read what it produced, from the
buffer, after the public command. A test that checks the operation ran is
compatible with all three of these.

**SILENT — a helper whose effect is invisible in its caller's output must be
exercised on both sides.** Every angular-snippets html snippet ends by calling
`ng-snip/maybe-space-after-attr`, which exists to stop the new attribute running
into what follows. Called directly it inserts the space correctly; through a real
`yas-expand` it never takes effect, yielding `<div ng-hide=""class="card">` —
joined and invalid. The trap is that without a baseline for correct, jammed
output reads as simply what the snippet produces. Drive the public route *and*
the helper alone: if they disagree the public route is broken, and if you only
ever look at the public route, what the helper was meant to add is
indistinguishable from it never having been intended. Distinct from the stand-in
notes — the package is not lying about its input, it is silently dropping its own
work.

**A correct assertion is the dangerous case**, because there is nothing to
notice. The android-mode fixture pinned the right list and could not fail for the
reason its test was named after; what exposed it was a peer quoting *counts* from
a four-activity manifest against a three-activity one, and the arithmetic not
lining up. So when forwarding a finding, put the **number** in the report, not
only the behaviour — the number is what another fixture can disagree with.

**Take the whole line when an echo-area capture begins with `[`.** Emacs collapses
a message identical to its predecessor into a `[N times]` suffix on the *existing*
line, so a capture starting after that line sees a bare `("[2 times]")` —
accurate and unreadable. This bites exactly where the repeat is the assertion, as
in angular-snippets' third documentation press echoing the same docstring.

**On a library with a large flat API, confidence about semantics is the hazard,
not ignorance.** Three `f` fixtures were wrong in one package, written by the
agent who had been filing these very notes — because the behaviour *seemed
obvious*. `f-copy` onto an existing name signals `file-already-exists` even when
the destination is an empty directory; the fixture asserted it copies inside, and
so produced identical trees for `f-copy` and `f-copy-contents`, unable to tell
them apart. The same agent read the source for `attrap` and `angular-snippets`
because that behaviour was unfamiliar. **Derive the prediction from the source
first and let the snapshot confirm it, rather than writing from memory and
letting the snapshot correct you** — the snapshot does catch it, but only after
you have built a fixture around the wrong belief.

**SILENT — key a stand-in on the repository *and* the argument vector, not the
argv alone.** Two fixtures can legitimately answer the same command
differently, and with one key the second recording silently overwrites the
first. ahg records `status` and `summary` against two repositories — one plain,
one with an MQ queue — and an argv-only key left both answering with the
queue's output, so the plain-repository workflow asserted the wrong tree while
passing. Distinct from the agtags stand-in note: there the collision is between
*invocations*, here between *fixtures*, and nothing in the run says so.

**SILENT — `RUST_LOG=debug` from the dev shell leaks into recordings.** The nix
develop banner sets it, and any tool with a Rust core honours it: Mercurial 7.1
wrote ANSI-coloured `DEBUG hg::dirstate…` tracing to stderr during a recording
pass, which would have been baked into the fixtures as though the tool had
emitted it. Unset it while recording, and assert every record's stderr is
empty rather than trusting that it is.

**For a tool where "no output" is a legitimate answer, silence is the dangerous
default.** A stand-in that exits nonzero with `UNRECORDED` on an argv it has no
recording for is only half the guard; the other half is that **every workflow
asserts the miss log is empty**. Without it the marker nothing-was-invented
never gets read. It matters most where the package ignores the tool's output
entirely — alda-mode only builds an argv and starts a process, so a stand-in
answering nothing is indistinguishable from success. It caught a real miss in
the alda-mode suite that had already gone green.

**SILENT — an argv fixture stored newline-separated cannot be read back.** The
alda-mode recorder wrote one argument per line, and alda-mode passes a whole
multi-line score to `--code`; read back, that one argument became six, the
replay key was computed over nine arguments instead of four, and every playback
missed. Store recorded arguments NUL-separated. The same ambiguity fragments a
call *log*: fold newlines to a placeholder there, or one invocation appears as
several.

**SILENT — `UPDATE_EXPECT=1` with a package-wide filter rewrites *stale peer
snapshots*, turning pre-existing failures into passes.** The update pass does
not distinguish "this snapshot is empty because I am writing it" from "this
snapshot has been failing since somebody changed the harness". Adding two
workflows to airline-themes and running the update over
`test(~parity_tests::airline_themes::)` silently rewrote two tests in files that
had not been opened: a package path that commit `26124b65b` had moved from
`package-cache/` to `source-install-cache/<sha>/<sha>/<sha>/`, and a git branch
that had gone from `nil` to `"main"`. Both had been red before the session
started, and the rewrite would have erased the evidence inside a commit whose
message said nothing about them.

Scope the update pass to the file being written -- `test(~<pkg>::workflows::)`
-- and afterwards run `git status` over the package directory and confirm only
files you meant to touch are modified. If something else was rewritten, revert
it and find out why it was failing before assuming the new value is right: the
first of the two above still failed after being rewritten, and the second
recorded a behavioural change nobody had explained.

**SILENT — an ineffective display fake is worse than no fake, because it looks
like the question was already asked and answered.** The existing note says not
to manufacture a display so a gated theme clause will match. This is the case
where somebody tried and it did not even work, and the attempt is what stopped
anyone looking again.

alabaster-themes' suite runs every test under a prelude named
`TRUE_COLOR_PRELUDE` that redefines `display-color-cells` to return 16777216.
It reads as though the min-colors problem had been handled. It has not been:
the package's specs are gated on `((class color) (min-colors 256))`, and a
batch frame's visual class is `static-gray`, so the clause fails on
`(class color)` and never on the colour count. Measured both ways,
`face-spec-set-match-display` returns nil with the fake in place exactly as
without it — and 15 tests across `rendering.rs` and `lifecycle.rs` record
`"unspecified-fg"` for every colour under names like "resolves titles todos
links blocks and metadata faces".

The claim is unfalsifiable without the measurement, so make the snapshot carry
it. Pin the clause's halves separately rather than a single boolean:

```elisp
:gate (:matches                   (face-spec-set-match-display '((class color) (min-colors 256)) nil)
       :colour-count-alone-matches (face-spec-set-match-display '((min-colors 256)) nil)
       :class-alone-matches        (face-spec-set-match-display '((class color)) nil))
;; => (:matches nil :colour-count-alone-matches t :class-alone-matches nil)
```

That reads as "the fake works, and it cannot help", which a reader can act on.
Before trusting any display fake already in a suite, run the gate with it and
without it and check the answer actually differs.

**SILENT — a whole-buffer assertion does not witness that the buffer changed
for the reason the test is named after.** This is the sharpest instance of the
class so far, because the failing test was the *strongest-looking* assertion in
its suite — a whole-buffer snapshot, which is what these notes tell everyone to
prefer. An asm-blox workflow named for typing a program into a code box typed at
point 1, which is board chrome. The board it recorded was correct, the test
passed, and the typing had done nothing at all: before and after were
byte-identical and the typed text appeared nowhere.

Two mechanisms hid it, and both are ordinary. `asm-blox-self-insert-command`
gates on `asm-blox-in-box-p` and silently `ding`s outside a box, so a refused
keystroke looks like a keystroke. And `asm-blox-next-cell` cannot rescue you
from the top of the buffer, because its fallback branch walks *backward* toward
`bobp` while point starts before every box — so the obvious "navigate to a cell
first" also does nothing.

The fix is structural, not a better cursor position: assert several things that
cannot all hold unless the change really happened. That workflow now pins
`:typing-changed-the-board`, `:typed-text-visible` and `:line-count-unchanged`
beside the board text — a character was inserted, it is visible, and the chrome
survived. Whenever a test's name claims a *cause*, assert something only that
cause produces, not only the state you expect afterwards.

**SILENT — a name-based sweep finds the tests someone remembered to name
consistently, which is exactly the population least likely to contain the bug.**
Four suites computed a payload digest as `(secure-hash 'sha256 path)`, where
`path` is a filename: `secure-hash` on a string hashes the string, so they
hashed the *path* and never read a byte of content, while also pinning a harness
path in a form nobody can read when it moves. The repair swept for the test name
`..._content_digests_match` and reported the family closed. It was not: `aider`
names its test `..._inventory_and_source_hashes_match` and still had the bug.

The one that got named differently is the one that got written differently.
Sweep the *pattern* — read the argument of every call site, `grep -rn
"secure-hash"` and look at what each one is actually hashing — which takes about
a minute across ninety sites and does not depend on what anyone chose to call
the test.

**SILENT — a wrong buffer name is a no-op in this harness, not an error.** A
workflow that looks for `*hg log:*` when the package creates `*hg log (details):*`
records "no such buffer" and **passes**, because both editors agree a missing
buffer is missing. Two ahg workflows did exactly that while asserting nothing
about the view they were named for. Have the helper report `no-buffer-matching
PREFIX` rather than nil, and read the captured value — the test name is
otherwise the only thing claiming a buffer was examined.

**SILENT — a fixture that cannot fail for the reason the test is named after.**
android-mode's `android-project-main-activities` uses `cl-member-if`, so it
returns the tail from the first match rather than the matches. The workflow
pinned the list correctly and proved nothing: the fixture's launcher activity was
*first*, so the tail was the whole list, and "returns everything after the first
match" and "filters correctly, all three match" produce the same answer. Adding a
query that exactly one activity satisfies made it visible — a correct filter
returns 1, the recorded answer is 2.

**A count is what exposes this class.** Three and three look like agreement; one
and two cannot. When a test is named for a *discrimination*, construct the input
so the wrong behaviour and the right behaviour give different-sized answers, and
pin the size a correct implementation would give beside the recorded one so the
expectation reads without the fixture in hand.

**SILENT — a multi-key sort fixture must tie on the primary key.** Two records
that differ in the field being sorted on are not enough: if the primary
comparison already fully orders the pair, the secondary key is never consulted
and `("author" "title")` and `("author" "year")` produce identical output, which
reads as the sort ignoring its argument. Construct the pair so the primary key
genuinely ties — identical author lists, titles that reverse the year order —
and check that it does.

**A theme's face spec replaces the standard definition, it does not merge with
it.** `face-spec-recalc` applies the defface spec only when no enabled theme has
one, so every attribute the theme omits is *dropped*, not inherited. amber-glow
sets `:bold t` and a foreground on `font-lock-warning-face` and says nothing
about `:inherit`; the stock `:inherit error` is gone while the theme is enabled
and back when it is disabled. A theme that omits `:weight` on a stock-bold face
silently un-bolds it — and a suite that reads only the attributes the theme
*sets* is structurally unable to see that happen.

**SILENT — "unset" has four spellings, and a filter that misses one produces a
green, stable, fictional answer.** A face attribute that is not set reads back
as the *symbol* `unspecified`, as the *strings* `"unspecified-bg"` /
`"unspecified-fg"` (a background or foreground with no theme loaded), or as
`nil` (a dropped `:inherit`). One agent got it wrong twice in a row on the same
helper:

- `(memq value '(unspecified "unspecified-bg" …))` is `eq`-based, so it filtered
  the symbol and kept the strings. An unset background reads as
  `"unspecified-bg"` bare and as `unspecified` under a theme, so **all 28 faces
  appeared to lose their background — 17 losses, stable across runs, entirely
  fictional.**
- `memq` → `member` fixed that and then *hid* the real losses, because a dropped
  `:inherit` is `nil`, not `unspecified`. **Answer: 0 losses.**

The truth was 14 of 28. Both wrong answers were self-consistent and would have
shipped. What caught it was that 17 identical `:background` losses looked too
uniform — **check concrete values against a hand-written probe when a count is
suspiciously round**, and never trust the count alone.

**Caveat, and it is the difference between a true report and an alarming one:
put `face-default-spec` in the report beside the before and after.** Two things
that look identical in a before/after diff are not the same finding, and the
standard spec is the only thing that tells them apart:

- an attribute on a `default` clause or an **unconditional `(t …)`** was in force
  on every display, so losing it is real for every user — in ample-theme,
  `font-lock-warning-face`'s `:inherit error`, `button`'s `:inherit link`,
  `header-line`'s `:inherit mode-line`, `error`'s `(default :weight bold)`;
- an attribute on the `(t …)` **fallback of a colour-conditional spec** was only
  in force because the batch frame reports zero colours, so a user on a real
  terminal never had it — in ample-theme, `show-paren-match`'s
  `:inherit underline`, which sits on the last clause after the colour-conditional
  ones.

Without the standard spec, a green test says "the theme destroys your
matching-paren underline" when it does no such thing on the display the theme
documents. And an attribute survives replacement whenever the theme happens to
set it again — ample restates `:underline t` on `link` and `button`, and
`:slant italic` on `completions-annotations`, so those are not losses at all.
Measured properly, ample sets 550 faces, 34 already exist at bare startup, and
**21 of the 34 lose at least one stock attribute** — that is the mechanism's real
size.

**A test can be wrong about its subject while being right about everything a
reader checks.** This is not the fixture-points-at-the-wrong-thing failure; the
fixture is correct, the path is real, and the assertion still cannot see the
feature. atom-one-dark-theme's `..._registered_hook_applies_real_html_font_lock_workflow`
enters a real `html-mode` buffer, runs the real `after-change-major-mode-hook`,
fontifies for real, and pins the `face` text property of six tokens. Every
signal a reviewer scans for is present and correct. **All six values are
identical with the remapping disabled** — measured, not reasoned about — because
`face-remapping-alist` is buffer-local and consumed by the display engine, and
never touches the `face` property. Six of that test's seven assertions cannot
distinguish the package's whole purpose working from it switched off; only the
`face-remapping-alist` element riding along in the same returned list saves it.

The general shape: **ask what the assertion would read if the feature were
removed, and go and measure it** — do not reason about it, because the reasoning
that picked the assertion in the first place is the reasoning that will clear
it. Where the answer is "the same", the test is documenting the path, not the
behaviour, and something else has to carry the finding. The cheap check is to
disable the feature by its own configuration variable and re-run the same
observation; if the snapshot does not move, the assertion is not about the
feature.

Two live examples of the same discipline catching a *wrong claim of mine* before
it shipped, both in this file's own subject matter. In auto-complete the
automatic trigger looked untested — every test sets `this-command` by hand and
calls `ac-handle-post-command` — but driving it faithfully showed
`call-interactively` sets neither `this-command` nor runs `post-command-hook` in
batch, and even with both simulated and `sit-for` letting the idle timer run,
`ac-start` yields no candidates. The corpus's simulation is what is *reachable*,
not laziness. And in auto-dark a control written to prove the package's `2>&1`
redirect was load-bearing proved the opposite: `shell-command-to-string` merges
standard error itself, so a stderr-only reply is captured with or without it.
Both claims were plausible, both were wrong, and both cost one probe to check.

**If a measurement is not behaviour, do not pin it at all.** An amread-mode
initial-delay assertion read 0 tenths in a probe and 17 under UPDATE_EXPECT on
identical code, because the baseline is taken before the mode-enabling call
returns and so absorbs however long loading its dependencies took. Both numbers
are "correct"; neither is behaviour. The repeat *interval* is stable and was
pinned instead. Dropping an assertion is better than pinning a wall-clock value
that passes here and fails on a loaded machine.

**Pin deltas, not absolute counts,** for anything editor-wide. amx's
`amx-detect-new-commands` returns a total command count; only the +1 per newly
defined command is portable. Same reasoning: keep a package's history length
small enough that the fixture fills it entirely, or real editor commands leak in.

**Themes with a `min-colors` clause cannot be asserted by resolved appearance.**
A batch frame is a 0-colour `mono` display, so the clause matches nothing and
`face-attribute … nil t` is `unspecified` for every themed face in *both*
editors. `display-color-cells` is 0 and neither `tty-color-mode` nor
`set-terminal-parameter` moves it. Pin the registered spec instead — exact
colour strings plus the display clause — and record the display facts with
`face-spec-set-match-display` so the reason is on the record. Themes using
`((t …))` (abyss, acme) are unaffected.

**Before concluding a theme is unassertable, check whether its display class is
itself a customization.** `alect-themes` reads its clause from
`alect-display-class`, which defaults to `((type graphic))` — unsatisfiable in
batch however many colours the display claims — but documents nil, "All
terminals", as a supported value, and a nil clause matches a batch display. So
that family is assertable by *resolved* appearance with no faking at all: pin
the stock graphical-only behaviour once, then set the documented option and read
real colours back through `face-attribute … nil t`.

**SILENT — compute fixture positions, do not eyeball them.** An afterglow
workflow whose fixture had a deliberately empty line was one character short, so
the empty-line guard case exercised a *non-empty* line and asserted an overlay
where the entire point was to assert none. It passed in both editors and looked
right. Same shape as the `transient-mark-mode` trap: a green test asserting the
opposite of its own name.

The fix for the *class*, not just the fixture: **assert text, not arithmetic.**
Make every assertion whole-buffer text rather than a column or offset, and make
each element of the fixture wrong by a *different* amount. Then a miscomputed
position shows up as text and cannot be hidden by an arithmetic mistake shared
between fixture and expectation. align-cljlet's suite is built that way.

**If an indenter consults faces, assert both with and without
`font-lock-ensure`.** actionscript-mode's `as3-count-scope-depth` decides whether
a brace counts by looking at its face, so the indenter only works on a fontified
buffer — a suite that always fontifies would silently depend on that.

## Harness ceilings

**The oracle's normaliser recurses once per cons cell, so a large return value
fails as if the package had.** `neomacs--test-oracle-normalize` calls itself on
`(cdr value)`, so the recursion depth is the list's **length**, not its nesting.
`max-lisp-eval-depth` is 1600 in both editors, and a flat list of a few hundred
elements already signals `excessive-lisp-nesting` — measured identically in GNU
and Neomacs, so it is a harness ceiling and never a divergence.

The damage is in how it presents: the failure arrives as `ERR ` with a broken or
absent payload, inside the wrapper that is supposed to report *the package's*
errors. One suite hit it as `void-variable (neomacs--oracle-error)` and the same
form returned cleanly when run standalone. **Splitting the workflow in two
cleared it**, which is also the better suite — two coherent user stories rather
than one grab-bag return value.

So: if a workflow fails only under the harness, only with a large aggregate
return, and the error names the harness's own variables, suspect this before the
package. Return per-story structures rather than one omnibus value.

**compat elides its fallbacks at load time, so on a modern host they are not
loaded at all.** Measured on GNU Emacs 31 with compat 31.0.0.2: exactly one
`compat--` function is defined after load, and it is `compat--maybe-require`.
`(compat-function assoc)` returns `assoc`, so `(compat-call assoc …)` *is*
`assoc`. There is no second implementation to compare against and no
configuration reaches one — the gating is on `emacs-version` at load. So the
compatibility-fallback note above does **not** apply to compat itself: it is the
one package where the fallback is not the least-run path but the never-loaded
one. What a compat suite can assert is the *contract*: that each API compat
claims for this generation is present **and behaves as documented under its
extended arguments** — `assoc` with TESTFN, `plist-get` with a PREDICATE, `sort`
with keyword arguments. That makes the package a specification of the host, which
is worth more here than testing its dispatch.

**A tight elisp loop ignores `with-timeout`, so some defects are describable but
not assertable.** `f-uniquify` does not return on duplicate input — its docstring
says it "expects no duplicate paths", and the consequence is not an error but
`f--uniquify` spinning until the group count reaches the input count, which two
identical paths never reach. It timed out a suite at 180 seconds and cannot be
pinned. Record it as a comment and keep the workflow inside the precondition;
this is a limit of the harness, not a gap in the suite.

**Drain pending sentinels before asserting — every one, not just the process you
started.** Async processes from an *earlier* probe in the same Emacs can have
their sentinels fire after your `setq`, so the assertion reads back the previous
state. In ac-html-csswatcher that looked exactly like "the package ignores the
failure correctly" and was luck. Wait for every process matching the package's
name prefix, not the one handle you happen to hold. Green and right for the wrong
reason, with the wrong reason in the *fixture* rather than the assertion.

**A MELPA recipe glob can ship a package's own test file, and installing the
package then runs it.** alectryon's recipe is
`:files ("etc/elisp/alectryon*.el")`, which matches `alectryon.el` **and**
`alectryon-tests.el`. That test file contains
`(use-package proof-general :ensure t)` and two more, and `:ensure` expands at
*byte-compile* time — so `package-install-file` reaches for three packages the
recipe never declares, and dies in `package--save-selected-packages` before any
test runs.

Nothing local is at fault: both editors fail identically, and both install the
same tarball fine when the dependencies are reachable. The package's
`Package-Requires` says only `flycheck` and `emacs 25.1`, so a dependency lock
derived from the header — which is the correct way to derive it — cannot predict
this. **A `Package-Requires` header is a claim about what the package needs to
*run*, not about what installing it will *do*.**

Before concluding a package is unbuildable, read its recipe in
`tmp/melpa/build-tools/melpa/recipes/<name>` and check whether `:files` pulls in
anything that executes at compile time. If it does, the honest report is that
the recipe is wider than the author's intent — a user running
`M-x package-install` gets the same file — rather than that the harness or the
lock is wrong.

**haskell-mode's process command queue does not drain in batch.** A real session
starts and the GHCi process runs, but `haskell-process-cmd` never returns to nil,
so `haskell-process-queue-sync-request` — and therefore
`haskell-process-get-repl-completions`, `haskell-process-do-type` and the rest —
blocks forever. **The blockage is the `\4` prompt-marker handshake, not
buffering and not the subprocess**: the pending command is the startup
`:set prompt "\4"`, whose accumulated response holds GHCi's banner and two
`ghci> ` prompts and never a `\4`. `process-connection-type` t (a pty, so GHCi
line-buffers) makes no difference — measured, same result at 300 tenths. Standing
in for the ghci binary cannot help either, since a stand-in sits behind the same
unfinished handshake. Cover the no-session branch and say why the other is
absent.

**`popup-tip` never returns under `--batch`** — it waits for an event to dismiss
the tooltip, so any popup path that really has something to show is unreachable.
Same family as "helm cannot be driven in batch".

## Measuring a suite instead of judging it

**Mutate the package and see which tests go red.** Reading a corpus tells you
what it *looks* like it covers; mutating the package tells you what it actually
catches. aggressive-fill-paragraph was checked with 7 mutated copies against 11
tests, and the targeting came out precise: killing the comments-only dispatch
reddened exactly one workflow, disabling the suppression predicate exactly two,
and four deeper mutations reddened all seven.

It also answered the read-before-replacing question with a measurement rather
than an opinion. The three modules that agent **kept** caught six of the seven
mutations and missed one — `ignore-fill-keys` — entirely, all four staying green.
So the existing corpus was genuinely good *and* had one real hole, which is
exactly the judgement that rule asks for and the only way anyone has demonstrated
it rather than asserted it. Harness at `tmp/afp-mutate.py`.

Use it when you are unsure whether to replace a corpus, and when you want to know
whether a new workflow adds coverage or restates it.

**A half-masked path is worse than an unmasked one, because the next sweep's
grep will not find it.** The repair that makes a test pass is not always the
repair that unpins it. `airline_themes_runtime_has_no_hidden_asset_dependency_and_locates_every_entrypoint`
asserts both `(locate-library "airline-themes")` and `(locate-library
"powerline")` — a **sibling** install, `elpa/powerline-20221110.1956/` next to
`elpa/airline-themes-20250502.1915/`. Masking the package's own directory turns
the test green while leaving the dependency's install path fully spelled out:
still layout-pinned, no longer greppable, and green enough that nobody looks
again. Masking the **elpa root** as `[ELPA]` resolves both and keeps the part
that carries meaning — each package's directory name and version.

The rule is **mask at the layer the layout can move at, not at the string that
happens to be failing.** Ask what the assertion is *for*, and mask the thing
whose stability you are actually relying on. The same question caught the other
member of this species: six digests computed by `(secure-hash 'sha256 source)`
on a *filename*, which hashes the string and not the file, so they were path
assertions laundered through a digest — invisible to a grep for the cache
directory and to a name-based sweep alike, and a re-capture would have re-pinned
the new path just as invisibly.

Both were found by asking what the assertion is for rather than what makes it
green, and in both cases the passing repair was the one that hid the problem.

## Before you replace a corpus

**Read the existing corpus before deciding to replace it.** `workflows.rs` is a
**filename convention from this campaign, not evidence of quality**. A package
converted before the convention existed, or by someone who chose another module
name, reads as unconverted to any filename check — and the pre-flight rule
(`git ls-files` + porcelain) catches *in-flight* work, not *already-good* work.

apiwrap is the case that proves it. Porcelain clean, no `workflows.rs`, so it
read as unconverted; a replacement suite was written and `practical.rs` was
`git rm`'d. Reading it afterwards showed it was already workflow-shaped and
**stronger than the replacement** — a six-method issue lifecycle against a
recording request primitive, `:pre-process-params`/`:pre-process-data`/`:around`
with endpoint-specific overrides, custom `define-error` conditions propagated and
recovered, two backends side by side. It was restored and the new work committed
*additively*.

So the sequence is: pre-flight for collisions → **read the corpus** → replace
only what is genuinely weak (symbol inventories, private helpers with fabricated
inputs, mocked-out subjects), and *add to* what is not. Most corpora in this tree
really are weak, which is exactly why the assumption is dangerous: it has been
right often enough to stop being checked.

**Report it if you find a corpus that was replaced when it should have been
extended.** The conversions already landed were made under that assumption.

## Who edits the shared catalogues

**Agents report; the lead files.** `DIVERGENCES.md` and `HARNESS-NOTES.md` are
not to be edited by a package agent. Send the entry — reduction, both editors'
output, blast radius — and it gets verified in both binaries and filed centrally.

This is a structural rule, not a courtesy, and it replaces the "read the diff
before committing a shared file" discipline below for these two files. That
discipline was followed carefully and still failed three times in one session, in
both directions: one agent's harness note landed inside another's package commit,
and an alchemist divergence landed under an ahungry-theme subject. Every author
was diligent; the shared index simply does not support concurrent append.

What is left of the old rule still applies to any *other* file two agents might
touch at once.

## Committing shared files

**`git commit -- <path>` takes the working-tree copy, so a shared Markdown file
sweeps up whatever a peer has written into it.** With several agents in one
worktree, `DIVERGENCES.md` and this file are routinely dirty with someone else's
in-progress additions, and the first commit to land takes them.

The convention: **read the full diff of a shared file before committing it, and
describe everything in it.** Attribution is not the real risk — the real risk is
a commit whose message documents two entries while its diff contains four, which
makes the history actively misleading to anyone bisecting later. If a sweep
happens, say so in the message and name what came from where. Entries should also
carry their own provenance in the text (the package that surfaced them), so
attribution survives independently of which commit they landed in.

Do **not** run `cargo fmt --all` in a commit retry loop for a Markdown-only
change: it rewrites peers' half-written `.rs` files. Wait for a fmt-clean tree
instead.

**`git commit --amend` silently drops the pathspec the original commit was
scoped with, and the index is shared.** This is the sharpest edge in this
section, and it does more than muddle a commit message — it can leave `main`
**unbuildable**.

`git commit -F msg -- <paths>` commits only those paths. Amending it with
`git commit --amend --no-edit` commits **everything staged**, which in a shared
worktree includes whatever a peer has staged in the seconds since. That is how
`543726dde` — an amend intended to add one missing file to the agitjo suite —
also carried a peer's staged deletions of `alan_mode/flycheck.rs` and
`surface.rs`, while the committed `alan_mode/mod.rs` still declared both modules.
A fresh checkout could not compile the crate, and the failure was nowhere near
either package.

So: **always re-state the pathspec when amending** — `git commit --amend
--no-edit -- <the same paths>` — and afterwards check `git show --stat` names
only what you meant. The same rule that applies to shared Markdown applies to the
shared index, and amending is where it is easiest to forget.

**And the restore leaves dead files that still look like tests.** This is the
second-order form, and it is worse than the sweep. When a swept deletion is
restored, the files come back **on disk** while the `mod.rs` that no longer
declares them is unchanged — and **Rust silently ignores an undeclared module**.
`cargo check` passes, the suite runs its expected count, and two files that read
exactly like live parity tests contribute nothing at all. Nobody is warned.

After any sweep-and-restore, check that every `.rs` in the package directory is
declared and every declaration has a file:

```sh
d=crates/neomacs-melpa-tests/src/parity_tests/<pkg>
grep -o '^mod [a-z_]*' $d/mod.rs | sed 's/mod //' | while read m; do
  test -f "$d/$m.rs" || echo "declared, missing: $m"; done
ls $d/*.rs | xargs -n1 basename | sed 's/\.rs$//' | while read f; do
  test "$f" = mod || grep -q "^mod $f;" $d/mod.rs || echo "on disk, undeclared: $f"; done
```

The agent who hit this noticed only because `git status` stopped showing the
deletions it had staged.

Related, and worth knowing before you reach for it: a repair commit for an
unbuildable `main` is one of the few legitimate uses of `--no-verify`, when the
pre-commit hook's workspace-wide `cargo fmt` check is failing on a peer's
in-flight file that the repair does not touch. Verify the restored files are
individually rustfmt-clean and byte-identical to their pre-sweep content first,
and say so in the message. Leaving `main` broken while waiting on an unrelated
file is worse.

**Do not use a detached retry loop to commit at all.** This is the strongest
rule in this section and it was learned twice. A backgrounded "wait for a clean
tree, then commit" loop keeps running after you have moved on, and it commits
**whatever is dirty when it finally wins**, under **whatever its message file
says at that moment**. Both halves drift:

- `96b446312` carries the `min-colors` note, the `.dir-locals.el` trap *and* the
  "Committing shared files" section, under a subject naming only the first —
  two jobs shared one scratch message path and the later message landed on the
  earlier job's content.
- `f7aa116fe` carries the invented-error-fixture and `compile`-sentinel notes
  under that *same* stale `min-colors` subject, because a zombie loop from the
  previous round was still alive and grabbed the next edits to touch the file.

Giving each job its own message file is not sufficient; the loop still outlives
its purpose. Commit synchronously with a short bounded retry you watch, and
verify with `git show --stat --format=%s` after it lands. A job's own
`COMMITTED`/`PUSHED` line is true and tells you nothing about *what* it
committed.

## Tests that never asserted anything

**SILENT — an empty `expect!` literal ships as a permanently failing test, and
nobody reads it as a bug.** The UPDATE_EXPECT cascade documented under
Snapshots does not only lose *one* update: when a later test in the run cannot
patch itself, its literal is left empty, and if the suite is committed in that
state the test fails in both editors forever. It never reads as a divergence,
because both editors disagree with `""` equally, and it never reads as a gap,
because the test is right there in the file with a plausible name.

Found seven of these across the tree in one sweep — three in `aidermacs`
(one of which was a symbol inventory and was deleted instead), one in
`ai-code`, three in `async-melpa`. Every one had been red since it was
committed. The sweep is one line and worth running after any UPDATE_EXPECT
session, including someone else's:

```sh
grep -rn 'expect!\[\[r#""#\]\]\|expect!\[""\]' crates/neomacs-melpa-tests/src/parity_tests/
```

Two cautions on reading its output. **An empty literal in an *untracked* or
modified file is normal** — that is an agent mid-record, and the empty literal
is how the recording pass is supposed to start; check `git status` on the file
before touching it. And **repairing one can surface a real divergence that the
empty literal was masking**: filling in `aidermacs`' gave entry 37 and
`ai-code`'s gave entry 38, both of which had been invisible for as long as the
suites existed. So the sweep is not tidying — it is a way of finding bugs that
were already caught and then dropped.

**A package directory with clean porcelain is not evidence that nothing was
done.** It is equally consistent with the package being finished and committed.
`git log --oneline -- <pkg dir>` and the presence of the module in `mod.rs`
distinguish the two; porcelain alone does not, and reading it as "untouched"
loses a completed package's worth of work to a redo.

## Mutating the package to test the tests

**A mutation that does not mutate reads as a gap in the test.** Two ways this
went wrong in one session, both making the matrix silently *under*-report:

- `defvar` on an already-bound variable is a **no-op**. A mutation written as
  `(defvar pkg-some-regexp "NEVER-MATCHES")` changes nothing, the workflow stays
  green, and the honest-looking conclusion is "this workflow does not cover the
  regexp". Use `setq`, and sanity-check a mutation by confirming at least one
  workflow reddens.
- A harness that extracts workflows by matching `fn NAME() {` immediately
  followed by `let elisp_form` **skips every test carrying a doc comment**, which
  in practice means it skips exactly the workflows whose behaviour was subtle
  enough to need explaining. Two of `alchemist`'s six were silently outside its
  matrix, and the run reported "4 workflows" without that being wrong enough to
  notice.

Both failures share a shape: the matrix reports a *smaller* claim than you think
it does, and a smaller claim still looks like a passing result. Print the number
of workflows the harness found and check it against the number in the file.


## A per-case timeout that is nearly enough

**SILENT — a marginal timeout produces a test that passes when you
investigate it and fails in the suite.** `ai_code`'s cap was 30 seconds and
one workflow needed about 56, so it passed run alone, failed at 50/52 under
package load, and passed again at 51/52 on the next run. Nothing about that
pattern points at the harness; it reads as non-determinism in the package,
which is the most expensive thing it could be mistaken for.

The distinguishing signal is what the failure does **not** contain: a
timeout has no mismatch text. A value disagreement prints both sides. If a
case fails with no diff, check the suite's `Duration::from_secs` before
touching the package.

**But raise the cap second, not first.** The instinct is to compare against
what other suites allow and move on. Look at what the case is *waiting for*
before doing that: two of that workflow's five arms were negative cases —
the helper is supposed to emit nothing when it has no frame prefix and no
file arguments — and each sat out its entire polling budget waiting for
output that by design never came. Twenty of the fifty-six seconds were
spent proving nothing, in exactly the arms that were working.

Waiting on the process **sentinel as well as** the output took the case from
56s to 8.9s, a sixth of the time, with the recorded values unchanged — which
also confirmed they were never timing-dependent. The cap went up too, as
defence in depth, but on its own it would have preserved a test that wasted
most of its runtime and stayed one slow machine away from red. Same rule as
the fixed-duration-wait note: wait on the condition, and for a case that
expects nothing, "the process exited" is the condition.


## Reducing a divergence

**Re-run a candidate reduction on its own before reporting it.** Some bugs
damage process-wide state — catalogue entry 25 leaves `mode-name` void
everywhere — so a probe file that runs the real trigger first and the candidate
second sees the candidate fail against already-broken state and blames the wrong
form. This is a property of the bug, not carelessness; it caught a careful agent.

**Prefer designing a known divergence out of reach over citing it.** When a
catalogued divergence is not what the package under test is about, build the
suite so it cannot be reached: `all-the-icons-completion` goes through
`completion-metadata-get` — the package's real route, which a UI calls when it
renders candidates — so entry 11 never arises without a minibuffer anywhere, and
passes explicit buffer candidates so entry 13's ordering cannot reach an
assertion. `all-the-icons-ibuffer` does the same with prefixed fixtures and
name-sorted assertions. Citing after the fact leaves a red test that says
nothing new; designing it out leaves a green test that covers the package.

**Assert a private-use glyph as character codes *and* its font family.** Code
lists stay readable and diff-friendly in a snapshot where a raw glyph does not,
but codes alone are not enough: an unknown extension and a directory can resolve
to the *same code point in different fonts*, which a code-only assertion calls
equal.

**Cite an existing catalogue entry rather than re-witnessing it.** Every red test
should be a distinct problem, or the failure count stops carrying information.

**Check which editor disagrees before diagnosing anything.** A test that fails
against **GNU** is not a divergence at all — it is a stale expectation, and the
question is what changed underneath it, not what Neomacs got wrong. Run the
suspect package against GNU first; it costs one run and it decides whether you
are looking for a bug or for a commit.

When it is a stale expectation, find the change that caused it before
re-capturing, because "a file vanished from a payload" and "the build dropped a
file" look identical from the test. The evidence that settles it is usually in
the lock: `26124b65b` switched the suite from installing downloaded MELPA
archive tarballs to building from pinned sources with a pinned `package-build`,
and archive tarballs carry a generated `README-elpa` that a source build does
not — so every expectation captured before that commit and naming `README-elpa`
is stale. Both pins it introduced had never moved since, which is what rules out
a later regression. Twelve packages carry that filename in a committed
expectation.

Two rules fall out of that. **Date the corpus against the infrastructure
commit** — `git log -1 --format=%ad` on both, and an corpus that predates a
build change is a suspect before it is a finding. And **check whether the
failure is a family before repairing it in your own package**: the same
expectation shape in eleven other packages is somebody's sweep, not eleven
independent bugs, and re-capturing one of them quietly hides how big the
repair really is.
