---
applyTo: "crates/**"
excludeAgent: "cloud-agent"
---

# GNU compatibility review

This repository's behavioral oracle is GNU Emacs 31.1 at commit
a360712c9d272d950d8d8255ef74570f7e90b7d9. The GNU source is not vendored in
this repository.

- Cite the exact GNU file and line for every compatibility finding, and quote
  the code you rely on. If you cannot, ask a question instead of reporting a
  defect.
- Do not infer GNU semantics from identifier names, comments, or from similar
  code elsewhere in this repository.
- Treat a code comment's GNU citation as unverified until checked.
- Different internal architecture is acceptable; only observable behavior
  must match.
