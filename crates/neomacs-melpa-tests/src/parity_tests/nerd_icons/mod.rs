use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, NERD_ICONS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const NERD_ICONS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const NERD_ICONS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'nerd-icons)

(defun nerd-icons-test-describe (icon)
  (let ((face (get-text-property 0 'face icon)))
    (list :glyph (substring-no-properties icon)
          :codes (string-to-list (substring-no-properties icon))
          :length (length icon)
          :face (copy-tree face)
          :font-lock-same
          (equal face (get-text-property 0 'font-lock-face icon))
          :display (copy-tree (get-text-property 0 'display icon))
          :rear-nonsticky (get-text-property 0 'rear-nonsticky icon))))

(defun nerd-icons-test-properties-at (position)
  (let ((face (get-text-property position 'face)))
    (list :face (copy-tree face)
          :font-lock-same
          (equal face (get-text-property position 'font-lock-face))
          :display (copy-tree (get-text-property position 'display))
          :rear-nonsticky
          (get-text-property position 'rear-nonsticky))))

(defun nerd-icons-test-describe-many (icons)
  (mapcar #'nerd-icons-test-describe icons))
"##;

fn nerd_icons_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NERD_ICONS_MELPA_PIN, "nerd-icons.el")
        .expect("prepare pinned Nerd Icons source below ./tmp")
        .with_prelude(NERD_ICONS_TEST_PRELUDE)
        .with_timeout(NERD_ICONS_TEST_TIMEOUT)
}

fn styled_glyphs_preserve_family_scale_color_raise_and_property_boundaries() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((nerd-icons-font-family "Parity Nerd Mono")
       (nerd-icons-scale-factor 2.0)
       (nerd-icons-default-adjust 0.125)
       (nerd-icons-color-icons t)
       (colored
        (nerd-icons-faicon "nf-fa-gitlab"
                           :height 0.75
                           :v-adjust 0.25
                           :face 'nerd-icons-red))
       (plain
        (let ((nerd-icons-color-icons nil))
          (nerd-icons-codicon "nf-cod-git_commit" :height 0.5)))
       (inserted
        (with-temp-buffer
          (insert colored " release " plain " done")
          (list :text (buffer-substring-no-properties (point-min) (point-max))
                :first (nerd-icons-test-properties-at 1)
                :after-first
                (and (text-properties-at 2)
                     (nerd-icons-test-properties-at 2))
                :second (nerd-icons-test-properties-at 11)
                :after-second
                (and (text-properties-at 12)
                     (nerd-icons-test-properties-at 12))))))
  (list :colored (nerd-icons-test-describe colored)
        :plain (nerd-icons-test-describe plain)
        :inserted inserted))
"##;
    let expect = expect![[
        r####"OK (:colored (:glyph "" :codes (62102) :length 1 :face (:family "Parity Nerd Mono" :height 1.5 :inherit nerd-icons-red) :font-lock-same t :display (raise 0.5) :rear-nonsticky t) :plain (:glyph "" :codes (60156) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.25) :rear-nonsticky t) :inserted (:text " release  done" :first (:face (:family "Parity Nerd Mono" :height 1.5 :inherit nerd-icons-red) :font-lock-same t :display (raise 0.5) :rear-nonsticky t) :after-first nil :second (:face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.25) :rear-nonsticky t) :after-second nil))"####
    ]];
    ParityBatchCase::value(
        "styled_glyphs_preserve_family_scale_color_raise_and_property_boundaries",
        elisp_form,
        expect,
    )
}

fn file_icons_apply_exact_name_before_extension_and_accept_display_overrides() -> ParityBatchCase {
    let elisp_form = r##"
(let ((nerd-icons-font-family "Parity Nerd Mono")
      (nerd-icons-scale-factor 1.0)
      (nerd-icons-color-icons t))
  (list
   :files
   (mapcar
    (lambda (file)
      (cons file (nerd-icons-test-describe (nerd-icons-icon-for-file file))))
    '("Cargo.toml" "src/main.rs" "README.md" ".gitignore"
      "archive.tar.gz" "Dockerfile" "notes.unknown"))
   :override
   (nerd-icons-test-describe
    (nerd-icons-icon-for-file "Cargo.toml"
                              :face 'nerd-icons-green
                              :height 1.5
                              :v-adjust -0.25))
   :extension-case-folding
   (nerd-icons-test-describe-many
    (list (nerd-icons-icon-for-extension "JSON")
          (nerd-icons-icon-for-extension "json")
          (nerd-icons-icon-for-extension nil)))))
"##;
    let expect = expect![[
        r####"OK (:files (("Cargo.toml" :glyph "" :codes (59304) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-yellow) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("src/main.rs" :glyph "" :codes (59304) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-maroon) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("README.md" :glyph "" :codes (62602) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-lblue) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) (".gitignore" :glyph "" :codes (58973) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-lred) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("archive.tar.gz" :glyph "" :codes (62577) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-lmaroon) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("Dockerfile" :glyph "" :codes (58960) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-blue) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("notes.unknown" :glyph "" :codes (61462) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) :override (:glyph "" :codes (59304) :length 1 :face (:family "Parity Nerd Mono" :height 1.5 :inherit nerd-icons-green) :font-lock-same t :display (raise -0.25) :rear-nonsticky t) :extension-case-folding ((:glyph "󰘦" :codes (984614) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-yellow) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) (:glyph "󰘦" :codes (984614) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-yellow) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) (:glyph "" :codes (61462) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)))"####
    ]];
    ParityBatchCase::value(
        "file_icons_apply_exact_name_before_extension_and_accept_display_overrides",
        elisp_form,
        expect,
    )
}

fn buffer_icons_choose_matching_file_types_then_fall_back_through_mode_parents() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((nerd-icons-font-family "Parity Nerd Mono")
      (auto-mode-alist '(("\\.rs\\'" . rust-mode)
                         ("\\.txt\\'" . text-mode))))
  (put 'nerd-icons-parity-log-mode 'derived-mode-parent 'text-mode)
  (list
   :matching-file
   (with-temp-buffer
     (setq buffer-file-name "/workspace/src/service.rs"
           major-mode 'rust-mode)
     (list :auto-match (and (nerd-icons-auto-mode-match?) t)
           :icon (nerd-icons-test-describe (nerd-icons-icon-for-buffer))))
   :mode-wins-on-mismatch
   (with-temp-buffer
     (setq buffer-file-name "/workspace/src/service.rs"
           major-mode 'text-mode)
     (list :auto-match (and (nerd-icons-auto-mode-match?) t)
           :icon (nerd-icons-test-describe (nerd-icons-icon-for-buffer))))
   :derived-mode
   (list :parents (nerd-icons--mode-parents 'nerd-icons-parity-log-mode)
         :icon
         (nerd-icons-test-describe
          (nerd-icons-icon-for-mode 'nerd-icons-parity-log-mode)))
   :unknown-mode
   (nerd-icons-test-describe
    (nerd-icons-icon-for-mode 'nerd-icons-parity-unknown-mode))))
"##;
    let expect = expect![[
        r####"OK (:matching-file (:auto-match t :icon (:glyph "" :codes (59304) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-maroon) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) :mode-wins-on-mismatch (:auto-match nil :icon (:glyph "" :codes (61788) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-cyan) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) :derived-mode (:parents (nerd-icons-parity-log-mode text-mode) :icon (:glyph "" :codes (61788) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-cyan) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) :unknown-mode (:glyph "" :codes (58930) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-dsilver) :font-lock-same t :display (raise 0.0) :rear-nonsticky t))"####
    ]];
    ParityBatchCase::value(
        "buffer_icons_choose_matching_file_types_then_fall_back_through_mode_parents",
        elisp_form,
        expect,
    )
}

fn directory_icons_distinguish_source_tests_repositories_and_generic_folders() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "nerd-icons-project-" t))
       (source (expand-file-name "src" root))
       (tests (expand-file-name "tests" root))
       (repository (expand-file-name "repository" root))
       (misc (expand-file-name "misc" root))
       (nerd-icons-font-family "Parity Nerd Mono"))
  (unwind-protect
      (progn
        (dolist (directory (list source tests repository misc))
          (make-directory directory t))
        (make-directory (expand-file-name ".git" repository) t)
        (list
         :source (nerd-icons-test-describe (nerd-icons-icon-for-dir source))
         :tests (nerd-icons-test-describe (nerd-icons-icon-for-dir tests))
         :repository
         (nerd-icons-test-describe (nerd-icons-icon-for-dir repository))
         :misc (nerd-icons-test-describe (nerd-icons-icon-for-dir misc))
         :override
         (nerd-icons-test-describe
          (nerd-icons-icon-for-dir source :height 1.25 :v-adjust 0.5))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r####"OK (:source (:glyph "" :codes (62543) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) :tests (:glyph "" :codes (62617) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) :repository (:glyph "" :codes (62465) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) :misc (:glyph "" :codes (59053) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) :override (:glyph "" :codes (62543) :length 1 :face (:family "Parity Nerd Mono" :height 1.25) :font-lock-same t :display (raise 0.5) :rear-nonsticky t))"####
    ]];
    ParityBatchCase::value(
        "directory_icons_distinguish_source_tests_repositories_and_generic_folders",
        elisp_form,
        expect,
    )
}

fn url_and_weather_dashboard_routes_specific_patterns_before_generic_fallbacks() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((nerd-icons-font-family "Parity Nerd Mono"))
  (list
   :urls
   (mapcar
    (lambda (url)
      (cons url (nerd-icons-test-describe (nerd-icons-icon-for-url url))))
    '("https://github.com/eval-exec/neomacs"
      "https://docs.example.org/manual.pdf"
      "https://unknown.example.org/home"))
   :weather
   (mapcar
   (lambda (condition)
      (let ((icon (nerd-icons-icon-for-weather condition)))
        (list condition
              (and icon (nerd-icons-test-describe icon)))))
    '("partly cloudy night" "rain and snow" "fair day"
      "not available" "volcanic ash"))))
"##;
    let expect = expect![[
        r####"OK (:urls (("https://github.com/eval-exec/neomacs" :glyph "" :codes (62472) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("https://docs.example.org/manual.pdf" :glyph "" :codes (60139) :length 1 :face (:family "Parity Nerd Mono" :height 1.0 :inherit nerd-icons-dred) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) ("https://unknown.example.org/home" :glyph "" :codes (61612) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) :weather (("partly cloudy night" (:glyph "" :codes (58233) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) ("rain and snow" (:glyph "" :codes (58134) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) ("fair day" (:glyph "" :codes (58179) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) ("not available" (:glyph "" :codes (58228) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t)) ("volcanic ash" nil)))"####
    ]];
    ParityBatchCase::value(
        "url_and_weather_dashboard_routes_specific_patterns_before_generic_fallbacks",
        elisp_form,
        expect,
    )
}

fn interactive_glyph_set_candidates_insert_the_selected_icon_with_properties() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((nerd-icons-font-family "Parity Nerd Mono")
       (candidates (nerd-icons--read-candidates-for-glyph-set 'codicon t))
       selected)
  (with-temp-buffer
    (cl-letf (((symbol-function 'completing-read)
               (lambda (_prompt collection &rest _)
                 (setq selected
                       (cl-find-if
                        (lambda (candidate)
                          (string-match-p
                           "nf-cod-home"
                           (substring-no-properties (car candidate))))
                        collection))
                 (car selected))))
      (nerd-icons-insert nil 'codicon))
    (let ((inserted (buffer-string)))
      (list
       :glyph-set (nerd-icons-codicon-glyph-set)
       :family (nerd-icons-codicon-family)
       :candidate-count (length candidates)
       :selected-label (substring-no-properties (car selected))
       :candidate-icon (nerd-icons-test-describe (cdr selected))
       :inserted (nerd-icons-test-describe inserted)
       :same (equal-including-properties inserted (cdr selected))))))
"##;
    let expect = expect![[
        r####"OK (:glyph-set "Codicons" :family "Parity Nerd Mono" :candidate-count 438 :selected-label "\11nf-cod-home" :candidate-icon (:glyph "" :codes (60166) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) :inserted (:glyph "" :codes (60166) :length 1 :face (:family "Parity Nerd Mono" :height 1.0) :font-lock-same t :display (raise 0.0) :rear-nonsticky t) :same t)"####
    ]];
    ParityBatchCase::value(
        "interactive_glyph_set_candidates_insert_the_selected_icon_with_properties",
        elisp_form,
        expect,
    )
}

fn cache_reuses_equal_argument_results_and_clears_after_the_configured_limit() -> ParityBatchCase {
    let elisp_form = r##"
(let ((nerd-icons-parity-call-count 0)
      (nerd-icons--cache-limit 1))
  (defun nerd-icons-parity-expensive (name &rest options)
    (setq nerd-icons-parity-call-count (1+ nerd-icons-parity-call-count))
    (list :name name :options options :call nerd-icons-parity-call-count))
  (put 'nerd-icons-parity-expensive 'nerd-icons--cached nil)
  (nerd-icons-cache #'nerd-icons-parity-expensive)
  (let ((alpha-1 (nerd-icons-parity-expensive "alpha" :height 1.0))
        (alpha-2 (nerd-icons-parity-expensive "alpha" :height 1.0))
        (beta (nerd-icons-parity-expensive "beta" :height 1.0))
        (gamma (nerd-icons-parity-expensive "gamma" :height 1.0))
        (alpha-3 (nerd-icons-parity-expensive "alpha" :height 1.0)))
    (list :values (mapcar #'copy-tree
                          (list alpha-1 alpha-2 beta gamma alpha-3))
          :same-object (eq alpha-1 alpha-2)
          :calls nerd-icons-parity-call-count
          :cached-property (get 'nerd-icons-parity-expensive
                                'nerd-icons--cached))))
"##;
    let expect = expect![[
        r####"OK (:values ((:name "alpha" :options (:height 1.0) :call 1) (:name "alpha" :options (:height 1.0) :call 1) (:name "beta" :options (:height 1.0) :call 2) (:name "gamma" :options (:height 1.0) :call 3) (:name "alpha" :options (:height 1.0) :call 4)) :same-object t :calls 4 :cached-property t)"####
    ]];
    ParityBatchCase::value(
        "cache_reuses_equal_argument_results_and_clears_after_the_configured_limit",
        elisp_form,
        expect,
    )
}

#[test]
fn nerd_icons_package_batch() {
    let cases = vec![
        styled_glyphs_preserve_family_scale_color_raise_and_property_boundaries(),
        file_icons_apply_exact_name_before_extension_and_accept_display_overrides(),
        buffer_icons_choose_matching_file_types_then_fall_back_through_mode_parents(),
        directory_icons_distinguish_source_tests_repositories_and_generic_folders(),
        url_and_weather_dashboard_routes_specific_patterns_before_generic_fallbacks(),
        interactive_glyph_set_candidates_insert_the_selected_icon_with_properties(),
        cache_reuses_equal_argument_results_and_clears_after_the_configured_limit(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Nerd Icons parity test");
    assert_oracle_batch_cases(nerd_icons_oracle(), test_name, "nerd_icons_parity", &cases);
}
