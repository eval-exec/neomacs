//! Practical parity for jedi / jedi-core.  The package is an EPC client
//! over the Python jedi library: completion, call signatures, goto
//! definition, documentation, and the defined-names (imenu) tree all
//! flow through one RPC channel from the buffer to the server and back.
//!
//! The Python backend is environmental: the prelude installs an
//! EPC-speaking stand-in `python' ahead of PATH (the documented
//! jedi:server-command customization points at it) that replays
//! responses recorded from jedi 0.20.0 with exactly the fixture sources
//! the suite authors, substituting the received source path into the
//! recorded module paths.  The package runs its real public path end to
//! end -- server lifecycle, request argument vectors, reply parsing,
//! buffer/window effects -- and the stand-in logs every call so the
//! suite can assert the exact contract the package sent.

use std::time::Duration;

use crate::{CachedMelpaOracle, JEDI_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar jedi--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar jedi--test-bin
  (file-name-as-directory (expand-file-name "bin" jedi--test-root)))
(defvar jedi--test-fixtures
  (file-name-as-directory (expand-file-name "jedi-fixtures" jedi--test-root)))
(defvar jedi--test-log
  (expand-file-name "jedi-calls.log" jedi--test-root))

;; Provenance: pinned upstream 0a92f57dcfd76f1daf6d382d1e2eb437784a71e0.
(defconst jedi--test-upstream-tree
  "bc79acd486975a713095f2de438777906d001350"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst jedi--test-manifest
  '(("jedi.el"
     . "0faad26a54058c5f7c7da677676bcc876d87ca6146b530bc3b85a0ac64f3b626")
    ("jedi-core.el"
     . "65c7d05cb7c5b80866a4bc79e43148e1c9fa62ba278ba7b6d95ccb563c3d46f6")
    ("jediepcserver.py"
     . "143cd075867202993782b469ce0b16b8306c218457cc3590984735a4a7df97b6"))
  "Per-file sha256 of the package-built sources the suite verifies.
jedi.el and jedi-core.el live in their own package directories;
package-build rewrites the upstream `Version:' headers, so the hashes
cover the built forms.")

(defconst jedi--test-standin-b64
  "IyEvdXNyL2Jpbi9lbnYgcHl0aG9uMwoiIiJFUEMgc3RhbmQtaW4gZm9yIGplZGkuZWwgLyBqZWRpLWNvcmUuZWwuCgpSZXBsYXlzIHJlc3BvbnNlcyByZWNvcmRlZCBmcm9tIGplZGkgMC4yMC4wIHdpdGggZXhhY3RseSB0aGUgZml4dHVyZQpzb3VyY2VzIHRoZSBzdWl0ZSBhdXRob3JzIChhcHAucHksIG5hbWVzLnB5KS4gIFNwZWFrcyB0aGUgRVBDIHdpcmUKcHJvdG9jb2w6IHByaW50cyB0aGUgVENQIHBvcnQgb24gc3Rkb3V0LCB0aGVuIHNlcnZlcyBsZW5ndGgtcHJlZml4ZWQKZWxpc3Agc2V4cHMgKCIlMDZ4IiBieXRlIGxlbmd0aCArIHByaW4xICsgbmV3bGluZSwgVVRGLTgpLgoKRXZlcnkgcmVxdWVzdCBpcyBsb2dnZWQgdG8gSkVESV9TVEFORElOX0xPRyAodGhlIHJhdyBwYXlsb2FkKSBzbyB0aGUKc3VpdGUgY2FuIGFzc2VydCB0aGUgZXhhY3QgYXJndW1lbnQgdmVjdG9ycyB0aGUgcGFja2FnZSBzZW50LiAgVGhlCnJlY29yZGVkIG1vZHVsZSBwYXRocyBjYXJyeSBAQEpFREktRElSQEAgLyBAQEpFREktUEFUSEBAIHBsYWNlaG9sZGVycwp0aGF0IGFyZSBzdWJzdGl0dXRlZCB3aXRoIHRoZSByZWNlaXZlZCBzb3VyY2UgcGF0aCBhdCByZXBseSB0aW1lLgoiIiIKaW1wb3J0IG9zCmltcG9ydCBzb2NrZXQKaW1wb3J0IHN5cwoKTE9HX1BBVEggPSBvcy5lbnZpcm9uLmdldCgiSkVESV9TVEFORElOX0xPRyIsICIiKQoKTUFJTl9TUkMgPSAiaW1wb3J0IG9zXG5cbmRlZiBncmVldChuYW1lKTpcbiAgICBcIlwiXCJSZXR1cm4gYSBmcmllbmRseSBncmVldGluZy5cIlwiXCJcbiAgICByZXR1cm4gXCJoZWxsbyBcIiArIG5hbWVcblxuY2xhc3MgQ291bnRlcihvYmplY3QpOlxuICAgIFwiXCJcIkEgc2ltcGxlIGNvdW50ZXIuXCJcIlwiXG4gICAgZGVmIF9faW5pdF9fKHNlbGYsIHN0YXJ0PTApOlxuICAgICAgICBzZWxmLnZhbHVlID0gc3RhcnRcblxuICAgIGRlZiBpbmNyZW1lbnQoc2VsZik6XG4gICAgICAgIHNlbGYudmFsdWUgKz0gMVxuICAgICAgICByZXR1cm4gc2VsZi52YWx1ZVxuXG5cbmRlZiBtYWluKCk6XG4gICAgcHJpbnQoZ3JlZXQoXCJ3b3JsZFwiKSlcbiAgICBjID0gQ291bnRlcigpXG4gICAgYy5pbmNyZW1lbnQoKVxuICAgIHByaW50KG9zLmdldGN3ZCgpKVxuIgpOQU1FU19TUkMgPSAiZGVmIGdyZWV0KG5hbWUpOlxuICAgIFwiXCJcIlJldHVybiBhIGZyaWVuZGx5IGdyZWV0aW5nLlwiXCJcIlxuICAgIHJldHVybiBcImhlbGxvIFwiICsgbmFtZVxuXG5jbGFzcyBDb3VudGVyKG9iamVjdCk6XG4gICAgXCJcIlwiQSBzaW1wbGUgY291bnRlci5cIlwiXCJcbiAgICBkZWYgX19pbml0X18oc2VsZiwgc3RhcnQ9MCk6XG4gICAgICAgIHNlbGYudmFsdWUgPSBzdGFydFxuXG4gICAgZGVmIGluY3JlbWVudChzZWxmKTpcbiAgICAgICAgc2VsZi52YWx1ZSArPSAxXG4gICAgICAgIHJldHVybiBzZWxmLnZhbHVlXG4iCgojIChtZXRob2QsIHNvdXJjZS1rZXksIGxpbmUsIGNvbHVtbiwgcmVzcG9uc2UpClJFU1BPTlNFUyA9IFsKICAgICgiY29tcGxldGUiLCAiTUFJTiIsIDIxLCAxNiwgW3sid29yZCI6ICJnZXRfYmxvY2tpbmciLCAiZG9jIjogImdldF9ibG9ja2luZyhmZDogaW50LCAvKSAtPiBib29sXG5cbkdldCB0aGUgYmxvY2tpbmcgbW9kZSBvZiB0aGUgZmlsZSBkZXNjcmlwdG9yLlxuXG5SZXR1cm4gRmFsc2UgaWYgdGhlIE9fTk9OQkxPQ0sgZmxhZyBpcyBzZXQsIFRydWUgaWYgdGhlIGZsYWcgaXMgY2xlYXJlZC4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldF9ibG9ja2luZyIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldF9leGVjX3BhdGgiLCAiZG9jIjogImdldF9leGVjX3BhdGgoZW52OiBNYXBwaW5nW3N0ciwgc3RyXSB8IE5vbmU9Tm9uZSkgLT4gbGlzdFtzdHJdXG5cblJldHVybnMgdGhlIHNlcXVlbmNlIG9mIGRpcmVjdG9yaWVzIHRoYXQgd2lsbCBiZSBzZWFyY2hlZCBmb3IgdGhlXG5uYW1lZCBleGVjdXRhYmxlIChzaW1pbGFyIHRvIGEgc2hlbGwpIHdoZW4gbGF1bmNoaW5nIGEgcHJvY2Vzcy5cblxuKmVudiogbXVzdCBiZSBhbiBlbnZpcm9ubWVudCB2YXJpYWJsZSBkaWN0IG9yIE5vbmUuICBJZiAqZW52KiBpcyBOb25lLFxub3MuZW52aXJvbiB3aWxsIGJlIHVzZWQuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRfZXhlY19wYXRoIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0X2hhbmRsZV9pbmhlcml0YWJsZSIsICJkb2MiOiAiZ2V0X2hhbmRsZV9pbmhlcml0YWJsZShoYW5kbGU6IGludCwgLykgLT4gYm9vbCIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0X2hhbmRsZV9pbmhlcml0YWJsZSIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldF9pbmhlcml0YWJsZSIsICJkb2MiOiAiZ2V0X2luaGVyaXRhYmxlKGZkOiBpbnQsIC8pIC0+IGJvb2xcblxuR2V0IHRoZSBjbG9zZS1vbi1leGUgZmxhZyBvZiB0aGUgc3BlY2lmaWVkIGZpbGUgZGVzY3JpcHRvci4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldF9pbmhlcml0YWJsZSIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldF90ZXJtaW5hbF9zaXplIiwgImRvYyI6ICJnZXRfdGVybWluYWxfc2l6ZShmZDogaW50PS4uLiwgLykgLT4gdGVybWluYWxfc2l6ZVxuXG5SZXR1cm4gdGhlIHNpemUgb2YgdGhlIHRlcm1pbmFsIHdpbmRvdyBhcyAoY29sdW1ucywgbGluZXMpLlxuXG5UaGUgb3B0aW9uYWwgYXJndW1lbnQgZmQgKGRlZmF1bHQgc3RhbmRhcmQgb3V0cHV0KSBzcGVjaWZpZXNcbndoaWNoIGZpbGUgZGVzY3JpcHRvciBzaG91bGQgYmUgcXVlcmllZC5cblxuSWYgdGhlIGZpbGUgZGVzY3JpcHRvciBpcyBub3QgY29ubmVjdGVkIHRvIGEgdGVybWluYWwsIGFuIE9TRXJyb3JcbmlzIHRocm93bi5cblxuVGhpcyBmdW5jdGlvbiB3aWxsIG9ubHkgYmUgZGVmaW5lZCBpZiBhbiBpbXBsZW1lbnRhdGlvbiBpc1xuYXZhaWxhYmxlIGZvciB0aGlzIHN5c3RlbS5cblxuc2h1dGlsLmdldF90ZXJtaW5hbF9zaXplIGlzIHRoZSBoaWdoLWxldmVsIGZ1bmN0aW9uIHdoaWNoIHNob3VsZFxubm9ybWFsbHkgYmUgdXNlZCwgb3MuZ2V0X3Rlcm1pbmFsX3NpemUgaXMgdGhlIGxvdy1sZXZlbCBpbXBsZW1lbnRhdGlvbi4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldF90ZXJtaW5hbF9zaXplIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0Y3dkIiwgImRvYyI6ICJnZXRjd2QoKSAtPiBzdHJcblxuUmV0dXJuIGEgdW5pY29kZSBzdHJpbmcgcmVwcmVzZW50aW5nIHRoZSBjdXJyZW50IHdvcmtpbmcgZGlyZWN0b3J5LiIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0Y3dkIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0Y3dkYiIsICJkb2MiOiAiZ2V0Y3dkYigpIC0+IGJ5dGVzXG5cblJldHVybiBhIGJ5dGVzIHN0cmluZyByZXByZXNlbnRpbmcgdGhlIGN1cnJlbnQgd29ya2luZyBkaXJlY3RvcnkuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRjd2RiIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0ZWdpZCIsICJkb2MiOiAiZ2V0ZWdpZCgpIC0+IGludFxuXG5SZXR1cm4gdGhlIGN1cnJlbnQgcHJvY2VzcydzIGVmZmVjdGl2ZSBncm91cCBpZC4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldGVnaWQiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRlbnYiLCAiZG9jIjogImdldGVudihrZXk6IHN0cikgLT4gc3RyIHwgTm9uZVxuZ2V0ZW52KGtleTogc3RyLCBkZWZhdWx0OiBfVCkgLT4gc3RyIHwgX1RcblxuR2V0IGFuIGVudmlyb25tZW50IHZhcmlhYmxlLCByZXR1cm4gTm9uZSBpZiBpdCBkb2Vzbid0IGV4aXN0LlxuVGhlIG9wdGlvbmFsIHNlY29uZCBhcmd1bWVudCBjYW4gc3BlY2lmeSBhbiBhbHRlcm5hdGUgZGVmYXVsdC5cbmtleSwgZGVmYXVsdCBhbmQgdGhlIHJlc3VsdCBhcmUgc3RyLiIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0ZW52IiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0ZW52YiIsICJkb2MiOiAiZ2V0ZW52YihrZXk6IGJ5dGVzKSAtPiBieXRlcyB8IE5vbmVcbmdldGVudmIoa2V5OiBieXRlcywgZGVmYXVsdDogX1QpIC0+IGJ5dGVzIHwgX1RcblxuR2V0IGFuIGVudmlyb25tZW50IHZhcmlhYmxlLCByZXR1cm4gTm9uZSBpZiBpdCBkb2Vzbid0IGV4aXN0LlxuVGhlIG9wdGlvbmFsIHNlY29uZCBhcmd1bWVudCBjYW4gc3BlY2lmeSBhbiBhbHRlcm5hdGUgZGVmYXVsdC5cbmtleSwgZGVmYXVsdCBhbmQgdGhlIHJlc3VsdCBhcmUgYnl0ZXMuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRlbnZiIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0ZXVpZCIsICJkb2MiOiAiZ2V0ZXVpZCgpIC0+IGludFxuXG5SZXR1cm4gdGhlIGN1cnJlbnQgcHJvY2VzcydzIGVmZmVjdGl2ZSB1c2VyIGlkLiIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0ZXVpZCIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldGdpZCIsICJkb2MiOiAiZ2V0Z2lkKCkgLT4gaW50XG5cblJldHVybiB0aGUgY3VycmVudCBwcm9jZXNzJ3MgZ3JvdXAgaWQuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRnaWQiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRncm91cGxpc3QiLCAiZG9jIjogImdldGdyb3VwbGlzdCh1c2VyOiBzdHIsIGdyb3VwOiBpbnQsIC8pIC0+IGxpc3RbaW50XVxuXG5SZXR1cm5zIGEgbGlzdCBvZiBncm91cHMgdG8gd2hpY2ggYSB1c2VyIGJlbG9uZ3MuXG5cbnVzZXJcbiAgdXNlcm5hbWUgdG8gbG9va3VwXG5ncm91cFxuICBiYXNlIGdyb3VwIGlkIG9mIHRoZSB1c2VyIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRncm91cGxpc3QiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRncm91cHMiLCAiZG9jIjogImdldGdyb3VwcygpIC0+IGxpc3RbaW50XVxuXG5SZXR1cm4gbGlzdCBvZiBzdXBwbGVtZW50YWwgZ3JvdXAgSURzIGZvciB0aGUgcHJvY2Vzcy4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldGdyb3VwcyIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldGxvYWRhdmciLCAiZG9jIjogImdldGxvYWRhdmcoKSAtPiB0dXBsZVtmbG9hdCwgZmxvYXQsIGZsb2F0XVxuXG5SZXR1cm4gYXZlcmFnZSByZWNlbnQgc3lzdGVtIGxvYWQgaW5mb3JtYXRpb24uXG5cblJldHVybiB0aGUgbnVtYmVyIG9mIHByb2Nlc3NlcyBpbiB0aGUgc3lzdGVtIHJ1biBxdWV1ZSBhdmVyYWdlZCBvdmVyXG50aGUgbGFzdCAxLCA1LCBhbmQgMTUgbWludXRlcyBhcyBhIHR1cGxlIG9mIHRocmVlIGZsb2F0cy5cblJhaXNlcyBPU0Vycm9yIGlmIHRoZSBsb2FkIGF2ZXJhZ2Ugd2FzIHVub2J0YWluYWJsZS4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldGxvYWRhdmciLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRsb2dpbiIsICJkb2MiOiAiZ2V0bG9naW4oKSAtPiBzdHJcblxuUmV0dXJuIHRoZSBhY3R1YWwgbG9naW4gbmFtZS4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldGxvZ2luIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0cGdpZCIsICJkb2MiOiAiZ2V0cGdpZChwaWQ6IGludCkgLT4gaW50XG5cbkNhbGwgdGhlIHN5c3RlbSBjYWxsIGdldHBnaWQoKSwgYW5kIHJldHVybiB0aGUgcmVzdWx0LiIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0cGdpZCIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldHBncnAiLCAiZG9jIjogImdldHBncnAoKSAtPiBpbnRcblxuUmV0dXJuIHRoZSBjdXJyZW50IHByb2Nlc3MgZ3JvdXAgaWQuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRwZ3JwIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0cGlkIiwgImRvYyI6ICJnZXRwaWQoKSAtPiBpbnRcblxuUmV0dXJuIHRoZSBjdXJyZW50IHByb2Nlc3MgaWQuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRwaWQiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRwcGlkIiwgImRvYyI6ICJnZXRwcGlkKCkgLT4gaW50XG5cblJldHVybiB0aGUgcGFyZW50J3MgcHJvY2VzcyBpZC5cblxuSWYgdGhlIHBhcmVudCBwcm9jZXNzIGhhcyBhbHJlYWR5IGV4aXRlZCwgV2luZG93cyBtYWNoaW5lcyB3aWxsIHN0aWxsXG5yZXR1cm4gaXRzIGlkOyBvdGhlcnMgc3lzdGVtcyB3aWxsIHJldHVybiB0aGUgaWQgb2YgdGhlICdpbml0JyBwcm9jZXNzICgxKS4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldHBwaWQiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRwcmlvcml0eSIsICJkb2MiOiAiZ2V0cHJpb3JpdHkod2hpY2g6IGludCwgd2hvOiBpbnQpIC0+IGludFxuXG5SZXR1cm4gcHJvZ3JhbSBzY2hlZHVsaW5nIHByaW9yaXR5LiIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0cHJpb3JpdHkiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXRyYW5kb20iLCAiZG9jIjogImdldHJhbmRvbShzaXplOiBpbnQsIGZsYWdzOiBpbnQ9MCkgLT4gYnl0ZXNcblxuT2J0YWluIGEgc2VyaWVzIG9mIHJhbmRvbSBieXRlcy4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldHJhbmRvbSIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldHJlc2dpZCIsICJkb2MiOiAiZ2V0cmVzZ2lkKCkgLT4gdHVwbGVbaW50LCBpbnQsIGludF1cblxuUmV0dXJuIGEgdHVwbGUgb2YgdGhlIGN1cnJlbnQgcHJvY2VzcydzIHJlYWwsIGVmZmVjdGl2ZSwgYW5kIHNhdmVkIGdyb3VwIGlkcy4iLCAiZGVzY3JpcHRpb24iOiAiZGVmIGdldHJlc2dpZCIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifSwgeyJ3b3JkIjogImdldHJlc3VpZCIsICJkb2MiOiAiZ2V0cmVzdWlkKCkgLT4gdHVwbGVbaW50LCBpbnQsIGludF1cblxuUmV0dXJuIGEgdHVwbGUgb2YgdGhlIGN1cnJlbnQgcHJvY2VzcydzIHJlYWwsIGVmZmVjdGl2ZSwgYW5kIHNhdmVkIHVzZXIgaWRzLiIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ2V0cmVzdWlkIiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9LCB7IndvcmQiOiAiZ2V0c2lkIiwgImRvYyI6ICJnZXRzaWQocGlkOiBpbnQsIC8pIC0+IGludFxuXG5DYWxsIHRoZSBzeXN0ZW0gY2FsbCBnZXRzaWQocGlkKSBhbmQgcmV0dXJuIHRoZSByZXN1bHQuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXRzaWQiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXR1aWQiLCAiZG9jIjogImdldHVpZCgpIC0+IGludFxuXG5SZXR1cm4gdGhlIGN1cnJlbnQgcHJvY2VzcydzIHVzZXIgaWQuIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXR1aWQiLCAic3ltYm9sIjogImZ1bmN0aW9uIn0sIHsid29yZCI6ICJnZXR4YXR0ciIsICJkb2MiOiAiZ2V0eGF0dHIocGF0aDogRmlsZURlc2NyaXB0b3JPclBhdGgsIGF0dHJpYnV0ZTogU3RyT3JCeXRlc1BhdGgsICosIGZvbGxvd19zeW1saW5rczogYm9vbD1UcnVlKSAtPiBieXRlc1xuXG5SZXR1cm4gdGhlIHZhbHVlIG9mIGV4dGVuZGVkIGF0dHJpYnV0ZSBhdHRyaWJ1dGUgb24gcGF0aC5cblxucGF0aCBtYXkgYmUgZWl0aGVyIGEgc3RyaW5nLCBhIHBhdGgtbGlrZSBvYmplY3QsIG9yIGFuIG9wZW4gZmlsZSBkZXNjcmlwdG9yLlxuSWYgZm9sbG93X3N5bWxpbmtzIGlzIEZhbHNlLCBhbmQgdGhlIGxhc3QgZWxlbWVudCBvZiB0aGUgcGF0aCBpcyBhIHN5bWJvbGljXG4gIGxpbmssIGdldHhhdHRyIHdpbGwgZXhhbWluZSB0aGUgc3ltYm9saWMgbGluayBpdHNlbGYgaW5zdGVhZCBvZiB0aGUgZmlsZVxuICB0aGUgbGluayBwb2ludHMgdG8uIiwgImRlc2NyaXB0aW9uIjogImRlZiBnZXR4YXR0ciIsICJzeW1ib2wiOiAiZnVuY3Rpb24ifV0pLAogICAgKCJjb21wbGV0ZSIsICJNQUlOIiwgMjAsIDcsIFt7IndvcmQiOiAiaW5jcmVtZW50IiwgImRvYyI6ICJpbmNyZW1lbnQoKSIsICJkZXNjcmlwdGlvbiI6ICJkZWYgaW5jcmVtZW50IiwgInN5bWJvbCI6ICJmdW5jdGlvbiJ9XSksCiAgICAoImdldF9pbl9mdW5jdGlvbl9jYWxsIiwgIk1BSU4iLCAxOCwgMTYsIHsicGFyYW1zIjogWyJwYXJhbSBuYW1lIl0sICJpbmRleCI6IDAsICJjYWxsX25hbWUiOiAiZ3JlZXQifSksCiAgICAoImdvdG8iLCAiTUFJTiIsIDE4LCAxMSwgW3siY29sdW1uIjogNCwgImxpbmVfbnIiOiAzLCAibW9kdWxlX3BhdGgiOiAiQEBKRURJLURJUkBAL2FwcC5weSIsICJtb2R1bGVfbmFtZSI6ICJfX21haW5fXyIsICJkZXNjcmlwdGlvbiI6ICJkZWYgZ3JlZXQifV0pLAogICAgKCJnZXRfZGVmaW5pdGlvbiIsICJNQUlOIiwgMTgsIDExLCBbeyJkb2MiOiAiZ3JlZXQobmFtZSlcblxuUmV0dXJuIGEgZnJpZW5kbHkgZ3JlZXRpbmcuIiwgImRlc2NyaXB0aW9uIjogImRlZiBncmVldCIsICJsaW5lX25yIjogMywgImNvbHVtbiI6IDQsICJtb2R1bGVfcGF0aCI6ICJAQEpFREktRElSQEAvYXBwLnB5IiwgIm5hbWUiOiAiZ3JlZXQiLCAiZnVsbF9uYW1lIjogIl9fbWFpbl9fLmdyZWV0IiwgInR5cGUiOiAiZnVuY3Rpb24ifV0pLAogICAgKCJnZXRfZGVmaW5pdGlvbiIsICJNQUlOIiwgMTksIDksIFt7ImRvYyI6ICJDb3VudGVyKHN0YXJ0PTApXG5cbkEgc2ltcGxlIGNvdW50ZXIuIiwgImRlc2NyaXB0aW9uIjogImNsYXNzIENvdW50ZXIiLCAibGluZV9uciI6IDcsICJjb2x1bW4iOiA2LCAibW9kdWxlX3BhdGgiOiAiQEBKRURJLURJUkBAL2FwcC5weSIsICJuYW1lIjogIkNvdW50ZXIiLCAiZnVsbF9uYW1lIjogIl9fbWFpbl9fLkNvdW50ZXIiLCAidHlwZSI6ICJjbGFzcyJ9XSksCiAgICAoImdldF9qZWRpX3ZlcnNpb24iLCAiTUFJTiIsIE5vbmUsIE5vbmUsIFtdKSwKICAgICgiZGVmaW5lZF9uYW1lcyIsICJOQU1FUyIsIE5vbmUsIE5vbmUsIFtbeyJkb2MiOiAiZ3JlZXQobmFtZSlcblxuUmV0dXJuIGEgZnJpZW5kbHkgZ3JlZXRpbmcuIiwgImRlc2NyaXB0aW9uIjogImRlZiBncmVldCIsICJsaW5lX25yIjogMSwgImNvbHVtbiI6IDQsICJtb2R1bGVfcGF0aCI6ICJAQEpFREktRElSQEAvbmFtZXMucHkiLCAibmFtZSI6ICJncmVldCIsICJmdWxsX25hbWUiOiAiX19tYWluX18uZ3JlZXQiLCAidHlwZSI6ICJmdW5jdGlvbiIsICJsb2NhbF9uYW1lIjogImdyZWV0In1dLCBbeyJkb2MiOiAiQ291bnRlcihzdGFydD0wKVxuXG5BIHNpbXBsZSBjb3VudGVyLiIsICJkZXNjcmlwdGlvbiI6ICJjbGFzcyBDb3VudGVyIiwgImxpbmVfbnIiOiA1LCAiY29sdW1uIjogNiwgIm1vZHVsZV9wYXRoIjogIkBASkVESS1ESVJAQC9uYW1lcy5weSIsICJuYW1lIjogIkNvdW50ZXIiLCAiZnVsbF9uYW1lIjogIl9fbWFpbl9fLkNvdW50ZXIiLCAidHlwZSI6ICJjbGFzcyIsICJsb2NhbF9uYW1lIjogIkNvdW50ZXIifSwgW3siZG9jIjogIl9faW5pdF9fKHNlbGYsIHN0YXJ0PTApIiwgImRlc2NyaXB0aW9uIjogImRlZiBfX2luaXRfXyIsICJsaW5lX25yIjogNywgImNvbHVtbiI6IDgsICJtb2R1bGVfcGF0aCI6ICJAQEpFREktRElSQEAvbmFtZXMucHkiLCAibmFtZSI6ICJfX2luaXRfXyIsICJmdWxsX25hbWUiOiAiX19tYWluX18uQ291bnRlci5fX2luaXRfXyIsICJ0eXBlIjogImZ1bmN0aW9uIiwgImxvY2FsX25hbWUiOiAiQ291bnRlci5fX2luaXRfXyJ9XSwgW3siZG9jIjogImluY3JlbWVudChzZWxmKSIsICJkZXNjcmlwdGlvbiI6ICJkZWYgaW5jcmVtZW50IiwgImxpbmVfbnIiOiAxMCwgImNvbHVtbiI6IDgsICJtb2R1bGVfcGF0aCI6ICJAQEpFREktRElSQEAvbmFtZXMucHkiLCAibmFtZSI6ICJpbmNyZW1lbnQiLCAiZnVsbF9uYW1lIjogIl9fbWFpbl9fLkNvdW50ZXIuaW5jcmVtZW50IiwgInR5cGUiOiAiZnVuY3Rpb24iLCAibG9jYWxfbmFtZSI6ICJDb3VudGVyLmluY3JlbWVudCJ9XV1dKSwKICAgICgiZ2V0X2plZGlfdmVyc2lvbiIsICJOQU1FUyIsIE5vbmUsIE5vbmUsIFtdKQpdCgpfU09VUkNFUyA9IHsiTUFJTiI6IE1BSU5fU1JDLCAiTkFNRVMiOiBOQU1FU19TUkN9CgoKZGVmIGxvZ19jYWxsKHRleHQpOgogICAgaWYgbm90IExPR19QQVRIOgogICAgICAgIHJldHVybgogICAgd2l0aCBvcGVuKExPR19QQVRILCAiYSIsIGVuY29kaW5nPSJ1dGYtOCIpIGFzIGhhbmRsZToKICAgICAgICBoYW5kbGUud3JpdGUodGV4dCArICJcbiIpCgoKZGVmIF9za2lwX3dzKHMsIGkpOgogICAgd2hpbGUgaSA8IGxlbihzKSBhbmQgc1tpXSBpbiAiIFx0XHJcbiI6CiAgICAgICAgaSArPSAxCiAgICByZXR1cm4gaQoKCmRlZiBfcmVhZF9zdHJpbmcocywgaSk6CiAgICBpICs9IDEKICAgIG91dCA9IFtdCiAgICB3aGlsZSBpIDwgbGVuKHMpOgogICAgICAgIGMgPSBzW2ldCiAgICAgICAgaWYgYyA9PSAiXFwiOgogICAgICAgICAgICBuID0gc1tpICsgMV0KICAgICAgICAgICAgc2ltcGxlID0geyJuIjogIlxuIiwgInQiOiAiXHQiLCAiciI6ICJcciIsICJmIjogIlxmIiwKICAgICAgICAgICAgICAgICAgICAgICJiIjogIlxiIiwgJyInOiAnIicsICJcXCI6ICJcXCIsICInIjogIicifQogICAgICAgICAgICBpZiBuIGluIHNpbXBsZToKICAgICAgICAgICAgICAgIG91dC5hcHBlbmQoc2ltcGxlW25dKQogICAgICAgICAgICAgICAgaSArPSAyCiAgICAgICAgICAgIGVsaWYgbiA9PSAiXG4iOgogICAgICAgICAgICAgICAgaSArPSAyCiAgICAgICAgICAgIGVsaWYgbiBpbiAiMDEyMzQ1NjciOgogICAgICAgICAgICAgICAgaiA9IGkgKyAxCiAgICAgICAgICAgICAgICBkaWdpdHMgPSBbXQogICAgICAgICAgICAgICAgd2hpbGUgaiA8IGxlbihzKSBhbmQgc1tqXSBpbiAiMDEyMzQ1NjciIGFuZCBsZW4oZGlnaXRzKSA8IDM6CiAgICAgICAgICAgICAgICAgICAgZGlnaXRzLmFwcGVuZChzW2pdKQogICAgICAgICAgICAgICAgICAgIGogKz0gMQogICAgICAgICAgICAgICAgb3V0LmFwcGVuZChjaHIoaW50KCIiLmpvaW4oZGlnaXRzKSwgOCkpKQogICAgICAgICAgICAgICAgaSA9IGoKICAgICAgICAgICAgZWxzZToKICAgICAgICAgICAgICAgIG91dC5hcHBlbmQobikKICAgICAgICAgICAgICAgIGkgKz0gMgogICAgICAgIGVsaWYgYyA9PSAnIic6CiAgICAgICAgICAgIHJldHVybiAiIi5qb2luKG91dCksIGkgKyAxCiAgICAgICAgZWxzZToKICAgICAgICAgICAgb3V0LmFwcGVuZChjKQogICAgICAgICAgICBpICs9IDEKICAgIHJhaXNlIFZhbHVlRXJyb3IoInVudGVybWluYXRlZCBzdHJpbmciKQoKCmRlZiBfcmVhZF9hdG9tKHMsIGkpOgogICAgc3RhcnQgPSBpCiAgICB3aGlsZSBpIDwgbGVuKHMpIGFuZCBzW2ldIG5vdCBpbiAiIFx0XHJcbigpXCIiOgogICAgICAgIGkgKz0gMQogICAgcmV0dXJuIHNbc3RhcnQ6aV0sIGkKCgpkZWYgX3BhcnNlKHMsIGkpOgogICAgaSA9IF9za2lwX3dzKHMsIGkpCiAgICBjID0gc1tpXQogICAgaWYgYyA9PSAiKCI6CiAgICAgICAgaXRlbXMgPSBbXQogICAgICAgIGkgKz0gMQogICAgICAgIHdoaWxlIFRydWU6CiAgICAgICAgICAgIGkgPSBfc2tpcF93cyhzLCBpKQogICAgICAgICAgICBpZiBzW2ldID09ICIpIjoKICAgICAgICAgICAgICAgIHJldHVybiBpdGVtcywgaSArIDEKICAgICAgICAgICAgaXRlbSwgaSA9IF9wYXJzZShzLCBpKQogICAgICAgICAgICBpdGVtcy5hcHBlbmQoaXRlbSkKICAgIGVsaWYgYyA9PSAnIic6CiAgICAgICAgcmV0dXJuIF9yZWFkX3N0cmluZyhzLCBpKQogICAgZWxpZiBjID09ICItIiBvciBjLmlzZGlnaXQoKToKICAgICAgICBhdG9tLCBpID0gX3JlYWRfYXRvbShzLCBpKQogICAgICAgIHJldHVybiBpbnQoYXRvbSksIGkKICAgIGVsc2U6CiAgICAgICAgYXRvbSwgaSA9IF9yZWFkX2F0b20ocywgaSkKICAgICAgICByZXR1cm4gYXRvbSwgaQoKCmRlZiBwYXJzZV9zZXhwKHRleHQpOgogICAgdmFsdWUsIGkgPSBfcGFyc2UodGV4dCwgMCkKICAgIHJldHVybiB2YWx1ZQoKCmRlZiBlbGlzcF9zdHIocyk6CiAgICBvdXQgPSBbJyInXQogICAgZm9yIGNoIGluIHM6CiAgICAgICAgaWYgY2ggPT0gIlxcIjoKICAgICAgICAgICAgb3V0LmFwcGVuZCgiXFxcXCIpCiAgICAgICAgZWxpZiBjaCA9PSAnIic6CiAgICAgICAgICAgIG91dC5hcHBlbmQoJ1xcIicpCiAgICAgICAgZWxpZiBjaCA9PSAiXG4iOgogICAgICAgICAgICBvdXQuYXBwZW5kKCJcXG4iKQogICAgICAgIGVsaWYgY2ggPT0gIlx0IjoKICAgICAgICAgICAgb3V0LmFwcGVuZCgiXFx0IikKICAgICAgICBlbGlmIGNoID09ICJcciI6CiAgICAgICAgICAgIG91dC5hcHBlbmQoIlxcciIpCiAgICAgICAgZWxpZiBvcmQoY2gpIDwgMzI6CiAgICAgICAgICAgIG91dC5hcHBlbmQoIlxcJTAzbyIgJSBvcmQoY2gpKQogICAgICAgIGVsc2U6CiAgICAgICAgICAgIG91dC5hcHBlbmQoY2gpCiAgICBvdXQuYXBwZW5kKCciJykKICAgIHJldHVybiAiIi5qb2luKG91dCkKCgpkZWYgZWxpc3AodmFsdWUpOgogICAgaWYgdmFsdWUgaXMgTm9uZToKICAgICAgICByZXR1cm4gIm5pbCIKICAgIGlmIGlzaW5zdGFuY2UodmFsdWUsIGJvb2wpOgogICAgICAgIHJldHVybiAidCIgaWYgdmFsdWUgZWxzZSAibmlsIgogICAgaWYgaXNpbnN0YW5jZSh2YWx1ZSwgc3RyKToKICAgICAgICByZXR1cm4gZWxpc3Bfc3RyKHZhbHVlKQogICAgaWYgaXNpbnN0YW5jZSh2YWx1ZSwgaW50KToKICAgICAgICByZXR1cm4gc3RyKHZhbHVlKQogICAgaWYgaXNpbnN0YW5jZSh2YWx1ZSwgKGxpc3QsIHR1cGxlKSk6CiAgICAgICAgcmV0dXJuICIoIiArICIgIi5qb2luKGVsaXNwKHYpIGZvciB2IGluIHZhbHVlKSArICIpIgogICAgaWYgaXNpbnN0YW5jZSh2YWx1ZSwgZGljdCk6CiAgICAgICAgcGFydHMgPSBbXQogICAgICAgIGZvciBrLCB2IGluIHZhbHVlLml0ZW1zKCk6CiAgICAgICAgICAgIHBhcnRzLmFwcGVuZCgiOiIgKyBrKQogICAgICAgICAgICBwYXJ0cy5hcHBlbmQoZWxpc3AodikpCiAgICAgICAgcmV0dXJuICIoIiArICIgIi5qb2luKHBhcnRzKSArICIpIgogICAgcmFpc2UgVHlwZUVycm9yKCJjYW5ub3QgZW1pdCAlciIgJSAodmFsdWUsKSkKCgpkZWYgc3Vic3RpdHV0ZSh2YWx1ZSwgc291cmNlX3BhdGgpOgogICAgaWYgaXNpbnN0YW5jZSh2YWx1ZSwgc3RyKToKICAgICAgICByZXR1cm4gKHZhbHVlLnJlcGxhY2UoIkBASkVESS1ESVJAQCIsCiAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIG9zLnBhdGguZGlybmFtZShzb3VyY2VfcGF0aCkgb3IgIi8iKQogICAgICAgICAgICAgICAgLnJlcGxhY2UoIkBASkVESS1QQVRIQEAiLCBzb3VyY2VfcGF0aCkpCiAgICBpZiBpc2luc3RhbmNlKHZhbHVlLCBsaXN0KToKICAgICAgICByZXR1cm4gW3N1YnN0aXR1dGUodiwgc291cmNlX3BhdGgpIGZvciB2IGluIHZhbHVlXQogICAgaWYgaXNpbnN0YW5jZSh2YWx1ZSwgZGljdCk6CiAgICAgICAgcmV0dXJuIHtrOiBzdWJzdGl0dXRlKHYsIHNvdXJjZV9wYXRoKSBmb3IgaywgdiBpbiB2YWx1ZS5pdGVtcygpfQogICAgcmV0dXJuIHZhbHVlCgoKZGVmIGRpc3BhdGNoKG1ldGhvZCwgYXJncywgcmF3X3RleHQpOgogICAgbG9nX2NhbGwocmF3X3RleHQpCiAgICBzb3VyY2UgPSBhcmdzWzBdCiAgICBzb3VyY2Vfa2V5ID0gTm9uZQogICAgaWYgc291cmNlID09IE1BSU5fU1JDOgogICAgICAgIHNvdXJjZV9rZXkgPSAiTUFJTiIKICAgIGVsaWYgc291cmNlID09IE5BTUVTX1NSQzoKICAgICAgICBzb3VyY2Vfa2V5ID0gIk5BTUVTIgogICAgZWxzZToKICAgICAgICByZXR1cm4gWyJuby1yZWNvcmRlZC1zb3VyY2UiLCBtZXRob2QsIHNvdXJjZVs6NDBdXQogICAgbGluZSA9IGFyZ3NbMV0gaWYgbGVuKGFyZ3MpID4gMSBhbmQgaXNpbnN0YW5jZShhcmdzWzFdLCBpbnQpIGVsc2UgTm9uZQogICAgY29sdW1uID0gYXJnc1syXSBpZiBsZW4oYXJncykgPiAyIGFuZCBpc2luc3RhbmNlKGFyZ3NbMl0sIGludCkgZWxzZSBOb25lCiAgICBmb3IgbSwgc2ssIGwsIGMsIHZhbHVlIGluIFJFU1BPTlNFUzoKICAgICAgICBpZiBtID09IG1ldGhvZCBhbmQgc2sgPT0gc291cmNlX2tleSBhbmQgbCA9PSBsaW5lIGFuZCBjID09IGNvbHVtbjoKICAgICAgICAgICAgcmV0dXJuIHN1YnN0aXR1dGUodmFsdWUsIGFyZ3NbM10gaWYgbGVuKGFyZ3MpID4gMyBlbHNlICIvIikKICAgIHJldHVybiBbIm5vLXJlY29yZGVkLXJlc3BvbnNlIiwgbWV0aG9kLCBzb3VyY2Vfa2V5LCBsaW5lLCBjb2x1bW5dCgoKZGVmIGZyYW1lKHNleHBfdGV4dCk6CiAgICBtc2cgPSAoc2V4cF90ZXh0ICsgIlxuIikuZW5jb2RlKCJ1dGYtOCIpCiAgICByZXR1cm4gKCIlMDZ4IiAlIGxlbihtc2cpKS5lbmNvZGUoImFzY2lpIikgKyBtc2cKCgpkZWYgc2VydmUoKToKICAgIHNvY2sgPSBzb2NrZXQuc29ja2V0KHNvY2tldC5BRl9JTkVUNiwgc29ja2V0LlNPQ0tfU1RSRUFNKQogICAgc29jay5zZXRzb2Nrb3B0KHNvY2tldC5JUFBST1RPX0lQVjYsIHNvY2tldC5JUFY2X1Y2T05MWSwgMCkKICAgIHNvY2suYmluZCgoIjo6IiwgMCkpCiAgICBzb2NrLmxpc3Rlbig0KQogICAgcG9ydCA9IHNvY2suZ2V0c29ja25hbWUoKVsxXQogICAgc3lzLnN0ZG91dC53cml0ZSgiJWRcbiIgJSBwb3J0KQogICAgc3lzLnN0ZG91dC5mbHVzaCgpCiAgICBjb25uLCBfID0gc29jay5hY2NlcHQoKQogICAgdHJ5OgogICAgICAgIGJ1ZiA9IGIiIgogICAgICAgIHdoaWxlIFRydWU6CiAgICAgICAgICAgIGNodW5rID0gY29ubi5yZWN2KDQwOTYpCiAgICAgICAgICAgIGlmIG5vdCBjaHVuazoKICAgICAgICAgICAgICAgIGJyZWFrCiAgICAgICAgICAgIGJ1ZiArPSBjaHVuawogICAgICAgICAgICB3aGlsZSBsZW4oYnVmKSA+PSA2OgogICAgICAgICAgICAgICAgdHJ5OgogICAgICAgICAgICAgICAgICAgIGxlbmd0aCA9IGludChidWZbOjZdLCAxNikKICAgICAgICAgICAgICAgIGV4Y2VwdCBWYWx1ZUVycm9yOgogICAgICAgICAgICAgICAgICAgIGJyZWFrCiAgICAgICAgICAgICAgICBpZiBsZW4oYnVmKSA8IDYgKyBsZW5ndGg6CiAgICAgICAgICAgICAgICAgICAgYnJlYWsKICAgICAgICAgICAgICAgIHBheWxvYWQgPSBidWZbNjo2ICsgbGVuZ3RoXQogICAgICAgICAgICAgICAgYnVmID0gYnVmWzYgKyBsZW5ndGg6XQogICAgICAgICAgICAgICAgbWVzc2FnZSA9IHBhcnNlX3NleHAocGF5bG9hZC5kZWNvZGUoInV0Zi04IikpCiAgICAgICAgICAgICAgICBpZiBub3QgbWVzc2FnZSBvciBtZXNzYWdlWzBdICE9ICJjYWxsIjoKICAgICAgICAgICAgICAgICAgICBjb250aW51ZQogICAgICAgICAgICAgICAgXywgdWlkLCBtZXRob2QgPSBtZXNzYWdlWzBdLCBtZXNzYWdlWzFdLCBtZXNzYWdlWzJdCiAgICAgICAgICAgICAgICBhcmdzID0gbWVzc2FnZVszOl0KICAgICAgICAgICAgICAgIGlmIGxlbihhcmdzKSA9PSAxIGFuZCBpc2luc3RhbmNlKGFyZ3NbMF0sIGxpc3QpOgogICAgICAgICAgICAgICAgICAgIGFyZ3MgPSBhcmdzWzBdCiAgICAgICAgICAgICAgICByZXN1bHQgPSBkaXNwYXRjaChtZXRob2QsIGFyZ3MsIHBheWxvYWQuZGVjb2RlKCJ1dGYtOCIpKQogICAgICAgICAgICAgICAgcmVwbHkgPSAiKHJldHVybiAlZCAlcykiICUgKHVpZCwgZWxpc3AocmVzdWx0KSkKICAgICAgICAgICAgICAgIGNvbm4uc2VuZGFsbChmcmFtZShyZXBseSkpCiAgICBmaW5hbGx5OgogICAgICAgIGNvbm4uY2xvc2UoKQogICAgICAgIHNvY2suY2xvc2UoKQoKCmlmIF9fbmFtZV9fID09ICJfX21haW5fXyI6CiAgICBzZXJ2ZSgpCg=="
  "Base64 of the EPC stand-in python program.  It replays responses
recorded from jedi 0.20.0 with exactly the fixture sources the suite
authors (app.py for RPC workflows, names.py for defined_names).")

(defun jedi--test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun jedi--test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path)
      (buffer-string))))

(defun jedi--test-normalize (text)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name jedi--test-root))
   "@@ROOT@@" text t t))

(defun jedi--test-editor-path ()
  "Return the absolute path of the editor under test.
A child that cannot be exec'd reports the failure itself, and GNU's
`emacs_perror' prefixes that line with `initial_argv0'
\(src/sysdep.c:2867-2887, reached from `exec_failed', src/callproc.c:1206-1216).
So the diagnostic differs between the two editors by construction, and a
pin that keeps the raw path records where THIS machine built GNU Emacs
rather than what either editor did."
  (expand-file-name invocation-name invocation-directory))

(defun jedi--test-normalize-editor (text)
  "Replace the running editor's own program path in TEXT."
  (replace-regexp-in-string
   (regexp-quote (jedi--test-editor-path)) "@@EMACS@@" text t t))

(defun jedi--test-install-standin ()
  "Install the EPC stand-in as `python' ahead of PATH and point
`jedi:server-command' at it."
  (let ((program (expand-file-name "python" jedi--test-bin)))
    (jedi--test-write
     program
     (decode-coding-string (base64-decode-string jedi--test-standin-b64)
                           'utf-8-unix))
    (set-file-modes program #o755)
    (setq jedi:server-command
          (list program
                (expand-file-name "jediepcserver.py"
                                  (file-name-directory
                                   (locate-library "jedi-core.el")))))
    (setenv "JEDI_STANDIN_LOG" jedi--test-log)
    (setenv "PATH" (concat jedi--test-bin
                           path-separator (getenv "PATH")))
    (setq exec-path (cons jedi--test-bin exec-path))
    program))

(defun jedi--test-reset-log ()
  (when (file-exists-p jedi--test-log)
    (delete-file jedi--test-log)))

(defun jedi--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "jedi.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/jedi.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed jedi location: %S" located))
    (dolist (entry jedi--test-manifest)
      (let* ((located (locate-library (car entry)))
             (file (and located (file-truename located))))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed jedi source: %S"
                   (car entry))))))
    (list :upstream-tree jedi--test-upstream-tree
          :feature (featurep 'jedi)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'jedi package-alist)))))))

(defun jedi--test-open (relpath content)
  "Write CONTENT into FIXTURES/RELPATH, visiting it in a fresh buffer."
  (let* ((path (expand-file-name relpath jedi--test-fixtures))
         (name (file-name-nondirectory relpath)))
    (when (get-buffer name)
      (with-current-buffer (get-buffer name)
        (set-buffer-modified-p nil)
        (kill-buffer)))
    (jedi--test-reset-log)
    (jedi--test-write path content)
    (find-file path)))

(defun jedi--test-app-c ()
  "The exact source the recorded RPC responses were generated from."
  "import os\n\ndef greet(name):\n    \"\"\"Return a friendly greeting.\"\"\"\n    return \"hello \" + name\n\nclass Counter(object):\n    \"\"\"A simple counter.\"\"\"\n    def __init__(self, start=0):\n        self.value = start\n\n    def increment(self):\n        self.value += 1\n        return self.value\n\n\ndef main():\n    print(greet(\"world\"))\n    c = Counter()\n    c.increment()\n    print(os.getcwd())\n")

(defun jedi--test-names-py ()
  "The exact source the recorded defined_names response was generated
from (no imports, so the module tree stays small)."
  "def greet(name):\n    \"\"\"Return a friendly greeting.\"\"\"\n    return \"hello \" + name\n\nclass Counter(object):\n    \"\"\"A simple counter.\"\"\"\n    def __init__(self, start=0):\n        self.value = start\n\n    def increment(self):\n        self.value += 1\n        return self.value\n")

(defun jedi--test-at (line col)
  "Move point to 0-based LINE and COLUMN of the visited fixture."
  (goto-char (point-min))
  (forward-line line)
  (forward-char col))

(defun jedi--test-pump ()
  "Pump process output and timers so deferred chains resolve in batch."
  (dotimes (_ 10)
    (sit-for 0.05)))

(defvar jedi--test-messages nil)

(defmacro jedi--test-with-message-capture (&rest body)
  "Run BODY with `message' captured."
  `(let ((jedi--test-messages nil))
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (push (apply #'format-message fmt args)
                        jedi--test-messages))))
       ,@body)))

(defun jedi--test-result (&rest plist)
  (append
   plist
   (list :messages (mapcar #'jedi--test-normalize-editor
                           (nreverse jedi--test-messages))
         :calls (jedi--test-normalize
                 (if (file-exists-p jedi--test-log)
                     (jedi--test-read jedi--test-log)
                   "")))))

(defun jedi--test-reset ()
  "Restore editor state mutated by the workflows."
  (jedi:stop-all-servers)
  (dolist (buf (list jedi:doc-buffer-name "app.py" "names.py"))
    (when (get-buffer buf)
      (with-current-buffer (get-buffer buf)
        (set-buffer-modified-p nil)
        (kill-buffer))))
  (setq jedi:server-command
        (list (expand-file-name "python" jedi--test-bin)
              (expand-file-name "jediepcserver.py"
                                (file-name-directory
                                 (locate-library "jedi-core.el")))))
  (setq jedi:use-shortcuts nil)
  (jedi--test-reset-log))

(jedi--test-install-standin)
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(JEDI_MELPA_PIN, "jedi.el")
        .expect("prepare pinned jedi source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn jedi_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "jedi_package_batch", "jedi_parity", &cases);
}
