# Copilot review instructions for Neomacs

Neomacs is a Rust rewrite of GNU Emacs. Observable GNU compatibility is a
correctness requirement; read `AGENTS.md` before reviewing.

## Compatibility claims

- The behavioral oracle is GNU Emacs 31.1 at commit
  a360712c9d272d950d8d8255ef74570f7e90b7d9
  (https://github.com/emacs-mirror/emacs). Its C source is not vendored in
  this repository, so it usually cannot be verified from the diff alone.
- Do not report a GNU compatibility deviation unless you can cite the exact
  GNU file and line from that commit and quote the relevant condition. When
  you cannot, ask a question or mark the concern explicitly as needing human
  verification instead of filing it as a finding.
- A code comment that cites a GNU file and line is a claim to verify, not
  evidence.
- Internal architecture may differ from GNU; only observable behavior must
  match. Do not flag an internal design difference as a compatibility bug.

## Review scope

- Prefer findings that are provable from this repository: Rust logic errors,
  ownership and GC rooting, test gaps, unsafe invariants, and error handling.
- Do not claim tests were run. Verification commands and results are reported
  in the PR body; evaluate whether they cover the change.
- Use `cargo nextest`, not `cargo test`, in suggested commands.
