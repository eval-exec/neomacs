#![forbid(unsafe_code)]

//! The copied product control/ring stands alone: no direct `std::sync`,
//! `crate::` or `super::super`, and the L6 ring API is present. Synchronization
//! belongs to the rendezvous `sync` seam; this model drives the product state
//! under Loom's own Mutex/Condvar.

const CONTROL: &str = include_str!(concat!(env!("OUT_DIR"), "/control.rs"));

#[test]
fn copied_control_uses_the_sync_seam_and_keeps_the_ring() {
    let code = strip_comments(CONTROL);
    for token in ["std::sync", "crate::", "super::super"] {
        assert!(
            !code.contains(token),
            "copied control.rs leaks `{token}` outside a comment"
        );
    }
    for expected in [
        "struct RequestId",
        "enum RingPhase",
        "struct RingEntry",
        "fn enqueue_serial",
        "fn try_grant_head",
        "fn retire_serial",
        "fn ring_head",
        "fn ring_len",
    ] {
        assert!(
            code.contains(expected),
            "copied control.rs lost `{expected}`: re-anchor the L6 ring seam"
        );
    }
    assert!(
        !code.contains("pub(in crate::tagged::gc)"),
        "crate visibility was not re-anchored for the standalone model"
    );
}

/// Drop `//` and `/* */` comments. String bodies stay, so a path inside a
/// string is still a hit. The product file uses no raw strings.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut previous = '\0';
                    for c in chars.by_ref() {
                        if previous == '*' && c == '/' {
                            break;
                        }
                        previous = c;
                    }
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}
