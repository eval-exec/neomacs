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
#[path = "execution_policy/tests/execution_policy_test.rs"]
mod tests;
