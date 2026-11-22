//! Both editors of a GUI pair resolve fonts only through the pinned fontconfig.
//!
//! The ETAF probes pin only the default face's family, so every Han character
//! falls back through fontconfig.  Before the harness pinned one private
//! `FONTCONFIG_FILE` for both engines, that fallback followed the host's
//! installed fonts and a GNU row recorded on one host disagreed with live GNU
//! on another (MOVE-NOTE 6059: `ebox--row-strut` fires when the Han fallback
//! is taller than the default face).  This probe is the pair-level proof that
//! the pinning worked at every layer: fontconfig itself answers U+6C49 (汉)
//! with the pinned Noto Sans CJK fixture and sees nothing else, and inside one
//! pair GNU Emacs and Neomacs resolve `0` to the pinned DejaVu Sans Mono and
//! 汉 to the pinned Noto Sans CJK with the same metrics.
//!
//! Font files are deliberately absent from the pinned rows: the private fonts
//! directory is named per run, so a path would move with every execution.

use std::path::{Path, PathBuf};
use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::CachedMelpaOracle;

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 100)
  (setq line-spacing nil)
  (let ((buffer (get-buffer-create " *font-hermeticity*")))
    (with-current-buffer buffer
      (erase-buffer)
      (insert "0汉"))
    (set-window-buffer (selected-window) buffer)
    (redisplay t)
    (let* ((zero (with-current-buffer buffer (font-at 1)))
           (han (with-current-buffer buffer (font-at 2)))
           (resolved
            (lambda (font)
              ;; GNU 31.1 font-info vector: [OPENED-NAME FULL-NAME SIZE
              ;; HEIGHT BASELINE-OFFSET RELATIVE-COMPOSE DEFAULT-ASCENT
              ;; MAX-WIDTH ASCENT DESCENT SPACE-WIDTH AVERAGE-WIDTH FILENAME
              ;; CAPABILITY].  FILENAME (index 12) is the private fonts dir
              ;; path, which moves every run, so only stable fields print.
              ;; `font-at' requires the window's buffer to be current (GNU
              ;; errors with "window is not displaying the current buffer"
              ;; otherwise), and the CJK collection's region face (JP/KR/…)
              ;; follows the reader's sort order while sharing metrics, so
              ;; the region is normalized out of the recorded XLFD.
              (let* ((xlfd (and font (font-xlfd-name font)))
                     (info (and font (font-info xlfd))))
                (and info
                     (list :xlfd (replace-regexp-in-string
                                  "Noto Sans CJK [A-Z]*" "Noto Sans CJK" xlfd)
                           :pixel-size (aref info 2)
                           :height (aref info 3)
                           :ascent (aref info 8)
                           :descent (aref info 9)
                           :space-width (aref info 10)))))))
      (list :zero (funcall resolved zero)
            :han (funcall resolved han)))))
"####;

#[test]
fn gui_pair_resolves_only_pinned_fonts() {
    let oracle = CachedMelpaOracle::new(("etaf-desktop", "0.1.7"), "etaf-desktop.el")
        .expect("prepare pinned etaf-desktop source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "gui-font-hermeticity",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    // Fontconfig-level evidence about the exact world both editors used: the
    // Han code point must resolve to the pinned CJK fixture, and every face
    // fontconfig can enumerate must come from the pair's private directory.
    // File-identity asserts (not family strings) keep this stable across
    // fontconfig versions, which may order the one OTC's faces differently.
    let han_file = PathBuf::from(fc_first_property(
        &pair.fonts_config,
        "fc-match",
        &[":charset=6c49"],
        "%{file}",
    ));
    assert_eq!(
        han_file.file_name().and_then(|name| name.to_str()),
        Some("NotoSansCJK-VF-2.004.otf.ttc"),
        "fc-match :charset=6c49 must resolve to the pinned CJK fixture, got {han_file:?}"
    );
    let pinned_dir = pair
        .fonts_config
        .parent()
        .expect("fonts.conf sits inside the private font root")
        .join("fonts");
    for line in fc_lines(&pair.fonts_config, "fc-list", &["--format", "%{file}\n"]) {
        let file = Path::new(&line);
        assert_eq!(
            file.parent(),
            Some(pinned_dir.as_path()),
            "fc-list must see only the pair's private fonts dir, saw {line:?}"
        );
    }

    // Editor-level evidence: both engines resolve the same pinned faces with
    // the same metrics.  GNU's row is pinned; Neomacs must equal it exactly.
    let expected_gnu = expect![[
        r##"OK (:zero (:xlfd "-PfEd-DejaVu Sans Mono-regular-normal-normal-*-14-*-*-*-m-0-iso10646-1" :pixel-size 14 :height 17 :ascent 13 :descent 4 :space-width 8) :han (:xlfd "-ADBO-Noto Sans CJK-regular-normal-normal-*-14-*-*-*-*-0-iso10646-1" :pixel-size 14 :height 22 :ascent 17 :descent 5 :space-width 3))"##
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    // Strict parity: the probe must produce GNU's exact observation.  There
    // is deliberately no pinned Neomacs outcome, so a divergence cannot be
    // blessed by re-recording it.
    assert_eq!(
        pair.neomacs_behavior.to_string(),
        pair.gnu_behavior.to_string(),
        "font resolution must be identical in both engines under the pinned fontconfig"
    );
}

/// Run a fontconfig CLI under the pair's pinned `FONTCONFIG_FILE` and return
/// its stdout lines.
fn fc_lines(config: &Path, program: &str, args: &[&str]) -> Vec<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .env("FONTCONFIG_FILE", config)
        .output()
        .unwrap_or_else(|error| {
            panic!("failed to run {program} under the pinned fontconfig: {error}")
        });
    assert!(
        output.status.success(),
        "{program} failed under the pinned fontconfig: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("{program} emitted non-UTF-8 output: {error}"))
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Ask a fontconfig CLI for one property of its single match.
fn fc_first_property(config: &Path, program: &str, args: &[&str], format: &str) -> String {
    let mut full_args = args.to_vec();
    full_args.push("--format");
    full_args.push(format);
    let formatted = fc_lines(config, program, &full_args);
    assert_eq!(
        formatted.len(),
        1,
        "expected exactly one formatted {program} line, got {formatted:?}"
    );
    formatted.into_iter().next().expect("one formatted line")
}
