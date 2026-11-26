//! Load and public-API-surface parity for the ETAF package ecosystem.
//!
//! Every package in <https://github.com/emacs-taf> is pinned in the package
//! lock (`crates/neomacs-infra/src/packages/melpa-package-lock.tsv`) and loaded
//! in both editors.  One probe per package compares the package's feature and
//! the sorted sets of package-prefixed symbols that are `fboundp` and
//! `boundp`, so a load failure, a missing primitive, or a macro expansion that
//! defines a different surface fails differentially instead of needing a
//! hand-written probe per package.
//!
//! Scope is deliberately load/surface only.  Network clients, TDLib, Godot and
//! display-dependent applications need fixtures or the GUI lane; the
//! engine-behaviour and GUI modules cover those layers separately.  A package
//! that cannot load in GNU Emacs at all fails here rather than passing as a
//! matching pair of errors, because a load-only test that never loads is
//! vacuous.

use std::time::Duration;

use neomacs_test_oracle::BatchProbe;

use crate::CachedMelpaOracle;

/// Timeout for preparing (first run) and loading one package graph.
const SURFACE_TIMEOUT: Duration = Duration::from_secs(240);

fn surface_oracle(package: &'static str, version: &'static str) -> CachedMelpaOracle {
    let source = format!("{package}.el");
    CachedMelpaOracle::new((package, version), &source)
        .unwrap_or_else(|error| panic!("prepare pinned {package} {version}: {error}"))
        .with_installed_autoloads()
        .with_timeout(SURFACE_TIMEOUT)
}

/// The probe returns the package's feature and its public surface: every
/// prefixed symbol that is bound as a function or as a variable.
fn surface_probe(package: &str) -> String {
    let prefix = format!("{package}-");
    format!(
        r#"(progn
  (require '{package})
  (let (functions variables)
    (mapatoms
     (lambda (symbol)
       (let ((name (symbol-name symbol)))
         (when (string-prefix-p "{prefix}" name)
           (when (fboundp symbol) (push name functions))
           (when (boundp symbol) (push name variables))))))
    (list :feature (featurep '{package})
          :functions (sort (delete-dups functions) #'string<)
          :variables (sort (delete-dups variables) #'string<))))"#,
        package = package,
        prefix = prefix,
    )
}

fn assert_surface(package: &'static str, version: &'static str) {
    let oracle = surface_oracle(package, version);
    let probe = surface_probe(package);
    let batch = format!("{package}-surface");
    let report = oracle
        .run_probes(
            &batch,
            &[BatchProbe {
                id: "surface",
                probe: probe.as_str(),
            }],
        )
        .unwrap_or_else(|error| panic!("{package} {version} surface batch: {error}"));

    let observed = report
        .cases
        .first()
        .unwrap_or_else(|| panic!("{package} surface batch returned no case report"));
    if !observed.gnu_emacs.is_value() {
        panic!(
            "{package} {version} did not load in GNU Emacs: {}",
            observed.gnu_emacs
        );
    }
    if !report.failures.is_empty() {
        let failures = report
            .failures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        panic!("ETAF package {package} {version} diverges:\n{failures}");
    }
}

macro_rules! etaf_surface_test {
    ($name:ident, $package:literal, $version:literal) => {
        #[test]
        fn $name() {
            assert_surface($package, $version);
        }
    };
}

etaf_surface_test!(etaf_surface_ebox, "ebox", "3.1.2");
etaf_surface_test!(etaf_surface_ebox_playground, "ebox-playground", "0.1.1");
etaf_surface_test!(etaf_surface_ecss, "ecss", "0.1.0");
etaf_surface_test!(etaf_surface_ekp, "ekp", "1.0.1");
etaf_surface_test!(etaf_surface_etaf, "etaf", "0.3.5");
etaf_surface_test!(etaf_surface_etaf_algo_lab, "etaf-algo-lab", "0.1.1");
etaf_surface_test!(etaf_surface_etaf_ascii_video, "etaf-ascii-video", "0.1.1");
etaf_surface_test!(etaf_surface_etaf_bili, "etaf-bili", "0.1.0");
etaf_surface_test!(etaf_surface_etaf_db, "etaf-db", "0.1.0");
etaf_surface_test!(etaf_surface_etaf_desktop, "etaf-desktop", "0.1.7");
etaf_surface_test!(etaf_surface_etaf_gamekit, "etaf-gamekit", "0.2.0");
etaf_surface_test!(etaf_surface_etaf_md, "etaf-md", "0.2.0");
etaf_surface_test!(etaf_surface_etaf_ncm, "etaf-ncm", "0.2.3");
etaf_surface_test!(etaf_surface_etaf_org_preview, "etaf-org-preview", "0.3.1");
etaf_surface_test!(etaf_surface_etaf_pi, "etaf-pi", "0.1.1");
etaf_surface_test!(etaf_surface_etaf_playground, "etaf-playground", "0.2.2");
etaf_surface_test!(etaf_surface_etaf_pomodoro, "etaf-pomodoro", "0.2.0");
etaf_surface_test!(etaf_surface_etaf_posframe, "etaf-posframe", "0.3.0");
etaf_surface_test!(etaf_surface_etaf_reader, "etaf-reader", "0.2.0");
etaf_surface_test!(etaf_surface_etaf_rss, "etaf-rss", "0.3.0");
etaf_surface_test!(etaf_surface_etaf_salvage, "etaf-salvage", "0.1.0");
etaf_surface_test!(etaf_surface_etaf_screensaver, "etaf-screensaver", "0.1.1");
etaf_surface_test!(etaf_surface_etaf_solitaire, "etaf-solitaire", "0.1.0");
etaf_surface_test!(etaf_surface_etaf_tanks, "etaf-tanks", "0.2.10");
etaf_surface_test!(etaf_surface_etaf_telega, "etaf-telega", "0.1.7");
etaf_surface_test!(etaf_surface_etaf_ui, "etaf-ui", "0.3.3");
etaf_surface_test!(etaf_surface_etaf_video_player, "etaf-video-player", "0.2.1");
etaf_surface_test!(etaf_surface_etaf_video_studio, "etaf-video-studio", "0.1.1");
etaf_surface_test!(etaf_surface_etaf_weread, "etaf-weread", "0.2.5");
etaf_surface_test!(etaf_surface_etaf_xiangqi, "etaf-xiangqi", "0.1.7");
etaf_surface_test!(etaf_surface_etaf_youtube, "etaf-youtube", "0.1.1");
etaf_surface_test!(etaf_surface_maillist, "maillist", "0.1.0");
