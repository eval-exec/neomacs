//! Coverage checks for oracle parity tests.

use std::collections::{BTreeSet, HashSet};
use std::process::Command;

use crate::common::live_oracle_enabled;

use crate::coverage_manifest::{
    ORACLE_TESTED_NONPRIMITIVE_NAMES, ORACLE_TESTED_PRIMITIVE_NAMES,
    ORACLE_TESTED_SPECIAL_FORM_NAMES,
};

fn oracle_emacs_path() -> String {
    std::env::var("NEOVM_FORCE_ORACLE_PATH").unwrap_or_else(|_| "emacs".to_string())
}

fn run_oracle_name_dump(program: &str) -> Result<BTreeSet<String>, String> {
    let oracle_bin = oracle_emacs_path();

    let output = Command::new(&oracle_bin)
        .args(["--batch", "-Q", "--eval", program])
        .output()
        .map_err(|e| format!("failed to run oracle Emacs: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "oracle Emacs failed: status={}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>())
}

fn run_oracle_primitive_name_dump() -> Result<BTreeSet<String>, String> {
    run_oracle_name_dump(
        r#"(let (out)
  (mapatoms
   (lambda (sym)
     (when (fboundp sym)
       (let ((fn (symbol-function sym)))
         (when (and (subr-primitive-p fn)
                    (not (special-form-p fn)))
           (push (symbol-name sym) out))))))
  (dolist (name (sort out #'string<))
    (princ name)
    (terpri)))"#,
    )
}

fn run_oracle_special_form_name_dump() -> Result<BTreeSet<String>, String> {
    run_oracle_name_dump(
        r#"(let (out)
  (mapatoms
   (lambda (sym)
     (when (fboundp sym)
       (let ((fn (symbol-function sym)))
         (when (special-form-p fn)
           (push (symbol-name sym) out))))))
  (dolist (name (sort out #'string<))
    (princ name)
    (terpri)))"#,
    )
}

fn parse_threshold_percent(env_key: &str, default_value: f64) -> f64 {
    let raw = match std::env::var(env_key) {
        Ok(value) => value,
        Err(_) => return default_value,
    };

    raw.parse::<f64>()
        .unwrap_or_else(|_| panic!("invalid {env_key}: expected f64 percent, got '{raw}'"))
}

#[test]
fn oracle_prop_coverage_manifest_sorted_unique() {
    fn assert_sorted_unique(label: &str, names: &[&str]) {
        let mut seen = HashSet::new();
        let mut prev = "";
        for &name in names {
            assert!(!name.is_empty(), "{label} contains an empty name");
            assert!(
                name >= prev,
                "{label} should be sorted: '{name}' after '{prev}'"
            );
            assert!(seen.insert(name), "{label} contains duplicate name: {name}");
            prev = name;
        }
    }

    assert_sorted_unique(
        "ORACLE_TESTED_PRIMITIVE_NAMES",
        ORACLE_TESTED_PRIMITIVE_NAMES,
    );
    assert_sorted_unique(
        "ORACLE_TESTED_SPECIAL_FORM_NAMES",
        ORACLE_TESTED_SPECIAL_FORM_NAMES,
    );
    assert_sorted_unique(
        "ORACLE_TESTED_NONPRIMITIVE_NAMES",
        ORACLE_TESTED_NONPRIMITIVE_NAMES,
    );

    let primitive_names = ORACLE_TESTED_PRIMITIVE_NAMES
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let special_names = ORACLE_TESTED_SPECIAL_FORM_NAMES
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let nonprimitive_names = ORACLE_TESTED_NONPRIMITIVE_NAMES
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    let primitive_special_overlap = primitive_names
        .intersection(&special_names)
        .copied()
        .collect::<Vec<_>>();
    let primitive_nonprimitive_overlap = primitive_names
        .intersection(&nonprimitive_names)
        .copied()
        .collect::<Vec<_>>();
    let special_nonprimitive_overlap = special_names
        .intersection(&nonprimitive_names)
        .copied()
        .collect::<Vec<_>>();

    assert!(
        primitive_special_overlap.is_empty(),
        "primitive/special-form overlap: {}",
        primitive_special_overlap.join(", ")
    );
    assert!(
        primitive_nonprimitive_overlap.is_empty(),
        "primitive/non-primitive overlap: {}",
        primitive_nonprimitive_overlap.join(", ")
    );
    assert!(
        special_nonprimitive_overlap.is_empty(),
        "special-form/non-primitive overlap: {}",
        special_nonprimitive_overlap.join(", ")
    );
}

#[test]
fn oracle_prop_coverage_snapshot() {
    if !live_oracle_enabled() {
        tracing::info!(
            "skipping oracle coverage snapshot: set NEOVM_ORACLE_MODE=verify/refresh/live and provide GNU Emacs"
        );
        return;
    }

    let min_primitive_pct = parse_threshold_percent("NEOVM_ORACLE_MIN_PRIMITIVE_COVERAGE_PCT", 2.5);
    let min_special_form_pct =
        parse_threshold_percent("NEOVM_ORACLE_MIN_SPECIAL_FORM_COVERAGE_PCT", 40.0);

    let oracle_primitives =
        run_oracle_primitive_name_dump().expect("oracle primitive name dump should succeed");
    let oracle_special_forms =
        run_oracle_special_form_name_dump().expect("oracle special-form name dump should succeed");

    let tested_primitives = ORACLE_TESTED_PRIMITIVE_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    let tested_special_forms = ORACLE_TESTED_SPECIAL_FORM_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    let tested_nonprimitives = ORACLE_TESTED_NONPRIMITIVE_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();

    let covered_primitives = tested_primitives
        .intersection(&oracle_primitives)
        .collect::<Vec<_>>();
    let primitive_manifest_nonprimitive = tested_primitives
        .difference(&oracle_primitives)
        .collect::<Vec<_>>();
    let covered_special_forms = tested_special_forms
        .intersection(&oracle_special_forms)
        .collect::<Vec<_>>();
    let special_form_manifest_non_special = tested_special_forms
        .difference(&oracle_special_forms)
        .collect::<Vec<_>>();
    let nonprimitive_aliases = tested_nonprimitives
        .difference(&oracle_primitives)
        .collect::<Vec<_>>();

    let primitive_missing_count = oracle_primitives
        .len()
        .saturating_sub(covered_primitives.len());
    let special_form_missing_count = oracle_special_forms
        .len()
        .saturating_sub(covered_special_forms.len());

    let primitive_coverage_pct = if oracle_primitives.is_empty() {
        0.0
    } else {
        (covered_primitives.len() as f64 * 100.0) / oracle_primitives.len() as f64
    };
    let special_form_coverage_pct = if oracle_special_forms.is_empty() {
        0.0
    } else {
        (covered_special_forms.len() as f64 * 100.0) / oracle_special_forms.len() as f64
    };

    tracing::info!(
        "oracle primitive coverage: covered={}/{} ({:.2}%), tested-primitives={}, manifest-non-primitive={}, missing={}",
        covered_primitives.len(),
        oracle_primitives.len(),
        primitive_coverage_pct,
        tested_primitives.len(),
        primitive_manifest_nonprimitive.len(),
        primitive_missing_count
    );
    tracing::info!(
        "oracle special-form coverage: covered={}/{} ({:.2}%), tested-special-forms={}, manifest-non-special={}, missing={}",
        covered_special_forms.len(),
        oracle_special_forms.len(),
        special_form_coverage_pct,
        tested_special_forms.len(),
        special_form_manifest_non_special.len(),
        special_form_missing_count
    );
    tracing::info!(
        "oracle non-primitive tested names: count={}, names={}",
        tested_nonprimitives.len(),
        tested_nonprimitives
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    if !primitive_manifest_nonprimitive.is_empty() {
        let preview = primitive_manifest_nonprimitive
            .iter()
            .take(20)
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!("primitive manifest names not primitive in oracle (first 20): {preview}");
    }

    if !special_form_manifest_non_special.is_empty() {
        let preview = special_form_manifest_non_special
            .iter()
            .take(20)
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!("special-form manifest names not special in oracle (first 20): {preview}");
    }

    if !nonprimitive_aliases.is_empty() {
        let preview = nonprimitive_aliases
            .iter()
            .take(20)
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!("verified non-primitive tested names (first 20): {preview}");
    }

    assert!(
        !covered_primitives.is_empty(),
        "coverage sanity check failed: no tested primitive names matched oracle primitive names"
    );
    assert!(
        !covered_special_forms.is_empty(),
        "coverage sanity check failed: no tested special-form names matched oracle special forms"
    );
    assert!(
        primitive_manifest_nonprimitive.is_empty(),
        "primitive manifest contains non-primitives: {}",
        primitive_manifest_nonprimitive
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(
        special_form_manifest_non_special.is_empty(),
        "special-form manifest contains non-special forms: {}",
        special_form_manifest_non_special
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(
        primitive_coverage_pct >= min_primitive_pct,
        "primitive coverage {:.2}% is below threshold {:.2}% (set NEOVM_ORACLE_MIN_PRIMITIVE_COVERAGE_PCT)",
        primitive_coverage_pct,
        min_primitive_pct
    );
    assert!(
        special_form_coverage_pct >= min_special_form_pct,
        "special-form coverage {:.2}% is below threshold {:.2}% (set NEOVM_ORACLE_MIN_SPECIAL_FORM_COVERAGE_PCT)",
        special_form_coverage_pct,
        min_special_form_pct
    );
    assert!(
        nonprimitive_aliases.len() == tested_nonprimitives.len(),
        "non-primitive manifest unexpectedly overlaps primitive names"
    );
}
