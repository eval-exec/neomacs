//! Request-owned JIT policy for controlled same-binary comparisons.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// One explicit setting, or removal of an inherited setting.
/// CLI spelling: NAME=VALUE sets; NAME alone restores the runtime default.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExecutionOverride {
    name: String,
    value: Option<String>,
}

impl FromStr for ExecutionOverride {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (name, value) = match input.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (input, None),
        };
        let supported = matches!(
            name,
            "NEOVM_JIT" | "NEOVM_JIT_OSR" | "NEOVM_JIT_THRESHOLD" | "NEOVM_JIT_LOOP_HEAT"
        );
        if !supported {
            return Err(format!(
                "unsupported execution setting {name:?}; expected NEOVM_JIT, NEOVM_JIT_OSR, NEOVM_JIT_THRESHOLD or NEOVM_JIT_LOOP_HEAT"
            ));
        }
        if let Some(value) = value {
            let valid = match name {
                "NEOVM_JIT" | "NEOVM_JIT_OSR" => {
                    matches!(
                        value,
                        "0" | "off" | "false" | "no" | "1" | "on" | "true" | "yes"
                    )
                }
                "NEOVM_JIT_THRESHOLD" => value.parse::<u32>().is_ok_and(|v| v > 0),
                "NEOVM_JIT_LOOP_HEAT" => value.parse::<u32>().is_ok(),
                _ => unreachable!("validated name"),
            };
            if !valid {
                return Err(format!("invalid value {value:?} for {name}"));
            }
        }
        Ok(Self {
            name: name.to_owned(),
            value: value.map(str::to_owned),
        })
    }
}

impl TryFrom<String> for ExecutionOverride {
    type Error = String;
    fn try_from(input: String) -> Result<Self, Self::Error> {
        input.parse()
    }
}

impl From<ExecutionOverride> for String {
    fn from(setting: ExecutionOverride) -> Self {
        match setting.value {
            Some(value) => format!("{}={value}", setting.name),
            None => setting.name,
        }
    }
}

/// Validated actions, with at most one action per knob.
/// Empty means inherit the request's captured parent environment.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "Vec<ExecutionOverride>", into = "Vec<ExecutionOverride>")]
pub struct ExecutionOverrides(Vec<ExecutionOverride>);

impl TryFrom<Vec<ExecutionOverride>> for ExecutionOverrides {
    type Error = String;
    fn try_from(mut settings: Vec<ExecutionOverride>) -> Result<Self, Self::Error> {
        settings.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(pair) = settings
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
        {
            return Err(format!("duplicate execution setting {}", pair[0].name));
        }
        Ok(Self(settings))
    }
}

impl From<ExecutionOverrides> for Vec<ExecutionOverride> {
    fn from(settings: ExecutionOverrides) -> Self {
        settings.0
    }
}

impl ExecutionOverrides {
    /// Merge without changing the process environment or the other arm.
    pub(crate) fn apply_to(&self, environment: &mut BTreeMap<String, OsString>) {
        for setting in &self.0 {
            if let Some(value) = &setting.value {
                environment.insert(setting.name.clone(), OsString::from(value));
            } else {
                environment.remove(&setting.name);
            }
        }
    }

    /// The bytecode-call workload owns an exact interpreter setting.
    /// Reject explicit actions that its later mandatory value would replace.
    pub(crate) fn validate_forced_interpreter(&self) -> Result<(), String> {
        if self
            .0
            .iter()
            .any(|s| s.name == "NEOVM_JIT" && s.value.as_deref() != Some("0"))
        {
            return Err("bytecode-call-loop requires NEOVM_JIT=0; this scenario cannot override or unset that setting".to_owned());
        }
        Ok(())
    }

    /// Every requested action must be reflected in the child's provenance.
    pub(crate) fn validate_recorded(
        &self,
        environment: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for setting in &self.0 {
            let actual = environment.get(&setting.name);
            if actual != setting.value.as_ref() {
                return Err(format!(
                    "execution setting {}: expected {:?}, recorded {:?}",
                    setting.name, setting.value, actual
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        assert!(
            serde_json::from_str::<ExecutionOverrides>(r#"["NEOVM_JIT=0","NEOVM_JIT"]"#).is_err()
        );
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
}
