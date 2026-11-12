use super::*;
use neomacs_display_protocol::font::FontBackendKind;
use neovm_eval::emacs_core::eval::{
    FontListError, FontListFilter, FontSpecListQuery, ListedFontEntity,
};

type Requests = Rc<RefCell<Vec<FontSpecListQuery>>>;
struct OtfHost {
    kind: Option<FontBackendKind>,
    requests: Requests,
}
impl DisplayHost for OtfHost {
    fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn prepare_font_listing(
        &mut self,
        query: &FontSpecListQuery,
    ) -> Result<Option<FontBackendKind>, FontListError> {
        if self.kind == Some(FontBackendKind::Fontconfig)
            && query.driver.is_none_or(|driver| driver == intern("ftcrhb"))
            && query
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
        Ok(self.kind)
    }
    fn list_fonts_for_spec(
        &mut self,
        query: FontSpecListQuery,
        _: Option<FontSpecListQuery>,
        _: Option<usize>,
        _: Option<&mut FontListFilter<'_>>,
    ) -> Result<Vec<ListedFontEntity>, FontListError> {
        self.prepare_font_listing(&query)?;
        self.requests.borrow_mut().push(query);
        Ok(Vec::new())
    }
}
fn context(kind: Option<FontBackendKind>) -> (Context, Requests) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    eval.set_gc_threshold(usize::MAX);
    ensure_selected_gui_frame(&mut eval);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(OtfHost {
        kind,
        requests: Rc::clone(&requests),
    }));
    (eval, requests)
}
fn spec(features: Value, gpos: bool, driver: Option<Value>) -> Value {
    let mut args = vec![
        Value::keyword("family"),
        Value::string("Hack"),
        Value::keyword("otf"),
        Value::list(vec![
            Value::symbol("latn"),
            Value::NIL,
            if gpos { Value::NIL } else { features },
            if gpos { features } else { Value::NIL },
        ]),
    ];
    if let Some(driver) = driver {
        args.extend([Value::keyword("type"), driver]);
    }
    font_spec(args).unwrap()
}

#[test]
fn mutated_otf_dotted_features_signal_the_gnu_list_length_condition() {
    for gpos in [false, true] {
        let (mut eval, requests) = context(Some(FontBackendKind::Fontconfig));
        let features = Value::list(vec![Value::symbol("ccmp")]);
        let primary = spec(features, gpos, None);
        features.set_cdr(Value::fixnum(42));
        let error = list_fonts(&mut eval, vec![primary])
            .expect_err("mutated dotted feature list must be rejected");
        let neovm_value::error::FlowRef::Signal(condition) = error.kind() else {
            panic!("expected a Lisp signal")
        };
        assert_eq!(condition.symbol, intern("wrong-type-argument"));
        assert_eq!(condition.data[0], Value::symbol("listp"));
        assert_eq!(condition.data[1], Value::fixnum(42));
        assert!(requests.borrow().is_empty());
    }
}

#[test]
fn mutated_otf_query_preserves_maximum_preference_and_ignored_trailing_slots() {
    let (mut eval, requests) = context(Some(FontBackendKind::Fontconfig));
    let features = Value::list(vec![Value::symbol("ccmp")]);
    let primary = spec(features, false, None);
    features.set_cdr(features);
    assert!(
        list_fonts(
            &mut eval,
            vec![primary, Value::NIL, Value::fixnum(0), Value::T]
        )
        .unwrap()
        .is_nil()
    );
    assert!(requests.borrow().is_empty());
    let unrestricted = font_spec(vec![Value::keyword("family"), Value::string("Hack")]).unwrap();
    list_fonts(
        &mut eval,
        vec![unrestricted, Value::NIL, Value::fixnum(1), primary],
    )
    .unwrap();
    assert_eq!(requests.borrow().len(), 1);
    assert!(requests.borrow()[0].otf.is_none());
    let otf = Value::list(vec![
        Value::symbol("latn"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    let ignored = font_spec(vec![Value::keyword("otf"), otf]).unwrap();
    otf.cons_cdr()
        .cons_cdr()
        .cons_cdr()
        .set_cdr(Value::fixnum(42));
    list_fonts(&mut eval, vec![ignored]).unwrap();
    assert!(
        requests.borrow()[1].otf.is_some(),
        "GNU ignores the tail beyond the four consumed slots"
    );
}

#[cfg(target_os = "linux")]
fn bounded_cycle_child(case: &str) {
    let kind = match case {
        "ns" => Some(FontBackendKind::CoreText),
        "windows" => Some(FontBackendKind::DirectWrite),
        "unavailable" => None,
        _ => Some(FontBackendKind::Fontconfig),
    };
    let (mut eval, requests) = context(kind);
    let features = Value::list(vec![Value::symbol("ccmp")]);
    let primary = spec(
        features,
        false,
        (case == "foreign-driver").then(|| Value::symbol("ns")),
    );
    features.set_cdr(features);
    // Limit only this isolated child's additional address space after startup.
    // The pre-fix capture loop cannot grow its Vec until the machine is OOM.
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
    let pages: u64 = statm.split_whitespace().next().unwrap().parse().unwrap();
    // SAFETY: sysconf reads the process page-size constant.
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
    // SAFETY: valid limit pointers; only this subprocess is constrained and exits.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) }, 0);
    // SAFETY: disabling core dumps applies only to the isolated probe process.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_CORE, &no_core) }, 0);
    let result = list_fonts(&mut eval, vec![primary]);
    if case == "ft" {
        let error = result.expect_err("cyclic features must signal before capture/allocation");
        let neovm_value::error::FlowRef::Signal(condition) = error.kind() else {
            panic!("expected a Lisp signal")
        };
        assert_eq!(condition.symbol, intern("circular-list"));
        assert!(requests.borrow().is_empty());
    } else {
        assert!(
            result.is_ok(),
            "a driver that ignores FT features must not validate them"
        );
        assert_eq!(requests.borrow().len(), 1);
        assert!(requests.borrow()[0].otf.is_none());
    }
    println!("OTF-MUTATION-CHECKED {case}");
}

#[cfg(target_os = "linux")]
#[test]
fn mutated_otf_cycle_is_bounded_and_obeys_the_actual_driver() {
    const ENV: &str = "NEOMACS_TEST_OTF_MUTATION_CHILD";
    const TEST: &str = "emacs_core::font::tests::otf_mutation_test::mutated_otf_cycle_is_bounded_and_obeys_the_actual_driver";
    if let Some(case) = std::env::var_os(ENV) {
        bounded_cycle_child(case.to_str().unwrap());
        return;
    }
    let mut failures = Vec::new();
    for case in ["ft", "foreign-driver", "ns", "windows", "unavailable"] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads", "1"])
            .env(ENV, case)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn the resource-bounded OTF probe");
        let started = std::time::Instant::now();
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if started.elapsed() > std::time::Duration::from_secs(10) {
                child.kill().unwrap();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        if !output.status.success()
            || !String::from_utf8_lossy(&output.stdout).contains("OTF-MUTATION-CHECKED")
        {
            failures.push(format!(
                "{case}: status={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "bounded OTF children failed: {}",
        failures.join("\n")
    );
}

#[test]
fn mutated_otf_registry_error_precedes_feature_list_validation() {
    let (mut eval, _requests) = context(Some(FontBackendKind::Fontconfig));
    let features = Value::list(vec![Value::symbol("ccmp")]);
    let primary = font_spec(vec![
        Value::keyword("family"),
        Value::string("Hack"),
        Value::keyword("registry"),
        Value::string("["),
        Value::keyword("otf"),
        Value::list(vec![Value::symbol("latn"), Value::NIL, features]),
    ])
    .unwrap();
    features.set_cdr(Value::fixnum(42));
    let error = list_fonts(&mut eval, vec![primary]).expect_err("registry must be validated first");
    let neovm_value::error::FlowRef::Signal(condition) = error.kind() else {
        panic!("expected signal")
    };
    assert_eq!(condition.symbol, intern("invalid-regexp"));
    assert_eq!(condition.data[0].as_utf8_str(), Some("Unmatched [ or [^"));
}
