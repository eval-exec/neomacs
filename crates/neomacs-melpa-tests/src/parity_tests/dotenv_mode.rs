use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DOTENV_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'dotenv-mode)

(defun neomacs-dotenv-test-face-spans ()
  "Return all semantically fontified spans in buffer order."
  (font-lock-ensure)
  (let ((position (point-min))
        spans)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list :range (list position next)
                      :text (buffer-substring-no-properties position next)
                      :face face)
                spans))
        (setq position next)))
    (nreverse spans)))

(defun neomacs-dotenv-test-syntax-at (needle offset)
  "Return syntax state OFFSET characters into NEEDLE."
  (goto-char (point-min))
  (search-forward needle)
  (let* ((start (- (point) (length needle)))
         (position (+ start offset))
         (state (syntax-ppss position)))
    (list :needle needle
          :position position
          :string (nth 3 state)
          :comment (nth 4 state)
          :start (nth 8 state))))
"###;

fn package_registration_exposes_dotenv_mode_state_and_safe_comment_customization() -> ParityBatchCase
{
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'dotenv-mode package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'dotenv-mode) t))
   :surface
   (list (fboundp 'dotenv-mode)
         (fboundp 'dotenv-mode-variables)
         (fboundp 'dotenv-mode--match-variables-in-double-quotes)
         (get 'dotenv-comment-column 'safe-local-variable))
   :mode
   (with-temp-buffer
     (dotenv-mode)
     (list :major major-mode
           :name mode-name
           :parent (and (derived-mode-p 'prog-mode) t)
           :comments
           (list comment-start comment-end comment-start-skip comment-column)
           :font-lock font-lock-defaults
           :syntax
           (mapcar (lambda (character)
                     (cons character (char-syntax character)))
                   '(?' ?\" ?# ?\n ?_ ?\\ ?$))))))
"###;
    let expected = expect![[
        r##"OK (:package (:name dotenv-mode :version "20191027.2129" :requirements ((emacs (24 3))) :feature t) :surface (t t t integerp) :mode (:major dotenv-mode :name ".env" :parent t :comments ("# " "" "#+ *" 32) :font-lock ((dotenv-mode-keywords)) :syntax ((39 . 34) (34 . 34) (35 . 60) (10 . 62) (95 . 95) (92 . 92) (36 . 39))))"##
    ]];
    ParityBatchCase::value(
        "package_registration_exposes_dotenv_mode_state_and_safe_comment_customization",
        elisp_form,
        expected,
    )
}

fn dotenv_and_example_files_select_the_mode_while_environment_variants_remain_opt_in()
-> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (path)
   (with-temp-buffer
     (setq buffer-file-name path)
     (set-auto-mode)
     (list path major-mode mode-name)))
 '("/work/service/.env"
   "/work/service/.env.example"
   "/work/service/app.env"
   "/work/service/app.env.example"
   "/work/service/.env.production"
   "/work/service/.envrc"))
"###;
    let expected = expect![[
        r#"OK (("/work/service/.env" dotenv-mode ".env") ("/work/service/.env.example" dotenv-mode ".env") ("/work/service/app.env" dotenv-mode ".env") ("/work/service/app.env.example" dotenv-mode ".env") ("/work/service/.env.production" fundamental-mode "Fundamental") ("/work/service/.envrc" fundamental-mode "Fundamental"))"#
    ]];
    ParityBatchCase::value(
        "dotenv_and_example_files_select_the_mode_while_environment_variants_remain_opt_in",
        elisp_form,
        expected,
    )
}

fn production_service_configuration_fontifies_exports_keys_interpolation_and_comments()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (dotenv-mode)
  (insert "# Production service\n"
          "export APP_ENV=production\n"
          "PORT: 8080\n"
          "DATABASE_URL=\"postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:5432/app\"\n"
          "CACHE_PREFIX='release-$APP_ENV'\n"
          "WORKER_COMMAND=\"$(which worker) --queues=$QUEUES\"\n"
          "PUBLIC_URL=https://example.com/$APP_ENV\n"
          "PASSWORD=FakeP@SSw0rd # rotated by operations\n"
          "HASHED=\"abc#def\"\n")
  (list :faces (neomacs-dotenv-test-face-spans)
        :lines (line-number-at-pos (point-max))))
"###;
    let expected = expect![[
        r##"OK (:faces ((:range (1 3) :text "# " :face font-lock-comment-delimiter-face) (:range (3 22) :text "Production service\n" :face font-lock-comment-face) (:range (22 28) :text "export" :face font-lock-keyword-face) (:range (29 36) :text "APP_ENV" :face font-lock-variable-name-face) (:range (48 52) :text "PORT" :face font-lock-variable-name-face) (:range (59 71) :text "DATABASE_URL" :face font-lock-variable-name-face) (:range (72 84) :text "\"postgres://" :face font-lock-string-face) (:range (84 85) :text "$" :face default) (:range (85 94) :text "{DB_USER}" :face font-lock-variable-name-face) (:range (94 95) :text ":" :face font-lock-string-face) (:range (95 96) :text "$" :face default) (:range (96 109) :text "{DB_PASSWORD}" :face font-lock-variable-name-face) (:range (109 110) :text "@" :face font-lock-string-face) (:range (110 111) :text "$" :face default) (:range (111 120) :text "{DB_HOST}" :face font-lock-variable-name-face) (:range (120 130) :text ":5432/app\"" :face font-lock-string-face) (:range (131 143) :text "CACHE_PREFIX" :face font-lock-variable-name-face) (:range (144 162) :text "'release-$APP_ENV'" :face font-lock-string-face) (:range (163 177) :text "WORKER_COMMAND" :face font-lock-variable-name-face) (:range (178 179) :text "\"" :face font-lock-string-face) (:range (179 180) :text "$" :face default) (:range (180 194) :text "(which worker)" :face font-lock-variable-name-face) (:range (194 204) :text " --queues=" :face font-lock-string-face) (:range (204 205) :text "$" :face default) (:range (205 211) :text "QUEUES" :face font-lock-variable-name-face) (:range (211 212) :text "\"" :face font-lock-string-face) (:range (213 223) :text "PUBLIC_URL" :face font-lock-variable-name-face) (:range (244 252) :text "$APP_ENV" :face font-lock-variable-name-face) (:range (253 261) :text "PASSWORD" :face font-lock-variable-name-face) (:range (275 277) :text "# " :face font-lock-comment-delimiter-face) (:range (277 299) :text "rotated by operations\n" :face font-lock-comment-face) (:range (299 305) :text "HASHED" :face font-lock-variable-name-face) (:range (306 315) :text "\"abc#def\"" :face font-lock-string-face)) :lines 10)"##
    ]];
    ParityBatchCase::value(
        "production_service_configuration_fontifies_exports_keys_interpolation_and_comments",
        elisp_form,
        expected,
    )
}

fn interpolation_matrix_distinguishes_double_single_unquoted_escaped_and_special_parameters()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (dotenv-mode)
  (insert "DOUBLE=\"hello $USER, ${REGION}, $(hostname), $@, $? and \\$ESCAPED\"\n"
          "SINGLE='hello $USER, ${REGION}, $(hostname)'\n"
          "UNQUOTED=$HOME/${APP_ROOT}/$(pwd)\n"
          "LITERAL=cost-$$-and-$9\n"
          "# ignored $COMMENT ${COMMENTED} $(ignored)\n")
  (neomacs-dotenv-test-face-spans))
"###;
    let expected = expect![[
        r##"OK ((:range (1 7) :text "DOUBLE" :face font-lock-variable-name-face) (:range (8 15) :text "\"hello " :face font-lock-string-face) (:range (15 16) :text "$" :face default) (:range (16 20) :text "USER" :face font-lock-variable-name-face) (:range (20 22) :text ", " :face font-lock-string-face) (:range (22 23) :text "$" :face default) (:range (23 31) :text "{REGION}" :face font-lock-variable-name-face) (:range (31 33) :text ", " :face font-lock-string-face) (:range (33 34) :text "$" :face default) (:range (34 44) :text "(hostname)" :face font-lock-variable-name-face) (:range (44 46) :text ", " :face font-lock-string-face) (:range (46 47) :text "$" :face default) (:range (47 48) :text "@" :face font-lock-variable-name-face) (:range (48 50) :text ", " :face font-lock-string-face) (:range (50 51) :text "$" :face default) (:range (51 52) :text "?" :face font-lock-variable-name-face) (:range (52 67) :text " and \\$ESCAPED\"" :face font-lock-string-face) (:range (68 74) :text "SINGLE" :face font-lock-variable-name-face) (:range (75 112) :text "'hello $USER, ${REGION}, $(hostname)'" :face font-lock-string-face) (:range (113 121) :text "UNQUOTED" :face font-lock-variable-name-face) (:range (122 127) :text "$HOME" :face font-lock-variable-name-face) (:range (128 139) :text "${APP_ROOT}" :face font-lock-variable-name-face) (:range (140 146) :text "$(pwd)" :face font-lock-variable-name-face) (:range (147 154) :text "LITERAL" :face font-lock-variable-name-face) (:range (170 172) :text "# " :face font-lock-comment-delimiter-face) (:range (172 213) :text "ignored $COMMENT ${COMMENTED} $(ignored)\n" :face font-lock-comment-face))"##
    ]];
    ParityBatchCase::value(
        "interpolation_matrix_distinguishes_double_single_unquoted_escaped_and_special_parameters",
        elisp_form,
        expected,
    )
}

fn hash_comment_syntax_respects_quotes_and_exposes_exact_parse_starts() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (dotenv-mode)
  (insert "PLAIN=value # deployment note\n"
          "DOUBLE=\"value#inside\" # outside double\n"
          "SINGLE='value#inside' # outside single\n"
          "ESCAPED=\"value\\\"#still-inside\" # final\n")
  (font-lock-ensure)
  (list
   :states
   (mapcar (lambda (probe)
             (neomacs-dotenv-test-syntax-at (car probe) (cdr probe)))
           '(("deployment note" . 3)
             ("value#inside" . 7)
             ("outside double" . 3)
             ("value#inside'" . 7)
             ("outside single" . 3)
             ("still-inside" . 3)
             ("final" . 2)))
   :faces (neomacs-dotenv-test-face-spans)))
"###;
    let expected = expect![[
        r##"OK (:states ((:needle "deployment note" :position 18 :string nil :comment t :start 13) (:needle "value#inside" :position 46 :string 34 :comment nil :start 38) (:needle "outside double" :position 58 :string nil :comment t :start 53) (:needle "value#inside'" :position 85 :string 39 :comment nil :start 77) (:needle "outside single" :position 97 :string nil :comment t :start 92) (:needle "still-inside" :position 129 :string 34 :comment nil :start 117) (:needle "final" :position 144 :string nil :comment t :start 140)) :faces ((:range (1 6) :text "PLAIN" :face font-lock-variable-name-face) (:range (13 15) :text "# " :face font-lock-comment-delimiter-face) (:range (15 31) :text "deployment note\n" :face font-lock-comment-face) (:range (31 37) :text "DOUBLE" :face font-lock-variable-name-face) (:range (38 52) :text "\"value#inside\"" :face font-lock-string-face) (:range (53 55) :text "# " :face font-lock-comment-delimiter-face) (:range (55 70) :text "outside double\n" :face font-lock-comment-face) (:range (70 76) :text "SINGLE" :face font-lock-variable-name-face) (:range (77 91) :text "'value#inside'" :face font-lock-string-face) (:range (92 94) :text "# " :face font-lock-comment-delimiter-face) (:range (94 109) :text "outside single\n" :face font-lock-comment-face) (:range (109 116) :text "ESCAPED" :face font-lock-variable-name-face) (:range (117 139) :text "\"value\\\"#still-inside\"" :face font-lock-string-face) (:range (140 142) :text "# " :face font-lock-comment-delimiter-face) (:range (142 148) :text "final\n" :face font-lock-comment-face)))"##
    ]];
    ParityBatchCase::value(
        "hash_comment_syntax_respects_quotes_and_exposes_exact_parse_starts",
        elisp_form,
        expected,
    )
}

fn changing_quote_style_and_variable_name_incrementally_updates_interpolation_highlighting()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (dotenv-mode)
  (insert "API_URL='https://${HOST}/v1'\n")
  (let ((single (neomacs-dotenv-test-face-spans)))
    (goto-char (point-min))
    (search-forward "'")
    (replace-match "\"")
    (search-forward "'")
    (replace-match "\"")
    (font-lock-flush)
    (let ((double (neomacs-dotenv-test-face-spans)))
      (goto-char (point-min))
      (search-forward "HOST")
      (replace-match "API_HOST")
      (font-lock-flush)
      (list :single single
            :double double
            :renamed (neomacs-dotenv-test-face-spans)
            :buffer (buffer-substring-no-properties
                     (point-min) (point-max))))))
"###;
    let expected = expect![[
        r#"OK (:single ((:range (1 8) :text "API_URL" :face font-lock-variable-name-face) (:range (9 29) :text "'https://${HOST}/v1'" :face font-lock-string-face)) :double ((:range (1 8) :text "API_URL" :face font-lock-variable-name-face) (:range (9 18) :text "\"https://" :face font-lock-string-face) (:range (18 19) :text "$" :face default) (:range (19 25) :text "{HOST}" :face font-lock-variable-name-face) (:range (25 29) :text "/v1\"" :face font-lock-string-face)) :renamed ((:range (1 8) :text "API_URL" :face font-lock-variable-name-face) (:range (9 18) :text "\"https://" :face font-lock-string-face) (:range (18 19) :text "$" :face default) (:range (19 29) :text "{API_HOST}" :face font-lock-variable-name-face) (:range (29 33) :text "/v1\"" :face font-lock-string-face)) :buffer "API_URL=\"https://${API_HOST}/v1\"\n")"#
    ]];
    ParityBatchCase::value(
        "changing_quote_style_and_variable_name_incrementally_updates_interpolation_highlighting",
        elisp_form,
        expected,
    )
}

fn commenting_secret_rotation_lines_round_trips_and_honors_the_configured_column() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (let ((dotenv-comment-column 28))
    (dotenv-mode))
  (insert "API_TOKEN=old-token\n"
          "DATABASE_PASSWORD=old-password\n"
          "ROTATED_AT=2026-08-05\n")
  (let ((configured-column comment-column))
    (comment-region (point-min) (point-max))
    (let ((commented (buffer-substring-no-properties
                      (point-min) (point-max))))
      (uncomment-region (point-min) (point-max))
      (let ((uncommented (buffer-substring-no-properties
                          (point-min) (point-max))))
        (goto-char (point-min))
        (end-of-line)
        (comment-indent)
        (insert "awaiting deployment")
        (list :column configured-column
              :commented commented
              :uncommented uncommented
              :inline (buffer-substring-no-properties
                       (point-min) (point-max))
              :comment-start-column
              (save-excursion
                (goto-char (point-min))
                (search-forward "#")
                (1- (current-column))))))))
"###;
    let expected = expect![[
        r##"OK (:column 28 :commented "# API_TOKEN=old-token\n# DATABASE_PASSWORD=old-password\n# ROTATED_AT=2026-08-05\n" :uncommented "API_TOKEN=old-token\nDATABASE_PASSWORD=old-password\nROTATED_AT=2026-08-05\n" :inline "API_TOKEN=old-token\11    # awaiting deployment\nDATABASE_PASSWORD=old-password\nROTATED_AT=2026-08-05\n" :comment-start-column 28)"##
    ]];
    ParityBatchCase::value(
        "commenting_secret_rotation_lines_round_trips_and_honors_the_configured_column",
        elisp_form,
        expected,
    )
}

fn mixed_assignment_forms_reveal_the_modes_exact_identifier_matching_boundaries() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (dotenv-mode)
  (insert "_PRIVATE_TOKEN=secret\n"
          "APP2_PORT=8080\n"
          "YAML_STYLE: enabled\n"
          "export EXPORTED_VALUE=ready\n"
          "export BAD_COLON: ignored\n"
          "2INVALID=value\n"
          "DASH-NAME=value\n"
          "EMPTY=\n")
  (neomacs-dotenv-test-face-spans))
"###;
    let expected = expect![[
        r#"OK ((:range (1 15) :text "_PRIVATE_TOKEN" :face font-lock-variable-name-face) (:range (23 32) :text "APP2_PORT" :face font-lock-variable-name-face) (:range (38 48) :text "YAML_STYLE" :face font-lock-variable-name-face) (:range (58 64) :text "export" :face font-lock-keyword-face) (:range (65 79) :text "EXPORTED_VALUE" :face font-lock-variable-name-face) (:range (86 92) :text "export" :face font-lock-keyword-face) (:range (113 120) :text "INVALID" :face font-lock-variable-name-face) (:range (132 136) :text "NAME" :face font-lock-variable-name-face) (:range (143 148) :text "EMPTY" :face font-lock-variable-name-face))"#
    ]];
    ParityBatchCase::value(
        "mixed_assignment_forms_reveal_the_modes_exact_identifier_matching_boundaries",
        elisp_form,
        expected,
    )
}

#[test]
fn dotenv_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(DOTENV_MODE_MELPA_PIN, "dotenv-mode.el")
            .expect("prepare revision-pinned Dotenv Mode source below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "dotenv-mode-package-batch",
        "Dotenv Mode",
        &[
            package_registration_exposes_dotenv_mode_state_and_safe_comment_customization(),
            dotenv_and_example_files_select_the_mode_while_environment_variants_remain_opt_in(),
            production_service_configuration_fontifies_exports_keys_interpolation_and_comments(),
            interpolation_matrix_distinguishes_double_single_unquoted_escaped_and_special_parameters(),
            hash_comment_syntax_respects_quotes_and_exposes_exact_parse_starts(),
            changing_quote_style_and_variable_name_incrementally_updates_interpolation_highlighting(),
            commenting_secret_rotation_lines_round_trips_and_honors_the_configured_column(),
            mixed_assignment_forms_reveal_the_modes_exact_identifier_matching_boundaries(),
        ],
    );
}
