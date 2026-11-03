//! Architecture gate and negative fixtures for the explicit source scanner.
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
        let fixture = Self {
            _directory: directory,
            manifest,
            project,
            output,
        };
        // The opcode enum lives below emacs_core at its production site; its
        // World marker stays at the upper module that imports it.
        fixture.replace_definition("pub enum Op { Fixture }\n");
        fixture.replace_registry(&registry_input());
        fixture
    }

    fn generate(&self) -> Result<(), String> {
        self.generate_with(generator::REGISTRY_SITES)
    }

    fn generate_with(&self, sites: generator::RegistrySites) -> Result<(), String> {
        generator::generate_fixture(&self.manifest, &self.project, &self.output, sites)
    }

    fn replace_input(&self, input: &str) {
        fs::write(self.manifest.join("src/fixture.rs"), input).expect("mutate one fixture line");
    }

    fn definition_path(&self) -> PathBuf {
        opcode_site().0.file(&self.manifest, &self.project)
    }

    fn registry_path(&self) -> PathBuf {
        self.manifest.join(opcode_site().1)
    }

    fn replace_definition(&self, input: &str) {
        let path = self.definition_path();
        fs::create_dir_all(path.parent().expect("definition directory"))
            .expect("create the opcode definition directory");
        fs::write(path, input).expect("write the lowered opcode definition");
    }

    fn replace_registry(&self, input: &str) {
        fs::write(self.registry_path(), input).expect("write the opcode registry site");
    }

    fn entries(&self) -> Vec<serde_json::Value> {
        let inventory: serde_json::Value = serde_json::from_slice(
            &fs::read(self.output.join("pentry-inventory.json")).expect("read generated inventory"),
        )
        .expect("parse generated inventory");
        inventory["entries"]
            .as_array()
            .expect("inventory entries")
            .clone()
    }
}

/// The production opcode sites. Fixtures follow them, so moving the
/// definition into another crate changes `REGISTRY_SITES` and nothing here.
fn opcode_site() -> (generator::CrateSource, &'static str) {
    match generator::REGISTRY_SITES.opcode {
        generator::RegistrySite::Lowered {
            definition,
            registry,
        } => (definition, registry),
        generator::RegistrySite::Colocated(source) => {
            panic!("the split lowered the opcode registry, but {source} is colocated")
        }
    }
}

/// The module path a registry-site `use` names the opcode definition by.
fn opcode_module() -> Vec<String> {
    opcode_site()
        .0
        .module()
        .expect("opcode definition module path")
}

/// The registry site as production writes it: the marker and the import.
fn registry_input() -> String {
    format!(
        "world_registry!(Op);\npub use {}::Op;\n",
        opcode_module().join("::")
    )
}

#[test]
fn pentry_inventory_generator_rejects_missing_authority_and_one_line_marker_repairs_it() {
    let input = include_str!("pentry_inventory_inputs/missing.rs");
    let fixture = Fixture::new(input);
    let error = fixture
        .generate()
        .expect_err("unclassified primitive must fail the source scanner");
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
        .expect_err("duplicate primitive must fail the source scanner");
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
fn pentry_inventory_generator_reads_lowered_variants_with_the_upper_registry_marker() {
    let fixture = Fixture::new("");
    fixture.replace_definition("pub enum Op { Lowered, Moved(u16) }\n");
    fixture
        .generate()
        .expect("a lowered enum classified at its importing registry site must generate");
    // Entries name the module their variants come from, not the marker site.
    let definition = opcode_site().0.name();
    let opcodes = fixture
        .entries()
        .into_iter()
        .filter(|entry| entry["kind"] == "Opcode")
        .map(|entry| {
            (
                entry["name"].as_str().expect("opcode name").to_string(),
                entry["source"].as_str().expect("opcode source").to_string(),
                entry["authority"]
                    .as_str()
                    .expect("opcode authority")
                    .to_string(),
            )
        })
        .filter(|(name, _, _)| !name.starts_with("TierI::"))
        .collect::<Vec<_>>();
    assert_eq!(
        opcodes,
        [
            ("Lowered".into(), definition.clone(), "World".into()),
            ("Moved".into(), definition, "World".into()),
        ]
    );
    let generated = fs::read_to_string(fixture.output.join("pentry_inventory.rs"))
        .expect("read generated Rust sidecar");
    assert!(!generated.contains(opcode_site().1));
    assert!(generated.contains("crate::emacs_core::bytecode::Op::Moved(..) =>"));
}

#[test]
fn pentry_inventory_generator_reads_a_definition_lowered_into_another_workspace_crate() {
    let fixture = Fixture::new("");
    let definition = "crates/fixture-value/src/bytecode/opcode.rs";
    fs::create_dir_all(fixture.project.join("crates/fixture-value/src/bytecode"))
        .expect("create the lower fixture crate");
    fs::write(fixture.project.join(definition), "pub enum Op { Moved }\n")
        .expect("write the lower crate's opcode definition");
    let sites = generator::RegistrySites {
        opcode: generator::RegistrySite::Lowered {
            definition: generator::CrateSource {
                owner: generator::SourceCrate::Workspace {
                    library: "fixture_value",
                    manifest: "crates/fixture-value",
                },
                path: "src/bytecode/opcode.rs",
            },
            registry: opcode_site().1,
        },
        ..generator::REGISTRY_SITES
    };
    // The production import no longer names the moved enum.
    assert_eq!(
        fixture
            .generate_with(sites)
            .expect_err("the registry must import the enum from its new crate"),
        format!(
            "{}: registry site does not import the Op defined in {definition}; expected `use fixture_value::bytecode::opcode::Op;`",
            fixture.registry_path().display()
        )
    );
    fixture
        .replace_registry("world_registry!(Op);\npub use fixture_value::bytecode::opcode::Op;\n");
    fixture
        .generate_with(sites)
        .expect("a cross-crate import of the defining enum must generate");
    let opcodes = fixture
        .entries()
        .into_iter()
        .filter(|entry| entry["kind"] == "Opcode")
        .map(|entry| {
            (
                entry["name"].as_str().expect("opcode name").to_string(),
                entry["source"].as_str().expect("opcode source").to_string(),
            )
        })
        .filter(|(name, _)| !name.starts_with("TierI::"))
        .collect::<Vec<_>>();
    assert_eq!(opcodes, [("Moved".to_string(), definition.to_string())]);
}

#[test]
fn pentry_inventory_generator_rejects_lowered_registry_without_the_definition_import() {
    let fixture = Fixture::new("");
    let module = opcode_module();
    let path = module.join("::");
    let parent = module[..module.len() - 1].join("::");
    let expected = format!(
        "{}: registry site does not import the Op defined in {}; expected `use {path}::Op;`",
        fixture.registry_path().display(),
        opcode_site().0.name()
    );
    for registry in [
        "world_registry!(Op);\n".to_string(),
        format!("world_registry!(Op);\npub use {parent}::Op;\n"),
        format!("world_registry!(Op);\npub use {path}::*;\n"),
        format!("world_registry!(Op);\npub use {path}::Other as Op;\n"),
        format!("world_registry!(Op);\n#[cfg(test)]\npub use {path}::Op;\n"),
    ] {
        fixture.replace_registry(&registry);
        assert_eq!(
            fixture
                .generate()
                .expect_err("a marker must name the enum whose variants it classifies"),
            expected,
            "registry site input: {registry:?}"
        );
    }
    fixture.replace_registry(&format!("world_registry!(Op);\npub use {path}::{{Op}};\n"));
    fixture
        .generate()
        .expect("one import of the defining enum must generate successfully");
}

#[test]
fn pentry_inventory_generator_leaves_lowered_registry_without_marker_unclassified() {
    let fixture = Fixture::new("");
    fixture.replace_registry(&format!("pub use {}::Op;\n", opcode_module().join("::")));
    assert_eq!(
        fixture
            .generate()
            .expect_err("an unmarked lowered registry must fail the source scanner"),
        format!(
            "primitive inventory: missing authority classification: Fixture ({})",
            opcode_site().0.name()
        )
    );
    fixture.replace_registry(&registry_input());
    fixture
        .generate()
        .expect("one registry-site marker must generate successfully");
}

#[test]
fn pentry_inventory_generator_rejects_world_marker_at_lowered_definition_site() {
    let fixture = Fixture::new("");
    fixture.replace_definition("pub enum Op { Fixture }\nworld_registry!(Op);\n");
    assert_eq!(
        fixture
            .generate()
            .expect_err("a lowered definition cannot carry the emacs_core marker"),
        format!(
            "{}: world_registry!(Op) belongs at its registry site {}",
            fixture.definition_path().display(),
            opcode_site().1
        )
    );
}

#[test]
fn pentry_inventory_generator_accepts_unrelated_cfg_select_expression() {
    let fixture = Fixture::new("fn flags() { let _ = std::cfg_select! { unix => 1, _ => 2 }; }");
    fixture
        .generate()
        .expect("nonregistration cfg_select expressions are outside the inventory grammar");
}

#[test]
fn architecture_pentry_inventory_source_matches_cold_sidecar() {
    let manifest = neomacs_infra::crate_root!();
    let project = manifest.join("../..");
    // Build.rs deliberately retains its narrow original inputs: this scan is
    // explicit test/tool work, never an input to every incremental core build.
    let build_script = fs::read_to_string(manifest.join("build.rs")).expect("core build script");
    assert!(
        !build_script.contains("build_support/pentry_inventory.rs"),
        "the full-source inventory scanner belongs in the architecture gate, not build.rs"
    );
    let evidence = project.join("tmp/pentry/architecture-inventory");
    fs::create_dir_all(&evidence).expect("architecture artifact root");
    let output = tempfile::Builder::new()
        .prefix("catalog-")
        .tempdir_in(evidence)
        .expect("independent architecture artifact directory")
        .keep();
    generator::generate(&manifest, &project, &output)
        .unwrap_or_else(|error| panic!("primitive inventory architecture gate: {error}"));
    let generated = fs::read(output.join("pentry_inventory.rs")).expect("fresh source inventory");
    let sidecar = fs::read(manifest.join("src/emacs_core/lisp/native/subr/effect_inventory.inc"))
        .expect("checked-in cold inventory");
    assert!(
        generated == sidecar,
        "stale cold inventory: run cargo xtask pentry-inventory <OUTPUT-DIR>, then copy pentry_inventory.rs to crates/neovm-core/src/emacs_core/lisp/native/subr/effect_inventory.inc; fresh artifacts: {}",
        output.display()
    );
    eprintln!("pentry architecture artifacts: {}", output.display());
}
