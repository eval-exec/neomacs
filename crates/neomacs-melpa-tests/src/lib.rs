//! Package ecosystem compatibility harness for Neomacs.
//!
//! A scenario installs packages into an isolated, workspace-local sandbox,
//! exits the editor, and launches a fresh process to probe the installed
//! packages. The same scenario can run against Neomacs or GNU Emacs and
//! against either revision-pinned package source or a local fixture archive.

use std::fmt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use neomacs_melpa_test_support::{
    CommandError, configure_process_environment, elisp_string, output_with_timeout,
    package_preparation_run_id, publish_package_preparation_failure, sanitize_label,
};
pub use neomacs_melpa_test_support::{
    EmacsRuntime, LockedPackageSource, MelpaSandbox, PackageActivation, PreparedPackageSet,
    SHALLOW_GIT_FETCH_ARGS, SourceBuild, locked_melpa_install_plan, locked_melpa_source,
    locked_melpa_sources, neomacs_binary, package_activation_elisp,
    preflight_locked_melpa_packages, prepare_cached_locked_melpa_package,
    prepare_cached_tree_sitter_grammar, prepare_cached_tree_sitter_grammar_from_subdirectory,
    workspace_root,
};
use neomacs_test_oracle::{
    BatchProbe, EvalOutcome, ExpectedOutcome, extract_marked_batch_protocol,
    extract_marked_outcome, oracle_normalizer_elisp, validate_batch_case_id,
    wrap_elisp_batch_outcomes, wrap_elisp_outcome,
};

const RESULT_MARKER: &str = "NEOMACS-MELPA-RESULT:";
const OUTCOME_MARKER: &str = "NEOMACS-MELPA-OUTCOME:";
const BATCH_BEGIN_MARKER: &str = "NEOMACS-MELPA-BEGIN:";
const BATCH_COMPLETE_MARKER: &str = "NEOMACS-MELPA-COMPLETE:";
const TRANSPORTED_FORM_FUNCTION: &str = "neomacs--melpa-oracle-transported-form";
const INSTALLED_MARKER: &str = "NEOMACS-MELPA-INSTALLED:";
const DEFAULT_PROCESS_TIMEOUT: Duration = neomacs_melpa_test_support::DEFAULT_PROCESS_TIMEOUT;

#[derive(Clone, Copy)]
struct PackageArchiveSpec {
    cache_directory: &'static str,
    label: &'static str,
    name: &'static str,
    url: &'static str,
}

const GNU_ELPA_ARCHIVE: PackageArchiveSpec = PackageArchiveSpec {
    cache_directory: "package-cache-gnu-elpa",
    label: "GNU ELPA",
    name: "gnu",
    url: "https://elpa.gnu.org/packages/",
};

/// The exact Async release selected from GNU ELPA by the comprehensive API
/// parity corpus.
pub const ASYNC_GNU_ELPA_PIN: (&str, &str) = ("async", "1.9.9");

/// The exact current MELPA Async package selected by the comprehensive
/// serialization, process, byte-compilation, Dired, package, and mail workflow
/// parity corpus. This remains distinct from `ASYNC_GNU_ELPA_PIN`.
/// MELPA built this archive from upstream commit
/// `5faab28916603bb324d9faba057021ce028ca847`.
pub const ASYNC_MELPA_PIN: (&str, &str) = ("async", "20260318.1803");

/// The exact async1 package selected by the comprehensive callback-chain,
/// scheduler, parallel aggregation, and timer parity corpus. MELPA built this
/// archive from upstream commit
/// `88cccffe14bdd0a61dbb2e33edf8c335706f24dc`.
pub const ASYNC1_MELPA_PIN: (&str, &str) = ("async1", "20260421.2116");

/// The exact asyncloop package selected by the comprehensive non-blocking
/// series, cancellation, timer-ordering, recovery, and lifecycle parity
/// corpus. MELPA built this archive from upstream commit
/// `7d60950d160098a879293e049b9863bc955f8666`.
pub const ASYNCLOOP_MELPA_PIN: (&str, &str) = ("asyncloop", "20240818.1247");

/// The exact atom-dark-theme package selected by the comprehensive theme
/// registration, face, customization, remapping, and lifecycle parity corpus.
/// MELPA built this archive from upstream commit
/// `2b3c7ad42bbcab3214a131f8957b92e717b36ad3`.
pub const ATOM_DARK_THEME_MELPA_PIN: (&str, &str) = ("atom-dark-theme", "20220114.1902");

/// The exact atom-one-dark-theme package selected by the comprehensive
/// palette, face, variable, remapping, and lifecycle parity corpus. MELPA
/// built this archive from upstream commit
/// `bba02fb2672a4c439d71920d8e068a3ff2ed463e`.
pub const ATOM_ONE_DARK_THEME_MELPA_PIN: (&str, &str) = ("atom-one-dark-theme", "20260119.1824");

/// The exact auctex-cluttex package selected by the practical AUCTeX mode,
/// command expansion, local process, ANSI output, and command-default parity
/// corpus. MELPA built this archive from upstream commit
/// `1a940892dcbe3e4874d2d60db92de1cb34a1b773`.
pub const AUCTEX_CLUTTEX_MELPA_PIN: (&str, &str) = ("auctex-cluttex", "20240519.1303");

/// The exact auctex-latexmk package selected by the practical LatexMk setup,
/// command expansion, local process, sentinel, recentering, and cleanup parity
/// corpus. MELPA built this archive from the Emacsmirror compatibility commit
/// `b00a95e6b34c94987fda5a57c20cfe2f064b1c7a`.
pub const AUCTEX_LATEXMK_MELPA_PIN: (&str, &str) = ("auctex-latexmk", "20221025.1219");

/// The exact auctex-lua package selected by the practical embedded-Lua edit,
/// save, cancellation, custom-environment, shared-state, and malformed-input
/// parity corpus.
/// MELPA built this archive from upstream commit
/// `799cd8ac10c96991bb63d9aa60528ae5d8c786b5`.
pub const AUCTEX_LUA_MELPA_PIN: (&str, &str) = ("auctex-lua", "20151121.1610");

/// The exact auto-complete package selected by the comprehensive source,
/// candidate, completion, dictionary, history, configuration, and lifecycle
/// parity corpus. MELPA built this archive from upstream commit
/// `07f9915e08342410b933145d7934998709753a29`.
pub const AUTO_COMPLETE_MELPA_PIN: (&str, &str) = ("auto-complete", "20251231.1622");

/// The exact auto-complete-auctex package selected by the comprehensive
/// argument-expansion, candidate, action, source, setup, and real LaTeX
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `855633f668bcc4b9408396742a7cb84e0c4a2f77`.
pub const AUTO_COMPLETE_AUCTEX_MELPA_PIN: (&str, &str) = ("auto-complete-auctex", "20140223.1758");

/// The exact AUCTeX release selected from GNU ELPA for the
/// auto-complete-auctex integration parity corpus.
pub const AUCTEX_GNU_ELPA_PIN: (&str, &str) = ("auctex", "14.1.2");

/// The exact auto-complete-c-headers package selected by the comprehensive
/// include-path, filesystem-cache, documentation, candidate-source, and
/// completion workflow parity corpus. MELPA built this archive from upstream
/// commit `52fef720c6f274ad8de52bef39a343421006c511`.
pub const AUTO_COMPLETE_C_HEADERS_MELPA_PIN: (&str, &str) =
    ("auto-complete-c-headers", "20150912.323");

/// The exact auto-complete-chunk package selected by the comprehensive
/// chunk-boundary, candidate, source, dictionary, and practical completion
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `a9aa77ffb84a1037984a7ce4dda25074272f13fe`.
pub const AUTO_COMPLETE_CHUNK_MELPA_PIN: (&str, &str) = ("auto-complete-chunk", "20140225.946");

/// The exact auto-complete-clang package selected by the comprehensive output
/// parsing, compiler invocation, language/argument, documentation, template,
/// and completion workflow parity corpus. MELPA built this archive from
/// upstream commit `a195db1d0593b4fb97efe50885e12aa6764d998c`.
pub const AUTO_COMPLETE_CLANG_MELPA_PIN: (&str, &str) = ("auto-complete-clang", "20140409.752");

/// The exact auto-complete-clang-async package selected by the comprehensive
/// completion parsing, template, client/server protocol, asynchronous process,
/// syntax-check, and C/C++ workflow parity corpus. MELPA built this archive
/// from upstream commit `a5114e3477793ccb9420acc5cd6a1cb26be65964`.
pub const AUTO_COMPLETE_CLANG_ASYNC_MELPA_PIN: (&str, &str) =
    ("auto-complete-clang-async", "20130526.1527");

/// The exact auto-complete-distel package selected by the comprehensive
/// prefix, source, Distel bridge, documentation, and practical Erlang
/// completion workflow parity corpus. MELPA built this archive from upstream
/// commit `acc4c0a5521904203d797fe96b08e5fae4233c7e`.
pub const AUTO_COMPLETE_DISTEL_MELPA_PIN: (&str, &str) = ("auto-complete-distel", "20180827.1344");

/// The exact companion Distel completion library required by
/// `AUTO_COMPLETE_DISTEL_MELPA_PIN`. MELPA built both archives from the same
/// upstream commit `acc4c0a5521904203d797fe96b08e5fae4233c7e`.
/// The exact Distel Completion Lib package selected for the practical Erlang
/// source indexing and completion workflow corpus, and as auto-complete-distel's
/// completion-library dependency. MELPA built this archive from upstream
/// commit `acc4c0a5521904203d797fe96b08e5fae4233c7e`.
pub const DISTEL_COMPLETION_LIB_MELPA_PIN: (&str, &str) =
    ("distel-completion-lib", "20180827.1344");

/// The exact auto-complete-exuberant-ctags package selected by the
/// comprehensive tag discovery, index parsing, candidate, hook, and practical
/// project workflow parity corpus. MELPA built this archive from upstream
/// commit `ff6121ff8b71beb5aa606d28fd389c484ed49765`.
pub const AUTO_COMPLETE_EXUBERANT_CTAGS_MELPA_PIN: (&str, &str) =
    ("auto-complete-exuberant-ctags", "20140320.724");

/// The exact auto-complete-nxml package selected by the comprehensive context,
/// candidate, documentation, namespace, action, and practical nXML workflow
/// parity corpus. MELPA built this archive from upstream commit
/// `ac7b09a23e45f9bd02affb31847263de4180163a`.
pub const AUTO_COMPLETE_NXML_MELPA_PIN: (&str, &str) = ("auto-complete-nxml", "20140221.458");

/// The exact auto-complete-pcmp package selected by the comprehensive
/// programmable-completion capture, action, advice, error, and practical
/// command workflow parity corpus. MELPA built this archive from upstream
/// commit `2595d3dab1ef3549271ca922f212928e9d830eec`.
pub const AUTO_COMPLETE_PCMP_MELPA_PIN: (&str, &str) = ("auto-complete-pcmp", "20140303.255");

/// The exact biblio-core package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `ee52f6cda82ea6fbc3b400e7b12132595cc0374c`.
pub const BIBLIO_CORE_MELPA_PIN: (&str, &str) = ("biblio-core", "20230202.1721");

/// The exact biblio package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `bb9d6b4b962fb2a4e965d27888268b66d868766b`.
pub const BIBLIO_MELPA_PIN: (&str, &str) = ("biblio", "20250812.1408");

/// The exact caml package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `744333dc4c4bd8b93e037efa8f7362b0903b96a2`.
pub const CAML_MELPA_PIN: (&str, &str) = ("caml", "20250227.1734");

/// The exact cape package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `96c26eb54ef27c404554272489b8f9d78f113a2b`.
pub const CAPE_MELPA_PIN: (&str, &str) = ("cape", "20260804.2303");

/// The exact company-auctex package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `9400a2ec7459dde8cbf1a5d50dfee4e300ed7e18`.
pub const COMPANY_AUCTEX_MELPA_PIN: (&str, &str) = ("company-auctex", "20200529.1835");

/// The exact company-go package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `31948b463f2fc18f8801e5a8fe511fef300eb3dd`.
pub const COMPANY_GO_MELPA_PIN: (&str, &str) = ("company-go", "20170825.1643");

/// The exact darktooth-theme package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `998639b2ce629dbdc0901ed560371f82de7af490`.
pub const DARKTOOTH_THEME_MELPA_PIN: (&str, &str) = ("darktooth-theme", "20251019.304");

/// The exact dart-mode package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `793d7bcc18a2636ebafe06450356c08ea6d638ca`.
pub const DART_MODE_MELPA_PIN: (&str, &str) = ("dart-mode", "20260529.1840");

/// The exact default-text-scale package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `bfc0987c37e93742255d3b23d86c17096fda8e7e`.
pub const DEFAULT_TEXT_SCALE_MELPA_PIN: (&str, &str) = ("default-text-scale", "20191226.2234");

/// The exact dired-hacks-utils package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `63b04d17936c98cb4ad7ce6bc3331cda8e30c55a`.
pub const DIRED_HACKS_UTILS_MELPA_PIN: (&str, &str) = ("dired-hacks-utils", "20240629.1906");

/// The exact disable-mouse package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `93a55a6453f34049375f97d3cf817b4e6db46f25`.
pub const DISABLE_MOUSE_MELPA_PIN: (&str, &str) = ("disable-mouse", "20240604.900");

/// The exact disaster package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `0299c129d4153e3a794358159737c3ff9d155654`.
pub const DISASTER_MELPA_PIN: (&str, &str) = ("disaster", "20250828.2224");

/// The exact easy-kill package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `98cbae5d8c378ad14d612d7c88a78484c49a80b8`.
pub const EASY_KILL_MELPA_PIN: (&str, &str) = ("easy-kill", "20260121.752");

/// The exact elm-mode package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `90b72cd2c9bc4506f531bcdcd73fa2530d9f4f7c`.
pub const ELM_MODE_MELPA_PIN: (&str, &str) = ("elm-mode", "20250401.915");

/// The exact flycheck-guile package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `dd7bbdc48fd21cf8d270c913c56cd580f8ec3d03`.
pub const FLYCHECK_GUILE_MELPA_PIN: (&str, &str) = ("flycheck-guile", "20230405.1154");

/// The exact gruber-darker-theme package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `2e9f99c41fe8ef0557e9ea0f3b94ef50c68b5557`.
pub const GRUBER_DARKER_THEME_MELPA_PIN: (&str, &str) = ("gruber-darker-theme", "20231026.2031");

/// The exact ido-completing-read+ package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `1609049c0a9b3f674ffff3083adc8f5359746fa9`.
pub const IDO_COMPLETING_READ_PLUS_MELPA_PIN: (&str, &str) =
    ("ido-completing-read+", "20240130.30");

/// The exact importmagic package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `e32ee9f6a5eef937b76eba82fdae8bae85d18088`.
pub const IMPORTMAGIC_MELPA_PIN: (&str, &str) = ("importmagic", "20180520.303");

/// The exact jedi-core package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `94a031d54c55d22aa36ad557f45c972cb3f5833b`.
pub const JEDI_CORE_MELPA_PIN: (&str, &str) = ("jedi-core", "20250602.2109");

/// The exact jedi package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `0a92f57dcfd76f1daf6d382d1e2eb437784a71e0`.
pub const JEDI_MELPA_PIN: (&str, &str) = ("jedi", "20250602.2107");

/// The exact kaolin-themes package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `432c6672b16e867ec40eaf312d2fbbeb38673fa9`.
pub const KAOLIN_THEMES_MELPA_PIN: (&str, &str) = ("kaolin-themes", "20260619.2211");

/// The exact less-css-mode package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `59bf174c4e9f053ec2a7ef8c8a8198490390f6fb`.
pub const LESS_CSS_MODE_MELPA_PIN: (&str, &str) = ("less-css-mode", "20161001.453");

/// The exact log4e dependency selected for the auto-complete-pcmp corpus and
/// by the practical logger lifecycle, formatting, messaging, navigation, and
/// source-instrumentation parity corpus. MELPA built this archive from
/// upstream commit `6d71462df9bf595d3861bfb328377346aceed422`.
pub const LOG4E_MELPA_PIN: (&str, &str) = ("log4e", "20240123.1313");

/// The exact logito package selected for practical EIEIO level filtering and
/// buffer logger insertion without interactive messaging. MELPA built this
/// archive from upstream commit `d5934ce10ba3a70d3fcfb94d742ce3b9136ce124`.
pub const LOGITO_MELPA_PIN: (&str, &str) = ("logito", "20201226.534");

/// The exact lsp-ivy package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `c0930544948dfdb7bf497fc9e58aa6b4b857e237`.
pub const LSP_IVY_MELPA_PIN: (&str, &str) = ("lsp-ivy", "20260507.1752");

/// The exact magit-todos package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `7294a95580bddf7232f2d205efae312dc24c5f61`.
pub const MAGIT_TODOS_MELPA_PIN: (&str, &str) = ("magit-todos", "20250928.1611");

/// The exact noflet package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `7ae84dc3257637af7334101456dafe1759c6b68a`.
pub const NOFLET_MELPA_PIN: (&str, &str) = ("noflet", "20141102.1454");

/// The exact nov package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `874daf5e4791a6d4f47741422c80e2736e907351`.
pub const NOV_MELPA_PIN: (&str, &str) = ("nov", "20251213.1501");

/// The exact org-journal package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `6460f6f2b0835b4b8aa87d5fdf40cac7deb319f5`.
pub const ORG_JOURNAL_MELPA_PIN: (&str, &str) = ("org-journal", "20260413.1401");

/// The exact ov package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `e2971ad986b6ac441e9849031d34c56c980cf40b`.
pub const OV_MELPA_PIN: (&str, &str) = ("ov", "20230522.1117");

/// The exact powershell package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `ae60e11c96cc1767f05ce0cab6a917240ce2e37a`.
pub const POWERSHELL_MELPA_PIN: (&str, &str) = ("powershell", "20251122.1430");

/// The exact prescient package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `5649977fa7789e4615efeca09397ed7eccd06dfc`.
pub const PRESCIENT_MELPA_PIN: (&str, &str) = ("prescient", "20260628.2243");

/// The exact Python Environment package pinned as the jedi-core dependency.
/// MELPA built this archive from upstream commit `401006584e32864a10c69d29f14414828909362e`.
pub const PYTHON_ENVIRONMENT_MELPA_PIN: (&str, &str) = ("python-environment", "20150310.853");

/// The exact request-deferred package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `38ed1d2e64138eb16a9d8ed2987cff2e01b4a93b`.
pub const REQUEST_DEFERRED_MELPA_PIN: (&str, &str) = ("request-deferred", "20220614.1604");

/// The exact rjsx-mode package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `0061587a06cdc2579a8d0e90863498d96bf982d8`.
pub const RJSX_MODE_MELPA_PIN: (&str, &str) = ("rjsx-mode", "20200224.2149");

/// The exact scratch package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `f000648c9663833a76a8de9b1e78c99a9d698e48`.
pub const SCRATCH_MELPA_PIN: (&str, &str) = ("scratch", "20220319.1705");

/// The exact session package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `3be207c50dfe964de3cbf5cd8fa9b07fc7d2e609`.
pub const SESSION_MELPA_PIN: (&str, &str) = ("session", "20210422.53");

/// The exact sphinx-doc package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `1eda612a44ef027e5229895daa77db99a21b8801`.
pub const SPHINX_DOC_MELPA_PIN: (&str, &str) = ("sphinx-doc", "20210213.1250");

/// The exact string-edit-at-point package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `87936d816ae24184dd83688136531b6b6f1943fe`.
pub const STRING_EDIT_AT_POINT_MELPA_PIN: (&str, &str) = ("string-edit-at-point", "20230118.1933");

/// The exact sublime-themes package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `60ee40af82eb55b79d5ed4026f1911326311603f`.
pub const SUBLIME_THEMES_MELPA_PIN: (&str, &str) = ("sublime-themes", "20170606.1844");

/// The exact switch-window package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `1ccbfa53df499cb31d5ebbe21306cdcc6b06c135`.
pub const SWITCH_WINDOW_MELPA_PIN: (&str, &str) = ("switch-window", "20260316.257");

/// The exact tao-theme package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `33c0d44048afe444e7a8aee30fbc101a00453799`.
pub const TAO_THEME_MELPA_PIN: (&str, &str) = ("tao-theme", "20250717.347");

/// The exact Yaxception package selected as an auto-complete-pcmp dependency
/// and by the practical custom-error, catch-selection, nested rethrow, finally,
/// error-wrapping, and stack-trace parity corpus. MELPA built this archive from
/// upstream commit `5941de88b19752c14e0dce0d2bf562b1288055a0`.
pub const YAXCEPTION_MELPA_PIN: (&str, &str) = ("yaxception", "20240107.504");

/// The exact auto-complete-rst package selected by the comprehensive source
/// generation, directive/option parsing, command, setup, and practical
/// reStructuredText workflow parity corpus. MELPA built this archive from
/// upstream commit `4803ce41a96224e6fa54e6741a5b5f40ebed7351`.
pub const AUTO_COMPLETE_RST_MELPA_PIN: (&str, &str) = ("auto-complete-rst", "20140225.944");

/// The exact auto-complete-sage package selected by the comprehensive
/// documentation-cache, REPL, edit-buffer, source, setup, and practical Sage
/// completion workflow parity corpus. MELPA built this archive from upstream
/// commit `51b8e3905196d266e1f8aa47881189833151b398`.
pub const AUTO_COMPLETE_SAGE_MELPA_PIN: (&str, &str) = ("auto-complete-sage", "20160514.751");

/// The exact current sage-shell-mode dependency selected for the
/// auto-complete-sage integration corpus. MELPA built this archive from
/// upstream commit `bb59cd559a9d7639d9ef16addbb0809ea4790392`.
pub const SAGE_SHELL_MODE_MELPA_PIN: (&str, &str) = ("sage-shell-mode", "20260523.1504");

/// The exact Deferred package selected for the practical asynchronous order,
/// recovery, ledger, parallel aggregation, and subprocess parity corpus, and
/// as the sage-shell-mode package graph dependency. MELPA built this archive
/// from upstream commit `2239671d94b38d92e9b28d4e12fd79814cfb9c16`.
pub const DEFERRED_MELPA_PIN: (&str, &str) = ("deferred", "20170901.1330");

/// The exact Concurrent package selected for generator, pseud-thread,
/// semaphore, dataflow, parent-environment, signal routing, disconnect, and
/// failure parity. MELPA built this archive from upstream commit
/// `d012a1ab50edcc2c44e3e49006f054dbff47cb6c`.
pub const CONCURRENT_MELPA_PIN: (&str, &str) = ("concurrent", "20170601.435");

/// The exact EditorConfig package selected for inherited project rules,
/// visiting and saving real files, local-variable precedence, coding-system,
/// and configuration-cache parity. MELPA built this archive from upstream
/// commit `1e9931d5f38a8d8cb8a92cf726d64696550bfc95`.
pub const EDITORCONFIG_MELPA_PIN: (&str, &str) = ("editorconfig", "20260118.718");

/// The exact EIN package selected for practical notebook parsing, rendered
/// cell editing, output lifecycle, serialization, notebook-list UI, and
/// failure-recovery parity. MELPA built this archive from upstream commit
/// `8fa836fcd1c22f45d36249b09590b32a890f2b9e`.
pub const EIN_MELPA_PIN: (&str, &str) = ("ein", "20251212.1623");

/// The exact Move Text package selected for practical line and active-region
/// reordering, boundary, narrowing, undo, point/mark, and global-key parity.
/// MELPA built this archive from upstream commit
/// `142890cfb46d9c374113b4b49021a4202033147b`.
pub const MOVE_TEXT_MELPA_PIN: (&str, &str) = ("move-text", "20260508.508");

/// The exact MMM Mode package selected for ERB parsing, submode transitions,
/// fontification, regexp/region management, narrowing, indentation dispatch,
/// mode-extension activation, cleanup, and invalid-class errors. MELPA built

/// The exact multi-term package selected for practical terminal buffer list
/// management, next/prev switching, dedicated window open/close/toggle, and
/// buffer naming. MELPA built this archive from upstream commit
/// `017c77c550115936860e2ea71b88e585371475d5`.
pub const MULTI_TERM_MELPA_PIN: (&str, &str) = ("multi-term", "20200514.428");

/// this archive from upstream commit `b1f5c7dbdc405e6e10d9ddd99a43a6b2ad61b176`.
pub const MMM_MODE_MELPA_PIN: (&str, &str) = ("mmm-mode", "20240222.428");

/// The exact modus-themes package selected for practical theme catalog loading,
/// light/dark toggle, palette lookup, contrast measurement, and background-mode
/// sorting. MELPA built this archive from upstream commit
/// `75aa3fa79efd04ddf7980a1d3ec0cef6e4f4af90`.
pub const MODUS_THEMES_MELPA_PIN: (&str, &str) = ("modus-themes", "20260730.719");

/// The exact Elixir Mode package selected by the practical indentation,
/// fontification, navigation, documentation, and formatter workflow corpus.
/// MELPA built this archive from upstream commit
/// `00d6580a040a750e019218f9392cf9a4c2dac23a`.
pub const ELIXIR_MODE_MELPA_PIN: (&str, &str) = ("elixir-mode", "20230626.1738");

/// The exact Elpy package selected for practical Python project discovery,
/// editing, multi-edit, test command, module navigation, and mode-lifecycle
/// parity. MELPA built this version from upstream commit
/// `9cdf26dfea1cb044b3cf1dfa9755b6479bfd9a1c`.
pub const ELPY_MELPA_PIN: (&str, &str) = ("elpy", "20260715.1747");

/// The exact Emmet Mode package selected by the practical HTML, CSS, JSX,
/// preview, wrapping, and edit-point workflow corpus. MELPA built this archive
/// from upstream commit `322d3bb112fced57d63b44863357f7a0b7eee1e3`.
pub const EMMET_MODE_MELPA_PIN: (&str, &str) = ("emmet-mode", "20240617.45");

/// The exact emr package selected for practical Elisp free-variable analysis,
/// let-form toggle, eval-and-replace, and declared-refactor availability.
/// MELPA built this archive from upstream commit
/// `cac1b52932926f56d7f6d2923732d20bbd20670d`.
pub const EMR_MELPA_PIN: (&str, &str) = ("emr", "20220108.548");

/// The exact EPC package selected for practical local RPC framing, method
/// registration, synchronous and deferred calls, method discovery, application
/// and protocol errors, Unicode payloads, and connection lifecycle parity.
/// MELPA built this archive from upstream commit
/// `94cd36a3bec752263ac9b1b3a9dd2def329d2af7`.
pub const EPC_MELPA_PIN: (&str, &str) = ("epc", "20140610.534");

/// The exact eshell-prompt-extras package selected for practical path
/// abbreviation, fish-style trimming, status formatting, and newline trimming
/// without a live remote/git prompt. MELPA built this archive from upstream
/// commit `36504072605a2044cf291d1c2ea987cb898c6394`.
pub const ESHELL_PROMPT_EXTRAS_MELPA_PIN: (&str, &str) = ("eshell-prompt-extras", "20260402.1141");

/// The exact eshell-z package selected for real Eshell navigation, ranking,
/// persistence, completion, failure, and cleanup parity. MELPA built this
/// archive from upstream commit `337cb241e17bd472bd3677ff166a0800f684213c`.
pub const ESHELL_Z_MELPA_PIN: (&str, &str) = ("eshell-z", "20191116.333");

/// The exact EPL package selected by the practical package metadata,
/// descriptor, database, installation, deletion, and built-in discovery
/// workflow corpus. MELPA built this archive from upstream commit
/// `78ab7a85c08222cd15582a298a364774e3282ce6`.
pub const EPL_MELPA_PIN: (&str, &str) = ("epl", "20180205.2049");

/// The exact ert-runner package selected for practical test path expansion,
/// selector composition, project scaffolding, reporter loading, and batch
/// runner hooks. MELPA built this archive from upstream commit
/// `98a5a6f683663f9f0357459d75ce1dc36c987e4a`.
pub const ERT_RUNNER_MELPA_PIN: (&str, &str) = ("ert-runner", "20231110.1358");

/// The exact Erlang Mode package selected by the practical OTP module
/// editing, semantic fontification, navigation, EDoc, skeleton, identifier,
/// and compile-option workflow corpus. MELPA built this archive from upstream
/// OTP commit `1259612946cb36a8bf9614b289090bb32fbcbeb2`.
pub const ERLANG_MELPA_PIN: (&str, &str) = ("erlang", "20260724.1508");

/// The exact GNU ELPA let-alist dependency selected for the flycheck-rust and
/// sage-shell-mode package graphs.
pub const LET_ALIST_GNU_ELPA_PIN: (&str, &str) = ("let-alist", "1.0.6");

/// The exact audio-notes-mode package selected by the comprehensive
/// filesystem, playback, process-control, mode-line, advice, and global-mode
/// lifecycle parity corpus. MELPA built this archive from upstream commit
/// `fa38350829c7e97257efc746a010471d33748a68`.
pub const AUDIO_NOTES_MODE_MELPA_PIN: (&str, &str) = ("audio-notes-mode", "20170611.2159");

/// The exact australia-holidays package selected by the comprehensive
/// national, state, territory, customization, date-calculation, and calendar
/// integration parity corpus. MELPA built this archive from upstream commit
/// `a73bbc940bc953164b8ed77e61e65a7a3aff4da5`.
pub const AUSTRALIA_HOLIDAYS_MELPA_PIN: (&str, &str) = ("australia-holidays", "20250706.1213");

/// The exact auth-source-kwallet package selected by the comprehensive
/// backend, process, cache, customization, and authentication-workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `1e1bff2403966c3a0683ee65fb28cb8d8ff2c389`.
pub const AUTH_SOURCE_KWALLET_MELPA_PIN: (&str, &str) = ("auth-source-kwallet", "20250419.1330");

/// The exact popup dependency selected for the auto-complete parity corpus and
/// practical text filling, rich-item, row-layout, filtering, rendering,
/// navigation, cascade-menu, and lifecycle parity. MELPA built this archive
/// from upstream commit
/// `45a0b759076ce4139aba36dde0a2904136282e73`.
pub const POPUP_MELPA_PIN: (&str, &str) = ("popup", "20251231.1622");

/// The exact Popwin package selected for practical multi-pane restoration,
/// directional popup, display routing, nested context, sticky and dedicated
/// lifecycle, tail, reuse, and killed/buried-buffer parity. MELPA built this
/// archive from upstream commit `f7a39759180fa88f3890c3c5f35379ab086e04fa`.
pub const POPWIN_MELPA_PIN: (&str, &str) = ("popwin", "20260103.1800");

/// The exact pos-tip package selected by the practical text layout, pixel
/// geometry, color selection, display-bound clamping, and tooltip dispatch
/// parity corpus. MELPA built this archive from upstream commit
/// 4889e08cf9077c8589ea6fea4e2ce558614dfcde.
pub const POS_TIP_MELPA_PIN: (&str, &str) = ("pos-tip", "20240209.837");

/// The exact posframe package selected by the practical child-frame
/// orchestration, frame-parameter, position-handler, sizing, movement, mouse,
/// timer, and lifecycle parity corpus. MELPA built this archive from upstream
/// commit 74c8c56131ed866db47ae4191364b72dd4852456.
pub const POSFRAME_MELPA_PIN: (&str, &str) = ("posframe", "20260527.857");

/// The exact cfrs package selected by the practical terminal fallback,
/// child-frame prompt, editing, sizing, acceptance, cancellation, focus, and
/// lifecycle parity corpus. MELPA built this archive from upstream commit
/// `981bddb3fb9fd9c58aed182e352975bd10ad74c8`.
pub const CFRS_MELPA_PIN: (&str, &str) = ("cfrs", "20250729.1422");

/// The exact powerline package selected by the practical segmented mode-line,
/// face/property, alignment, separator, scroll-HUD, memoization, selection,
/// mouse-action, and theme lifecycle parity corpus. MELPA built this archive
/// from upstream commit c35c35bdf5ce2d992882c1f06f0f078058870d4a.
pub const POWERLINE_MELPA_PIN: (&str, &str) = ("powerline", "20221110.1956");

/// The exact promise package selected by the practical asynchronous-chain,
/// resolver, aggregation, thenable, timer, rejection-tracking, bounded-worker,
/// and subprocess parity corpus. MELPA built this archive from upstream commit
/// cec51feb5f957e8febe6325335cf57dc2db6be30.
pub const PROMISE_MELPA_PIN: (&str, &str) = ("promise", "20210307.727");

/// The exact pyim package selected by the practical Chinese composition,
/// candidate, conversion, search, punctuation, page, dictionary, and
/// input-method lifecycle parity corpus. MELPA built this archive from
/// upstream commit a56c8d992c872addcfc295c409a7bae70d00af87.
pub const PYIM_MELPA_PIN: (&str, &str) = ("pyim", "20251230.809");

/// The exact pythonic package selected by the practical path-alias, Tramp
/// connection, Docker Compose, synchronous/asynchronous process, and
/// virtual-environment parity corpus. MELPA built this archive from upstream

/// The exact pyenv-mode package selected for practical version set/unset,
/// mode-line indication, keymap, and pythonic activation parity. MELPA built
/// this archive from upstream commit `6820aa6673e6a51ace88611a58b423b5b1effb19`.
pub const PYENV_MODE_MELPA_PIN: (&str, &str) = ("pyenv-mode", "20230821.1645");

/// The exact Python Mode package selected for practical editing, structural
/// navigation, completion, and interpreter-process parity. MELPA built this
/// archive from upstream commit
/// `dbbfaa9bbfa1e330f4d9ec81b3793fbb2a297ecd`.
pub const PYTHON_MODE_MELPA_PIN: (&str, &str) = ("python-mode", "20260710.1059");

/// commit f6e0bec552319341f260a5c4740288799c2b3a5b.
pub const PYTHONIC_MELPA_PIN: (&str, &str) = ("pythonic", "20230821.1733");

/// The exact Pyvenv package selected for practical activation, executable
/// dispatch, environment restoration, workon discovery, directory tracking,
/// mode-line, hook ordering, switching, and failure-atomicity parity. MELPA
/// built this archive from upstream commit
/// `31ea715f2164dd611e7fc77b26390ef3ca93509b`.
pub const PYVENV_MELPA_PIN: (&str, &str) = ("pyvenv", "20211014.707");

/// The exact Queue release selected from GNU ELPA by the practical scheduler,
/// priority, copy, iterator, traversal, event-loop, and live-view parity
/// corpus.
pub const QUEUE_GNU_ELPA_PIN: (&str, &str) = ("queue", "0.2");

/// The exact Quelpa package selected for practical source checkout, package
/// build/install, dependency, upgrade, cache, async, and failure-state parity.
/// MELPA built this archive from upstream commit
/// `cf01224edd82920a0fb8a90568d2e14347354fc8`.
pub const QUELPA_MELPA_PIN: (&str, &str) = ("quelpa", "20250113.1906");

/// The exact Quickrun package selected for current-buffer and region execution,
/// replacement, arguments, working directory, stdin sidecars, multi-outputters,
/// cleanup, hooks, process failures, and registration errors. MELPA built this
/// archive from upstream commit `9199e222f95104ee83e115a9d5ac159d86816706`.
pub const QUICKRUN_MELPA_PIN: (&str, &str) = ("quickrun", "20260103.1800");

/// The exact racer package selected by the practical subprocess, completion,
/// protocol, documentation, navigation, placeholder, Eldoc, and diagnostics
/// parity corpus. MELPA built this archive from upstream commit
/// `1e63e98626737ea9b662d4a9b1ffd6842b1c648c`.
pub const RACER_MELPA_PIN: (&str, &str) = ("racer", "20210307.243");

/// The exact reformatter package selected by the practical generated-command,
/// region, file-backed, working-directory, exit-policy, diagnostics, save-hook,
/// and temporary-file safety parity corpus. MELPA built this archive from
/// upstream commit `f2cb59466b1c3f85a8c960f7d4b7b7ead015bedc`.
pub const REFORMATTER_MELPA_PIN: (&str, &str) = ("reformatter", "20241204.1051");

/// The exact request package selected by the practical request-building,
/// response-header, callback, redirect, curl-command, cookie, timeout/abort,
/// and local-file transport parity corpus. MELPA built this archive from
/// upstream commit `6f419b5cdd2dfa83675ae53f04d8463d00a533f8`.
pub const REQUEST_MELPA_PIN: (&str, &str) = ("request", "20250219.2213");

/// The exact RSpec Mode package selected for practical source/spec navigation,
/// pending-example editing, compilation, failure navigation, rerun, and
/// session-state parity. MELPA built this archive from upstream commit
/// `b5d48de9b56a0070d7a0d3e642b139992a1ce3f0`.
pub const RSPEC_MODE_MELPA_PIN: (&str, &str) = ("rspec-mode", "20260618.548");

/// The exact rtags package selected by the practical rc transport,
/// navigation, source-extraction, diagnostics, result-buffer, and completion
/// parity corpus. MELPA built this archive from upstream commit
/// `b0bd2b276f810a291f08c05ba2860ca07285a2eb`.
pub const RTAGS_MELPA_PIN: (&str, &str) = ("rtags", "20260727.1603");

/// The exact rust-mode package selected by the practical syntax, indentation,
/// font-lock, navigation, documentation, editing, Cargo, and rustfmt parity
/// corpus, and used as a racer dependency. MELPA built this archive from
/// upstream commit `0058837c048cc031ca1a13f598a6a6604777458b`.
pub const RUST_MODE_MELPA_PIN: (&str, &str) = ("rust-mode", "20260725.1442");

/// The exact Rustic package selected for practical mode, Cargo command,
/// test-at-point, compilation navigation, formatting, and recovery parity.
/// MELPA built this archive from upstream commit
/// `b6c7e095145bb1fd0dc9cfb90ce36884e944556d`.
pub const RUSTIC_MELPA_PIN: (&str, &str) = ("rustic", "20260407.1712");

/// The exact Scala Mode package used to exercise Multi-Line's advertised
/// Scala editing route with the real language syntax and indentation engine.
/// MELPA built this archive from upstream commit
/// `50bcafa181baec7054e27f4bca55d5f9277c6350`.
pub const SCALA_MODE_MELPA_PIN: (&str, &str) = ("scala-mode", "20260118.942");

/// The exact shell-maker package selected by the practical comint session,
/// streaming, validation, interruption, history, transcript, process, and UI
/// parity corpus. MELPA built this archive from upstream commit
/// `679cfbc02e206e0a702048cfd7c663eb5c9d1059`.
pub const SHELL_MAKER_MELPA_PIN: (&str, &str) = ("shell-maker", "20260727.1508");

/// The exact Shift Number dependency used by the Evil Numbers parity corpus.
/// MELPA built this archive from upstream commit
/// `d5e8bece6e6ab21ad5a93330d49b2554e9eb72a9`.
pub const SHIFT_NUMBER_MELPA_PIN: (&str, &str) = ("shift-number", "20260620.1211");

/// The exact simple-httpd package selected by the practical request parsing,
/// servlet routing, response generation, static-file, and live loopback server
/// parity corpus. MELPA built this archive from upstream commit
/// `ceb208f96601be09397fc9e64fa96014ac1c8739`.
pub const SIMPLE_HTTPD_MELPA_PIN: (&str, &str) = ("simple-httpd", "20260623.1110");

/// The exact skewer-mode package selected by the practical browser queue,
/// JavaScript, CSS, HTML, hosted-script, REPL, error-reporting, and client
/// lifecycle parity corpus. MELPA built this archive from upstream commit
/// `e5bed351939c92a1f788f78398583c2f83f1bb3c`.
pub const SKEWER_MODE_MELPA_PIN: (&str, &str) = ("skewer-mode", "20200304.1142");

/// The exact SLIME package selected by the practical editor and protocol
/// parity corpus. MELPA built this archive from upstream commit
/// `055c1c98c2b7791162b0e8c994051a7d72208dc1`.
pub const SLIME_MELPA_PIN: (&str, &str) = ("slime", "20260719.420");

/// The exact SLY package selected by the practical editor, Slynk protocol,
/// MREPL, apropos, inspector, and debugger parity corpus. MELPA built this
/// archive from upstream commit `759c0ff8741ced8793257f2b7ed95a23e13e1407`.
pub const SLY_MELPA_PIN: (&str, &str) = ("sly", "20260402.2249");

/// The exact Spaceline package selected by the practical theme, custom
/// segment, responsive layout, selection, and mode-line lifecycle parity
/// corpus. MELPA built this archive from upstream commit
/// `086420d16e526c79b67fc1edec4c2ae1e699f372`.
pub const SPACELINE_MELPA_PIN: (&str, &str) = ("spaceline", "20230922.1127");

/// The exact Spaceline All The Icons package selected by the practical
/// installed-theme, mode-line interaction, customization, and failure-
/// diagnosis parity corpus. MELPA built this archive from upstream commit
/// `5afd48c10f1bd42d9b9648c5e64596b72f3e9042`.
pub const SPACELINE_ALL_THE_ICONS_MELPA_PIN: (&str, &str) =
    ("spaceline-all-the-icons", "20190325.1602");

/// The exact Spinner package selected by the practical mode-line, timer,
/// delayed-animation, multi-buffer, and lifecycle parity corpus. MELPA built
/// this archive from upstream commit `bca794fa6f6b007292cdac9b0a850a3711986db5`.
pub const SPINNER_MELPA_PIN: (&str, &str) = ("spinner", "1.7.4");

/// The exact Splitter package selected by the practical split sizing, grids,
/// nested layouts, round trips, buffer restoration, and layout-shrinking
/// parity corpus. MELPA built this archive from upstream commit
/// `6bdb51e9a346907d60a9625f6180bddd06be6674`.
pub const SPLITTER_MELPA_PIN: (&str, &str) = ("splitter", "20170809.2208");

/// The exact svg-lib package selected by the practical themed status widgets,
/// cached icons, composition, dates, and interactive button parity corpus.
/// MELPA built this archive from upstream commit
/// `f2cc9615ef3a052747135d34f31c423a26592f14`.
pub const SVG_LIB_MELPA_PIN: (&str, &str) = ("svg-lib", "0.3");

/// The exact Tablist package selected by the practical marking, filtering,
/// sorting, resizing, editing, operations, and CSV export parity corpus.
/// MELPA built this archive from upstream commit
/// `01f065e387ffe6b7a41f180f257cd12551c7a9c2`.
pub const TABLIST_MELPA_PIN: (&str, &str) = ("tablist", "20260623.1855");

/// The exact Tern package selected for practical JavaScript completion,
/// argument hints, type/documentation lookup, cross-file navigation,
/// refactoring, reference highlighting, transport, and failure parity.
/// MELPA built this archive from upstream commit
/// `fab80daebd798b233a9a40d5a8b99359ace63b5e`.
pub const TERN_MELPA_PIN: (&str, &str) = ("tern", "20260514.1348");

/// The exact Terraform Mode package selected for practical mode, font-lock,
/// indentation, Imenu, outline, documentation, formatting, and save lifecycle
/// parity. MELPA built this archive from upstream commit
/// `01635df3625c0cec2bb4613a6f920b8569d41009`.
pub const TERRAFORM_MODE_MELPA_PIN: (&str, &str) = ("terraform-mode", "20251115.2210");

/// The exact Tide package selected for practical JavaScript setup, external
/// TypeScript-server lifecycle, Eldoc/Imenu/Xref navigation, references,
/// Flycheck diagnostics, code edits, rename, framing, and recovery parity.
/// MELPA built this archive from upstream commit
/// `9498c4c7fc97d8042fdff532f47f1dc79ebd163a`.
pub const TIDE_MELPA_PIN: (&str, &str) = ("tide", "20260219.336");

/// The exact TinySegmenter package selected by the practical Japanese text,
/// mixed-script release-note, document-indexing, buffer-editing, and boundary
/// parity corpus. MELPA built this archive from upstream commit
/// `872134704bd25c13a4c59552433da4c6881b5230`.
pub const TINYSEGMENTER_MELPA_PIN: (&str, &str) = ("tinysegmenter", "20141124.1013");

/// The exact Toc-Org package selected for practical Org and Markdown table of
/// contents generation, filtering, anchor deduplication, link navigation,
/// save-hook updates, and mode-lifecycle parity. MELPA built this archive from
/// upstream commit `781376e9dc9a901116c0c39914aeb4d46e524e0a`.
pub const TOC_ORG_MELPA_PIN: (&str, &str) = ("toc-org", "20260514.1415");

/// The exact TypeScript Mode package selected for practical source editing,
/// indentation policy, syntax, fontification, template conversion, defun
/// navigation, comment filling, and compiler-diagnostic parity. MELPA built
/// this archive from upstream commit
/// `481df3ad2cdf569d8e6697679669ff6206fbd2f9`.
pub const TYPESCRIPT_MODE_MELPA_PIN: (&str, &str) = ("typescript-mode", "20250118.2056");

/// The exact Undo Tree package selected for practical linear undo/redo,
/// branching history, register restoration, persistent history, visualizer,
/// and mode lifecycle parity. MELPA removed its recipe on March 24, 2018
/// after the package moved to GNU ELPA, so this historical top-500 roadmap
/// entry uses the maintained upstream version 0.8.2 source at commit
/// `2bf5e230f1d11df7bbd9d8c722749e34482bc458`.
pub const UNDO_TREE_SOURCE_PIN: (&str, &str) = ("undo-tree", "0.8.2");

/// The exact yaml.el package selected by the practical deployment parsing,
/// anchors, scalar policy, round-trip encoding, dialect, source-position, and
/// validation parity corpus. MELPA built this archive from upstream commit
/// `5546f36bde24a9a8c1934e0f6ce205cd41d72537`.
pub const YAML_MELPA_PIN: (&str, &str) = ("yaml", "20260605.834");

/// The exact youdao-dictionary package selected by the practical signed API,
/// rendered lookup, replacement, presentation, credentials, and voice parity
/// corpus. MELPA built this archive from upstream commit
/// `eae8efb1efd3fc82cfe87a357fe8f764116d94ef`.
pub const YOUDAO_DICTIONARY_MELPA_PIN: (&str, &str) = ("youdao-dictionary", "20231005.1920");

/// The exact youdotcom package selected by the practical search/RAG session,
/// command lifecycle, request validation, and malformed-response parity
/// corpus. MELPA built this archive from upstream commit
/// `0b835f143e88c3321006a3e48ac5190d071b872c`.
pub const YOUDOTCOM_MELPA_PIN: (&str, &str) = ("youdotcom", "20240207.1853");

/// The exact youtube-music package selected by the practical status-buffer,
/// playback, search, authentication, library, rating, and radio parity corpus.
/// MELPA built this archive from upstream commit
/// `2a962d972d8a59fed718aec039c9c61ef3c0392d`.
pub const YOUTUBE_MUSIC_MELPA_PIN: (&str, &str) = ("youtube-music", "20260717.1039");

/// The exact youtube-sub-extractor package selected by the practical subtitle
/// extraction, language selection, timestamp navigation, and failure parity
/// corpus. MELPA built this archive from upstream commit
/// `d69f732299fdf256504e15767c1d7e5de771220e`.
pub const YOUTUBE_SUB_EXTRACTOR_MELPA_PIN: (&str, &str) = ("youtube-sub-extractor", "20221116.653");

/// The exact ytdious package selected by the practical Invidious search,
/// tabulated navigation, thumbnail, external playback, and failure parity
/// corpus. MELPA built this archive from upstream commit
/// 941460b51e43ef6764e15e2b9c4af54c3e56115f.
pub const YTDIOUS_MELPA_PIN: (&str, &str) = ("ytdious", "20210228.2111");

/// The exact ytdl package selected by the practical asynchronous download,
/// playlist, format-selection, download-list, and failure parity corpus.
/// MELPA built this archive from upstream commit
/// `309ad5ce95368ad2e35d1c1701a1f3c0043415a3`.
pub const YTDL_MELPA_PIN: (&str, &str) = ("ytdl", "20241025.1913");

/// The exact yuck-mode package selected by the practical file activation,
/// fontification, indentation, commenting, and structural editing parity
/// corpus. MELPA built this archive from upstream commit
/// `e084416fa3e7f91bb429edbf7ff1585aa5674367`.
pub const YUCK_MODE_MELPA_PIN: (&str, &str) = ("yuck-mode", "20230113.2304");

/// The exact xr package selected for the practical regexp translation,
/// round-trip matching, lint diagnostics, skip-set, pretty-printing, and parse
/// failure parity corpus, and as a dependency of the pyim parity corpus. MELPA
/// built this archive from upstream commit
/// `694defa220113d0acaa78fd646dcff9f1a08fad9`.
pub const XR_MELPA_PIN: (&str, &str) = ("xr", "2.2");

/// The exact xterm-color package selected for practical process-stream,
/// property-preservation, whole-buffer, overlay, and palette-cache parity.
/// MELPA built this archive from upstream commit
/// `0b0d808f8bc5007037341dc5f63149cc32cf2c5b`.
pub const XTERM_COLOR_MELPA_PIN: (&str, &str) = ("xterm-color", "20260531.1854");

/// The exact z3-mode package selected by the practical SMT-LIB editing,
/// solver execution, Flycheck diagnostics, and complete command-vocabulary
/// parity corpus. MELPA built this archive from upstream commit
/// `0356cbe1e1e2b780ba0ddb4aaa055fa246a67931`.
pub const Z3_MODE_MELPA_PIN: (&str, &str) = ("z3-mode", "20211116.138");

/// The exact zathura package selected by the practical PDF-link, document
/// viewing, process-session, outline, and global integration parity corpus.
/// MELPA built this archive from upstream commit
/// `874dadbf07e22811b6b309200cad32b4ccca0e51`.
pub const ZATHURA_MELPA_PIN: (&str, &str) = ("zathura", "20260603.1620");

/// The exact zeal-at-point package selected by the practical symbol, region,
/// docset completion, query protocol, process launch, and failure parity
/// corpus. MELPA built this archive from upstream commit
/// `0fc3263f44e95acd3e9d91057677621ce4d297ee`.
pub const ZEAL_AT_POINT_MELPA_PIN: (&str, &str) = ("zeal-at-point", "20180131.2354");

/// The exact zen-and-art-theme package selected by the practical editing,
/// selection, theme lifecycle, precedence, and legacy-face parity corpus.
/// MELPA built this archive from upstream commit
/// `a7226cbce0bca2501d69a620cb2aeabfc396c232`.
pub const ZEN_AND_ART_THEME_MELPA_PIN: (&str, &str) = ("zen-and-art-theme", "20120622.1437");

/// The exact zen-mode package selected by the practical file activation,
/// fontification, indentation, commenting, live-editing, and Imenu parity
/// corpus. MELPA built this archive from upstream commit
/// `c1b1806358f3cce6c04b30699987d82dc7d42559`.
pub const ZEN_MODE_MELPA_PIN: (&str, &str) = ("zen-mode", "20200609.822");

/// The exact zenburn-theme package selected by the practical editing,
/// palette customization, application-integration, theme lifecycle, and
/// invalid-configuration parity corpus. MELPA built this archive from upstream
/// commit `3797f3ae26b3649c99fc74a09a0bd6a31b40597f`.
pub const ZENBURN_THEME_MELPA_PIN: (&str, &str) = ("zenburn-theme", "20260725.707");

/// The exact zencoding-mode package selected by the practical nested-markup,
/// file-filter, editable-preview, acceptance, and abort parity corpus. MELPA
/// built this archive from upstream commit
/// `58e42af182c98cb9941d27cd042d227fbf4e146c`.
pub const ZENCODING_MODE_MELPA_PIN: (&str, &str) = ("zencoding-mode", "20140213.822");

/// The exact zenity-color-picker package selected by the practical color
/// insertion, adjustment, DWIM, process-protocol, cancellation, and failure
/// parity corpus. MELPA built this archive from upstream commit
/// `bdece51052ef7037e0a3481fc1f487939f57777e`.
pub const ZENITY_COLOR_PICKER_MELPA_PIN: (&str, &str) = ("zenity-color-picker", "20160302.1154");

/// The exact zeno-theme package selected by the practical editing, documented
/// italics, built-in workflow, optional-integration, and theme lifecycle parity
/// corpus. MELPA built this archive from upstream commit
/// `70fa7b7442f24ea25eab538b5a22da690745fef5`.
pub const ZENO_THEME_MELPA_PIN: (&str, &str) = ("zeno-theme", "20211205.2148");

/// The exact GNU ELPA rainbow-mode source used to exercise Zenburn's optional
/// public font-lock integration, acquired directly from GNU ELPA commit
/// `ac68593018ef3555e64ea592d72334f4e3e39209`.
pub const RAINBOW_MODE_SOURCE_PIN: (&str, &str) = ("rainbow-mode", "1.0.6");

/// The exact 0blayout package selected by the comprehensive API parity corpus.
pub const ZERO_B_LAYOUT_MELPA_PIN: (&str, &str) = ("0blayout", "20190703.527");

/// The exact 0x0 package selected by the comprehensive API parity corpus.
pub const ZERO_X_ZERO_MELPA_PIN: (&str, &str) = ("0x0", "20230823.2214");

/// The exact 0xc package selected by the comprehensive API parity corpus.
pub const ZERO_X_C_MELPA_PIN: (&str, &str) = ("0xc", "20201025.2105");

/// The exact 2048-game package selected by the comprehensive API parity corpus.
pub const GAME_2048_MELPA_PIN: (&str, &str) = ("2048-game", "20230809.356");

/// The exact 2bit package selected by the comprehensive API parity corpus.
pub const TWO_BIT_MELPA_PIN: (&str, &str) = ("2bit", "20200926.1418");

/// The exact 750words package selected by the comprehensive API parity corpus.
pub const SEVEN_FIFTY_WORDS_MELPA_PIN: (&str, &str) = ("750words", "20220625.1407");

/// The exact @ package selected by the comprehensive API parity corpus.
pub const AT_MELPA_PIN: (&str, &str) = ("@", "20240923.1318");

/// The exact a package selected by the comprehensive API parity corpus.
pub const A_MELPA_PIN: (&str, &str) = ("a", "20210929.1510");

/// The exact aa-edit-mode package selected by the comprehensive API parity
/// corpus.
pub const AA_EDIT_MODE_MELPA_PIN: (&str, &str) = ("aa-edit-mode", "20170119.320");

/// The exact Aangit package selected by the comprehensive API parity corpus.
pub const AANGIT_MELPA_PIN: (&str, &str) = ("aangit", "20231106.2115");

/// The exact AAS package selected by the comprehensive API parity corpus.
pub const AAS_MELPA_PIN: (&str, &str) = ("aas", "20230303.2214");

/// The exact abc-mode package selected by the comprehensive API parity corpus.
pub const ABC_MODE_MELPA_PIN: (&str, &str) = ("abc-mode", "20220713.1359");

/// The exact Abgaben package selected by the comprehensive API parity corpus.
pub const ABGABEN_MELPA_PIN: (&str, &str) = ("abgaben", "20171119.646");

/// The exact abl-mode package selected by the comprehensive API parity corpus.
pub const ABL_MODE_MELPA_PIN: (&str, &str) = ("abl-mode", "20240423.1214");

/// The exact abridge-diff package selected by the comprehensive API parity
/// corpus.
pub const ABRIDGE_DIFF_MELPA_PIN: (&str, &str) = ("abridge-diff", "20230307.2159");

/// The exact abs-mode package selected by the comprehensive API parity corpus.
pub const ABS_MODE_MELPA_PIN: (&str, &str) = ("abs-mode", "20260415.813");

/// The exact abyss-theme package selected by the comprehensive API parity
/// corpus.
pub const ABYSS_THEME_MELPA_PIN: (&str, &str) = ("abyss-theme", "20260125.1959");

/// The exact ac-alchemist package selected by the comprehensive API parity
/// corpus.
pub const AC_ALCHEMIST_MELPA_PIN: (&str, &str) = ("ac-alchemist", "20150908.656");

/// The exact ac-c-headers package selected by the comprehensive API parity
/// corpus.
pub const AC_C_HEADERS_MELPA_PIN: (&str, &str) = ("ac-c-headers", "20200816.1007");

/// The exact ac-capf package selected by the comprehensive API parity corpus.
pub const AC_CAPF_MELPA_PIN: (&str, &str) = ("ac-capf", "20151101.217");

/// The exact ac-clang package selected by the comprehensive API parity corpus.
pub const AC_CLANG_MELPA_PIN: (&str, &str) = ("ac-clang", "20180710.546");

/// The exact ac-dcd package selected by the comprehensive API parity corpus.
pub const AC_DCD_MELPA_PIN: (&str, &str) = ("ac-dcd", "20250925.946");

/// The exact ac-emmet package selected by the comprehensive API parity corpus.
pub const AC_EMMET_MELPA_PIN: (&str, &str) = ("ac-emmet", "20131015.1558");

/// The exact ac-emoji package selected by the comprehensive API parity corpus.
pub const AC_EMOJI_MELPA_PIN: (&str, &str) = ("ac-emoji", "20150823.711");

/// The exact ac-etags package selected by the comprehensive API parity corpus.
pub const AC_ETAGS_MELPA_PIN: (&str, &str) = ("ac-etags", "20161001.1507");

/// The exact ac-geiser package selected by the comprehensive API parity corpus.
pub const AC_GEISER_MELPA_PIN: (&str, &str) = ("ac-geiser", "20200318.824");

/// The exact ac-haskell-process package selected by the comprehensive API
/// parity corpus.
pub const AC_HASKELL_PROCESS_MELPA_PIN: (&str, &str) = ("ac-haskell-process", "20150423.1402");

/// The exact ac-helm package selected by the comprehensive API parity corpus.
pub const AC_HELM_MELPA_PIN: (&str, &str) = ("ac-helm", "20160319.233");

/// The exact ac-html package selected by the comprehensive API parity corpus.
pub const AC_HTML_MELPA_PIN: (&str, &str) = ("ac-html", "20151005.731");

/// The exact ac-html-angular package selected by the comprehensive API parity
/// corpus.
pub const AC_HTML_ANGULAR_MELPA_PIN: (&str, &str) = ("ac-html-angular", "20151225.719");

/// The exact ac-html-bootstrap package selected by the comprehensive API
/// parity corpus.
pub const AC_HTML_BOOTSTRAP_MELPA_PIN: (&str, &str) = ("ac-html-bootstrap", "20160302.1701");

/// The exact ac-html-csswatcher package selected by the comprehensive API
/// parity corpus.
pub const AC_HTML_CSSWATCHER_MELPA_PIN: (&str, &str) = ("ac-html-csswatcher", "20151208.2113");

/// The exact ac-inf-ruby package selected by the comprehensive API parity
/// corpus.
pub const AC_INF_RUBY_MELPA_PIN: (&str, &str) = ("ac-inf-ruby", "20131115.1150");

/// The exact ac-ispell package selected by the comprehensive API parity
/// corpus.
pub const AC_ISPELL_MELPA_PIN: (&str, &str) = ("ac-ispell", "20151101.226");

/// The exact ac-js2 package selected by the comprehensive API parity corpus.
pub const AC_JS2_MELPA_PIN: (&str, &str) = ("ac-js2", "20190101.933");

/// The exact ac-math package selected by the comprehensive API parity corpus.
pub const AC_MATH_MELPA_PIN: (&str, &str) = ("ac-math", "20141116.2127");

/// The exact ac-mozc package selected by the comprehensive API parity corpus.
pub const AC_MOZC_MELPA_PIN: (&str, &str) = ("ac-mozc", "20150227.1619");

/// The exact ac-octave package selected by the comprehensive API parity corpus.
pub const AC_OCTAVE_MELPA_PIN: (&str, &str) = ("ac-octave", "20180406.334");

/// The exact ac-php package selected by the comprehensive API parity corpus.
pub const AC_PHP_MELPA_PIN: (&str, &str) = ("ac-php", "20240328.1036");

/// The exact ac-php-core package selected by the comprehensive API parity
/// corpus.
pub const AC_PHP_CORE_MELPA_PIN: (&str, &str) = ("ac-php-core", "20260210.846");

/// The exact ac-racer package selected by the comprehensive API parity corpus.
pub const AC_RACER_MELPA_PIN: (&str, &str) = ("ac-racer", "20170114.809");

/// The exact ac-rtags package selected by the comprehensive API parity corpus.
pub const AC_RTAGS_MELPA_PIN: (&str, &str) = ("ac-rtags", "20191222.920");

/// The exact ac-skk package selected by the comprehensive API parity corpus.
pub const AC_SKK_MELPA_PIN: (&str, &str) = ("ac-skk", "20141230.119");

/// The exact ac-slime package selected by the comprehensive API parity corpus.
pub const AC_SLIME_MELPA_PIN: (&str, &str) = ("ac-slime", "20171027.2100");

/// The exact ac-sly package selected by the comprehensive API parity corpus.
pub const AC_SLY_MELPA_PIN: (&str, &str) = ("ac-sly", "20170728.1027");

/// The exact Academic Phrases package selected by the comprehensive API parity
/// corpus.
pub const ACADEMIC_PHRASES_MELPA_PIN: (&str, &str) = ("academic-phrases", "20180723.1021");

/// The exact Accent package selected by the comprehensive API parity corpus.
pub const ACCENT_MELPA_PIN: (&str, &str) = ("accent", "20250210.906");

/// The exact Ace Flyspell package selected by the comprehensive API parity
/// corpus.
pub const ACE_FLYSPELL_MELPA_PIN: (&str, &str) = ("ace-flyspell", "20170309.509");

/// The exact Ace Isearch package selected by the comprehensive API parity
/// corpus.
pub const ACE_ISEARCH_MELPA_PIN: (&str, &str) = ("ace-isearch", "20220809.1748");

/// The exact Ace Jump Buffer package selected by the comprehensive API parity
/// corpus.
pub const ACE_JUMP_BUFFER_MELPA_PIN: (&str, &str) = ("ace-jump-buffer", "20171031.1550");

/// The exact Ace Jump Helm Line package selected by the comprehensive API
/// parity corpus.
pub const ACE_JUMP_HELM_LINE_MELPA_PIN: (&str, &str) = ("ace-jump-helm-line", "20160918.1836");

/// The exact ace-jump-mode package selected by the comprehensive API parity
/// corpus.
pub const ACE_JUMP_MODE_MELPA_PIN: (&str, &str) = ("ace-jump-mode", "20140616.815");

/// The exact ace-jump-zap package selected by the comprehensive API parity
/// corpus.
pub const ACE_JUMP_ZAP_MELPA_PIN: (&str, &str) = ("ace-jump-zap", "20170717.1849");

/// The exact ace-link package selected by the comprehensive API parity corpus.
pub const ACE_LINK_MELPA_PIN: (&str, &str) = ("ace-link", "20241101.1344");

/// The exact ace-mc package selected by the comprehensive API parity corpus.
pub const ACE_MC_MELPA_PIN: (&str, &str) = ("ace-mc", "20190206.749");

/// The exact ace-pinyin package selected by the comprehensive API parity
/// corpus.
pub const ACE_PINYIN_MELPA_PIN: (&str, &str) = ("ace-pinyin", "20210827.355");

/// The exact ace-popup-menu package selected by the comprehensive API parity
/// corpus.
pub const ACE_POPUP_MENU_MELPA_PIN: (&str, &str) = ("ace-popup-menu", "20230606.1445");

/// The exact ace-window package selected by the comprehensive API parity
/// corpus.
pub const ACE_WINDOW_MELPA_PIN: (&str, &str) = ("ace-window", "20220911.358");

/// The exact achievements package selected by the comprehensive API parity
/// corpus.
pub const ACHIEVEMENTS_MELPA_PIN: (&str, &str) = ("achievements", "20240703.318");

/// The exact ack-menu package selected by the comprehensive API parity corpus.
pub const ACK_MENU_MELPA_PIN: (&str, &str) = ("ack-menu", "20150504.2022");

/// The exact acme-theme package selected by the comprehensive API parity
/// corpus.
pub const ACME_THEME_MELPA_PIN: (&str, &str) = ("acme-theme", "20210430.302");

/// The exact acp package selected by the comprehensive API parity corpus.
pub const ACP_MELPA_PIN: (&str, &str) = ("acp", "20260719.342");

/// The exact act-mode package selected by the comprehensive API parity corpus.
pub const ACT_MODE_MELPA_PIN: (&str, &str) = ("act-mode", "20240718.39");

/// The exact actionscript-mode package selected by the comprehensive API parity
/// corpus.
pub const ACTIONSCRIPT_MODE_MELPA_PIN: (&str, &str) = ("actionscript-mode", "20180527.1701");

/// The exact activity-watch-mode package selected by the comprehensive API
/// parity corpus.
pub const ACTIVITY_WATCH_MODE_MELPA_PIN: (&str, &str) = ("activity-watch-mode", "20260311.835");

/// The exact acton-mode package selected by the comprehensive API parity corpus.
pub const ACTON_MODE_MELPA_PIN: (&str, &str) = ("acton-mode", "20250113.1059");

/// The exact ada-ts-mode package selected by the comprehensive API parity
/// corpus.
pub const ADA_TS_MODE_MELPA_PIN: (&str, &str) = ("ada-ts-mode", "20260627.1553");

/// The exact adafruit-wisdom package selected by the comprehensive API parity
/// corpus.
pub const ADAFRUIT_WISDOM_MELPA_PIN: (&str, &str) = ("adafruit-wisdom", "20200217.306");

/// The exact add-hooks package selected by the comprehensive API parity corpus.
pub const ADD_HOOKS_MELPA_PIN: (&str, &str) = ("add-hooks", "20171217.123");

/// The exact add-node-modules-path package selected by the comprehensive API
/// parity corpus.
pub const ADD_NODE_MODULES_PATH_MELPA_PIN: (&str, &str) = ("add-node-modules-path", "20230307.655");

/// The exact addressbook-bookmark package selected by the comprehensive API
/// parity corpus.
pub const ADDRESSBOOK_BOOKMARK_MELPA_PIN: (&str, &str) = ("addressbook-bookmark", "20260105.453");

/// The exact ado-mode package selected by the comprehensive API parity corpus.
pub const ADO_MODE_MELPA_PIN: (&str, &str) = ("ado-mode", "20260210.1431");

/// The exact adoc-mode package selected by the comprehensive API parity corpus.
pub const ADOC_MODE_MELPA_PIN: (&str, &str) = ("adoc-mode", "20260612.638");

/// The exact advent-mode package selected by the comprehensive API parity
/// corpus.
pub const ADVENT_MODE_MELPA_PIN: (&str, &str) = ("advent-mode", "20260209.1903");

/// The exact adwaita-dark-theme package selected by the comprehensive API
/// parity corpus.
pub const ADWAITA_DARK_THEME_MELPA_PIN: (&str, &str) = ("adwaita-dark-theme", "20231209.1033");

/// The exact AES package selected by the comprehensive API parity corpus.
pub const AES_MELPA_PIN: (&str, &str) = ("aes", "20211204.2348");

/// The exact Affe package selected by the comprehensive API parity corpus.
pub const AFFE_MELPA_PIN: (&str, &str) = ("affe", "20260519.1026");

/// The exact Afterglow package selected by the comprehensive API parity corpus.
pub const AFTERGLOW_MELPA_PIN: (&str, &str) = ("afterglow", "20240312.953");

/// The exact afternoon-theme package selected by the comprehensive API parity
/// corpus.
pub const AFTERNOON_THEME_MELPA_PIN: (&str, &str) = ("afternoon-theme", "20140104.1859");

/// The exact ag package selected by the comprehensive API parity corpus.
pub const AG_MELPA_PIN: (&str, &str) = ("ag", "20201031.2202");

/// The exact agda-editor-tactics package selected by the comprehensive API
/// parity corpus.
pub const AGDA_EDITOR_TACTICS_MELPA_PIN: (&str, &str) = ("agda-editor-tactics", "20211024.2357");

/// The exact agda-lib-mode package selected by the comprehensive API parity
/// corpus.
pub const AGDA_LIB_MODE_MELPA_PIN: (&str, &str) = ("agda-lib-mode", "20251013.2307");

/// The exact age package selected by the comprehensive API parity corpus.
pub const AGE_MELPA_PIN: (&str, &str) = ("age", "20250806.1723");

/// The exact agenix package selected by the comprehensive API parity corpus.
pub const AGENIX_MELPA_PIN: (&str, &str) = ("agenix", "20250209.551");

/// The exact agent-recall package selected by the comprehensive API parity
/// corpus.
pub const AGENT_RECALL_MELPA_PIN: (&str, &str) = ("agent-recall", "20260710.1707");

/// The exact agent-shell package selected by the comprehensive API parity
/// corpus.
pub const AGENT_SHELL_MELPA_PIN: (&str, &str) = ("agent-shell", "20260728.953");

/// The exact aggressive-fill-paragraph package selected by the comprehensive
/// API parity corpus.
pub const AGGRESSIVE_FILL_PARAGRAPH_MELPA_PIN: (&str, &str) =
    ("aggressive-fill-paragraph", "20240213.2320");

/// The exact aggressive-indent package selected by the comprehensive API
/// parity corpus.
pub const AGGRESSIVE_INDENT_MELPA_PIN: (&str, &str) = ("aggressive-indent", "20230112.1300");

/// The exact agitjo package selected by the comprehensive API parity corpus.
pub const AGITJO_MELPA_PIN: (&str, &str) = ("agitjo", "20260523.2048");

/// The exact agtags package selected by the comprehensive API parity corpus.
pub const AGTAGS_MELPA_PIN: (&str, &str) = ("agtags", "20250523.1654");

/// The exact ah package selected by the comprehensive API parity corpus.
pub const AH_MELPA_PIN: (&str, &str) = ("ah", "20220730.1058");

/// The exact aHg package selected by the comprehensive API parity corpus.
pub const AHG_MELPA_PIN: (&str, &str) = ("ahg", "20241113.748");

/// The exact ahk-mode package selected by the comprehensive API parity corpus.
pub const AHK_MODE_MELPA_PIN: (&str, &str) = ("ahk-mode", "20200412.1832");

/// The exact ahungry-theme package selected by the comprehensive API parity
/// corpus.
pub const AHUNGRY_THEME_MELPA_PIN: (&str, &str) = ("ahungry-theme", "20180131.328");

/// The exact ai-code package selected by the comprehensive API parity corpus.
pub const AI_CODE_MELPA_PIN: (&str, &str) = ("ai-code", "20260727.2322");

/// The exact aider package selected by the comprehensive API parity corpus.
pub const AIDER_MELPA_PIN: (&str, &str) = ("aider", "20251201.133");

/// The exact Aidermacs package selected by the comprehensive API parity corpus.
pub const AIDERMACS_MELPA_PIN: (&str, &str) = ("aidermacs", "20260726.839");

/// The exact aidev-mode package selected by the comprehensive API parity
/// corpus.
pub const AIDEV_MODE_MELPA_PIN: (&str, &str) = ("aidev-mode", "20250318.2144");

/// The exact aiken-mode package selected by the comprehensive API parity
/// corpus.
pub const AIKEN_MODE_MELPA_PIN: (&str, &str) = ("aiken-mode", "20230920.1210");

/// The exact aio package selected by the comprehensive API parity corpus.
pub const AIO_MELPA_PIN: (&str, &str) = ("aio", "20260214.1529");

/// The exact airline-themes package selected by the comprehensive API parity
/// corpus.
pub const AIRLINE_THEMES_MELPA_PIN: (&str, &str) = ("airline-themes", "20250502.1915");

/// The exact airplay package selected by the comprehensive API parity corpus.
pub const AIRPLAY_MELPA_PIN: (&str, &str) = ("airplay", "20130212.1226");

/// The exact alabaster-themes package selected by the comprehensive API parity
/// corpus.
pub const ALABASTER_THEMES_MELPA_PIN: (&str, &str) = ("alabaster-themes", "20260113.657");

/// The exact alan-mode package selected by the comprehensive API parity corpus.
pub const ALAN_MODE_MELPA_PIN: (&str, &str) = ("alan-mode", "20260523.1330");

/// The exact alarm-clock package selected by the comprehensive API parity
/// corpus.
pub const ALARM_CLOCK_MELPA_PIN: (&str, &str) = ("alarm-clock", "20250123.556");

/// The exact Alchemist package selected by the comprehensive API parity corpus.
pub const ALCHEMIST_MELPA_PIN: (&str, &str) = ("alchemist", "20180312.1304");

/// The exact alda-mode package selected by the comprehensive API parity corpus.
pub const ALDA_MODE_MELPA_PIN: (&str, &str) = ("alda-mode", "20251223.6");

/// The exact alect-themes package selected by the comprehensive API parity
/// corpus.
pub const ALECT_THEMES_MELPA_PIN: (&str, &str) = ("alect-themes", "20251205.1503");

/// The exact Alectryon package selected by the comprehensive API parity
/// corpus.
pub const ALECTRYON_MELPA_PIN: (&str, &str) = ("alectryon", "20260525.2000");

/// The exact alert package selected by the comprehensive API parity corpus.
pub const ALERT_MELPA_PIN: (&str, &str) = ("alert", "20260316.2025");

/// The exact alert-termux package selected by the comprehensive API parity
/// corpus.
pub const ALERT_TERMUX_MELPA_PIN: (&str, &str) = ("alert-termux", "20181119.951");

/// The exact alert-toast package selected by the comprehensive API parity
/// corpus.
pub const ALERT_TOAST_MELPA_PIN: (&str, &str) = ("alert-toast", "20220312.229");

/// The exact align-cljlet package selected by the comprehensive API parity
/// corpus.
pub const ALIGN_CLJLET_MELPA_PIN: (&str, &str) = ("align-cljlet", "20160112.2101");

/// The exact all-ext package selected by the comprehensive API parity corpus.
/// MELPA built this archive from upstream commit
/// `c865c62506af2c9edc7705a7c24dc8b70d5d4de2`.
pub const ALL_EXT_MELPA_PIN: (&str, &str) = ("all-ext", "20200315.1443");

/// The exact all-the-icons package selected by the comprehensive API parity
/// corpus.
pub const ALL_THE_ICONS_MELPA_PIN: (&str, &str) = ("all-the-icons", "20250527.927");

/// The exact all-the-icons-completion package selected by the comprehensive
/// API parity corpus.
pub const ALL_THE_ICONS_COMPLETION_MELPA_PIN: (&str, &str) =
    ("all-the-icons-completion", "20240128.2048");

/// The exact all-the-icons-dired package selected by the comprehensive API
/// parity corpus.
pub const ALL_THE_ICONS_DIRED_MELPA_PIN: (&str, &str) = ("all-the-icons-dired", "20231207.1324");

/// The exact all-the-icons-gnus package selected by the comprehensive API
/// parity corpus.
pub const ALL_THE_ICONS_GNUS_MELPA_PIN: (&str, &str) = ("all-the-icons-gnus", "20180511.654");

/// The exact all-the-icons-ibuffer package selected by the comprehensive API
/// parity corpus.
pub const ALL_THE_ICONS_IBUFFER_MELPA_PIN: (&str, &str) =
    ("all-the-icons-ibuffer", "20230503.1625");

/// The exact all-the-icons-ivy package selected by the comprehensive API
/// parity corpus.
pub const ALL_THE_ICONS_IVY_MELPA_PIN: (&str, &str) = ("all-the-icons-ivy", "20190508.1803");

/// The exact all-the-icons-ivy-rich package selected by the comprehensive API
/// parity corpus. MELPA built this archive from upstream commit
/// `c098cc85123a401b0ab8f2afd3a25853e61d7d28`.
pub const ALL_THE_ICONS_IVY_RICH_MELPA_PIN: (&str, &str) =
    ("all-the-icons-ivy-rich", "20230420.1234");

/// The exact all-the-icons-nerd-fonts package selected by the comprehensive
/// API parity corpus.
pub const ALL_THE_ICONS_NERD_FONTS_MELPA_PIN: (&str, &str) =
    ("all-the-icons-nerd-fonts", "20260614.1246");

/// The exact almost-mono-themes package selected by the comprehensive API
/// parity corpus.
pub const ALMOST_MONO_THEMES_MELPA_PIN: (&str, &str) = ("almost-mono-themes", "20250722.1957");

/// The exact alsamixer package selected by the comprehensive API parity
/// corpus.
pub const ALSAMIXER_MELPA_PIN: (&str, &str) = ("alsamixer", "20250106.1025");

/// The exact alt-codes package selected by the comprehensive API parity
/// corpus.
pub const ALT_CODES_MELPA_PIN: (&str, &str) = ("alt-codes", "20260101.557");

/// The exact Amaranth Dark theme selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `624e0b5ef632b3adfdc03e44dce7a98cd48d47ed`.
pub const AMARANTH_DARK_THEME_MELPA_PIN: (&str, &str) = ("amaranth-dark-theme", "20251228.1916");

/// The exact amber-glow-theme package selected by the comprehensive API
/// parity corpus.
pub const AMBER_GLOW_THEME_MELPA_PIN: (&str, &str) = ("amber-glow-theme", "20250305.936");

/// The exact amd-mode package selected by the comprehensive API parity corpus.
pub const AMD_MODE_MELPA_PIN: (&str, &str) = ("amd-mode", "20180111.1402");

/// The exact Ameba package selected by the comprehensive API parity corpus.
/// MELPA built this archive from upstream commit
/// `0c4925ae0e998818326adcb47ed27ddf9761c7dc`.
pub const AMEBA_MELPA_PIN: (&str, &str) = ("ameba", "20200103.1454");

/// The exact ample-regexps package selected by the comprehensive API parity
/// corpus.
pub const AMPLE_REGEXPS_MELPA_PIN: (&str, &str) = ("ample-regexps", "20200508.1021");

/// The exact ample-theme package selected by the comprehensive API parity
/// corpus.
pub const AMPLE_THEME_MELPA_PIN: (&str, &str) = ("ample-theme", "20260611.1532");

/// The exact ample-zen-theme package selected by the comprehensive API parity
/// corpus.
pub const AMPLE_ZEN_THEME_MELPA_PIN: (&str, &str) = ("ample-zen-theme", "20150119.2154");

/// The exact amread-mode package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `bf06b05c6322fe74f0e5ac2436cad46f66f673c6`.
pub const AMREAD_MODE_MELPA_PIN: (&str, &str) = ("amread-mode", "20240903.1534");

/// The exact amsreftex package selected by the comprehensive API parity corpus.
pub const AMSREFTEX_MELPA_PIN: (&str, &str) = ("amsreftex", "20240512.1746");

/// The exact amx package selected by the comprehensive API parity corpus.
pub const AMX_MELPA_PIN: (&str, &str) = ("amx", "20230413.1210");

/// The exact anaconda-mode package selected by the comprehensive API parity
/// corpus.
pub const ANACONDA_MODE_MELPA_PIN: (&str, &str) = ("anaconda-mode", "20250430.227");

/// The exact anakondo package selected by the comprehensive API parity corpus.
/// MELPA built this archive from upstream commit
/// `16b0ba14d94a5d7e55655efc9e1d6d069a9306f2`.
pub const ANAKONDO_MELPA_PIN: (&str, &str) = ("anakondo", "20210221.1727");

/// The exact anaphora package selected by the comprehensive API parity corpus.
pub const ANAPHORA_MELPA_PIN: (&str, &str) = ("anaphora", "20260720.903");

/// The exact ancient-one-dark-theme package selected by the comprehensive
/// theme parity corpus.
pub const ANCIENT_ONE_DARK_THEME_MELPA_PIN: (&str, &str) =
    ("ancient-one-dark-theme", "20211030.1358");

/// The exact ancient-theme package selected by the comprehensive API parity
/// corpus.
pub const ANCIENT_THEME_MELPA_PIN: (&str, &str) = ("ancient-theme", "20260322.1856");

/// The exact android-env package selected by the comprehensive API parity
/// corpus.
pub const ANDROID_ENV_MELPA_PIN: (&str, &str) = ("android-env", "20220810.1449");

/// The exact android-mode package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `67f7c0d7d37605efc7f055b76d731556861c3eb9`.
pub const ANDROID_MODE_MELPA_PIN: (&str, &str) = ("android-mode", "20250106.1022");

/// The exact angry-police-captain package selected by the comprehensive API
/// parity corpus.
pub const ANGRY_POLICE_CAPTAIN_MELPA_PIN: (&str, &str) = ("angry-police-captain", "20120829.1252");

/// The exact angular-mode package selected by the comprehensive API parity
/// corpus.
pub const ANGULAR_MODE_MELPA_PIN: (&str, &str) = ("angular-mode", "20151201.2127");

/// The exact angular-snippets package selected by the comprehensive API
/// parity corpus.
pub const ANGULAR_SNIPPETS_MELPA_PIN: (&str, &str) = ("angular-snippets", "20140514.523");

/// The exact Anju package selected by the comprehensive mouse UI parity
/// corpus.
pub const ANJU_MELPA_PIN: (&str, &str) = ("anju", "20260701.2139");

/// The exact anki-connect package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `e32e611d54a3819f88c5ff58009df70c9ae01934`.
pub const ANKI_CONNECT_MELPA_PIN: (&str, &str) = ("anki-connect", "20250414.1301");

/// The exact anki-editor package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `4a55c3f937b176d31e36d484c196682cae9f9104`.
pub const ANKI_EDITOR_MELPA_PIN: (&str, &str) = ("anki-editor", "20260714.1156");

/// The exact anki-editor-view package selected by the comprehensive API parity
/// corpus.
pub const ANKI_EDITOR_VIEW_MELPA_PIN: (&str, &str) = ("anki-editor-view", "20230807.806");

/// The exact anki-mode package selected by the comprehensive API parity corpus.
pub const ANKI_MODE_MELPA_PIN: (&str, &str) = ("anki-mode", "20201223.719");

/// The exact anki-vocabulary package selected by the comprehensive API parity
/// corpus.
pub const ANKI_VOCABULARY_MELPA_PIN: (&str, &str) = ("anki-vocabulary", "20200103.325");

/// The exact Annalist package selected by the comprehensive recording and Org
/// rendering parity corpus.
pub const ANNALIST_MELPA_PIN: (&str, &str) = ("annalist", "20260531.1558");

/// The exact Annotate package selected by the comprehensive API parity corpus.
pub const ANNOTATE_MELPA_PIN: (&str, &str) = ("annotate", "20260514.1320");

/// The exact annotate-depth package selected by the comprehensive API parity
/// corpus.
pub const ANNOTATE_DEPTH_MELPA_PIN: (&str, &str) = ("annotate-depth", "20160520.2040");

/// The exact annotation package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `213db6e50bb89c1b0b2832eab4c6caafb137eb6d`.
pub const ANNOTATION_MELPA_PIN: (&str, &str) = ("annotation", "20250805.1029");

/// The exact annoying-arrows-mode package selected by the comprehensive API
/// parity corpus.
pub const ANNOYING_ARROWS_MODE_MELPA_PIN: (&str, &str) = ("annoying-arrows-mode", "20161024.646");

/// The exact ansi package selected by the practical terminal-rendering parity
/// corpus. MELPA built this archive from upstream commit
/// `a3aa9daa37a75fec22186399014a790a6c554311`.
pub const ANSI_MELPA_PIN: (&str, &str) = ("ansi", "20251118.230");

/// The exact Ansible package selected by the comprehensive playbook editing
/// and vault workflow parity corpus.
pub const ANSIBLE_MELPA_PIN: (&str, &str) = ("ansible", "20260607.1852");

/// The exact ansible-doc package selected by the comprehensive documentation
/// workflow parity corpus.
pub const ANSIBLE_DOC_MELPA_PIN: (&str, &str) = ("ansible-doc", "20160924.824");

/// The exact ansible-vault package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `74f96ce226f51bec203af343f73182ea132749a6`.
pub const ANSIBLE_VAULT_MELPA_PIN: (&str, &str) = ("ansible-vault", "20251029.2146");

/// The exact Ansilove package selected by the practical ANSI-art conversion
/// and viewing parity corpus. MELPA built this archive from upstream commit
/// `a75eb6c89a1d96e1b4fa028ecca9be8b13c95230`.
pub const ANSILOVE_MELPA_PIN: (&str, &str) = ("ansilove", "20250105.1853");

/// The exact ant package selected by the comprehensive build-workflow parity
/// corpus.
pub const ANT_MELPA_PIN: (&str, &str) = ("ant", "20160211.1543");

/// The exact Anti-Zenburn Theme package selected by the practical editing,
/// review, and build-output parity corpus. MELPA built this archive from
/// upstream commit `dbafbaa86be67c1d409873f57a5c0bbe1e7ca158`.
pub const ANTI_ZENBURN_THEME_MELPA_PIN: (&str, &str) = ("anti-zenburn-theme", "20180712.1838");

/// The exact anx-api package selected by the comprehensive API workflow parity
/// corpus.
pub const ANX_API_MELPA_PIN: (&str, &str) = ("anx-api", "20140208.1514");

/// The exact AnyBar package selected by the practical indicator lifecycle,
/// custom-image, and multi-instance parity corpus. MELPA built this archive
/// from upstream commit
/// `7a0743e0d31bcb36ab1bb2e351f3e7139c422ac5`.
pub const ANYBAR_MELPA_PIN: (&str, &str) = ("anybar", "20160816.1421");

/// The exact Anyins package selected by the comprehensive API parity corpus.
pub const ANYINS_MELPA_PIN: (&str, &str) = ("anyins", "20131229.1041");

/// The exact Anzu package selected by the practical incremental-search,
/// scoped-rename, selective-replacement, and global-mode parity corpus. MELPA
/// built this archive from upstream commit
/// `bc3a0032bb6aa7f5886f10460cd53eb7b8b020af`.
pub const ANZU_MELPA_PIN: (&str, &str) = ("anzu", "20240929.201");

/// The exact aozora-view package selected by the practical reading, bookmark,
/// redraw, and cache-resume parity corpus. MELPA built this archive from
/// upstream commit
/// `b0390616d19e45f15f9a2f5d5688274831e721fd`.
pub const AOZORA_VIEW_MELPA_PIN: (&str, &str) = ("aozora-view", "20140310.1317");

/// The exact Apache Mode package selected by the comprehensive editing parity
/// corpus.
pub const APACHE_MODE_MELPA_PIN: (&str, &str) = ("apache-mode", "20210519.1931");

/// The exact APDL Mode package selected by the practical authoring, inspection,
/// help, solver, artifact, and license-operation parity corpus. MELPA built
/// this archive from upstream commit
/// `4883ab085811b85cc75c44b5af478ab8f7e98386`.
pub const APDL_MODE_MELPA_PIN: (&str, &str) = ("apdl-mode", "20250508.908");

/// The exact APEL package selected by the practical legacy-package,
/// message-routing, product, MIME, richtext, filesystem, and CCL parity
/// corpus. MELPA built this archive from upstream commit
/// `1b043cfea58ea146356c237a5286ead69e97417b`.
pub const APEL_MELPA_PIN: (&str, &str) = ("apel", "20250608.1806");

/// The exact Apheleia package selected by the practical formatter, point
/// preservation, project configuration, save-mode, concurrency, and
/// diagnostic parity corpus. MELPA built this archive from upstream commit
/// `14a0bb4454fb2cc3b5b377619288b742ce117da5`.
pub const APHELEIA_MELPA_PIN: (&str, &str) = ("apheleia", "20260619.1935");

/// The exact APIB Mode package selected by the comprehensive API parity corpus.
pub const APIB_MODE_MELPA_PIN: (&str, &str) = ("apib-mode", "20200101.1017");

/// The exact apiwrap package selected by the practical generated-client
/// lifecycle, policy, error-recovery, and discovery parity corpus. MELPA built
/// this archive from upstream commit
/// `e4c9c57d6620a788ec8a715ff1bb50542edea3a6`.
pub const APIWRAP_MELPA_PIN: (&str, &str) = ("apiwrap", "20180602.2231");

/// The exact app-monochrome-themes package selected by the practical code,
/// writing, Dired, and theme-lifecycle parity corpus. MELPA built this archive
/// from upstream commit
/// `bd8bfee0b64bf10543f4cefaf40bb5dcd4cf123b`.
pub const APP_MONOCHROME_THEMES_MELPA_PIN: (&str, &str) =
    ("app-monochrome-themes", "20250710.2315");

/// The exact apparmor-mode package selected by the practical policy authoring,
/// fontification, completion, and live-diagnostics parity corpus. MELPA built
/// this archive from upstream commit
/// `b0e4bbcd30aafd71f484c74164351af40ef885bf`.
pub const APPARMOR_MODE_MELPA_PIN: (&str, &str) = ("apparmor-mode", "20260515.454");

/// The exact Apple Container TRAMP package selected by the practical
/// interactive completion, optional-user remote editing, and cleanup lifecycle
/// parity corpus. MELPA built this archive from upstream commit
/// `f47d58d029c594f4c9e9b1cfff79630de68a9cb5`.
pub const APPLE_CONTAINER_TRAMP_MELPA_PIN: (&str, &str) =
    ("apple-container-tramp", "20260504.1350");

/// The exact apples-mode package exercised by practical authoring, installed
/// snippet, execution, toolchain, error-recovery, and scratch persistence
/// workflows. MELPA built this archive from upstream commit
/// `83a9ab0d6ba82496e2f7df386909b1a55701fccb`.
pub const APPLES_MODE_MELPA_PIN: (&str, &str) = ("apples-mode", "20110121.418");

/// The exact AppleScript Mode package selected by the practical authoring,
/// outline-navigation, file-saving, macOS execution-boundary, failure-state,
/// one-off command, and structured-result parity corpus. MELPA built this
/// archive from upstream commit
/// `00c141bbff46c89a96598b605dee05dd1d89f624`.
pub const APPLESCRIPT_MODE_MELPA_PIN: (&str, &str) = ("applescript-mode", "20210802.1715");

/// The exact apropospriate-theme package selected by the practical code,
/// diff, Org, ANSI output, customization, and dark/light lifecycle parity
/// corpus. MELPA built this archive from upstream commit
/// `2b26eed7e2063ca93998a6807f5a4e602483a23d`.
pub const APROPOSPRIATE_THEME_MELPA_PIN: (&str, &str) = ("apropospriate-theme", "20251010.121");

/// The exact apt-sources-list package selected by the practical repository
/// authoring, interactive editing, suite migration, navigation, validation,
/// fontification, and file-persistence parity corpus. MELPA built this archive
/// from upstream commit `44112833b3fa7f4d7e43708e5996782e22bb2fa3`.
pub const APT_SOURCES_LIST_MELPA_PIN: (&str, &str) = ("apt-sources-list", "20180527.1241");

/// The exact AQI package selected by the comprehensive data, cache, request,
/// and reporting parity corpus.
pub const AQI_MELPA_PIN: (&str, &str) = ("aqi", "20230530.1204");

/// The exact arch-packer package selected by the practical package listing,
/// detail, repository refresh, search, install, marking, upgrade, and removal
/// parity corpus. MELPA built this archive from upstream commit
/// `940e96f7d357c6570b675a0f942181c787f1bfd7`.
pub const ARCH_PACKER_MELPA_PIN: (&str, &str) = ("arch-packer", "20170730.1321");

/// The exact arduino-mode package selected by the comprehensive API parity
/// corpus. MELPA built this archive from upstream commit
/// `b2ffd8441851659cb1cc844156073967729585e5`.
pub const ARDUINO_MODE_MELPA_PIN: (&str, &str) = ("arduino-mode", "20240527.1603");

/// The exact Flycheck package selected by its direct diagnostics parity
/// corpus and by arduino-mode's optional integration coverage.
pub const FLYCHECK_MELPA_PIN: (&str, &str) = ("flycheck", "20260728.931");

/// The exact flycheck-rust package selected for practical Cargo target
/// discovery, documented Flycheck hook setup, feature and crate-kind
/// configuration, failure recovery, and end-to-end rust-cargo diagnostics.
/// MELPA built this archive from upstream commit
/// `b9db73a7a5980ca884d5dd0cbe79b3291a185972`.
pub const FLYCHECK_RUST_MELPA_PIN: (&str, &str) = ("flycheck-rust", "20251231.1617");

/// The exact flycheck-elsa package selected for practical Elsa checker setup,
/// Cask/Eask backend command selection, enable predicates, and config-dir
/// discovery. MELPA built this archive from upstream commit
/// `d60db9544d0c4213f2478bcea0fd0e668e31cf34`.
pub const FLYCHECK_ELSA_MELPA_PIN: (&str, &str) = ("flycheck-elsa", "20230217.1640");

/// The exact flyspell-correct package selected for practical correction
/// interface defaults, completing-read candidate actions, highlight overlays,
/// and direction-aware move wrappers. MELPA built this archive from upstream
/// commit `a5a41c0f3a7881bd3eba07bee424ecb7c7d5061e`.
pub const FLYSPELL_CORRECT_MELPA_PIN: (&str, &str) = ("flyspell-correct", "20260106.955");

/// The exact Flyspell Correct Helm adapter selected for practical suggestion,
/// action, dictionary, abort, and interface-registration parity. MELPA built
/// this archive from upstream commit
/// `a5a41c0f3a7881bd3eba07bee424ecb7c7d5061e`.
pub const FLYSPELL_CORRECT_HELM_MELPA_PIN: (&str, &str) = ("flyspell-correct-helm", "20260106.955");

/// The exact Command Log Mode package selected for practical command/text,
/// repetition, window-toggle, local/global lifecycle, clear, and save parity.
/// MELPA built this archive from upstream commit
/// `af600e6b4129c8115f464af576505ea8e789db27`.
pub const COMMAND_LOG_MODE_MELPA_PIN: (&str, &str) = ("command-log-mode", "20160413.447");

/// The exact flycheck-dmd-dub package selected by the practical DUB project
/// discovery, metadata, subprocess, cache, and buffer-local flag parity corpus.
/// MELPA built this archive from upstream commit
/// c1bf54b7eca8951a38ce9f6ae12e07a011f03eb5.
pub const FLYCHECK_DMD_DUB_MELPA_PIN: (&str, &str) = ("flycheck-dmd-dub", "20250304.1432");

/// The exact flycheck-package package selected by the practical package
/// detection, checker-chain, diagnostic, navigation, recheck, and multi-file
/// parity corpus. MELPA built this archive from upstream commit
/// `ecd03f83790611888d693c684d719e033f69cb40`.
pub const FLYCHECK_PACKAGE_MELPA_PIN: (&str, &str) = ("flycheck-package", "20210509.2325");

/// The exact flycheck-pos-tip package selected by the practical graphical
/// diagnostic, TTY fallback, hook-driven hiding, and global-mode lifecycle
/// parity corpus. MELPA built this archive from upstream commit
/// `dc57beac0e59669926ad720c7af38b27c3a30467`.
pub const FLYCHECK_POS_TIP_MELPA_PIN: (&str, &str) = ("flycheck-pos-tip", "20200516.1600");

/// The exact Geiser package selected by the practical Scheme editing,
/// implementation, completion, evaluation-protocol, and source-navigation
/// parity corpus. MELPA built this archive from upstream commit
/// 3e506d06b34ccda8a50ac3e43c90d722c00065fe.
pub const GEISER_MELPA_PIN: (&str, &str) = ("geiser", "20260718.8");

/// The exact gntp package selected by the practical Growl registration,
/// notification-wire, file-icon, network-send, and reply-handling parity
/// corpus. MELPA built this archive from upstream commit
/// 767571135e2c0985944017dc59b0be79af222ef5.
pub const GNTP_MELPA_PIN: (&str, &str) = ("gntp", "20141025.250");

/// The exact Gnuplot package selected for practical script editing, syntax,
/// indentation, command navigation, option toggling, parser, contextual
/// completion, documentation, and command-dispatch parity. MELPA built this
/// archive from upstream commit
/// `81e3cb30297f0d12df41b865d2a76c8ba179089c`.
pub const GNUPLOT_MELPA_PIN: (&str, &str) = ("gnuplot", "20260623.1111");

/// The exact go-eldoc package selected for practical go-mode eldoc setup,
/// gocode function signatures, builtin make/len fallback, variable and
/// package types, and assignment-return highlighting parity. MELPA built
/// this archive from upstream commit
/// `cbbd2ea1e94a36004432a9ac61414cb5a95a39bd`.
pub const GO_ELDOC_MELPA_PIN: (&str, &str) = ("go-eldoc", "20170305.1427");

/// The exact Go Mode package selected for practical source editing,
/// indentation, semantic fontification, comment filling, signature
/// navigation, import management, formatter, module/workspace, and coverage
/// parity. MELPA built this archive from upstream commit
/// `8aaaa9d2574d7862ecbbe1ff369e88fe3796c8be`.
pub const GO_MODE_MELPA_PIN: (&str, &str) = ("go-mode", "20260510.1707");

/// The exact haskell-mode package selected by the practical source-editing,
/// fontification, declaration indexing, import formatting, layout indentation,
/// navigation, folding, and SCC annotation parity corpus. MELPA built this
/// archive from upstream commit 2dd755a5fa11577a9388af88f385d2a8e18f7a8d.
pub const HASKELL_MODE_MELPA_PIN: (&str, &str) = ("haskell-mode", "20260206.1050");

/// The exact hcl-mode package selected for practical Hashicorp file
/// opening, block/map/array indentation, assignment and interpolation
/// fontification, heredoc strings, and defun motion parity. MELPA built
/// this archive from upstream commit
/// `b2a03a446c1fe324ff494c28b9321486fa6fc672`.
pub const HCL_MODE_MELPA_PIN: (&str, &str) = ("hcl-mode", "20240220.1534");

/// The exact arscript-mode package selected by the comprehensive mode,
/// font-lock, indentation, and editing parity corpus. MELPA built this archive
/// from upstream commit `797e1d0ef1312e8ff846abd0c6853358041f7691`.
pub const ARSCRIPT_MODE_MELPA_PIN: (&str, &str) = ("arscript-mode", "20240819.1927");

/// The exact arxiv-citation package selected by the comprehensive parsing,
/// citation, dependency, editing, and download-workflow parity corpus. MELPA
/// built this archive from upstream commit
/// `04de0dae1121fb92c30b393449c6f8d6d940dbed`.
pub const ARXIV_CITATION_MELPA_PIN: (&str, &str) = ("arxiv-citation", "20230713.627");

/// The exact arxiv-mode package selected by the comprehensive query, rendering,
/// bibliography, navigation, and command-workflow parity corpus. MELPA built
/// this archive from upstream commit
/// `f629ec64f8bbac0cadb472c6741f8f33d49e9160`.
pub const ARXIV_MODE_MELPA_PIN: (&str, &str) = ("arxiv-mode", "20240111.2203");

/// The exact asciidoc-mode package selected by the comprehensive Tree-sitter,
/// editing, navigation, completion, and diagnostics parity corpus. MELPA built
/// this archive from upstream commit
/// `8914fad451f9c7f9c2286cf18db5edaa51a92cd7`.
pub const ASCIIDOC_MODE_MELPA_PIN: (&str, &str) = ("asciidoc-mode", "20260612.645");

/// The exact asdf-vm package selected by the comprehensive tool-version,
/// process, environment, installer, plugin, and menu workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `f6dbb4b6560cd7e5bb05006e9fc416c5c323b567`.
pub const ASDF_VM_MELPA_PIN: (&str, &str) = ("asdf-vm", "20250710.1053");

/// The exact ast-grep package selected by the comprehensive command, stream,
/// rewrite, completion-backend, and outline workflow parity corpus. MELPA
/// built this archive from upstream commit
/// `28bc6e9ac21acf1d1ef58b962b6acd670c27e80f`.
pub const AST_GREP_MELPA_PIN: (&str, &str) = ("ast-grep", "20260702.238");

/// The exact archive-phar package selected by the comprehensive archive
/// browsing and extraction parity corpus.
pub const ARCHIVE_PHAR_MELPA_PIN: (&str, &str) = ("archive-phar", "20221009.2129");

/// The exact Archive Region package selected by the comprehensive editing and
/// filesystem workflow parity corpus.
pub const ARCHIVE_REGION_MELPA_PIN: (&str, &str) = ("archive-region", "20200316.1425");

/// The exact archive-rpm package selected by the practical archive browsing,
/// extraction, metadata, compression, and binary-fidelity workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `cb48fee04cb0cbb26f760a3b95649f7dac78c6ec`.
pub const ARCHIVE_RPM_MELPA_PIN: (&str, &str) = ("archive-rpm", "20220527.632");

/// The exact arduino-cli-mode package selected by the practical sketch,
/// compilation, upload, dependency, menu, and serial-monitor workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `d5614acdca80871cf4db65843227223b5a0e3a2c`.
pub const ARDUINO_CLI_MODE_MELPA_PIN: (&str, &str) = ("arduino-cli-mode", "20260628.2219");

/// The exact aria2 package selected by the practical downloads-dashboard,
/// transfer-control, URL-dialog, and torrent-import workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `1f2cbe624f3a4e0109b5dc123bb4bbed496b15a7`.
pub const ARIA2_MELPA_PIN: (&str, &str) = ("aria2", "20230314.2131");

/// The exact Arjen Grey Theme package selected by the practical editor,
/// installed-loading, Helm selection, stacking, and restoration workflow
/// parity corpus. MELPA built this archive from upstream commit
/// `4cd0be72b65d42390e2105cfdaa408a1ead8d8d1`.
pub const ARJEN_GREY_THEME_MELPA_PIN: (&str, &str) = ("arjen-grey-theme", "20170522.2047");

/// The exact Ariadne package selected by the practical key-bound definition,
/// live BERT-RPC stream, navigation, reply, and offline workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `6fe401c7f996bcbc2f685e7971324c6f5e5eaf15`.
pub const ARIADNE_MELPA_PIN: (&str, &str) = ("ariadne", "20131117.1711");

/// The exact Art Bollocks Mode package selected by the practical documented
/// text-editing, comment/docstring review, customized editorial-policy, and
/// readability-metrics workflow parity corpus. MELPA built this archive from
/// upstream commit `63d20ed2846226f45b35eded69a776143a772ea4`.
pub const ARTBOLLOCKS_MODE_MELPA_PIN: (&str, &str) = ("artbollocks-mode", "20251211.1624");

/// The exact arview package selected by the comprehensive archive detection,
/// extraction, Dired lifecycle, process, and cleanup parity corpus.
pub const ARVIEW_MELPA_PIN: (&str, &str) = ("arview", "20160419.2109");

/// The exact ASCII Table package selected by the comprehensive formatting,
/// rendering, navigation, and command-workflow parity corpus.
pub const ASCII_TABLE_MELPA_PIN: (&str, &str) = ("ascii-table", "20231215.1527");

/// The exact Asilea package selected by the comprehensive annealing,
/// compiler-option, process, and callback parity corpus.
pub const ASILEA_MELPA_PIN: (&str, &str) = ("asilea", "20150105.1525");

/// The exact asm-blox package selected by the comprehensive parser, virtual
/// machine, gameboard, editor, persistence, and puzzle parity corpus.
pub const ASM_BLOX_MELPA_PIN: (&str, &str) = ("asm-blox", "20240106.1930");

/// The exact asn1-mode package selected by the comprehensive lexical,
/// indentation, font-lock, outline, and editing-workflow parity corpus. MELPA
/// built this archive from upstream commit
/// `d5d4a8259daf708411699bcea85d322f18beb972`.
pub const ASN1_MODE_MELPA_PIN: (&str, &str) = ("asn1-mode", "20170729.226");

/// The exact Assess package selected by the comprehensive buffer, filesystem,
/// indentation, fontification, discovery, and call-capture parity corpus.
/// MELPA built this archive from upstream commit
/// `cadeb24a5d8261fad4bdfdc09e7d571cc395a6ca`.
pub const ASSESS_MELPA_PIN: (&str, &str) = ("assess", "20240303.1454");

/// The exact astro-ts-mode package selected by the comprehensive mixed-language
/// Tree-sitter, indentation, font-lock, and editing-workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `1d24c9d399dee4cfea6ed9b49d8e08891665e16c`.
pub const ASTRO_TS_MODE_MELPA_PIN: (&str, &str) = ("astro-ts-mode", "20260417.101");

/// The exact Astute package selected by the comprehensive typography,
/// font-lock, customization, and minor-mode lifecycle parity corpus.
pub const ASTUTE_MELPA_PIN: (&str, &str) = ("astute", "20241015.444");

/// The exact Astyle package selected by the comprehensive argument selection,
/// formatter-command, region, buffer, failure, and on-save parity corpus.
/// MELPA built this archive from upstream commit
/// `04ff2941f08c4b731fe6a18ee1697436d1ca1cc0`.
pub const ASTYLE_MELPA_PIN: (&str, &str) = ("astyle", "20200328.616");

/// The exact ASX package selected by the comprehensive search, DOM,
/// request, navigation, and Org rendering parity corpus.
pub const ASX_MELPA_PIN: (&str, &str) = ("asx", "20191024.1100");

/// The exact async-await package selected by the comprehensive Promise,
/// generator, macro-expansion, and asynchronous-workflow parity corpus. MELPA
/// built this archive from upstream commit
/// `e0d15e8057ed7520100bc50c5552278292ebcb07`.
pub const ASYNC_AWAIT_MELPA_PIN: (&str, &str) = ("async-await", "20220827.437");

/// The exact async-backup package selected by the comprehensive path,
/// predicate, process, save-hook, and backup-lifecycle parity corpus. MELPA
/// built this archive from upstream commit
/// `d07a7bd4a5c3332a8a585680d67925385c595927`.
pub const ASYNC_BACKUP_MELPA_PIN: (&str, &str) = ("async-backup", "20230412.1534");

/// The exact async-http-queue package selected by the comprehensive state,
/// scheduling, response, callback, and lifecycle parity corpus. MELPA built
/// this archive from upstream commit
/// `bd37342372a0b24ce0d54e9dad8070af997b0a0b`.
pub const ASYNC_HTTP_QUEUE_MELPA_PIN: (&str, &str) = ("async-http-queue", "20260316.755");

/// The exact async-job-queue package selected by the comprehensive fixed-slot
/// dispatch, FIFO, callback, saturation, and lifecycle parity corpus.
/// MELPA built this archive from upstream commit
/// `eeafcce7f960305666b2a51aec55cc6333f6af1b`.
pub const ASYNC_JOB_QUEUE_MELPA_PIN: (&str, &str) = ("async-job-queue", "20230427.2122");

/// The exact async-status package selected by the comprehensive filesystem,
/// indicator-item, rendering, and progress-lifecycle parity corpus. MELPA
/// built this archive from upstream commit
/// `d2f5becc9850c26aa71fb581f9fc389eac740f52`.
pub const ASYNC_STATUS_MELPA_PIN: (&str, &str) = ("async-status", "20230821.204");

/// The exact atcoder-tools package selected by the comprehensive run
/// configuration, command construction, metadata, filesystem, and contest
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `cfe61ed18ea9b3b1bfb6f9e7d80a47599680cd1f`.
pub const ATCODER_TOOLS_MELPA_PIN: (&str, &str) = ("atcoder-tools", "20200109.1236");

/// The exact attrap package selected by the comprehensive option, diagnostic
/// dispatch, Elisp, GHC, HLint, LaTeX, and repair-workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `ad1d9443fcd93e32f2aefadc5af2646701664581`.
pub const ATTRAP_MELPA_PIN: (&str, &str) = ("attrap", "20260304.1504");

/// The exact atl-long-lines package selected by the comprehensive mode,
/// line-measurement, timer, toggle, and end-to-end workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `82cdd4edefba2d5b1d491bf3fcc487385819d713`.
pub const ATL_LONG_LINES_MELPA_PIN: (&str, &str) = ("atl-long-lines", "20240101.929");

/// The exact atl-markup package selected by the comprehensive cursor
/// classification, truncation, timer, and minor-mode workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `b616343ffe17060d521b214b8e90f5da1e880934`.
pub const ATL_MARKUP_MELPA_PIN: (&str, &str) = ("atl-markup", "20240101.933");

/// The exact atomic-chrome package selected by the comprehensive websocket,
/// browser-buffer, HTTP protocol, process, and server-lifecycle parity corpus.
/// MELPA built this archive from upstream commit
/// `f1b077be7e414f457191d72dcf5eedb4371f9309`.
pub const ATOMIC_CHROME_MELPA_PIN: (&str, &str) = ("atomic-chrome", "20230304.112");

/// The exact auth-source-gopass package selected by the comprehensive path,
/// process, backend-registration, cache, and credential-workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `6f7f0cc0d682f66d11f7fac4fa5c1e79904232da`.
pub const AUTH_SOURCE_GOPASS_MELPA_PIN: (&str, &str) = ("auth-source-gopass", "20230109.1213");

/// The exact auth-source-xoauth2 package selected by the comprehensive token,
/// credential-provider, transport, protocol, and auth-source-workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `99a03f8ce835412943d311b2746e77fcf5a1b500`.
pub const AUTH_SOURCE_XOAUTH2_MELPA_PIN: (&str, &str) = ("auth-source-xoauth2", "20220804.2219");

/// The exact aurel package selected by the comprehensive AUR URL,
/// parsing, filtering, package-management, and UI workflow parity corpus.
/// MELPA built this archive from upstream commit
/// `c571cc44ea3b9aa96399056bff22919efffbbb06`.
pub const AUREL_MELPA_PIN: (&str, &str) = ("aurel", "20260429.458");

/// The exact audacious package selected by the comprehensive command,
/// playlist, song-selection, metadata, and end-to-end playback parity corpus.
/// MELPA built this archive from upstream commit
/// `65c37f12a5c774a0ae434beee27ff7737006dd2f`.
pub const AUDACIOUS_MELPA_PIN: (&str, &str) = ("audacious", "20210917.51");

/// The exact aurora-config-mode package selected by the comprehensive
/// metadata, prompting, command, Python-derived mode, font-lock, and practical
/// configuration-workflow parity corpus. MELPA built this archive from
/// upstream commit `8273ec7937a21b469b9dbb6c11714255b890f410`.
pub const AURORA_CONFIG_MODE_MELPA_PIN: (&str, &str) = ("aurora-config-mode", "20180216.2302");

/// The exact auth-source-1password package selected by the comprehensive
/// metadata, secret-reference, CLI, backend, cache, and end-to-end auth-source
/// parity corpus. MELPA built this archive from upstream commit
/// `10961bdc8a3ed551dde29fde416843058bea2374`.
pub const AUTH_SOURCE_1PASSWORD_MELPA_PIN: (&str, &str) =
    ("auth-source-1password", "20260221.2058");

/// The exact auth-source-keytar package selected by the comprehensive
/// credential lookup, parsing, backend registration, cache, and auth-source
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `ae32dd807aa3cff59e4384ce8c9d7de259e45998`.
pub const AUTH_SOURCE_KEYTAR_MELPA_PIN: (&str, &str) = ("auth-source-keytar", "20251231.1726");

/// The exact auto-auto-indent package selected by the comprehensive
/// indentation, editing-command, post-command, timer, and practical typing
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `0139378577f936d34b20276af6f022fb457af490`.
pub const AUTO_AUTO_INDENT_MELPA_PIN: (&str, &str) = ("auto-auto-indent", "20131106.1903");

/// The exact es-lib package selected as auto-auto-indent's runtime utility
/// dependency. MELPA built this archive from upstream commit
/// `753b27363e39c10edc9e4e452bdbbbe4d190df4a`.
pub const ES_LIB_MELPA_PIN: (&str, &str) = ("es-lib", "20141111.1830");

/// The exact Esh Help package selected for practical Eshell command
/// discovery, ElDoc, manual synopsis, cache, and full-help parity. MELPA built
/// this archive from upstream commit
/// `417673ed18a983930a66a6692dbfb288a995cb80`.
pub const ESH_HELP_MELPA_PIN: (&str, &str) = ("esh-help", "20190905.22");

/// The exact ESS package selected for practical R source editing,
/// indentation, syntax/fontification, assignment and call filling,
/// navigation/indexing, command generation, project discovery, package
/// development, and inferior-mode parity. MELPA built this archive from
/// upstream commit `c3960e09f37550d300437c46ca03fb28975378a1`.
pub const ESS_MELPA_PIN: (&str, &str) = ("ess", "20260723.934");

/// The exact esup package selected for practical Emacs startup profiling,
/// child-process launch, result rendering, navigation, and visit parity.
/// MELPA built this archive from upstream commit
/// `4b49c8d599d4cc0fbf994e9e54a9c78e5ab62a5f`.
pub const ESUP_MELPA_PIN: (&str, &str) = ("esup", "20220202.2335");

/// The exact Keytar package selected as auth-source-keytar's runtime
/// credential-provider dependency and by the practical credential lifecycle,
/// shell-quoting, executable discovery, and npm installation parity corpus.
/// MELPA built this archive from upstream commit
/// `f0485df065bcdc8f446be3e00aa77a43629ec84e`.
pub const KEYTAR_MELPA_PIN: (&str, &str) = ("keytar", "20251231.1727");

/// The exact Llama package selected by the practical data-pipeline, callback,
/// closure, macro-contract, completion, and fontification parity corpus.
/// MELPA built this archive from upstream commit
/// `4d4024048053b898a01521046e0f063ee47615b0`.
pub const LLAMA_MELPA_PIN: (&str, &str) = ("llama", "20260601.1455");

/// The exact auto-async-byte-compile package selected by the comprehensive
/// metadata, save-hook, asynchronous process, status, display, and real
/// byte-compilation lifecycle parity corpus. MELPA built this archive from
/// upstream commit `8681e74ddb8481789c5dbb3cafabb327db4c4484`.
pub const AUTO_ASYNC_BYTE_COMPILE_MELPA_PIN: (&str, &str) =
    ("auto-async-byte-compile", "20160916.454");

/// The exact auto-compile package selected by the comprehensive source
/// recognition, byte-compilation, mode-line, save/load advice, recursive
/// toggle, and failure-recovery parity corpus. MELPA built this archive from
/// upstream commit `4db3a0e497feecc8b3dbeeefacdf363ae60a6392`.
pub const AUTO_COMPILE_MELPA_PIN: (&str, &str) = ("auto-compile", "20260601.1449");

/// The exact auto-dark package selected by the comprehensive metadata, theme,
/// customization, platform-detection, command-adapter, listener, timer, hook,
/// and global-mode lifecycle parity corpus. MELPA built this archive from
/// upstream commit `6d1e8d2fc493dccbf05c9191611805c7e7881c70`.
pub const AUTO_DARK_MELPA_PIN: (&str, &str) = ("auto-dark", "20260313.2356");

/// The exact auto-dictionary package selected by the comprehensive language
/// scoring, dictionary switching, idle-timer, Flyspell filtering, conditional
/// insertion, and multilingual workflow parity corpus. MELPA built this
/// archive from upstream commit `b364e08009fe0062cf0927d8a0582fad5a12b8e7`.
pub const AUTO_DICTIONARY_MELPA_PIN: (&str, &str) = ("auto-dictionary", "20150410.1610");

/// The exact auto-dim-other-buffers package selected by the comprehensive
/// face-remapping, window-selection, focus, customization, hook, advice, and
/// global-mode lifecycle parity corpus. MELPA built this archive from upstream
/// commit `cf0263073470190b85f6013066856126aac67d19`.
pub const AUTO_DIM_OTHER_BUFFERS_MELPA_PIN: (&str, &str) =
    ("auto-dim-other-buffers", "20260624.950");

/// The exact auto-highlight-symbol package selected by the comprehensive
/// symbol detection, overlay, navigation, edit, timer, lifecycle, and
/// multi-buffer workflow parity corpus. MELPA built this archive from
/// upstream commit `e84da32e7cf1baefb0a9eef42a2fc842cf18f8b3`.
pub const AUTO_HIGHLIGHT_SYMBOL_MELPA_PIN: (&str, &str) = ("auto-highlight-symbol", "20260101.552");

/// The exact auto-indent-mode package selected by the comprehensive
/// indentation, yank, deletion, kill, repository, lifecycle, and practical
/// editing workflow parity corpus. MELPA built this archive from upstream
/// commit `664006b67329a8e27330541547f8c2187dab947c`.
pub const AUTO_INDENT_MODE_MELPA_PIN: (&str, &str) = ("auto-indent-mode", "20211029.11");

/// The exact auto-minor-mode package selected by the comprehensive filename,
/// magic-content, advice, repeat-activation, use-package, and practical file
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `c62f4e04c7b73835c399f0348bea0ade2720bcbb`.
pub const AUTO_MINOR_MODE_MELPA_PIN: (&str, &str) = ("auto-minor-mode", "20180527.1123");

/// The exact auto-read-only package selected by the comprehensive filename
/// matching, project suppression, hook, global-mode, and practical read-only
/// workflow parity corpus. MELPA built this archive from upstream commit
/// `206d4559762fe6ef9e91de8f9dc43e1e41c0f42c`.
pub const AUTO_READ_ONLY_MELPA_PIN: (&str, &str) = ("auto-read-only", "20260521.1659");

/// The exact auto-org-md package selected by the comprehensive export,
/// hook-lifecycle, global-state, and practical Org-to-Markdown workflow parity
/// corpus. MELPA built this archive from upstream commit
/// `9318338bdb7fe8bd698d88f3af89b2d6413efdd2`.
pub const AUTO_ORG_MD_MELPA_PIN: (&str, &str) = ("auto-org-md", "20180213.2343");

/// The exact auto-package-update package selected by the comprehensive update
/// selection, scheduling, prompting, package transaction, results buffer, old
/// version cleanup, and async lifecycle parity corpus. MELPA built this archive
/// from upstream commit `e966c6c95de1742d867250dc15b1c6bd570b6ea5`.
pub const AUTO_PACKAGE_UPDATE_MELPA_PIN: (&str, &str) = ("auto-package-update", "20260601.1804");

/// The exact auto-pause package selected by the practical idle controller,
/// subprocess pause/resume, advice scope, sentinel, and failure-lifecycle
/// parity corpus. MELPA built this archive from upstream commit
/// `a4d778de774ca3895542cb559a953e0d98657338`.
pub const AUTO_PAUSE_MELPA_PIN: (&str, &str) = ("auto-pause", "20160426.1216");

/// The exact auto-rename-tag package selected by the practical HTML/XML
/// typing, nested-pairing, protected-context, customization, undo, and mode
/// lifecycle parity corpus. MELPA built this archive from upstream commit
/// `ace6de8bc8200aa9c9f37c8266d0e1b51627b559`.
pub const AUTO_RENAME_TAG_MELPA_PIN: (&str, &str) = ("auto-rename-tag", "20260101.547");

/// The exact auto-save-buffers-enhanced package selected by the practical
/// scheduled-save, filtering, quiet-save, activity, scratch-persistence, and
/// checkout-scoping parity corpus. MELPA built this archive from upstream
/// commit `461e8c816c1b7c650be5f209078b381fe55da8c6`.
pub const AUTO_SAVE_BUFFERS_ENHANCED_MELPA_PIN: (&str, &str) =
    ("auto-save-buffers-enhanced", "20161109.710");

/// The exact auto-save-visited-local-mode package selected by the practical
/// per-buffer timer, idle-save, predicate, interval-watcher, save-hook, error,
/// and buffer-lifecycle parity corpus. MELPA built this archive from upstream
/// commit `78a46d8a02360b4c63e45496bd32efe351459c81`.
pub const AUTO_SAVE_VISITED_LOCAL_MODE_MELPA_PIN: (&str, &str) =
    ("auto-save-visited-local-mode", "20251021.1126");

/// The exact auto-shell-command package selected by the practical save-hook,
/// asynchronous shell, queueing, command-selection, failure, toggle, and
/// settings-lifecycle parity corpus. MELPA built this archive from upstream
/// commit `a8f9213e3c773b5687b81881240e6e648f2f56ba`.
pub const AUTO_SHELL_COMMAND_MELPA_PIN: (&str, &str) = ("auto-shell-command", "20180817.1502");

/// The exact auto-space-mode package selected by the practical multilingual
/// typing, undo, region-editing, global-mode, selection-boundary, and
/// text-property parity corpus. MELPA built this archive from upstream commit
/// `38cd6bc259522250c1df88f24d0a3cc3727fb982`.
pub const AUTO_SPACE_MODE_MELPA_PIN: (&str, &str) = ("auto-space-mode", "20260204.255");

/// The exact auto-sort-mode package selected by the practical manual-sort,
/// save-hook, narrowing, delimiter-policy, mode-lifecycle, and text-property
/// parity corpus. MELPA built this archive from upstream commit
/// `3ffa4e2a76a6dda949fdfd200f623a17c4796559`.
pub const AUTO_SORT_MODE_MELPA_PIN: (&str, &str) = ("auto-sort-mode", "20230827.2124");

/// The exact Auto-YASnippet package selected for practical disposable-snippet
/// creation, Yasnippet expansion, mixed-case mirrors, region wrapping,
/// history, export, and persistence parity. MELPA built version 1.0.0 from
/// upstream commit `6a9e406d0d7f9dfd6dff7647f358cb05a0b1637e`.
pub const AUTO_YASNIPPET_MELPA_PIN: (&str, &str) = ("auto-yasnippet", "20230208.331");

/// The exact Bind Map package selected for practical global leader,
/// major/minor-mode activation, remapping/aliasing, inherited declaration,
/// override-precedence, and Evil-state parity. MELPA built this archive from
/// upstream commit `75aac732c10d97bc8dc49196c6623a09faf30d37`.
pub const BIND_MAP_MELPA_PIN: (&str, &str) = ("bind-map", "20251119.201");

/// The exact ht package selected by the practical configuration, nested state,
/// job pipeline, custom-key, and snapshot parity corpus, and as
/// auto-highlight-symbol's hash-table dependency. MELPA built this archive from upstream commit
/// `1c49aad1c820c86f7ee35bf9fff8429502f60fef`.
pub const HT_MELPA_PIN: (&str, &str) = ("ht", "20230703.558");

/// The exact Htmlize package selected for practical syntax-highlighted source
/// publishing, rich region copying, CSS/inline/font rendering, link/image and
/// visibility transformations, and batch file conversion parity. MELPA built
/// this archive from upstream commit
/// `c9a8196a59973fabb3763b28069af9a4822a5260`.
pub const HTMLIZE_MELPA_PIN: (&str, &str) = ("htmlize", "20250724.1703");

/// The exact Hungry Delete package selected for practical whitespace cleanup,
/// conservative word joining, region and prefix deletion, overwrite-mode,
/// protected-text boundaries, and mode-lifecycle parity. MELPA built this
/// archive from upstream commit `d919e555e5c13a2edf4570f3ceec84f0ade71657`.
pub const HUNGRY_DELETE_MELPA_PIN: (&str, &str) = ("hungry-delete", "20210409.1643");

/// The exact Hydra package selected by the practical command-family,
/// transient-keymap, extension, radio, and source-editing parity corpus.
/// MELPA built this archive from upstream commit
/// `59a2a45a35027948476d1d7751b0f0215b1e61aa`.
/// The exact Hy Mode package selected for practical `.hy' activation through
/// the auto-mode-alist autoload, Lisp-derived syntax table with Hy character
/// classes, font-lock corpus over a real Hy program, specialized lisp
/// indentation, and shell/describe/pdb command surface parity. MELPA built
/// this archive from upstream commit `df814865a1faa8414dacdbb35b2a9029995312ec`.
pub const HY_MODE_MELPA_PIN: (&str, &str) = ("hy-mode", "20211016.2011");

pub const HYDRA_MELPA_PIN: (&str, &str) = ("hydra", "20250316.1254");

/// The exact Highlight package selected for practical overlay and text
/// property lifecycles, regexp groups, symbol navigation, property transfer,
/// duplicate-log detection, invisibility, and semantic-property parity.
/// MELPA built this archive from upstream commit
/// `28557cb8d99b96eb509aaec1334c7cdda162517f`.
pub const HIGHLIGHT_MELPA_PIN: (&str, &str) = ("highlight", "20210318.2248");

/// The exact Highlight Indentation package selected for practical guide,
/// live-edit, blank-line, current-column, and offset-selection parity.
/// MELPA built this archive from upstream commit
/// `d88db4248882da2d4316e76ed673b4ac1fa99ce3`.
pub const HIGHLIGHT_INDENTATION_MELPA_PIN: (&str, &str) =
    ("highlight-indentation", "20210221.1418");

/// The exact Highlight Numbers package selected for practical Emacs Lisp,
/// Scheme, C-family, inherited customization, opt-out, editing, and mode
/// lifecycle parity. MELPA built this archive from upstream commit
/// `8b4744c7f46c72b1d3d599d4fb75ef8183dee307`.
pub const HIGHLIGHT_NUMBERS_MELPA_PIN: (&str, &str) = ("highlight-numbers", "20181013.1744");

/// The exact Highlight Parentheses package selected for practical nested
/// syntax, navigation, adjacent delimiters, unbalanced editing, live
/// customization, mode-lifecycle, and debounce parity. MELPA built this
/// archive from upstream commit `965b18dd69eff4457e17c9e84b3cbfdbfca2ddfb`.
pub const HIGHLIGHT_PARENTHESES_MELPA_PIN: (&str, &str) =
    ("highlight-parentheses", "20240408.1126");

/// The exact Ido Completing Read+ package selected for practical ubiquitous
/// mode, transformed collections/defaults, caller policy, dynamic collection,
/// fallback, and minibuffer-hook lifecycle parity. This is the current archive
/// containing the formerly separate `ido-ubiquitous` package. MELPA built it
/// from upstream commit `1609049c0a9b3f674ffff3083adc8f5359746fa9`.
pub const IDO_UBIQUITOUS_MELPA_PIN: (&str, &str) = ("ido-completing-read+", "20240130.30");

/// The exact Indent Guide package selected for practical space, tab, blank
/// line, recursive, live-editing, mode-lifecycle, and debounce parity. MELPA
/// built this archive from upstream commit
/// `1332f95d6f08afee35f62621793e2622b9f86f27`.
pub const INDENT_GUIDE_MELPA_PIN: (&str, &str) = ("indent-guide", "20260629.918");

/// The exact Ido Vertical Mode package selected for real minibuffer rendering,
/// candidate navigation, confirmation, mode lifecycle, and file-selection
/// parity. MELPA built this archive from upstream commit
/// `58ad6d8b645e6211c7c564a4fbebf39a72691c7e`.
pub const IDO_VERTICAL_MODE_MELPA_PIN: (&str, &str) = ("ido-vertical-mode", "20260420.1855");

/// The exact Impatient Mode package selected for practical public publishing,
/// real loopback HTTP, long-poll, related-resource, filter, timer, and cleanup
/// parity. MELPA built this archive from upstream commit
/// `4bb8009c6c6a6339a8fd7b4dea4a165af3721812`.
pub const IMPATIENT_MODE_MELPA_PIN: (&str, &str) = ("impatient-mode", "20260426.1323");

/// The exact Ledger Mode package selected for practical journal editing,
/// completion, navigation, transaction state, occur, schedules, reports, and
/// source-navigation parity. MELPA built this archive from upstream commit
/// `b0e71b7e9ee612ccb0b0e5f8bfefcfddb69ae861`.
pub const LEDGER_MODE_MELPA_PIN: (&str, &str) = ("ledger-mode", "20260727.518");

/// The exact Leuven Theme package selected for practical light and dark
/// theme lifecycle, real fontified Org/Elisp/diff rendering, display-gate,
/// and scaling-customization parity. MELPA built this archive from upstream
/// commit `c3546e6a84c138fd8cdbd33998fefcf834c45018`.
pub const LEUVEN_THEME_MELPA_PIN: (&str, &str) = ("leuven-theme", "20260213.1052");

/// The exact Iedit package selected by the practical symbol-refactoring,
/// scoped replay, visibility, buffering/undo, and rectangle-editing corpus.
/// MELPA built this archive from upstream commit
/// `7e513d573c6a5dd2a01aeeb1d8587d74630a2f80`.
pub const IEDIT_MELPA_PIN: (&str, &str) = ("iedit", "20251017.410");

/// The exact Package-Build package selected by the practical recipe,
/// file-plan, reproducible archive, archive-maintenance, and authoring corpus.
/// MELPA built this archive from upstream commit
/// `80206e27d7b007464e6b28e8150662ba9d14f2bc`.
pub const PACKAGE_BUILD_MELPA_PIN: (&str, &str) = ("package-build", "20260731.2245");

/// The exact pcre2el package selected for practical PCRE translation,
/// production-log parsing, multiline extraction, redaction, finite-language,
/// source-editing, cache, mode-lifecycle, and validation parity. MELPA built
/// this archive from upstream commit
/// `b4d846d80dddb313042131cf2b8fbf647567e000`.
pub const PCRE2EL_MELPA_PIN: (&str, &str) = ("pcre2el", "20240629.2322");

/// The exact inf-ruby package selected by the practical comint-mode, source
/// dispatch, completion, project-console, and debugger lifecycle parity corpus.
/// MELPA built this archive from upstream commit
/// `274398a24288a7db430a656b580ffbf889ca02aa`.
pub const INF_RUBY_MELPA_PIN: (&str, &str) = ("inf-ruby", "20251224.216");

/// The exact Imenu-List package selected for practical source indexing,
/// sidebar rendering, navigation, folding, refresh, unavailable-index,
/// translated-position, window, timer, and mode-lifecycle parity. MELPA built
/// this archive from upstream commit
/// `76f2335ee6f2f066d87fe4e4729219d70c9bc70d`.
pub const IMENU_LIST_MELPA_PIN: (&str, &str) = ("imenu-list", "20210420.1200");

/// The exact Imenu Anywhere package selected for practical cross-buffer
/// completion, real Imenu indexing, Xref round trips, cache invalidation,
/// narrowing, and failure-lifecycle parity. MELPA built this archive from
/// upstream commit `06ec33d79e33edf01b9118aead1eabeae8ee08b1`.
pub const IMENU_ANYWHERE_MELPA_PIN: (&str, &str) = ("imenu-anywhere", "20210201.1704");

/// The exact iter2 package selected by the practical resumable-workflow,
/// composition, resource-cleanup, editor-state, nonlocal-exit, and tracing
/// parity corpus. MELPA built this archive from upstream commit
/// `632232b5ee627bf5d299db0b7714b3b687a0124c`.
pub const ITER2_MELPA_PIN: (&str, &str) = ("iter2", "20250209.1516");

/// The exact Ivy package selected by the practical interactive selection,
/// action dispatch, completing-read lifecycle, search-language,
/// completion-in-region, and resumable-session parity corpus. MELPA built
/// this archive from upstream commit
/// `0d02f5063d36ff4fa6138f0973c83c6d3874fba0`.
pub const IVY_MELPA_PIN: (&str, &str) = ("ivy", "20260413.2102");

/// The exact Ivy Hydra package selected for practical minibuffer navigation,
/// option toggling, action dispatch, and action-menu parity. MELPA built this
/// archive from upstream commit `4defb814ce00fbbc2cf2ad626e630a39f4da1456`.
pub const IVY_HYDRA_MELPA_PIN: (&str, &str) = ("ivy-hydra", "20260213.941");

/// The exact Ivy Rich package selected by the practical transformer,
/// buffer-dashboard, project-cache, file/bookmark, and package-catalog parity
/// corpus. MELPA built this archive from upstream commit
/// `aff9b6bd53e0fdcf350ab83c90e64e651b47dba4`.
pub const IVY_RICH_MELPA_PIN: (&str, &str) = ("ivy-rich", "20230425.1422");

/// The exact JSON Mode package selected for practical file association,
/// formatting, structural navigation, JSONC, value editing, and JSON-path
/// parity. MELPA built this archive from upstream commit
/// `77125b01c0ddce537085201098bea9b4b8ba6be3`.
pub const JSON_MODE_MELPA_PIN: (&str, &str) = ("json-mode", "20240427.1245");

/// The exact JSON Navigator package selected for practical region, after-point,
/// hierarchy rendering, widget expansion, truncation, and recovery parity.
/// MELPA built this archive from upstream commit
/// `8ab49b066bc23de731a29ef07bbafa29999e1852`.
pub const JSON_NAVIGATOR_MELPA_PIN: (&str, &str) = ("json-navigator", "20241031.630");

/// The exact JSON Reformat package selected for practical document, selected
/// region, indentation, string-policy, object-order, scalar-root, and parser
/// diagnostic parity. MELPA built this archive from upstream commit
/// `e9999b1f1fc933c02ff44f4136602b6a45ed59c6`.
pub const JSON_REFORMAT_MELPA_PIN: (&str, &str) = ("json-reformat", "20220905.2342");

/// The exact JSON Snatcher package selected for practical nested object and
/// array paths, Python/jq output, kill-ring, token-boundary, cache reuse, and
/// buffer-lifecycle parity. MELPA built this archive from upstream commit
/// `b28d1c0670636da6db508d03872d96ffddbc10f2`.
pub const JSON_SNATCHER_MELPA_PIN: (&str, &str) = ("json-snatcher", "20200916.1717");

/// The exact js2-mode package selected by the practical parsing, diagnostics,
/// indentation, navigation, JSON-path, Imenu, and editor-aid parity corpus.
/// MELPA built this archive from upstream commit
/// `41d0e7f5ef51109c682016baa6fc6846e03e8517`.
pub const JS2_MODE_MELPA_PIN: (&str, &str) = ("js2-mode", "20260627.1342");

/// The exact js-doc package selected for practical JsDoc function metadata
/// parsing, parameter/return tag insertion, and format-string expansion.
/// MELPA built this archive from upstream commit
/// `f0606e89d5aa89146f96edb38cf69af0068a9d1e`.
pub const JS_DOC_MELPA_PIN: (&str, &str) = ("js-doc", "20160715.434");

/// The exact Multi-Line package selected for practical language-aware
/// expression formatting, repeated-command cycling, explicit single-lining,
/// candidate highlighting, mode-hook lifecycle, and malformed-form parity.
/// MELPA built this archive from upstream commit
/// `06ea7294c4e4ace0c3253b7952a6d937a169eb55`.
pub const MULTI_LINE_MELPA_PIN: (&str, &str) = ("multi-line", "20230721.1814");

/// The exact multiple-cursors package selected for practical multi-line,
/// occurrence-based, ordered, region-transforming, alignment, lifecycle, and
/// focused-context editing parity, and required by js2-refactor's scope-aware
/// rename workflow. This MELPA version is pinned to upstream commit
/// `94b8b07a4bab87f803123723b68227565429dfa1`.
pub const MULTIPLE_CURSORS_MELPA_PIN: (&str, &str) = ("multiple-cursors", "20260419.931");

/// The exact MWIM package selected for practical logical, visual, comment,
/// prefix, configurable-cycle, Shift Selection, and composed editing parity.
/// MELPA built this archive from upstream commit
/// `e44a7a0a76262d7000b726bfa848cc62e21d7985`.
pub const MWIM_MELPA_PIN: (&str, &str) = ("mwim", "20260227.705");

/// The exact Nameless package selected for practical namespace discovery,
/// font-lock presentation, filtered copying, alias insertion, private and
/// non-hyphen namespaces, file-local refresh, and mode-lifecycle parity.
/// MELPA built this archive from upstream commit
/// `e468f3eea4518b9827419611868c897dce20453f`.
pub const NAMELESS_MELPA_PIN: (&str, &str) = ("nameless", "20230112.1259");

/// The exact Names package selected for practical collision-free modules,
/// split declarations, macro pipelines, keyword APIs, derived modes, and
/// customization metadata parity. This MELPA version is pinned to upstream
/// commit `45a272fae915148d9a74d4cb3c39917b272ee9c3`.
pub const NAMES_MELPA_PIN: (&str, &str) = ("names", "20221227.1825");

/// The exact Navi2ch package selected for practical archived-post rendering,
/// HTTP compatibility, request and cookie construction, multi-backend URL
/// routing, local-board persistence, and image metadata parity. This MELPA
/// version is pinned to upstream commit
/// `7811dba052f679bd920a1f648d621a6fecace10f`.
pub const NAVI2CH_MELPA_PIN: (&str, &str) = ("navi2ch", "20200130.36");

/// The exact Nerd Icons package selected for practical styled-glyph,
/// file/mode/directory routing, dashboard, interactive insertion, and cache
/// parity. This MELPA version is pinned to upstream commit
/// `674909974637ff0ec2b5ebf43f9a8aefa35d93e9`.
pub const NERD_ICONS_MELPA_PIN: (&str, &str) = ("nerd-icons", "20260710.1627");

/// The exact Page-Break-Lines package selected for practical page navigation,
/// display-table ownership, live window refresh, customization, and global
/// mode parity. MELPA version `20250218.1607` packages upstream source version
/// 0.15 at commit `982571749c8fe2b5e2997dd043003a1b9fe87b38`.
pub const PAGE_BREAK_LINES_MELPA_PIN: (&str, &str) = ("page-break-lines", "20250218.1607");

/// The exact Paradox package selected for practical package-menu rendering,
/// filtering, transaction reporting, homepage, GitHub-star, commit-history,
/// and enable/disable lifecycle parity. MELPA built this archive from upstream
/// commit `96401577ed02f433debe7604e49afd478e9eda61`.
pub const PARADOX_MELPA_PIN: (&str, &str) = ("paradox", "20191011.1119");

/// The exact Paredit package selected for practical balanced insertion,
/// structural deletion, slurp/barf, splice/raise/convolute, split/join,
/// string/comment editing, kill-ring, and mode-lifecycle parity. MELPA built
/// this archive from upstream commit
/// `89e75b4cb21f525a6f4cabcd12f1bd4204e682ab`.
pub const PAREDIT_MELPA_PIN: (&str, &str) = ("paredit", "20241103.2046");

/// The exact parseclj package selected for practical Clojure/EDN lexing,
/// AST construction, and unparse round-trips without a live Clojure process.
/// MELPA built this archive from upstream commit
/// `ca828c202c026e45bd60503984cf510d904cae50`.
pub const PARSECLJ_MELPA_PIN: (&str, &str) = ("parseclj", "20260526.1843");

/// The exact parseedn package selected for practical EDN read/print round-trips
/// of scalars, vectors, maps, and tagged literals. MELPA built this archive
/// from upstream commit `1a28a88e2aabd99b41e02f491d6b8874ec128d7d`.
pub const PARSEEDN_MELPA_PIN: (&str, &str) = ("parseedn", "20260601.1258");

/// The exact Parent Mode package selected for practical derived-mode feature
/// dispatch, built-in hierarchies, aliased parents, runtime reparenting,
/// additional-parent comparison, and failure-contract parity. MELPA built
/// this archive from upstream commit `9fe5363b2a190619641c79b3a40d874d8c8f9f40`.
pub const PARENT_MODE_MELPA_PIN: (&str, &str) = ("parent-mode", "20240210.1906");

/// The exact Pcache package selected for practical repository lifecycle,
/// nested-path persistence, mixed Lisp and EIEIO values, deterministic
/// expiration, delayed saves, malformed-file recovery, and stale-version
/// acceptance parity. MELPA built this archive from upstream commit
/// `17d785afa4532043afa8b2dc9ae3d9733528e758`.
pub const PCACHE_MELPA_PIN: (&str, &str) = ("pcache", "20260728.1657");

/// The exact PDF Tools package selected for practical document geometry,
/// search-query, outline/link, occurrence-batching, virtual-page, annotation,
/// sequence-alignment, and cache parity. This MELPA version is pinned to
/// upstream commit e4b7f1f37cf59ddf025d609ffcdabe732a6e99ba.
pub const PDF_TOOLS_MELPA_PIN: (&str, &str) = ("pdf-tools", "20260102.1101");

/// The exact persp-mode package selected for practical workspace lifecycle,
/// shared-buffer ownership, real window-layout, persistence, and automatic
/// perspective parity. MELPA built this archive from upstream commit
/// `fab4bf76927445d2e431f06e74572acba81f47d5`.
pub const PERSP_MODE_MELPA_PIN: (&str, &str) = ("persp-mode", "20250830.955");

/// The exact PHP Mode package selected for practical PHP 8 indentation,
/// fontification, syntax, Imenu, navigation, completion, template routing,
/// and Composer-project parity. This MELPA version is pinned to upstream
/// commit 6ebe4a618aa64db3e15f809b036c1b1a6d05c030.
pub const PHP_MODE_MELPA_PIN: (&str, &str) = ("php-mode", "20260719.209");

/// The exact PHP Runtime package selected for practical inline execution,
/// string/buffer/file stdin, NUL-safe script fallback, output ownership,
/// error handling, quoting, and extension-query parity. This MELPA version is
/// pinned to upstream commit 37beef404c70d7b80dc085b1ee1e13fd9c375fe6.
pub const PHP_RUNTIME_MELPA_PIN: (&str, &str) = ("php-runtime", "20241024.1622");

/// The exact Pfuture package selected for practical asynchronous stdout/stderr,
/// timeout, parallel-await, callback, working-directory, custom-filter,
/// startup-failure, and cleanup parity. MELPA built this archive from upstream
/// commit `19b53aebbc0f2da31de6326c495038901bffb73c`.
pub const PFUTURE_MELPA_PIN: (&str, &str) = ("pfuture", "20220913.1401");

/// The exact Pinyinlib package selected for practical contact, locale,
/// punctuation, literal-metacharacter, buffer-navigation, and case-folding
/// search parity. This MELPA version is pinned to upstream commit
/// 1772c79b6f319b26b6a394a8dda065be3ea4498d.
pub const PINYINLIB_MELPA_PIN: (&str, &str) = ("pinyinlib", "20200911.1723");

/// The exact Pip Requirements package selected for practical file detection,
/// PEP 440 fontification, comments, built-in CAPF completion, PyPI simple-index
/// parsing, and optional Auto Complete integration parity. MELPA built version
/// 0.7 from upstream commit `31e0dc62abb2d88fa765e0ea88b919d756cc0e4f`.
pub const PIP_REQUIREMENTS_MELPA_PIN: (&str, &str) = ("pip-requirements", "20240621.2151");

/// The exact Pipenv porcelain selected for practical command-process,
/// activation, environment-file, module-opening, shell, and minor-mode parity.
/// MELPA built this archive from upstream commit
/// `3af159749824c03f59176aff7f66ddd6a5785a10`.
pub const PIPENV_MELPA_PIN: (&str, &str) = ("pipenv", "20220514.123");

/// The exact Pkg Info package selected for practical library-header,
/// defining-function, installed-package, version-precedence, MELPA-recipe,
/// HTTP/JSON lifecycle, and diagnostic parity. This MELPA version is pinned to
/// upstream commit 4dbe328c9eced79e0004e3fdcd7bfb997a928be5.
pub const PKG_INFO_MELPA_PIN: (&str, &str) = ("pkg-info", "20150517.1143");

/// The exact js2-refactor package selected by the practical scope rewrite,
/// signature migration, extraction, IIFE, and structural editing parity
/// corpus. MELPA built this archive from upstream commit
/// `e1177c728ae52a5e67157fb18ee1409d8e95386a`.
pub const JS2_REFACTOR_MELPA_PIN: (&str, &str) = ("js2-refactor", "20250210.1811");

/// The exact key-chord package selected for practical two-key and
/// double-tap chord definition, input-method lookup, typing detection,
/// and macro/read-char recovery parity. MELPA built this archive from
/// upstream commit `cb646e815c61f253ad9fdfbe058049dda4e2b32b`.
pub const KEY_CHORD_MELPA_PIN: (&str, &str) = ("key-chord", "20250330.2011");

/// The exact Keyfreq package selected by the practical command accounting,
/// report generation, export, cooperative persistence, and autosave lifecycle
/// parity corpus. MELPA built this archive from upstream commit
/// `c6955162307f37c2ac631d9daf118781009f8dda`.
pub const KEYFREQ_MELPA_PIN: (&str, &str) = ("keyfreq", "20231107.106");

/// The exact Leaf package selected for practical declarative expansion,
/// deferred autoload, mode/hook/key dispatch, variable configuration,
/// protected-failure recovery, and definition-navigation parity. MELPA built
/// this archive from upstream merge `b49e68613b8efba89c702141f49ad9b4460a7204`.
pub const LEAF_MELPA_PIN: (&str, &str) = ("leaf", "20260302.652");

/// The exact linum-relative package selected for practical relative-number
/// rendering, customization, backend ownership, and global-mode lifecycle
/// parity. MELPA built this archive from upstream commit
/// `8fbe89ad897921849665a3e8da18cee7d0721441`.
pub const LINUM_RELATIVE_MELPA_PIN: (&str, &str) = ("linum-relative", "20221025.517");

/// The exact LSP Docker package selected for practical legacy and persistent
/// client registration, project path mapping, container discovery, Docker
/// command construction, and registration-failure parity. MELPA built this
/// archive from upstream commit `f666fba72b496c7750bb3f349771b07aa51714f0`.
pub const LSP_DOCKER_MELPA_PIN: (&str, &str) = ("lsp-docker", "20260507.1750");

/// The exact LSP Java package selected for practical JDT-LS startup, Java
/// project build/configuration, source-action, hierarchy, class-file, and
/// debugger/test workflow parity. MELPA built this archive from upstream
/// commit `5294db2ac033a289e4878fa8386629b75cb3ccb6`.
/// The exact LSP Haskell package selected for practical client registration
/// with lsp-mode, language-id mapping for the five Haskell modes, the
/// customization surface, pure server-command assembly, and the code-action
/// boolean-filter parity. MELPA built this archive from upstream commit
/// `4c3001aeb116fb489223269ea353359b90e2a5e1`.
pub const LSP_HASKELL_MELPA_PIN: (&str, &str) = ("lsp-haskell", "20260507.1745");

pub const LSP_JAVA_MELPA_PIN: (&str, &str) = ("lsp-java", "20260510.647");

/// The exact LSP Mode package selected for practical JSON-RPC transport,
/// Unicode position, workspace edit, completion, diagnostics, and URI parity.
/// MELPA built this archive from upstream commit
/// `6bfc593d7b1bc0dd656f09ffce52cc085ebced05`.
pub const LSP_MODE_MELPA_PIN: (&str, &str) = ("lsp-mode", "20260716.755");

/// The exact LSP Origami package selected for practical documented-hook,
/// capability, folding-range conversion, real Origami overlay, and mode
/// lifecycle parity. MELPA built this archive from upstream commit
/// `dd398afcf8e9077231dc26ea189916e6ea64c6ab`.
pub const LSP_ORIGAMI_MELPA_PIN: (&str, &str) = ("lsp-origami", "20260507.1743");

/// The exact LSP Pyright package selected for practical Python workspace
/// startup, configuration, editing, import organization, environment
/// discovery, and language-server failure parity. MELPA built this archive
/// from upstream commit `187e08caee4e1630a9975f492274c739f325392f`.
pub const LSP_PYRIGHT_MELPA_PIN: (&str, &str) = ("lsp-pyright", "20260507.1742");

/// The exact historical Microsoft Python Language Server client selected for
/// practical client registration, Python environment discovery,
/// initialization, progress rendering, and installer lifecycle parity. MELPA
/// built this archive from upstream commit
/// `7bda327bec7b219d140c34dab4b1e1fbd41bc516`.
pub const LSP_PYTHON_MS_MELPA_PIN: (&str, &str) = ("lsp-python-ms", "20230731.1458");

/// The exact LSP UI package selected for practical hover-documentation,
/// sideline, peek, Imenu, diagnostics, reference-navigation, and lifecycle
/// parity. MELPA built this archive from upstream commit
/// `8d888a3ab1ba9e46bd4711398c57d39d0b709a45`.
pub const LSP_UI_MELPA_PIN: (&str, &str) = ("lsp-ui", "20260512.1516");

/// The exact lv package required by the practical Hydra parity corpus and
/// selected for the hint-window lifecycle, refresh, layout, GUI separator,
/// failure-atomicity, and pre-existing-buffer parity corpus. MELPA built this
/// archive from upstream commit
/// `87873d788891029d9e44fa5458321d6a05849b94`.
pub const LV_MELPA_PIN: (&str, &str) = ("lv", "20200507.1518");

/// The exact m-buffer package selected for the scoped search, marker-safe
/// rewrite, line classification, log segmentation, annotation, and stateless
/// location parity corpus. MELPA built this archive from upstream commit
/// `5e7714835b2289f61dad24c0b5cf98d28fc313b0`.
pub const M_BUFFER_MELPA_PIN: (&str, &str) = ("m-buffer", "20241215.2214");

/// The exact Macrostep package selected for the practical inline expansion,
/// nested lifecycle, local environment, compiler macro, pretty-printing,
/// separate-buffer, and failure-atomicity parity corpus. MELPA built this
/// archive from upstream commit
/// `d0928626b4711dcf9f8f90439d23701118724199`.
pub const MACROSTEP_MELPA_PIN: (&str, &str) = ("macrostep", "20250202.2205");

/// The exact Mag Menu package selected for the practical command option,
/// rendered menu, keyboard interaction, action dispatch, help, and
/// splitter-backed window lifecycle parity corpus. MELPA built this archive
/// from upstream commit
/// `9b9277021cd09fb1dba64b1d2a00705d20914bd6`.
pub const MAG_MENU_MELPA_PIN: (&str, &str) = ("mag-menu", "20150505.1850");

/// The exact Makey package selected for the practical generated-command,
/// mixed command-line/Lisp option, rendered popup, keyboard dispatch, help,
/// literal action, and window restoration parity corpus. MELPA built this
/// archive from upstream commit
/// `a61781e69d3b451551e269446e1c5f624ab81137`.
pub const MAKEY_MELPA_PIN: (&str, &str) = ("makey", "20131231.1430");

/// The exact Markdown Mode package selected for the practical release-note
/// editing, outline reorganization, task-list, reference and footnote,
/// report-table, and fenced-code parsing parity corpus. MELPA built this
/// archive from upstream commit
/// `f441e8bc9951e73b12c61e9198658488dd8e86e1`.
pub const MARKDOWN_MODE_MELPA_PIN: (&str, &str) = ("markdown-mode", "20260722.40");

/// The exact Markdown-Toc package selected for practical README generation,
/// refresh, customization, anchor, navigation, deletion, and mode-lifecycle
/// parity. MELPA built this archive from upstream commit
/// `d22633b654193bcab322ec51b6dd3bb98dd5f69f`.
pub const MARKDOWN_TOC_MELPA_PIN: (&str, &str) = ("markdown-toc", "20260131.1444");

/// The exact Marshal package selected for practical EIEIO alist, plist,
/// JSON, typed recursive value, subclass-discriminator, custom-driver, and
/// failure/recovery parity. MELPA built this archive from upstream commit
/// `490496d974d03906f784707ecc2e0ac36ed84b96`.
pub const MARSHAL_MELPA_PIN: (&str, &str) = ("marshal", "20201223.1853");

/// The exact Math Symbol Lists package selected for the practical completion,
/// Unicode formula rendering, package-requirement, conflict-resolution,
/// scripted-character, and full-corpus integrity parity suite. MELPA built
/// this archive from upstream commit
/// `ac3eb053d3b576fcdd192b0ac6ad5090ea3a7079`.
pub const MATH_SYMBOL_LISTS_MELPA_PIN: (&str, &str) = ("math-symbol-lists", "20220828.2047");

/// The exact Maude Mode package selected for the practical module editing,
/// indentation, navigation, abbrev authoring, source transport, and inferior
/// diagnostic parity corpus. MELPA built this archive from upstream commit
/// `2e1f68a890493d964f933d6e40b0ede047f70ede`.
pub const MAUDE_MODE_MELPA_PIN: (&str, &str) = ("maude-mode", "20230504.937");

/// The exact Mozc package selected for the practical input-mode lifecycle,
/// key translation, placeholder editing, preedit and candidate rendering,
/// helper framing, and session protocol parity corpus. This MELPA version is
/// pinned to upstream commit
/// `76887c679e1e4f156102e4bc62ea9cf9174678a3`.
pub const MOZC_MELPA_PIN: (&str, &str) = ("mozc", "20260624.1355");

/// The exact Dash package selected by the live lifecycle and comprehensive
/// API parity corpora.
pub const DASH_MELPA_PIN: (&str, &str) = ("dash", "20260221.1346");

/// The exact Dash Functional compatibility package selected for practical
/// legacy-consumer feature loading, combinator pipelines, callbacks,
/// convergence, and obsolescence-warning parity. MELPA built this archive
/// from upstream commit `fcb5d831fc08a43f984242c7509870f30983c27c`.
pub const DASH_FUNCTIONAL_MELPA_PIN: (&str, &str) = ("dash-functional", "20250312.1307");

/// The exact Evil package selected by the comprehensive API parity corpus.
pub const EVIL_MELPA_PIN: (&str, &str) = ("evil", "20260603.654");

/// The exact Evil Easymotion package selected for practical key-driven jump,
/// Evil operator, line action, scoped collection, multi-window, and
/// cancellation parity. MELPA built this archive from upstream commit
/// `629c894af63336028a61cc93d6465d10837eb82b`.
pub const EVIL_EASYMOTION_MELPA_PIN: (&str, &str) = ("evil-easymotion", "20260602.2314");

/// The final Evil Magit snapshot selected for practical Vim-style Magit
/// status, rebase, text-mode, yank, and reversible-setup parity. MELPA built
/// this archive from upstream commit
/// `f2e8dddbb22f6f300f2a8f05fe0444414ce71e04`.
pub const EVIL_MAGIT_MELPA_PIN: (&str, &str) = ("evil-magit", "20210117.1749");

/// The exact evil-org package selected for practical org-item open/insert
/// commands, empty-element detection, and key-theme population. MELPA built
/// this archive from upstream commit `b1f309726b1326e1a103742524ec331789f2bf94`.
pub const EVIL_ORG_MELPA_PIN: (&str, &str) = ("evil-org", "20221001.2335");

/// The exact Evil Cleverparens package selected for practical modal structural
/// editing, navigation, form transformation, and lifecycle parity. MELPA built
/// this archive from upstream commit
/// `4c413a132934695b975004d429b0b0a6e3d8ca38`.
pub const EVIL_CLEVERPARENS_MELPA_PIN: (&str, &str) = ("evil-cleverparens", "20250518.1741");

/// The exact Evil Visual Mark Mode package selected for practical local and
/// global mark rendering, live edit tracking, state visibility, deletion, and
/// lifecycle parity. MELPA built this archive from upstream commit
/// `2bbaaae56ae53e68a8bcc7bc2cfe830a14843b4d`.
pub const EVIL_VISUAL_MARK_MODE_MELPA_PIN: (&str, &str) = ("evil-visual-mark-mode", "20230202.318");

/// The exact Fill Column Indicator package selected for practical rule
/// placement, live editing, display-table coexistence, mode lifecycle,
/// overlay competition, and textual/bitmap rendering parity. MELPA built
/// this archive from upstream commit
/// `c35f9de072c241699b57bcb46da84bed5af29cfe`.
pub const FILL_COLUMN_INDICATOR_MELPA_PIN: (&str, &str) =
    ("fill-column-indicator", "20200806.2239");

/// The exact Evil Collection package selected for practical deferred setup,
/// Calendar, Dired, Help, compilation, binding-policy, and button-dispatch
/// parity. MELPA built this archive from upstream commit
/// `fa8da0ebba4bbf2a84a78183420d8303179ef427`.
pub const EVIL_COLLECTION_MELPA_PIN: (&str, &str) = ("evil-collection", "20260729.1654");

/// The final Evil Ediff package published by MELPA before the recipe was
/// removed in favor of Evil Collection. The historical single-file package is
/// reconstructed from upstream commit
/// `67b0e69f65c196eff5b39dacb7a9ec05bb919c74` instead of relying on MELPA's
/// deleted rolling artifact.
pub const EVIL_EDIFF_MELPA_PIN: (&str, &str) = ("evil-ediff", "20170724.1223");

/// The exact DAP Mode package selected for practical wire framing, protocol
/// messages, live breakpoints, launch templates, variable expansion,
/// launch.json, and output-session parity. MELPA built this archive from
/// upstream commit `c73a587d613788003986a11ffe393b46affe8322`.
pub const DAP_MODE_MELPA_PIN: (&str, &str) = ("dap-mode", "20260616.1526");

/// The exact LSP Treemacs package selected for practical generic-tree,
/// symbols, references, call-hierarchy, diagnostics, and workspace-sync parity.
/// MELPA built this archive from upstream commit
/// `3519ac907ea391e18d9599375b116aeeb6f8a38a`.
pub const LSP_TREEMACS_MELPA_PIN: (&str, &str) = ("lsp-treemacs", "20260515.746");

/// The exact NeoTree package selected for practical tree rendering, keyboard
/// traversal, file operations, root changes, and side-window lifecycle parity.
/// MELPA built this archive from upstream commit
/// `3178805a0942696d1e5162575d9cab43d14b7970`.
pub const NEOTREE_MELPA_PIN: (&str, &str) = ("neotree", "20250703.2202");

/// The exact Evil Lisp State package selected for real Evil-state lifecycle,
/// leader dispatch, Smartparens structural edits, insertion, navigation, and
/// evaluation parity. MELPA built this archive from upstream commit
/// `3c65fecd9917a41eaf6460f22187e2323821f3ce`.
pub const EVIL_LISP_STATE_MELPA_PIN: (&str, &str) = ("evil-lisp-state", "20160404.248");

/// The exact Evil Lion package selected for practical left and right
/// alignment, regex prompting, major-mode rules, whitespace policy, range,
/// failure, and global key-binding parity. MELPA built this archive from
/// upstream commit `5a0bca151466960e090d1803c4c5ded88875f90a`.
pub const EVIL_LION_MELPA_PIN: (&str, &str) = ("evil-lion", "20241120.1351");

/// The exact Evil-Tutor package selected for persisted tutorial sessions,
/// resume behavior, lesson navigation, and major-mode parity. MELPA built this
/// archive from upstream commit `909273bac88b98a565f1b89bbb13d523b7edce2b`.
pub const EVIL_TUTOR_MELPA_PIN: (&str, &str) = ("evil-tutor", "20150103.653");

/// The exact Eyebrowse package selected for real window-layout switching,
/// workspace navigation, slot mutation, mode-line, and input parity. MELPA
/// built this archive from upstream commit
/// `473381f4f9e847eb50a40ef2306c027432789754`.
pub const EYEBROWSE_MELPA_PIN: (&str, &str) = ("eyebrowse", "20240407.1342");

/// The exact Evil Anzu package selected for practical repeated-search,
/// regexp, no-highlight cleanup, disabled-mode, failure, and unload/reload
/// integration parity. MELPA built this archive from upstream commit
/// `7309650425797420944075c9c1556c7c1ff960b3`.
pub const EVIL_ANZU_MELPA_PIN: (&str, &str) = ("evil-anzu", "20250316.1617");

/// The exact Evil Args package selected for practical nested and multiline
/// argument motion, text-object deletion, custom delimiter, and enclosing
/// context navigation parity. MELPA built this archive from upstream commit
/// `a8151556f63c9d45d0c44c8a7ef9e5a542f3cdc7`.
pub const EVIL_ARGS_MELPA_PIN: (&str, &str) = ("evil-args", "20240210.504");

/// The exact Evil Escape package selected for practical timed insert, literal
/// fallback, unordered/case-insensitive sequence, state exit, exclusion,
/// inhibition, buffer-modification, and global-hook lifecycle parity. MELPA
/// built this archive from upstream commit
/// `aebd1a78a6bd33e5164e7552096b3fe1172d3012`.
pub const EVIL_ESCAPE_MELPA_PIN: (&str, &str) = ("evil-escape", "20241212.1318");

/// The exact Evil Exchange package selected for practical word, adjacent,
/// edited-marker, line, cross-buffer, block, cancellation, and binding parity.
/// MELPA built this archive from upstream commit
/// `3030e21ee16a42dfce7f7cf86147b778b3f5d8c1`.
pub const EVIL_EXCHANGE_MELPA_PIN: (&str, &str) = ("evil-exchange", "20200118.252");

/// The exact Evil Goggles package selected for practical yank, paste, delete,
/// change, shift, join, marker, visual-gating, face-preset, and hint-lifecycle
/// parity. MELPA built this archive from upstream commit
/// `34ca276a85f615d2b45e714c9f8b5875bcb676f3`.
pub const EVIL_GOGGLES_MELPA_PIN: (&str, &str) = ("evil-goggles", "20231021.738");

/// The exact Evil Iedit State package selected for practical multi-occurrence
/// edits, state transitions, selective operations, replacement, restriction,
/// numbering, undo, and cleanup parity. MELPA built this archive from upstream
/// commit `44c64c71692e5b2f608ad3e3c537ec0a0e0ea0f8`.
pub const EVIL_IEDIT_STATE_MELPA_PIN: (&str, &str) = ("evil-iedit-state", "20220219.1432");

/// The exact Evil Indent Plus package selected for practical indentation text
/// object ranges, nested-block deletion, parent-context change, shifting,
/// yanking, whitespace, and narrowing parity. MELPA built this archive from
/// upstream commit `f392696e4813f1d3a92c7eeed333248914ba6dae`.
pub const EVIL_INDENT_PLUS_MELPA_PIN: (&str, &str) = ("evil-indent-plus", "20230927.1513");

/// The exact evil-textobj-line package selected for practical inner/outer line
/// text objects, range calculation, and Evil map bindings. MELPA built this
/// archive from upstream commit `9eaf9a5485c2b5c05e16552b34632ca520cd681d`.
pub const EVIL_TEXTOBJ_LINE_MELPA_PIN: (&str, &str) = ("evil-textobj-line", "20211101.1429");

/// The exact Restart Emacs package selected for practical restart transaction,
/// launch strategy, command quoting, startup-directory, desktop handoff,
/// terminal notification, and command-line restoration parity. MELPA built
/// this archive from upstream commit
/// `d0fca7fba014b2d0d4dedcb9744a1e73cd9a6409`.
pub const RESTART_EMACS_MELPA_PIN: (&str, &str) = ("restart-emacs", "20201127.1425");

/// The exact WS Butler package selected for practical modified-line,
/// save-hook, virtual-space, predicate, EOF, indentation, undo, narrowing,
/// and mode-lifecycle parity. MELPA built this archive from upstream commit
/// `9ee5a7657a22e836618813c2e2b64a548d27d2ff`.
pub const WS_BUTLER_MELPA_PIN: (&str, &str) = ("ws-butler", "20250310.205");

/// The exact Golden Ratio package selected for practical layout resizing,
/// exclusions, scaling, navigation advice, recentering, timer scheduling,
/// and global-mode lifecycle parity. MELPA built this archive from upstream
/// commit `375c9f287dfad68829582c1e0a67d0c18119dab9`.
pub const GOLDEN_RATIO_MELPA_PIN: (&str, &str) = ("golden-ratio", "20230912.1825");

/// The exact Git.el package selected for practical repository lifecycle,
/// staging, history, checkout, reset, removal, stash, remote, bare-repository,
/// and error parity. MELPA built this archive from upstream commit
/// `8b7f1477ef367b5b7de452589dd9a8ab30150d0a`.
pub const GIT_MELPA_PIN: (&str, &str) = ("git", "20140128.1041");

/// The exact Eval Sexp Fu package selected for practical inner-expression
/// evaluation, source navigation, overlay lifecycle, delimiter flashing,
/// timer ordering, advice, error feedback, and mode-gating parity. MELPA built
/// this archive from upstream commit
/// `36d2fe3bcf602e15ca10a7f487da103515ef391a`.
pub const EVAL_SEXP_FU_MELPA_PIN: (&str, &str) = ("eval-sexp-fu", "20191128.825");

/// The exact final Packed package selected for practical library discovery,
/// main-library inference, feature/dependency parsing, load-path management,
/// source lookup, byte-compilation, and autoload parity. MELPA built this
/// historical archive from upstream commit
/// `169064f7acfe198cc7dd43d02518b773691e1314` before retiring the package.
pub const PACKED_MELPA_PIN: (&str, &str) = ("packed", "20221130.2228");

/// The exact Define Word package selected for practical service dispatch,
/// word/region/PDF selection, URL retrieval, inflection expansion, HTML
/// parsing and styling, result limits, and offline dictionary parity. MELPA
/// built this archive from upstream commit
/// `31a8c67405afa99d0e25e7c86a4ee7ef84a808fe`.
pub const DEFINE_WORD_MELPA_PIN: (&str, &str) = ("define-word", "20220104.1848");

/// The exact Evil Visualstar package selected for practical literal,
/// forward/backward, multiline, persistent-selection, search-history,
/// missing-match, and mode-binding parity. MELPA built this archive from
/// upstream commit `06c053d8f7381f91c53311b1234872ca96ced752`.
pub const EVIL_VISUALSTAR_MELPA_PIN: (&str, &str) = ("evil-visualstar", "20160223.48");

/// The exact Open Junk File package selected for practical dated-file
/// creation, prompt and opener customization, real file visits, junk hooks,
/// canonical paths, aliases, failure recovery, and bug-report metadata parity.
/// MELPA built this archive from upstream commit
/// `558bec7372b0fed4c4cb6074ab906535fae615bd`.
pub const OPEN_JUNK_FILE_MELPA_PIN: (&str, &str) = ("open-junk-file", "20161210.1114");

/// The exact Org Bullets package selected for practical outline rendering,
/// custom bullet and face policy, subtree editing, narrowing, inline-task,
/// keymap, and mode-lifecycle parity. MELPA built this archive from upstream
/// commit `9ec0dbd30be7c6310804141ee952ac8c5f753557`.
pub const ORG_BULLETS_MELPA_PIN: (&str, &str) = ("org-bullets", "20200317.1740");

/// The exact Org Brain package selected for practical knowledge-base entry,
/// relationship, visualization, resource, navigation, and recovery parity.
/// MELPA built version 0.94 from upstream commit
/// `2bad7732aae1a3051e2a14de2e30f970bbe43c25`.
pub const ORG_BRAIN_MELPA_PIN: (&str, &str) = ("org-brain", "20230217.1908");

/// The exact Org Cliplink package selected for practical asynchronous link
/// insertion, synchronous capture, HTML title normalization, authenticated
/// retrieval, gzip, curl, customization, and failure parity. MELPA built this
/// archive from upstream commit
/// `13e0940b65d22bec34e2de4bc8cba1412a7abfbc`.
pub const ORG_CLIPLINK_MELPA_PIN: (&str, &str) = ("org-cliplink", "20201126.1020");

/// The exact Org Download package selected for practical heading-based image
/// storage, local copy and link insertion, Org attachment, screenshot,
/// content detection, rename/delete, and drag-and-drop parity. MELPA built
/// this archive from upstream commit
/// `c8be2611786d1d8d666b7b4f73582de1093f25ac`.
pub const ORG_DOWNLOAD_MELPA_PIN: (&str, &str) = ("org-download", "20241118.1846");

/// The exact Org MIME package selected for practical Org-to-HTML export,
/// multipart mail, quoted replies, inline images, message conversion and
/// reversion, mail properties, hooks, and dedicated Org editing parity.
/// MELPA built version 0.3.4 from upstream commit
/// `ffaad784a8597ee52842a578c01bd347d3e0281d`.
pub const ORG_MIME_MELPA_PIN: (&str, &str) = ("org-mime", "20251201.245");

/// The exact Org Pomodoro package selected for practical task clocking,
/// timer transitions, overtime, break, cancellation, notification, audio,
/// expiry, and mode-line parity. MELPA built this archive from upstream
/// commit `3f5bcfb80d61556d35fc29e5ddb09750df962cc6`.
pub const ORG_POMODORO_MELPA_PIN: (&str, &str) = ("org-pomodoro", "20220318.1618");

/// The exact Org Present package selected for title/slide narrowing,
/// navigation, overlays, scale, one-page, cursor, read-only, hooks, folded
/// startup, and quit-lifecycle parity. MELPA built this archive from upstream
/// commit `4ec04e1b77dea76d7c30066ccf3200d2e0b7bee9`.
pub const ORG_PRESENT_MELPA_PIN: (&str, &str) = ("org-present", "20220806.1847");

/// The exact Org Ref package selected for practical Org/BibTeX activation,
/// documented insertion, citation and reference navigation, browser dispatch,
/// citeproc export, analysis, and failure-recovery parity. MELPA built this
/// archive from upstream commit `dc2481d430906fe2552f9318f4405242e6d37396`.
pub const ORG_REF_MELPA_PIN: (&str, &str) = ("org-ref", "20251206.1422");

/// The exact Org-roam package selected for practical plain-text knowledge-base
/// synchronization, node metadata and lookup, links and backlinks, node
/// editing, dedicated buffers, and autosync lifecycle parity. MELPA built
/// version 2.3.1 from upstream commit

/// The exact orderless package selected for practical multi-component
/// completion styles, filtering, try-completion, match highlighting, and
/// affix dispatch. MELPA built this archive from upstream commit
/// `cebe19e3cf0f30604d1ed1bfaa74fff21a4e89a5`.
pub const ORDERLESS_MELPA_PIN: (&str, &str) = ("orderless", "20260519.1029");

/// `c54c523dec175695645399705606ea19056a3053`.
pub const ORG_ROAM_MELPA_PIN: (&str, &str) = ("org-roam", "20260425.1623");

/// The exact Org Superstar package selected for practical headline, TODO,
/// plain-list, ordered-list, source-block, hook, accessor, restart, and
/// mode-lifecycle parity. MELPA built version 1.7.0 from upstream commit
/// `ce6f7f421f995893f72d75ffdfa92964b9bea2e3`.
pub const ORG_SUPERSTAR_MELPA_PIN: (&str, &str) = ("org-superstar", "20250914.1308");

/// The exact Orgit package selected for practical Org links to Magit status,
/// log, revision, and blob buffers, repository-id lookup, web export, and
/// broken-link parity. MELPA built version 2.2.0 from upstream commit
/// `47b3568fce775c756fb5bb3545c2edd48b8e2fc1`.
pub const ORGIT_MELPA_PIN: (&str, &str) = ("orgit", "20260717.1740");

/// The exact Marginalia package selected for practical completion-annotation
/// parity: symbol/function/variable/command class and documentation fields,
/// character and environment-variable annotators, field truncation and
/// formatting, censored variable values, prompt/command classifiers, and the
/// global minor-mode advice lifecycle. MELPA built version 2.11 from upstream
/// commit `10b170ad8006bad535599e5b3e007e643e34345a`.
pub const MARGINALIA_MELPA_PIN: (&str, &str) = ("marginalia", "20260724.810");

/// The exact Overseer package selected for practical project discovery,
/// automatic test-file activation, ERT selection, command dispatch, and real
/// compilation-process parity. MELPA built this archive from upstream commit
/// `7fdcf1a6fba6b1569a09c1666b4e51bcde266ed9`.
pub const OVERSEER_MELPA_PIN: (&str, &str) = ("overseer", "20240109.800");

/// The exact EmacSQL package selected for practical schema and CRUD query
/// compilation, prepared-input escaping, reporting expressions, transaction
/// retry and rollback, protocol parsing, authoring, and failure parity. MELPA
/// built this archive from upstream commit
/// `d811bbefcb5e27841af55cae53aa939ba720de77`.
pub const EMACSQL_MELPA_PIN: (&str, &str) = ("emacsql", "20260601.1722");

/// The final standalone EmacSQL SQLite package selected for practical
/// migration-warning and feature lifecycle parity.  MELPA built version
/// 20240825.1837 from the exact `legacy-stubs` revision
/// `b9f19ac5e17a90d5b7314d67e3b790992be7d82d`; this package intentionally
/// provides no database backend and directs users to uninstall it because the
/// backend moved into `emacsql` itself.
pub const EMACSQL_SQLITE_MELPA_PIN: (&str, &str) = ("emacsql-sqlite", "20240825.1837");

/// The exact Ghub package selected for practical REST and GraphQL request,
/// JSON response, pagination, authentication, forge identity, retry, and
/// failure parity. MELPA built this archive from upstream commit
/// `59d0b9b33e780d6cff5131886904ff26033dd2e6`.
pub const GHUB_MELPA_PIN: (&str, &str) = ("ghub", "20260701.1318");

/// The exact Forge package selected for practical repository detection,
/// persisted issue and pull-request, topic rendering, template, and Git-ref
/// parity. MELPA built this archive from upstream commit
/// `29f45d8f247079a1d8d2247efdacb5b50a3b1e51`.
pub const FORGE_MELPA_PIN: (&str, &str) = ("forge", "20260731.2255");

/// The exact Treepy package selected as Ghub's GraphQL response traversal
/// dependency. MELPA built this archive from upstream commit
/// `806c000bd40153d17dfa5709c6d19546d507a416`.
pub const TREEPY_MELPA_PIN: (&str, &str) = ("treepy", "20260531.1144");

/// The exact Lua Mode package selected for practical Lua authoring,
/// navigation, evaluation, documentation, and tooling parity. MELPA built
/// this archive from upstream commit
/// `2f6b8d7a6317e42c953c5119b0119ddb337e0a5f`.
pub const LUA_MODE_MELPA_PIN: (&str, &str) = ("lua-mode", "20250310.1150");

/// The exact Clean Aindent Mode package selected for practical smart and
/// simple newline indentation, abandoned-whitespace cleanup, undo, nested and
/// tabbed unindent, kill fallback, buffer isolation, and mode lifecycle parity.
/// MELPA built this archive from upstream commit
/// `a97bcae8f43a9ff64e95473e4ef0d8bafe829211`.
pub const CLEAN_AINDENT_MODE_MELPA_PIN: (&str, &str) = ("clean-aindent-mode", "20171017.2043");

/// The exact Clang-Format package selected for practical whole-buffer, region,
/// UTF-8 cursor, configuration, save-hook, version-control diff, and failure
/// atomicity parity. MELPA built this archive from upstream commit
/// `a099177b5cd5060597d454e4c1ffdc96b92ba985`.
pub const CLANG_FORMAT_MELPA_PIN: (&str, &str) = ("clang-format", "20250223.1620");

/// The exact Vi Tilde Fringe package selected for practical local and global
/// mode lifecycle, bitmap customization and registration, empty-line mapping,
/// per-buffer isolation, minibuffer exclusion, and failure-state parity.
/// MELPA built this archive from upstream commit
/// `e6e15638e8c45a5e68d0874d5d8c9a46c4f38a54`.
pub const VI_TILDE_FRINGE_MELPA_PIN: (&str, &str) = ("vi-tilde-fringe", "20141028.242");

/// The exact Evil Matchit package selected for practical nested-code, markup,
/// indentation, preprocessor, Org block, diff, conflict-resolution, region,
/// deletion, and jump-hook parity. MELPA built this archive from upstream
/// commit `751e74ce2e29c3f32b30e6a1012a33fe81ba0700`.
pub const EVIL_MATCHIT_MELPA_PIN: (&str, &str) = ("evil-matchit", "20260409.936");

/// The exact Evil MC package selected for practical all-match refactoring,
/// incremental cursor selection, visual-line insertion, pause/resume, and
/// grouped undo parity. MELPA built this archive from upstream commit
/// `7e363dd6b0a39751e13eb76f2e9b7b13c7054a43`.
pub const EVIL_MC_MELPA_PIN: (&str, &str) = ("evil-mc", "20241025.2045");

/// The exact Evil Nerd Commenter package selected for practical line, region,
/// paragraph, copy-and-comment, kill-ring, HTML, Org source-block, and comment
/// navigation parity. MELPA built this archive from upstream commit
/// `db5ee61a6e75db074b7d20e9dcb68e0b94b4edc7`.
pub const EVIL_NERD_COMMENTER_MELPA_PIN: (&str, &str) = ("evil-nerd-commenter", "20260507.414");

/// The exact Evil Numbers package selected for practical mixed-radix,
/// grouped-literal, Unicode-script, linewise, blockwise, and search-policy
/// parity. MELPA built this archive from upstream commit
/// `616aff9e5cee012954756ed2715209fa90308cdf`.
pub const EVIL_NUMBERS_MELPA_PIN: (&str, &str) = ("evil-numbers", "20260103.850");

/// The exact Evil Search Highlight Persist package selected for literal and
/// regexp searches, minimum-length policy, multi-window behavior, clear/mode
/// lifecycle, global activation, and advised isearch-exit parity. MELPA built
/// this archive from upstream commit `6e04a8c075f5fd62526d222447048faab8bfa187`.
pub const EVIL_SEARCH_HIGHLIGHT_PERSIST_MELPA_PIN: (&str, &str) =
    ("evil-search-highlight-persist", "20170523.334");

/// The exact Evil Surround package selected for practical characterwise,
/// linewise, blockwise, nested-delimiter, markup, custom-pair, repeat, marker,
/// text-property, and undo parity. MELPA built this archive from upstream
/// commit `14dc693ed971053feb9596d4bc1b1de0b0006584`.
pub const EVIL_SURROUND_MELPA_PIN: (&str, &str) = ("evil-surround", "20240325.852");

/// The exact Exec Path From Shell package selected for practical exported
/// environment import, executable-path synchronization, interactive copying,
/// non-POSIX shell adaptation, and failure-diagnostic parity. MELPA built this
/// archive from upstream commit `dae820da35ad46234cbca31626ffb6da7928694a`.
pub const EXEC_PATH_FROM_SHELL_MELPA_PIN: (&str, &str) = ("exec-path-from-shell", "20260423.1833");

/// The exact elisp-def package selected for real macro-aware Emacs Lisp
/// definition navigation, xref, highlighting, completion, and failure parity.
/// MELPA built this archive from upstream commit
/// `61a5f64498c9c8de8e9aab84a22775162f336144`.
pub const ELISP_DEF_MELPA_PIN: (&str, &str) = ("elisp-def", "20250818.2223");

/// The exact Elisp Slime Nav package selected for practical key-driven
/// function, variable, library, and face navigation; xref return history;
/// prompted symbol selection; help display; and stale-symbol recovery parity.
/// MELPA built this archive from upstream commit
/// `8588d80d414aee1fafce5b9da0e913612ee0bcdd`.
pub const ELISP_SLIME_NAV_MELPA_PIN: (&str, &str) = ("elisp-slime-nav", "20210510.528");

/// The exact elisp-refs package selected for practical reference search helpers
/// (format/pluralize/unindent) used by helpful. MELPA built this archive from
/// upstream commit `541a064c3ce27867872cf708354a65d83baf2a6d`.
pub const ELISP_REFS_MELPA_PIN: (&str, &str) = ("elisp-refs", "20230920.201");

/// The exact Dumb Jump package selected for practical ripgrep-backed xref
/// definition and reference navigation, project configuration, contextual
/// disambiguation, jump history, and missing-definition parity. MELPA built
/// this archive from upstream commit
/// `cf06b4ccdce6a39346c32f05139f9ee8b77ee229`.
pub const DUMB_JUMP_MELPA_PIN: (&str, &str) = ("dumb-jump", "20260603.1700");

/// The exact Flx package selected for practical file-finder, command-palette,
/// incremental narrowing, annotated display, and Unicode case-intent parity.
/// MELPA built this archive from upstream commit
/// `4b1346eb9a8a76ee9c9dede69738c63ad97ac5b6`.
pub const FLX_MELPA_PIN: (&str, &str) = ("flx", "20240205.356");

/// The exact Flx-Ido package selected for practical advised matching,
/// incremental narrowing, merged-choice metadata, threshold fallback,
/// highlighting, per-session cache identity, and reset lifecycle parity.
/// MELPA built this archive from upstream commit
/// `4b1346eb9a8a76ee9c9dede69738c63ad97ac5b6`.
pub const FLX_IDO_MELPA_PIN: (&str, &str) = ("flx-ido", "20240205.356");

/// The exact Volatile Highlights package selected for practical clipboard,
/// replacement, transposition, undo, deletion, custom-extension, and xref
/// feedback parity. MELPA built this archive from upstream commit
/// `f68ac37451c1226d6f13c1b299ec7516f74888a1`.
pub const VOLATILE_HIGHLIGHTS_MELPA_PIN: (&str, &str) = ("volatile-highlights", "20260315.1109");

/// The exact VTerm package selected for practical mode/process setup,
/// terminal filtering, input translation, prompt copying, wrapped-line,
/// directory, message-passing, and buffer-lifecycle parity. Its native module
/// contract is represented by a deterministic recording seam. MELPA built this
/// archive from upstream commit `70921114908ebb260d6686db8cbe2445a64f90a2`.
pub const VTERM_MELPA_PIN: (&str, &str) = ("vterm", "20260730.1414");

/// The exact Haml Mode package selected for practical mode setup,
/// mixed Haml/Ruby/filter fontification, indentation, attribute parsing,
/// structural navigation and editing, tolerant sexp scanning, and compiler
/// integration parity. MELPA built this archive from upstream commit
/// `3bb4a96535eb5c81dbe6a43bfa8d67a778d449c0`.
pub const HAML_MODE_MELPA_PIN: (&str, &str) = ("haml-mode", "20250714.1441");

/// The exact Solarized Theme package selected for practical palette,
/// face-spec, option, variant, and enable/disable lifecycle parity. MELPA
/// built this archive from upstream commit
/// `1443d6dce378ad2d65a8a8c45d5279481a79dfab`.
pub const SOLARIZED_THEME_MELPA_PIN: (&str, &str) = ("solarized-theme", "20260728.833");

/// The exact Smex package selected for practical command discovery,
/// execution, ranking, history, persistence, major-mode filtering, and
/// maintenance parity. MELPA built this archive from upstream commit
/// `55aaebe3d793c2c990b39a302eb26c184281c42c`.
pub const SMEX_MELPA_PIN: (&str, &str) = ("smex", "20151212.2209");

/// The exact Smooth Scrolling package selected for practical global-mode,
/// cursor-motion, viewport-margin, strict/logical line, small-window, and
/// conditional-advice lifecycle parity. MELPA built this archive from upstream
/// commit `2462c13640aa4c75ab3ddad443fedc29acf68f84`.
pub const SMOOTH_SCROLLING_MELPA_PIN: (&str, &str) = ("smooth-scrolling", "20161002.1949");

/// The exact Smeargle package selected for practical real-Git blame,
/// last-update-time and commit-age highlighting, rerender/clear behavior, and
/// non-repository failure parity. MELPA built version 0.03 from upstream
/// commit `1c5c1e1d66aa96b818fbfcdf9fbec84e509b87be`.
pub const SMEARGLE_MELPA_PIN: (&str, &str) = ("smeargle", "20200323.533");

/// The exact Bind-Key release selected from GNU ELPA by the comprehensive API
/// parity corpus.
pub const BIND_KEY_GNU_ELPA_PIN: (&str, &str) = ("bind-key", "2.4.1");

/// The exact BUI package selected by the practical service dashboard,
/// marking, filtering, detail action, and history parity corpus, and as
/// aurel's runtime buffer-interface dependency.
pub const BUI_MELPA_PIN: (&str, &str) = ("bui", "20260502.730");

/// The exact Cargo package selected for practical public command construction,
/// compilation diagnostics, Rust error help, project creation, metadata/Xref,
/// and command-recovery parity. MELPA built this archive from upstream commit
/// `7f8466063381eed05d4e222ce822b1dd44e3bf17`.
pub const CARGO_MELPA_PIN: (&str, &str) = ("cargo", "20231229.915");

/// The exact Browse At Remote package selected for real Git repository,
/// public browser/clipboard, remote-routing, and Gitea URL parity. MELPA
/// built this archive from upstream commit
/// `38e5ffd77493c17c821fd88f938dbf42705a5158`.
pub const BROWSE_AT_REMOTE_MELPA_PIN: (&str, &str) = ("browse-at-remote", "20260126.608");

/// The exact browse-kill-ring package selected for practical kill-ring browser
/// display, elision, insertion helpers, and default keybinding setup. MELPA
/// built this archive from upstream commit
/// `39d65a830b93530c9bd68a7dc14353cbffd1d01f`.
pub const BROWSE_KILL_RING_MELPA_PIN: (&str, &str) = ("browse-kill-ring", "20260503.1620");

/// The exact Buffer Move package selected for practical multi-window swap,
/// move-history, transient-arrow, failure-ordering, and recovery parity.
/// MELPA built this archive from upstream commit
/// `e7800b3ab1bd76ee475ef35507ec51ecd5a3f065`.
pub const BUFFER_MOVE_MELPA_PIN: (&str, &str) = ("buffer-move", "20220512.755");

/// The exact Buttercup package selected for practical suite lifecycle,
/// expectations, failure recovery, spies, and test-discovery parity. MELPA
/// built this archive from upstream commit
/// `39c8e762408a166a5afa03b8e79dd8d1a0de5caa`.
pub const BUTTERCUP_MELPA_PIN: (&str, &str) = ("buttercup", "20260512.2141");

/// The exact Casual package selected by the practical EditKit, Elisp, CSV,
/// Dired, and Ibuffer menu-command parity corpus.
pub const CASUAL_MELPA_PIN: (&str, &str) = ("casual", "20260718.1803");

/// The exact CCC package selected by the practical buffer-local cursor,
/// frame-color baseline, terminal fallback, and setup lifecycle parity corpus.
pub const CCC_MELPA_PIN: (&str, &str) = ("ccc", "20260322.1316");

/// The exact CDB package selected by the practical indexed lookup, collision,
/// binary payload, enumeration, and cached-reader lifecycle parity corpus.
pub const CDB_MELPA_PIN: (&str, &str) = ("cdb", "20230318.2152");

/// The exact Ccls client package selected for practical LSP extension,
/// preprocessing, semantic-highlight, code-lens, and hierarchy parity. MELPA
/// built this archive from upstream commit
/// `f728c92e33844f1da54cb47ecb4e44160f2042a8`.
pub const CCLS_MELPA_PIN: (&str, &str) = ("ccls", "20260507.1746");

/// The exact Centered Cursor Mode package selected for practical local and
/// global mode lifecycle, real-window recentering, ignored-command, EOF,
/// viewport adjustment, paging, and multi-window parity. MELPA built this
/// version from upstream commit `67ef719e685407dbc455c7430765e4e685fd95a9`.
pub const CENTERED_CURSOR_MODE_MELPA_PIN: (&str, &str) = ("centered-cursor-mode", "20230914.1358");

/// The exact Chinese Word at Point package selected by the practical external
/// segmentation, mixed-language extraction, and bounds-driven editing corpus.
pub const CHINESE_WORD_AT_POINT_MELPA_PIN: (&str, &str) = ("chinese-word-at-point", "20170811.941");

/// The exact CIDER package selected by the practical nREPL transport,
/// completion-context, compilation, stacktrace, test-report, inspector, REPL,
/// and source-navigation parity corpus. MELPA built this archive from upstream
/// commit `567503cec96bf463e031eef6e0d258ba87b17188`.
pub const CIDER_MELPA_PIN: (&str, &str) = ("cider", "20260729.1056");

/// The exact clj-refactor package selected for practical mode/keybinding,
/// local structural edit, namespace cleanup, project dependency sorting, and
/// unavailable-middleware recovery parity. MELPA built this archive from
/// upstream commit `2805bd5f505fdb199a8c5a25fca398ec9c161e5b`.
pub const CLJ_REFACTOR_MELPA_PIN: (&str, &str) = ("clj-refactor", "20260716.1545");

/// The exact El Mock package selected for practical stub, mock, verification,
/// failure, teardown, and mixed `mocklet` parity. MELPA built this archive from
/// upstream commit `6cfbc9de8f1927295dca6864907fe4156bd71910`.
pub const EL_MOCK_MELPA_PIN: (&str, &str) = ("el-mock", "20220625.1949");

/// The exact Ecukes package selected for practical project scaffolding,
/// feature parsing, hook/step execution, spec reporting, tag/pattern
/// selection, and failure/recovery parity. MELPA built this archive from
/// upstream commit `70cb0748b222b7c96ab9821ef898ffbdb45eacd8`.
pub const ECUKES_MELPA_PIN: (&str, &str) = ("ecukes", "20241226.1759");

/// The exact Espuds package selected for practical buffer, cursor, region,
/// action-chain, file, mode, face, message, and assertion-recovery
/// step-definition parity.
/// MELPA built this archive from upstream commit
/// `57c18a48f1a01d8174298eaab4fcf3b2c6549291`.
pub const ESPUDS_MELPA_PIN: (&str, &str) = ("espuds", "20230218.910");

/// The exact esxml package selected for practical esxml/sxml rendering,
/// local document parsing, CSS-selector query, and invalid-form recovery
/// parity. MELPA built this archive from upstream commit
/// `35940903049f05858d2519c9d8316d00bc228953`.
pub const ESXML_MELPA_PIN: (&str, &str) = ("esxml", "20260329.1617");

/// The exact Google Translate package selected by the practical request,
/// response, dictionary, suggestion, speech, language-selection, editing, and
/// backend-dispatch parity corpus. MELPA built this archive from upstream
/// commit `47c5719b7dd51a37a6ad270489738187a436d920`.
pub const GOOGLE_TRANSLATE_MELPA_PIN: (&str, &str) = ("google-translate", "20260419.134");

/// The exact Package Lint package selected by the practical release audit,
/// dependency, compatibility, package-structure, interactive-report, batch,
/// and version-maintenance parity corpus. MELPA built this archive from
/// upstream commit `35996f478d81e51dae4fa30d051f741895d07399`.
pub const PACKAGE_LINT_MELPA_PIN: (&str, &str) = ("package-lint", "20260619.1246");

/// The exact Polymode package selected as EIN's rendered multi-mode notebook
/// dependency. MELPA built this archive from upstream commit
/// `8cb72fa5dcc0d98746c680043dc121edc7621e3a`.
pub const POLYMODE_MELPA_PIN: (&str, &str) = ("polymode", "20260505.1803");

/// The final standalone Csharp Mode package selected for practical automatic
/// file activation, C# font-lock, indentation, brace editing, defun/statement
/// navigation, compilation diagnostics, and malformed-string recovery parity.
/// MELPA built this archive from upstream commit
/// `d8b058c9e9d0429ea7e81d121ce19b064bd7e0f5` before the mode moved into Emacs.
pub const CSHARP_MODE_MELPA_PIN: (&str, &str) = ("csharp-mode", "20221126.2005");

/// The exact Format All package selected for practical built-in and external
/// formatter chains, buffer/region commands, error recovery, and format-on-save
/// parity. MELPA built this archive from upstream commit
/// `0dbe9c70eaf8b92dca1a42552761eaa13c3139cf`.
pub const FORMAT_ALL_MELPA_PIN: (&str, &str) = ("format-all", "20260620.1824");

/// The exact CSV Mode release selected from GNU ELPA by the practical quoted
/// row, column editing, sorting, alignment, and transpose parity corpus.
pub const CSV_MODE_GNU_ELPA_PIN: (&str, &str) = ("csv-mode", "1.27");

/// The exact Datetime Format package selected by the practical protocol date,
/// timezone, DST transition, scheduler normalization, and validation corpus.
pub const DATETIME_FORMAT_MELPA_PIN: (&str, &str) = ("datetime-format", "20240105.1901");

/// The exact DDSKK package selected by the practical Japanese input,
/// dictionary conversion, learned-candidate, numeric, and punctuation corpus.
pub const DDSKK_MELPA_PIN: (&str, &str) = ("ddskk", "20260329.1317");

/// The exact Avy package selected by the practical keyboard-driven jump,
/// cross-window, dispatch action, line editing, and cancellation corpus.
pub const AVY_MELPA_PIN: (&str, &str) = ("avy", "20241101.1357");

/// The exact Link Hint package selected by the practical visible-link,
/// link-priority, custom-type, fallback, filesystem, and cross-window parity
/// corpus. MELPA built this archive from upstream commit
/// `8fda5dcb9caff5a3c49d22b82e570ac9e29af7dd`.
pub const LINK_HINT_MELPA_PIN: (&str, &str) = ("link-hint", "20250911.57");

/// The exact List Utils package selected by the practical append pipeline,
/// proper/improper data, cyclic graph, flattening, ordered collection, set,
/// and plist parity corpus. MELPA built this archive from upstream commit
/// `bbea0e7cc7ab7d96e7f062014bde438aa8ffcd43`.
pub const LIST_UTILS_MELPA_PIN: (&str, &str) = ("list-utils", "20241106.1849");

/// The exact Live Py Mode package selected for real bundled Space Tracer
/// execution, live edits, scroll/narrow alignment, driver/path/args, directory
/// module calculation, lifecycle cleanup, and missing-file errors. MELPA built
/// this archive from upstream commit `7655ee7a7294cd486fd02603d76061c1c773e058`.
pub const LIVE_PY_MODE_MELPA_PIN: (&str, &str) = ("live-py-mode", "20260227.509");

/// The exact Livid Mode package selected for practical automatic JavaScript
/// evaluation, trimming, validation, pause/resume, editing, and buffer-local
/// lifecycle parity. MELPA built this archive from upstream commit
/// `dfe5212fa64738bc4138bfebf349fbc8bc237c26`.
pub const LIVID_MODE_MELPA_PIN: (&str, &str) = ("livid-mode", "20131116.1344");

/// The exact Lorem-Ipsum package selected for deterministic prose, list,
/// buffer-local formatting, SGML integration, and keybinding parity.
/// MELPA built this archive from upstream commit
/// `4e87a899868e908a7a9e1812831d76c8d072f885`.
pub const LOREM_IPSUM_MELPA_PIN: (&str, &str) = ("lorem-ipsum", "20221214.1857");

/// The exact Avy Menu package selected by the practical rendered menu,
/// multi-level selection, inactive item, and cancellation lifecycle corpus.
pub const AVY_MENU_MELPA_PIN: (&str, &str) = ("avy-menu", "20230606.1519");

/// The exact Bash Completion package selected for practical setup, command-line
/// tokenization, nocomint/capf completion, file-name escaping, timeout, debug,
/// and reset parity. MELPA built this archive from upstream commit
/// `5b621db96efc549c64436011a81fd658c6dcf6a0`.
pub const BASH_COMPLETION_MELPA_PIN: (&str, &str) = ("bash-completion", "20260206.1459");

/// The exact Beacon package selected for practical command-loop movement,
/// scrolling, overlay, timer, suppression, mark, and display parity. MELPA
/// built this archive from upstream commit
/// `85261a928ae0ec3b41e639f05291ffd6bf7c231c`.
pub const BEACON_MELPA_PIN: (&str, &str) = ("beacon", "20220730.100");

/// The exact Blacken package selected for practical Python buffer formatting,
/// formatter-option, project save-hook, process-failure, and long-line parity.
/// MELPA built this archive from upstream commit
/// `a43695f9cb412df93ac8d38b55ab1515e86e217e`.
pub const BLACKEN_MELPA_PIN: (&str, &str) = ("blacken", "20231129.654");

/// The exact BERT package selected by the practical external-term fixture,
/// RPC, signed metric, UTF-8 binary, and bulk tuple parity corpus.
pub const BERT_MELPA_PIN: (&str, &str) = ("bert", "20131117.1014");

/// The exact Compat release selected from GNU ELPA by the comprehensive API
/// parity corpus.
pub const COMPAT_GNU_ELPA_PIN: (&str, &str) = ("compat", "31.0.0.2");

/// The exact Corfu package selected for practical in-buffer completion,
/// candidate navigation, preview, insertion, cancellation, and extension-mode
/// parity. MELPA built this archive from upstream commit
/// `4b440fb30ff2fff4291af676b59da7ab22130a45`.
pub const CORFU_MELPA_PIN: (&str, &str) = ("corfu", "20260802.2028");

/// The exact Clojure Mode package selected by the practical project namespace,
/// formatting, structural refactoring, and source-navigation parity corpus.
pub const CLOJURE_MODE_MELPA_PIN: (&str, &str) = ("clojure-mode", "20260709.952");

/// The exact Closql package selected for practical SQLite-backed object,
/// relation, query, transaction, and schema-lifecycle parity. MELPA built
/// version 2.4.1 from upstream commit
/// `d382e7427f5d375ffc872851b049e9f9c4a43dfc`.
pub const CLOSQL_MELPA_PIN: (&str, &str) = ("closql", "20260601.1540");

/// The exact SQLite3 module package used as the Closql parity backend because
/// the local GNU Emacs and Neomacs builds do not provide builtin SQLite.
pub const SQLITE3_MELPA_PIN: (&str, &str) = ("sqlite3", "20251014.536");

/// The exact Company package selected by the practical interactive
/// completion, CAPF, asynchronous backend, and file workflow parity corpus.
pub const COMPANY_MELPA_PIN: (&str, &str) = ("company", "20260721.100");

/// The exact Company C Headers package selected for real C/C++/Objective-C
/// header completion, nested include, location-preview, and delimiter parity.
/// MELPA built this archive from upstream commit
/// `986cef8c7aae821ae65b193ea15e7f4a7097821c`.
pub const COMPANY_C_HEADERS_MELPA_PIN: (&str, &str) = ("company-c-headers", "20260511.1117");

/// The exact company-quickhelp package selected for practical frontend enable,
/// docstring truncation, timer arm/cancel, and doc-buffer extraction. MELPA
/// built this archive from upstream commit
/// `5bda859577582cc42d16fc0eaf5f7c8bedfd9e69`.
pub const COMPANY_QUICKHELP_MELPA_PIN: (&str, &str) = ("company-quickhelp", "20231026.1714");

/// The exact Company Statistics package selected for practical real-Company
/// learning, contextual ranking, bounded decay, persistence, and failure
/// recovery parity. MELPA built this archive from upstream commit
/// `120e982f47e01945c044e0762ba376741c41b76c`.
pub const COMPANY_STATISTICS_MELPA_PIN: (&str, &str) = ("company-statistics", "20250805.1524");

/// The exact Company Web package selected for practical HTML, Pug, and Slim
/// completion, candidate metadata, documentation, CSS delegation, and invalid
/// source parity. MELPA built this archive from upstream commit
/// `e0c6bfa3ae7006c73d0fdfc0fdb69816309baf1b`.
pub const COMPANY_WEB_MELPA_PIN: (&str, &str) = ("company-web", "20220115.2146");

/// The exact Company Anaconda package selected for Python prefix eligibility,
/// asynchronous candidate metadata, real Company selection/insertion,
/// annotation customization, docs, locations, empty results, and policy parity.
/// MELPA built this archive from upstream commit
/// `14867265e474f7a919120bbac74870c3256cbacf`.
pub const COMPANY_ANACONDA_MELPA_PIN: (&str, &str) = ("company-anaconda", "20230821.2126");

/// The exact Commander package selected for practical CLI parsing,
/// configuration, usage generation, defaults, and error parity.
/// This MELPA version corresponds to upstream tag `v0.7.0`, commit
/// `2c8a57b9c619e29ccbe2d5a85921b9c689e95bf9`.
pub const COMMANDER_MELPA_PIN: (&str, &str) = ("commander", "20140120.1852");

/// The exact cmake-mode package selected for practical CMake major-mode
/// activation, indentation, fontification, function navigation, command
/// lowercasing, and help-command orchestration. MELPA built this archive from
/// CMake commit `c162d6852d09a82cf87ff0cda6a23abc775dfdb6`.
pub const CMAKE_MODE_MELPA_PIN: (&str, &str) = ("cmake-mode", "20260731.1301");

/// The exact coffee-mode package selected for practical CoffeeScript
/// major-mode activation, indentation, fontification, comment/fat-arrow
/// editing, and compile-command orchestration. MELPA built this archive from
/// upstream commit `35a41c7d8233eac0b267d9593e67fb8b6235e134`.
pub const COFFEE_MODE_MELPA_PIN: (&str, &str) = ("coffee-mode", "20200315.1133");

/// The exact Cond-Let package selected by the practical conditional binding,
/// validation pipeline, authorization, and queue workflow parity corpus.
pub const COND_LET_MELPA_PIN: (&str, &str) = ("cond-let", "20260701.1237");

/// The exact Consult package selected by the practical line, symbol, and
/// buffer-navigation workflow parity corpus.
pub const CONSULT_MELPA_PIN: (&str, &str) = ("consult", "20260716.1105");

/// The exact Embark Consult package selected for practical location and grep
/// export, navigation, and buffer table-of-contents parity. MELPA built this
/// archive from upstream commit `ec5dd1475595277ef908567d0a18d32f1c40bc91`.
pub const EMBARK_CONSULT_MELPA_PIN: (&str, &str) = ("embark-consult", "20260503.118");

/// The exact Counsel package selected for practical command-palette, file and
/// Git navigation, kill-ring recovery, structural navigation, and project
/// compilation parity. MELPA built this archive from upstream commit
/// `ee79f68215ae7e2b8a38ba6bf7f82b3fe57dc16c`.
pub const COUNSEL_MELPA_PIN: (&str, &str) = ("counsel", "20260214.1004");

/// The exact counsel-projectile package selected for practical action-list
/// mutation, file matchers, and project buffer/file helpers. MELPA built this
/// archive from upstream commit `e30150792a96968f55f34638cbfe63eaa30839cc`.
pub const COUNSEL_PROJECTILE_MELPA_PIN: (&str, &str) = ("counsel-projectile", "20211004.2003");

/// The exact ctable package selected for practical model construction, text,
/// buffer, and embedded-region rendering, sorting, navigation, selection,
/// event hooks, model replacement, formatting, and async-wrapper parity.
/// MELPA built version 0.1.3 from upstream commit
/// `48b73742757a3ae5736d825fe49e00034cc453b5`.
pub const CTABLE_MELPA_PIN: (&str, &str) = ("ctable", "20210128.629");

/// The exact Cyberpunk Theme package selected for practical theme loading,
/// palette registration for a documented `((class color) (min-colors 89))'
/// display, variable settings, disable/enable lifecycle, and overlay-theme
/// precedence parity. MELPA built this archive from upstream commit
/// `1fd5350ddfc53c30e6eef82af77c62d7c825df3c`.
pub const CYBERPUNK_THEME_MELPA_PIN: (&str, &str) = ("cyberpunk-theme", "20240112.1944");

/// The exact cython-mode package selected for practical Cython major-mode
/// derivation from python-mode, defun navigation, block detection, and compile
/// command setup. MELPA built this archive from upstream commit
/// `3e4790559d3168fe992cf2aa62f01423038cedb5`.
pub const CYTHON_MODE_MELPA_PIN: (&str, &str) = ("cython-mode", "20221130.1257");

/// The exact obsolete Color Theme package selected for practical warning,
/// selection-buffer, installation, history, snapshot, and recovery parity.
/// MELPA built this archive from upstream commit
/// `3a2f6b615f5e2401e30d93a3e0adc210bbb4b7aa`.
pub const COLOR_THEME_MELPA_PIN: (&str, &str) = ("color-theme", "20190220.1115");

/// The exact Sanityinc Tomorrow theme package selected for five-variant palette,
/// wrapper-command, source fontification, diff/ANSI integration, lifecycle,
/// registry, and color-helper parity. MELPA built this archive from upstream
/// commit `d32469ec6529e3a7f84b45277f233497b74f5bab`.
pub const COLOR_THEME_SANITYINC_TOMORROW_MELPA_PIN: (&str, &str) =
    ("color-theme-sanityinc-tomorrow", "20260710.1606");

/// The exact DevDocs package selected for installed-document rendering,
/// entry/page/history navigation, lookup, hyperlinks, bookmarks, document
/// management, and error parity. MELPA built this archive from upstream commit
/// `25c746024ddf73570195bf42b841f761a2fee10c`.
pub const DEVDOCS_MELPA_PIN: (&str, &str) = ("devdocs", "20251022.1255");

/// The exact f package selected by the comprehensive API parity corpus.

/// The exact fuzzy package selected for practical Jaro-Winkler scoring,
/// fuzzy completions, search regexps, and isearch activation. MELPA built
/// this archive from upstream commit `3dc04f0a037d53d1174a1f38dce8a4b3498fa947`.
pub const FUZZY_MELPA_PIN: (&str, &str) = ("fuzzy", "20251231.1622");

pub const F_MELPA_PIN: (&str, &str) = ("f", "20241003.1131");

/// The exact fringe-helper package selected for practical fringe bitmap
/// conversion, definition, point/region insertion, removal, and stock library
/// loading. MELPA built this archive from upstream commit
/// `9bc3d3e82c9cc3937aa090248dc4dd2e289fc55c`.
pub const FRINGE_HELPER_MELPA_PIN: (&str, &str) = ("fringe-helper", "20140620.2109");

/// The exact Fancy Battery package selected by the practical cached-status,
/// mode-line rendering, update-hook, backend, and global-mode lifecycle parity
/// corpus. MELPA built this archive from upstream commit
/// `bcc2d7960ba207b5b4db96fe40f7d72670fdbb68`.
pub const FANCY_BATTERY_MELPA_PIN: (&str, &str) = ("fancy-battery", "20150101.1204");

/// The exact Find File in Project package selected for practical real-project
/// discovery, completion, history, relative-path, and diff-navigation parity.
/// MELPA built this archive from upstream commit
/// `6d6e132f5e9ebcbe5b475df939c556794dd1ce64`.
pub const FIND_FILE_IN_PROJECT_MELPA_PIN: (&str, &str) = ("find-file-in-project", "20250612.234");

/// The exact Fish Mode package selected for practical fish-script opening,
/// fontification, indentation, fish_indent save-hook, and unmatched
/// end/case recovery parity. MELPA built this archive from upstream
/// commit `2526b1803b58cf145bc70ff6ce2adb3f6c246f89`.
pub const FISH_MODE_MELPA_PIN: (&str, &str) = ("fish-mode", "20240129.1213");

/// The exact Gruvbox Theme package selected for practical public variant,
/// palette, fontification, option, failure, display-gate, and restoration
/// parity. MELPA built this archive from upstream commit
/// `6cbf80b6cde3c2390502dc94a911ab7378495249`.
pub const GRUVBOX_THEME_MELPA_PIN: (&str, &str) = ("gruvbox-theme", "20250117.222");

/// The exact gh.el API client selected for practical authenticated repository,
/// issue, gist, paging, mutation, and cache workflow parity. MELPA built this
/// archive from upstream commit `b1551245d3404eac6394abaebe1a9e0b2c504235`.
pub const GH_MELPA_PIN: (&str, &str) = ("gh", "20260210.1535");

/// The historical gist.el package selected for practical public creation,
/// listing, filtering, fetching, editing, mutation, and Dired integration
/// parity. MELPA built this archive from upstream commit
/// `b2712a61d04af98a05cc2556d85479803b6626be`.
pub const GIST_MELPA_PIN: (&str, &str) = ("gist", "20171128.406");

/// The exact gh-md package selected for practical GitHub Markdown API
/// rendering, region/buffer conversion, GFM context payloads, HTML export
/// customization, Unicode request encoding, and transport-error parity.
/// MELPA built this archive from upstream commit
/// `e721fd5e41e682f47f2dd4ce26ef2ba28c7fa0b5`.
pub const GH_MD_MELPA_PIN: (&str, &str) = ("gh-md", "20220316.1432");

/// The exact ghostel package pin for its MELPA parity corpus.
/// MELPA built this archive from upstream commit `02d0e3743dbe1a8c607adcfdc526367d798f4c23`.
pub const GHOSTEL_MELPA_PIN: (&str, &str) = ("ghostel", "20260820.1035");

/// The exact Magit package containing the Git-Commit source selected by the
/// comprehensive API parity corpus.
pub const GIT_COMMIT_MELPA_PIN: (&str, &str) = ("magit", "20260724.2338");

/// The terminal historical git-commit-mode package selected for practical
/// activation, fontification, editing, history, and editor-session parity.
/// MELPA built version 20141106.1722 from the exact recipe-selected
/// `git-commit-mode.el` at upstream commit
/// `7138eecb882e58466079d79925ccf85e3c24e866` before replacing the retired
/// package with modern `git-commit`.
pub const GIT_COMMIT_MODE_MELPA_PIN: (&str, &str) = ("git-commit-mode", "20141106.1722");

/// The terminal standalone Git-Commit package required by the retired
/// Git-Gutter+ archive. MELPA built it from Magit commit
/// `b8133ab8c9be47139019d97ccace49d807cac17a` before folding it into Magit.
pub const GIT_COMMIT_STANDALONE_MELPA_PIN: (&str, &str) = ("git-commit", "20180607.906");

/// The terminal Git-Gutter+ package selected for practical diff rendering,
/// hunk navigation, staging, reverting, commit-failure, and recovery parity.
/// MELPA built version 0.4 from upstream commit
/// `b7726997806d9a2da9fe84ff00ecf21d62b6f975`.
pub const GIT_GUTTER_PLUS_MELPA_PIN: (&str, &str) = ("git-gutter+", "20151204.923");

/// The terminal historical git-rebase-mode package selected for practical
/// mode, editing, external-boundary, and server-client lifecycle parity.
/// MELPA's recipe-selected first-parent source change is upstream merge
/// `acccc25f5207cfa93fe3faf36d315bdc1cecebfc`.
pub const GIT_REBASE_MODE_MELPA_PIN: (&str, &str) = ("git-rebase-mode", "20150122.1914");

/// The exact Git Link package selected for practical repository file, region,
/// commit, homepage, remote-resolution, hosting-provider, and error parity.
/// MELPA built version 0.11.0 from upstream commit
/// `ca01d013bd575710e2cd47001ee1ef6ee41667cf`.
pub const GIT_LINK_MELPA_PIN: (&str, &str) = ("git-link", "20260723.2213");

/// The exact Git Messenger package selected for practical blame popups,
/// commit details, uncommitted work, copy actions, revision buffers, parent
/// navigation, repository discovery, and failure parity. MELPA built this
/// archive from upstream commit `fb9a049ac3b5fba7369ef1f027b97881f1e377ec`.
pub const GIT_MESSENGER_MELPA_PIN: (&str, &str) = ("git-messenger", "20201202.1637");

/// The exact Git Gutter package selected for real repository hunk detection,
/// gutter rendering, navigation, staging, reverting, customization, and mode
/// lifecycle parity. MELPA built this archive from upstream commit
/// `3bdead17db7b84270c00e5a6b5ad02fa87ddd52e`.
pub const GIT_GUTTER_MELPA_PIN: (&str, &str) = ("git-gutter", "20241212.1415");

/// The exact Git Gutter Fringe adapter selected for real repository,
/// asynchronous refresh, fringe overlay ownership, and graphical row parity.
/// MELPA built this archive from upstream commit
/// `648cb5b57faec55711803cdc9434e55a733c3eba`.
pub const GIT_GUTTER_FRINGE_MELPA_PIN: (&str, &str) = ("git-gutter-fringe", "20211003.2228");

/// The exact Git Timemachine package selected for practical revision
/// navigation, renamed-file, branch, introduction-search, hash-copy, and
/// validation parity. MELPA built this archive from upstream commit
/// `d1346a76122595aeeb7ebb292765841c6cfd417b`.
pub const GIT_TIMEMACHINE_MELPA_PIN: (&str, &str) = ("git-timemachine", "20250128.940");

/// The exact Gitignore Templates package selected for practical GitHub and
/// gitignore.io edit, file-creation, provider-cache, transport-failure, and
/// response-cleanup parity. MELPA built this archive from upstream commit
/// `d28cd1cec00242b688861648d36d086818b06099`.
pub const GITIGNORE_TEMPLATES_MELPA_PIN: (&str, &str) = ("gitignore-templates", "20210814.144");

/// The exact Ggtags package selected for real GNU Global project lifecycle,
/// compilation navigation, completion, Xref, incremental-update, and failure
/// parity. MELPA built this archive from upstream commit
/// `4e3630c30fb836872b5d8f2ae3e5d5ae003365d8`.
pub const GGTAGS_MELPA_PIN: (&str, &str) = ("ggtags", "20230602.133");

/// The exact Git Modes successor selected to cover the historical standalone
/// `gitconfig-mode`, `gitignore-mode`, and `gitattributes-mode` Top-500 entries
/// through practical file detection, rendering, editing, documentation, and
/// navigation parity. MELPA built version 1.5.0 from upstream commit
/// `f291a4cc4a8b02a25d5cf93b4ab6af29e6f060d9`.
pub const GIT_MODES_MELPA_PIN: (&str, &str) = ("git-modes", "20260601.1550");

/// The exact General package selected by the comprehensive API parity corpus.
pub const GENERAL_MELPA_PIN: (&str, &str) = ("general", "20250612.2309");

/// The exact goto-chg package selected by the comprehensive API parity corpus.
pub const GOTO_CHG_MELPA_PIN: (&str, &str) = ("goto-chg", "20240407.1110");

/// The exact Gptel package selected for practical chat-buffer, request,
/// context, tool, preset, and response-navigation parity. MELPA built this
/// archive from upstream commit `dc0280821c344ec10547e13179ea2095f6165f05`.
pub const GPTEL_MELPA_PIN: (&str, &str) = ("gptel", "20260812.1855");

/// The exact Helm package selected by the practical source, matching, action,
/// completion, imenu, and occur parity corpus, and as audacious' runtime
/// user-interface dependency.

/// The exact helpful package selected for practical help buffer formatting,
/// alias resolution, and pretty-print paths. MELPA built this archive from
/// upstream commit `03756fa6ad4dcca5e0920622b1ee3f70abfc4e39`.
pub const HELPFUL_MELPA_PIN: (&str, &str) = ("helpful", "20250408.334");

pub const HELM_MELPA_PIN: (&str, &str) = ("helm", "20260728.709");

/// The exact Helm Org Rifle package selected for practical public search,
/// occur, navigation, timestamp-sort compatibility, directory-discovery, and
/// recovery coverage.
/// MELPA built this archive from upstream merge
/// `03a52265040b8c6510a8269213d750c451779c38`.
pub const HELM_ORG_RIFLE_MELPA_PIN: (&str, &str) = ("helm-org-rifle", "20230821.1927");

/// The exact Helm Company package selected for practical Company-backed
/// candidate presentation, completion, cancellation, documentation, and
/// source-location parity. MELPA built version 0.2.8 from upstream commit
/// `4622b82353220ee6cc33468f710fa5b6b253b7f1`.
pub const HELM_COMPANY_MELPA_PIN: (&str, &str) = ("helm-company", "20231113.701");

/// The exact Helm-Mode-Manager package selected for command discovery,
/// major-mode switching, minor-mode toggling, and persistent-help parity.
/// MELPA built this archive from upstream commit
/// `7df8ed3ddd46a0402838b748d317c01454346164`.
pub const HELM_MODE_MANAGER_MELPA_PIN: (&str, &str) = ("helm-mode-manager", "20210108.2330");

/// The exact archived Helm-Themes 0.05 source (normalized by Emacs to 0.5)
/// selected for theme candidate,
/// preview, acceptance, cancellation, and restoration parity. The final MELPA
/// source revision was `1fc4a5d6114bc6c8c444c5ca73f22abe141a690d`.
pub const HELM_THEMES_SOURCE_PIN: (&str, &str) = ("helm-themes", "0.5");

/// The exact Helm Ag package selected for practical search-command, result
/// highlighting, saved-result navigation, context-stack, multi-file edit, and
/// project-discovery parity. The released 0.64 source is pinned at upstream
/// commit `a7b43d9622ea5dcff3e3e0bb0b7dcc342b272171`.
pub const HELM_AG_MELPA_PIN: (&str, &str) = ("helm-ag", "0.64");

/// The exact Helm C Yasnippet package selected for practical snippet
/// discovery, completion, insertion, authoring, navigation, rename, delete,
/// and failure parity. MELPA built version 20230911.444 from upstream commit
/// `c6c9a14a65d11de967be593e5bead3196c1f4ecf`.
pub const HELM_C_YASNIPPET_MELPA_PIN: (&str, &str) = ("helm-c-yasnippet", "20230911.444");

/// The exact helm-core package selected by the practical source-extension,
/// candidate-buffer, pipeline, preview, and path parity corpus, and required
/// by the Helm parity corpus.
pub const HELM_CORE_MELPA_PIN: (&str, &str) = ("helm-core", "20260720.1307");

/// The exact Helm CSS SCSS package selected for real selector navigation,
/// comment generation, cache lifecycle, and modern-Helm compatibility parity.
/// MELPA built this archive from upstream commit
/// `2169d83d8fdc661241df208cb3235112735d936e`.
pub const HELM_CSS_SCSS_MELPA_PIN: (&str, &str) = ("helm-css-scss", "20230522.1113");

/// The exact Helm-Descbinds package selected for practical active-keymap
/// collection, prefix narrowing, Helm source/candidate construction, command
/// execution and help actions, global-mode lifecycle, and launch parity.
/// MELPA built this archive from upstream commit
/// `0aff44badad976ebf2666a7e9b6ddf4db53e59e5`.
pub const HELM_DESCBINDS_MELPA_PIN: (&str, &str) = ("helm-descbinds", "20250705.942");

/// The exact Helm-Flx package selected for practical command-palette,
/// display/real candidate, large-candidate, highlighting, find-file, locate,
/// and global-mode lifecycle parity. MELPA built this archive from upstream
/// commit `5220099e695a3586dba2d59640217fe378e66310`.
pub const HELM_FLX_MELPA_PIN: (&str, &str) = ("helm-flx", "20221020.1739");

/// The exact final Helm Git Grep source selected for practical real-Git
/// search, result navigation, option reruns, pathspec listing, saved-result,
/// and recovery parity. The final upstream release is commit
/// `744cea07dba6e6a5effbdba83f1b786c78fd86d3` (version 0.10.1).
pub const HELM_GIT_GREP_MELPA_PIN: (&str, &str) = ("helm-git-grep", "20170614.1411");

/// The exact Helm Gtags package selected for practical real-protocol Global
/// database creation, definition/reference/pattern/file navigation, parse-file,
/// update, mode lifecycle, failure, and recovery parity. MELPA built this
/// archive from upstream commit
/// `bfafd3d4a7f028d42f3f46c3273eaed930269ec6`.
pub const HELM_GTAGS_MELPA_PIN: (&str, &str) = ("helm-gtags", "20260204.1753");

/// The exact Helm Gitignore package selected for real interactive template
/// lookup, single and ordered multi-selection, generated-buffer editing,
/// saving, refresh, and HTTP failure parity. MELPA built this archive from
/// upstream commit `85c34065e6fceac8fa7287e6ec79ea3d1182d654`.
pub const HELM_GITIGNORE_MELPA_PIN: (&str, &str) = ("helm-gitignore", "20230310.1829");

/// The exact helm-make package selected for practical nested-project target
/// discovery, source saving, cache invalidation, Ninja, Projectile, and GNU
/// make database workflows. MELPA built this archive from upstream commit
/// `ebd71e85046d59b37f6a96535e01993b6962c559`.
pub const HELM_MAKE_MELPA_PIN: (&str, &str) = ("helm-make", "20200620.27");

/// The exact helm-org package selected for practical Org heading candidate
/// collection, depth filtering, preselect, marker navigation, and Helm source
/// construction. MELPA built this archive from upstream commit
/// `4744ca7f8b35e17bafce9cb0093deb87a232699d`.
pub const HELM_ORG_MELPA_PIN: (&str, &str) = ("helm-org", "20250405.1720");

/// The exact Helm Pydoc package selected by its practical parity workflows.
/// MELPA built version 20220721.433 from upstream commit
/// `cac7b8953adcab85e898bc42b699c3afde5d33c6`.
pub const HELM_PYDOC_MELPA_PIN: (&str, &str) = ("helm-pydoc", "20220721.433");

/// The exact helm-ls-git package selected for practical git-root discovery,
/// ls-files listing, branch normalization, status/source defaults, and project
/// manager command surface. MELPA built this archive from upstream commit
/// `dd0ed5847d4bf1b27e767cf194475ada88ee8898`.
pub const HELM_LS_GIT_MELPA_PIN: (&str, &str) = ("helm-ls-git", "20260105.455");

/// The exact Helm LSP package selected for practical workspace-symbol,
/// code-action, diagnostic filtering, navigation, and failure parity. MELPA
/// built this archive from upstream commit
/// `056bb16b5f69137218613b7558b477f6b21f22be`.
pub const HELM_LSP_MELPA_PIN: (&str, &str) = ("helm-lsp", "20260507.1749");

/// The exact final Helm Swoop source selected for practical candidate,
/// narrowing, query, editing, face, and buffer-selection parity. MELPA's last
/// rolling build came from upstream commit
/// `df90efd4476dec61186d80cace69276a95b834d2` before its recipe was retired.
pub const HELM_SWOOP_MELPA_PIN: (&str, &str) = ("helm-swoop", "20240104.2356");

/// The exact Helm-Projectile package selected for practical project file,
/// buffer, Dired, search, switching, DWIM, and mode-lifecycle parity.
/// This MELPA version corresponds to upstream commit
/// `4dae1d072cc2650749846cfcab1f60686471cc45`.
pub const HELM_PROJECTILE_MELPA_PIN: (&str, &str) = ("helm-projectile", "20260724.27");

/// The exact Helm Purpose package selected for practical Purpose-filtered
/// Helm buffer switching, purpose selection, preferred-prompt setup, and
/// Purpose-ignoring mini sessions. MELPA built this archive from upstream
/// commit `9ff4c21c1e9ebc7afb851b738f815df7343bb287`.
pub const HELM_PURPOSE_MELPA_PIN: (&str, &str) = ("helm-purpose", "20170114.1636");

/// The exact helm-xref package selected for practical Xref definition,
/// reference, candidate-formatting, preview, window-action, history, and
/// failure parity. MELPA built this archive from upstream commit
/// `ea0e4ed8a9baf236e4085cbc7178241f109a53fa`.
pub const HELM_XREF_MELPA_PIN: (&str, &str) = ("helm-xref", "20211017.1334");

/// The exact Hl-Todo package selected for practical source annotation,
/// navigation, insertion, fontification, Flymake, and global-mode parity.
/// This MELPA version corresponds to upstream commit
/// `527d545b8c2f36243194cbe4a8d0e6ac9d50e6a7` (tag `v3.9.4`).
pub const HL_TODO_MELPA_PIN: (&str, &str) = ("hl-todo", "20260601.1508");

/// The exact wfnames package selected for the practical filename-list setup,
/// edit tracking, cross-directory rename, swap, overwrite, revert, and
/// reordering parity corpus, and required by the Helm parity corpus. MELPA
/// built this archive from upstream commit
/// `d8839fa42a24f7c781cd2d8c3f40eda31faa19be`.
pub const WFNAMES_MELPA_PIN: (&str, &str) = ("wfnames", "20260706.903");

/// The exact w3m package selected for practical HTML rendering, link/form
/// navigation, history, bookmarks, and external-renderer failure/recovery
/// parity. MELPA built this archive from upstream commit
/// `bb01ba0329ee5b02c2ff260d8881bbc6f389d80a`.
pub const W3M_MELPA_PIN: (&str, &str) = ("w3m", "20260811.923");

/// The exact Writeroom Mode package selected for practical focused-writing
/// buffer state, Visual Fill Column cooperation, width adjustment, mode-line,
/// multi-buffer, frame-effect, admission, and window-layout parity. MELPA
/// built version 3.12 from upstream commit
/// `cca2b4b3cfcfea1919e1870519d79ed1a69aa5e2`.
pub const WRITEROOM_MODE_MELPA_PIN: (&str, &str) = ("writeroom-mode", "20250204.2335");

/// The exact web-completion-data package selected for the practical HTML tag,
/// attribute, value, documentation, source-extension, and dataset-integrity
/// parity corpus. MELPA built this archive from upstream commit
/// `c272c94e8a71b779c29653a532f619acad433a4f`.
pub const WEB_COMPLETION_DATA_MELPA_PIN: (&str, &str) = ("web-completion-data", "20160318.848");

/// The exact Web Mode package selected for practical mixed-language editing,
/// indentation, fontification, structural transformation, and template parity.
/// MELPA built this archive from upstream commit
/// `aeee2d4c82a791ff69657c1413873bf9265544df`.
/// The exact web-beautify package selected for practical HTML/CSS/JS formatter
/// command wiring, shell-command construction, and missing-program messaging.
/// MELPA built this archive from upstream commit
/// `e1b45321d8c11b404b12c8e55afe55eaa7c84ee9`.
pub const WEB_BEAUTIFY_MELPA_PIN: (&str, &str) = ("web-beautify", "20161115.2247");

pub const WEB_MODE_MELPA_PIN: (&str, &str) = ("web-mode", "20260623.932");

/// The exact websocket package selected for the practical loopback client and
/// server lifecycle, fragmented text, extended binary frame, and callback
/// recovery parity corpus. MELPA built this archive from upstream commit
/// `2195e1247ecb04c30321702aa5f5618a51c329c5`.
pub const WEBSOCKET_MELPA_PIN: (&str, &str) = ("websocket", "20260301.157");

/// The exact Wgrep package selected for practical writable grep-buffer
/// parsing, context editing, multi-file replacement, deletion, abort, saving,
/// and stale-source rejection parity. MELPA built version 3.0.0 from upstream
/// commit `b4d69280d8a6a5ded1597e02afbaa811a160383b`.
pub const WGREP_MELPA_PIN: (&str, &str) = ("wgrep", "20230203.1214");

/// The exact YAML Mode package selected for practical configuration,
/// indentation, electric editing, syntax/fontification, filling, Imenu, and
/// multi-document navigation parity. MELPA built this archive from upstream
/// commit `62cbd80507765aa8326bd6aef3aacd8d9be2d71a`.
pub const YAML_MODE_MELPA_PIN: (&str, &str) = ("yaml-mode", "20260420.156");

/// The exact xcscope package selected for the practical source-search,
/// navigation, history, rerun, and recursive indexing parity corpus. MELPA
/// built this archive from upstream commit
/// `2f35b26428dd82c016941744f03aad97df80c47b`.
pub const XCSCOPE_MELPA_PIN: (&str, &str) = ("xcscope", "20230626.2109");

/// The exact Magit package selected by the comprehensive API parity corpus.
pub const MAGIT_MELPA_PIN: (&str, &str) = ("magit", "20260724.2338");

/// The final Magit Gitflow package selected for practical status-buffer,
/// Git Flow initialization, branch lifecycle, configuration, diff, and
/// failure parity. MELPA built this version from upstream commit
/// `cc41b561ec6eea947fe9a176349fb4f771ed865b`. External-command responses
/// are recorded from Git Flow AVH 1.12.3 at commit
/// `d409eff2896b02e1ae1ac76c291aaf15213aac6d`.
pub const MAGIT_GITFLOW_MELPA_PIN: (&str, &str) = ("magit-gitflow", "20170929.824");

/// The final Magit Popup package selected for practical popup rendering,
/// infix refresh, suffix dispatch, prefix defaults, extension, CLI argument,
/// and sequence-mode parity. MELPA built version 2.13.3 from upstream commit
/// `d8585fa39f88956963d877b921322530257ba9f5`.
pub const MAGIT_POPUP_MELPA_PIN: (&str, &str) = ("magit-popup", "20200719.1015");

/// The exact magit-section package selected by the comprehensive API parity
/// corpus.
pub const MAGIT_SECTION_MELPA_PIN: (&str, &str) = ("magit-section", "20260722.2131");

/// The exact Memoize package selected for structured-key, expiry, function
/// restoration, recursive computation, nil/error, and buffer-content cache
/// parity. MELPA built version 1.1 from upstream commit
/// `51b075935ca7070f62fae1d69fe0ff7d8fa56fdd`.
pub const MEMOIZE_MELPA_PIN: (&str, &str) = ("memoize", "20200103.2036");

/// The exact Password Generator package selected for practical interactive,
/// returned-value, phonetic, word-list, custom-alphabet, and invalid-setting
/// parity. MELPA built this version from upstream commit
/// `2d0deb52f2fd978bff9001e155e36ac5bd287d52`.
pub const PASSWORD_GENERATOR_MELPA_PIN: (&str, &str) = ("password-generator", "20250615.2300");

/// The exact Projectile package selected by the comprehensive API parity
/// corpus.
pub const PROJECTILE_MELPA_PIN: (&str, &str) = ("projectile", "20260728.945");

/// The exact s package selected by the live lifecycle and comprehensive API
/// parity corpora.

/// The exact sesman package selected for practical generic session lifecycle,
/// linking (buffer/directory/project), and multi-system registration parity.
/// MELPA built this archive from upstream commit
/// `7eb733acb33e610a53979fa7fc13393eeda3cc53`.
pub const SESMAN_MELPA_PIN: (&str, &str) = ("sesman", "20260616.1239");

pub const S_MELPA_PIN: (&str, &str) = ("s", "20220902.1511");

/// The exact Shrink Path package selected for practical eshell/modeline
/// rendering, file labels, real filesystem expansion, ambiguity, and mixed
/// project-relative path parity, as well as Doom Modeline's path-rendering
/// dependency. MELPA assigned this version to upstream commit
/// `c14882c8599aec79a6e8ef2d06454254bb3e1e41`.
pub const SHRINK_PATH_MELPA_PIN: (&str, &str) = ("shrink-path", "20190208.1335");

/// The exact SCSS Mode package selected for practical project activation,
/// nested stylesheet authoring, syntax/fontification, compilation,
/// compile-on-save, legacy Flymake, and diagnostic-navigation parity. MELPA
/// built this version from upstream commit
/// `cf58dbec5394280503eb5502938f3b5445d1b53d`.
pub const SCSS_MODE_MELPA_PIN: (&str, &str) = ("scss-mode", "20180123.1708");

/// The exact Sass Mode package selected for practical project activation,
/// nested stylesheet indentation and navigation, fontification, and compiler
/// success/error parity. MELPA built this archive from upstream commit
/// `247a0d4b509f10b28e4687cd8763492bca03599b`.
pub const SASS_MODE_MELPA_PIN: (&str, &str) = ("sass-mode", "20190502.53");

/// The exact slim-mode package selected for practical Slim major-mode
/// indentation, nested sexp navigation, comment blocks, and .slim auto-mode
/// association. MELPA built this archive from upstream commit
/// `8c92169817f2fa59255f547f0a9fb4fbb8309db9`.
pub const SLIM_MODE_MELPA_PIN: (&str, &str) = ("slim-mode", "20240513.211");

/// The exact Treemacs package selected for practical project admission,
/// workspace lifecycle, persistence, selection, and terminal tree parity.
/// MELPA built this archive from upstream commit
/// `2ab5a3c89fa01bbbd99de9b8986908b2bc5a7b49`.
pub const TREEMACS_MELPA_PIN: (&str, &str) = ("treemacs", "20251226.1307");

/// The exact Treemacs-Persp package selected for practical persp-mode scope
/// registration, workspace creation on perspective switch/rename, and hook
/// lifecycle parity. MELPA built this archive from Treemacs commit
/// `55079b017fb821a34ace398cd3d8c5b556a22f6d`.
pub const TREEMACS_PERSP_MELPA_PIN: (&str, &str) = ("treemacs-persp", "20250320.2145");

/// The exact Treemacs Evil integration selected for practical Evil state
/// activation, Treemacs navigation and action bindings, mouse-state recovery,
/// window-move compatibility, and advice registration parity. MELPA built this
/// archive from upstream commit `55079b017fb821a34ace398cd3d8c5b556a22f6d`.
pub const TREEMACS_EVIL_MELPA_PIN: (&str, &str) = ("treemacs-evil", "20250320.2145");

/// The exact Treemacs Icons Dired package selected for practical graphical
/// Dired activation, entry insertion, subdirectory, revert, teardown, and
/// one-shot enablement parity. MELPA built this version from the Treemacs
/// monorepo commit `55079b017fb821a34ace398cd3d8c5b556a22f6d`.
pub const TREEMACS_ICONS_DIRED_MELPA_PIN: (&str, &str) = ("treemacs-icons-dired", "20250320.2145");

/// The exact Treemacs-Magit integration selected for practical real-repository
/// staging, update coalescing, Treemacs refresh, and disabled-mode parity.
/// MELPA built this archive from the Treemacs monorepo commit
/// `68e444e066a30d70a201fb162c8cf3d472226853`.
pub const TREEMACS_MAGIT_MELPA_PIN: (&str, &str) = ("treemacs-magit", "20250726.2233");

/// The exact Treemacs-Projectile package selected for practical workspace
/// admission, project discovery, startup-root, file-buffer/cache, mouse-menu,
/// keymap, and hook-registration parity. MELPA built this archive from the
/// Treemacs monorepo commit `f80a309319c2374585babcb3e00ea6f3314160f3`.
pub const TREEMACS_PROJECTILE_MELPA_PIN: (&str, &str) = ("treemacs-projectile", "20250320.2206");

/// The exact Org Category Capture package selected for practical category
/// indexing, property precedence, heading creation, subtree reporting,
/// marker navigation, template construction, and capture workflow parity.
/// MELPA built it from commit `0521cbb6bb371cbfd9b7b5688b82ac119af1bf30`.
pub const ORG_CATEGORY_CAPTURE_MELPA_PIN: (&str, &str) = ("org-category-capture", "20260127.711");

/// The exact Org Project Capture dependency selected for the public
/// Org-Projectile compatibility workflows. MELPA built it from upstream
/// commit `6a95fb90bcdb7fcdeba9d9421d7c511cea95ef70`.
pub const ORG_PROJECT_CAPTURE_MELPA_PIN: (&str, &str) = ("org-project-capture", "20260313.1738");

/// The exact Org-Projectile compatibility package selected for practical
/// Projectile-backed project capture, navigation, and storage parity. MELPA
/// built it from upstream commit `6a95fb90bcdb7fcdeba9d9421d7c511cea95ef70`.
pub const ORG_PROJECTILE_MELPA_PIN: (&str, &str) = ("org-projectile", "20260313.1738");

/// The exact Diminish package selected for practical live minor-mode,
/// abbreviation, reporting, restoration, and configuration-order parity.
/// MELPA built this archive from upstream commit
/// `fbd5d846611bad828e336b25d2e131d1bc06b83d`.
pub const DIMINISH_MELPA_PIN: (&str, &str) = ("diminish", "20220909.847");

/// The exact Diff HL package selected for practical working-tree hunk,
/// navigation, selective staging, revert, unsaved flydiff, reference-revision,
/// and mode-lifecycle parity. MELPA built this archive from upstream commit
/// `91fcd4fa42fef895a754e80c4435ae6314be7822`.

/// The exact dired-quick-sort package selected for practical persistent Dired
/// listing switch formatting, sort criteria, reverse/group toggles, and setup
/// wiring. MELPA built this archive from upstream commit
/// `3c9b41799b0424eb78f54caba56e4de1d7224e8b`.
pub const DIRED_QUICK_SORT_MELPA_PIN: (&str, &str) = ("dired-quick-sort", "20260331.2219");

/// The exact diredfl package selected for practical extra Dired fontification
/// of a real listing (privileges, numbers, dates, names, suffixes, symlinks,
/// marks, and deletion flags), customization, and mode lifecycle parity.
/// MELPA built this archive from upstream commit
/// `fe72d2e42ee18bf6228bba9d7086de4098f18a70`.
pub const DIREDFL_MELPA_PIN: (&str, &str) = ("diredfl", "20241201.1141");

pub const DIFF_HL_MELPA_PIN: (&str, &str) = ("diff-hl", "20260723.238");

/// The exact Dockerfile Mode package selected for practical multi-stage
/// editing, fontification, Imenu, indentation, comments, image naming, and
/// deterministic build-command parity. MELPA built this archive from upstream
/// commit `97733ce074b1252c1270fd5e8a53d178b66668ed`.
pub const DOCKERFILE_MODE_MELPA_PIN: (&str, &str) = ("dockerfile-mode", "20251221.1644");

/// The exact docker package selected for practical human-size parsing, column
/// format helpers, process argv assembly, and terminal-backend selection.
/// MELPA built this archive from upstream commit
/// `8a51aee19a7931bc16aa63cf076b109cdd6a1c62`.
pub const DOCKER_MELPA_PIN: (&str, &str) = ("docker", "20260803.930");

/// The exact docker-tramp package selected for practical docker TRAMP
/// method registration, running-container completion, cache cleanup, and
/// modern-tramp compat no-op parity. MELPA built this archive from
/// upstream commit `19d0771db4e6b89e19c00af5806438e315779c15`.
pub const DOCKER_TRAMP_MELPA_PIN: (&str, &str) = ("docker-tramp", "20230809.511");

/// The exact Doom Modeline package selected for practical file-buffer,
/// state-transition, selection, encoding, extension, layout, and global-mode
/// lifecycle parity. MELPA built this archive from upstream commit
/// `017854c6484dd6a38e4b039dad04ce6dbec02f08`.
pub const DOOM_MODELINE_MELPA_PIN: (&str, &str) = ("doom-modeline", "20260708.823");

/// The exact Doom Themes package selected for practical full-catalog loading,
/// dark/light switching, code fontification, typography policy, custom face
/// composition, palette derivation, and Org extension parity. MELPA built this

/// The exact drag-stuff package selected for practical line/word/region
/// dragging vertically and horizontally with minor-mode bindings. MELPA built
/// this archive from upstream commit `d49fe376d24f0f8ac5ade67b6d7fccc2487c81db`.
pub const DRAG_STUFF_MELPA_PIN: (&str, &str) = ("drag-stuff", "20161108.749");

/// archive from upstream commit `53645a905dfb3055db52f5d418d5ef612027e062`.
pub const DOOM_THEMES_MELPA_PIN: (&str, &str) = ("doom-themes", "20260117.2323");

/// The exact Dotenv Mode package selected for practical environment-file
/// detection, mixed assignment syntax, quote-sensitive interpolation,
/// comments, incremental refontification, and editing parity. The final
/// upstream release is commit `e3701bf739bde44f6484eb7753deadaf691b73fb`
/// (tag `v0.2.5`).
pub const DOTENV_MODE_MELPA_PIN: (&str, &str) = ("dotenv-mode", "20191027.2129");

/// The exact Visual Fill Column package selected for practical soft-wrapping,
/// centered and right-to-left layouts, text scaling, multi-window resizing,
/// hook lifecycle, and file-buffer global-mode parity. MELPA built this archive

/// The exact Vertico package selected for practical vertical completion UI
/// mode, candidate cycling/navigation helpers, sorting, and minibuffer
/// integration. MELPA built this archive from upstream commit
/// `be96000c2b0b3501723291b3721ceba12f784dcd`.
pub const VERTICO_MELPA_PIN: (&str, &str) = ("vertico", "20260805.1129");

/// from upstream commit `9c0ecc2af21d3024a2a838c30d574e86265a52be`.
pub const VISUAL_FILL_COLUMN_MELPA_PIN: (&str, &str) = ("visual-fill-column", "20251110.1039");

/// The exact Column Enforce Mode package selected for practical code-width,
/// comment policy, incremental editing, contextual limits, interactive rules,
/// overlay lifecycle, and global admission parity. MELPA built this archive
/// from upstream commit `14a7622f2268890e33536ccd29510024d51ee96f`.
pub const COLUMN_ENFORCE_MODE_MELPA_PIN: (&str, &str) = ("column-enforce-mode", "20200605.1933");

/// The exact Expand Region package selected for practical progressive
/// selection, contraction, history invalidation, register autocopy, and
/// language-specific region parity. MELPA built this archive from upstream
/// commit `351279272330cae6cecea941b0033a8dd8bcc4e8`.
pub const EXPAND_REGION_MELPA_PIN: (&str, &str) = ("expand-region", "20241217.1840");

/// The exact Rainbow Delimiters package selected for practical syntax depth,
/// incremental refontification, language, diff, face cycling, customization,
/// and mode lifecycle parity. MELPA built this archive from upstream commit
/// `7919681b0d883502155d5b26e791fec15da6aeca`.
pub const RAINBOW_DELIMITERS_MELPA_PIN: (&str, &str) = ("rainbow-delimiters", "20210515.1254");

/// The exact rake.el package selected for practical Rakefile discovery,
/// task listing, bundler/zeus/spring prefixes, cache, find-task, and
/// rerun recovery parity. MELPA built this archive from upstream commit
/// `452ea0caca33376487103c64177c295ed2960cca`.
pub const RAKE_MELPA_PIN: (&str, &str) = ("rake", "20220211.827");

/// The exact Racket Mode package selected for practical classic-mode,
/// font-lock, indentation, editing, completion, Imenu, Xref, folding, and
/// recovery parity. MELPA built this archive from upstream commit
/// `f92a33dcc3b604f53ef23a538e26e1f25c4fea47`.
pub const RACKET_MODE_MELPA_PIN: (&str, &str) = ("racket-mode", "20260726.2002");

/// The exact Autothemer package selected for practical theme definition,
/// palette reuse, interactive color insertion, conversion/sorting, JSON
/// export, and guarded failure-recovery parity. MELPA built this archive from
/// upstream commit `99fd9b45ef6cc931fcf030b1a6c050ca3c17ce04`.
pub const AUTOTHEMER_MELPA_PIN: (&str, &str) = ("autothemer", "20260530.2349");

/// The exact Shut Up package selected for practical output capture, cleanup,
/// deterministic file generation, quiet loading, nested scopes, bypass, and
/// noninteractive startup parity. MELPA built this archive from upstream
/// commit `ed62a7fefdf04c81346061016f1bc69ca045aaf6`.
pub const SHUT_UP_MELPA_PIN: (&str, &str) = ("shut-up", "20240429.605");

/// The exact shell-pop package selected for practical shell buffer naming,
/// window size calculation, position translation, and pop-up/out lifecycle.
/// MELPA built this archive from upstream commit
/// `446b1691454e65be648dcb7e316639aa7dd73be2`.
pub const SHELL_POP_MELPA_PIN: (&str, &str) = ("shell-pop", "20260610.223");

/// The exact origami package selected for practical fold-tree construction,
/// overlay hide/show, history undo/redo, and parser-driven folding. MELPA
/// built this archive from upstream commit
/// `e558710a975e8511b9386edc81cd6bdd0a5bda74`.
pub const ORIGAMI_MELPA_PIN: (&str, &str) = ("origami", "20200331.1019");

/// The exact pytest package selected for practical command formatting, project
/// root discovery, and testable-name extraction. MELPA built this archive from
/// upstream commit `8692f965bf4ddf3d755cf1fbf77a7a768e22460e`.
pub const PYTEST_MELPA_PIN: (&str, &str) = ("pytest", "20230810.1218");

/// The exact Smartparens package selected for practical balanced typing,
/// structural refactoring, wrapping, strict editing, Python, and Markdown
/// parity. MELPA built this archive from upstream commit
/// `82d2cf084a19b0c2c3812e0550721f8a61996056`.
pub const SMARTPARENS_MELPA_PIN: (&str, &str) = ("smartparens", "20260129.1214");

/// The exact Smart Mode Line package selected for practical mode-line
/// installation through `sml/setup', rendered file-buffer lines, prefix
/// replacement rules, buffer identification, and the three theme palettes.
/// MELPA built this archive from upstream commit
/// `bbed708eb8393697e01ab2474dfb54d7c5ea7905`.
pub const SMART_MODE_LINE_MELPA_PIN: (&str, &str) = ("smart-mode-line", "20240924.2322");

/// The exact Swiper package selected for practical line search, match search,
/// query replacement, occur export, visibility, and multi-buffer parity.
/// MELPA built this archive from upstream commit
/// `d489b4f0d48fd215119261d92de103c5b5580895`.
pub const SWIPER_MELPA_PIN: (&str, &str) = ("swiper", "20260101.2125");

/// The exact String Inflection package selected for practical identifier
/// conversion, language-specific cycling, syntax-aware symbol selection,
/// multi-symbol region editing, and cursor/mark lifecycle parity. MELPA built
/// this archive from upstream commit
/// `072f7dff43140570788d64ac0ec9d930c3c2a96b` (package version 1.2.1).
pub const STRING_INFLECTION_MELPA_PIN: (&str, &str) = ("string-inflection", "20251114.1041");

/// The exact Symbol Overlay package selected for practical identifier
/// highlighting, face allocation, navigation, scoped rename, incremental
/// editing, temporary highlighting, and language-aware filtering parity.
/// MELPA built this archive from upstream commit
/// `85d100b0cca35b70cee1b260e09af8e1fb2fcc08` (package version 4.3).
pub const SYMBOL_OVERLAY_MELPA_PIN: (&str, &str) = ("symbol-overlay", "20260703.1437");

/// The exact symon package selected for practical custom-monitor generation,
/// history and display formatting, sparkline bitmap and XPM rendering,
/// multipage echo-area updates, redisplay, and global mode lifecycle parity.
/// MELPA built version 1.2.3 from upstream commit
/// `294668d63da642276a0003cb4e9d6f8b40a13788`.
pub const SYMON_MELPA_PIN: (&str, &str) = ("symon", "20260411.1454");

/// The exact Yasnippet package selected by the direct parity corpus and as
/// angular-snippets' manually documented runtime dependency.
pub const YASNIPPET_MELPA_PIN: (&str, &str) = ("yasnippet", "20250602.1342");

/// The exact Yasnippet-Snippets collection selected for practical snippet-tree
/// loading and expansion parity across representative major modes.
pub const YASNIPPET_SNIPPETS_MELPA_PIN: (&str, &str) = ("yasnippet-snippets", "20251215.1231");

/// The exact Transient package selected by the comprehensive API parity

/// The exact tagedit package selected for practical HTML tag insertion,
/// attribute editing, slurp/barf, raise/splice/split/join, and multiline
/// toggle parity. MELPA built this archive from upstream commit
/// `b3a70101a0dcf85498c92b7fcfa7fdbac869746c`.
pub const TAGEDIT_MELPA_PIN: (&str, &str) = ("tagedit", "20161121.855");

/// corpus.
pub const TRANSIENT_MELPA_PIN: (&str, &str) = ("transient", "20260725.1105");

/// The exact Transpose Frame package selected for practical window-tree
/// transpose, flip, flop, 180/90-degree rotate, dedicated-window, and
/// single-window recovery parity. MELPA built this archive from upstream
/// commit `94c87794d53883a2358d13da264ad8dab9a52daa`.
pub const TRANSPOSE_FRAME_MELPA_PIN: (&str, &str) = ("transpose-frame", "20221109.2053");

/// The exact Tuareg package selected for practical OCaml editing, SMIE
/// indentation, fontification, phrase/defun motion, comment-dwim,
/// compiler-error navigation, and unbraced-eval recovery parity. MELPA
/// built this archive from upstream commit
/// `2d67d53a66fbf9d83c0416dba3275080b1bc6dfd`.
pub const TUAREG_MELPA_PIN: (&str, &str) = ("tuareg", "20260626.936");

/// The exact Tree-sitter package selected as the runtime integration layer for
/// the pinned language bundle. MELPA built this archive from upstream commit
/// `8f0bd387ad7a1cf7e8fdd5977d386a17ea70a82d`.
pub const TREE_SITTER_MELPA_PIN: (&str, &str) = ("tree-sitter", "20260116.9");

/// The exact Tree-sitter Languages package selected for practical grammar
/// discovery, major-mode registration, bundled highlighting-query, download,
/// installation, skip, reinstall, and failure-recovery parity. MELPA built
/// this archive from upstream commit
/// `1a827f821fbcb967db5eecccd569b7fa4b93d152`.
pub const TREE_SITTER_LANGS_MELPA_PIN: (&str, &str) = ("tree-sitter-langs", "20260729.1912");

/// The exact tsc native binding package required by the pinned Tree-sitter
/// runtime. MELPA built it from upstream commit
/// `8f0bd387ad7a1cf7e8fdd5977d386a17ea70a82d`.
pub const TSC_MELPA_PIN: (&str, &str) = ("tsc", "20260116.9");

/// The exact Use-Package release selected from GNU ELPA by the comprehensive
/// API parity corpus.
pub const USE_PACKAGE_GNU_ELPA_PIN: (&str, &str) = ("use-package", "2.4.6");

/// The exact Uuidgen package selected for practical deterministic and random
/// UUID v1/v3/v4/v5 generation, namespace hashing, CID/URN serialization,
/// interactive insertion, clock, network, and validation parity. MELPA built
/// this archive from upstream commit `cebbe09d27c63abe61fe8c2e2248587d90265b59`.
pub const UUIDGEN_MELPA_PIN: (&str, &str) = ("uuidgen", "20240201.2318");

/// The exact Undercover package selected for practical Edebug instrumentation,
/// wildcard/configuration, text/LCOV/SimpleCov report, merge, lifecycle, and
/// failure-recovery parity. MELPA built this archive from upstream commit
/// `1d3587f1fad66a747688f36636b67b33b73447d3`.
pub const UNDERCOVER_MELPA_PIN: (&str, &str) = ("undercover", "20210602.2119");

/// The exact unfill package selected for practical paragraph/region unwrap and
/// fill/unfill toggle against filled prose. MELPA built this archive from
/// upstream commit `075052ce0b4451d7d3ede013ce5a77e6a7a92360`.
pub const UNFILL_MELPA_PIN: (&str, &str) = ("unfill", "20230227.1349");

/// The exact Which-Key package selected by the comprehensive API parity corpus.
pub const WHICH_KEY_MELPA_PIN: (&str, &str) = ("which-key", "20240620.2145");

/// The exact With-Editor package selected by the comprehensive API parity
/// corpus.
pub const WITH_EDITOR_MELPA_PIN: (&str, &str) = ("with-editor", "20260701.1252");

/// The exact Window Purpose package selected for practical buffer
/// classification, purpose-aware display routing, dedication, edge-window,
/// layout persistence, and mode-lifecycle parity. MELPA built this archive
/// from upstream commit `c827f45cd9b278b3eb9c2f4bcb55ef2fca5d3048`.
/// The exact Window Numbering package selected for practical numbered-window
/// enablement, per-window number assignment in `window-list' order, mode-line
/// installation, numbered selection and deletion commands, hook and
/// assign-func customization, and disable-lifecycle parity. MELPA built this
/// archive from upstream commit `10809b3993a97c7b544240bf5d7ce9b1110a1b89`.
pub const WINDOW_NUMBERING_MELPA_PIN: (&str, &str) = ("window-numbering", "20160809.1810");

/// The exact Window Purpose package selected for practical buffer
/// classification, purpose-aware display routing, dedication, edge-window,
/// layout persistence, and mode-lifecycle parity. MELPA built this archive
/// from upstream commit `c827f45cd9b278b3eb9c2f4bcb55ef2fca5d3048`.
pub const WINDOW_PURPOSE_MELPA_PIN: (&str, &str) = ("window-purpose", "20241207.148");

/// The exact Winum package selected for practical numbered-layout, selection,
/// deletion, custom-assignment, keymap, mode-line, and live-update parity.
/// MELPA built this archive from upstream commit
/// `098249c65042ee0308b8236d1ee838c8da8fdf25`.
pub const WINUM_MELPA_PIN: (&str, &str) = ("winum", "20190911.1607");

/// Package archive used by a scenario.
#[derive(Clone, Debug)]
pub struct PackageSource {
    archives: Vec<(String, PathBuf)>,
}

impl PackageSource {
    pub fn frozen(archive_dir: impl Into<PathBuf>) -> Self {
        Self {
            archives: vec![("frozen".to_string(), archive_dir.into())],
        }
    }

    pub fn local<I, N, P>(archives: I) -> Self
    where
        I: IntoIterator<Item = (N, P)>,
        N: Into<String>,
        P: Into<PathBuf>,
    {
        Self {
            archives: archives
                .into_iter()
                .map(|(name, path)| (name.into(), path.into()))
                .collect(),
        }
    }

    fn archive_form(&self) -> String {
        let entries = self
            .archives
            .iter()
            .map(|(name, directory)| {
                let directory = directory
                    .canonicalize()
                    .unwrap_or_else(|_| directory.clone());
                let directory = format!("{}/", directory.display());
                format!("({} . {})", elisp_string(name), elisp_string(&directory))
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("'({entries})")
    }
}

/// Packages and the post-restart Elisp probe that define one compatibility
/// scenario.
#[derive(Clone, Debug)]
pub struct PackageScenario {
    pub name: String,
    packages: PackageSelection,
    pub probe: String,
}

#[derive(Clone, Debug)]
enum PackageSelection {
    Unversioned(Vec<String>),
    Versioned(Vec<PackagePin>),
}

/// An exact package name/version selected for a live archive scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackagePin {
    pub name: String,
    pub version: String,
}

impl PackageScenario {
    pub fn new<I, P>(name: impl Into<String>, packages: I, probe: impl Into<String>) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<String>,
    {
        Self {
            name: name.into(),
            packages: PackageSelection::Unversioned(packages.into_iter().map(Into::into).collect()),
            probe: probe.into(),
        }
    }

    /// Define a scenario whose selected third-party packages have exact
    /// versions.
    pub fn versioned<I, N, V>(
        name: impl Into<String>,
        packages: I,
        probe: impl Into<String>,
    ) -> Self
    where
        I: IntoIterator<Item = (N, V)>,
        N: Into<String>,
        V: Into<String>,
    {
        Self {
            name: name.into(),
            packages: PackageSelection::Versioned(
                packages
                    .into_iter()
                    .map(|(name, version)| PackagePin {
                        name: name.into(),
                        version: version.into(),
                    })
                    .collect(),
            ),
            probe: probe.into(),
        }
    }

    pub fn from_probe_file<I, P>(
        name: impl Into<String>,
        packages: I,
        probe_path: impl AsRef<Path>,
    ) -> Result<Self, String>
    where
        I: IntoIterator<Item = P>,
        P: Into<String>,
    {
        let probe_path = probe_path.as_ref();
        let probe = fs::read_to_string(probe_path).map_err(|error| {
            format!(
                "failed to read package probe {}: {error}",
                probe_path.display()
            )
        })?;
        Ok(Self::new(name, packages, probe))
    }

    /// Build a package-agnostic probe of the post-restart autoload surface.
    ///
    /// This is the scalable baseline for a package corpus: it does not guess
    /// arguments or invoke arbitrary package commands. It inventories
    /// autoloaded functions/macros, custom variables, and emitted bytecode for
    /// the complete dependency graph. Curated probes can be added separately
    /// when meaningful behavior and inputs are known.
    pub fn autoload_surface<I, P>(name: impl Into<String>, packages: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<String>,
    {
        let packages = packages.into_iter().map(Into::into).collect::<Vec<_>>();
        let package_strings = packages
            .iter()
            .map(|package| elisp_string(package))
            .collect::<Vec<_>>()
            .join(" ");
        let probe = format!(
            r##"(let* ((requested
                         (mapcar #'intern '({package_strings})))
                       (libraries (make-hash-table :test 'equal))
                       (known-library-p
                        (lambda (library)
                          (and
                           (stringp library)
                           (or (gethash library libraries)
                               (gethash
                                (file-name-sans-extension library)
                                libraries)
                               (gethash
                                (file-name-base library)
                                libraries)))))
                       (autoloads nil)
                       (customs nil)
                       (bytecode nil))
                  (dolist (package requested)
                    (unless (package-installed-p package)
                      (error "requested package was not installed: %S" package)))
                  (dolist (entry package-alist)
                    (let* ((description (cadr entry))
                           (directory (package-desc-dir description))
                           (files
                            (and directory
                                 (file-directory-p directory)
                                 (directory-files-recursively
                                  directory "\\.elc?\\'")))
                           (compiled nil))
                      (dolist (file files)
                        (let* ((relative
                                (file-relative-name file directory))
                               (library
                                (file-name-sans-extension relative)))
                          (puthash library t libraries)
                          (puthash (file-name-base library) t libraries)
                          (when (string-suffix-p ".elc" relative)
                            (push relative compiled))))
                      (push
                       (list
                        (car entry)
                        (package-version-join
                         (package-desc-version description))
                        (sort compiled #'string<))
                       bytecode)))
                  (mapatoms
                   (lambda (symbol)
                     (let ((definition
                            (and (fboundp symbol)
                                 (symbol-function symbol))))
                       (when (and (autoloadp definition)
                                  (funcall known-library-p (nth 1 definition)))
                         (push
                          (list symbol
                                (nth 1 definition)
                                (if (eq (nth 4 definition) 'macro)
                                    'macro
                                  (if (nth 3 definition)
                                      'command
                                    'function)))
                          autoloads)))
                     (let ((custom-libraries nil))
                       (dolist (library (get symbol 'custom-loads))
                         (let ((library-name
                                (cond
                                 ((stringp library) library)
                                 ((symbolp library) (symbol-name library)))))
                           (when (and library-name
                                      (funcall known-library-p library-name))
                             (push library-name custom-libraries))))
                       (when custom-libraries
                         (push
                          (list symbol
                                (sort custom-libraries #'string<))
                          customs)))))
                  (list
                   :autoloads
                   (sort autoloads
                         (lambda (left right)
                           (string< (symbol-name (car left))
                                    (symbol-name (car right)))))
                   :customs
                   (sort customs
                         (lambda (left right)
                           (string< (symbol-name (car left))
                                    (symbol-name (car right)))))
                   :bytecode
                   (sort bytecode
                         (lambda (left right)
                           (string< (symbol-name (car left))
                                    (symbol-name (car right)))))))"##
        );
        Self::new(name, packages, probe)
    }

    fn package_names(&self) -> Vec<&str> {
        match &self.packages {
            PackageSelection::Unversioned(packages) => {
                packages.iter().map(String::as_str).collect()
            }
            PackageSelection::Versioned(packages) => packages
                .iter()
                .map(|package| package.name.as_str())
                .collect(),
        }
    }

    fn package_pins(&self) -> Option<&[PackagePin]> {
        match &self.packages {
            PackageSelection::Unversioned(_) => None,
            PackageSelection::Versioned(packages) => Some(packages),
        }
    }
}

/// One ERT selector loaded from an Emacs Lisp test file.
#[derive(Clone, Debug)]
pub struct ErtScenario {
    pub name: String,
    pub test_file: PathBuf,
    pub selector: String,
}

impl ErtScenario {
    pub fn new(
        name: impl Into<String>,
        test_file: impl Into<PathBuf>,
        selector: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            test_file: test_file.into(),
            selector: selector.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioPhase {
    Install,
    RestartProbe,
    QuickstartProbe,
    VcInstall,
    VcRestart,
    VcUpgrade,
    VcDelete,
    VcRestartAfterDelete,
    Ert,
}

#[derive(Debug)]
pub struct PhaseReport {
    pub phase: ScenarioPhase,
    pub duration: Duration,
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct ScenarioReport {
    pub runtime: String,
    pub scenario: String,
    pub phases: Vec<PhaseReport>,
    pub installed_packages: Vec<InstalledPackage>,
    pub outcome: EvalOutcome,
}

/// GNU Emacs and Neomacs reports after package lifecycle parity is verified.
#[derive(Debug)]
pub struct OracleScenarioReport {
    pub neomacs: ScenarioReport,
    pub gnu_emacs: ScenarioReport,
}

/// GNU Emacs and Neomacs outcomes for one direct Elisp form.
#[derive(Debug)]
pub struct ElispOracleReport {
    pub neomacs: EvalOutcome,
    pub gnu_emacs: EvalOutcome,
}

/// One named probe for [`CachedPackageOracle::run_batch`].
#[derive(Clone, Copy, Debug)]
pub struct OracleBatchCase<'a> {
    /// Stable case id (no `:` or whitespace). Used in failures and expect keys.
    pub id: &'a str,
    /// Elisp forms evaluated after shared package setup.
    pub probe: &'a str,
    /// Whether this case must return a value or signal an error.
    pub expected_outcome: ExpectedOutcome,
}

/// Differential outcomes for one case inside a multi-probe batch.
#[derive(Debug)]
pub struct OracleBatchCaseReport {
    pub id: String,
    pub neomacs: EvalOutcome,
    pub gnu_emacs: EvalOutcome,
}

/// The editor whose outcome violated a typed batch expectation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleEditor {
    GnuEmacs,
    Neomacs,
}

/// A behavioral failure for one case in an otherwise valid batch protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleBatchFailure {
    OutcomeMismatch {
        id: String,
        neomacs: EvalOutcome,
        gnu_emacs: EvalOutcome,
    },
    UnexpectedOutcome {
        id: String,
        editor: OracleEditor,
        expected: ExpectedOutcome,
        actual: EvalOutcome,
    },
}

impl OracleBatchFailure {
    pub fn id(&self) -> &str {
        match self {
            Self::OutcomeMismatch { id, .. } | Self::UnexpectedOutcome { id, .. } => id,
        }
    }
}

impl std::fmt::Display for OracleBatchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutcomeMismatch {
                id,
                neomacs,
                gnu_emacs,
            } => write!(
                formatter,
                "case `{id}` outcome mismatch:\n  Neomacs: {neomacs}\n  GNU Emacs: {gnu_emacs}"
            ),
            Self::UnexpectedOutcome {
                id,
                editor,
                expected,
                actual,
            } => {
                let editor = match editor {
                    OracleEditor::GnuEmacs => "GNU Emacs",
                    OracleEditor::Neomacs => "Neomacs",
                };
                let expected = match expected {
                    ExpectedOutcome::Value => "a value",
                    ExpectedOutcome::Signal => "a signal",
                };
                write!(
                    formatter,
                    "case `{id}` expected {editor} to return {expected}, got {actual}"
                )
            }
        }
    }
}

/// All case outcomes and behavioral failures from one valid batch execution.
#[derive(Debug)]
pub struct OracleBatchReport {
    pub cases: Vec<OracleBatchCaseReport>,
    pub failures: Vec<OracleBatchFailure>,
}

/// Differential oracle for one exact package cached below `./tmp`.
#[derive(Clone)]
pub struct CachedPackageOracle {
    packages: PreparedPackageSet,
    timeout: Duration,
}

/// MELPA-focused name retained for package-specific parity modules.
pub type CachedMelpaOracle = CachedPackageOracle;

impl CachedPackageOracle {
    /// Build an exact revision-pinned package from source and select its file.
    pub fn new(package: (&str, &str), source_file_name: &str) -> Result<Self, String> {
        Self::new_from_manifest_with_runtime(&EmacsRuntime::gnu_emacs(), package, source_file_name)
    }

    fn new_from_manifest_with_runtime(
        gnu_emacs: &EmacsRuntime,
        package: (&str, &str),
        source_file_name: &str,
    ) -> Result<Self, String> {
        validate_cached_source_file_name("source-built package", source_file_name)?;
        let packages = PreparedPackageSet::from_locked_melpa(gnu_emacs, package, source_file_name)?;
        Ok(Self {
            packages,
            timeout: DEFAULT_PROCESS_TIMEOUT,
        })
    }

    /// Prepare one pinned GNU ELPA package and select its Elisp source file.
    pub fn new_from_gnu_elpa(
        package: (&str, &str),
        source_file_name: &str,
    ) -> Result<Self, String> {
        validate_cached_source_file_name(GNU_ELPA_ARCHIVE.label, source_file_name)?;
        let package_dir = prepare_cached_gnu_elpa_package(&EmacsRuntime::gnu_emacs(), package)?;
        Self::from_package_dir(package, source_file_name, package_dir)
    }

    fn from_package_dir(
        package: (&str, &str),
        source_file_name: &str,
        package_dir: PathBuf,
    ) -> Result<Self, String> {
        Ok(Self {
            packages: PreparedPackageSet::from_package_dir(package, source_file_name, package_dir)?,
            timeout: DEFAULT_PROCESS_TIMEOUT,
        })
    }

    /// Evaluate an additional setup form before loading the package source.
    pub fn with_prelude(mut self, prelude: impl Into<String>) -> Self {
        self.packages = self.packages.with_prelude(prelude);
        self
    }

    /// Exercise the package state established by `package-initialize` without
    /// loading the selected source file afterward.
    pub fn with_installed_autoloads(mut self) -> Self {
        self.packages = self.packages.with_installed_autoloads();
        self
    }

    fn with_prepared_dependency(
        mut self,
        package: (&str, &str),
        package_dir: PathBuf,
    ) -> Result<Self, String> {
        self.packages = self
            .packages
            .with_prepared_dependency(package, package_dir)?;
        Ok(self)
    }

    /// Make another exact source-built package cache visible as a system-wide
    /// package directory while loading the package under test.
    pub fn with_melpa_dependency(self, package: (&str, &str)) -> Result<Self, String> {
        let package_dir = prepare_cached_locked_melpa_package(&EmacsRuntime::gnu_emacs(), package)?;
        self.with_prepared_dependency(package, package_dir)
    }

    /// Make another exact GNU ELPA package cache visible as a system-wide
    /// package directory while loading the package under test.
    pub fn with_gnu_elpa_dependency(self, package: (&str, &str)) -> Result<Self, String> {
        let package_dir = prepare_cached_gnu_elpa_package(&EmacsRuntime::gnu_emacs(), package)?;
        self.with_prepared_dependency(package, package_dir)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The immutable package setup shared by batch and interactive adapters.
    pub fn prepared_packages(&self) -> &PreparedPackageSet {
        &self.packages
    }

    /// Run one command-loop-sensitive probe directly in each editor.
    ///
    /// Unlike the ordinary `--eval` oracle, this adapter loads a script as a
    /// top-level command-line action. That leaves `recursive-edit` at a real
    /// command-loop boundary. Each editor owns a separate process tree and
    /// atomically publishes exactly one schema-tagged outcome file, so a
    /// timeout can reap the editor and every Git/tool child before its
    /// workspace-local sandbox is removed.
    pub fn run_direct_command_loop_probe(
        &self,
        name: &str,
        probe: &str,
        expected: ExpectedOutcome,
    ) -> Result<OracleBatchReport, String> {
        validate_batch_case_id(name)
            .map_err(|error| format!("invalid direct command-loop case id: {error}"))?;
        let gnu_emacs = self.configured_runtime(EmacsRuntime::gnu_emacs());
        let neomacs = self.configured_runtime(EmacsRuntime::neomacs());
        let setup = self.packages.startup_elisp();
        let (gnu_result, neomacs_result) = thread::scope(|scope| {
            let gnu_handle =
                scope.spawn(|| run_direct_editor_probe(&gnu_emacs, name, &setup, probe));
            let neomacs_handle =
                scope.spawn(|| run_direct_editor_probe(&neomacs, name, &setup, probe));
            (
                gnu_handle
                    .join()
                    .unwrap_or_else(|_| Err("GNU Emacs direct-probe thread panicked".into())),
                neomacs_handle
                    .join()
                    .unwrap_or_else(|_| Err("Neomacs direct-probe thread panicked".into())),
            )
        });
        let gnu_emacs =
            gnu_result.map_err(|error| format!("GNU Emacs direct probe failed: {error}"))?;
        let neomacs =
            neomacs_result.map_err(|error| format!("Neomacs direct probe failed: {error}"))?;
        Ok(oracle_batch_report(name, neomacs, gnu_emacs, expected))
    }

    fn configured_runtime(&self, mut runtime: EmacsRuntime) -> EmacsRuntime {
        for (name, value) in self.packages.process_environment() {
            runtime = runtime.with_env(name, value);
        }
        runtime.with_timeout(self.timeout)
    }

    /// Run a parity case that must complete with a value in both editors.
    pub fn run_value(&self, name: &str, probe: &str) -> Result<ElispOracleReport, String> {
        self.run_expected(name, probe, ExpectedOutcome::Value)
    }

    /// Run a parity case that must signal in both editors.
    pub fn run_signal(&self, name: &str, probe: &str) -> Result<ElispOracleReport, String> {
        self.run_expected(name, probe, ExpectedOutcome::Signal)
    }

    /// Run one probe with setup inside the outcome catcher and retain every
    /// behavioral failure as report data.
    pub fn run_case(
        &self,
        name: &str,
        probe: &str,
        expected: ExpectedOutcome,
    ) -> Result<OracleBatchReport, String> {
        let neomacs = self.configured_runtime(EmacsRuntime::neomacs());
        let gnu_emacs = self.configured_runtime(EmacsRuntime::gnu_emacs());
        let observed = run_elisp_oracle_case(
            &neomacs,
            &gnu_emacs,
            name,
            &self.packages.startup_elisp(),
            probe,
        )?;
        Ok(oracle_batch_report(
            name,
            observed.neomacs,
            observed.gnu_emacs,
            expected,
        ))
    }

    /// Run many named probes in one GNU Emacs process and one Neomacs process.
    ///
    /// Shared package setup (`package-initialize`, load, prelude) runs once per
    /// editor. Probes emit separate outcome markers; a signal in one probe does
    /// not stop later probes. GNU Emacs and Neomacs evaluations run in parallel.
    pub fn run_batch(
        &self,
        batch_name: &str,
        cases: &[OracleBatchCase<'_>],
    ) -> Result<OracleBatchReport, String> {
        if cases.is_empty() {
            return Err(format!(
                "{} batch `{batch_name}` requires at least one probe",
                self.packages.package_name()
            ));
        }
        let probes: Vec<BatchProbe<'_>> = cases
            .iter()
            .map(|case| BatchProbe {
                id: case.id,
                probe: case.probe,
            })
            .collect();
        let neomacs = self.configured_runtime(EmacsRuntime::neomacs());
        let gnu_emacs = self.configured_runtime(EmacsRuntime::gnu_emacs());
        let setup = self.packages.startup_elisp();
        let mut report = run_elisp_oracle_batch(&neomacs, &gnu_emacs, batch_name, &setup, &probes)?;
        for (case, observed) in cases.iter().zip(report.cases.iter()) {
            for (editor, actual) in [
                (OracleEditor::GnuEmacs, &observed.gnu_emacs),
                (OracleEditor::Neomacs, &observed.neomacs),
            ] {
                if !case.expected_outcome.matches(actual) {
                    report.failures.push(OracleBatchFailure::UnexpectedOutcome {
                        id: case.id.to_string(),
                        editor,
                        expected: case.expected_outcome,
                        actual: actual.clone(),
                    });
                }
            }
        }
        Ok(report)
    }

    fn run_expected(
        &self,
        name: &str,
        probe: &str,
        expected_outcome: ExpectedOutcome,
    ) -> Result<ElispOracleReport, String> {
        let mut report = self.run_case(name, probe, expected_outcome)?;
        if !report.failures.is_empty() {
            return Err(format!(
                "{} parity case `{name}` failed:\n{}",
                self.packages.package_name(),
                report
                    .failures
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        let report = report.cases.remove(0);
        Ok(ElispOracleReport {
            neomacs: report.neomacs,
            gnu_emacs: report.gnu_emacs,
        })
    }
}

fn oracle_batch_report(
    name: &str,
    neomacs: EvalOutcome,
    gnu_emacs: EvalOutcome,
    expected: ExpectedOutcome,
) -> OracleBatchReport {
    let mut failures = Vec::new();
    if neomacs != gnu_emacs {
        failures.push(OracleBatchFailure::OutcomeMismatch {
            id: name.to_string(),
            neomacs: neomacs.clone(),
            gnu_emacs: gnu_emacs.clone(),
        });
    }
    for (editor, actual) in [
        (OracleEditor::GnuEmacs, &gnu_emacs),
        (OracleEditor::Neomacs, &neomacs),
    ] {
        if !expected.matches(actual) {
            failures.push(OracleBatchFailure::UnexpectedOutcome {
                id: name.to_string(),
                editor,
                expected,
                actual: actual.clone(),
            });
        }
    }
    OracleBatchReport {
        cases: vec![OracleBatchCaseReport {
            id: name.to_string(),
            neomacs,
            gnu_emacs,
        }],
        failures,
    }
}

const DIRECT_PROBE_SCHEMA: &str = "neomacs-melpa-direct-v1";
const DIRECT_PROBE_MAX_BYTES: u64 = 2 * 1024 * 1024;
const DIRECT_PROBE_LOG_MAX_BYTES: u64 = 1024 * 1024;
const DIRECT_PROBE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const DIRECT_PROBE_TERM_GRACE: Duration = Duration::from_millis(500);

fn run_direct_editor_probe(
    runtime: &EmacsRuntime,
    name: &str,
    setup: &str,
    probe: &str,
) -> Result<EvalOutcome, String> {
    let sandbox = MelpaSandbox::new(&format!("{name}-direct-{}", runtime.name))?;
    let script_path = sandbox.root().join("direct-probe.el");
    let outcome_path = sandbox.root().join("direct-outcome.el");
    let outcome_tmp_path = sandbox.root().join("direct-outcome.el.partial");
    let stdout_path = sandbox.root().join("editor.stdout");
    let stderr_path = sandbox.root().join("editor.stderr");
    let script = direct_probe_script(name, setup, probe, &outcome_path, &outcome_tmp_path);
    fs::write(&script_path, script).map_err(|error| {
        format!(
            "failed to write direct probe script {}: {error}",
            script_path.display()
        )
    })?;

    let stdout = File::create(&stdout_path).map_err(|error| {
        format!(
            "failed to create direct probe stdout {}: {error}",
            stdout_path.display()
        )
    })?;
    let stderr = File::create(&stderr_path).map_err(|error| {
        format!(
            "failed to create direct probe stderr {}: {error}",
            stderr_path.display()
        )
    })?;
    let mut command = runtime.command();
    sandbox.configure(&mut command);
    command
        .arg("--quick")
        .arg("--batch")
        .arg("--load")
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = DirectEditorChild::spawn(&mut command).map_err(|error| {
        format!(
            "failed to launch {} for direct probe `{name}`: {error}",
            runtime.name
        )
    })?;
    let status = child.wait_for_exit(runtime.timeout).map_err(|error| {
        direct_probe_process_error(runtime, name, error, &stdout_path, &stderr_path)
    })?;

    if !status.success() {
        return Err(direct_probe_process_error(
            runtime,
            name,
            format!("exited with status {status}"),
            &stdout_path,
            &stderr_path,
        ));
    }
    let stdout = read_direct_probe_file(&stdout_path, DIRECT_PROBE_LOG_MAX_BYTES, "stdout")?;
    let stderr = read_direct_probe_file(&stderr_path, DIRECT_PROBE_LOG_MAX_BYTES, "stderr")?;
    let outcome = read_direct_probe_outcome(name, &outcome_path, &outcome_tmp_path).map_err(
        |error| {
        format!(
            "{} direct probe `{name}` emitted an invalid {DIRECT_PROBE_SCHEMA} outcome: {error}",
            runtime.name
        )
        },
    )?;
    Ok(wrap_direct_probe_logs(outcome, stdout, stderr, &sandbox))
}

fn direct_probe_script(
    name: &str,
    setup: &str,
    probe: &str,
    outcome_path: &Path,
    outcome_tmp_path: &Path,
) -> String {
    let outcome_path = elisp_string(&outcome_path.to_string_lossy());
    let outcome_tmp_path = elisp_string(&outcome_tmp_path.to_string_lossy());
    let case_id = elisp_string(name);
    let normalizer = oracle_normalizer_elisp();
    format!(
        r####";;; -*- lexical-binding: t; -*-
(progn
  {normalizer}
  {setup}
  (let ((direct-case-id {case_id})
        (direct-outcome-file {outcome_path})
        (direct-outcome-tmp {outcome_tmp_path})
        direct-kind
        direct-value)
    (condition-case direct-error
        (setq direct-kind "value"
              direct-value
              (neomacs--test-oracle-normalized (progn {probe})))
      (error
       (setq direct-kind "signal"
             direct-value
             (neomacs--test-oracle-normalized direct-error))))
    (let ((direct-payload
           (with-temp-buffer
        (let ((print-circle t)
              (print-length nil)
              (print-level nil)
              (print-escape-newlines t)
              (print-escape-control-characters t))
               (prin1 direct-value (current-buffer))
               (buffer-substring-no-properties (point-min) (point-max))))))
      (let* ((encoded direct-payload)
           (read-eval nil)
           (decoded (read-from-string encoded))
           (trailing (substring encoded (cdr decoded))))
        (unless (equal (car decoded) direct-value)
          (error "Direct payload failed its Elisp round-trip check"))
        (unless (string-match-p "\\`[[:space:]]*\\'" trailing)
          (error "Direct payload contains trailing protocol bytes")))
      (let* ((coding-system-for-write 'no-conversion)
             (newline (string-as-unibyte "\n"))
             (schema-bytes
              (encode-coding-string "{DIRECT_PROBE_SCHEMA}" 'utf-8-unix t))
             (case-bytes
              (encode-coding-string direct-case-id 'utf-8-unix t))
             (kind-bytes
              (encode-coding-string direct-kind 'utf-8-unix t))
             (payload-bytes
              (encode-coding-string direct-payload 'utf-8-unix t))
             (envelope
              (concat
               schema-bytes newline
               (string-as-unibyte (number-to-string (length case-bytes))) newline
               case-bytes newline
               kind-bytes newline
               (string-as-unibyte (number-to-string (length payload-bytes))) newline
               payload-bytes)))
        (with-temp-file direct-outcome-tmp
          (set-buffer-multibyte nil)
          (insert envelope))))
    (rename-file direct-outcome-tmp direct-outcome-file t)))
"####
    )
}

struct DirectEditorChild {
    child: Child,
    status: Option<ExitStatus>,
    #[cfg(unix)]
    process_group: i32,
    #[cfg(windows)]
    job: DirectWindowsJob,
    tree_armed: bool,
}

impl DirectEditorChild {
    fn spawn(command: &mut Command) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        let job = {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

            command.creation_flags(CREATE_SUSPENDED);
            DirectWindowsJob::new()?
        };
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to spawn editor: {error}"))?;
        #[cfg(unix)]
        {
            let child_id = child.id();
            let process_group = match i32::try_from(child_id) {
                Ok(process_group) => process_group,
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "editor pid {child_id} does not fit a process-group id"
                    ));
                }
            };
            if process_group <= 1 {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "refusing unsafe direct-probe process group {process_group}"
                ));
            }
            Ok(Self {
                child,
                status: None,
                process_group,
                tree_armed: true,
            })
        }
        #[cfg(windows)]
        {
            if let Err(error) = job.assign_and_resume(&child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            Ok(Self {
                child,
                status: None,
                job,
                tree_armed: true,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(Self {
                child,
                status: None,
                tree_armed: true,
            })
        }
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> Result<ExitStatus, String> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.poll_child() {
                Ok(Some(status)) => {
                    if self.process_tree_has_members() {
                        let cleanup = self.shutdown_process_tree();
                        return Err(append_cleanup_error(
                            "exited while descendant processes were still live; the adapter reaped the process tree",
                            cleanup,
                        ));
                    }
                    self.tree_armed = false;
                    return Ok(status);
                }
                Ok(None) => {}
                Err(error) => {
                    let cleanup = self.shutdown_process_tree();
                    return Err(append_cleanup_error(error, cleanup));
                }
            }
            if Instant::now() >= deadline {
                let cleanup = self.shutdown_process_tree();
                return Err(append_cleanup_error(
                    format!("timed out after {timeout:?}"),
                    cleanup,
                ));
            }
            thread::sleep(DIRECT_PROBE_POLL_INTERVAL);
        }
    }

    fn poll_child(&mut self) -> Result<Option<ExitStatus>, String> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let status = self
            .child
            .try_wait()
            .map_err(|error| format!("failed while waiting for editor: {error}"))?;
        if let Some(status) = status {
            self.status = Some(status);
        }
        Ok(status)
    }

    fn shutdown_process_tree(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if let Err(error) = self.request_process_tree_stop() {
            errors.push(error);
        }
        #[cfg(not(any(unix, windows)))]
        if let Err(error) = self.child.kill() {
            errors.push(format!("failed to terminate editor: {error}"));
        }

        if self.wait_until_process_tree_empty(DIRECT_PROBE_TERM_GRACE, &mut errors) {
            self.tree_armed = false;
            return errors_if_any(errors);
        }

        if let Err(error) = self.force_process_tree_stop() {
            errors.push(error);
        }
        #[cfg(not(any(unix, windows)))]
        if let Err(error) = self.child.kill() {
            errors.push(format!("failed to kill editor: {error}"));
        }
        if self.status.is_none() {
            match self.child.wait() {
                Ok(status) => self.status = Some(status),
                Err(error) => errors.push(format!("failed to reap killed editor: {error}")),
            }
        }
        if self.wait_until_process_tree_empty(DIRECT_PROBE_TERM_GRACE, &mut errors) {
            self.tree_armed = false;
        } else {
            errors.push("direct-probe process tree remained live after forced termination".into());
        }
        errors_if_any(errors)
    }

    fn wait_until_process_tree_empty(&mut self, grace: Duration, errors: &mut Vec<String>) -> bool {
        let deadline = Instant::now() + grace;
        loop {
            if self.status.is_none() {
                match self.child.try_wait() {
                    Ok(Some(status)) => self.status = Some(status),
                    Ok(None) => {}
                    Err(error) => errors.push(format!("failed while reaping editor: {error}")),
                }
            }
            let child_reaped = self.status.is_some();
            if child_reaped && !self.process_tree_has_members() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(DIRECT_PROBE_POLL_INTERVAL);
        }
    }

    fn process_tree_has_members(&self) -> bool {
        #[cfg(unix)]
        {
            if !self.tree_armed {
                return false;
            }
            // Signal 0 probes group existence without delivering a signal.
            let result = unsafe { libc::kill(-self.process_group, 0) };
            result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        }
        #[cfg(windows)]
        {
            // The Job handle remains queryable after cleanup is disarmed, so
            // Windows runtime contracts can prove that ActiveProcesses is
            // actually zero rather than observing only adapter bookkeeping.
            self.job.has_members().unwrap_or(true)
        }
        #[cfg(not(any(unix, windows)))]
        self.status.is_none()
    }

    #[cfg(all(test, windows))]
    fn active_windows_job_processes(&self) -> Result<u32, String> {
        self.job.active_processes()
    }

    #[cfg(unix)]
    fn signal_process_group(&self, signal: i32) -> Result<(), String> {
        if !self.tree_armed {
            return Ok(());
        }
        // The child was spawned with process_group(0), so its pid is the
        // exact, positively validated group id. A negative kill target is
        // therefore scoped to this adapter-owned editor/tool process tree.
        let result = unsafe { libc::kill(-self.process_group, signal) };
        if result == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(format!(
                "failed to signal direct-probe process group {} with {signal}: {error}",
                self.process_group
            ))
        }
    }

    #[cfg(unix)]
    fn request_process_tree_stop(&self) -> Result<(), String> {
        self.signal_process_group(libc::SIGTERM)
    }

    #[cfg(windows)]
    fn request_process_tree_stop(&self) -> Result<(), String> {
        self.job.terminate()
    }

    #[cfg(not(any(unix, windows)))]
    fn request_process_tree_stop(&self) -> Result<(), String> {
        Ok(())
    }

    #[cfg(unix)]
    fn force_process_tree_stop(&self) -> Result<(), String> {
        self.signal_process_group(libc::SIGKILL)
    }

    #[cfg(windows)]
    fn force_process_tree_stop(&self) -> Result<(), String> {
        self.job.terminate()
    }

    #[cfg(not(any(unix, windows)))]
    fn force_process_tree_stop(&self) -> Result<(), String> {
        Ok(())
    }
}

impl Drop for DirectEditorChild {
    fn drop(&mut self) {
        if self.tree_armed {
            let _ = self.force_process_tree_stop();
        }
        if self.status.is_none() {
            let _ = self.child.kill();
            if let Ok(status) = self.child.wait() {
                self.status = Some(status);
            }
        }
        if self.tree_armed {
            let mut ignored_errors = Vec::new();
            if self.wait_until_process_tree_empty(DIRECT_PROBE_TERM_GRACE, &mut ignored_errors) {
                self.tree_armed = false;
            }
        }
    }
}

#[cfg(windows)]
struct DirectWindowsJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl DirectWindowsJob {
    fn new() -> Result<Self, String> {
        use std::mem::{size_of, zeroed};
        use std::ptr;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        // SAFETY: null attributes/name request a private job with default
        // security. The returned owned handle is closed by Drop.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(format!(
                "failed to create direct-probe Job Object: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: this Windows ABI struct is plain data and accepts an
        // all-zero initial state before its documented limit flag is set.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .expect("Windows Job Object limit structure size fits u32");
        // SAFETY: HANDLE is live, pointer/size describe LIMITS for the exact
        // requested information class, and the call does not retain it.
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size,
            )
        };
        if configured == 0 {
            let error = std::io::Error::last_os_error();
            // SAFETY: HANDLE was created above and has not been closed.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err(format!(
                "failed to configure direct-probe Job Object: {error}"
            ));
        }
        Ok(Self { handle })
    }

    fn assign_and_resume(&self, child: &Child) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let process_handle = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
        // SAFETY: both handles are live; the suspended process cannot create
        // descendants before it becomes owned by the kill-on-close job.
        if unsafe { AssignProcessToJobObject(self.handle, process_handle) } == 0 {
            return Err(format!(
                "failed to assign suspended editor to direct-probe Job Object: {}",
                std::io::Error::last_os_error()
            ));
        }
        resume_windows_process_primary_thread(child.id())
    }

    fn terminate(&self) -> Result<(), String> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        // SAFETY: HANDLE remains owned by SELF for the entire call.
        if unsafe { TerminateJobObject(self.handle, 1) } == 0 {
            return Err(format!(
                "failed to terminate direct-probe Job Object: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn active_processes(&self) -> Result<u32, String> {
        use std::mem::{size_of, zeroed};
        use std::ptr;
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            QueryInformationJobObject,
        };

        // SAFETY: this accounting structure is plain output data.
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let size = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .expect("Windows Job Object accounting structure size fits u32");
        // SAFETY: HANDLE is live, output pointer/size match the requested
        // information class, and no return-length storage is required.
        if unsafe {
            QueryInformationJobObject(
                self.handle,
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast(),
                size,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(format!(
                "failed to query direct-probe Job Object: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(accounting.ActiveProcesses)
    }

    fn has_members(&self) -> Result<bool, String> {
        self.active_processes().map(|active| active != 0)
    }
}

#[cfg(windows)]
impl Drop for DirectWindowsJob {
    fn drop(&mut self) {
        // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE makes this the final fail-safe
        // for every editor/tool descendant still assigned to the job.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(windows)]
fn resume_windows_process_primary_thread(process_id: u32) -> Result<(), String> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    // SAFETY: snapshot has no input pointers and returns an owned handle.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(format!(
            "failed to enumerate suspended editor threads: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: THREADENTRY32 is an output-only C structure whose dwSize field
    // is initialized to the documented structure size before enumeration.
    let mut entry: THREADENTRY32 = unsafe { zeroed() };
    entry.dwSize = u32::try_from(size_of::<THREADENTRY32>())
        .expect("Windows thread-entry structure size fits u32");
    // SAFETY: SNAPSHOT and ENTRY are live for the call.
    let mut has_entry = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
    let mut result = Err(format!(
        "suspended editor process {process_id} had no enumerable primary thread"
    ));
    while has_entry {
        if entry.th32OwnerProcessID == process_id {
            // SAFETY: thread id came from the live system snapshot; the
            // returned owned handle is closed below.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                result = Err(format!(
                    "failed to open suspended editor thread {}: {}",
                    entry.th32ThreadID,
                    std::io::Error::last_os_error()
                ));
            } else {
                // SAFETY: THREAD is live and was opened with suspend/resume access.
                let previous_count = unsafe { ResumeThread(thread) };
                // SAFETY: THREAD is owned by this function and no longer used.
                unsafe { CloseHandle(thread) };
                result = match previous_count {
                    1 => Ok(()),
                    u32::MAX => Err(format!(
                        "failed to release suspended editor startup gate: {}",
                        std::io::Error::last_os_error()
                    )),
                    unexpected => Err(format!(
                        "suspended editor startup gate had suspension count {unexpected}, expected exactly 1"
                    )),
                };
            }
            break;
        }
        // SAFETY: SNAPSHOT and ENTRY remain live for the call.
        has_entry = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: SNAPSHOT is owned by this function and no longer used.
    unsafe { CloseHandle(snapshot) };
    result
}

fn append_cleanup_error(reason: impl fmt::Display, cleanup: Result<(), String>) -> String {
    match cleanup {
        Ok(()) => reason.to_string(),
        Err(error) => format!("{reason}; process-tree cleanup also failed: {error}"),
    }
}

fn errors_if_any(errors: Vec<String>) -> Result<(), String> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn direct_probe_process_error(
    runtime: &EmacsRuntime,
    name: &str,
    reason: impl fmt::Display,
    stdout_path: &Path,
    stderr_path: &Path,
) -> String {
    let stdout = read_direct_probe_file(stdout_path, DIRECT_PROBE_LOG_MAX_BYTES, "stdout")
        .unwrap_or_else(|error| format!("<unavailable: {error}>"));
    let stderr = read_direct_probe_file(stderr_path, DIRECT_PROBE_LOG_MAX_BYTES, "stderr")
        .unwrap_or_else(|error| format!("<unavailable: {error}>"));
    format!(
        "{} direct probe `{name}` {reason}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        runtime.name
    )
}

fn read_direct_probe_file(path: &Path, limit: u64, label: &str) -> Result<String, String> {
    let bytes = read_direct_probe_bytes(path, limit, label)?;
    String::from_utf8(bytes)
        .map_err(|error| format!("failed to read UTF-8 {label} {}: {error}", path.display()))
}

fn read_direct_probe_bytes(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {label} {}: {error}", path.display()))?;
    if metadata.len() > limit {
        return Err(format!(
            "{label} {} is {} bytes, exceeding the {limit}-byte protocol limit",
            path.display(),
            metadata.len()
        ));
    }
    fs::read(path).map_err(|error| format!("failed to read {label} {}: {error}", path.display()))
}

fn read_direct_probe_outcome(
    name: &str,
    outcome_path: &Path,
    outcome_tmp_path: &Path,
) -> Result<EvalOutcome, String> {
    if outcome_tmp_path.exists() {
        return Err(format!(
            "incomplete atomic outcome remains at {}",
            outcome_tmp_path.display()
        ));
    }
    let encoded = read_direct_probe_bytes(outcome_path, DIRECT_PROBE_MAX_BYTES, "outcome")?;
    parse_direct_probe_outcome(name, &encoded)
}

fn parse_direct_probe_outcome(name: &str, encoded: &[u8]) -> Result<EvalOutcome, String> {
    struct EnvelopeReader<'a> {
        remaining: &'a [u8],
    }

    impl<'a> EnvelopeReader<'a> {
        fn line(&mut self, label: &str) -> Result<&'a [u8], String> {
            let newline = self
                .remaining
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or_else(|| format!("{label} line is incomplete"))?;
            let (line, rest) = self.remaining.split_at(newline);
            self.remaining = &rest[1..];
            Ok(line)
        }

        fn length(&mut self, label: &str) -> Result<usize, String> {
            let line = self.line(label)?;
            let text = std::str::from_utf8(line)
                .map_err(|error| format!("{label} length is not UTF-8: {error}"))?;
            let length = text
                .parse::<usize>()
                .map_err(|error| format!("{label} length `{text}` is invalid: {error}"))?;
            if text != length.to_string() {
                return Err(format!("{label} length `{text}` is not canonical decimal"));
            }
            Ok(length)
        }

        fn bytes(&mut self, length: usize, label: &str) -> Result<&'a [u8], String> {
            if self.remaining.len() < length {
                return Err(format!(
                    "{label} declares {length} bytes but only {} remain",
                    self.remaining.len()
                ));
            }
            let (bytes, rest) = self.remaining.split_at(length);
            self.remaining = rest;
            Ok(bytes)
        }

        fn separator(&mut self, label: &str) -> Result<(), String> {
            if self.remaining.first() != Some(&b'\n') {
                return Err(format!("{label} is not followed by a newline separator"));
            }
            self.remaining = &self.remaining[1..];
            Ok(())
        }
    }

    let mut reader = EnvelopeReader { remaining: encoded };
    let schema = std::str::from_utf8(reader.line("schema")?)
        .map_err(|error| format!("schema is not UTF-8: {error}"))?;
    if schema != DIRECT_PROBE_SCHEMA {
        return Err(format!(
            "expected schema `{DIRECT_PROBE_SCHEMA}`, got `{schema}`"
        ));
    }
    let case_length = reader.length("case id")?;
    let case_id = std::str::from_utf8(reader.bytes(case_length, "case id")?)
        .map_err(|error| format!("case id is not UTF-8: {error}"))?;
    if case_id != name {
        return Err(format!("expected case id `{name}`, got `{case_id}`"));
    }
    reader.separator("case id")?;
    let kind = std::str::from_utf8(reader.line("outcome kind")?)
        .map_err(|error| format!("outcome kind is not UTF-8: {error}"))?;
    let payload_length = reader.length("payload")?;
    let payload = std::str::from_utf8(reader.bytes(payload_length, "payload")?)
        .map_err(|error| format!("payload is not UTF-8: {error}"))?;
    if !reader.remaining.is_empty() {
        return Err(format!(
            "outcome has {} trailing bytes after its payload",
            reader.remaining.len()
        ));
    }
    match kind {
        "value" => Ok(EvalOutcome::Value(payload.to_string())),
        "signal" => Ok(EvalOutcome::Signal(payload.to_string())),
        _ => Err(format!("unknown outcome kind `{kind}`")),
    }
}

fn wrap_direct_probe_logs(
    outcome: EvalOutcome,
    stdout: String,
    stderr: String,
    sandbox: &MelpaSandbox,
) -> EvalOutcome {
    fn normalize_known_paths(value: String, sandbox: &MelpaSandbox) -> String {
        value
            .replace(
                &sandbox.root().to_string_lossy().into_owned(),
                "[ORACLE-SANDBOX]",
            )
            .replace(
                &workspace_root().to_string_lossy().into_owned(),
                "[ORACLE-WORKSPACE]",
            )
    }

    fn normalize_empty_terminal_noise(value: String) -> String {
        if value
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        {
            String::new()
        } else {
            value
        }
    }

    let stdout = direct_log_elisp_string(&normalize_empty_terminal_noise(normalize_known_paths(
        stdout, sandbox,
    )));
    let stderr = direct_log_elisp_string(&normalize_empty_terminal_noise(normalize_known_paths(
        stderr, sandbox,
    )));
    match outcome {
        EvalOutcome::Value(value) => EvalOutcome::Value(format!(
            "(:value {value} :stdout {stdout} :stderr {stderr})"
        )),
        EvalOutcome::Signal(signal) => EvalOutcome::Signal(format!(
            "(:signal {signal} :stdout {stdout} :stderr {stderr})"
        )),
    }
}

/// Serialize a UTF-8 editor log as the readable string syntax produced by
/// GNU Emacs with `print-escape-newlines' and
/// `print-escape-control-characters' enabled.
fn direct_log_elisp_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\n' => encoded.push_str("\\n"),
            '\u{c}' => encoded.push_str("\\f"),
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\0'..='\u{1f}' | '\u{7f}' => {
                let code = u32::from(character);
                let next_is_octal = characters
                    .peek()
                    .is_some_and(|next| matches!(next, '0'..='7'));
                let width = if code > 0o77 || next_is_octal {
                    3
                } else if code > 0o7 {
                    2
                } else {
                    1
                };
                encoded.push('\\');
                encoded.push_str(&format!("{code:0width$o}"));
            }
            _ => encoded.push(character),
        }
    }
    encoded.push('"');
    encoded
}

fn validate_cached_source_file_name(
    archive_label: &str,
    source_file_name: &str,
) -> Result<(), String> {
    let mut components = Path::new(source_file_name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(format!(
            "cached {archive_label} source must be one file name, got `{source_file_name}`"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErtSummary {
    pub total: usize,
    pub expected: usize,
    pub unexpected: usize,
    pub skipped: usize,
}

#[derive(Debug)]
pub struct ErtReport {
    pub runtime: String,
    pub scenario: String,
    pub phase: PhaseReport,
    pub summary: ErtSummary,
}

#[derive(Debug)]
pub struct PackageVcReport {
    pub runtime: String,
    pub phases: Vec<PhaseReport>,
    pub checkpoints: Vec<String>,
}

struct PackageVcProgress {
    phases: Vec<PhaseReport>,
    checkpoints: Vec<String>,
}

impl PackageVcProgress {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            phases: Vec::with_capacity(capacity),
            checkpoints: Vec::with_capacity(capacity),
        }
    }
}

impl fmt::Display for ScenarioReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{} scenario `{}` installed: {}",
            self.runtime,
            self.scenario,
            format_installed_packages(&self.installed_packages)
        )?;
        for phase in &self.phases {
            writeln!(
                formatter,
                "{:?}: status {:?}, {:.2?}",
                phase.phase, phase.status_code, phase.duration
            )?;
        }
        write!(formatter, "outcome: {}", self.outcome)
    }
}

impl fmt::Display for ErtReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ERT scenario `{}`: {} total, {} expected, {} unexpected, {} skipped ({:.2?})",
            self.runtime,
            self.scenario,
            self.summary.total,
            self.summary.expected,
            self.summary.unexpected,
            self.summary.skipped,
            self.phase.duration
        )
    }
}

impl fmt::Display for PackageVcReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "{} package-vc lifecycle: {}",
            self.runtime,
            self.checkpoints.join(" -> ")
        )?;
        for phase in &self.phases {
            writeln!(
                formatter,
                "{:?}: status {:?}, {:.2?}",
                phase.phase, phase.status_code, phase.duration
            )?;
        }
        Ok(())
    }
}

/// Load an Emacs Lisp test file and run one ERT selector inside an isolated
/// editor process.
pub fn run_ert_scenario(
    runtime: &EmacsRuntime,
    scenario: &ErtScenario,
) -> Result<ErtReport, String> {
    if !scenario.test_file.is_file() {
        return Err(format!(
            "ERT scenario `{}` test file does not exist: {}",
            scenario.name,
            scenario.test_file.display()
        ));
    }

    let sandbox = MelpaSandbox::new(&scenario.name)?;
    let load_directory = scenario
        .test_file
        .parent()
        .expect("ERT test files have a parent directory");
    let eval = format!(r##"(ert-run-tests-batch {})"##, scenario.selector);
    let mut command = runtime.command();
    sandbox.configure(&mut command);
    command
        .env("NEOMACS_RUNTIME_ROOT", workspace_root())
        .args(["--batch", "--quick", "-L"])
        .arg(load_directory)
        .arg("-l")
        .arg(&scenario.test_file)
        .args(["--eval", &eval]);

    let started = Instant::now();
    let output = output_with_timeout(&mut command, runtime.timeout).map_err(|error| {
        command_error_message(error, runtime, &sandbox, &scenario.name, ScenarioPhase::Ert)
    })?;
    let phase = phase_report(ScenarioPhase::Ert, started.elapsed(), output);
    if phase.status_code != Some(0) {
        return Err(format!(
            "{} ERT scenario `{}` failed (exit {:?})\nstdout:\n{}\nstderr:\n{}",
            runtime.name, scenario.name, phase.status_code, phase.stdout, phase.stderr
        ));
    }
    let summary = extract_ert_summary(&phase.stdout, &phase.stderr).ok_or_else(|| {
        format!(
            "{} ERT scenario `{}` did not emit an ERT summary\nstdout:\n{}\nstderr:\n{}",
            runtime.name, scenario.name, phase.stdout, phase.stderr
        )
    })?;
    if summary.unexpected != 0 {
        return Err(format!(
            "{} ERT scenario `{}` reported {} unexpected result(s)\nstdout:\n{}\nstderr:\n{}",
            runtime.name, scenario.name, summary.unexpected, phase.stdout, phase.stderr
        ));
    }

    Ok(ErtReport {
        runtime: runtime.name.clone(),
        scenario: scenario.name.clone(),
        phase,
        summary,
    })
}

/// Install a scenario's packages, exit the editor, and probe them in a fresh
/// process using the same isolated home.
pub fn run_scenario(
    runtime: &EmacsRuntime,
    source: &PackageSource,
    scenario: &PackageScenario,
) -> Result<ScenarioReport, String> {
    run_install_and_probe(
        runtime,
        scenario,
        install_form(source, &scenario.package_names(), ""),
        probe_form(&scenario.probe),
        ScenarioPhase::RestartProbe,
    )
}

/// Install one exact GNU ELPA package into a validated, cross-process cache.
///
/// Like the MELPA cache, this remains a workspace-local runtime artifact.
pub fn prepare_cached_gnu_elpa_package(
    gnu_emacs: &EmacsRuntime,
    package: (&str, &str),
) -> Result<PathBuf, String> {
    prepare_cached_package(gnu_emacs, package, GNU_ELPA_ARCHIVE)
}

fn prepare_cached_package(
    gnu_emacs: &EmacsRuntime,
    package: (&str, &str),
    archive: PackageArchiveSpec,
) -> Result<PathBuf, String> {
    let (name, version) = package;
    if name.is_empty()
        || version.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '@'))
        || !version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
    {
        return Err(format!(
            "cached {} package must have a safe hard-coded name and version, got `{name}` `{version}`",
            archive.label
        ));
    }

    let root = workspace_root()
        .join("tmp/melpa")
        .join(archive.cache_directory)
        .join(name)
        .join(version);
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create package cache root {}: {error}",
            root.display()
        )
    })?;
    let lock_path = root.join("prepare.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "failed to open package cache lock {}: {error}",
                lock_path.display()
            )
        })?;
    fs4::FileExt::lock(&lock)
        .map_err(|error| format!("failed to lock package cache {}: {error}", root.display()))?;

    let home = root.join("home");
    let tmp = root.join("tmp");
    let package_dir = home.join(".emacs.d/elpa").join(format!("{name}-{version}"));
    let descriptor = package_dir.join(format!("{name}-pkg.el"));
    let ready_marker = root.join("ready");
    let failed_marker = root.join("failed");
    let expected_marker = format!("{name}\t{version}\n");
    let cache_is_ready = descriptor.is_file()
        && fs::read_to_string(&ready_marker).is_ok_and(|contents| contents == expected_marker);
    if cache_is_ready {
        return Ok(package_dir);
    }
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{expected_marker}error\n",
        package_preparation_run_id()
    );
    if let Ok(contents) = fs::read_to_string(&failed_marker)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }

    if home.exists() {
        fs::remove_dir_all(&home).map_err(|error| {
            format!(
                "failed to remove incomplete package cache {}: {error}",
                home.display()
            )
        })?;
    }
    if ready_marker.exists() {
        fs::remove_file(&ready_marker).map_err(|error| {
            format!(
                "failed to remove invalid package cache marker {}: {error}",
                ready_marker.display()
            )
        })?;
    }
    if failed_marker.exists() {
        fs::remove_file(&failed_marker).map_err(|error| {
            format!(
                "failed to remove stale package preparation failure {}: {error}",
                failed_marker.display()
            )
        })?;
    }
    for directory in [
        home.join(".emacs.d"),
        tmp.clone(),
        root.join("xdg/config"),
        root.join("xdg/cache"),
        root.join("xdg/data"),
        root.join("xdg/state"),
    ] {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create package cache directory {}: {error}",
                directory.display()
            )
        })?;
    }

    let name_string = elisp_string(name);
    let version_string = elisp_string(version);
    let archive_name_string = elisp_string(archive.name);
    let archive_url_string = elisp_string(archive.url);
    let package_archives = format!(
        r##"(list
                      (cons {archive_name_string}
                            {archive_url_string}))"##
    );
    let form = format!(
        r##"(progn
               (require 'package)
               (setq package-user-dir
                     (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                     package-check-signature nil
                     package-archives {package_archives})
               (package-refresh-contents)
               (let* ((package-name {name_string})
                      (expected-version {version_string})
                      (package-symbol (intern package-name))
                      (description
                       (cadr
                        (assq package-symbol package-archive-contents)))
                      (archive-version
                       (and description
                            (package-version-join
                             (package-desc-version description)))))
                 (unless description
                   (error "Package is absent from selected archive: %s"
                          package-name))
                 (unless (equal archive-version expected-version)
                   (error
                    "Package version changed: %s expected %s, current %s"
                    package-name expected-version archive-version))
                 (package-install description)
                 (package-initialize)
                 (let* ((installed
                         (cadr (assq package-symbol package-alist)))
                        (installed-version
                         (and installed
                              (package-version-join
                               (package-desc-version installed))))
                        (directory
                         (and installed (package-desc-dir installed)))
                        (descriptor
                         (and directory
                              (expand-file-name
                               (concat package-name "-pkg.el")
                               directory))))
                   (unless (equal installed-version expected-version)
                     (error
                      "Installed package version mismatch: %s expected %s, got %s"
                      package-name expected-version installed-version))
                   (unless (and descriptor (file-readable-p descriptor))
                     (error
                      "Installed package descriptor is unreadable: %s"
                      descriptor))))
               (princ "NEOMACS-PACKAGE-CACHE:ready"))"##
    );
    let mut command = gnu_emacs.command();
    configure_process_environment(&mut command, &root, &home, &tmp);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output = match output_with_timeout(&mut command, gnu_emacs.timeout) {
        Ok(output) => output,
        Err(error) => {
            let error = match error {
                CommandError::Launch(error) => format!(
                    "failed to launch {} for cached package `{name}` in {}: {error}",
                    gnu_emacs.name,
                    root.display()
                ),
                CommandError::TimedOut(_) => format!(
                    "{} cached package `{name}` timed out after {:?} in {}",
                    gnu_emacs.name,
                    gnu_emacs.timeout,
                    root.display()
                ),
                CommandError::Capture(error) => format!(
                    "failed to capture {} cached package `{name}` output: {error}",
                    gnu_emacs.name
                ),
            };
            return Err(publish_package_preparation_failure(
                &failed_marker,
                &failure_prefix,
                error,
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || !stdout.contains("NEOMACS-PACKAGE-CACHE:ready")
        || !descriptor.is_file()
    {
        let error = format!(
            "failed to prepare cached {} package {name} {version} below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            archive.label,
            root.display(),
            output.status.code()
        );
        return Err(publish_package_preparation_failure(
            &failed_marker,
            &failure_prefix,
            error,
        ));
    }

    let marker_tmp = root.join(format!("ready.{}.tmp", std::process::id()));
    fs::write(&marker_tmp, &expected_marker).map_err(|error| {
        format!(
            "failed to write package cache marker {}: {error}",
            marker_tmp.display()
        )
    })?;
    fs::rename(&marker_tmp, &ready_marker).map_err(|error| {
        format!(
            "failed to publish package cache marker {}: {error}",
            ready_marker.display()
        )
    })?;
    Ok(package_dir)
}

/// Run the same package lifecycle and probe against GNU Emacs and Neomacs.
///
/// The editors receive separate homes but the same package source and probe.
/// Package/version graph differences and normalized value/signal differences
/// are both oracle failures.
pub fn run_oracle_scenario(
    neomacs: &EmacsRuntime,
    gnu_emacs: &EmacsRuntime,
    source: &PackageSource,
    scenario: &PackageScenario,
) -> Result<OracleScenarioReport, String> {
    let gnu_report = run_scenario(gnu_emacs, source, scenario)
        .map_err(|error| format!("GNU Emacs baseline failed: {error}"))?;
    let neomacs_report = run_scenario(neomacs, source, scenario)
        .map_err(|error| format!("Neomacs comparison failed: {error}"))?;

    if neomacs_report.installed_packages != gnu_report.installed_packages {
        return Err(format!(
            "package graph mismatch for scenario `{}`\n  Neomacs: {}\n  GNU Emacs: {}",
            scenario.name,
            format_installed_packages(&neomacs_report.installed_packages),
            format_installed_packages(&gnu_report.installed_packages)
        ));
    }
    if neomacs_report.outcome != gnu_report.outcome {
        return Err(format!(
            "oracle outcome mismatch for scenario `{}`\n  Neomacs: {}\n  GNU Emacs: {}",
            scenario.name, neomacs_report.outcome, gnu_report.outcome
        ));
    }

    Ok(OracleScenarioReport {
        neomacs: neomacs_report,
        gnu_emacs: gnu_report,
    })
}

/// Run the same setup and Elisp form in isolated GNU Emacs and Neomacs
/// processes without installing a package.
///
/// This is useful for dense behavioral corpora that load one previously
/// prepared package source while the package lifecycle remains covered by a
/// separate scenario.
pub fn run_elisp_oracle(
    neomacs: &EmacsRuntime,
    gnu_emacs: &EmacsRuntime,
    name: &str,
    setup: &str,
    probe: &str,
) -> Result<ElispOracleReport, String> {
    let report = run_elisp_oracle_case(neomacs, gnu_emacs, name, setup, probe)?;
    if report.neomacs != report.gnu_emacs {
        return Err(format!(
            "oracle outcome mismatch for direct form `{name}`\n  Neomacs: {}\n  GNU Emacs: {}",
            report.neomacs, report.gnu_emacs
        ));
    }
    Ok(report)
}

fn run_elisp_oracle_case(
    neomacs: &EmacsRuntime,
    gnu_emacs: &EmacsRuntime,
    name: &str,
    setup: &str,
    probe: &str,
) -> Result<ElispOracleReport, String> {
    fn evaluate(
        runtime: &EmacsRuntime,
        name: &str,
        setup: &str,
        probe: &str,
    ) -> Result<EvalOutcome, String> {
        let sandbox = MelpaSandbox::new(name)?;
        let form = wrap_elisp_outcome(setup, probe, OUTCOME_MARKER);
        let phase = run_outcome_phase(runtime, &sandbox, name, ScenarioPhase::RestartProbe, &form)?;
        extract_marked_outcome(&phase.stderr, OUTCOME_MARKER).map_err(|error| {
            format!(
                "{} direct oracle `{name}` emitted an invalid outcome: {error}\nstdout:\n{}\nstderr:\n{}",
                runtime.name, phase.stdout, phase.stderr
            )
        })
    }

    let gnu_outcome = evaluate(gnu_emacs, name, setup, probe)
        .map_err(|error| format!("GNU Emacs baseline failed: {error}"))?;
    let neomacs_outcome = evaluate(neomacs, name, setup, probe)
        .map_err(|error| format!("Neomacs comparison failed: {error}"))?;
    Ok(ElispOracleReport {
        neomacs: neomacs_outcome,
        gnu_emacs: gnu_outcome,
    })
}

/// Run the same setup and many named probes in one process per editor.
///
/// GNU Emacs and Neomacs evaluations run concurrently. Each probe id must
/// appear exactly once in both editors' ordered debugging-output protocol,
/// and the outcomes must match pairwise.
pub fn run_elisp_oracle_batch(
    neomacs: &EmacsRuntime,
    gnu_emacs: &EmacsRuntime,
    batch_name: &str,
    setup: &str,
    cases: &[BatchProbe<'_>],
) -> Result<OracleBatchReport, String> {
    fn evaluate_batch(
        runtime: &EmacsRuntime,
        batch_name: &str,
        setup: &str,
        cases: &[BatchProbe<'_>],
    ) -> Result<Vec<(String, EvalOutcome)>, String> {
        let sandbox = MelpaSandbox::new(batch_name)?;
        let form = wrap_elisp_batch_outcomes(
            setup,
            cases,
            BATCH_BEGIN_MARKER,
            BATCH_COMPLETE_MARKER,
            OUTCOME_MARKER,
        )?;
        let phase = run_outcome_phase(
            runtime,
            &sandbox,
            batch_name,
            ScenarioPhase::RestartProbe,
            &form,
        )?;
        let expected_ids: Vec<&str> = cases.iter().map(|case| case.id).collect();
        let protocol = extract_marked_batch_protocol(
            &phase.stderr,
            BATCH_BEGIN_MARKER,
            OUTCOME_MARKER,
            BATCH_COMPLETE_MARKER,
        )
        .map_err(|error| {
            format!(
                "{} batch oracle `{batch_name}` emitted invalid protocol records: {error}\nstdout:\n{}\nstderr:\n{}",
                runtime.name, phase.stdout, phase.stderr
            )
        })?;
        let got_case_ids: Vec<&str> = protocol.case_ids.iter().map(String::as_str).collect();
        if got_case_ids != expected_ids {
            return Err(format!(
                "{} batch oracle `{batch_name}` ran cases {got_case_ids:?}, expected {expected_ids:?}\nstdout:\n{}\nstderr:\n{}",
                runtime.name, phase.stdout, phase.stderr
            ));
        }
        if let Some(active) = protocol.unfinished_case_id {
            return Err(format!(
                "{} batch oracle `{batch_name}` exited with unfinished case `{active}`\nstdout:\n{}\nstderr:\n{}",
                runtime.name, phase.stdout, phase.stderr
            ));
        }
        let got_ids: Vec<&str> = protocol
            .outcomes
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        if got_ids != expected_ids {
            return Err(format!(
                "{} batch oracle `{batch_name}` returned case ids {got_ids:?}, expected {expected_ids:?}\nstdout:\n{}\nstderr:\n{}",
                runtime.name, phase.stdout, phase.stderr
            ));
        }
        Ok(protocol
            .outcomes
            .into_iter()
            .map(|item| (item.id, item.outcome))
            .collect())
    }

    let (gnu_result, neomacs_result) = thread::scope(|scope| {
        let gnu_handle = scope.spawn(|| evaluate_batch(gnu_emacs, batch_name, setup, cases));
        let neomacs_handle = scope.spawn(|| evaluate_batch(neomacs, batch_name, setup, cases));
        (
            gnu_handle
                .join()
                .unwrap_or_else(|_| Err("GNU Emacs batch oracle thread panicked".into())),
            neomacs_handle
                .join()
                .unwrap_or_else(|_| Err("Neomacs batch oracle thread panicked".into())),
        )
    });

    let gnu_outcomes = gnu_result.map_err(|error| format!("GNU Emacs baseline failed: {error}"))?;
    let neomacs_outcomes =
        neomacs_result.map_err(|error| format!("Neomacs comparison failed: {error}"))?;

    if gnu_outcomes.len() != neomacs_outcomes.len() {
        return Err(format!(
            "oracle batch `{batch_name}` length mismatch: Neomacs {} cases, GNU Emacs {} cases",
            neomacs_outcomes.len(),
            gnu_outcomes.len()
        ));
    }

    let mut reports = Vec::with_capacity(cases.len());
    let mut failures = Vec::new();
    for ((gnu_id, gnu_outcome), (neo_id, neo_outcome)) in
        gnu_outcomes.into_iter().zip(neomacs_outcomes)
    {
        debug_assert_eq!(gnu_id, neo_id);
        if neo_outcome != gnu_outcome {
            failures.push(OracleBatchFailure::OutcomeMismatch {
                id: gnu_id.clone(),
                neomacs: neo_outcome.clone(),
                gnu_emacs: gnu_outcome.clone(),
            });
        }
        reports.push(OracleBatchCaseReport {
            id: gnu_id,
            neomacs: neo_outcome,
            gnu_emacs: gnu_outcome,
        });
    }
    Ok(OracleBatchReport {
        cases: reports,
        failures,
    })
}

/// Install packages, generate `package-quickstart-file`, then load that file
/// and probe package activation in a fresh editor process.
pub fn run_quickstart_scenario(
    runtime: &EmacsRuntime,
    source: &PackageSource,
    scenario: &PackageScenario,
) -> Result<ScenarioReport, String> {
    let quickstart_setup = r##"
           (setq package-quickstart t
                 package-quickstart-file
                 (expand-file-name ".emacs.d/package-quickstart.el"
                                   (getenv "HOME")))
           (package-quickstart-refresh)
           (unless (file-exists-p package-quickstart-file)
             (error "package quickstart file was not generated"))"##;
    let quickstart_probe = format!(
        r##"(progn
           (require 'package)
           (setq package-user-dir (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                 package-quickstart t
                 package-quickstart-file
                 (expand-file-name ".emacs.d/package-quickstart.el"
                                   (getenv "HOME")))
           (load package-quickstart-file nil nil t)
           {})"##,
        scenario.probe
    );
    run_install_and_probe(
        runtime,
        scenario,
        install_form(source, &scenario.package_names(), quickstart_setup),
        wrap_elisp_outcome("", &quickstart_probe, OUTCOME_MARKER),
        ScenarioPhase::QuickstartProbe,
    )
}

/// Install packages, delete one archive package, then verify the resulting
/// package state in a fresh editor process.
pub fn run_delete_and_probe_scenario(
    runtime: &EmacsRuntime,
    source: &PackageSource,
    scenario: &PackageScenario,
    package_to_delete: &str,
) -> Result<ScenarioReport, String> {
    let delete_setup = format!(
        r##"
           (let* ((name (intern {}))
                  (description (cadr (assq name package-alist))))
             (unless description
               (error "package selected for deletion was not installed"))
             (package-delete description t)
             (when (package-installed-p name)
               (error "archive package remained installed after delete")))"##,
        elisp_string(package_to_delete)
    );
    run_install_and_probe(
        runtime,
        scenario,
        install_form(source, &scenario.package_names(), &delete_setup),
        probe_form(&scenario.probe),
        ScenarioPhase::RestartProbe,
    )
}

/// Exercise `package-vc` against a local Git repository through install,
/// restart, upgrade, delete, and restart-after-delete.
pub fn run_package_vc_lifecycle(runtime: &EmacsRuntime) -> Result<PackageVcReport, String> {
    let scenario_name = "offline-package-vc-lifecycle";
    let sandbox = MelpaSandbox::new(scenario_name)?;
    let repository = sandbox.root().join("neo-vc-fixture-remote");
    fs::create_dir_all(&repository).map_err(|error| {
        format!(
            "failed to create package-vc fixture repository {}: {error}",
            repository.display()
        )
    })?;
    let fixture_root = workspace_root().join("crates/neomacs-melpa-tests/fixtures/package-vc");
    let package_file = repository.join("neo-vc-fixture.el");
    fs::copy(fixture_root.join("neo-vc-fixture-v1.el"), &package_file)
        .map_err(|error| format!("failed to seed package-vc v1 fixture: {error}"))?;
    initialize_git_fixture(&sandbox, &repository)?;

    let repository_string = elisp_string(&repository.to_string_lossy());
    let package_setup = r##"
           (require 'package)
           (require 'package-vc)
           (setq package-user-dir (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                 package-archives nil
                 package-vc--archive-data-alist '((offline)))
           (package-initialize)"##;
    let install_form = format!(
        r##"(progn
           {package_setup}
           (package-vc-install
            '(neo-vc-fixture :url {repository_string} :vc-backend Git))
           (let* ((description (cadr (assq 'neo-vc-fixture package-alist)))
                  (directory (and description (package-desc-dir description)))
                  (bytecode (and directory
                                 (expand-file-name "neo-vc-fixture.elc" directory))))
             (unless (and description bytecode (file-exists-p bytecode))
               (error "package-vc did not install and compile v1")))
           (princ "{RESULT_MARKER}installed-v1"))"##
    );
    let restart_v1_form = format!(
        r##"(progn
           {package_setup}
           (unless (and (package-installed-p 'neo-vc-fixture)
                        (fboundp 'neo-vc-fixture-version)
                        (string= (neo-vc-fixture-version) "v1"))
             (error "package-vc v1 did not survive restart"))
           (princ "{RESULT_MARKER}restarted-v1"))"##
    );

    let mut progress = PackageVcProgress::with_capacity(5);
    run_checkpoint(
        runtime,
        &sandbox,
        scenario_name,
        ScenarioPhase::VcInstall,
        &install_form,
        &mut progress,
    )?;
    run_checkpoint(
        runtime,
        &sandbox,
        scenario_name,
        ScenarioPhase::VcRestart,
        &restart_v1_form,
        &mut progress,
    )?;

    fs::copy(fixture_root.join("neo-vc-fixture-v2.el"), &package_file)
        .map_err(|error| format!("failed to update package-vc v2 fixture: {error}"))?;
    git(&sandbox, &repository, ["add", "neo-vc-fixture.el"])?;
    git(&sandbox, &repository, ["commit", "-m", "fixture v2"])?;

    let upgrade_form = format!(
        r##"(progn
           {package_setup}
           (package-vc-upgrade
            (cadr (assq 'neo-vc-fixture package-alist)))
           (let ((deadline (+ (float-time) 30)))
             (while (and
                     (not
                      (equal
                       (package-desc-version
                        (cadr (assq 'neo-vc-fixture package-alist)))
                       '(2 0)))
                     (< (float-time) deadline))
               (accept-process-output nil 0.05)))
           (unless (equal
                    (package-desc-version
                     (cadr (assq 'neo-vc-fixture package-alist)))
                    '(2 0))
             (error "package-vc upgrade did not install v2"))
           (princ "{RESULT_MARKER}upgraded-v2"))"##
    );
    run_checkpoint(
        runtime,
        &sandbox,
        scenario_name,
        ScenarioPhase::VcUpgrade,
        &upgrade_form,
        &mut progress,
    )?;

    let delete_form = format!(
        r##"(progn
           {package_setup}
           (unless (and (fboundp 'neo-vc-fixture-version)
                        (string= (neo-vc-fixture-version) "v2"))
             (error "package-vc v2 did not survive restart"))
           (package-delete (cadr (assq 'neo-vc-fixture package-alist)) t)
           (when (package-installed-p 'neo-vc-fixture)
             (error "package-vc package remained installed after delete"))
           (princ "{RESULT_MARKER}deleted"))"##
    );
    run_checkpoint(
        runtime,
        &sandbox,
        scenario_name,
        ScenarioPhase::VcDelete,
        &delete_form,
        &mut progress,
    )?;

    let absent_form = format!(
        r##"(progn
           {package_setup}
           (when (or (package-installed-p 'neo-vc-fixture)
                     (fboundp 'neo-vc-fixture-version))
             (error "deleted package-vc package reappeared after restart"))
           (princ "{RESULT_MARKER}absent-after-restart"))"##
    );
    run_checkpoint(
        runtime,
        &sandbox,
        scenario_name,
        ScenarioPhase::VcRestartAfterDelete,
        &absent_form,
        &mut progress,
    )?;

    Ok(PackageVcReport {
        runtime: runtime.name.clone(),
        phases: progress.phases,
        checkpoints: progress.checkpoints,
    })
}

fn initialize_git_fixture(sandbox: &MelpaSandbox, repository: &Path) -> Result<(), String> {
    git(sandbox, repository, ["init", "--initial-branch=main"])?;
    git(
        sandbox,
        repository,
        ["config", "user.email", "melpa-test@example.invalid"],
    )?;
    git(sandbox, repository, ["config", "user.name", "MELPA Test"])?;
    git(sandbox, repository, ["add", "neo-vc-fixture.el"])?;
    git(sandbox, repository, ["commit", "-m", "fixture v1"])
}

fn git<const N: usize>(
    sandbox: &MelpaSandbox,
    repository: &Path,
    args: [&str; N],
) -> Result<(), String> {
    let mut command = Command::new("git");
    sandbox.configure(&mut command);
    let output = command
        .current_dir(repository)
        .args(args)
        .output()
        .map_err(|error| format!("failed to launch git in {}: {error}", repository.display()))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git failed in {} (exit {:?})\nstdout:\n{}\nstderr:\n{}",
        repository.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn run_checkpoint(
    runtime: &EmacsRuntime,
    sandbox: &MelpaSandbox,
    scenario_name: &str,
    phase: ScenarioPhase,
    form: &str,
    progress: &mut PackageVcProgress,
) -> Result<(), String> {
    let report = run_phase(runtime, sandbox, scenario_name, phase, form)?;
    let checkpoint = extract_marker(&report.stdout, RESULT_MARKER).ok_or_else(|| {
        format!(
            "{} scenario `{scenario_name}` did not emit `{RESULT_MARKER}` during {phase:?}\nstdout:\n{}\nstderr:\n{}",
            runtime.name, report.stdout, report.stderr
        )
    })?;
    progress.phases.push(report);
    progress.checkpoints.push(checkpoint);
    Ok(())
}

fn run_install_and_probe(
    runtime: &EmacsRuntime,
    scenario: &PackageScenario,
    install_form: String,
    probe_form: String,
    probe_phase: ScenarioPhase,
) -> Result<ScenarioReport, String> {
    let sandbox = MelpaSandbox::new(&scenario.name)?;
    let mut phases = Vec::with_capacity(2);

    let install = run_phase(
        runtime,
        &sandbox,
        &scenario.name,
        ScenarioPhase::Install,
        &install_form,
    )?;
    let installed_packages = extract_installed_packages(&install.stdout).map_err(|error| {
        format!(
            "{} scenario `{}` emitted an invalid installed-package report during Install: {error}\nstdout:\n{}\nstderr:\n{}",
            runtime.name, scenario.name, install.stdout, install.stderr
        )
    })?;
    if let Some(expected_packages) = scenario.package_pins() {
        for expected in expected_packages {
            let actual = installed_packages
                .iter()
                .find(|installed| installed.name == expected.name);
            if actual.map(|installed| installed.version.as_str()) != Some(expected.version.as_str())
            {
                return Err(format!(
                    "{} scenario `{}` installed an unexpected version of `{}`: expected {}, got {}",
                    runtime.name,
                    scenario.name,
                    expected.name,
                    expected.version,
                    actual
                        .map(|installed| installed.version.as_str())
                        .unwrap_or("<not installed>")
                ));
            }
        }
    }
    phases.push(install);

    let probe = run_outcome_phase(runtime, &sandbox, &scenario.name, probe_phase, &probe_form)
        .map_err(|error| {
            format!(
                "{error}\ninstalled packages: {}",
                format_installed_packages(&installed_packages)
            )
        })?;
    let outcome = extract_marked_outcome(&probe.stderr, OUTCOME_MARKER).map_err(|error| {
        format!(
            "{} scenario `{}` emitted an invalid oracle outcome during {probe_phase:?}: {error}\ninstalled packages: {}\nstdout:\n{}\nstderr:\n{}",
            runtime.name,
            scenario.name,
            format_installed_packages(&installed_packages),
            probe.stdout,
            probe.stderr
        )
    })?;
    phases.push(probe);

    Ok(ScenarioReport {
        runtime: runtime.name.clone(),
        scenario: scenario.name.clone(),
        phases,
        installed_packages,
        outcome,
    })
}

fn run_phase(
    runtime: &EmacsRuntime,
    sandbox: &MelpaSandbox,
    scenario_name: &str,
    phase: ScenarioPhase,
    form: &str,
) -> Result<PhaseReport, String> {
    run_phase_with_validation(runtime, sandbox, scenario_name, phase, form, true)
}

fn run_outcome_phase(
    runtime: &EmacsRuntime,
    sandbox: &MelpaSandbox,
    scenario_name: &str,
    phase: ScenarioPhase,
    form: &str,
) -> Result<PhaseReport, String> {
    run_phase_with_validation(runtime, sandbox, scenario_name, phase, form, false)
}

fn run_phase_with_validation(
    runtime: &EmacsRuntime,
    sandbox: &MelpaSandbox,
    scenario_name: &str,
    phase: ScenarioPhase,
    form: &str,
    check_editor_error_output: bool,
) -> Result<PhaseReport, String> {
    let form_directory = workspace_root().join("tmp/melpa/editor-forms");
    fs::create_dir_all(&form_directory).map_err(|error| {
        format!(
            "failed to create editor-form directory {}: {error}",
            form_directory.display()
        )
    })?;
    let form_file = tempfile::Builder::new()
        .prefix(&format!("{}-", sanitize_label(scenario_name)))
        .suffix(".form.el")
        .tempfile_in(&form_directory)
        .map_err(|error| {
            format!(
                "failed to create {phase:?} form for scenario `{scenario_name}` in {}: {error}",
                form_directory.display()
            )
        })?;
    fs::write(form_file.path(), form).map_err(|error| {
        format!(
            "failed to write {phase:?} form for scenario `{scenario_name}` to {}: {error}",
            form_file.path().display()
        )
    })?;
    let loader_file = tempfile::Builder::new()
        .prefix(&format!("{}-", sanitize_label(scenario_name)))
        .suffix(".loader.el")
        .tempfile_in(&form_directory)
        .map_err(|error| {
            format!(
                "failed to create {phase:?} loader for scenario `{scenario_name}` in {}: {error}",
                form_directory.display()
            )
        })?;
    let loader = format!(
        r##";;; -*- lexical-binding: t; -*-
(defun {TRANSPORTED_FORM_FUNCTION} ()
  (let ((form
         (with-temp-buffer
           (insert-file-contents (getenv "NEOMACS_MELPA_ORACLE_FORM_FILE"))
           (goto-char (point-min))
           (read (current-buffer)))))
    (eval form t)))
"##
    );
    fs::write(loader_file.path(), loader).map_err(|error| {
        format!(
            "failed to write {phase:?} loader for scenario `{scenario_name}` to {}: {error}",
            loader_file.path().display()
        )
    })?;
    let mut command = runtime.command();
    sandbox.configure(&mut command);
    command
        .env("NEOMACS_RUNTIME_ROOT", workspace_root())
        .env("NEOMACS_MELPA_ORACLE_FORM_FILE", form_file.path())
        .args(["--batch", "--quick", "--load"])
        .arg(loader_file.path())
        .args(["--eval", &format!("({TRANSPORTED_FORM_FUNCTION})")]);
    let started = Instant::now();
    let output = output_with_timeout(&mut command, runtime.timeout)
        .map_err(|error| command_error_message(error, runtime, sandbox, scenario_name, phase))?;
    let report = phase_report(phase, started.elapsed(), output);
    if report.status_code != Some(0) {
        return Err(format!(
            "{} scenario `{scenario_name}` failed during {phase:?} (exit {:?})\nstdout:\n{}\nstderr:\n{}",
            runtime.name, report.status_code, report.stdout, report.stderr
        ));
    }
    if check_editor_error_output {
        check_error_markers(&report.stdout, &report.stderr).map_err(|error| {
            format!(
                "{} scenario `{scenario_name}` failed during {phase:?}: {error}",
                runtime.name
            )
        })?;
    }
    Ok(report)
}

fn command_error_message(
    error: CommandError,
    runtime: &EmacsRuntime,
    sandbox: &MelpaSandbox,
    scenario_name: &str,
    phase: ScenarioPhase,
) -> String {
    match error {
        CommandError::Launch(error) => format!(
            "failed to launch {} for {phase:?} in scenario `{scenario_name}` sandbox {}: {error}",
            runtime.name,
            sandbox.root().display()
        ),
        CommandError::TimedOut(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let protocol_context = match extract_marked_batch_protocol(
                &stderr,
                BATCH_BEGIN_MARKER,
                OUTCOME_MARKER,
                BATCH_COMPLETE_MARKER,
            ) {
                Ok(protocol) => protocol
                    .unfinished_case_id
                    .map(|id| format!("; active case `{id}`"))
                    .unwrap_or_default(),
                Err(error) => format!("; invalid partial batch protocol: {error}"),
            };
            format!(
                "{} scenario `{scenario_name}` timed out during {phase:?} after {:?} in sandbox {}{protocol_context}\npartial stdout:\n{stdout}\npartial stderr:\n{stderr}",
                runtime.name,
                runtime.timeout,
                sandbox.root().display()
            )
        }
        CommandError::Capture(error) => format!(
            "failed to capture {} scenario `{scenario_name}` output during {phase:?}: {error}",
            runtime.name
        ),
    }
}

fn phase_report(phase: ScenarioPhase, duration: Duration, output: Output) -> PhaseReport {
    PhaseReport {
        phase,
        duration,
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn extract_ert_summary(stdout: &str, stderr: &str) -> Option<ErtSummary> {
    stdout
        .lines()
        .chain(stderr.lines())
        .filter_map(parse_ert_summary_line)
        .next_back()
}

fn parse_ert_summary_line(line: &str) -> Option<ErtSummary> {
    let fields = line
        .trim()
        .trim_end_matches(')')
        .split_once("Ran ")?
        .1
        .split_whitespace()
        .map(|field| field.trim_end_matches(','))
        .collect::<Vec<_>>();
    if fields.get(1) != Some(&"tests") || fields.get(3..6) != Some(&["results", "as", "expected"]) {
        return None;
    }
    Some(ErtSummary {
        total: fields.first()?.parse().ok()?,
        expected: fields.get(2)?.parse().ok()?,
        unexpected: count_before(&fields, "unexpected").unwrap_or(0),
        skipped: count_before(&fields, "skipped").unwrap_or(0),
    })
}

fn count_before(fields: &[&str], label: &str) -> Option<usize> {
    let index = fields.iter().position(|field| *field == label)?;
    fields.get(index.checked_sub(1)?)?.parse().ok()
}

fn install_form(source: &PackageSource, packages: &[&str], post_install: &str) -> String {
    let installs = packages
        .iter()
        .map(|package| format!("(package-install '{package})"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r##"(progn
           (require 'package)
           (setq package-user-dir (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                 package-archives {}
                 package-check-signature nil)
           (package-initialize)
           (package-refresh-contents)
           {}
           {}
           (let ((installed
                  (mapcar
                   (lambda (entry)
                     (cons (car entry)
                           (package-version-join
                            (package-desc-version (cadr entry)))))
                   package-alist)))
             (setq installed
                   (sort installed
                         (lambda (left right)
                           (string< (symbol-name (car left))
                                    (symbol-name (car right))))))
             (dolist (entry installed)
               (princ "\n{INSTALLED_MARKER}")
               (princ (symbol-name (car entry)))
               (princ "\t")
               (princ (cdr entry)))))"##,
        source.archive_form(),
        installs,
        post_install
    )
}

fn probe_form(probe: &str) -> String {
    let setup = r##"
           (require 'package)
           (setq package-user-dir (expand-file-name ".emacs.d/elpa" (getenv "HOME")))
           (package-initialize)"##;
    wrap_elisp_outcome(setup, probe, OUTCOME_MARKER)
}

fn extract_marker(stdout: &str, marker: &str) -> Option<String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once(marker).map(|(_, value)| value.trim()))
        .next_back()
        .map(str::to_string)
}

fn extract_installed_packages(stdout: &str) -> Result<Vec<InstalledPackage>, String> {
    let mut installed = Vec::new();
    for value in stdout
        .lines()
        .filter_map(|line| line.split_once(INSTALLED_MARKER).map(|(_, value)| value))
    {
        let (name, version) = value.trim().split_once('\t').ok_or_else(|| {
            format!(r##"expected `{INSTALLED_MARKER}<name>\t<version>`, got `{value}`"##)
        })?;
        installed.push(InstalledPackage {
            name: name.to_string(),
            version: version.to_string(),
        });
    }
    if installed.is_empty() {
        return Err(format!("did not emit `{INSTALLED_MARKER}`"));
    }
    Ok(installed)
}

fn format_installed_packages(installed: &[InstalledPackage]) -> String {
    installed
        .iter()
        .map(|package| format!("{}@{}", package.name, package.version))
        .collect::<Vec<_>>()
        .join(", ")
}

fn check_error_markers(stdout: &str, stderr: &str) -> Result<(), String> {
    for needle in [
        "wrong-type-argument",
        "void-function",
        "file-missing",
        "invalid-read-syntax",
        "end-of-file",
        "Error:",
    ] {
        if stdout.contains(needle) || stderr.contains(needle) {
            return Err(format!(
                "editor emitted `{needle}`:\nstdout:\n{stdout}\nstderr:\n{stderr}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod parity_tests;

#[cfg(test)]
mod direct_adapter_tests;

#[cfg(all(test, target_os = "linux"))]
mod gui_parity_tests;

#[cfg(all(test, unix))]
mod tui_parity_tests;

/// The exact dashboard package selected for practical startup screen widgets and buffer name.
/// MELPA built this archive from upstream commit `176d641a55543bda1f0c7506fb954702350c1857`.
pub const DASHBOARD_MELPA_PIN: (&str, &str) = ("dashboard", "20260402.436");

/// The exact yapfify package selected for practical yapf region line-range process invocation.
/// MELPA built this archive from upstream commit `c9347e3b1dec5fc8d34883e206fcdc8500d22368`.
pub const YAPFIFY_MELPA_PIN: (&str, &str) = ("yapfify", "20210914.634");

/// The exact prettier-js package selected for practical prettier command/width/diff assembly without live prettier.
/// MELPA built this archive from upstream commit `1ce7a310b000200e333f0015b87d910672ebdb7e`.
pub const PRETTIER_JS_MELPA_PIN: (&str, &str) = ("prettier-js", "20250705.322");

/// The exact toml-mode package selected for practical TOML major-mode setup and align rules.
/// MELPA built this archive from upstream commit `f6c61817b00f9c4a3cab1bae9c309e0fc45cdd06`.
pub const TOML_MODE_MELPA_PIN: (&str, &str) = ("toml-mode", "20161107.1800");

/// The exact inflections package selected for practical English singular/plural conversion.
/// MELPA built this archive from upstream commit `55caa66a7cc6e0b1a76143fd40eff38416928941`.
pub const INFLECTIONS_MELPA_PIN: (&str, &str) = ("inflections", "20210110.2237");

/// The exact py-isort package selected for practical isort settings-path and option plumbing.
/// MELPA built this archive from upstream commit `e67306f459c47c53a65604e4eea88a3914596560`.
pub const PY_ISORT_MELPA_PIN: (&str, &str) = ("py-isort", "20160925.1018");

/// The exact pug-mode package selected for practical Pug indent computation without a live compiler.
/// MELPA built this archive from upstream commit `73f8c2f95eba695f701df20c8436f49abadebdc1`.
pub const PUG_MODE_MELPA_PIN: (&str, &str) = ("pug-mode", "20211114.1645");

/// The exact Jade Mode package used to exercise Company Web's documented
/// Jade backend against its real major mode. MELPA built this archive from
/// upstream commit `111460b056838854e470a6383041a99f843b93ee`.
pub const JADE_MODE_MELPA_PIN: (&str, &str) = ("jade-mode", "20210908.2121");

/// The exact monokai-theme package selected for practical theme load and palette defaults.
/// MELPA built this archive from upstream commit `dacd9d8a8867afea3ed76b15a6c997053ff88093`.
pub const MONOKAI_THEME_MELPA_PIN: (&str, &str) = ("monokai-theme", "20240911.1046");

/// The exact Moe Theme package selected for practical light/dark theme,
/// modeline color, palette flavour, and timed-switcher parity.
/// MELPA built version 1.1.0 from upstream commit
/// `d091865eeb97b0894e6517137dc0544560bc57fb`.
pub const MOE_THEME_MELPA_PIN: (&str, &str) = ("moe-theme", "20260811.1919");

/// The exact Node.js REPL package selected for practical live REPL startup,
/// source submission, completion, and recovery parity. MELPA built this
/// archive from upstream commit `77a864ca72a6c30217085f1c4db5de72e47eb4da`.
pub const NODEJS_REPL_MELPA_PIN: (&str, &str) = ("nodejs-repl", "20240218.2357");

/// The exact Org Rich Yank package selected for practical source, Org block,
/// clipboard-link, formatting, and advice lifecycle parity. MELPA built this
/// archive from upstream commit `fe2ba1c9d9f1f7943d8f76879a1b2b9b15928147`.
pub const ORG_RICH_YANK_MELPA_PIN: (&str, &str) = ("org-rich-yank", "20250923.919");

/// The exact Pippel package selected for practical pip process protocol,
/// package-menu actions, installation, and recovery parity. MELPA built this
/// archive from upstream commit `19153aa8845aa95d080f224d4fcaf2d75224bd5a`.
pub const PIPPEL_MELPA_PIN: (&str, &str) = ("pippel", "20220416.1743");

/// The exact spacemacs-theme package selected for practical theme load and defcustom defaults.
/// Pinned to upstream commit `cbd290dfde96f53a7b41730c7840850a8a7b8a02`.
pub const SPACEMACS_THEME_MELPA_PIN: (&str, &str) = ("spacemacs-theme", "0.2");

/// The exact restclient package selected for practical major-mode setup and MIME mapping.
/// Pinned to upstream commit `e2a2b13482d72634f8e49864cd9e5c907a5fe137`.
pub const RESTCLIENT_MELPA_PIN: (&str, &str) = ("restclient", "20231010.1427");

/// The exact Rg package selected for practical grouped-search workflows over
/// a real fixture tree with the pinned `rg' executable, file navigation,
/// hidden command handling, wgrep edit round trips, the transient menu
/// surface, and the configuration defaults. MELPA built this archive from
/// upstream commit `e46a16b8bdba111c9c0036d0e209490dd7a3690f`.
pub const RG_MELPA_PIN: (&str, &str) = ("rg", "20260517.1310");

/// The exact embark package selected for practical action-menu setup and defaults.
/// Pinned to upstream commit `350ca86924c5027e80875943fba7b912a71e5791`.
pub const EMBARK_MELPA_PIN: (&str, &str) = ("embark", "20260609.2102");

/// The exact julia-mode package selected for practical indentation and font-lock.
/// Pinned to upstream commit `1b5a4c2f5b7c3f842785985bf8778b8805cc6766`.
pub const JULIA_MODE_MELPA_PIN: (&str, &str) = ("julia-mode", "20260529.1624");

/// The exact elfeed package selected for practical feed/OPML/date parsing.
/// Pinned to upstream commit `2970e5d1aa2a6f5c4cb607e0b835b91a6bffec4f`.
pub const ELFEED_MELPA_PIN: (&str, &str) = ("elfeed", "20260805.1030");

/// The exact color-theme-sanityinc-solarized package selected for practical theme palette parity.
/// Pinned to upstream commit `f42431850e0ff0cff90c6cc39edc222faa40323d`.
pub const COLOR_THEME_SANITYINC_SOLARIZED_MELPA_PIN: (&str, &str) =
    ("color-theme-sanityinc-solarized", "20241126.1028");

/// The exact parsebib package selected for practical BibTeX entry parsing.
/// Pinned to upstream commit `5b837e0a5b91a69cc0e5086d8e4a71d6d86dac93`.
pub const PARSEBIB_MELPA_PIN: (&str, &str) = ("parsebib", "20251127.1731");

/// The exact groovy-mode package selected for practical indentation and font-lock.
/// Pinned to upstream commit `7b8520b2e2d3ab1d62b35c426e17ac25ed0120bb`.
pub const GROOVY_MODE_MELPA_PIN: (&str, &str) = ("groovy-mode", "20230317.2233");

/// The exact dracula-theme package selected for practical theme palette parity.
/// Pinned to upstream commit `df2be56b03fcbbafcc211013ff93ba50e34a4397`.
pub const DRACULA_THEME_MELPA_PIN: (&str, &str) = ("dracula-theme", "20260719.2250");

/// The exact material-theme package selected for practical theme palette parity.
/// Pinned to upstream commit `6823009bc92f82aa3a90e27e1009f7da8e87b648`.
pub const MATERIAL_THEME_MELPA_PIN: (&str, &str) = ("material-theme", "20210904.1426");

/// The exact Rich Minority package selected as the smart-mode-line
/// dependency for minor-mode list rendering. MELPA built this archive from
/// upstream commit `77cf5ec620aaef18385d2e1d2dad05b4f63dad95`.
pub const RICH_MINORITY_MELPA_PIN: (&str, &str) = ("rich-minority", "20240924.2317");

/// The exact Robe package selected for practical Ruby completion, ElDoc,
/// documentation, definition navigation, and server-lifecycle parity. MELPA
/// built this archive from upstream commit
/// `73a78e55394c1c70c11f9354ef52e7ffce31547c`.
pub const ROBE_MELPA_PIN: (&str, &str) = ("robe", "20250219.1910");
