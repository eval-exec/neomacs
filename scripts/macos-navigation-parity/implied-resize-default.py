"""Direct paired default row for the pinned linux-gtk3 and macos-ns builds."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--gnu-prefix", type=Path, required=True)
parser.add_argument("--neo-binary", type=Path, required=True)
parser.add_argument("--runtime-root", type=Path, required=True)
parser.add_argument("--product-sha", required=True)
args = parser.parse_args()
attest = subprocess.run(
    ["scripts/ci/check-gnu-oracle.sh", str(args.gnu_prefix / "bin/emacs"), "31.1",
     str(args.gnu_prefix / "share/gnu-emacs-oracle/build-manifest"),
     "a360712c9d272d950d8d8255ef74570f7e90b7d9"],
    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
print(attest.stdout, flush=True)
attest.check_returncode()
forms = {
    "default": "(prin1 frame-inhibit-implied-resize)",
    "special_and_restore": """(prin1 (list frame-inhibit-implied-resize
      (special-variable-p 'frame-inhibit-implied-resize)
      (let ((frame-inhibit-implied-resize nil)) frame-inhibit-implied-resize)
      frame-inhibit-implied-resize))"""}
results = {}
for editor, binary in {"gnu": args.gnu_prefix / "bin/emacs", "neo": args.neo_binary}.items():
    version = subprocess.check_output([str(binary), "--version"], text=True)
    if editor == "neo":
        assert args.product_sha in version
    print(editor.upper(), json.dumps({"binary": str(binary), "version": version,
          "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
          "controller_host_policy": os.environ.get("NEOMACS_BENCH_CONTROLLER_POLICY", "not recorded")}),
          flush=True)
    results[editor] = {}
    for row, form in forms.items():
        env = dict(os.environ, RUST_LOG="error", NEOMACS_RUNTIME_ROOT=str(args.runtime_root))
        p = subprocess.run([str(binary), "-Q", "--batch", "--eval", form], env=env,
                           text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        print("ROW", editor, row, "rc", p.returncode, json.dumps(p.stdout), flush=True)
        p.check_returncode()
        results[editor][row] = p.stdout
assert results["gnu"]["default"] == "(tab-bar-lines)"
assert results["gnu"]["special_and_restore"] == "((tab-bar-lines) t nil (tab-bar-lines))"
assert results["neo"] == results["gnu"], "frame-inhibit-implied-resize differs from canonical GNU build"
