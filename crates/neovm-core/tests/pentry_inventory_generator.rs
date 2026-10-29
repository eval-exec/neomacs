//! Negative tests of the exact source scanner and validator invoked by build.rs.
//!
//! Inputs are syntax fixtures, not alternative native implementations. The
//! resulting mock source/output trees stay under this worktree's tmp/pentry.

use std::fs;
use std::path::PathBuf;

#[path = "../build_support/pentry_inventory.rs"]
mod generator;

struct Fixture {
    _directory: tempfile::TempDir,
    manifest: PathBuf,
    project: PathBuf,
    output: PathBuf,
}

impl Fixture {
    fn new(input: &str) -> Self {
        let evidence = neomacs_infra::crate_root!().join("../../tmp/pentry/generator-fixtures");
        fs::create_dir_all(&evidence).expect("create worktree fixture evidence directory");
        let directory = tempfile::Builder::new()
            .prefix("inventory-")
            .tempdir_in(evidence)
            .expect("create isolated inventory fixture");
        let manifest = directory.path().join("crate");
        let project = directory.path().join("project");
        let output = directory.path().join("out");
        for path in [
            manifest.join("src/emacs_core/runtime/eval"),
            manifest.join("src/emacs_core/runtime/eval/tier_i"),
            manifest.join("src/emacs_core/runtime/bytecode"),
            manifest.join("src/emacs_core/lisp/native/subr"),
            project.join("lisp"),
            output.clone(),
        ] {
            fs::create_dir_all(path).expect("create mock inventory directories");
        }
        fs::write(manifest.join("src/fixture.rs"), input).expect("write scanner input");
        fs::write(
            manifest.join("src/emacs_core/runtime/eval/subrs.rs"),
            "fn special(name: &str) { WorldSubr::new(SubrSpec::evaluator(name, arity, dispatch)); }\n\
             fn callable(name: &str) { WorldSubr::new(SubrSpec::evaluator(name, arity, dispatch)); }\n",
        )
        .expect("write explicit evaluator-template declarations");
        fs::write(
            manifest.join("src/emacs_core/runtime/bytecode/opcode.rs"),
            "pub enum Op { Fixture }\nworld_registry!(Op);\n",
        )
        .expect("write explicit opcode registry");
        fs::write(
            manifest.join("src/emacs_core/runtime/eval/tier_i/compile.rs"),
            "pub(super) enum Op { FixtureCall, FixtureBranch { target: usize } }\nworld_registry!(Op);\n",
        )
        .expect("write explicit private Tier-I opcode registry");
        fs::write(
            manifest.join("src/emacs_core/lisp/native/subr/leaf.rs"),
            "pub enum LeafId { Fixture }\nworld_registry!(LeafId);\n",
        )
        .expect("write explicit leaf registry");
        fs::write(
            project.join("lisp/subr.el"),
            "(defalias 'fixture-alias #'fixture-target)\n",
        )
        .expect("write alias source fixture");
        Self {
            _directory: directory,
            manifest,
            project,
            output,
        }
    }

    fn generate(&self) -> Result<(), String> {
        generator::generate(&self.manifest, &self.project, &self.output)
    }

    fn replace_input(&self, input: &str) {
        fs::write(self.manifest.join("src/fixture.rs"), input).expect("mutate one fixture line");
    }
}

#[test]
fn pentry_inventory_generator_rejects_missing_authority_and_one_line_marker_repairs_it() {
    let input = include_str!("pentry_inventory_inputs/missing.rs");
    let fixture = Fixture::new(input);
    let error = fixture
        .generate()
        .expect_err("unclassified primitive must fail the build generator");
    assert_eq!(
        error,
        "primitive inventory: missing authority classification: pentry-missing (src/fixture.rs)"
    );
    eprintln!("{error}");
    fixture.replace_input(&input.replace(
        "ctx.register_subr(SubrSpec::fixed0(\"pentry-missing\", no_op));",
        "ctx.register_subr(world_subr!(SubrSpec::fixed0(\"pentry-missing\", no_op)));",
    ));
    fixture
        .generate()
        .expect("one-line explicit classification must generate successfully");
    let inventory = fs::read_to_string(fixture.output.join("pentry-inventory.json"))
        .expect("read repaired inventory artifact");
    assert!(inventory.contains("pentry-missing"));
}

#[test]
fn pentry_inventory_generator_rejects_duplicate_registration_and_one_line_removal_repairs_it() {
    let input = include_str!("pentry_inventory_inputs/duplicate.rs");
    let fixture = Fixture::new(input);
    let error = fixture
        .generate()
        .expect_err("duplicate primitive must fail the build generator");
    assert_eq!(
        error,
        "primitive inventory: duplicate registration: pentry-duplicate (src/fixture.rs)"
    );
    eprintln!("{error}");
    fixture.replace_input(&input.replacen(
        "    ctx.register_subr(world_subr!(SubrSpec::fixed0(\"pentry-duplicate\", no_op)));\n",
        "",
        1,
    ));
    fixture
        .generate()
        .expect("removing one duplicate line must generate successfully");
}

#[test]
fn pentry_inventory_generator_rejects_unknown_constructor_and_one_line_known_constructor_repairs_it()
 {
    let input = include_str!("pentry_inventory_inputs/unknown.rs");
    let fixture = Fixture::new(input);
    assert_eq!(
        fixture
            .generate()
            .expect_err("unknown constructor must fail closed"),
        "src/fixture.rs: unsupported SubrSpec constructor: future_constructor"
    );
    fixture.replace_input(&input.replace("future_constructor", "fixed0"));
    fixture
        .generate()
        .expect("a classified known constructor must generate successfully");
}

#[test]
fn pentry_inventory_generator_rejects_subr_struct_literal_bypass() {
    let fixture = Fixture::new("fn register() { let _ = SubrSpec { name: \"bypass\" }; }");
    assert_eq!(
        fixture
            .generate()
            .expect_err("struct-literal bypass must fail closed"),
        "src/fixture.rs: unsupported SubrSpec struct literal; use an explicitly classified constructor"
    );
}

#[test]
fn pentry_inventory_generator_rejects_marker_mentioned_outside_macro_constructor() {
    let fixture = Fixture::new(
        "macro_rules! register_pure_subr {\n\
           ($ctx:expr, $name:expr, $func:expr, $min:expr, $max:expr) => {\n\
             let _ = stringify!(world_subr);\n\
             $ctx.register_subr(SubrSpec::new($name, NativeFn::ContextVec($func), SubrArity::new($min, $max)));\n\
           };\n\
         }\n\
         fn register(ctx: &mut Context) { register_pure_subr!(ctx, \"pentry-template\", no_op, 0, Some(0)); }\n",
    );
    let error = fixture
        .generate()
        .expect_err("a mentioned marker cannot classify another expression");
    assert!(error.contains("register_pure_subr template lacks an explicitly World-classified name-parameter constructor"));
}

#[test]
fn pentry_inventory_generator_covers_private_tier_i_enum_without_exposing_it() {
    let fixture = Fixture::new("");
    fixture
        .generate()
        .expect("explicit Tier-I registry must generate successfully");
    let inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.output.join("pentry-inventory.json")).expect("read generated inventory"),
    )
    .expect("parse generated inventory");
    let names = inventory["entries"]
        .as_array()
        .expect("inventory entries")
        .iter()
        .filter(|entry| entry["source"] == "src/emacs_core/runtime/eval/tier_i/compile.rs")
        .map(|entry| entry["name"].as_str().expect("opcode name"))
        .collect::<Vec<_>>();
    assert_eq!(names, ["TierI::FixtureCall", "TierI::FixtureBranch"]);
    let generated = fs::read_to_string(fixture.output.join("pentry_inventory.rs"))
        .expect("read generated Rust sidecar");
    assert!(generated.contains("static TIER_I_OPCODES"));
    assert!(!generated.contains("tier_i::compile::Op"));
}

#[test]
fn pentry_inventory_generator_accepts_unrelated_cfg_select_expression() {
    let fixture = Fixture::new("fn flags() { let _ = std::cfg_select! { unix => 1, _ => 2 }; }");
    fixture
        .generate()
        .expect("nonregistration cfg_select expressions are outside the inventory grammar");
}
