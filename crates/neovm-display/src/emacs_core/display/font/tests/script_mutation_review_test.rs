use super::*;
use neovm_eval::emacs_core::eval::{
    FontListError, FontListFilter, FontSpecListQuery, ListedFontEntity,
};

type Requests = Rc<RefCell<Vec<FontSpecListQuery>>>;
struct ScriptHost(Requests);
impl DisplayHost for ScriptHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn prepare_font_listing(
        &mut self,
        query: &FontSpecListQuery,
    ) -> Result<Option<neovm_eval::emacs_core::eval::FontListingPreparation>, FontListError> {
        if query.driver.is_some_and(|d| d != intern("ftcrhb")) {
            return Ok(None);
        }
        if query
            .specification
            .registry
            .as_ref()
            .and_then(LispString::as_utf8_str)
            == Some("[")
        {
            return Err(FontListError::Filter(signal(
                "invalid-regexp",
                vec![Value::string("Unmatched [ or [^")],
            )));
        }
        use neovm_eval::emacs_core::eval::FontListingPreparation as Prepared;
        Ok(Some(
            if query
                .specification
                .registry
                .as_ref()
                .and_then(LispString::as_utf8_str)
                == Some("iso8859-2")
            {
                Prepared::FontconfigWithCharset
            } else {
                Prepared::FontconfigWithoutCharset
            },
        ))
    }
    fn list_fonts_for_spec(
        &mut self,
        query: FontSpecListQuery,
        _: Option<FontSpecListQuery>,
        _: Option<usize>,
        _: Option<&mut FontListFilter<'_>>,
        _rescale: Option<&mut neovm_eval::emacs_core::eval::FontListRescale<'_>>,
    ) -> Result<Vec<ListedFontEntity>, FontListError> {
        self.prepare_font_listing(&query)?;
        self.0.borrow_mut().push(query);
        Ok(Vec::new())
    }
}
fn context() -> (Context, Requests) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    eval.set_gc_threshold(usize::MAX);
    ensure_selected_gui_frame(&mut eval);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(ScriptHost(Rc::clone(&requests))));
    (eval, requests)
}
fn script_spec() -> Value {
    font_spec(vec![
        Value::keyword("family"),
        Value::string("Hack"),
        Value::keyword("script"),
        Value::symbol("latin"),
    ])
    .unwrap()
}
fn install_reps(eval: &mut Context, chars: Value) {
    eval.set_variable(
        "script-representative-chars",
        Value::list(vec![Value::cons(Value::symbol("latin"), chars)]),
    );
}
fn condition(result: EvalResult, name: &str) {
    let error = result.expect_err("query must reject malformed consumed policy");
    let neovm_value::error::FlowRef::Signal(value) = error.kind() else {
        panic!("expected signal")
    };
    assert_eq!(value.symbol, intern(name));
}

#[test]
fn script_mutation_early_match_and_dotted_character_tail_remain_usable() {
    let (mut eval, requests) = context();
    let chars = Value::cons(Value::fixnum(65), Value::fixnum(42));
    let alist = Value::list(vec![Value::cons(Value::symbol("latin"), chars)]);
    alist.set_cdr(alist);
    eval.set_variable("script-representative-chars", alist);
    list_fonts(&mut eval, vec![script_spec()]).unwrap();
    assert_eq!(
        requests.borrow()[0].script_coverage,
        Some(neovm_eval::emacs_core::eval::FontScriptCoverage::All(vec![
            65
        ]))
    );
    let mappings = Value::list(vec![Value::cons(
        Value::symbol("latn"),
        Value::symbol("latin"),
    )]);
    mappings.set_cdr(mappings);
    eval.set_variable("otf-script-alist", mappings);
    let primary = font_spec(vec![
        Value::keyword("family"),
        Value::string("Hack"),
        Value::keyword("otf"),
        Value::list(vec![Value::symbol("latn")]),
    ])
    .unwrap();
    list_fonts(&mut eval, vec![primary]).unwrap();
    assert_eq!(
        requests.borrow()[1].script_coverage,
        Some(neovm_eval::emacs_core::eval::FontScriptCoverage::All(vec![
            65
        ]))
    );
}

#[test]
fn script_mutation_maximum_and_preference_do_not_consume_representatives() {
    let (mut eval, requests) = context();
    let chars = Value::list(vec![Value::fixnum(65)]);
    chars.set_cdr(chars);
    install_reps(&mut eval, chars);
    assert!(
        list_fonts(
            &mut eval,
            vec![script_spec(), Value::NIL, Value::fixnum(0), Value::T]
        )
        .unwrap()
        .is_nil()
    );
    assert!(requests.borrow().is_empty());
    let plain = font_spec(vec![Value::keyword("family"), Value::string("Hack")]).unwrap();
    list_fonts(
        &mut eval,
        vec![plain, Value::NIL, Value::fixnum(1), script_spec()],
    )
    .unwrap();
    assert!(requests.borrow()[0].script_coverage.is_none());
}

#[cfg(target_os = "linux")]
fn child(case: &str) {
    let (mut eval, requests) = context();
    let chars = Value::list(vec![Value::fixnum(65)]);
    let mut primary = script_spec();
    install_reps(&mut eval, chars);
    if case == "representative-alist" {
        let alist = Value::list(vec![Value::cons(Value::symbol("other"), chars)]);
        alist.set_cdr(alist);
        eval.set_variable("script-representative-chars", alist);
    } else if case == "otf-alist" {
        primary = font_spec(vec![
            Value::keyword("family"),
            Value::string("Hack"),
            Value::keyword("otf"),
            Value::list(vec![Value::symbol("latn")]),
        ])
        .unwrap();
        let alist = Value::list(vec![Value::cons(
            Value::symbol("other"),
            Value::symbol("latin"),
        )]);
        alist.set_cdr(alist);
        eval.set_variable("otf-script-alist", alist);
    } else {
        chars.set_cdr(chars);
    }
    match case {
        "foreign" => {
            font_put(vec![primary, Value::keyword("type"), Value::symbol("ns")]).unwrap();
        }
        "legacy" | "invalid-registry" => {
            font_put(vec![
                primary,
                Value::keyword("registry"),
                Value::string(if case == "legacy" { "iso8859-2" } else { "[" }),
            ])
            .unwrap();
        }
        "dotted-otf" => {
            let features = Value::list(vec![Value::symbol("ccmp")]);
            font_put(vec![
                primary,
                Value::keyword("otf"),
                Value::list(vec![Value::symbol("latn"), Value::NIL, features]),
            ])
            .unwrap();
            features.set_cdr(Value::fixnum(42));
        }
        _ => {}
    }
    let pages: u64 = std::fs::read_to_string("/proc/self/statm")
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    // SAFETY: sysconf reads a process constant; limit writes target only this
    // isolated, parent-timed subprocess, never the nextest worker or desktop.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    assert!(page_size > 0);
    let bytes = pages * page_size as u64 + 32 * 1024 * 1024;
    let limit = libc::rlimit {
        rlim_cur: bytes,
        rlim_max: bytes,
    };
    let no_core = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: both rlimit pointers are initialized and remain live for the calls.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) }, 0);
    // SAFETY: disabling core dumps is local to this owned subprocess.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) }, 0);
    let result = list_fonts(&mut eval, vec![primary, Value::NIL, Value::fixnum(1)]);
    match case {
        "foreign" | "legacy" => {
            result.unwrap();
            assert!(
                requests
                    .borrow()
                    .iter()
                    .all(|q| q.script_coverage.is_none())
            );
        }
        "invalid-registry" => condition(result, "invalid-regexp"),
        "dotted-otf" => condition(result, "wrong-type-argument"),
        _ => condition(result, "circular-list"),
    }
    println!("SCRIPT-MUTATION-CHECKED {case}");
}

#[cfg(target_os = "linux")]
#[test]
fn script_mutation_cycles_are_bounded_and_respect_driver_and_error_order() {
    const ENV: &str = "NEOMACS_SCRIPT_MUTATION_CHILD";
    const TEST: &str = "emacs_core::font::tests::script_mutation_review_test::script_mutation_cycles_are_bounded_and_respect_driver_and_error_order";
    if let Some(case) = std::env::var_os(ENV) {
        child(case.to_str().unwrap());
        return;
    }
    let mut failures = Vec::new();
    for case in [
        "characters",
        "representative-alist",
        "otf-alist",
        "foreign",
        "legacy",
        "invalid-registry",
        "dotted-otf",
    ] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads", "1"])
            .env(ENV, case)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() {
            if started.elapsed() > std::time::Duration::from_secs(10) {
                child.kill().unwrap();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let output = child.wait_with_output().unwrap();
        if !output.status.success()
            || !String::from_utf8_lossy(&output.stdout)
                .contains(&format!("SCRIPT-MUTATION-CHECKED {case}"))
        {
            failures.push(format!(
                "{case}: {} {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn width_style_render_round_trips_without_extra_xlfd_field_separators() {
    let _eval = Context::new(crate::test_utils::unit_runtime());
    for (style, expected) in [
        ("semi-condensed", "semicondensed"),
        ("semi-expanded", "semiexpanded"),
        ("normal", "normal"),
    ] {
        let spec = font_spec(vec![
            Value::keyword("family"),
            Value::string("Hack"),
            Value::keyword("width"),
            Value::symbol(style),
        ])
        .unwrap();
        let name = font_xlfd_name(vec![spec]).unwrap();
        assert_eq!(
            name.as_utf8_str(),
            Some(format!("-*-Hack-*-*-{expected}-*-*-*-*-*-*-*-*-*").as_str())
        );
        let parsed = font_spec(vec![Value::keyword("name"), name]).unwrap();
        let width = font_get(vec![parsed, Value::keyword("width")]).unwrap();
        assert_eq!(width.as_bare_symbol_name(), Some(expected));
    }
}
