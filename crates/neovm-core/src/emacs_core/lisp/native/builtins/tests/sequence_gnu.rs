//! GNU 31.1 pins for allocation failures, list prefixes and numeric value ordering.
//! Refresh only from GNU with UPDATE_EXPECT=1 EMACS=$HOME/.local/bin/emacs.
use crate::test_utils::runtime_startup_eval_one;
use std::path::PathBuf;

pub(super) fn assert_gnu(name: &str, form: &str, fixture: &str) {
    crate::test_utils::init_test_tracing();
    let expected = if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let script = root
            .join("../../tmp")
            .join(format!("sequence-{name}-{}.el", std::process::id()));
        std::fs::write(&script, format!("(prin1 {form})\n")).expect("GNU script");
        let result =
            std::process::Command::new(std::env::var_os("EMACS").unwrap_or_else(|| "emacs".into()))
                .args(["-Q", "--batch", "-l"])
                .arg(script)
                .output()
                .expect("GNU oracle");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let output = format!(
            "OK {}",
            String::from_utf8(result.stdout)
                .expect("GNU UTF-8")
                .trim_end()
        );
        std::fs::write(
            root.join("src/emacs_core/lisp/native/builtins/tests/sequence_gnu")
                .join(format!("{name}.expect")),
            format!("{output}\n"),
        )
        .expect("GNU fixture");
        output
    } else {
        fixture.trim_end().to_owned()
    };
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[cfg(test)]
mod prefix;

#[cfg(test)]
mod ntake;

#[cfg(test)]
mod comparison;

#[cfg(test)]
mod allocation;

#[cfg(test)]
mod compiled;
