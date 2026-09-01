use flate2::read::GzDecoder;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Instant;

type DynError = Box<dyn Error>;
type Result<T> = std::result::Result<T, DynError>;

const FINGERPRINT_MAGIC_START: &[u8; 16] = b"NEOMACS-FP-START";
const FINGERPRINT_MAGIC_END: &[u8; 16] = b"NEOMACS-FP-END!!";
const FINGERPRINT_PLACEHOLDER: &[u8; 32] = b"NEOMACS_PDUMP_FINGERPRINT_SLOT!!";
const FINGERPRINT_RECORD_LEN: usize =
    FINGERPRINT_MAGIC_START.len() + FINGERPRINT_PLACEHOLDER.len() + FINGERPRINT_MAGIC_END.len();

#[derive(Debug, Clone)]
struct FreshBuildOptions {
    repo_root: PathBuf,
    runtime_root: PathBuf,
    bin_dir: PathBuf,
    /// Cargo profile to build with. Determines both the `cargo build` flag and
    /// the `target/<dir>` the binaries come from, so a non-release profile does
    /// NOT overwrite the release build.
    profile: BuildProfile,
    dry_run: bool,
    native_comp: bool,
    skip_build: bool,
    no_byte_compile: bool,
    features: Vec<String>,
    /// R2-B1: enable the in-neomacs dump-time AOT preload producer. xtask sets
    /// `NEOVM_AOT_PRELOAD=1` on the `--temacs=pdump` step so the producer (which
    /// lives in neovm-core, runs inside `dump-emacs-portable`) emits
    /// `libneomacs-preload.so` + manifest beside the pdump, then xtask verifies
    /// they landed. With `--dry-run` the producer only LISTS candidates + dedup
    /// stats (no link/write). xtask itself does not link neovm-core.
    aot_preload: bool,
}

#[derive(Debug, Clone)]
struct PipelinePaths {
    temacs: PathBuf,
    bootstrap: PathBuf,
    final_bin: PathBuf,
    etc_root: PathBuf,
    lisp_root: PathBuf,
    leim_root: PathBuf,
    admin_charsets_root: PathBuf,
    admin_grammars_root: PathBuf,
    admin_unidata_root: PathBuf,
    makefile_in: PathBuf,
}

#[derive(Debug)]
struct ExecutableFingerprintImage {
    path: PathBuf,
    normalized: Vec<u8>,
    slots: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SemanticGrammarKind {
    Bovine,
    Wisent,
}

#[derive(Debug, Clone, Copy)]
struct SemanticGrammarTarget {
    kind: SemanticGrammarKind,
    source_rel: &'static str,
    output_rel: &'static str,
    grammar_rel: &'static str,
}

const SEMANTIC_GRAMMAR_TARGETS: &[SemanticGrammarTarget] = &[
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Bovine,
        source_rel: "c.by",
        output_rel: "cedet/semantic/bovine/c-by.el",
        grammar_rel: "cedet/semantic/bovine/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Bovine,
        source_rel: "make.by",
        output_rel: "cedet/semantic/bovine/make-by.el",
        grammar_rel: "cedet/semantic/bovine/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Bovine,
        source_rel: "scheme.by",
        output_rel: "cedet/semantic/bovine/scm-by.el",
        grammar_rel: "cedet/semantic/bovine/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Wisent,
        source_rel: "grammar.wy",
        output_rel: "cedet/semantic/grammar-wy.el",
        grammar_rel: "cedet/semantic/wisent/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Wisent,
        source_rel: "java-tags.wy",
        output_rel: "cedet/semantic/wisent/javat-wy.el",
        grammar_rel: "cedet/semantic/wisent/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Wisent,
        source_rel: "js.wy",
        output_rel: "cedet/semantic/wisent/js-wy.el",
        grammar_rel: "cedet/semantic/wisent/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Wisent,
        source_rel: "python.wy",
        output_rel: "cedet/semantic/wisent/python-wy.el",
        grammar_rel: "cedet/semantic/wisent/grammar.el",
    },
    SemanticGrammarTarget {
        kind: SemanticGrammarKind::Wisent,
        source_rel: "srecode-template.wy",
        output_rel: "cedet/srecode/srt-wy.el",
        grammar_rel: "cedet/semantic/wisent/grammar.el",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeimGenerationKind {
    TitDic,
    MiscDic,
    Pinyin,
}

#[derive(Debug, Clone, Copy)]
struct LeimGenerationRule {
    kind: LeimGenerationKind,
    source_rel: &'static str,
    output_rels: &'static [&'static str],
}

#[derive(Debug)]
struct LeimGenerationJob {
    source: PathBuf,
    args: Vec<OsString>,
}

#[derive(Debug)]
struct GeneratedLispJob {
    name: String,
    args: Vec<OsString>,
}

const LEIM_GENERATION_RULES: &[LeimGenerationRule] = &[
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/CCDOSPY.tit",
        output_rels: &["leim/quail/CCDOSPY.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/Punct.tit",
        output_rels: &["leim/quail/Punct.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/QJ.tit",
        output_rels: &["leim/quail/QJ.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/SW.tit",
        output_rels: &["leim/quail/SW.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/TONEPY.tit",
        output_rels: &["leim/quail/TONEPY.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/4Corner.tit",
        output_rels: &["leim/quail/4Corner.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/ARRAY30.tit",
        output_rels: &["leim/quail/ARRAY30.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/ECDICT.tit",
        output_rels: &["leim/quail/ECDICT.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/ETZY.tit",
        output_rels: &["leim/quail/ETZY.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/Punct-b5.tit",
        output_rels: &["leim/quail/Punct-b5.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/PY-b5.tit",
        output_rels: &["leim/quail/PY-b5.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/QJ-b5.tit",
        output_rels: &["leim/quail/QJ-b5.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::TitDic,
        source_rel: "CXTERM-DIC/ZOZY.tit",
        output_rels: &["leim/quail/ZOZY.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::MiscDic,
        source_rel: "MISC-DIC/cangjie-table.b5",
        output_rels: &["leim/quail/tsang-b5.el", "leim/quail/quick-b5.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::MiscDic,
        source_rel: "MISC-DIC/cangjie-table.cns",
        output_rels: &["leim/quail/tsang-cns.el", "leim/quail/quick-cns.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::MiscDic,
        source_rel: "MISC-DIC/pinyin.map",
        output_rels: &["leim/quail/PY.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::MiscDic,
        source_rel: "MISC-DIC/ziranma.cin",
        output_rels: &["leim/quail/ZIRANMA.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::MiscDic,
        source_rel: "MISC-DIC/CTLau.html",
        output_rels: &["leim/quail/CTLau.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::MiscDic,
        source_rel: "MISC-DIC/CTLau-b5.html",
        output_rels: &["leim/quail/CTLau-b5.el"],
    },
    LeimGenerationRule {
        kind: LeimGenerationKind::Pinyin,
        source_rel: "MISC-DIC/pinyin.map",
        output_rels: &["language/pinyin.el"],
    },
];

fn main() {
    if let Err(err) = try_main() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let repo_root = repository_root();
    run_xtask(repo_root, env::args_os().skip(1))
}

fn run_xtask(repo_root: PathBuf, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let mut args = args.into_iter().peekable();
    if matches!(args.peek().and_then(|arg| arg.to_str()), Some("perf")) {
        args.next();
        neomacs_perf::run_cli(&repo_root, args)?;
        return Ok(());
    }
    let options = FreshBuildOptions::parse(repo_root, args)?;
    run_fresh_build(&options)
}

impl FreshBuildOptions {
    fn parse(
        repo_root: PathBuf,
        args: impl IntoIterator<Item = OsString>,
    ) -> Result<FreshBuildOptions> {
        let mut args = args.into_iter().peekable();

        if matches!(args.peek(), Some(arg) if arg == "help" || arg == "--help" || arg == "-h") {
            print_usage();
            std::process::exit(0);
        }

        if matches!(args.peek(), Some(arg) if arg == "fresh-build") {
            args.next();
        }

        let mut runtime_root = repo_root.clone();
        let mut bin_dir = None;
        let mut profile: Option<BuildProfile> = None;
        let mut dry_run = false;
        let mut native_comp =
            env::var("NEOMACS_NATIVE_COMP").is_ok_and(|value| value.eq_ignore_ascii_case("yes"));
        let mut skip_build = false;
        let mut no_byte_compile = false;
        let mut features: Vec<String> = Vec::new();
        let mut aot_preload = false;

        while let Some(arg) = args.next() {
            match arg.to_string_lossy().as_ref() {
                "--bin-dir" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--bin-dir requires a path".to_string())?;
                    bin_dir = Some(resolve_cli_path(&repo_root, value));
                }
                "--runtime-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--runtime-root requires a path".to_string())?;
                    runtime_root = resolve_cli_path(&repo_root, value);
                }
                "--release" => profile = Some(BuildProfile::Release),
                "--profile" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--profile requires a profile name".to_string())?;
                    profile = Some(BuildProfile::parse(value.to_string_lossy().trim()));
                }
                "--dry-run" => dry_run = true,
                "--native-comp" => native_comp = true,
                "--no-native-comp" => native_comp = false,
                "--skip-build" => skip_build = true,
                "--no-byte-compile" => no_byte_compile = true,
                "--aot-preload" => aot_preload = true,
                "--features" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--features requires a comma-separated list".to_string())?;
                    features = value
                        .to_string_lossy()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("unknown option: {other}\n\n{}", usage_text()).into());
                }
            }
        }

        let profile = match profile {
            Some(profile) => profile,
            None => {
                return Err(
                    "fresh-build must be used with --release or --profile NAME.\n\n\
                 fresh-build builds the runnable GNU-shaped runtime pipeline \
                 (cargo build, temacs bootstrap, byte-compilation, pdump) and is an \
                 optimized-build operation. Re-run with:\n    cargo xtask fresh-build --release\n\
                 or, for a symbol-preserving profiling build that leaves target/release \
                 alone:\n    cargo xtask fresh-build --profile profiling"
                        .to_string()
                        .into(),
                );
            }
        };
        // The pipeline byte-compiles the whole Lisp tree with the binary it
        // just built; an unoptimized profile makes that take hours, which is
        // what the original --release requirement existed to prevent.
        if !profile.is_optimized() {
            return Err(format!(
                "fresh-build cannot use the `{}` profile: the pipeline \
                 byte-compiles the Lisp tree with the binary it builds, so it \
                 needs an optimized profile (`release`, or `profiling` which \
                 inherits it).",
                profile.as_name()
            )
            .into());
        }

        let bin_dir = bin_dir.unwrap_or_else(|| default_bin_dir(&repo_root, &profile));

        Ok(FreshBuildOptions {
            repo_root,
            runtime_root,
            bin_dir,
            profile,
            dry_run,
            native_comp,
            skip_build,
            no_byte_compile,
            features,
            aot_preload,
        })
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

fn default_bin_dir(repo_root: &Path, profile: &BuildProfile) -> PathBuf {
    env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        })
        .unwrap_or_else(|| repo_root.join("target"))
        .join(profile.target_subdir())
}

fn resolve_cli_path(repo_root: &Path, raw: OsString) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

/// A cargo profile `fresh-build` was asked to build with.
///
/// Cargo profiles are user-extensible, so this is deliberately NOT a closed
/// set: `Custom` carries anything defined in Cargo.toml that xtask has no
/// opinion about. What the enum buys is that the profiles xtask *does* reason
/// about are named values rather than string comparisons -- in particular the
/// PGO classification below, where getting `ReleasePgoGen` on the wrong side of
/// the test would make the instrumented pass recurse into itself.
#[derive(Clone, Debug, PartialEq, Eq, strum::IntoStaticStr, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
enum BuildProfile {
    Release,
    Profiling,
    /// Profile-guided shipping build.
    ReleasePgo,
    /// Profile-guided build that keeps symbols, for profiling what is shipped.
    ReleasePgoProfiling,
    /// The instrumented pass that PRODUCES a profile. Never a PGO consumer.
    ReleasePgoGen,
    /// Unoptimized; rejected, since the pipeline byte-compiles the Lisp tree
    /// with the binary it just built.
    Dev,
    Debug,
    Test,
    #[strum(disabled)]
    Custom(String),
}

impl BuildProfile {
    fn parse(name: &str) -> Self {
        use std::str::FromStr;
        Self::from_str(name).unwrap_or_else(|_| Self::Custom(name.to_string()))
    }

    /// The name to hand cargo, and the `target/<dir>` it writes to.
    fn as_name(&self) -> &str {
        match self {
            Self::Custom(name) => name,
            other => other.into(),
        }
    }

    /// Whether this profile CONSUMES a PGO profile, so `fresh-build` must run
    /// the instrument-train-merge passes first.
    ///
    /// `ReleasePgoGen` is excluded by being a distinct variant rather than by a
    /// string inequality, so the recursion is impossible rather than guarded.
    fn consumes_pgo(&self) -> bool {
        matches!(self, Self::ReleasePgo | Self::ReleasePgoProfiling)
    }

    /// Optimized enough to byte-compile the Lisp tree with.
    fn is_optimized(&self) -> bool {
        !matches!(self, Self::Dev | Self::Debug | Self::Test)
    }

    /// Directory name cargo writes this profile's artifacts into.
    ///
    /// Cargo does not simply use the profile name: `dev` and `test` share
    /// `target/debug`, and `bench` shares `target/release`.
    fn target_subdir(&self) -> &str {
        match self {
            Self::Dev | Self::Test => "debug",
            Self::Custom(name) if name == "bench" => "release",
            other => other.as_name(),
        }
    }
}

/// Where a PGO build keeps its raw counters and merged profile.
fn pgo_dirs(options: &FreshBuildOptions) -> (PathBuf, PathBuf) {
    let base = options.repo_root.join("target").join("pgo");
    (base.join("raw"), base.join("merged.profdata"))
}

/// Build the runtime once with instrumentation, run the committed training
/// workload against it, and merge the counters into a profile.
///
/// Split out because PGO is inherently two-pass: the profile has to come from
/// a binary that already exists, so `--profile release-pgo` runs this first and
/// then rebuilds normally with `-Cprofile-use`.
fn run_pgo_training(options: &FreshBuildOptions) -> Result<PathBuf> {
    let (raw_dir, merged) = pgo_dirs(options);
    let train = options
        .repo_root
        .join("crates")
        .join("xtask")
        .join("pgo-train.el");
    if !train.exists() {
        return Err(format!("missing PGO training workload: {}", train.display()).into());
    }

    // Instrumented pass: its own profile (and so its own target dir), so the
    // instrumented binary never displaces a normal build.
    let gen_options = FreshBuildOptions {
        profile: BuildProfile::ReleasePgoGen,
        ..options.clone()
    };
    let gen_options = FreshBuildOptions {
        bin_dir: default_bin_dir(&gen_options.repo_root, &gen_options.profile),
        ..gen_options
    };
    if !options.dry_run {
        let _ = std::fs::remove_dir_all(&raw_dir);
        std::fs::create_dir_all(&raw_dir)?;
        // Same stale-artifact hazard as the optimized pass below: the
        // instrumented build's RUSTFLAGS is also constant across runs.
        let stale = default_bin_dir(&gen_options.repo_root, &gen_options.profile);
        if stale.exists() {
            std::fs::remove_dir_all(&stale)?;
        }
    }
    println!("+ PGO pass 1/2: instrumented build");
    run_fresh_build_inner(
        &gen_options,
        &[(
            OsString::from("RUSTFLAGS"),
            OsString::from(format!("-Cprofile-generate={}", raw_dir.display())),
        )],
    )?;

    println!("+ PGO: running training workload {}", train.display());
    let gen_paths = pipeline_paths(&gen_options);
    run_command(
        options,
        &options.repo_root,
        &gen_paths.final_bin,
        &[
            OsString::from("--batch"),
            OsString::from("-Q"),
            OsString::from("-l"),
            train.as_os_str().to_os_string(),
        ],
        &[(
            OsString::from("NEOMACS_RUNTIME_ROOT"),
            options.runtime_root.as_os_str().to_os_string(),
        )],
    )?;

    println!("+ PGO: merging counters -> {}", merged.display());
    let profdata = llvm_profdata_path()?;
    let mut merge_args = vec![OsString::from("merge"), OsString::from("-o")];
    merge_args.push(merged.as_os_str().to_os_string());
    merge_args.push(raw_dir.as_os_str().to_os_string());
    run_command(options, &options.repo_root, &profdata, &merge_args, &[])?;
    Ok(merged)
}

/// `llvm-profdata` from the active toolchain -- it must match rustc's LLVM, so
/// a system copy is not a safe substitute.
fn llvm_profdata_path() -> Result<PathBuf> {
    let sysroot = Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()?;
    let sysroot = String::from_utf8(sysroot.stdout)?.trim().to_string();
    let path =
        PathBuf::from(&sysroot).join("lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata");
    if path.exists() {
        return Ok(path);
    }
    Err(format!(
        "llvm-profdata not found at {}.\n\nInstall it with:\n    rustup component add llvm-tools",
        path.display()
    )
    .into())
}

fn run_fresh_build(options: &FreshBuildOptions) -> Result<()> {
    if options.profile.consumes_pgo() {
        let merged = run_pgo_training(options)?;
        // Cargo cannot see that the profile CONTENTS changed: RUSTFLAGS is
        // byte-identical between runs (`-Cprofile-use=<same path>`), so its
        // fingerprint matches and it happily reuses objects compiled against
        // the PREVIOUS profile. With LTO on, mixing them fails at link time
        // with "failed to load bitcode of module ...". Dropping the output
        // directory is the honest fix -- a changed profile invalidates every
        // object anyway, so there is nothing to salvage.
        if !options.dry_run {
            let stale = default_bin_dir(&options.repo_root, &options.profile);
            if stale.exists() {
                std::fs::remove_dir_all(&stale)?;
            }
        }
        println!("+ PGO pass 2/2: optimized build using {}", merged.display());
        return run_fresh_build_inner(
            options,
            &[(
                OsString::from("RUSTFLAGS"),
                OsString::from(format!("-Cprofile-use={}", merged.display())),
            )],
        );
    }
    run_fresh_build_inner(options, &[])
}

fn run_fresh_build_inner(
    options: &FreshBuildOptions,
    build_envs: &[(OsString, OsString)],
) -> Result<()> {
    let paths = pipeline_paths(options);
    ensure_runtime_inputs(&paths)?;

    if !options.skip_build {
        let cargo_args = initial_cargo_build_args(options);
        run_command(
            options,
            &options.repo_root,
            &cargo_program(),
            &cargo_args,
            build_envs,
        )?;
    }

    patch_primary_executable_fingerprint(options, &paths)?;
    copy_executable_role_images(options, &paths)?;

    // macOS: re-sign all role binaries after patching.  Patching the pdump
    // fingerprint modifies the executable image in-place, which invalidates
    // the code signature.  Without a fresh ad-hoc signature the kernel sends
    // SIGKILL when the binary is executed (exit status: signal 9).
    #[cfg(target_os = "macos")]
    {
        for bin in [&paths.temacs, &paths.bootstrap, &paths.final_bin] {
            if bin.exists() {
                let status = std::process::Command::new("codesign")
                    .args(["--force", "--sign", "-", bin.to_str().unwrap()])
                    .status()?;
                if !status.success() {
                    return Err(format!("codesign failed on {}", bin.display()).into());
                }
            }
        }
    }

    if !options.dry_run {
        ensure_binaries_exist(&paths)?;
    }

    let envs = [(
        OsString::from("NEOMACS_RUNTIME_ROOT"),
        options.runtime_root.as_os_str().to_os_string(),
    )];
    let loaddefs_el = paths.lisp_root.join("loaddefs.el");
    let theme_loaddefs_el = paths.lisp_root.join("theme-loaddefs.el");
    let ldefs_boot = paths.lisp_root.join("ldefs-boot.el");

    // GNU's bootstrap-clean removes Lisp bytecode before building
    // bootstrap-emacs.  Keep primary loaddefs sources available for
    // pbootstrap/COMPILE_FIRST; GNU removes loaddefs.el later, in
    // autoloads-force, immediately before regenerating it.
    if !options.no_byte_compile {
        remove_stale_lisp_bytecode(options, &paths)?;
    }
    remove_stale_generated_leim_sources(options, &paths)?;
    remove_stale_generated_custom_finder_sources(options, &paths)?;
    remove_stale_generated_unidata_sources(options, &paths)?;
    // GNU admin/grammars/Makefile.in has an intentionally empty
    // bootstrap-clean (with a comment "IMO this should run gen-clean"),
    // so stale grammar outputs can survive across rebuilds and cause
    // hard-to-diagnose bootstrap failures when the engine binary changes.
    remove_stale_semantic_grammar_outputs(options, &paths)?;
    // GNU src/Makefile.in makes temacs depend on the generated charset
    // translation tables plus the AWK-generated Unicode helpers needed by
    // early loadup.  These are source artifacts in lisp/international/, but
    // they are intentionally not tracked in Git.
    run_early_international_generation(options, &paths)?;
    // GNU src/Makefile.in runs `make -C ../lisp update-subdirs` before
    // bootstrap-emacs is dumped, so the bootstrap load path sees current
    // generated subdirectory metadata.
    run_update_subdirs(options, &paths)?;

    run_command(
        options,
        &options.repo_root,
        &paths.temacs,
        &[
            OsString::from("--batch"),
            OsString::from("-l"),
            OsString::from("loadup"),
            OsString::from("--temacs=pbootstrap"),
        ],
        &envs,
    )?;

    if !options.no_byte_compile {
        // ---------------------------------------------------------------
        // COMPILE_FIRST: byte-compile the compiler infrastructure.
        //
        // GNU lisp/Makefile.in compiles COMPILE_FIRST files ONE AT A TIME,
        // each in a SEPARATE emacs process, in the listed order:
        //   macroexp.elc → cconv.elc → byte-opt.elc → bytecomp.elc
        //   → loaddefs-gen.elc → radix-tree.elc
        //
        // This ordering is critical: each file is compiled with a compiler
        // that already has the previously-compiled .elc files loaded,
        // making each successive compilation faster.  The comment in GNU's
        // Makefile explains: "They're ordered by size, so we use the
        // slowest-compiler on the smallest file and move to larger files
        // as the compiler gets faster."
        //
        // This MUST run before loaddefs generation, because
        // loaddefs-generate--emacs-batch loads bytecomp.el which loads
        // byte-opt.el.  Without compiled .elc files, the pcase macro
        // expansion in byte-opt.el runs as interpreted elisp and hangs.
        // ---------------------------------------------------------------
        let compile_first_sources =
            parse_compile_first_sources(&paths.makefile_in, &paths.lisp_root, options.native_comp)?;
        let compile_first_sources: Vec<PathBuf> = compile_first_sources
            .into_iter()
            .filter(|source| options.dry_run || compile_first_needs_rebuild(source))
            .collect();
        // Compile one file at a time, each in its own bootstrap-neomacs
        // process.  This matches GNU's make suffix rule which runs
        // `$(emacs) -f batch-byte-compile $<` per file.  Each process
        // picks up the .elc files from previous compilations.
        for source in &compile_first_sources {
            let compile_args = compile_first_args_for_source(options.native_comp, source);
            run_command(
                options,
                &options.repo_root,
                &paths.bootstrap,
                &compile_args,
                &envs,
            )?;
        }
    }

    // GNU src/Makefile.in generates the full Unicode data set with
    // bootstrap-emacs before final dumping and before Lisp files such as
    // ucs-normalize.el are byte-compiled.
    run_unidata_lisp_generation(options, &paths, &envs)?;

    // GNU lisp/Makefile.in makes both autoloads and compile-main depend on
    // gen-lisp.  This generates Lisp sources that are intentionally not
    // checked into the Neomacs tree, such as leim-list.el and CEDET parser
    // tables, before autoload scanning and byte compilation see the tree.
    run_gen_lisp(options, &paths, &envs)?;

    // ---------------------------------------------------------------
    // Loaddefs generation: uses the now-compiled .elc files.
    //
    // This mirrors GNU lisp/Makefile.in's `autoloads-force` target:
    // bootstrap-neomacs loads loaddefs-gen.elc and runs loaddefs-generate
    // with GENERATE-FULL non-nil.  The same call writes lisp/loaddefs.el,
    // lisp/theme-loaddefs.el, and secondary loaddefs such as
    // org/org-loaddefs.el and dired-loaddefs.el.
    // ---------------------------------------------------------------
    // GNU lisp/Makefile.in guarantees loaddefs-gen.elc exists here by make
    // dependency ($(lisp)/loaddefs.el depends on $(LOADDEFS_GEN), which the
    // compile rules build first).  A --no-byte-compile Neomacs pipeline
    // deliberately drops that guarantee, so on a pristine source-only tree
    // the .elc is absent; fall back to loading loaddefs-gen.el from source,
    // which produces the identical generated loaddefs set (only the scrape
    // itself runs slower).
    let loaddefs_gen = {
        let compiled = paths.lisp_root.join("emacs-lisp/loaddefs-gen.elc");
        if compiled.is_file() {
            compiled
        } else {
            paths.lisp_root.join("emacs-lisp/loaddefs-gen.el")
        }
    };
    let loaddefs_dirs = loaddefs_dirs(&paths.lisp_root)?;
    let loaddefs_args = loaddefs_generation_args(&loaddefs_gen, &loaddefs_dirs);
    remove_primary_loaddefs_for_regeneration(options, &paths, &loaddefs_el, &theme_loaddefs_el)?;
    // Remove secondary loaddefs from previous builds at the same phase as the
    // full regeneration, so stale generated files cannot influence the new set.
    remove_stale_secondary_loaddefs(options, &paths)?;
    run_command(
        options,
        &options.repo_root,
        &paths.bootstrap,
        &loaddefs_args,
        &envs,
    )?;

    print_synthetic_step(&format!(
        "generate {} from {}",
        ldefs_boot.display(),
        loaddefs_el.display()
    ));
    if !options.dry_run {
        validate_primary_loaddefs(&loaddefs_el)?;
        write_ldefs_boot(&loaddefs_el, &ldefs_boot)?;
    }

    // GNU lisp/Makefile.in's top-level `all' target explicitly includes
    // cus-load.el and finder-inf.el because ordinary dependencies do not
    // request them.  They are independent targets, and both generated files
    // mark themselves no-byte-compile, so run them together before the final
    // dump sees the completed generated-source set.
    run_custom_finder_generation(options, &paths, &envs)?;

    // GNU `make install` copies the binary last, so its timestamp is newer
    // than every .el file — `byte-compile-refresh-preloaded` never reloads
    // anything.  Loaddefs generation above wrote generated .el files (e.g.
    // ldefs-boot.el, theme-loaddefs.el) that are now newer than the pdump
    // created by the earlier pbootstrap step.  Touch the pdump to match
    // GNU's invariant: the dump is always the newest file.
    //
    // Without this, bootstrap-neomacs in the preloaded compile step would
    // detect those generated files as "stale" and reload them as source,
    // wasting ~200ms per file on the 3-pass load (full-file read + UTF-8
    // decode + eager macroexpand).
    touch_bootstrap_pdump(options, &paths.bootstrap)?;

    // GNU src/Makefile.in generates src/lisp.mk from loadup.el, then makes
    // the final emacs target depend on that preloaded Lisp set.  That means
    // loadup's libraries are byte-compiled by bootstrap-emacs before the final
    // pdump, while the broad lisp/compile-main pass still runs later.
    if !options.no_byte_compile {
        run_preloaded_lisp_byte_compile(options, &paths, &envs)?;
    }

    // R2-B1 dry-run gate (resolution B): when `--aot-preload --dry-run`, run the
    // dump FOR REAL with `NEOVM_AOT_PRELOAD_DRY_RUN=1` so the in-neomacs producer
    // LOGS its candidates + dedup stats (skipping link/write), then stop. This is
    // the only way to observe the producer enumeration — it needs a live neomacs
    // process — so this combination intentionally executes the dump even under
    // `--dry-run` (logged explicitly below so it is not a silent contract break).
    if options.aot_preload && options.dry_run {
        run_aot_preload_dry_run_gate(options, &paths, &envs)?;
        return Ok(());
    }

    print_synthetic_step("dump final Emacs executable (GNU temacs --temacs=pdump)");
    // R2-B1: dump-time AOT preload (resolution B). The preload `.so` is built
    // INSIDE the neomacs dump process — `dump-emacs-portable`, after the pdump is
    // written, gated by `NEOVM_AOT_PRELOAD`. That process owns the patched
    // fingerprint slot + the live obarray (the #A eq-identity source), so the
    // emitted `.so`'s content-hashes + the manifest fingerprint match the runtime
    // by construction (xtask cannot satisfy the pdump fingerprint check itself).
    // xtask only sets the env here + verifies the artifacts afterward.
    let pdump_envs = aot_preload_dump_envs(options, &envs);
    run_command(
        options,
        &options.repo_root,
        &paths.temacs,
        &[
            OsString::from("--batch"),
            OsString::from("-l"),
            OsString::from("loadup"),
            OsString::from("--temacs=pdump"),
        ],
        &pdump_envs,
    )?;

    // Verify the in-neomacs producer actually emitted the artifacts (real run).
    if options.aot_preload {
        verify_aot_preload_artifacts(&paths)?;
    }

    // GNU top-level Makefile.in makes `lisp' depend on `src', so the general
    // lisp/compile-main pass uses the final dumped src/emacs, not
    // bootstrap-emacs.  This matters for Unicode property users such as
    // sgml-mode and char-fold: charprop.el is generated after pbootstrap, then
    // loaded into the final pdump before the broad Lisp byte-compile pass.
    if !options.no_byte_compile {
        run_compile_main(options, &paths, &envs)?;
    }

    let mode = options.profile.as_name();
    println!(
        concat!(
            "+ xtask fresh-build finished successfully ({mode})\n",
            "  bin      = {bin}\n",
            "  runtime  = {rt}\n",
            "  repo     = {repo}\n",
            "  options  = skip_build={sb} no_byte_compile={nbc}",
        ),
        mode = mode,
        bin = options.bin_dir.display(),
        rt = options.runtime_root.display(),
        repo = options.repo_root.display(),
        sb = options.skip_build,
        nbc = options.no_byte_compile,
    );
    Ok(())
}

// R2-B1: dump-time AOT preload (resolution B). The producer lives ENTIRELY in
// neovm-core and runs INSIDE the neomacs `dump-emacs-portable` builtin (after the
// pdump is written), gated by the env vars below. That process owns the patched
// pdump fingerprint slot + the live obarray, so the emitted `.so` matches the
// runtime by construction; xtask only sets the env + verifies the artifacts.
// (xtask deliberately does NOT link neovm-core — it cannot satisfy the pdump
// fingerprint check, which is why the in-xtask-load approach was abandoned.)

/// Enable the in-neomacs preload producer for the dump process.
const AOT_PRELOAD_ENV: &str = "NEOVM_AOT_PRELOAD";
/// Make the in-neomacs producer LOG candidates + stats without linking/writing.
const AOT_PRELOAD_DRY_RUN_ENV: &str = "NEOVM_AOT_PRELOAD_DRY_RUN";
/// Artifact names the in-neomacs producer writes beside the pdump (must match
/// `aot::PRELOAD_SO_NAME` / `aot::PRELOAD_MANIFEST_NAME` in neovm-core).
const PRELOAD_SO_NAME: &str = "libneomacs-preload.so";
const PRELOAD_MANIFEST_NAME: &str = "libneomacs-preload.manifest";

/// Build the env for the `--temacs=pdump` step: the base `envs` plus
/// `NEOVM_AOT_PRELOAD=1` when `--aot-preload` (real run). The dry-run gate uses a
/// separate path ([`run_aot_preload_dry_run_gate`]), so this only ever sets the
/// real-build flag.
fn aot_preload_dump_envs(
    options: &FreshBuildOptions,
    base: &[(OsString, OsString)],
) -> Vec<(OsString, OsString)> {
    let mut envs = base.to_vec();
    if options.aot_preload {
        envs.push((OsString::from(AOT_PRELOAD_ENV), OsString::from("1")));
    }
    envs
}

/// R2-B1 dry-run gate: run the `--temacs=pdump` dump FOR REAL with the preload
/// producer in DRY-RUN mode, so the in-neomacs builtin logs its AOT candidates +
/// dedup stats without linking/writing. This intentionally executes the dump even
/// under `--dry-run` (the only way to observe the producer enumeration needs a
/// live neomacs process); it is logged explicitly so it is not a silent break of
/// the global `--dry-run` "print, don't run" contract.
fn run_aot_preload_dry_run_gate(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    base_envs: &[(OsString, OsString)],
) -> Result<()> {
    print_synthetic_step(
        "AOT preload DRY-RUN gate: run --temacs=pdump with the in-neomacs producer in \
         dry-run mode (logs candidates + dedup stats, no link/write)",
    );
    println!(
        "  NOTE  --aot-preload --dry-run executes the dump for real (with \
         {AOT_PRELOAD_DRY_RUN_ENV}=1) so the producer can enumerate candidates"
    );
    let mut envs = base_envs.to_vec();
    envs.push((OsString::from(AOT_PRELOAD_ENV), OsString::from("1")));
    envs.push((OsString::from(AOT_PRELOAD_DRY_RUN_ENV), OsString::from("1")));
    // Run directly (NOT through run_command, which would skip under --dry-run).
    print_command(
        paths.temacs.as_os_str(),
        &[
            OsString::from("--batch"),
            OsString::from("-l"),
            OsString::from("loadup"),
            OsString::from("--temacs=pdump"),
        ],
    );
    if !paths.temacs.exists() {
        return Err(format!(
            "aot-preload dry-run: missing {} (run a full build first, or drop --skip-build)",
            paths.temacs.display()
        )
        .into());
    }
    let mut command = Command::new(&paths.temacs);
    command
        .current_dir(&options.repo_root)
        .args([
            OsStr::new("--batch"),
            OsStr::new("-l"),
            OsStr::new("loadup"),
            OsStr::new("--temacs=pdump"),
        ])
        .envs(envs.iter().map(|(k, v)| (k, v)));
    remove_build_time_emacs_env(&mut command);
    let status = command.status()?;
    if !status.success() {
        return Err(format!(
            "aot-preload dry-run: --temacs=pdump exited with {}",
            status
                .code()
                .map_or_else(|| "signal".to_string(), |c| c.to_string())
        )
        .into());
    }
    Ok(())
}

/// Verify the in-neomacs producer emitted the preload artifacts beside the final
/// pdump (real `--aot-preload` run). The pdump lives in `bin_dir` next to
/// `neomacs`, so the `.so` + manifest land there too.
fn verify_aot_preload_artifacts(paths: &PipelinePaths) -> Result<()> {
    print_synthetic_step("AOT preload: verify libneomacs-preload.so + manifest");
    let dir = paths
        .final_bin
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let so = dir.join(PRELOAD_SO_NAME);
    let manifest = dir.join(PRELOAD_MANIFEST_NAME);
    for artifact in [&so, &manifest] {
        if !artifact.exists() {
            return Err(format!(
                "aot-preload: expected artifact not found: {} (did the in-neomacs producer run? \
                 it is gated by {AOT_PRELOAD_ENV}=1 on the --temacs=pdump dump)",
                artifact.display()
            )
            .into());
        }
    }
    println!("  OK    {}", so.display());
    println!("  OK    {}", manifest.display());
    Ok(())
}

fn initial_cargo_build_args(options: &FreshBuildOptions) -> Vec<OsString> {
    let mut cargo_args = vec![
        OsString::from("build"),
        OsString::from("--verbose"),
        OsString::from("-p"),
        OsString::from("neomacs"),
    ];
    if !options.features.is_empty() {
        cargo_args.push(OsString::from("--features"));
        cargo_args.push(OsString::from(options.features.join(",")));
    }
    // `--profile release` is accepted by cargo and is equivalent to
    // `--release`, so one uniform flag covers every profile.
    cargo_args.push(OsString::from("--profile"));
    cargo_args.push(OsString::from(options.profile.as_name()));
    cargo_args
}

fn loaddefs_generation_args(loaddefs_gen: &Path, loaddefs_dirs: &[PathBuf]) -> Vec<OsString> {
    let mut loaddefs_args = vec![
        OsString::from("--batch"),
        OsString::from("-l"),
        loaddefs_gen.as_os_str().to_os_string(),
        OsString::from("-f"),
        OsString::from("loaddefs-generate--emacs-batch"),
    ];
    loaddefs_args.extend(
        loaddefs_dirs
            .iter()
            .map(|path| path.as_os_str().to_os_string()),
    );
    loaddefs_args
}

fn run_early_international_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    run_charset_translation_generation(options, paths)?;
    run_unidata_awk_generation(options, paths)?;
    Ok(())
}

fn run_charset_translation_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let mut generated = 0usize;

    let cp51932_output = paths.lisp_root.join("international/cp51932.el");
    let cp51932_script = paths.admin_charsets_root.join("cp51932.awk");
    let cp932_map = paths.etc_root.join("charsets/CP932-2BYTE.map");
    let cp51932_deps = vec![cp51932_script.clone(), cp932_map.clone()];
    ensure_generation_input(&cp51932_script)?;
    ensure_generation_input(&cp932_map)?;
    if generated_file_needs_rebuild(&cp51932_output, &cp51932_deps) {
        if generated == 0 {
            print_synthetic_step("generate charset translation Lisp (GNU src/admin charsets)");
        }
        run_awk_stdin_to_output(options, &cp51932_script, &cp932_map, &cp51932_output)?;
        generated += 1;
    }

    let eucjp_output = paths.lisp_root.join("international/eucjp-ms.el");
    let eucjp_script = paths.admin_charsets_root.join("eucjp-ms.awk");
    let eucjp_charmap = paths.admin_charsets_root.join("glibc/EUC-JP-MS.gz");
    let eucjp_deps = vec![eucjp_script.clone(), eucjp_charmap.clone()];
    ensure_generation_input(&eucjp_script)?;
    ensure_generation_input(&eucjp_charmap)?;
    if generated_file_needs_rebuild(&eucjp_output, &eucjp_deps) {
        if generated == 0 {
            print_synthetic_step("generate charset translation Lisp (GNU src/admin charsets)");
        }
        run_gunzip_awk_to_output(options, &eucjp_script, &eucjp_charmap, &eucjp_output)?;
    }

    Ok(())
}

fn run_unidata_awk_generation(options: &FreshBuildOptions, paths: &PipelinePaths) -> Result<()> {
    let mut generated = 0usize;

    let charscript_output = paths.lisp_root.join("international/charscript.el");
    let blocks_script = paths.admin_unidata_root.join("blocks.awk");
    let blocks_txt = paths.admin_unidata_root.join("Blocks.txt");
    let emoji_data = paths.admin_unidata_root.join("emoji-data.txt");
    let charscript_deps = vec![
        blocks_script.clone(),
        blocks_txt.clone(),
        emoji_data.clone(),
    ];
    ensure_generation_inputs(&charscript_deps)?;
    if generated_file_needs_rebuild(&charscript_output, &charscript_deps) {
        if generated == 0 {
            print_synthetic_step("generate Unicode AWK Lisp helpers (GNU src/admin unidata)");
        }
        run_awk_files_to_output(
            options,
            &blocks_script,
            &[blocks_txt, emoji_data],
            &charscript_output,
        )?;
        generated += 1;
    }

    let emoji_zwj_output = paths.lisp_root.join("international/emoji-zwj.el");
    let emoji_zwj_script = paths.admin_unidata_root.join("emoji-zwj.awk");
    let emoji_zwj_sequences = paths.admin_unidata_root.join("emoji-zwj-sequences.txt");
    let emoji_sequences = paths.admin_unidata_root.join("emoji-sequences.txt");
    let emoji_zwj_deps = vec![
        emoji_zwj_script.clone(),
        emoji_zwj_sequences.clone(),
        emoji_sequences.clone(),
    ];
    ensure_generation_inputs(&emoji_zwj_deps)?;
    if generated_file_needs_rebuild(&emoji_zwj_output, &emoji_zwj_deps) {
        if generated == 0 {
            print_synthetic_step("generate Unicode AWK Lisp helpers (GNU src/admin unidata)");
        }
        run_awk_files_to_output(
            options,
            &emoji_zwj_script,
            &[emoji_zwj_sequences, emoji_sequences],
            &emoji_zwj_output,
        )?;
    }

    Ok(())
}

fn run_unidata_lisp_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    let unidata_gen = paths.admin_unidata_root.join("unidata-gen.el");
    ensure_generation_input(&unidata_gen)?;

    let unifiles = unidata_generated_lisp_files(paths)?;
    let unidata_txt = paths.admin_unidata_root.join("unidata.txt");
    let unifile_deps = vec![
        unidata_gen.clone(),
        paths.admin_unidata_root.join("UnicodeData.txt"),
        paths.admin_unidata_root.join("BidiMirroring.txt"),
        paths.admin_unidata_root.join("BidiBrackets.txt"),
        unidata_txt.clone(),
    ];
    ensure_generation_inputs(&unifile_deps[..unifile_deps.len() - 1])?;

    let unifiles_need_rebuild = unifiles
        .iter()
        .any(|output| generated_file_needs_rebuild(output, &unifile_deps));
    let charprop_output = paths.lisp_root.join("international/charprop.el");
    let mut charprop_deps = Vec::with_capacity(unifiles.len() + 1);
    charprop_deps.push(unidata_gen.clone());
    charprop_deps.extend(unifiles.iter().cloned());
    let charprop_needs_rebuild =
        unifiles_need_rebuild || generated_file_needs_rebuild(&charprop_output, &charprop_deps);

    let extra_jobs = unidata_extra_lisp_jobs(paths, &unidata_gen)?;
    let extra_needs_rebuild = extra_jobs
        .iter()
        .any(|job| generated_file_needs_rebuild(&job.output, &job.dependencies));

    if !unifiles_need_rebuild && !charprop_needs_rebuild && !extra_needs_rebuild {
        return Ok(());
    }

    print_synthetic_step("generate Unicode Lisp data (GNU src/admin unidata)");
    run_unidata_txt_generation(options, paths)?;
    if bytecode_needs_rebuild(&unidata_gen) {
        let args = vec![
            OsString::from("--batch"),
            OsString::from("--no-site-file"),
            OsString::from("--no-site-lisp"),
            OsString::from("-f"),
            OsString::from("batch-byte-compile"),
            unidata_gen.as_os_str().to_os_string(),
        ];
        run_command(options, &options.repo_root, &paths.bootstrap, &args, envs)?;
    }

    if unifiles_need_rebuild {
        let unifile_jobs =
            unidata_gen_file_jobs(options, paths, &unifiles, &unifile_deps, &unidata_txt)?;
        if !unifile_jobs.is_empty() {
            let jobs = compile_main_jobs();
            println!(
                "  INFO  generating {} Unicode property .el files with {jobs} parallel jobs",
                unifile_jobs.len(),
            );
            let errors = run_generated_lisp_jobs(options, paths, envs, unifile_jobs)?;
            if !errors.is_empty() {
                eprintln!(
                    "  ERROR  {} Unicode property job{} failed:",
                    errors.len(),
                    if errors.len() == 1 { "" } else { "s" }
                );
                for error in &errors {
                    eprintln!("    - {error}");
                }
                return Err(generated_lisp_failure_summary(&errors).into());
            }
        }
    }

    if charprop_needs_rebuild {
        run_unidata_generator_function(
            options,
            paths,
            envs,
            "unidata-gen-charprop",
            &charprop_output,
            &[],
        )?;
    }

    let extra_jobs = unidata_extra_jobs_to_run(options, extra_jobs)?;
    if !extra_jobs.is_empty() {
        let jobs = compile_main_jobs();
        println!(
            "  INFO  generating {} extra Unicode .el files with {jobs} parallel jobs",
            extra_jobs.len(),
        );
        let errors = run_generated_lisp_jobs(options, paths, envs, extra_jobs)?;
        if !errors.is_empty() {
            eprintln!(
                "  ERROR  {} extra Unicode job{} failed:",
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            );
            for error in &errors {
                eprintln!("    - {error}");
            }
            return Err(generated_lisp_failure_summary(&errors).into());
        }
    }

    Ok(())
}

fn unidata_gen_file_jobs(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    unifiles: &[PathBuf],
    dependencies: &[PathBuf],
    unidata_txt: &Path,
) -> Result<Vec<GeneratedLispJob>> {
    let mut jobs = Vec::new();
    for output in unifiles {
        if !generated_file_needs_rebuild(output, dependencies) {
            continue;
        }
        ensure_output_parent(options, output)?;
        make_output_writable(options, output)?;
        jobs.push(GeneratedLispJob {
            name: format!("{} (GNU unidata-gen-file)", output.display()),
            args: unidata_gen_file_args(paths, output, unidata_txt),
        });
    }
    Ok(jobs)
}

fn unidata_extra_jobs_to_run(
    options: &FreshBuildOptions,
    jobs: Vec<UnidataExtraJob>,
) -> Result<Vec<GeneratedLispJob>> {
    let mut jobs_to_run = Vec::new();
    for job in jobs {
        if !generated_file_needs_rebuild(&job.output, &job.dependencies) {
            continue;
        }
        ensure_output_parent(options, &job.output)?;
        make_output_writable(options, &job.output)?;
        jobs_to_run.push(GeneratedLispJob {
            name: format!("{} (GNU unidata)", job.output.display()),
            args: job.args,
        });
    }
    Ok(jobs_to_run)
}

#[derive(Debug)]
struct UnidataExtraJob {
    output: PathBuf,
    dependencies: Vec<PathBuf>,
    args: Vec<OsString>,
}

fn unidata_extra_lisp_jobs(
    paths: &PipelinePaths,
    unidata_gen: &Path,
) -> Result<Vec<UnidataExtraJob>> {
    let output = |name: &str| paths.lisp_root.join("international").join(name);
    let admin = |name: &str| paths.admin_unidata_root.join(name);
    let unidata_gen_os = unidata_gen.as_os_str().to_os_string();
    let admin_dir_os = paths.admin_unidata_root.as_os_str().to_os_string();

    let specs = [
        (
            "emoji-labels.el",
            vec![
                paths.lisp_root.join("international/emoji.el"),
                admin("emoji-test.txt"),
            ],
            vec![
                OsString::from("--batch"),
                OsString::from("--no-site-file"),
                OsString::from("--no-site-lisp"),
                OsString::from("-l"),
                OsString::from("emoji.el"),
                OsString::from("-f"),
                OsString::from("emoji--generate-file"),
            ],
        ),
        (
            "uni-scripts.el",
            vec![
                unidata_gen.to_path_buf(),
                admin("Scripts.txt"),
                admin("ScriptExtensions.txt"),
                admin("PropertyValueAliases.txt"),
            ],
            unidata_generator_args(&admin_dir_os, &unidata_gen_os, "unidata-gen-scripts"),
        ),
        (
            "uni-confusable.el",
            vec![unidata_gen.to_path_buf(), admin("confusables.txt")],
            unidata_generator_args(&admin_dir_os, &unidata_gen_os, "unidata-gen-confusable"),
        ),
        (
            "idna-mapping.el",
            vec![unidata_gen.to_path_buf(), admin("IdnaMappingTable.txt")],
            unidata_generator_args(&admin_dir_os, &unidata_gen_os, "unidata-gen-idna-mapping"),
        ),
    ];

    specs
        .into_iter()
        .map(|(name, dependencies, mut args)| {
            ensure_generation_inputs(&dependencies)?;
            let output = output(name);
            args.push(output.as_os_str().to_os_string());
            Ok(UnidataExtraJob {
                output,
                dependencies,
                args,
            })
        })
        .collect()
}

fn unidata_generator_args(
    admin_dir: &OsString,
    unidata_gen: &OsString,
    function: &str,
) -> Vec<OsString> {
    vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("-L"),
        admin_dir.clone(),
        OsString::from("-l"),
        unidata_gen.clone(),
        OsString::from("-f"),
        OsString::from(function),
    ]
}

fn unidata_gen_file_args(
    paths: &PipelinePaths,
    output: &Path,
    unidata_txt: &Path,
) -> Vec<OsString> {
    let mut args = unidata_generator_args(
        &paths.admin_unidata_root.as_os_str().to_os_string(),
        &paths
            .admin_unidata_root
            .join("unidata-gen.el")
            .as_os_str()
            .to_os_string(),
        "unidata-gen-file",
    );
    args.push(output.as_os_str().to_os_string());
    args.push(paths.admin_unidata_root.as_os_str().to_os_string());
    args.push(unidata_txt.as_os_str().to_os_string());
    args
}

fn run_unidata_generator_function(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    function: &str,
    output: &Path,
    extra_args: &[OsString],
) -> Result<()> {
    ensure_output_parent(options, output)?;
    make_output_writable(options, output)?;
    let mut args = unidata_generator_args(
        &paths.admin_unidata_root.as_os_str().to_os_string(),
        &paths
            .admin_unidata_root
            .join("unidata-gen.el")
            .as_os_str()
            .to_os_string(),
        function,
    );
    args.push(output.as_os_str().to_os_string());
    args.extend(extra_args.iter().cloned());
    run_command(options, &options.repo_root, &paths.bootstrap, &args, envs)
}

fn run_unidata_txt_generation(options: &FreshBuildOptions, paths: &PipelinePaths) -> Result<()> {
    let unicode_data = paths.admin_unidata_root.join("UnicodeData.txt");
    let output = paths.admin_unidata_root.join("unidata.txt");
    ensure_generation_input(&unicode_data)?;
    let deps = vec![unicode_data.clone()];
    if !generated_file_needs_rebuild(&output, &deps) {
        return Ok(());
    }
    ensure_output_parent(options, &output)?;
    make_output_writable(options, &output)?;
    run_sed_unicode_data_to_output(options, &unicode_data, &output)
}

fn unidata_generated_lisp_files(paths: &PipelinePaths) -> Result<Vec<PathBuf>> {
    let contents = fs::read_to_string(paths.admin_unidata_root.join("unidata-gen.el"))?;
    Ok(unidata_generated_lisp_file_names_from_str(&contents)
        .into_iter()
        .map(|name| paths.lisp_root.join("international").join(name))
        .collect())
}

fn unidata_generated_lisp_file_names_from_str(contents: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for line in contents.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("(\"uni-") else {
            continue;
        };
        let Some((name_rest, _)) = rest.split_once('"') else {
            continue;
        };
        let name = format!("uni-{name_rest}");
        if name.ends_with(".el") && seen.insert(name.clone()) {
            out.push(name);
        }
    }
    out.sort();
    out
}

fn run_gen_lisp(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    run_leim_generation(options, paths, envs)?;
    run_semantic_grammar_generation(options, paths, envs)?;
    Ok(())
}

fn run_custom_finder_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    let mut jobs = Vec::new();
    if let Some(job) = custom_dependencies_generation_job(paths)? {
        jobs.push(job);
    }
    if let Some(job) = finder_data_generation_job(paths)? {
        jobs.push(job);
    }

    if jobs.is_empty() {
        return Ok(());
    }

    print_synthetic_step("generate custom/finder data (GNU lisp all)");
    println!(
        "  INFO  generating {} independent Lisp data target{} with {} parallel job{}",
        jobs.len(),
        if jobs.len() == 1 { "" } else { "s" },
        jobs.len(),
        if jobs.len() == 1 { "" } else { "s" }
    );
    let errors = run_generated_lisp_jobs(options, paths, envs, jobs)?;
    if !errors.is_empty() {
        eprintln!(
            "  ERROR  {} generated Lisp job{} failed:",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        );
        for error in &errors {
            eprintln!("    - {error}");
        }
        return Err(generated_lisp_failure_summary(&errors).into());
    }

    Ok(())
}

fn custom_dependencies_generation_job(paths: &PipelinePaths) -> Result<Option<GeneratedLispJob>> {
    let output = paths.lisp_root.join("cus-load.el");
    let dirs = lisp_dirs_for_custom_dependencies(&paths.lisp_root)?;
    let mut dependencies = dirs.clone();
    dependencies.push(paths.lisp_root.join("cus-dep.el"));
    if !generated_file_needs_rebuild(&output, &dependencies) {
        return Ok(None);
    }

    let args = custom_dependencies_generation_args(&paths.lisp_root, &output, &dirs);
    Ok(Some(GeneratedLispJob {
        name: "lisp/cus-load.el (GNU custom-deps)".to_string(),
        args,
    }))
}

fn finder_data_generation_job(paths: &PipelinePaths) -> Result<Option<GeneratedLispJob>> {
    let output = paths.lisp_root.join("finder-inf.el");
    let dirs = lisp_dirs_for_finder_data(&paths.lisp_root)?;
    let mut dependencies = dirs.clone();
    dependencies.push(paths.lisp_root.join("finder.el"));
    if !generated_file_needs_rebuild(&output, &dependencies) {
        return Ok(None);
    }

    let args = finder_data_generation_args(&paths.lisp_root, &output, &dirs);
    Ok(Some(GeneratedLispJob {
        name: "lisp/finder-inf.el (GNU finder-data)".to_string(),
        args,
    }))
}

fn run_generated_lisp_jobs(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    jobs: Vec<GeneratedLispJob>,
) -> Result<Vec<String>> {
    if options.dry_run {
        return Ok(jobs
            .iter()
            .filter_map(|job| {
                run_command(
                    options,
                    &options.repo_root,
                    &paths.bootstrap,
                    &job.args,
                    envs,
                )
                .err()
                .map(|err| format!("{} ({err})", job.name))
            })
            .collect());
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.len().min(compile_main_jobs()).max(1))
        .build()?;
    Ok(pool.install(|| {
        jobs.par_iter()
            .filter_map(|job| {
                run_command(
                    options,
                    &options.repo_root,
                    &paths.bootstrap,
                    &job.args,
                    envs,
                )
                .err()
                .map(|err| format!("{} ({err})", job.name))
            })
            .collect()
    }))
}

fn generated_lisp_failure_summary(errors: &[String]) -> String {
    format!(
        "generated Lisp data failed for {} target{}",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" }
    )
}

fn custom_dependencies_generation_args(
    lisp_root: &Path,
    output: &Path,
    dirs: &[PathBuf],
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("-l"),
        OsString::from("cus-dep"),
        OsString::from("--eval"),
        OsString::from(format!(
            "(setq generated-custom-dependencies-file (unmsys--file-name {}))",
            elisp_string_literal(output)
        )),
        OsString::from("-f"),
        OsString::from("custom-make-dependencies"),
    ];
    args.extend(
        dirs.iter()
            .filter(|dir| dir.starts_with(lisp_root))
            .map(|dir| dir.as_os_str().to_os_string()),
    );
    args
}

fn finder_data_generation_args(lisp_root: &Path, output: &Path, dirs: &[PathBuf]) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("-l"),
        OsString::from("finder"),
        OsString::from("--eval"),
        OsString::from(format!(
            "(setq generated-finder-keywords-file (unmsys--file-name {}))",
            elisp_string_literal(output)
        )),
        OsString::from("-f"),
        OsString::from("finder-compile-keywords-make-dist"),
    ];
    args.extend(
        dirs.iter()
            .filter(|dir| dir.starts_with(lisp_root))
            .map(|dir| dir.as_os_str().to_os_string()),
    );
    args
}

fn run_leim_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    let titdic_cnv = paths.lisp_root.join("international/titdic-cnv.el");
    if compile_main_needs_rebuild(&titdic_cnv) {
        print_synthetic_step("compile leim generator (GNU gen-lisp leim)");
        run_bootstrap_byte_compile_source(options, paths, envs, &titdic_cnv)?;
    }

    let quail_dir = paths.lisp_root.join("leim/quail");
    if !options.dry_run {
        fs::create_dir_all(&quail_dir)?;
    }

    let mut generation_jobs = Vec::new();
    for rule in LEIM_GENERATION_RULES {
        let source = paths.leim_root.join(rule.source_rel);
        ensure_generation_input(&source)?;
        let outputs = rule
            .output_rels
            .iter()
            .map(|rel| paths.lisp_root.join(rel))
            .collect::<Vec<_>>();
        if !generated_outputs_need_rebuild(&outputs, std::slice::from_ref(&source)) {
            continue;
        }

        for output in &outputs {
            ensure_output_parent(options, output)?;
        }
        let args = leim_generation_args(rule.kind, &quail_dir, &source, &outputs[0]);
        generation_jobs.push(LeimGenerationJob { source, args });
    }

    if !generation_jobs.is_empty() {
        print_synthetic_step("generate leim sources (GNU gen-lisp leim)");
        let jobs = compile_main_jobs();
        println!(
            "  INFO  generating {} LEIM source rule{} with {jobs} parallel jobs",
            generation_jobs.len(),
            if generation_jobs.len() == 1 { "" } else { "s" }
        );
        let errors = run_leim_generation_jobs(options, paths, envs, generation_jobs, jobs)?;
        if !errors.is_empty() {
            eprintln!(
                "  ERROR  {} LEIM generation job{} failed:",
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            );
            for error in &errors {
                eprintln!("    - {error}");
            }
            return Err(leim_generation_failure_summary(&errors).into());
        }
    }

    run_leim_list_generation(options, paths, envs)?;
    Ok(())
}

fn run_leim_generation_jobs(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    jobs_to_run: Vec<LeimGenerationJob>,
    jobs: usize,
) -> Result<Vec<String>> {
    if options.dry_run {
        return Ok(jobs_to_run
            .iter()
            .filter_map(|job| {
                run_command(
                    options,
                    &options.repo_root,
                    &paths.bootstrap,
                    &job.args,
                    envs,
                )
                .err()
                .map(|err| format!("{} ({err})", job.source.display()))
            })
            .collect());
    }

    let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs).build()?;
    Ok(pool.install(|| {
        jobs_to_run
            .par_iter()
            .filter_map(|job| {
                run_command(
                    options,
                    &options.repo_root,
                    &paths.bootstrap,
                    &job.args,
                    envs,
                )
                .err()
                .map(|err| format!("{} ({err})", job.source.display()))
            })
            .collect()
    }))
}

fn leim_generation_failure_summary(errors: &[String]) -> String {
    format!(
        "LEIM generation failed for {} source rule{}",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" }
    )
}

fn run_leim_list_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    let leim_dir = paths.lisp_root.join("leim");
    let leim_ext = paths.leim_root.join("leim-ext.el");
    ensure_generation_input(&leim_ext)?;

    let output = leim_dir.join("leim-list.el");
    let mut dependencies = leim_generated_output_paths(paths);
    dependencies.push(leim_ext.clone());
    if !generated_file_needs_rebuild(&output, &dependencies) {
        return Ok(());
    }

    print_synthetic_step("generate lisp/leim/leim-list.el (GNU gen-lisp leim)");
    if !options.dry_run {
        ensure_output_parent(options, &output)?;
        let _ = remove_file_if_exists(&output)?;
    }

    let args = leim_list_generation_args(&leim_dir);
    run_command(options, &options.repo_root, &paths.bootstrap, &args, envs)?;
    if !options.dry_run {
        append_leim_ext(&output, &leim_ext)?;
    }
    Ok(())
}

fn leim_generated_output_paths(paths: &PipelinePaths) -> Vec<PathBuf> {
    LEIM_GENERATION_RULES
        .iter()
        .flat_map(|rule| {
            rule.output_rels
                .iter()
                .map(|rel| paths.lisp_root.join(rel))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn leim_generation_args(
    kind: LeimGenerationKind,
    quail_dir: &Path,
    source: &Path,
    output: &Path,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("-l"),
        OsString::from("titdic-cnv"),
        OsString::from("-f"),
    ];
    match kind {
        LeimGenerationKind::TitDic => {
            args.push(OsString::from("batch-tit-dic-convert"));
            args.push(OsString::from("-dir"));
            args.push(quail_dir.as_os_str().to_os_string());
            args.push(source.as_os_str().to_os_string());
        }
        LeimGenerationKind::MiscDic => {
            args.push(OsString::from("batch-tit-miscdic-convert"));
            args.push(OsString::from("-dir"));
            args.push(quail_dir.as_os_str().to_os_string());
            args.push(source.as_os_str().to_os_string());
        }
        LeimGenerationKind::Pinyin => {
            args.push(OsString::from("tit-pinyin-convert"));
            args.push(source.as_os_str().to_os_string());
            args.push(output.as_os_str().to_os_string());
        }
    }
    args
}

fn leim_list_generation_args(leim_dir: &Path) -> Vec<OsString> {
    vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("-l"),
        OsString::from("international/quail"),
        OsString::from("--eval"),
        OsString::from(format!(
            "(update-leim-list-file (unmsys--file-name {}))",
            elisp_string_literal(leim_dir)
        )),
    ]
}

fn append_leim_ext(output: &Path, leim_ext: &Path) -> Result<()> {
    let contents = fs::read_to_string(leim_ext)?;
    let append = leim_ext_append_contents(&contents);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(output)?;
    file.write_all(append.as_bytes())?;
    Ok(())
}

fn leim_ext_append_contents(contents: &str) -> String {
    let mut output = String::new();
    for line in contents.lines() {
        if !line.starts_with(';') {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        let mut chars = line.chars();
        if chars.next() != Some(';') {
            continue;
        }
        let semicolons = chars.by_ref().take_while(|ch| *ch == ';').count();
        let rest = &line[1 + semicolons..];
        if let Some(payload) = rest.strip_prefix("inc ") {
            output.push(';');
            for _ in 0..semicolons {
                output.push(';');
            }
            output.push(' ');
            output.push_str(payload);
            output.push('\n');
        }
    }
    output
}

fn run_semantic_grammar_generation(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    let mut generated = 0usize;
    for target in semantic_grammar_targets(paths) {
        ensure_generation_input(&target.source)?;
        ensure_generation_input(&target.grammar)?;
        if !generated_file_needs_rebuild(&target.output, &[target.source.clone(), target.grammar]) {
            continue;
        }

        if generated == 0 {
            print_synthetic_step("generate semantic grammars (GNU gen-lisp semantic)");
        }
        ensure_output_parent(options, &target.output)?;
        make_output_writable(options, &target.output)?;
        let args = semantic_grammar_args(target.kind, &target.output, &target.source);
        run_command(options, &options.repo_root, &paths.bootstrap, &args, envs)?;
        generated += 1;
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SemanticGrammarJob {
    kind: SemanticGrammarKind,
    source: PathBuf,
    output: PathBuf,
    grammar: PathBuf,
}

fn semantic_grammar_targets(paths: &PipelinePaths) -> Vec<SemanticGrammarJob> {
    SEMANTIC_GRAMMAR_TARGETS
        .iter()
        .map(|target| SemanticGrammarJob {
            kind: target.kind,
            source: paths.admin_grammars_root.join(target.source_rel),
            output: paths.lisp_root.join(target.output_rel),
            grammar: paths.lisp_root.join(target.grammar_rel),
        })
        .collect()
}

fn semantic_grammar_args(kind: SemanticGrammarKind, output: &Path, source: &Path) -> Vec<OsString> {
    let (library, function) = match kind {
        SemanticGrammarKind::Bovine => ("semantic/bovine/grammar", "bovine-batch-make-parser"),
        SemanticGrammarKind::Wisent => ("semantic/wisent/grammar", "wisent-batch-make-parser"),
    };

    vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("--eval"),
        OsString::from("(setq load-prefer-newer t)"),
        // Force-load cl-extra so `cl-find-class` is defined. The grammar generator
        // runs on the BOOTSTRAP neomacs, which predates the loaddefs regen that
        // provides cl-find-class's autoload — so it would otherwise error
        // `void-function: cl-find-class`. (GNU's admin/grammars/Makefile sidesteps
        // this by running the generator with the FULLY-BUILT emacs, whose complete
        // loaddefs autoload it; the bootstrap lacks those, so we load it directly.)
        OsString::from("-l"),
        OsString::from("cl-extra"),
        OsString::from("-l"),
        OsString::from(library),
        OsString::from("-f"),
        OsString::from(function),
        OsString::from("-o"),
        output.as_os_str().to_os_string(),
        source.as_os_str().to_os_string(),
    ]
}

fn generated_outputs_need_rebuild(outputs: &[PathBuf], dependencies: &[PathBuf]) -> bool {
    outputs
        .iter()
        .any(|output| generated_file_needs_rebuild(output, dependencies))
}

fn generated_file_needs_rebuild(output: &Path, dependencies: &[PathBuf]) -> bool {
    let Ok(output_meta) = fs::metadata(output) else {
        return true;
    };
    let Ok(output_mtime) = output_meta.modified() else {
        return true;
    };
    dependencies.iter().any(|dependency| {
        fs::metadata(dependency)
            .and_then(|metadata| metadata.modified())
            .map_or(true, |dependency_mtime| dependency_mtime > output_mtime)
    })
}

fn ensure_generation_input(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(format!("missing generated-source input: {}", path.display()).into());
    }
    Ok(())
}

fn ensure_generation_inputs(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        ensure_generation_input(path)?;
    }
    Ok(())
}

fn ensure_output_parent(options: &FreshBuildOptions, output: &Path) -> Result<()> {
    let Some(parent) = output.parent() else {
        return Ok(());
    };
    if options.dry_run {
        return Ok(());
    }
    fs::create_dir_all(parent)?;
    Ok(())
}

fn make_output_writable(options: &FreshBuildOptions, output: &Path) -> Result<()> {
    if options.dry_run || !output.exists() {
        return Ok(());
    }
    let mut permissions = fs::metadata(output)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    permissions.set_readonly(false);
    fs::set_permissions(output, permissions)?;
    Ok(())
}

fn run_awk_stdin_to_output(
    options: &FreshBuildOptions,
    script: &Path,
    input: &Path,
    output: &Path,
) -> Result<()> {
    ensure_output_parent(options, output)?;
    make_output_writable(options, output)?;
    let awk = tool_program("awk");
    let args = vec![OsString::from("-f"), script.as_os_str().to_os_string()];
    print_redirected_command(awk.as_os_str(), &args, Some(input), output);
    if options.dry_run {
        return Ok(());
    }

    let input_file = fs::File::open(input)?;
    let output_file = fs::File::create(output)?;
    let status = Command::new(&awk)
        .args(args.iter().map(OsString::as_os_str))
        .stdin(input_file)
        .stdout(output_file)
        .status()?;
    if !status.success() {
        return Err(redirected_command_failure(&awk, &args, Some(input), output, status).into());
    }
    Ok(())
}

fn run_awk_files_to_output(
    options: &FreshBuildOptions,
    script: &Path,
    inputs: &[PathBuf],
    output: &Path,
) -> Result<()> {
    ensure_output_parent(options, output)?;
    make_output_writable(options, output)?;
    let awk = tool_program("awk");
    let mut args = vec![OsString::from("-f"), script.as_os_str().to_os_string()];
    args.extend(inputs.iter().map(|input| input.as_os_str().to_os_string()));
    print_redirected_command(awk.as_os_str(), &args, None, output);
    if options.dry_run {
        return Ok(());
    }

    let output_file = fs::File::create(output)?;
    let status = Command::new(&awk)
        .args(args.iter().map(OsString::as_os_str))
        .stdout(output_file)
        .status()?;
    if !status.success() {
        return Err(redirected_command_failure(&awk, &args, None, output, status).into());
    }
    Ok(())
}

fn run_gunzip_awk_to_output(
    options: &FreshBuildOptions,
    script: &Path,
    input: &Path,
    output: &Path,
) -> Result<()> {
    ensure_output_parent(options, output)?;
    make_output_writable(options, output)?;
    let awk = tool_program("awk");
    print_gzip_decompress_command(input);
    let awk_args = vec![OsString::from("-f"), script.as_os_str().to_os_string()];
    print_redirected_command(awk.as_os_str(), &awk_args, None, output);
    if options.dry_run {
        return Ok(());
    }

    let decoded = read_gzip_file(input)?;

    let output_file = fs::File::create(output)?;
    let mut child = Command::new(&awk)
        .args(awk_args.iter().map(OsString::as_os_str))
        .stdin(Stdio::piped())
        .stdout(output_file)
        .spawn()?;
    child
        .stdin
        .take()
        .expect("awk child stdin should be piped")
        .write_all(&decoded)?;
    let status = child.wait()?;
    if !status.success() {
        return Err(redirected_command_failure(&awk, &awk_args, None, output, status).into());
    }
    Ok(())
}

fn read_gzip_file(path: &Path) -> Result<Vec<u8>> {
    let mut decoded = Vec::new();
    GzDecoder::new(fs::File::open(path)?).read_to_end(&mut decoded)?;
    Ok(decoded)
}

fn run_sed_unicode_data_to_output(
    options: &FreshBuildOptions,
    input: &Path,
    output: &Path,
) -> Result<()> {
    let sed = tool_program("sed");
    let args = vec![
        OsString::from("-e"),
        OsString::from(r#"s/\([^;]*\);\(.*\)/(#x\1 "\2")/"#),
        OsString::from("-e"),
        OsString::from(r#"s/;/" "/g"#),
    ];
    print_redirected_command(sed.as_os_str(), &args, Some(input), output);
    if options.dry_run {
        return Ok(());
    }

    let input_file = fs::File::open(input)?;
    let output_file = fs::File::create(output)?;
    let status = Command::new(&sed)
        .args(args.iter().map(OsString::as_os_str))
        .stdin(input_file)
        .stdout(output_file)
        .status()?;
    if !status.success() {
        return Err(redirected_command_failure(&sed, &args, Some(input), output, status).into());
    }
    Ok(())
}

fn elisp_string_literal(path: &Path) -> String {
    let mut output = String::from("\"");
    for ch in path.to_string_lossy().chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            _ => output.push(ch),
        }
    }
    output.push('"');
    output
}

fn remove_stale_lisp_bytecode(options: &FreshBuildOptions, paths: &PipelinePaths) -> Result<()> {
    let files = generated_lisp_bytecode_files(&paths.lisp_root)?;
    if files.is_empty() {
        return Ok(());
    }

    print_synthetic_step("remove stale Lisp bytecode");
    if options.dry_run {
        println!(
            "  would remove {} .elc files under {}",
            files.len(),
            paths.lisp_root.display()
        );
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &files {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale .elc files");
    Ok(())
}

fn generated_lisp_bytecode_files(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_lisp_bytecode_files(lisp_root, &mut files)?;
    files.sort();
    Ok(files)
}

fn generated_leim_source_files(paths: &PipelinePaths) -> Vec<PathBuf> {
    let mut files = leim_generated_output_paths(paths);
    files.push(paths.lisp_root.join("leim/leim-list.el"));
    files.sort();
    files.dedup();
    files
}

fn remove_stale_generated_leim_sources(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let files = generated_leim_source_files(paths);
    let existing = files
        .into_iter()
        .filter(|path| options.dry_run || path.exists())
        .collect::<Vec<_>>();
    if existing.is_empty() {
        return Ok(());
    }

    print_synthetic_step("remove stale generated LEIM sources");
    if options.dry_run {
        for file in &existing {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &existing {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale generated LEIM source files");
    Ok(())
}

fn generated_custom_finder_source_files(paths: &PipelinePaths) -> Vec<PathBuf> {
    vec![
        paths.lisp_root.join("cus-load.el"),
        paths.lisp_root.join("finder-inf.el"),
    ]
}

fn remove_stale_generated_custom_finder_sources(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let files = generated_custom_finder_source_files(paths)
        .into_iter()
        .filter(|path| options.dry_run || path.exists())
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(());
    }

    print_synthetic_step("remove stale generated custom/finder sources");
    if options.dry_run {
        for file in &files {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &files {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale generated custom/finder source files");
    Ok(())
}

fn remove_stale_semantic_grammar_outputs(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let files: Vec<PathBuf> = semantic_grammar_targets(paths)
        .into_iter()
        .map(|target| target.output)
        .filter(|path| options.dry_run || path.exists())
        .collect();
    if files.is_empty() {
        return Ok(());
    }

    print_synthetic_step("remove stale semantic grammar outputs");
    if options.dry_run {
        for file in &files {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &files {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale semantic grammar outputs");
    Ok(())
}

fn generated_unidata_source_files(paths: &PipelinePaths) -> Result<Vec<PathBuf>> {
    let mut files = vec![
        paths.lisp_root.join("international/charscript.el"),
        paths.lisp_root.join("international/emoji-zwj.el"),
        paths.lisp_root.join("international/charprop.el"),
        paths.lisp_root.join("international/emoji-labels.el"),
        paths.lisp_root.join("international/idna-mapping.el"),
        paths.lisp_root.join("international/uni-confusable.el"),
        paths.lisp_root.join("international/uni-scripts.el"),
    ];
    files.extend(unidata_generated_lisp_files(paths)?);
    files.sort();
    files.dedup();
    Ok(files)
}

fn generated_unidata_admin_files(paths: &PipelinePaths) -> Vec<PathBuf> {
    vec![
        paths.admin_unidata_root.join("unidata.txt"),
        paths.admin_unidata_root.join("unidata-gen.elc"),
        paths.admin_unidata_root.join("uvs.elc"),
    ]
}

fn remove_stale_generated_unidata_sources(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let mut files = generated_unidata_source_files(paths)?;
    files.extend(generated_unidata_admin_files(paths));
    let files = files
        .into_iter()
        .filter(|path| options.dry_run || path.exists())
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(());
    }

    print_synthetic_step("remove stale generated Unicode data sources");
    if options.dry_run {
        for file in &files {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &files {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale generated Unicode data source files");
    Ok(())
}

fn collect_lisp_bytecode_files(current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_lisp_bytecode_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "elc") {
            out.push(path);
        }
    }

    Ok(())
}

fn remove_stale_secondary_loaddefs(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let files = generated_secondary_loaddefs_files(&paths.lisp_root)?;
    if files.is_empty() {
        return Ok(());
    }

    print_synthetic_step("remove stale secondary loaddefs");
    if options.dry_run {
        for file in &files {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &files {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
        if remove_file_if_exists(&file.with_extension("elc"))? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale secondary loaddefs artifacts");
    Ok(())
}

fn remove_lisp_bytecode_without_source(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    let files = generated_lisp_bytecode_files(&paths.lisp_root)?
        .into_iter()
        .filter(|file| !file.with_extension("el").is_file())
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(());
    }

    print_synthetic_step("compile-main clean stale Lisp bytecode");
    if options.dry_run {
        for file in &files {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    let mut removed = 0usize;
    for file in &files {
        if remove_file_if_exists(file)? {
            removed += 1;
        }
    }
    println!("  INFO  removed {removed} stale compile-main .elc files");
    Ok(())
}

/// Touch the pdump file so its timestamp is newer than all generated
/// .el files.  This matches GNU `make install` semantics: the binary
/// (dump) is always the last artifact, so `byte-compile-refresh-preloaded`
/// never finds stale files and never reloads source during compilation.
fn touch_bootstrap_pdump(options: &FreshBuildOptions, bootstrap_bin: &Path) -> Result<()> {
    let bin_name = bootstrap_bin
        .file_name()
        .unwrap_or(bootstrap_bin.as_os_str())
        .to_string_lossy();
    let parent = bootstrap_bin.parent().unwrap_or(Path::new("."));
    // pdump filenames are {binary}-{64-hex-chars}.pdump
    let prefix = format!("{bin_name}-");
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(&*prefix) && name_str.ends_with(".pdump") {
            if options.dry_run {
                println!("  would touch: {}", entry.path().display());
            } else {
                #[cfg(unix)]
                {
                    let path = entry.path();
                    let status = std::process::Command::new("touch")
                        .arg("-m")
                        .arg(&path)
                        .status()?;
                    if !status.success() {
                        eprintln!("  WARN  touch {} failed", path.display());
                    }
                }
            }
        }
    }
    Ok(())
}

fn remove_primary_loaddefs_for_regeneration(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    loaddefs_el: &Path,
    theme_loaddefs_el: &Path,
) -> Result<()> {
    print_synthetic_step("force full primary loaddefs regeneration");
    let files = [
        loaddefs_el.to_path_buf(),
        theme_loaddefs_el.to_path_buf(),
        loaddefs_el.with_extension("elc"),
        theme_loaddefs_el.with_extension("elc"),
        paths.lisp_root.join("ldefs-boot.elc"),
        paths.lisp_root.join("emacs-lisp/cl-loaddefs.elc"),
    ];

    if options.dry_run {
        for file in &files {
            println!("  would remove: {}", file.display());
        }
        return Ok(());
    }

    for file in &files {
        let _ = remove_file_if_exists(file)?;
    }
    Ok(())
}

fn generated_secondary_loaddefs_files(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_secondary_loaddefs_files(lisp_root, lisp_root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_secondary_loaddefs_files(
    lisp_root: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_secondary_loaddefs_files(lisp_root, &path, out)?;
        } else if is_generated_secondary_loaddefs_file(lisp_root, &path) {
            out.push(path);
        }
    }

    Ok(())
}

fn is_generated_secondary_loaddefs_file(lisp_root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(lisp_root) else {
        return false;
    };

    if matches!(
        relative,
        rel if rel == Path::new("loaddefs.el")
            || rel == Path::new("ldefs-boot.el")
            || rel == Path::new("theme-loaddefs.el")
            || rel == Path::new("emacs-lisp/cl-loaddefs.el")
    ) {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    file_name == "loaddefs.el" || file_name.ends_with("-loaddefs.el")
}

fn remove_file_if_exists(path: &Path) -> Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn pipeline_paths(options: &FreshBuildOptions) -> PipelinePaths {
    let lisp_root = options.runtime_root.join("lisp");
    PipelinePaths {
        temacs: options.bin_dir.join(executable_name("neomacs-temacs")),
        bootstrap: options.bin_dir.join(executable_name("bootstrap-neomacs")),
        final_bin: options.bin_dir.join(executable_name("neomacs")),
        etc_root: options.runtime_root.join("etc"),
        makefile_in: lisp_root.join("Makefile.in"),
        leim_root: options.repo_root.join("leim"),
        admin_charsets_root: options.repo_root.join("admin/charsets"),
        admin_grammars_root: options.repo_root.join("admin/grammars"),
        admin_unidata_root: options.repo_root.join("admin/unidata"),
        lisp_root,
    }
}

fn executable_name(stem: &str) -> String {
    format!("{stem}{}", env::consts::EXE_SUFFIX)
}

fn ensure_runtime_inputs(paths: &PipelinePaths) -> Result<()> {
    for required in [
        paths.lisp_root.join("loadup.el"),
        paths.makefile_in.clone(),
        paths.lisp_root.join("emacs-lisp/loaddefs-gen.el"),
        paths.leim_root.join("Makefile.in"),
        paths.admin_charsets_root.join("Makefile.in"),
        paths.admin_grammars_root.join("Makefile.in"),
        paths.admin_unidata_root.join("Makefile.in"),
    ] {
        if !required.exists() {
            return Err(format!("missing required path: {}", required.display()).into());
        }
    }
    Ok(())
}

fn ensure_binaries_exist(paths: &PipelinePaths) -> Result<()> {
    for binary in [&paths.temacs, &paths.bootstrap, &paths.final_bin] {
        if !binary.exists() {
            return Err(format!("missing required path: {}", binary.display()).into());
        }
    }
    Ok(())
}

fn patch_primary_executable_fingerprint(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
) -> Result<()> {
    // GNU Emacs hashes and patches the just-linked temacs image, then copies
    // that same executable image for bootstrap-emacs/emacs. Cargo links the
    // user-facing neomacs binary, so treat it as the primary linked image and
    // create the build-role executable names by copying it after patching.
    print_synthetic_step("patch executable pdump fingerprint");
    if options.dry_run {
        println!("  would patch: {}", paths.final_bin.display());
        return Ok(());
    }

    if !paths.final_bin.exists() {
        return Err(format!("missing required path: {}", paths.final_bin.display()).into());
    }
    let image = load_executable_fingerprint_image(&paths.final_bin)?;
    let fingerprint = executable_fingerprint_from_image(&image);
    patch_loaded_executable_fingerprint(image, &fingerprint)?;
    println!(
        "  INFO  patched pdump fingerprint {}",
        uppercase_hex(&fingerprint)
    );
    Ok(())
}

fn copy_executable_role_images(options: &FreshBuildOptions, paths: &PipelinePaths) -> Result<()> {
    print_synthetic_step("copy executable role images");
    for destination in [&paths.temacs, &paths.bootstrap] {
        if options.dry_run {
            println!(
                "  would copy: {} -> {}",
                paths.final_bin.display(),
                destination.display()
            );
        } else {
            copy_executable_role_image(&paths.final_bin, destination)?;
        }
    }
    Ok(())
}

fn copy_executable_role_image(source: &Path, destination: &Path) -> Result<()> {
    if source == destination {
        return Ok(());
    }
    remove_file_if_exists(destination)?;
    fs::copy(source, destination)?;
    fs::set_permissions(destination, fs::metadata(source)?.permissions())?;
    Ok(())
}

#[cfg(test)]
fn executable_fingerprint(binary: &Path) -> Result<[u8; 32]> {
    let image = load_executable_fingerprint_image(binary)?;
    Ok(executable_fingerprint_from_image(&image))
}

fn load_executable_fingerprint_image(binary: &Path) -> Result<ExecutableFingerprintImage> {
    let mut normalized = fs::read(binary)?;
    let slots = executable_fingerprint_slots(&normalized);
    if slots.is_empty() {
        return Err(format!("missing pdump fingerprint record in {}", binary.display()).into());
    }
    for slot in &slots {
        normalized[*slot..*slot + FINGERPRINT_PLACEHOLDER.len()]
            .copy_from_slice(FINGERPRINT_PLACEHOLDER);
    }

    Ok(ExecutableFingerprintImage {
        path: binary.to_path_buf(),
        normalized,
        slots,
    })
}

fn executable_fingerprint_from_image(image: &ExecutableFingerprintImage) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&image.normalized);

    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
fn patch_executable_fingerprint(path: &Path, fingerprint: &[u8; 32]) -> Result<()> {
    let bytes = fs::read(path)?;
    let slots = executable_fingerprint_slots(&bytes);
    if slots.is_empty() {
        return Err(format!("missing pdump fingerprint record in {}", path.display()).into());
    }
    patch_executable_fingerprint_slots(path, &slots, fingerprint)?;
    Ok(())
}

fn patch_loaded_executable_fingerprint(
    image: ExecutableFingerprintImage,
    fingerprint: &[u8; 32],
) -> Result<()> {
    patch_executable_fingerprint_slots(&image.path, &image.slots, fingerprint)?;
    Ok(())
}

fn patch_executable_fingerprint_slots(
    path: &Path,
    slots: &[usize],
    fingerprint: &[u8; 32],
) -> Result<()> {
    let mut file = fs::OpenOptions::new().write(true).open(path)?;
    for slot in slots {
        file.seek(SeekFrom::Start((*slot).try_into()?))?;
        file.write_all(fingerprint)?;
    }
    Ok(())
}

fn executable_fingerprint_slots(bytes: &[u8]) -> Vec<usize> {
    let mut slots = Vec::new();
    let mut start = 0usize;
    while let Some(relative) = find_bytes(&bytes[start..], FINGERPRINT_MAGIC_START) {
        let record_start = start + relative;
        let slot_start = record_start + FINGERPRINT_MAGIC_START.len();
        let record_end = record_start + FINGERPRINT_RECORD_LEN;
        if record_end <= bytes.len()
            && &bytes[slot_start + FINGERPRINT_PLACEHOLDER.len()..record_end]
                == FINGERPRINT_MAGIC_END
        {
            slots.push(slot_start);
            start = record_end;
        } else {
            start = record_start + 1;
        }
    }
    slots
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn uppercase_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02X}").expect("write to string");
    }
    out
}

fn cargo_program() -> PathBuf {
    tool_program("cargo")
}

fn tool_program(name: &str) -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_program_on_path(name, env::var_os("PATH").as_deref(), &cwd)
        .unwrap_or_else(|| PathBuf::from(name))
}

fn resolve_program_on_path(program: &str, path: Option<&OsStr>, cwd: &Path) -> Option<PathBuf> {
    let path = path?;
    let candidate_names = path_lookup_candidate_names(program);
    for dir in env::split_paths(path) {
        let dir = if dir.is_absolute() {
            dir
        } else {
            cwd.join(dir)
        };
        for candidate_name in &candidate_names {
            let candidate = dir.join(candidate_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn path_lookup_candidate_names(program: &str) -> Vec<OsString> {
    path_lookup_candidate_names_with(program, env::var_os("PATHEXT").as_deref())
}

fn path_lookup_candidate_names_with(program: &str, pathext: Option<&OsStr>) -> Vec<OsString> {
    let program_path = Path::new(program);
    if program_path.extension().is_some() {
        return vec![OsString::from(program)];
    }

    #[cfg(windows)]
    {
        let pathext = pathext.unwrap_or_else(|| OsStr::new(".COM;.EXE;.BAT;.CMD"));
        let mut candidates = Vec::new();
        for ext in env::split_paths(pathext) {
            let ext = ext.as_os_str().to_string_lossy();
            let ext = ext.trim_matches(';').trim();
            if ext.is_empty() {
                continue;
            }
            let ext = if ext.starts_with('.') {
                ext.to_string()
            } else {
                format!(".{ext}")
            };
            let candidate = OsString::from(format!("{program}{ext}"));
            if !candidates.iter().any(|existing: &OsString| {
                existing
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&candidate.to_string_lossy())
            }) {
                candidates.push(candidate);
            }
        }
        candidates
    }

    #[cfg(not(windows))]
    {
        let _ = pathext;
        vec![OsString::from(program)]
    }
}

fn run_command(
    options: &FreshBuildOptions,
    cwd: &Path,
    program: &Path,
    args: &[OsString],
    envs: &[(OsString, OsString)],
) -> Result<()> {
    print_command(program.as_os_str(), args);
    if options.dry_run {
        return Ok(());
    }

    let started = Instant::now();
    let program_name = program.file_name().unwrap_or(program.as_os_str());

    let mut command = Command::new(program);
    command.current_dir(cwd);
    command.args(args.iter().map(OsString::as_os_str));
    command.envs(envs.iter().map(|(key, value)| (key, value)));
    remove_build_time_emacs_env(&mut command);
    if program.file_name() == Some(OsStr::new("cargo")) {
        remove_outer_cargo_env(&mut command);
    }

    let status = command.status()?;
    let elapsed_ms = started.elapsed().as_millis();
    let args_str = args
        .iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    println!(
        "  INFO  [{program_name:?} {args_str}] exited with {} in {elapsed_ms}ms",
        status
            .code()
            .map_or_else(|| "signal".to_string(), |c| c.to_string()),
    );
    if !status.success() {
        return Err(command_failure(program, args, status).into());
    }
    Ok(())
}

const BUILD_TIME_EMACS_ENV_VARS: [&str; 2] = ["EMACSLOADPATH", "EMACSNATIVELOADPATH"];

/// Keep xtask's bootstrap, generation, compilation, and dump subprocesses
/// isolated from the invoking user's installed Emacs packages.  GNU Emacs's
/// build makefiles likewise unexport these variables before invoking Emacs.
fn remove_build_time_emacs_env(command: &mut Command) {
    for key in BUILD_TIME_EMACS_ENV_VARS {
        command.env_remove(key);
    }
}

fn remove_outer_cargo_env(command: &mut Command) {
    for (key, _) in env::vars_os() {
        if should_remove_outer_cargo_env(&key) {
            command.env_remove(key);
        }
    }
}

fn should_remove_outer_cargo_env(key: &OsStr) -> bool {
    let Some(key) = key.to_str() else {
        return false;
    };

    matches!(
        key,
        "CARGO"
            | "CARGO_CRATE_NAME"
            | "CARGO_MANIFEST_DIR"
            | "CARGO_MANIFEST_LINKS"
            | "CARGO_MANIFEST_PATH"
            | "CARGO_PRIMARY_PACKAGE"
            | "OUT_DIR"
    ) || key.starts_with("CARGO_BIN_EXE_")
        || key.starts_with("CARGO_CFG_")
        || key.starts_with("CARGO_FEATURE_")
        || key.starts_with("CARGO_PKG_")
}

fn command_failure(program: &Path, args: &[OsString], status: ExitStatus) -> String {
    let mut rendered = String::new();
    write!(
        &mut rendered,
        "command failed with status {status}: {}",
        shell_quote(program.as_os_str())
    )
    .expect("write to string");
    for arg in args {
        rendered.push(' ');
        rendered.push_str(&shell_quote(arg.as_os_str()));
    }
    rendered
}

fn redirected_command_failure(
    program: &Path,
    args: &[OsString],
    input: Option<&Path>,
    output: &Path,
    status: ExitStatus,
) -> String {
    let mut rendered = String::new();
    write!(
        &mut rendered,
        "command failed with status {status}: {}",
        redirected_command_string(program.as_os_str(), args, input, output)
    )
    .expect("write to string");
    rendered
}

fn print_command(program: &OsStr, args: &[OsString]) {
    let mut rendered = String::from("+ ");
    rendered.push_str(&shell_quote(program));
    for arg in args {
        rendered.push(' ');
        rendered.push_str(&shell_quote(arg.as_os_str()));
    }
    println!("{rendered}");
}

fn print_gzip_decompress_command(input: &Path) {
    println!("+ decompress gzip {}", shell_quote(input.as_os_str()));
}

fn print_redirected_command(
    program: &OsStr,
    args: &[OsString],
    input: Option<&Path>,
    output: &Path,
) {
    println!(
        "+ {}",
        redirected_command_string(program, args, input, output)
    );
}

fn redirected_command_string(
    program: &OsStr,
    args: &[OsString],
    input: Option<&Path>,
    output: &Path,
) -> String {
    let mut rendered = String::new();
    rendered.push_str(&shell_quote(program));
    for arg in args {
        rendered.push(' ');
        rendered.push_str(&shell_quote(arg.as_os_str()));
    }
    if let Some(input) = input {
        rendered.push_str(" < ");
        rendered.push_str(&shell_quote(input.as_os_str()));
    }
    rendered.push_str(" > ");
    rendered.push_str(&shell_quote(output.as_os_str()));
    rendered
}

fn print_synthetic_step(message: &str) {
    println!("+ {message}");
}

fn shell_quote(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.is_empty()
        || text
            .chars()
            .any(|ch| ch.is_whitespace() || "'\"\\$`()[]{}*?&;<>|!".contains(ch))
    {
        format!("'{}'", text.replace('\'', "'\"'\"'"))
    } else {
        text.into_owned()
    }
}

fn loaddefs_dirs(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    lisp_dirs_matching_gnu_subdirs(
        lisp_root,
        |relative| !matches!(relative, rel if rel == Path::new("obsolete") || rel == Path::new("term")),
    )
}

fn lisp_dirs_for_custom_dependencies(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    lisp_dirs_matching_gnu_subdirs(
        lisp_root,
        |relative| !matches!(relative, rel if rel == Path::new("obsolete") || rel == Path::new("term")),
    )
}

fn lisp_dirs_for_finder_data(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    lisp_dirs_matching_gnu_subdirs(lisp_root, |relative| {
        !matches!(relative, rel if rel == Path::new("obsolete") || rel == Path::new("term"))
            && !matches!(
                relative
                    .components()
                    .next()
                    .and_then(|component| component.as_os_str().to_str()),
                Some("leim")
            )
    })
}

fn lisp_dirs_for_subdirs_update(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    lisp_dirs_matching_gnu_subdirs(lisp_root, |relative| {
        let relative = relative.as_os_str().to_string_lossy();
        !relative.starts_with("cedet") && !relative.starts_with("leim")
    })
}

fn lisp_dirs_matching_gnu_subdirs(
    lisp_root: &Path,
    include_relative: impl Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    collect_lisp_dirs(lisp_root, &mut dirs)?;
    dirs.retain(|dir| dir.strip_prefix(lisp_root).map_or(true, &include_relative));
    dirs.sort();
    Ok(dirs)
}

fn run_update_subdirs(options: &FreshBuildOptions, paths: &PipelinePaths) -> Result<()> {
    print_synthetic_step("update subdirs.el files");
    if options.dry_run {
        return Ok(());
    }

    let mut scanned = 0;
    let mut written = 0;
    let mut removed = 0;
    for dir in lisp_dirs_for_subdirs_update(&paths.lisp_root)? {
        scanned += 1;
        match update_subdirs_file(&dir)? {
            UpdateSubdirsChange::Unchanged => {}
            UpdateSubdirsChange::Written => written += 1,
            UpdateSubdirsChange::Removed => removed += 1,
        }
    }

    println!("  INFO  update-subdirs scanned {scanned} dirs, wrote {written}, removed {removed}");
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateSubdirsChange {
    Unchanged,
    Written,
    Removed,
}

fn update_subdirs_file(dir: &Path) -> Result<UpdateSubdirsChange> {
    let subdirs = update_subdirs_expression(dir)?;
    let target = dir.join("subdirs.el");
    if subdirs.is_empty() {
        match fs::remove_file(&target) {
            Ok(()) => return Ok(UpdateSubdirsChange::Removed),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Ok(UpdateSubdirsChange::Unchanged);
            }
            Err(err) => return Err(format!("remove {}: {err}", target.display()).into()),
        }
    }

    let contents = update_subdirs_contents(&subdirs);
    let temp = dir.join("subdirs.el~");
    match fs::remove_file(&temp) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(format!("remove {}: {err}", temp.display()).into()),
    }
    fs::write(&temp, contents.as_bytes())
        .map_err(|err| format!("write {}: {err}", temp.display()))?;

    let existing = fs::read(&target).ok();
    if existing.as_deref() == Some(contents.as_bytes()) {
        fs::remove_file(&temp).map_err(|err| format!("remove {}: {err}", temp.display()))?;
        Ok(UpdateSubdirsChange::Unchanged)
    } else {
        fs::rename(&temp, &target)
            .map_err(|err| format!("rename {} to {}: {err}", temp.display(), target.display()))?;
        Ok(UpdateSubdirsChange::Written)
    }
}

fn update_subdirs_expression(dir: &Path) -> Result<String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(String::new()),
        Err(err) => return Err(format!("read {}: {err}", dir.display()).into()),
    };
    let mut entries = entries.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut subdirs = String::new();
    for entry in entries {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("non-utf8 Lisp subdirectory under {}", dir.display()))?;
        if update_subdirs_ignores_name(name) {
            continue;
        }

        if name == "obsolete" {
            write!(&mut subdirs, " \"{name}\"").expect("write to string");
        } else {
            subdirs = format!("\"{name}\" {subdirs}");
        }
    }

    Ok(subdirs)
}

fn update_subdirs_ignores_name(name: &str) -> bool {
    name.starts_with('.')
        || name.ends_with(".elc")
        || name.ends_with(".el")
        || name == "term"
        || name == "RCS"
        || name == "CVS"
        || name == "Old"
        || name.starts_with('=')
        || name.ends_with('~')
        || name.ends_with(".orig")
        || name.ends_with(".rej")
}

fn update_subdirs_contents(subdirs: &str) -> String {
    format!(
        ";; In load-path, after this directory should come  -*- lexical-binding: t -*-\n\
;; certain of its subdirectories.  Here we specify them.\n\
(normal-top-level-add-to-load-path '({subdirs}))\n\
;; Local Variables:\n\
;; version-control: never\n\
;; no-byte-compile: t\n\
;; no-update-autoloads: t\n\
;; End:\n"
    )
}

fn run_compile_main(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    remove_lisp_bytecode_without_source(options, paths)?;

    let main_first_sources = parse_main_first_sources(&paths.makefile_in, &paths.lisp_root)?;
    let mut seen = BTreeSet::new();
    let mut main_first = Vec::new();
    for source in main_first_sources {
        push_compile_main_source(source, &mut seen, &mut main_first)?;
    }
    let mut general = Vec::new();
    for source in compile_main_sources(&paths.lisp_root)? {
        if seen.contains(&source) {
            continue;
        }
        push_compile_main_source(source, &mut seen, &mut general)?;
    }
    let dependencies = parse_compile_main_dependencies(&paths.makefile_in, &paths.lisp_root)?;

    let main_first = main_first
        .into_iter()
        .filter(|source| compile_main_needs_rebuild(source))
        .collect::<Vec<_>>();
    let general = compile_main_sources_needing_rebuild(general, &dependencies);

    if main_first.is_empty() && general.is_empty() {
        return Ok(());
    }

    print_synthetic_step("compile Lisp bytecode (GNU compile-main)");
    println!(
        "  INFO  byte-compiling {} .el files",
        main_first.len() + general.len()
    );
    let mut errors = Vec::new();
    let jobs = compile_main_jobs();

    if !main_first.is_empty() {
        println!(
            "  INFO  byte-compiling {} MAIN_FIRST .el files sequentially",
            main_first.len(),
        );
        errors.extend(run_compile_main_serial(options, paths, envs, main_first));
    }

    if !general.is_empty() {
        println!(
            "  INFO  byte-compiling {} general .el files with {jobs} parallel jobs",
            general.len()
        );
        errors.extend(run_compile_main_parallel(
            options,
            paths,
            envs,
            general,
            &dependencies,
            jobs,
        )?);
    }

    if !errors.is_empty() {
        eprintln!("  ERROR  {} files failed to byte-compile:", errors.len());
        for e in &errors {
            eprintln!("    - {}", e);
        }
        return Err(compile_main_failure_summary(&errors).into());
    }

    Ok(())
}

fn compile_main_failure_summary(errors: &[String]) -> String {
    format!(
        "compile-main failed to byte-compile {} file{}",
        errors.len(),
        if errors.len() == 1 { "" } else { "s" }
    )
}

fn run_preloaded_lisp_byte_compile(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
) -> Result<()> {
    print_synthetic_step("byte-compile loadup preloaded Lisp (GNU src/lisp.mk)");
    let all_sources =
        parse_preloaded_lisp_sources(&paths.lisp_root.join("loadup.el"), &paths.lisp_root)?;
    let characters_source = paths.lisp_root.join("international/characters.el");
    let unicode_deps = preloaded_characters_dependency_sources(&paths.lisp_root);
    if all_sources.contains(&characters_source) {
        let deps_to_compile = unicode_deps
            .iter()
            .filter(|source| options.dry_run || bytecode_needs_rebuild(source))
            .collect::<Vec<_>>();
        for source in deps_to_compile {
            run_preloaded_lisp_byte_compile_source(options, paths, envs, source)?;
        }
    }

    let sources = all_sources
        .into_iter()
        .filter(|source| {
            options.dry_run
                || if *source == characters_source {
                    bytecode_needs_rebuild_with_dependencies(source, &unicode_deps)
                } else {
                    bytecode_needs_rebuild(source)
                }
        })
        .collect::<Vec<_>>();

    if sources.is_empty() {
        println!("  INFO  loadup preloaded .elc files are up to date");
        return Ok(());
    }

    let jobs = compile_main_jobs();
    println!(
        "  INFO  byte-compiling {} loadup preloaded .el files with {jobs} parallel jobs",
        sources.len()
    );

    if options.dry_run {
        for source in &sources {
            run_preloaded_lisp_byte_compile_source(options, paths, envs, source)?;
        }
    } else {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs).build()?;
        let errors: Vec<String> = pool.install(|| {
            sources
                .par_iter()
                .filter_map(|source| {
                    run_preloaded_lisp_byte_compile_source(options, paths, envs, source)
                        .err()
                        .map(|err| format!("{} ({err})", source.display()))
                })
                .collect()
        });

        if !errors.is_empty() {
            eprintln!(
                "  ERROR  {} preloaded .el file{} failed to byte-compile:",
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            );
            for error in &errors {
                eprintln!("    - {error}");
            }
            return Err(format!(
                "{} preloaded file{} failed to byte-compile",
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            )
            .into());
        }
    }

    Ok(())
}

fn compile_main_jobs() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .max(1)
}

fn run_compile_main_parallel(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    sources: Vec<PathBuf>,
    dependencies: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    jobs: usize,
) -> Result<Vec<String>> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs).build()?;
    let waves = compile_main_dependency_waves(sources, dependencies)?;
    let mut errors = Vec::new();

    for wave in waves {
        let wave_errors = if options.dry_run {
            wave.iter()
                .filter_map(|source| {
                    run_final_compile_main_source(options, paths, envs, source)
                        .err()
                        .map(|err| format!("{} ({err})", source.display()))
                })
                .collect::<Vec<_>>()
        } else {
            pool.install(|| {
                wave.par_iter()
                    .filter_map(|source| {
                        run_final_compile_main_source(options, paths, envs, source)
                            .err()
                            .map(|err| format!("{} ({err})", source.display()))
                    })
                    .collect::<Vec<_>>()
            })
        };

        errors.extend(wave_errors);
    }

    Ok(errors)
}

fn run_compile_main_serial(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    sources: Vec<PathBuf>,
) -> Vec<String> {
    sources
        .iter()
        .filter_map(|source| {
            run_final_compile_main_source(options, paths, envs, source)
                .err()
                .map(|err| format!("{} ({err})", source.display()))
        })
        .collect()
}

fn compile_main_dependency_waves(
    sources: Vec<PathBuf>,
    dependencies: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
) -> Result<Vec<Vec<PathBuf>>> {
    let mut pending = sources.into_iter().collect::<BTreeSet<_>>();
    let mut waves = Vec::new();

    while !pending.is_empty() {
        let ready = pending
            .iter()
            .filter(|source| {
                dependencies
                    .get(*source)
                    .is_none_or(|deps| deps.iter().all(|dep| !pending.contains(dep)))
            })
            .cloned()
            .collect::<Vec<_>>();

        if ready.is_empty() {
            return Err(format!(
                "compile-main dependency cycle or missing wave among {} pending files",
                pending.len()
            )
            .into());
        }

        for source in &ready {
            pending.remove(source);
        }
        waves.push(ready);
    }

    Ok(waves)
}

fn compile_main_sources_needing_rebuild(
    sources: Vec<PathBuf>,
    dependencies: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
) -> Vec<PathBuf> {
    let initial_rebuild = sources
        .iter()
        .filter(|source| {
            compile_main_needs_rebuild(source)
                || compile_main_dependency_bytecode_newer(source, dependencies)
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let rebuild = compile_main_rebuild_closure(&sources, dependencies, initial_rebuild);

    sources
        .into_iter()
        .filter(|source| rebuild.contains(source))
        .collect()
}

fn compile_main_rebuild_closure(
    sources: &[PathBuf],
    dependencies: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    mut rebuild: BTreeSet<PathBuf>,
) -> BTreeSet<PathBuf> {
    let source_set = sources.iter().cloned().collect::<BTreeSet<_>>();

    loop {
        let mut changed = false;
        for source in sources {
            if rebuild.contains(source) {
                continue;
            }
            let dependency_will_rebuild = dependencies.get(source).is_some_and(|deps| {
                deps.iter()
                    .any(|dep| source_set.contains(dep) && rebuild.contains(dep))
            });
            if dependency_will_rebuild {
                changed |= rebuild.insert(source.clone());
            }
        }
        if !changed {
            break;
        }
    }

    rebuild
}

fn compile_main_dependency_bytecode_newer(
    source: &Path,
    dependencies: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
) -> bool {
    let Some(deps) = dependencies.get(source) else {
        return false;
    };
    let target_elc = source.with_extension("elc");
    let Ok(target_mtime) = fs::metadata(&target_elc).and_then(|metadata| metadata.modified())
    else {
        return true;
    };

    deps.iter().any(|dep| {
        let dep_elc = dep.with_extension("elc");
        fs::metadata(&dep_elc)
            .and_then(|metadata| metadata.modified())
            .map_or_else(
                |_| dep.is_file() && compile_main_needs_rebuild(dep),
                |dep_mtime| dep_mtime > target_mtime,
            )
    })
}

fn run_bootstrap_byte_compile_source(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    source: &Path,
) -> Result<()> {
    run_byte_compile_source_with(options, bootstrap_byte_compile_emacs(paths), envs, source)
}

fn run_preloaded_lisp_byte_compile_source(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    source: &Path,
) -> Result<()> {
    let args = preloaded_lisp_args_for_source(options.native_comp, source);
    run_command(
        options,
        &options.repo_root,
        bootstrap_byte_compile_emacs(paths),
        &args,
        envs,
    )
}

fn run_final_compile_main_source(
    options: &FreshBuildOptions,
    paths: &PipelinePaths,
    envs: &[(OsString, OsString)],
    source: &Path,
) -> Result<()> {
    run_byte_compile_source_with(options, compile_main_emacs(paths), envs, source)
}

fn run_byte_compile_source_with(
    options: &FreshBuildOptions,
    program: &Path,
    envs: &[(OsString, OsString)],
    source: &Path,
) -> Result<()> {
    let args = compile_main_args_for_source(options.native_comp, source);
    run_command(options, &options.repo_root, program, &args, envs)
}

fn compile_main_emacs(paths: &PipelinePaths) -> &Path {
    &paths.final_bin
}

fn bootstrap_byte_compile_emacs(paths: &PipelinePaths) -> &Path {
    &paths.bootstrap
}

fn push_compile_main_source(
    source: PathBuf,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    if !source.is_file() {
        return Err(format!("compile-main source does not exist: {}", source.display()).into());
    }

    if seen.insert(source.clone()) {
        out.push(source);
    }
    Ok(())
}

fn parse_main_first_sources(makefile_in: &Path, lisp_root: &Path) -> Result<Vec<PathBuf>> {
    let contents = fs::read_to_string(makefile_in)?;
    Ok(parse_main_first_sources_from_str(&contents, lisp_root))
}

fn parse_main_first_sources_from_str(contents: &str, lisp_root: &Path) -> Vec<PathBuf> {
    let mut capture = false;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim_end();
        if let Some(rest) = strip_makefile_assignment(line, "MAIN_FIRST") {
            capture = line.ends_with('\\');
            emit_lisp_source_paths(rest, lisp_root, &mut seen, &mut out);
            continue;
        }

        if capture {
            emit_lisp_source_paths(line, lisp_root, &mut seen, &mut out);
            capture = line.ends_with('\\');
        }
    }

    out
}

fn compile_main_sources(lisp_root: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    collect_lisp_dirs(lisp_root, &mut dirs)?;
    dirs.sort();

    let mut sources = Vec::new();
    for dir in dirs {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        let mut files = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.is_file()
                    && path.extension() == Some(OsStr::new("el"))
                    && !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with('.'))
            })
            .collect::<Vec<_>>();
        files.sort();

        for source in files {
            if compile_main_should_consider(&source)? {
                sources.push(source);
            }
        }
    }

    Ok(sources)
}

fn collect_lisp_dirs(current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    out.push(current.to_path_buf());

    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect_lisp_dirs(&path, out)?;
        }
    }

    Ok(())
}

fn compile_main_should_consider(source: &Path) -> Result<bool> {
    if source.with_extension("elc").is_file() {
        return Ok(true);
    }

    Ok(!source_has_no_byte_compile_marker(source)?)
}

fn compile_main_needs_rebuild(source: &Path) -> bool {
    if !compile_main_should_consider(source).unwrap_or(true) {
        return false;
    }
    bytecode_needs_rebuild(source)
}

fn source_has_no_byte_compile_marker(source: &Path) -> Result<bool> {
    let contents = fs::read(source)?;
    let contents = String::from_utf8_lossy(&contents);
    Ok(contents.lines().any(gnu_no_byte_compile_marker_line))
}

fn parse_compile_main_dependencies(
    makefile_in: &Path,
    lisp_root: &Path,
) -> Result<BTreeMap<PathBuf, BTreeSet<PathBuf>>> {
    let contents = fs::read_to_string(makefile_in)?;
    Ok(parse_compile_main_dependencies_from_str(
        &contents, lisp_root,
    ))
}

fn parse_compile_main_dependencies_from_str(
    contents: &str,
    lisp_root: &Path,
) -> BTreeMap<PathBuf, BTreeSet<PathBuf>> {
    let mut dependencies = BTreeMap::new();
    let mut logical = String::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim_end();
        let continuation = line.ends_with('\\');
        let fragment = line.strip_suffix('\\').unwrap_or(line);
        logical.push_str(fragment);
        logical.push(' ');

        if continuation {
            continue;
        }

        if let Some((targets, deps)) = logical.split_once(':') {
            let targets = compile_main_dependency_paths(targets, lisp_root);
            let deps = compile_main_dependency_paths(deps, lisp_root)
                .into_iter()
                .collect::<BTreeSet<_>>();
            if !targets.is_empty() && !deps.is_empty() {
                for target in targets {
                    dependencies
                        .entry(target)
                        .or_insert_with(BTreeSet::new)
                        .extend(deps.iter().cloned());
                }
            }
        }

        logical.clear();
    }

    dependencies
}

fn compile_main_dependency_paths(fragment: &str, lisp_root: &Path) -> Vec<PathBuf> {
    let normalized = fragment.replace('\\', " ");
    normalized
        .split_whitespace()
        .filter_map(|token| {
            let stripped = token.strip_prefix("$(lisp)/")?;
            let mut path = lisp_root.join(stripped);
            if path.extension() != Some(OsStr::new("elc")) {
                return None;
            }
            path.set_extension("el");
            Some(path)
        })
        .collect()
}

fn gnu_no_byte_compile_marker_line(line: &str) -> bool {
    if !line.starts_with(';') {
        return false;
    }

    let needle = "no-byte-compile:";
    let mut search_from = 0;
    while let Some(relative_index) = line[search_from..].find(needle) {
        let index = search_from + relative_index;
        let previous = line[..index].chars().next_back();
        if previous.is_some_and(|ch| !ch.is_ascii_alphabetic())
            && line[index + needle.len()..].trim_start().starts_with('t')
        {
            return true;
        }
        search_from = index + needle.len();
    }

    false
}

fn compile_main_args_for_source(native_comp: bool, source: &Path) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("--eval"),
        OsString::from("(setq load-prefer-newer t byte-compile-warnings 'all)"),
        OsString::from("--eval"),
        OsString::from("(setq org--inhibit-version-check t)"),
    ];
    if native_comp {
        args.push(OsString::from("-l"));
        args.push(OsString::from("comp"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("batch-byte+native-compile"));
    } else {
        args.push(OsString::from("-f"));
        args.push(OsString::from("batch-byte-compile"));
    }
    args.push(source.as_os_str().to_os_string());
    args
}

fn preloaded_lisp_args_for_source(native_comp: bool, source: &Path) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--batch"),
        OsString::from("--no-site-file"),
        OsString::from("--no-site-lisp"),
        OsString::from("--eval"),
        OsString::from("(setq load-prefer-newer t byte-compile-warnings 'all)"),
        OsString::from("--eval"),
        OsString::from("(setq org--inhibit-version-check t)"),
    ];
    if native_comp {
        args.push(OsString::from("-l"));
        args.push(OsString::from("comp"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("byte-compile-refresh-preloaded"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("batch-byte+native-compile"));
    } else {
        args.push(OsString::from("-l"));
        args.push(OsString::from("bytecomp"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("byte-compile-refresh-preloaded"));
        args.push(OsString::from("-f"));
        args.push(OsString::from("batch-byte-compile"));
    }
    args.push(source.as_os_str().to_os_string());
    args
}

fn parse_compile_first_sources(
    makefile_in: &Path,
    lisp_root: &Path,
    native_comp: bool,
) -> Result<Vec<PathBuf>> {
    let contents = fs::read_to_string(makefile_in)?;
    Ok(parse_compile_first_sources_from_str(
        &contents,
        lisp_root,
        native_comp,
    ))
}

fn parse_preloaded_lisp_sources(loadup: &Path, lisp_root: &Path) -> Result<Vec<PathBuf>> {
    let contents = fs::read_to_string(loadup)?;
    Ok(parse_preloaded_lisp_sources_from_str(&contents, lisp_root))
}

fn parse_preloaded_lisp_sources_from_str(contents: &str, lisp_root: &Path) -> Vec<PathBuf> {
    let mut targets = BTreeSet::new();

    // GNU src/Makefile.in generates src/lisp.mk with:
    //   sed -n 's/^[ \t]*(load "\([^"]*\)".*/\1/p' loadup.el |
    //     sed -e 's/$/.elc \\/' -e 's/\.el\.elc/.el/'
    for line in contents.lines() {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let Some(rest) = trimmed.strip_prefix("(load \"") else {
            continue;
        };
        let Some((library, _)) = rest.split_once('"') else {
            continue;
        };
        let target = if library.ends_with(".el") {
            library.to_string()
        } else {
            format!("{library}.elc")
        };
        targets.insert(target);
    }

    targets.remove("leim/leim-list.el");
    targets.remove("site-load.elc");
    targets.remove("site-init.elc");
    targets.insert("loaddefs.elc".to_string());

    let mut sources = Vec::new();
    push_preloaded_lisp_source("loaddefs.elc", lisp_root, &mut sources);
    for target in targets {
        if target == "loaddefs.elc" {
            continue;
        }
        push_preloaded_lisp_source(&target, lisp_root, &mut sources);
    }
    sources
}

fn push_preloaded_lisp_source(target: &str, lisp_root: &Path, out: &mut Vec<PathBuf>) {
    let Some(source) = target.strip_suffix(".elc") else {
        return;
    };
    let source = lisp_root.join(format!("{source}.el"));
    if source.is_file() && !source_has_no_byte_compile_marker(&source).unwrap_or(false) {
        out.push(source);
    }
}

fn parse_compile_first_sources_from_str(
    contents: &str,
    lisp_root: &Path,
    native_comp: bool,
) -> Vec<PathBuf> {
    let mut capture = false;
    let mut in_native_block = false;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim_end();
        if line == "ifeq ($(HAVE_NATIVE_COMP),yes)" {
            in_native_block = true;
            continue;
        }
        if line == "endif" {
            in_native_block = false;
            continue;
        }

        if let Some(rest) = strip_compile_first_assignment(line) {
            if in_native_block && !native_comp {
                capture = line.ends_with('\\');
                continue;
            }
            capture = line.ends_with('\\');
            emit_compile_first_paths(rest, lisp_root, &mut seen, &mut out);
            continue;
        }

        if capture {
            emit_compile_first_paths(line, lisp_root, &mut seen, &mut out);
            capture = line.ends_with('\\');
        }
    }

    out.into_iter().filter(|path| path.is_file()).collect()
}

/// Return true if `source` (a .el file) needs to be byte-compiled because
/// its .elc sibling is missing or older.  Mirrors what GNU make would do
/// for a `%.elc: %.el` pattern rule under lisp/Makefile.in.
fn compile_first_needs_rebuild(source: &Path) -> bool {
    bytecode_needs_rebuild(source)
}

fn bytecode_needs_rebuild(source: &Path) -> bool {
    let elc = source.with_extension("elc");
    let Ok(source_meta) = fs::metadata(source) else {
        // Can't stat the source — let the compiler surface the
        // error rather than silently skipping it.
        return true;
    };
    let Ok(elc_meta) = fs::metadata(&elc) else {
        return true; // .elc missing
    };
    let source_mtime = source_meta.modified().ok();
    let elc_mtime = elc_meta.modified().ok();
    match (source_mtime, elc_mtime) {
        (Some(s), Some(e)) => s > e,
        _ => true,
    }
}

fn bytecode_needs_rebuild_with_dependencies(source: &Path, dependencies: &[PathBuf]) -> bool {
    if bytecode_needs_rebuild(source) {
        return true;
    }
    let elc = source.with_extension("elc");
    let Ok(elc_meta) = fs::metadata(&elc) else {
        return true;
    };
    let elc_mtime = elc_meta.modified().ok();
    dependencies.iter().any(|dependency| {
        bytecode_needs_rebuild(dependency)
            || fs::metadata(dependency.with_extension("elc"))
                .and_then(|metadata| metadata.modified())
                .ok()
                .zip(elc_mtime)
                .is_some_and(|(dep_mtime, target_mtime)| dep_mtime > target_mtime)
    })
}

fn preloaded_characters_dependency_sources(lisp_root: &Path) -> Vec<PathBuf> {
    // GNU src/Makefile.in:
    //   international/characters.elc: international/charscript.elc
    //                                international/emoji-zwj.elc
    // `characters.elc' loads these generated helpers while dump-mode is non-nil,
    // so they must be byte-compiled before the final pdump.
    ["international/charscript.el", "international/emoji-zwj.el"]
        .into_iter()
        .map(|relative| lisp_root.join(relative))
        .filter(|source| source.is_file())
        .filter(|source| !source_has_no_byte_compile_marker(source).unwrap_or(false))
        .collect()
}

fn compile_first_args_for_source(native_comp: bool, source: &Path) -> Vec<OsString> {
    compile_first_args_for_sources(native_comp, std::slice::from_ref(&source.to_path_buf()))
}

fn compile_first_args_for_sources(native_comp: bool, sources: &[PathBuf]) -> Vec<OsString> {
    let mut args = vec![OsString::from("--batch")];
    if native_comp {
        args.push(OsString::from("-l"));
        args.push(OsString::from("comp"));
    }
    args.push(OsString::from("-f"));
    args.push(OsString::from("batch-byte-compile"));
    for source in sources {
        args.push(source.as_os_str().to_os_string());
    }
    args
}

fn strip_compile_first_assignment(line: &str) -> Option<&str> {
    strip_makefile_assignment(line, "COMPILE_FIRST")
}

fn emit_compile_first_paths(
    fragment: &str,
    lisp_root: &Path,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    emit_lisp_source_paths(fragment, lisp_root, seen, out)
}

fn strip_makefile_assignment<'a>(line: &'a str, variable: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(variable)?;
    let rest = rest.trim_start();
    rest.strip_prefix("+=")
        .or_else(|| rest.strip_prefix('='))
        .map(str::trim_start)
}

fn emit_lisp_source_paths(
    fragment: &str,
    lisp_root: &Path,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<PathBuf>,
) {
    let normalized = fragment.replace('\\', " ");
    for token in normalized.split_whitespace() {
        let Some(stripped) = token
            .strip_prefix("$(lisp)/")
            .or_else(|| token.strip_prefix("./"))
        else {
            continue;
        };
        let mut path = lisp_root.join(stripped);
        if path.extension() == Some(OsStr::new("elc")) {
            path.set_extension("el");
        }
        if seen.insert(path.clone()) {
            out.push(path);
        }
    }
}

fn write_ldefs_boot(loaddefs_el: &Path, ldefs_boot: &Path) -> Result<()> {
    let input = fs::read_to_string(loaddefs_el)?;
    let output = inject_no_byte_compile(&input);
    fs::write(ldefs_boot, output)?;
    Ok(())
}

const LOADDEFS_END_BOUNDARY: &str = "\n\x0c\n;;; End of scraped data";
// GNU Emacs 31.0.90's `loaddefs-generate` prints the autoload docstring on its
// own line (verified against the system emacs-31.0.90 binary), not the older
// `"\<newline>...` continuation style. neomacs's synced 31.0.90 loaddefs-gen
// matches GNU exactly; the previous expected layout was the pre-31.0.90 format.
const GNU_EBROWSE_DECLARATION_AUTOLOAD: &str = concat!(
    "(autoload 'ebrowse-tags-find-declaration \"ebrowse\"\n",
    "\"Find declaration of member at point.\" t)"
);
const MISPLACED_EBROWSE_DECLARATION_DOCSTRING: &str =
    "Find declaration of member at point.\"\x0c\n;;; End of scraped data";

fn validate_primary_loaddefs(loaddefs_el: &Path) -> Result<()> {
    let contents = fs::read_to_string(loaddefs_el)
        .map_err(|err| format!("read generated {}: {err}", loaddefs_el.display()))?;
    validate_primary_loaddefs_contents(&contents).map_err(|err| -> DynError {
        format!("validate generated {}: {err}", loaddefs_el.display()).into()
    })
}

fn validate_primary_loaddefs_contents(contents: &str) -> Result<()> {
    if contents.contains(MISPLACED_EBROWSE_DECLARATION_DOCSTRING) {
        return Err("generated loaddefs.el moved an ebrowse docstring to the final page".into());
    }

    if !contents.contains(LOADDEFS_END_BOUNDARY) {
        return Err(format!(
            "generated loaddefs.el is missing GNU end boundary {:?}",
            LOADDEFS_END_BOUNDARY
        )
        .into());
    }

    if !contents.contains(GNU_EBROWSE_DECLARATION_AUTOLOAD) {
        return Err(
            "generated loaddefs.el is missing GNU ebrowse autoload docstring layout".into(),
        );
    }

    Ok(())
}

fn inject_no_byte_compile(contents: &str) -> String {
    let needle = ";; Local Variables:";
    if let Some(index) = contents.find(needle) {
        let insert_at = index + needle.len();
        let mut output = String::with_capacity(contents.len() + 24);
        output.push_str(&contents[..insert_at]);
        output.push('\n');
        output.push_str(";; no-byte-compile: t");
        output.push_str(&contents[insert_at..]);
        output
    } else {
        let mut output = contents.to_string();
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(";; Local Variables:\n");
        output.push_str(";; no-byte-compile: t\n");
        output.push_str(";; End:\n");
        output
    }
}

fn print_usage() {
    print!("{}", usage_text());
}

fn usage_text() -> &'static str {
    "\
Usage: cargo xtask [fresh-build] (--release | --profile NAME) [--bin-dir DIR] [--runtime-root DIR] [--dry-run] [--native-comp|--no-native-comp] [--skip-build] [--no-byte-compile] [--aot-preload]
       cargo xtask perf list
       cargo xtask perf run SCENARIO [--editor PATH] [--iterations N] [--frontend batch|tui|gui]

--release (or --profile NAME) is required: fresh-build produces the runnable runtime
binary by byte-compiling the Lisp tree with it, so it needs an optimized profile.

Build the GNU-shaped Neomacs runtime pipeline:
  1. cargo build --verbose -p neomacs [--features webview] [--release]
  2. generate GNU early charset/unidata Lisp sources
  3. regenerate GNU subdirs.el files
  4. neomacs-temacs --temacs=pbootstrap
  5. bootstrap-neomacs byte-compiles the GNU COMPILE_FIRST set into .elc files
  6. bootstrap-neomacs generates GNU Unicode Lisp data
  7. bootstrap-neomacs runs GNU gen-lisp generators for leim and semantic
  8. bootstrap-neomacs generates loaddefs / ldefs-boot
  9. bootstrap-neomacs byte-compiles the GNU src/lisp.mk preloaded Lisp set
 10. neomacs-temacs --temacs=pdump
 11. neomacs byte-compiles the GNU compile-main Lisp set into .elc files

Options:
  --bin-dir DIR       Directory containing neomacs and generated role copies
                      (an INPUT: the binaries must already be there, e.g. with
                      --skip-build; it is not a cargo output directory)
  --features LIST     Comma-separated extra cargo features for stage 1, e.g.
                      vm-profile. Combine with --profile so the feature build
                      lands in its own target dir instead of replacing the
                      release binary (whose pdump would then stop matching).
  --runtime-root DIR  Runtime root containing lisp/ and etc/
  --release           Build with the release profile, using target/release
                      (equivalent to --profile release)
  --profile release-pgo
                      Profile-guided build. Runs the pipeline TWICE: an
                      instrumented pass (target/release-pgo-gen), then the
                      committed training workload crates/xtask/pgo-train.el, then
                      llvm-profdata merge, then the optimized build into
                      target/release-pgo. Needs `rustup component add
                      llvm-tools`. Measured on this tree: STARTUP -17%,
                      byte-compile -17% instructions, batch font-lock benches
                      -23%/-27% cycles. It does NOT speed up the interactive
                      edit loop: a real TTY keystroke->redisplay measurement is
                      +2%, because the training workload runs in --batch and so
                      never exercises redisplay. Scope claims accordingly.
  --profile release-pgo-profiling
                      As release-pgo, but keeps debug symbols so the SHIPPED
                      configuration can be profiled -- PGO changes inlining and
                      block layout, so hotspots from a plain release build do
                      not necessarily carry over.
  --profile NAME      Build with cargo profile NAME, using target/<dir> for it.
                      `profiling` inherits release but keeps debug symbols, and
                      lives in target/profiling -- so it does NOT disturb an
                      existing target/release build or its pdump. Unoptimized
                      profiles (dev/debug/test) are rejected.
  --dry-run           Print planned commands without running them
  --native-comp       Include native-comp-only COMPILE_FIRST entries
  --no-native-comp    Exclude native-comp-only COMPILE_FIRST entries
  --skip-build        Skip the initial cargo build -p neomacs stage
  --no-byte-compile   Skip byte-compilation steps (5, 9, 11); keep existing .elc
  --aot-preload       Enable the in-neomacs dump-time AOT producer: sets
                      NEOVM_AOT_PRELOAD=1 on the --temacs=pdump step (10) so it
                      emits libneomacs-preload.so + manifest beside the pdump,
                      then verifies they landed. With --dry-run the producer only
                      lists candidates + dedup stats (no link/write); that combo
                      runs the dump for real to observe the enumeration.

Environment:
  NEOMACS_NATIVE_COMP=yes
      Include the native-comp-only COMPILE_FIRST entries from lisp/Makefile.in.
"
}

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;
