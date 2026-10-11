//! GNU-backed live-spine regressions for already lowered interpreted let forms.

use super::super::compile::{Node, Op};
use crate::emacs_core::eval::{TierIEvent, TierIMode};
use crate::emacs_core::format_eval_result;

fn gnu_fixture(form: &str) -> String {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("src/emacs_core/runtime/eval/tier_i/tests/list_bindings_cases.expect");
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return std::fs::read_to_string(fixture)
            .expect("cached GNU tier-I binding fixture")
            .trim_end()
            .to_owned();
    }
    let script = root
        .join("../../tmp")
        .join(format!("tier-i-bindings-{}.el", std::process::id()));
    std::fs::write(&script, format!("(prin1 {form})\n")).expect("GNU fixture input");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    let mut command = if let Some(sandbox) = std::env::var_os("NEOVM_LISP_SANDBOX") {
        let mut command = std::process::Command::new(sandbox);
        command.arg(emacs);
        command
    } else {
        std::process::Command::new(emacs)
    };
    let output = command
        .args(["-Q", "--batch", "-l"])
        .arg(script)
        .output()
        .expect("GNU oracle");
    assert!(
        output.status.success(),
        "GNU tier-I bindings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = format!(
        "OK {}",
        String::from_utf8(output.stdout)
            .expect("GNU UTF-8")
            .trim_end()
    );
    std::fs::write(fixture, format!("{expected}\n")).expect("GNU fixture output");
    expected
}

#[test]
fn circular_lists_tier_i_live_binding_spines_match_gnu() {
    let form = include_str!("list_bindings_cases.el");
    let expected = gnu_fixture(form);
    for mode in [TierIMode::Off, TierIMode::On, TierIMode::Verify] {
        let mut context = crate::test_utils::runtime_startup_context();
        context.tier_i.set_mode(mode);
        context.tier_i.set_threshold(1);
        context.tier_i.clear_for_test();
        let forms = crate::emacs_core::value_reader::read_all(form, &context.obarray)
            .expect("parse GNU-backed tier-I binding form");
        assert_eq!(forms.len(), 1);
        let roots = context.save_specpdl_roots();
        context.push_specpdl_root(forms[0]);
        let actual = format_eval_result(&context.eval_form(forms[0]));
        context.restore_specpdl_roots(roots);
        assert_eq!(actual, expected, "mode {mode:?}");
        if mode != TierIMode::Off {
            let lowered_binding_bodies = context
                .tier_i
                .entries
                .values()
                .filter(|entry| {
                    entry.calls >= 3
                        && entry.code.as_ref().is_some_and(|code| {
                            matches!(code.seq.get(0), Some(Node::Form(form))
                                 if matches!(&form.op, Op::Let(_) | Op::LetStar(_)))
                        })
                })
                .count();
            assert_eq!(
                lowered_binding_bodies, 32,
                "each live binding spine must belong to an already lowered body"
            );
            assert!(
                context.tier_i.stats().count(TierIEvent::Run) >= 96,
                "warm and mutated interpreted functions must run compiled bodies: {}",
                context.tier_i.stats().report()
            );
        }
    }
}
