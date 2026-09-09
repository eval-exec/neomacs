#!/usr/bin/env python3
"""Linux Cargo linkage regression: split/unsplit and shared/static ncurses.

Run from the repository: python3 crates/neomacs-terminfo/tests/linkage.py
Uses isolated miniature consumers, real Cargo/rustc/linker invocations, and
synthetic native libraries. No environment or files in the caller are changed.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile

CRATE = Path(__file__).resolve().parents[1]


def run(args, directory, env=None):
    result = subprocess.run(args, cwd=directory, env=env, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"{' '.join(map(str, args))}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def main():
    with tempfile.TemporaryDirectory(prefix="neomacs-linkage-") as temp:
        root = Path(temp)
        for split in (False, True):
            for static in (False, True):
                case = root / f"{'split' if split else 'unsplit'}-{'static' if static else 'shared'}"
                case.mkdir()
                lib = case / "native"
                lib.mkdir()
                (lib / "terminfo.c").write_text('''
int tgetent(char *buffer, const char *term) { return 1; }
void *set_curterm(void *term) { return 0; }
char *tgetstr(const char *name, char **area) { return "fixture"; }
char *tigetstr(const char *name) { return "fixture"; }
int tgetnum(const char *name) { return 256; }
int tgetflag(const char *name) { return 1; }
int tigetflag(const char *name) { return 1; }
char *tparm(const char *format, ...) { return "expanded"; }
''')
                (lib / "empty.c").write_text("int ncurses_placeholder(void) { return 0; }\n")
                names = [("ncursesw", "empty" if split else "terminfo")]
                if split:
                    names.append(("tinfow", "terminfo"))
                for name, source in names:
                    obj = lib / f"{source}.o"
                    run(["cc", "-fPIC", "-c", str(lib / f"{source}.c"), "-o", str(obj)], case)
                    if static:
                        run(["ar", "crs", str(lib / f"lib{name}.a"), str(obj)], case)
                    else:
                        run(["cc", "-shared", str(obj), "-o", str(lib / f"lib{name}.so")], case)
                flags = " ".join(f"-l{name}" for name, _ in names)
                (lib / "ncursesw.pc").write_text(
                    f"Name: ncursesw\nDescription: synthetic linkage fixture\nVersion: 6.6\nLibs: -L{lib} {flags}\n"
                )
                (case / "Cargo.toml").write_text(
                    '[package]\nname="link-consumer"\nversion="0.0.0"\nedition="2024"\n'
                    f'[dependencies]\nneomacs-terminfo={{path={json.dumps(str(CRATE))}}}\n[workspace]\n'
                )
                (case / "build.rs").write_text('''
fn main() {
    if let Some(paths) = std::env::var_os("DEP_NEOMACS_TERMINFO_RUNTIME_LIBDIRS") {
        for path in std::env::split_paths(&paths) {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path.display());
        }
    }
}
''')
                (case / "src").mkdir()
                (case / "src/lib.rs").write_text('#[path="main.rs"] pub mod startup;\n')
                (case / "src/main.rs").write_text('''
use neomacs_terminfo::{Database, Query, StringCapability};
fn main() {
    assert_eq!(neomacs_terminfo::expand_numeric(b"%p1%d", [0; 9]).unwrap(), b"expanded");
    let db = Database::load("fixture", &[Query::String(StringCapability::Termcap("md"))]).unwrap();
    assert_eq!(db.string(StringCapability::Termcap("md")), Some(b"fixture".as_slice()));
}
#[test] fn native_dependency() { main(); }
''')
                env = dict(os.environ, PKG_CONFIG_LIBDIR=str(lib), PKG_CONFIG_PATH=str(lib),
                           CARGO_TARGET_DIR=str(case / "target"))
                env.pop("NCURSESW_DYNAMIC", None)
                # Ensure static metadata is exercised when requested.
                env["NCURSESW_STATIC"] = "1" if static else "0"
                if not static:
                    env.pop("NCURSESW_STATIC", None)
                    env["NCURSESW_DYNAMIC"] = "1"
                run(["cargo", "build", "--release", "--offline"], case, env)
                runtime_env = dict(env)
                runtime_env.pop("LD_LIBRARY_PATH", None)
                run([str(case / "target/release/link-consumer")], case, runtime_env)
                run(["cargo", "nextest", "run", "--offline"], case, env)
                print(f"{case.name}: release executable and library/binary tests passed", flush=True)


if __name__ == "__main__":
    main()
