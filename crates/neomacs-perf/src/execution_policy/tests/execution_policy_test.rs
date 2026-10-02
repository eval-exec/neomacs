use super::*;

fn policy(settings: &[&str]) -> ExecutionOverrides {
    settings
        .iter()
        .map(|s| s.parse().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}

#[test]
fn arm_actions_do_not_mutate_inheritance_or_the_other_arm() {
    let inherited = BTreeMap::from([
        ("NEOVM_JIT_OSR".to_owned(), OsString::from("off")),
        ("NEOVM_JIT_THRESHOLD".to_owned(), OsString::from("1000")),
        (
            "NEOVM_JIT_PROFILE".to_owned(),
            OsString::from("diagnostic.csv"),
        ),
    ]);
    let mut baseline = inherited.clone();
    let mut candidate = inherited.clone();
    policy(&["NEOVM_JIT_OSR"]).apply_to(&mut baseline);
    policy(&["NEOVM_JIT_LOOP_HEAT=0", "NEOVM_JIT_THRESHOLD=2"]).apply_to(&mut candidate);
    assert!(!baseline.contains_key("NEOVM_JIT_OSR"));
    assert_eq!(candidate["NEOVM_JIT_OSR"], "off");
    assert_eq!(candidate["NEOVM_JIT_LOOP_HEAT"], "0");
    assert_eq!(candidate["NEOVM_JIT_THRESHOLD"], "2");
    assert_eq!(baseline["NEOVM_JIT_THRESHOLD"], "1000");
    assert_eq!(inherited["NEOVM_JIT_OSR"], "off");
    assert!(!inherited.contains_key("NEOVM_JIT_LOOP_HEAT"));
    assert_eq!(candidate["NEOVM_JIT_PROFILE"], "diagnostic.csv");
}

#[test]
fn invalid_or_duplicate_actions_fail_in_cli_and_artifacts() {
    for input in [
        "PATH=x",
        "NEOVM_JIT_PROFILE=x",
        "NEOVM_JIT=maybe",
        "NEOVM_JIT_OSR=OFF",
        "NEOVM_JIT_THRESHOLD=0",
        "NEOVM_JIT_THRESHOLD=-1",
        "NEOVM_JIT_LOOP_HEAT=4294967296",
        "NEOVM_JIT=",
    ] {
        assert!(input.parse::<ExecutionOverride>().is_err(), "{input}");
        assert!(
            serde_json::from_str::<ExecutionOverride>(&serde_json::to_string(input).unwrap())
                .is_err(),
            "{input}"
        );
    }
    let repeated = vec!["NEOVM_JIT=0".parse().unwrap(), "NEOVM_JIT".parse().unwrap()];
    assert!(ExecutionOverrides::try_from(repeated).is_err());
    assert!(serde_json::from_str::<ExecutionOverrides>(r#"["NEOVM_JIT=0","NEOVM_JIT"]"#).is_err());
}

#[test]
fn explicit_actions_and_forced_interpreter_must_match_provenance() {
    let overrides = policy(&["NEOVM_JIT_OSR", "NEOVM_JIT_LOOP_HEAT=0"]);
    let mut recorded = BTreeMap::from([("NEOVM_JIT_LOOP_HEAT".to_owned(), "0".to_owned())]);
    assert!(overrides.validate_recorded(&recorded).is_ok());
    recorded.insert("NEOVM_JIT_OSR".to_owned(), "off".to_owned());
    assert!(overrides.validate_recorded(&recorded).is_err());
    for action in ["NEOVM_JIT", "NEOVM_JIT=1", "NEOVM_JIT=off"] {
        assert!(policy(&[action]).validate_forced_interpreter().is_err());
    }
    assert!(
        policy(&["NEOVM_JIT=0"])
            .validate_forced_interpreter()
            .is_ok()
    );
    assert!(
        ExecutionOverrides::default()
            .validate_forced_interpreter()
            .is_ok()
    );
    let encoded = serde_json::to_string(&overrides).unwrap();
    assert_eq!(
        serde_json::from_str::<ExecutionOverrides>(&encoded).unwrap(),
        overrides
    );
}

#[test]
fn regex_dfa_actions_preserve_values_and_validate_provenance() {
    for mode in ["off", "on", "verify"] {
        let input = format!("NEOVM_REGEX_DFA={mode}");
        let action: ExecutionOverride = input.parse().unwrap();
        assert_eq!(String::from(action.clone()), input);
        let encoded = serde_json::to_string(&action).unwrap();
        assert_eq!(
            serde_json::from_str::<ExecutionOverride>(&encoded).unwrap(),
            action
        );
    }
    let overrides = policy(&[
        "NEOVM_REGEX_DFA=verify",
        "NEOVM_REGEX_DFA_COLD=on",
        "NEOVM_REGEX_DFA_FIRST_STEP",
        "NEOVM_REGEX_SUFFIX_LITERAL=on",
        "NEOVM_SORT_CAPTURE=off",
    ]);
    let mut environment = BTreeMap::from([
        ("NEOVM_REGEX_DFA".to_owned(), OsString::from("off")),
        (
            "NEOVM_REGEX_DFA_FIRST_STEP".to_owned(),
            OsString::from("on"),
        ),
    ]);
    overrides.apply_to(&mut environment);
    assert_eq!(environment["NEOVM_REGEX_DFA"], "verify");
    assert_eq!(environment["NEOVM_REGEX_DFA_COLD"], "on");
    assert!(!environment.contains_key("NEOVM_REGEX_DFA_FIRST_STEP"));
    assert_eq!(environment["NEOVM_REGEX_SUFFIX_LITERAL"], "on");
    assert_eq!(environment["NEOVM_SORT_CAPTURE"], "off");
    let mut recorded = environment
        .into_iter()
        .map(|(key, value)| (key, value.into_string().unwrap()))
        .collect::<BTreeMap<_, _>>();
    assert!(overrides.validate_recorded(&recorded).is_ok());
    recorded.insert("NEOVM_REGEX_DFA".to_owned(), "on".to_owned());
    assert!(overrides.validate_recorded(&recorded).is_err());
    for invalid in [
        "NEOVM_REGEX_DFA=maybe",
        "NEOVM_REGEX_DFA_COLD=verify",
        "NEOVM_REGEX_DFA_FIRST_STEP=verify",
        "NEOVM_REGEX_SUFFIX_LITERAL=verify",
        "NEOVM_SORT_CAPTURE=true",
    ] {
        assert!(invalid.parse::<ExecutionOverride>().is_err(), "{invalid}");
    }
}
