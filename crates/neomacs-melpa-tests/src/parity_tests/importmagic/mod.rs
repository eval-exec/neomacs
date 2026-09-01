//! Practical parity for importmagic.  The package is a thin EPC client
//! over the importmagic Python library; its value is the fix-symbol /
//! fix-imports round trip: the buffer goes out, an import block comes
//! back and is inserted at the server-computed line range.
//!
//! The Python backend (and the real importmagic index) are environmental:
//! the suite installs an EPC-speaking stand-in executable ahead of PATH
//! that replays responses recorded from importmagic 0.2.0 with an index
//! built from exactly the fixture project the prelude authors
//! (widgets.py, gadgets/spinner.py).  The package runs its real public
//! path end to end -- mode enable, EPC server startup, RPC argument
//! vectors, buffer transformation -- and the stand-in logs every call so
//! the suite can assert the exact contract the package sent.

use std::time::Duration;

use crate::{CachedMelpaOracle, IMPORTMAGIC_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar importmagic--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar importmagic--test-project
  (file-name-as-directory (expand-file-name "importmagic-proj" importmagic--test-root)))
(defvar importmagic--test-calls-log
  (expand-file-name "importmagic-calls.log" importmagic--test-root))
(defvar importmagic--test-bin
  (file-name-as-directory (expand-file-name "bin" importmagic--test-root)))

;; Provenance: pinned upstream e32ee9f6a5eef937b76eba82fdae8bae85d18088.
(defconst importmagic--test-upstream-tree
  "ac43d5984b59b8bbff3d7707f1eb74730d1a49ee"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst importmagic--test-manifest
  '(("importmagic.el"
     . "fcec0958e7a252b5db84f103fdfbe688989b2a75b11c84d41caa3e36f2264f91")
    ("importmagicserver.py"
     . "09e42a10a8bade01c1c7b2b8ff5236e1961f01e325791abe54e1c1ddcf140c37"))
  "Per-file sha256 of the package-built sources the suite verifies.
package-build replaces the upstream `Version:' header with
`Package-Version:' and `Package-Revision:', so importmagic.el hashes
the built form; importmagicserver.py is copied verbatim.")

(defun importmagic--test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun importmagic--test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path)
      (buffer-string))))

(defun importmagic--test-normalize (text)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name importmagic--test-root))
   "@@ROOT@@" text t t))

(defun importmagic--test-reset-calls-log ()
  (setenv "IM_STANDIN_LOG" importmagic--test-calls-log)
  (when (file-exists-p importmagic--test-calls-log)
    (delete-file importmagic--test-calls-log)))

(defconst importmagic--test-standin-b64
  "IyEvdXNyL2Jpbi9lbnYgcHl0aG9uMwoiIiJFUEMgc3RhbmQtaW4gZm9yIGltcG9ydG1hZ2ljLmVsLgoKUmVwbGF5cyByZXNwb25zZXMgcmVjb3JkZWQgZnJvbSBpbXBvcnRtYWdpYyAwLjIuMCB3aXRoIGEgU3ltYm9sSW5kZXgKYnVpbHQgZnJvbSBleGFjdGx5IHRoZSBmaXh0dXJlIHByb2plY3QgdGhlIHN1aXRlIGF1dGhvcnMgKHdpZGdldHMucHksCmdhZGdldHMvc3Bpbm5lci5weSkuICBTcGVha3MgdGhlIEVQQyB3aXJlIHByb3RvY29sOiBwcmludHMgdGhlIFRDUApwb3J0IG9uIHN0ZG91dCwgdGhlbiBzZXJ2ZXMgbGVuZ3RoLXByZWZpeGVkIGVsaXNwIHNleHBzCigiJTA2eCIgYnl0ZSBsZW5ndGggKyBwcmluMSArIG5ld2xpbmUsIFVURi04KS4KCkVhY2ggcmVxdWVzdCBpcyBsb2dnZWQgdG8gSU1fU1RBTkRJTl9MT0cgKGVsaXNwIHJlbmRlcmluZyBvZiB0aGUKY2FsbCkgc28gdGhlIHN1aXRlIGNhbiBhc3NlcnQgdGhlIGV4YWN0IGFyZ3VtZW50IHZlY3RvcnMgdGhlIHBhY2thZ2UKc2VudC4KIiIiCmltcG9ydCBvcwppbXBvcnQgc29ja2V0CmltcG9ydCBzeXMKCkxPR19QQVRIID0gb3MuZW52aXJvbi5nZXQoIklNX1NUQU5ESU5fTE9HIiwgIiIpClNPQ0tFVF9QQVRIID0gb3MuZW52aXJvbi5nZXQoIklNX1NUQU5ESU5fU09DS0VUIiwgIiIpICAjIHVuaXggc29ja2V0PyBubzsgdGNwIG9ubHkKCiMgLS0gcmVjb3JkZWQgcmVzcG9uc2VzIC0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0KClVOUkVTT0xWRUQgPSB7CiAgICAicHJpbnQoV2lkZ2V0KCkpXG4iOiBbIldpZGdldCJdLAogICAgInByaW50KFNwaW5uZXIoKSlcbiI6IFsiU3Bpbm5lciJdLAogICAgIm9zLnBhdGguam9pbigncGF0aDEnLCAncGF0aDInKVxuIjogWyJvcy5wYXRoLmpvaW4iXSwKICAgICJXaWRnZXQoKS5yZW5kZXIoU3Bpbm5lcigpKVxuIjogWyJTcGlubmVyIiwgIldpZGdldCJdLAogICAgImZyb2JuaWNhdGUoKVxuIjogWyJmcm9ibmljYXRlIl0sCiAgICAicHJpbnQoV2lkZ2V0KCkpXG5wcmludChTcGlubmVyKCkpXG4iOiBbIlNwaW5uZXIiLCAiV2lkZ2V0Il0sCiAgICAiZnJvYm5pY2F0ZSgpXG5wcmludChXaWRnZXQoKSlcbiI6IFsiZnJvYm5pY2F0ZSIsICJXaWRnZXQiXSwKfQoKQ0FORElEQVRFUyA9IHsKICAgICJXaWRnZXQiOiBbImZyb20gd2lkZ2V0cyBpbXBvcnQgV2lkZ2V0Il0sCiAgICAiU3Bpbm5lciI6IFsiZnJvbSBnYWRnZXRzLnNwaW5uZXIgaW1wb3J0IFNwaW5uZXIiXSwKICAgICJvcyI6IFsiaW1wb3J0IG9zIl0sCiAgICAib3MucGF0aC5qb2luIjogWyJpbXBvcnQgb3MucGF0aCJdLAogICAgIndpZGdldHMiOiBbImltcG9ydCB3aWRnZXRzIl0sCiAgICAic3Bpbm5lciI6IFsiZnJvbSBnYWRnZXRzIGltcG9ydCBzcGlubmVyIl0sCiAgICAiZnJvYm5pY2F0ZSI6IFtdLAp9CgojIChzb3VyY2UsIHN0YXRlbWVudCwgKG11bHRpbGluZS1zdHlsZSwgbWF4X2NvbHVtbnMpKSAtPiBbc3RhcnQsIGVuZCwgYmxvY2tdCkRFRkFVTFRfU1RZTEUgPSAoInBhcmVudGhlc2VzIiwgNzkpClNUQVRFTUVOVFMgPSB7CiAgICAoInByaW50KFdpZGdldCgpKVxuIiwgImZyb20gd2lkZ2V0cyBpbXBvcnQgV2lkZ2V0IiwgREVGQVVMVF9TVFlMRSk6CiAgICAgICAgWzAsIDAsICJmcm9tIHdpZGdldHMgaW1wb3J0IFdpZGdldFxuXG5cbiJdLAogICAgKCJwcmludChTcGlubmVyKCkpXG4iLCAiZnJvbSBnYWRnZXRzLnNwaW5uZXIgaW1wb3J0IFNwaW5uZXIiLCBERUZBVUxUX1NUWUxFKToKICAgICAgICBbMCwgMCwgImZyb20gZ2FkZ2V0cy5zcGlubmVyIGltcG9ydCBTcGlubmVyXG5cblxuIl0sCiAgICAoInByaW50KFdpZGdldCgpKVxuIiwgImltcG9ydCB3aWRnZXRzIiwgREVGQVVMVF9TVFlMRSk6CiAgICAgICAgWzAsIDAsICJpbXBvcnQgd2lkZ2V0c1xuXG5cbiJdLAogICAgKCJvcy5wYXRoLmpvaW4oJ3BhdGgxJywgJ3BhdGgyJylcbiIsICJpbXBvcnQgb3MiLCBERUZBVUxUX1NUWUxFKToKICAgICAgICBbMCwgMCwgImltcG9ydCBvc1xuXG5cbiJdLAogICAgKCJwcmludChXaWRnZXQoKSlcbnByaW50KFNwaW5uZXIoKSlcbiIsICJmcm9tIGdhZGdldHMuc3Bpbm5lciBpbXBvcnQgU3Bpbm5lciIsCiAgICAgREVGQVVMVF9TVFlMRSk6CiAgICAgICAgWzAsIDAsICJmcm9tIGdhZGdldHMuc3Bpbm5lciBpbXBvcnQgU3Bpbm5lclxuXG5cbiJdLAogICAgKCJmcm9tIGdhZGdldHMuc3Bpbm5lciBpbXBvcnQgU3Bpbm5lclxuXG5cbnByaW50KFdpZGdldCgpKVxucHJpbnQoU3Bpbm5lcigpKVxuIiwKICAgICAiZnJvbSB3aWRnZXRzIGltcG9ydCBXaWRnZXQiLCBERUZBVUxUX1NUWUxFKToKICAgICAgICBbMCwgMywgImZyb20gZ2FkZ2V0cy5zcGlubmVyIGltcG9ydCBTcGlubmVyXG5mcm9tIHdpZGdldHMgaW1wb3J0IFdpZGdldFxuXG5cbiJdLAogICAgKCJmcm9ibmljYXRlKClcbnByaW50KFdpZGdldCgpKVxuIiwgImZyb20gd2lkZ2V0cyBpbXBvcnQgV2lkZ2V0IiwgREVGQVVMVF9TVFlMRSk6CiAgICAgICAgWzAsIDAsICJmcm9tIHdpZGdldHMgaW1wb3J0IFdpZGdldFxuXG5cbiJdLAogICAgKCJmcm9tIHNvbWUubG9uZy5tb2R1bGUucGF0aCBpbXBvcnQgdmVyeV9sb25nX3N5bWJvbF9uYW1lX2hlcmVcblxuXG4iCiAgICAgInByaW50KFdpZGdldCgpKVxuIiwgImZyb20gd2lkZ2V0cyBpbXBvcnQgV2lkZ2V0IiwgREVGQVVMVF9TVFlMRSk6CiAgICAgICAgWzAsIDMsICJmcm9tIHNvbWUubG9uZy5tb2R1bGUucGF0aCBpbXBvcnQgdmVyeV9sb25nX3N5bWJvbF9uYW1lX2hlcmVcbiIKICAgICAgICAgICAgICAgImZyb20gd2lkZ2V0cyBpbXBvcnQgV2lkZ2V0XG5cblxuIl0sCiAgICAoImZyb20gc29tZS5sb25nLm1vZHVsZS5wYXRoIGltcG9ydCB2ZXJ5X2xvbmdfc3ltYm9sX25hbWVfaGVyZVxuXG5cbiIKICAgICAicHJpbnQoV2lkZ2V0KCkpXG4iLCAiZnJvbSB3aWRnZXRzIGltcG9ydCBXaWRnZXQiLCAoInBhcmVudGhlc2VzIiwgNDApKToKICAgICAgICBbMCwgMywgImZyb20gc29tZS5sb25nLm1vZHVsZS5wYXRoIGltcG9ydCAoXG4gICAgdmVyeV9sb25nX3N5bWJvbF9uYW1lX2hlcmUpXG4iCiAgICAgICAgICAgICAgICJmcm9tIHdpZGdldHMgaW1wb3J0IFdpZGdldFxuXG5cbiJdLAogICAgKCJmcm9tIHNvbWUubG9uZy5tb2R1bGUucGF0aCBpbXBvcnQgdmVyeV9sb25nX3N5bWJvbF9uYW1lX2hlcmVcblxuXG4iCiAgICAgInByaW50KFdpZGdldCgpKVxuIiwgImZyb20gd2lkZ2V0cyBpbXBvcnQgV2lkZ2V0IiwgKCJiYWNrc2xhc2giLCA3OSkpOgogICAgICAgIFswLCAzLCAiZnJvbSBzb21lLmxvbmcubW9kdWxlLnBhdGggaW1wb3J0IHZlcnlfbG9uZ19zeW1ib2xfbmFtZV9oZXJlXG4iCiAgICAgICAgICAgICAgICJmcm9tIHdpZGdldHMgaW1wb3J0IFdpZGdldFxuXG5cbiJdLAp9CgoKIyAtLSBlbGlzcCBzZXhwIHJlYWRlciAtLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tCgpkZWYgX3NraXBfd3MocywgaSk6CiAgICB3aGlsZSBpIDwgbGVuKHMpIGFuZCBzW2ldIGluICIgXHRcclxuIjoKICAgICAgICBpICs9IDEKICAgIHJldHVybiBpCgoKZGVmIF9yZWFkX3N0cmluZyhzLCBpKToKICAgIGkgKz0gMQogICAgb3V0ID0gW10KICAgIHdoaWxlIGkgPCBsZW4ocyk6CiAgICAgICAgYyA9IHNbaV0KICAgICAgICBpZiBjID09ICJcXCI6CiAgICAgICAgICAgIG4gPSBzW2kgKyAxXQogICAgICAgICAgICBzaW1wbGUgPSB7Im4iOiAiXG4iLCAidCI6ICJcdCIsICJyIjogIlxyIiwgImYiOiAiXGYiLAogICAgICAgICAgICAgICAgICAgICAgImIiOiAiXGIiLCAnIic6ICciJywgIlxcIjogIlxcIiwgIiciOiAiJyJ9CiAgICAgICAgICAgIGlmIG4gaW4gc2ltcGxlOgogICAgICAgICAgICAgICAgb3V0LmFwcGVuZChzaW1wbGVbbl0pCiAgICAgICAgICAgICAgICBpICs9IDIKICAgICAgICAgICAgZWxpZiBuID09ICJcbiI6CiAgICAgICAgICAgICAgICBpICs9IDIKICAgICAgICAgICAgZWxpZiBuIGluICIwMTIzNDU2NyI6CiAgICAgICAgICAgICAgICBqID0gaSArIDEKICAgICAgICAgICAgICAgIGRpZ2l0cyA9IFtdCiAgICAgICAgICAgICAgICB3aGlsZSBqIDwgbGVuKHMpIGFuZCBzW2pdIGluICIwMTIzNDU2NyIgYW5kIGxlbihkaWdpdHMpIDwgMzoKICAgICAgICAgICAgICAgICAgICBkaWdpdHMuYXBwZW5kKHNbal0pCiAgICAgICAgICAgICAgICAgICAgaiArPSAxCiAgICAgICAgICAgICAgICBvdXQuYXBwZW5kKGNocihpbnQoIiIuam9pbihkaWdpdHMpLCA4KSkpCiAgICAgICAgICAgICAgICBpID0gagogICAgICAgICAgICBlbHNlOgogICAgICAgICAgICAgICAgb3V0LmFwcGVuZChuKQogICAgICAgICAgICAgICAgaSArPSAyCiAgICAgICAgZWxpZiBjID09ICciJzoKICAgICAgICAgICAgcmV0dXJuICIiLmpvaW4ob3V0KSwgaSArIDEKICAgICAgICBlbHNlOgogICAgICAgICAgICBvdXQuYXBwZW5kKGMpCiAgICAgICAgICAgIGkgKz0gMQogICAgcmFpc2UgVmFsdWVFcnJvcigidW50ZXJtaW5hdGVkIHN0cmluZyIpCgoKZGVmIF9yZWFkX2F0b20ocywgaSk6CiAgICBzdGFydCA9IGkKICAgIHdoaWxlIGkgPCBsZW4ocykgYW5kIHNbaV0gbm90IGluICIgXHRcclxuKClcIiI6CiAgICAgICAgaSArPSAxCiAgICByZXR1cm4gc1tzdGFydDppXSwgaQoKCmRlZiBfcGFyc2UocywgaSk6CiAgICBpID0gX3NraXBfd3MocywgaSkKICAgIGMgPSBzW2ldCiAgICBpZiBjID09ICIoIjoKICAgICAgICBpdGVtcyA9IFtdCiAgICAgICAgaSArPSAxCiAgICAgICAgd2hpbGUgVHJ1ZToKICAgICAgICAgICAgaSA9IF9za2lwX3dzKHMsIGkpCiAgICAgICAgICAgIGlmIHNbaV0gPT0gIikiOgogICAgICAgICAgICAgICAgcmV0dXJuIGl0ZW1zLCBpICsgMQogICAgICAgICAgICBpdGVtLCBpID0gX3BhcnNlKHMsIGkpCiAgICAgICAgICAgIGl0ZW1zLmFwcGVuZChpdGVtKQogICAgZWxpZiBjID09ICciJzoKICAgICAgICByZXR1cm4gX3JlYWRfc3RyaW5nKHMsIGkpCiAgICBlbGlmIGMgPT0gIi0iIG9yIGMuaXNkaWdpdCgpOgogICAgICAgIGF0b20sIGkgPSBfcmVhZF9hdG9tKHMsIGkpCiAgICAgICAgcmV0dXJuIGludChhdG9tKSwgaQogICAgZWxzZToKICAgICAgICBhdG9tLCBpID0gX3JlYWRfYXRvbShzLCBpKQogICAgICAgIHJldHVybiBhdG9tLCBpCgoKZGVmIHBhcnNlX3NleHAodGV4dCk6CiAgICB2YWx1ZSwgaSA9IF9wYXJzZSh0ZXh0LCAwKQogICAgcmV0dXJuIHZhbHVlCgoKIyAtLSBlbGlzcCBlbWl0dGVyIC0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLQoKZGVmIGVsaXNwX3N0cihzKToKICAgIG91dCA9IFsnIiddCiAgICBmb3IgY2ggaW4gczoKICAgICAgICBpZiBjaCA9PSAiXFwiOgogICAgICAgICAgICBvdXQuYXBwZW5kKCJcXFxcIikKICAgICAgICBlbGlmIGNoID09ICciJzoKICAgICAgICAgICAgb3V0LmFwcGVuZCgnXFwiJykKICAgICAgICBlbGlmIGNoID09ICJcbiI6CiAgICAgICAgICAgIG91dC5hcHBlbmQoIlxcbiIpCiAgICAgICAgZWxpZiBjaCA9PSAiXHQiOgogICAgICAgICAgICBvdXQuYXBwZW5kKCJcXHQiKQogICAgICAgIGVsaWYgY2ggPT0gIlxyIjoKICAgICAgICAgICAgb3V0LmFwcGVuZCgiXFxyIikKICAgICAgICBlbGlmIG9yZChjaCkgPCAzMjoKICAgICAgICAgICAgb3V0LmFwcGVuZCgiXFwlMDNvIiAlIG9yZChjaCkpCiAgICAgICAgZWxzZToKICAgICAgICAgICAgb3V0LmFwcGVuZChjaCkKICAgIG91dC5hcHBlbmQoJyInKQogICAgcmV0dXJuICIiLmpvaW4ob3V0KQoKCmRlZiBlbGlzcCh2YWx1ZSk6CiAgICBpZiBpc2luc3RhbmNlKHZhbHVlLCBzdHIpOgogICAgICAgIHJldHVybiBlbGlzcF9zdHIodmFsdWUpCiAgICBpZiBpc2luc3RhbmNlKHZhbHVlLCBib29sKToKICAgICAgICByZXR1cm4gInQiIGlmIHZhbHVlIGVsc2UgIm5pbCIKICAgIGlmIGlzaW5zdGFuY2UodmFsdWUsIGludCk6CiAgICAgICAgcmV0dXJuIHN0cih2YWx1ZSkKICAgIGlmIGlzaW5zdGFuY2UodmFsdWUsIChsaXN0LCB0dXBsZSkpOgogICAgICAgIHJldHVybiAiKCIgKyAiICIuam9pbihlbGlzcCh2KSBmb3IgdiBpbiB2YWx1ZSkgKyAiKSIKICAgIHJhaXNlIFR5cGVFcnJvcigiY2Fubm90IGVtaXQgJXIiICUgKHZhbHVlLCkpCgoKZGVmIGxvZ19jYWxsKHRleHQpOgogICAgaWYgbm90IExPR19QQVRIOgogICAgICAgIHJldHVybgogICAgd2l0aCBvcGVuKExPR19QQVRILCAiYSIsIGVuY29kaW5nPSJ1dGYtOCIpIGFzIGhhbmRsZToKICAgICAgICBoYW5kbGUud3JpdGUodGV4dCArICJcbiIpCgoKCmRlZiBsb29rdXBfc3RhdGVtZW50KHNvdXJjZSwgc3RhdGVtZW50LCBzdHlsZV9rZXkpOgogICAga2V5ID0gKHNvdXJjZSwgc3RhdGVtZW50LCBzdHlsZV9rZXkpCiAgICBpZiBrZXkgaW4gU1RBVEVNRU5UUzoKICAgICAgICByZXR1cm4gU1RBVEVNRU5UU1trZXldCiAgICBkZWZhdWx0X2tleSA9IChzb3VyY2UsIHN0YXRlbWVudCwgREVGQVVMVF9TVFlMRSkKICAgIGlmIGRlZmF1bHRfa2V5IGluIFNUQVRFTUVOVFM6CiAgICAgICAgcmV0dXJuIFNUQVRFTUVOVFNbZGVmYXVsdF9rZXldCiAgICByZXR1cm4gTm9uZQoKCmRlZiBzdHlsZV9rZXkoc3R5bGUpOgogICAgIiIiTm9ybWFsaXplIHRoZSBzdHlsZSBhcmd1bWVudCBgKChrZXlzKSAodmFsdWVzKSlgIHRvCiAgICAobXVsdGlsaW5lLXZhbHVlLCBtYXhfY29sdW1ucy12YWx1ZSkuIiIiCiAgICBpZiBub3Qgc3R5bGU6CiAgICAgICAgcmV0dXJuIERFRkFVTFRfU1RZTEUKICAgIGtleXMgPSBzdHlsZVswXSBpZiBsZW4oc3R5bGUpID4gMCBlbHNlIFtdCiAgICB2YWx1ZXMgPSBzdHlsZVsxXSBpZiBsZW4oc3R5bGUpID4gMSBlbHNlIFtdCiAgICBvcHRpb25zID0gZGljdCh6aXAoa2V5cywgdmFsdWVzKSkKICAgIG11bHRpbGluZSA9IG9wdGlvbnMuZ2V0KCJtdWx0aWxpbmUiLCBERUZBVUxUX1NUWUxFWzBdKQogICAgbWF4X2NvbHVtbnMgPSBvcHRpb25zLmdldCgibWF4X2NvbHVtbnMiLCBERUZBVUxUX1NUWUxFWzFdKQogICAgcmV0dXJuIChtdWx0aWxpbmUsIG1heF9jb2x1bW5zKQoKCmRlZiBkaXNwYXRjaChtZXRob2QsIGFyZ3MsIHJhd190ZXh0KToKICAgIGlmIG1ldGhvZCA9PSAiYWRkX3BhdGhfdG9faW5kZXgiOgogICAgICAgIGxvZ19jYWxsKHJhd190ZXh0KQogICAgICAgIHJldHVybiAwCiAgICBpZiBtZXRob2QgPT0gImdldF91bnJlc29sdmVkX3N5bWJvbHMiOgogICAgICAgIHNvdXJjZSA9IGFyZ3NbMF0KICAgICAgICBsb2dfY2FsbChyYXdfdGV4dCkKICAgICAgICBpZiBzb3VyY2UgaW4gVU5SRVNPTFZFRDoKICAgICAgICAgICAgcmV0dXJuIFVOUkVTT0xWRURbc291cmNlXQogICAgICAgIHJldHVybiBbIm5vLXJlY29yZGVkLXVucmVzb2x2ZWQtc3ltYm9scy1yZXNwb25zZSIsIHNvdXJjZV0KICAgIGlmIG1ldGhvZCA9PSAiZ2V0X2NhbmRpZGF0ZXNfZm9yX3N5bWJvbCI6CiAgICAgICAgc3ltYm9sID0gYXJnc1swXQogICAgICAgIGxvZ19jYWxsKHJhd190ZXh0KQogICAgICAgIGlmIHN5bWJvbCBpbiBDQU5ESURBVEVTOgogICAgICAgICAgICByZXR1cm4gQ0FORElEQVRFU1tzeW1ib2xdCiAgICAgICAgcmV0dXJuIFsibm8tcmVjb3JkZWQtY2FuZGlkYXRlcy1yZXNwb25zZSIsIHN5bWJvbF0KICAgIGlmIG1ldGhvZCA9PSAiZ2V0X2ltcG9ydF9zdGF0ZW1lbnQiOgogICAgICAgIHNvdXJjZSwgc3RhdGVtZW50ID0gYXJnc1swXSwgYXJnc1sxXQogICAgICAgIGxvZ19jYWxsKHJhd190ZXh0KQogICAgICAgIGtleSA9IHN0eWxlX2tleShhcmdzWzJdIGlmIGxlbihhcmdzKSA+IDIgZWxzZSBOb25lKQogICAgICAgIGZvdW5kID0gbG9va3VwX3N0YXRlbWVudChzb3VyY2UsIHN0YXRlbWVudCwga2V5KQogICAgICAgIGlmIGZvdW5kIGlzIG5vdCBOb25lOgogICAgICAgICAgICByZXR1cm4gZm91bmQKICAgICAgICByZXR1cm4gWyJuby1yZWNvcmRlZC1pbXBvcnQtc3RhdGVtZW50LXJlc3BvbnNlIiwgc291cmNlLCBzdGF0ZW1lbnQsIGxpc3Qoa2V5KV0KICAgIHJldHVybiBbIm5vLXJlY29yZGVkLXJlc3BvbnNlIiwgbWV0aG9kXQoKCmRlZiBzZXJ2ZSgpOgogICAgc29jayA9IHNvY2tldC5zb2NrZXQoc29ja2V0LkFGX0lORVQ2LCBzb2NrZXQuU09DS19TVFJFQU0pCiAgICBzb2NrLnNldHNvY2tvcHQoc29ja2V0LklQUFJPVE9fSVBWNiwgc29ja2V0LklQVjZfVjZPTkxZLCAwKQogICAgc29jay5iaW5kKCgiOjoiLCAwKSkKICAgIHNvY2subGlzdGVuKDQpCiAgICBwb3J0ID0gc29jay5nZXRzb2NrbmFtZSgpWzFdCiAgICBzeXMuc3Rkb3V0LndyaXRlKCIlZFxuIiAlIHBvcnQpCiAgICBzeXMuc3Rkb3V0LmZsdXNoKCkKICAgIGNvbm4sIF8gPSBzb2NrLmFjY2VwdCgpCiAgICB0cnk6CiAgICAgICAgYnVmID0gYiIiCiAgICAgICAgd2hpbGUgVHJ1ZToKICAgICAgICAgICAgY2h1bmsgPSBjb25uLnJlY3YoNDA5NikKICAgICAgICAgICAgaWYgbm90IGNodW5rOgogICAgICAgICAgICAgICAgYnJlYWsKICAgICAgICAgICAgYnVmICs9IGNodW5rCiAgICAgICAgICAgIHdoaWxlIGxlbihidWYpID49IDY6CiAgICAgICAgICAgICAgICB0cnk6CiAgICAgICAgICAgICAgICAgICAgbGVuZ3RoID0gaW50KGJ1Zls6Nl0sIDE2KQogICAgICAgICAgICAgICAgZXhjZXB0IFZhbHVlRXJyb3I6CiAgICAgICAgICAgICAgICAgICAgYnJlYWsKICAgICAgICAgICAgICAgIGlmIGxlbihidWYpIDwgNiArIGxlbmd0aDoKICAgICAgICAgICAgICAgICAgICBicmVhawogICAgICAgICAgICAgICAgcGF5bG9hZCA9IGJ1Zls2OjYgKyBsZW5ndGhdCiAgICAgICAgICAgICAgICBidWYgPSBidWZbNiArIGxlbmd0aDpdCiAgICAgICAgICAgICAgICB0cnk6CiAgICAgICAgICAgICAgICAgICAgbWVzc2FnZSA9IHBhcnNlX3NleHAocGF5bG9hZC5kZWNvZGUoInV0Zi04IikpCiAgICAgICAgICAgICAgICBleGNlcHQgKFZhbHVlRXJyb3IsIFVuaWNvZGVEZWNvZGVFcnJvcikgYXMgZXhjOgogICAgICAgICAgICAgICAgICAgIHJlcGx5ID0gJyhlcGMtZXJyb3IgJXMgJXMpJyAlICgKICAgICAgICAgICAgICAgICAgICAgICAgZWxpc3Bfc3RyKHN0cihleGMpKSwgZWxpc3Bfc3RyKCJiYWQgcmVxdWVzdCIpKQogICAgICAgICAgICAgICAgICAgIGNvbm4uc2VuZGFsbChmcmFtZShyZXBseSkpCiAgICAgICAgICAgICAgICAgICAgY29udGludWUKICAgICAgICAgICAgICAgIGlmIG5vdCBtZXNzYWdlIG9yIG1lc3NhZ2VbMF0gIT0gImNhbGwiOgogICAgICAgICAgICAgICAgICAgIGNvbnRpbnVlCiAgICAgICAgICAgICAgICBfLCB1aWQsIG1ldGhvZCA9IG1lc3NhZ2VbMF0sIG1lc3NhZ2VbMV0sIG1lc3NhZ2VbMl0KICAgICAgICAgICAgICAgIGFyZ3MgPSBtZXNzYWdlWzM6XQogICAgICAgICAgICAgICAgIyBUaGUgZWxpc3Agc2lkZSBwYXNzZXMgdGhlIGFyZ3VtZW50IGxpc3QgYXMgb25lIG5lc3RlZAogICAgICAgICAgICAgICAgIyBsaXN0IHdoZW4gaXQgaGFzIG1vcmUgdGhhbiBvbmUgZWxlbWVudDsgc3BsYXkgaXQgdGhlCiAgICAgICAgICAgICAgICAjIHdheSBlcGMucHkgZG9lcy4KICAgICAgICAgICAgICAgIGlmIGxlbihhcmdzKSA9PSAxIGFuZCBpc2luc3RhbmNlKGFyZ3NbMF0sIGxpc3QpOgogICAgICAgICAgICAgICAgICAgIGFyZ3MgPSBhcmdzWzBdCiAgICAgICAgICAgICAgICByZXN1bHQgPSBkaXNwYXRjaChtZXRob2QsIGFyZ3MsIHBheWxvYWQuZGVjb2RlKCJ1dGYtOCIpKQogICAgICAgICAgICAgICAgcmVwbHkgPSAiKHJldHVybiAlZCAlcykiICUgKHVpZCwgZWxpc3AocmVzdWx0KSkKICAgICAgICAgICAgICAgIGNvbm4uc2VuZGFsbChmcmFtZShyZXBseSkpCiAgICBmaW5hbGx5OgogICAgICAgIGNvbm4uY2xvc2UoKQogICAgICAgIHNvY2suY2xvc2UoKQoKCmRlZiBmcmFtZShzZXhwX3RleHQpOgogICAgbXNnID0gKHNleHBfdGV4dCArICJcbiIpLmVuY29kZSgidXRmLTgiKQogICAgcmV0dXJuICgiJTA2eCIgJSBsZW4obXNnKSkuZW5jb2RlKCJhc2NpaSIpICsgbXNnCgoKaWYgX19uYW1lX18gPT0gIl9fbWFpbl9fIjoKICAgIHNlcnZlKCkK"
  "Base64 of the EPC stand-in python program.  It replays responses
recorded from importmagic 0.2.0 with a SymbolIndex built from exactly
the fixture project this suite authors.")

(defun importmagic--test-install-standin ()
  "Install the EPC stand-in as `python' ahead of PATH."
  (let ((program (expand-file-name "python" importmagic--test-bin)))
    (importmagic--test-write
     program
     (decode-coding-string (base64-decode-string
                            importmagic--test-standin-b64)
                           'utf-8-unix))
    (set-file-modes program #o755)
    (setq importmagic-python-interpreter program)
    (importmagic--test-reset-calls-log)
    (setenv "PATH" (concat importmagic--test-bin
                            path-separator (getenv "PATH")))
    (setq exec-path (cons importmagic--test-bin exec-path))
    program))

(defun importmagic--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "importmagic.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/importmagic.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed importmagic location: %S" located))
    (dolist (entry importmagic--test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed importmagic source: %S"
                   (car entry))))))
    (list :upstream-tree importmagic--test-upstream-tree
          :feature (featurep 'importmagic)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'importmagic package-alist)))))))

(defun importmagic--test-open (relpath content)
  "Write CONTENT into PROJECT/RELPATH, visiting it in a fresh buffer."
  (let ((path (expand-file-name relpath importmagic--test-project))
        (name (file-name-nondirectory relpath)))
    (when (get-buffer name)
      (with-current-buffer (get-buffer name)
        (set-buffer-modified-p nil)
        (kill-buffer)))
    (importmagic--test-reset-calls-log)
    (importmagic--test-write path content)
    (find-file path)))

(defvar importmagic--test-messages nil)
(defvar importmagic--test-reads nil)

(defmacro importmagic--test-with-ui-capture (&rest body)
  "Run BODY with `message' captured and `completing-read' fed the
first option of the real collection it was offered.  The completing-read
fake is the unattended-minibuffer stand-in permitted for interactive
input; everything else runs the package's real public path."
  `(let ((importmagic--test-messages nil)
         (importmagic--test-reads nil))
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (push (apply #'format-message fmt args)
                        importmagic--test-messages)))
               ((symbol-function 'completing-read)
                (lambda (prompt collection &rest _)
                  (push (list :prompt prompt :options collection)
                        importmagic--test-reads)
                  (car collection))))
       ,@body)))

(defun importmagic--test-result (&rest plist)
  (append
   plist
   (list :messages (nreverse importmagic--test-messages)
         :reads (nreverse importmagic--test-reads)
         :calls (importmagic--test-normalize
                 (if (file-exists-p importmagic--test-calls-log)
                     (importmagic--test-read importmagic--test-calls-log)
                   "")))))

(defun importmagic--test-reset ()
  "Restore editor state mutated by the workflows."
  (when (bound-and-true-p importmagic-mode)
    (importmagic-mode -1))
  (setq importmagic-python-interpreter
        (expand-file-name "python" importmagic--test-bin))
  (when (get-buffer "app.py")
    (with-current-buffer (get-buffer "app.py")
      (set-buffer-modified-p nil)
      (kill-buffer))))

;; fixture project modules the stand-in's recorded index is built from
(importmagic--test-write
 (expand-file-name "widgets.py" importmagic--test-project)
 "class Widget(object):\n    def render(self, surface):\n        return surface.draw(self)\n")
(importmagic--test-write
 (expand-file-name "gadgets/__init__.py" importmagic--test-project) "")
(importmagic--test-write
 (expand-file-name "gadgets/spinner.py" importmagic--test-project)
 "class Spinner(object):\n    frames = ['-', '\\\\', '|', '/']\n\n    def next_frame(self):\n        return self.frames[0]\n")

(importmagic--test-install-standin)
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IMPORTMAGIC_MELPA_PIN, "importmagic.el")
        .expect("prepare pinned importmagic source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn importmagic_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(
        oracle(),
        "importmagic_package_batch",
        "importmagic_parity",
        &cases,
    );
}
