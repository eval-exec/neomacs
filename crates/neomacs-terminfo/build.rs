#![forbid(unsafe_code)]

fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let candidates: &[&str] = match target.as_str() {
        "linux" => &["ncursesw", "ncurses"],
        "macos" => &["ncurses", "ncursesw"],
        _ => return,
    };
    for name in candidates {
        if let Ok(library) = pkg_config::Config::new().probe(name) {
            // This crate's own test executables also need non-system libraries
            // at runtime. These flags do not propagate to consumer executables.
            for path in &library.link_paths {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path.display());
            }
            // Native libraries belong to this library target. Executable rpaths
            // are packaging policy: convey paths to direct consumers via links
            // metadata instead of assuming link-arg propagates through an rlib.
            let paths = std::env::join_paths(library.link_paths)
                .expect("ncurses library paths must be representable in a search path");
            println!("cargo:runtime_libdirs={}", paths.to_string_lossy());
            return;
        }
    }
    println!("cargo:rustc-link-lib={}", candidates[0]);
    println!(
        "cargo:warning=pkg-config could not describe ncurses (tried {}); \
         assuming unsplit -l{}. Install ncurses development files and pkg-config \
         if linking fails with undefined termcap/terminfo symbols.",
        candidates.join(", "),
        candidates[0]
    );
}
