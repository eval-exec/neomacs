//! The generic publication owner is compiled by two crates that are not
//! `neovm-value`.
//!
//! `publication_view_confinement.rs` includes `publication/mod.rs` by path.
//! `n5-grace-loom/build.rs` copies `publication/mod.rs` and `ledger.rs`.
//! Those copies resolve `mod ledger` and `mod sync` only. A `crate::` path
//! or a `super::super` hop fails before the confinement errors those
//! consumers exist to record.

#[cfg(test)]
mod tests {
    const OWNER_FILES: &[(&str, &str)] = &[
        ("publication/mod.rs", include_str!("publication/mod.rs")),
        (
            "publication/ledger.rs",
            include_str!("publication/ledger.rs"),
        ),
        ("publication/sync.rs", include_str!("publication/sync.rs")),
    ];

    #[test]
    fn publication_owner_compiles_for_the_standalone_consumers() {
        for (name, source) in OWNER_FILES {
            assert_owner_stands_alone(name, source);
        }
    }

    #[test]
    fn a_comment_may_name_a_crate_path() {
        let sample = "// crate:: and super::super stay in comments\nfn owner() {}\n";
        let code = strip_comments(sample);
        assert!(
            !code.contains("crate::") && !code.contains("super::super"),
            "comment text leaked into the scanned code: {code}"
        );
    }

    fn assert_owner_stands_alone(name: &str, source: &str) {
        let code = strip_comments(source);
        for token in ["crate::", "super::super"] {
            if let Some(byte) = code.find(token) {
                panic!(
                    "{name} contains `{token}` outside a comment at byte {byte}. \
                     publication_view_confinement.rs and n5-grace-loom/build.rs \
                     compile publication/{{mod,ledger,sync}}.rs on their own; \
                     heap binding belongs in the sibling modules"
                );
            }
        }
    }

    /// Drop `//` and `/* */` comments. String bodies stay, so a path inside
    /// a string is still a hit. These three files do not use raw strings.
    fn strip_comments(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '/' {
                match chars.peek().copied() {
                    Some('/') => {
                        chars.next();
                        for next in chars.by_ref() {
                            if next == '\n' {
                                out.push('\n');
                                break;
                            }
                        }
                        continue;
                    }
                    Some('*') => {
                        chars.next();
                        while let Some(next) = chars.next() {
                            if next == '*' && chars.peek().copied() == Some('/') {
                                chars.next();
                                break;
                            }
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            if c == '"' {
                out.push(c);
                while let Some(next) = chars.next() {
                    out.push(next);
                    if next == '\\' {
                        if let Some(escaped) = chars.next() {
                            out.push(escaped);
                        }
                        continue;
                    }
                    if next == '"' {
                        break;
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }
}
