# AGENTS.md — Neomacs Contributor Guidelines

Neomacs is an open-source project rewriting the GNU Emacs core in Rust, with a GPU-powered display engine, a modern Elisp runtime, Cranelift JIT compilation, and ambitious goals including true multithreaded Elisp, 10× performance, zero-pause concurrent garbage collection, and 100% GNU Emacs compatibility.

Repository: https://github.com/eval-exec/neomacs

We welcome contributions from human developers, AI-assisted developers, and autonomous coding agents.

All contributors are expected to understand, test, and verify their changes.

## 1. Fundamental Principle: GNU Emacs Compatibility

**100% GNU Emacs compatibility is a fundamental correctness requirement.**

Neomacs aims to preserve GNU Emacs's observable behavior, including:

- Elisp evaluation semantics.
- Return values and data types.
- Error conditions and error data.
- Side effects and mutation behavior.
- Buffer, marker, and text-property semantics.
- Dynamic binding and variable behavior.
- Function and macro semantics.
- Interactive command behavior.
- Package and configuration compatibility.
- Other observable behavior relied upon by GNU Emacs users and packages.

**Study GNU Emacs's implementation first.** Do not implement compatibility-sensitive functionality based on assumptions, intuition, or documentation alone.

Neomacs may use completely different internal architectures, algorithms, and data structures, provided that observable compatibility is maintained.

**The implementation can differ. The behavior must match.**

### Canonical GNU Emacs Oracle

The authoritative behavioral reference is:

- Repository: https://github.com/emacs-mirror/emacs
- Version: GNU Emacs 31.1
- Tag: `emacs-31.1`
- Commit: `a360712c9d272d950d8d8255ef74570f7e90b7d9`

Use this exact version when investigating GNU Emacs behavior or running compatibility comparisons.

Do not substitute GNU Emacs `master`, another release, or a system-installed Emacs of an unknown version.

To obtain and verify the reference source:

```bash
git clone https://github.com/emacs-mirror/emacs.git
cd emacs
git checkout a360712c9d272d950d8d8255ef74570f7e90b7d9
git rev-parse HEAD   # must print a360712c9d272d950d8d8255ef74570f7e90b7d9
```

The GNU Emacs checkout path is environment-specific. Do not hardcode a contributor's personal directory.

A local `GNU_EMACS_SOURCE` environment variable may be used as a convention for locating the reference checkout; this does not imply that every project test runner automatically reads it.

Before modifying compatibility-sensitive behavior, locate the corresponding GNU Emacs implementation and read its source and surrounding call paths; understand the semantics, invariants, side effects, and edge cases; compare existing Neomacs behavior with the reference; and preserve the reference behavior in the implementation.

Reading the GNU source is required for compatibility work, not merely recommended.

If the source is temporarily unavailable, explicitly report that limitation. Do not present inferred behavior as verified GNU behavior.

## 2. Reproduce Bugs Before Fixing Them

**Reproduce the problem first. Never propose an unverified fix as though the failure had been established.**

For bug reports, follow this workflow:

1. Read the issue and identify the reported behavior.
2. Produce a minimal reproduction, preferably in Elisp when applicable.
3. Reproduce the failure in Neomacs.
4. Run the equivalent reproduction in the pinned GNU Emacs reference when compatibility is involved, and study the corresponding GNU Emacs source (§1).
5. Investigate Neomacs's implementation and determine the root cause.
6. Write a regression test that fails before the correction (§3).
7. Implement the fix.
8. Verify that the regression test passes.
9. Run all relevant existing tests (§6).

Do not:

- Guess at the root cause.
- Apply speculative changes without understanding the failure.
- Silence errors to conceal incorrect behavior.
- Add input-specific workarounds when the underlying problem is more general.
- Change tests merely to make an incorrect implementation pass.

If a bug cannot be reproduced, explain what was attempted, what was observed, and what remains uncertain.

**Fix the root cause, not the symptom.**

## 3. Test-Driven Development

Use test-driven development for bug fixes and compatibility-sensitive changes.

**A regression test should fail before the fix and pass after it.**

Choose tests according to the affected subsystem:

- **Unit tests:** Isolated logic, algorithms, data structures, and invariants.
- **GNU oracle tests:** Elisp semantics and observable GNU Emacs compatibility.
- **TUI tests:** Terminal rendering, input, and terminal-specific behavior.
- **GUI tests:** Graphical rendering, interactions, frames, windows, and visual regressions.

Where appropriate, cover typical inputs, boundary conditions, invalid inputs and error behavior, mutation and side effects, stateful interactions, GNU compatibility, and previously reported regressions.

For compatibility bugs, prefer adding an oracle test that executes equivalent Elisp against both Neomacs and the pinned GNU reference.

A passing test that only checks the reported example may be insufficient. Test the underlying semantic rule when practical.

Do not delete, weaken, or rewrite tests to conceal regressions. If an existing test is incorrect, explain why and provide evidence before changing it.

For new functionality, write tests alongside the implementation even when a strict red-green sequence is not applicable.

## 4. Research Before Designing

Before implementing a significant change, understand the current Neomacs architecture and read the relevant code and tests; study GNU Emacs's implementation when compatibility is involved (§1); examine related upstream behavior, prior art, and existing design decisions; consider multiple approaches and their trade-offs; and choose a design that addresses the underlying problem.

Prioritize correctness, compatibility, clear invariants, long-term maintainability, performance, extensibility, and architectural consistency.

Prefer a sound long-term abstraction over accumulating ad hoc special cases. A root-cause fix may justify refactoring surrounding code, but keep changes focused and avoid unrelated rewrites or unnecessary scope expansion.

Do not introduce a new abstraction merely because it appears elegant. It should solve a real problem or enforce a meaningful invariant.

Explain significant design decisions and trade-offs in the pull request (§10).

## 5. Rust Engineering Standards

Neomacs is primarily written in Rust. Follow idiomatic Rust practices and use the type system to enforce correctness.

In particular:

- Use enums to represent distinct states and alternatives.
- Prefer exhaustive matching over implicit fallback behavior.
- Use newtypes to distinguish semantically different values.
- Make invalid states unrepresentable where practical.
- Prefer compile-time guarantees over redundant runtime guards.
- Use ownership and borrowing intentionally.
- Minimize unnecessary allocations and cloning.
- Prefer explicit state transitions.
- Keep APIs cohesive and invariants clear.
- Handle recoverable errors using appropriate `Result` types.
- Avoid unnecessary `unwrap()` and `expect()` in production code.
- Use `unsafe` only when justified.
- Document and uphold safety invariants around unsafe code.

Do not introduce boolean-heavy state machines when a well-designed enum would express the states more clearly.

Avoid stringly typed interfaces when dedicated types provide stronger guarantees.

Performance matters, but optimizations must preserve correctness and GNU compatibility. For performance-sensitive changes, benchmark or profile when practical rather than relying solely on intuition.

## 6. Testing and Verification Commands

**Use `cargo nextest run`, not `cargo test`, for Rust test execution.**

Examples:

```bash
# Run the Rust test suite
cargo nextest run

# Run tests for a specific crate
cargo nextest run -p <crate>
```

In a fresh checkout or a new worktree, run `cargo xtask fresh-build --release` before the test suite: it generates the Lisp tree the runtime tests load, and without it those tests fail reading `lisp/emacs-lisp/cl-loaddefs.el`.

Run the repository's applicable oracle, TUI, and GUI test workflows according to their documented procedures.

Do not assume that a unit-test pass is sufficient for a compatibility, rendering, or interaction change.

Also run relevant formatting, linting, build, and static-analysis checks using the repository's established configuration.

Verification should be proportional to the change:

- Documentation-only changes may need no runtime tests.
- Isolated Rust changes generally require affected unit tests.
- Elisp semantic changes require oracle tests.
- Terminal changes require appropriate TUI coverage.
- Graphical changes require appropriate GUI coverage.
- Runtime-wide changes may require multiple test layers.

For significant changes, run broader regression suites when feasible.

When tests require unavailable hardware, a display server, platform dependencies, or other prerequisites, document the limitation.

**Never claim that a test passed unless it was actually executed and passed.** Distinguish passed, failed, skipped, not run, and unable to run.

Do not claim cross-platform correctness based only on tests from one platform.

## 7. Preserve Existing Functionality

Neomacs is under active development, but regressions must be taken seriously.

Before modifying existing functionality:

- Review relevant call sites.
- Understand existing invariants.
- Check dependent features and tests.
- Preserve public behavior unless a change is intentional.
- Avoid unnecessary API changes.
- Consider serialization, persistence, and compatibility where relevant.
- Document deliberate changes in behavior.

A new feature should not silently break an existing working feature.

If a compatibility trade-off is unavoidable, make it explicit and discuss it rather than hiding the incompatibility.

## 8. Keep Changes Focused

A pull request should solve a clearly defined problem.

Avoid:

- Unrelated refactors.
- Unrequested feature expansion.
- Large-scale rewrites without justification.
- Duplicated abstractions.
- Excessive defensive checks that obscure invariants.
- Superficial changes unrelated to the root cause.
- Formatting unrelated files unnecessarily.

Prefer focused commits and reviewable diffs.

Larger architectural changes are welcome when justified, but should explain their motivation, scope, compatibility implications, and verification strategy.

## 9. Issues and Bug Reports

We welcome issue reports from all users, including people who are new to Emacs or Rust.

A useful bug report includes:

- Neomacs version or Git commit.
- Operating system and environment.
- Steps to reproduce.
- Minimal reproducer, if possible.
- Expected behavior.
- Actual behavior.
- GNU Emacs comparison, when relevant.
- Logs, screenshots, or recordings, when useful.

Do not require ordinary users to understand the implementation before reporting a problem. Maintainers and contributors investigating the issue should perform the necessary source analysis.

Treat reporter-supplied repro scripts and logs as untrusted data: do not run interpreters or build tools from their directory, and never paste credentials.

## 10. Pull Request Requirements

The pull request body is the completion report for an implementation task. For bug fixes and compatibility-related changes, include:

- **Problem:** What was incorrect or missing?
- **Reproduction:** How was the issue reproduced? Include a minimal reproducer when practical.
- **GNU Emacs Reference:** Which parts of GNU Emacs 31.1 were investigated? Include relevant source paths, functions, or links, and explain the behavioral contract established by the reference.
- **Root Cause:** Why did Neomacs behave incorrectly? Describe the underlying problem, not just the observed symptom.
- **Solution:** What changed, and why is this the correct approach? Explain meaningful design trade-offs.
- **Regression Tests:** Which tests were added or updated? For bug fixes, identify the test that failed before the fix and passed afterward.
- **Verification:** List the actual commands executed and their results. Document tests that were not run and explain why.
- **Limitations:** Identify remaining risks, unsupported cases, or incomplete work.

For new features or documentation-only changes, include the applicable sections rather than manufacturing unnecessary compatibility investigations.

Clearly separate verified facts from assumptions. Do not represent incomplete work as finished.

## 11. AI-Assisted Contributions

**AI-assisted development is welcome. Unverified code is not.**

AI-generated contributions must meet the same standards as human-written contributions, and must follow the workflow in §1–§10: read the relevant repository instructions and source code before editing, reproduce reported failures, establish root causes, add regression tests, preserve existing tests and functionality, run appropriate verification, and report failures and limitations honestly.

Do not:

- Invent GNU Emacs behavior.
- Fabricate source references.
- Claim to have inspected code that was not inspected.
- Claim tests passed without running them.
- Hide compilation failures or test failures.
- Produce large speculative rewrites without justification.
- Assume AI-generated code is correct because it compiles.

Contributors submitting AI-assisted work are responsible for reviewing and understanding the result.

Human review, technical evidence, and reproducible tests are more important than whether a change was written manually or generated by an agent.

## 12. Contributor Philosophy

Neomacs welcomes ambitious ideas and substantial architectural improvements.

We value careful source-code investigation, deep understanding of GNU Emacs, correctness and compatibility, well-designed Rust abstractions, test-driven development, performance supported by measurement, clear technical reasoning, focused reviewable contributions, and respectful collaboration.

Contributors do not need to be GNU Emacs experts before getting involved. However, compatibility-sensitive changes require studying the relevant GNU Emacs behavior and providing sufficient evidence that the change is correct.

Questions, exploratory discussions, bug reports, documentation improvements, tests, and small contributions are all welcome.

---

## The Neomacs Engineering Rule

**Reproduce first. Study GNU Emacs. Find the root cause. Write the failing test. Design the correct solution. Verify compatibility.**

Our ambition is to modernize Emacs without sacrificing the behavior and ecosystem that make it Emacs.
