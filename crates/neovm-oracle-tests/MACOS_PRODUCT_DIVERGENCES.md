# macOS product divergences (GNU 31.1 NS oracle)

The macOS reference is now the **NS** build: pinned GNU 31.1 `a360712c9d27`
configured with `--with-ns --disable-ns-self-contained --without-x` (plus the
batch profile's lines) by `scripts/ci/build-gnu-oracle.sh`, attested by
`scripts/ci/check-gnu-oracle.sh` (`(featurep 'ns)`, the NS/TREE_SITTER/LCMS2/
RSVG/WEBP/KQUEUE features, the AppKit native image API and image types, and the
recorded configure line).  An NS build answers `--batch` without a display, so
this is a real GUI reference; the previous `macos-batch`
(`--without-x --without-ns`) oracle was window-system-less and compared a GUI
Neomacs against a batch GNU.

Re-baseline (2026-11-01, macmini, CI-recipe fresh-build tree): the 12,225-test
macOS oracle-parity selection passes unchanged against the NS oracle
(12,225/12,225; no stored expectation changes).  The complete combo (6,613),
divergence (6,742) and cus_start/defvar/fontset/image/face module families
(985) run in `NEOVM_ORACLE_MODE=live` against GNU NS leave **116 unique tests**
where Neomacs differs from GNU NS, grouped below by root cause.  No row is in
the STOP-AND-REPORT window-system-identity class: none of the 116 forms probes
`window-system`, `initial-window-system`, `framep` or `(featurep 'ns)`.

This file is the shrinking work queue: each fix commit deletes its rows.
Every row is a product divergence to fix; no row may be closed by
weakening, skipping or re-scoping a test, and no family is excluded from a
parity selection merely because it fails (user rule, 2026-11-01).  The
pre-NS batch-era rows are superseded (that oracle had no `syms_of_image`/
`syms_of_fontset` at all, so e.g. `image-types`/`x-bitmap-file-path` being void
said nothing about Neomacs's own GUI surface).

Reproduce a row: build the NS oracle with the recipe, then
`NEOVM_ORACLE_MODE=live NEOVM_FORCE_ORACLE_PATH=<ns-prefix>/bin/emacs cargo
nextest run -p neovm-oracle-tests --test oracle --lib -E 'test(=NAME)'`.

## A. NS face table: `ns-working-text-face` shifts every face ID by +1 (62 tests)

GNU source: `lisp/term/ns-win.el:297` `(defface ns-working-text-face ...)`, loaded by
`lisp/loadup.el:347-356` under `(featurep 'ns)`; the face is created during startup,
so every later face's id is one higher than a build without it.  Probed: GNU NS
face-list 189 / batch 188 / Neomacs 188; the only NS-extra face is
`ns-working-text-face`, and the vectors differ by exactly +1.
Fix class: host-keyed face table -- define the NS face in the same creation position.

| # | test | GNU NS | Neomacs |
|---|---|---|---|
| 1 | `oracle_divergence::divergence::face_id_matrix::div_face_id_eldoc_highlight_function_argument` | OK 185 | OK 184 |
| 2 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_ampersand` | OK 158 | OK 157 |
| 3 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_binding_variable` | OK 145 | OK 144 |
| 4 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_bound_variable` | OK 146 | OK 145 |
| 5 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_charset` | OK 170 | OK 169 |
| 6 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_coding` | OK 168 | OK 167 |
| 7 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_completion_category` | OK 172 | OK 171 |
| 8 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_completion_category_definition` | OK 173 | OK 172 |
| 9 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_condition` | OK 131 | OK 130 |
| 10 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_constant` | OK 159 | OK 158 |
| 11 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defcharset` | OK 171 | OK 170 |
| 12 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defcoding` | OK 169 | OK 168 |
| 13 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defface` | OK 163 | OK 162 |
| 14 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_deficon` | OK 165 | OK 164 |
| 15 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defmacro` | OK 161 | OK 160 |
| 16 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defoclosure` | OK 167 | OK 166 |
| 17 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defun` | OK 160 | OK 159 |
| 18 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_defvar` | OK 162 | OK 161 |
| 19 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_face` | OK 133 | OK 132 |
| 20 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_feature` | OK 142 | OK 141 |
| 21 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_free_variable` | OK 129 | OK 128 |
| 22 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_function` | OK 136 | OK 135 |
| 23 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_function_property_declaration` | OK 151 | OK 150 |
| 24 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_group` | OK 156 | OK 155 |
| 25 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_icon` | OK 164 | OK 163 |
| 26 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_macro` | OK 139 | OK 138 |
| 27 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_major_mode_name` | OK 132 | OK 131 |
| 28 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_nnoo_backend` | OK 157 | OK 156 |
| 29 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_non_local_exit` | OK 137 | OK 136 |
| 30 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_oclosure` | OK 166 | OK 165 |
| 31 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_rx` | OK 143 | OK 142 |
| 32 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_shadowed_variable` | OK 148 | OK 147 |
| 33 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_shadowing_variable` | OK 147 | OK 146 |
| 34 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_shorthand_font_lock_face` | OK 184 | OK 183 |
| 35 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_slot` | OK 153 | OK 152 |
| 36 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_special_form` | OK 140 | OK 139 |
| 37 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_special_variable_declaration` | OK 130 | OK 129 |
| 38 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_symbol_at_mouse` | OK 128 | OK 127 |
| 39 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_symbol_role` | OK 134 | OK 133 |
| 40 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_symbol_role_definition` | OK 135 | OK 134 |
| 41 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_theme` | OK 144 | OK 143 |
| 42 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_thing` | OK 152 | OK 151 |
| 43 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_throw_tag` | OK 141 | OK 140 |
| 44 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_type` | OK 155 | OK 154 |
| 45 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_unknown_call` | OK 138 | OK 137 |
| 46 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_variable_at_point` | OK 149 | OK 148 |
| 47 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_warning_type` | OK 150 | OK 149 |
| 48 | `oracle_divergence::divergence::face_id_matrix::div_face_id_elisp_widget_type` | OK 154 | OK 153 |
| 49 | `oracle_divergence::divergence::face_id_matrix::div_face_id_tooltip` | OK 186 | OK 185 |
| 50 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_conflict_state` | OK 179 | OK 178 |
| 51 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_edited_state` | OK 182 | OK 181 |
| 52 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_ignored_state` | OK 183 | OK 182 |
| 53 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_locally_added_state` | OK 178 | OK 177 |
| 54 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_locked_state` | OK 177 | OK 176 |
| 55 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_missing_state` | OK 181 | OK 180 |
| 56 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_needs_update_state` | OK 176 | OK 175 |
| 57 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_removed_state` | OK 180 | OK 179 |
| 58 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_state_base` | OK 174 | OK 173 |
| 59 | `oracle_divergence::divergence::face_id_matrix::div_face_id_vc_up_to_date_state` | OK 175 | OK 174 |
| 60 | `oracle_divergence::divergence::faces::div_face_list_count_and_known_faces` | OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspeci… | OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspeci… |
| 61 | `oracle_divergence_combo_complex::divergence_combo_complex::case_097::div_cx97_face_id_assignment_order` | OK (187 188 189 t t) | OK (186 187 188 t t) |
| 62 | `oracle_divergence_combo_complex::divergence_combo_complex::case_429::div_cx429_face_name_id` | OK ("neo-cx429-fnid" 187) | OK ("neo-cx429-fnid" 186) |

## B. NS Lisp preloads: `ucs-normalize` and the `utf-8-nfd`/`utf-8-hfs` coding systems (24 tests)

GNU source: `lisp/loadup.el:347-356` -- under `(featurep 'ns)` and `(featurep 'charprop)`
the NS dump loads `international/mule-util`, `international/ucs-normalize` and
`term/ns-win`; `lisp/international/ucs-normalize.el:687` defines `utf-8-hfs`, and
`lisp/term/ns-win.el:381` aliases `utf-8-nfd` onto it.  GNU NS therefore has
`ucs-normalize-string` (arity 1) and both coding systems preloaded; Neomacs has
neither (`void-function ucs-normalize-string`, coding-system-list lacks the two).
Fix class: build/host loadup -- preload the NS Lisp branch when the unidata
(`charprop`) is present, as GNU does.

| # | test | GNU NS | Neomacs |
|---|---|---|---|
| 63 | `oracle_divergence::divergence::coding_charset_deep::divergence_coding_system_list` | OK (t (utf-8 utf-8-with-signature utf-8-auto utf-8-emacs utf-16le utf-16be utf-16-le utf-16le-with-signature utf-16-be utf-16be-with-signature utf-16… | OK (t (utf-8 utf-8-with-signature utf-8-auto utf-8-emacs utf-16le utf-16be utf-16-le utf-16le-with-signature utf-16-be utf-16be-with-signature utf-16… |
| 64 | `oracle_divergence::divergence::utf8_charset_conv_deep::div_utf8_ucs_normalize_compose_decompose` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 65 | `oracle_divergence::divergence::utf8_charset_conv_deep::div_utf8_ucs_normalize_korean_hangul` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 66 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_already_normalized` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 67 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_combined_chars` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 68 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_empty_string` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 69 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_greek_diacritics` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 70 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_hangul_composition` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 71 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_idempotent` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 72 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_nfc_nfd_roundtrip_latin` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 73 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_nfkc_nfkd_compatibility` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 74 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_region_in_buffer` | OK (:errored wrong-number-of-arguments) | OK (:errored void-function) |
| 75 | `oracle_divergence_combo_complex::divergence_combo_complex::case_204::div_cx204_ucs_normalize_with_marker_overlay_undo_narrow_mega` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 76 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_already_normalized_ascii` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 77 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_combined_precomposed_decomposed` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 78 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_empty_string` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 79 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_greek_diacritics` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 80 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_hangul_composition` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 81 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_idempotent` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 82 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_nfc_nfd_roundtrip_latin` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 83 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_nfkc_nfkd_compatibility` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 84 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_region_in_buffer` | OK (:errored wrong-number-of-arguments) | OK (:errored void-function) |
| 85 | `oracle_divergence_combo_complex::divergence_combo_complex::case_356::div_cx356_ucs_normalize_with_marker_overlay_undo_narrow_mega` | ERR (wrong-number-of-arguments (1 . 1) 2) | ERR (void-function ucs-normalize-string) |
| 86 | `oracle_divergence_combo_complex::divergence_combo_complex::case_427::div_cx427_unicode_normalize` | OK ("café" "café" "1") | ERR (void-function ucs-normalize-NFD-string) |

## C. Host-keyed variable/function/doc tables (`ns-*` vs X11-only `x-*`) (17 tests)

GNU source: the NS build registers the NS names and omits the X11-only ones.
  - `src/nsterm.m:11568-11720`: `ns-alternate-modifier`, `ns-command-modifier`,
    `ns-control-modifier`, `ns-function-modifier`, `ns-antialias-text`,
    `ns-confirm-quit`, `ns-auto-hide-menu-bar`, `ns-use-native-fullscreen`,
    `ns-use-srgb-colorspace`, `ns-click-through`.
  - `src/nsselect.m:812`: `ns-sent-selection-hooks` (in the NS hook lists).
  - X11-only names Neomacs binds unconditionally: e.g. `x-scroll-event-delta-factor`
    (`src/xterm.c:32821`), `x-auto-preserve-selections` (`src/xterm.c:32964`),
    `x-gtk-use-system-tooltips` (X toolkit builds), `x-bitmap-file-path` is bound
    under NS too but with the recorded bitmapdir (`src/image.c:13268`).
Fix class: per-host platform tables with the GNU file:line for every name.

| # | test | GNU NS | Neomacs |
|---|---|---|---|
| 87 | `cus_start_platform_declarations::oracle_cus_start_platform_names_gnu_binds_here_carry_gnus_declaration` | ERR (void-variable x-gtk-use-system-tooltips) | OK ((window-combination-limit t window-size t t nil) (void-text-area-pointer t arrow t t nil) (x-bitmap-file-path t (".") t t nil) (x-gtk-use-system-… |
| 88 | `cus_start_platform_declarations::oracle_platform_names_bind_dynamically_under_let_like_gnu` | ERR (void-variable x-scroll-event-delta-factor) | OK ((x-bitmap-file-path . probe-marker) (x-scroll-event-delta-factor . probe-marker) (x-auto-preserve-selections . probe-marker) (vertical-centering-… |
| 89 | `cus_start_platform_declarations::oracle_platform_names_no_build_declares_have_no_documentation` | OK ((dos-hyper-key nil nil) (dos-keypad-mode nil nil) (dos-unsupported-char-glyph nil nil) (imagemagick-render-type nil nil) (ns-antialias-text t t) … | OK ((dos-hyper-key nil nil) (dos-keypad-mode nil nil) (dos-unsupported-char-glyph nil nil) (imagemagick-render-type nil nil) (ns-antialias-text nil n… |
| 90 | `cus_start_platform_declarations::oracle_x_gtk_use_system_tooltips_is_an_alias_onto_the_defvar_bool` | ERR (void-variable x-gtk-use-system-tooltips) | OK (use-system-tooltips t (t t) (nil nil) (t) t) |
| 91 | `cus_start_platform_declarations::oracle_x_platform_defaults_have_gnus_types_not_just_gnus_printed_form` | ERR (void-variable x-scroll-event-delta-factor) | OK (t t t t SECONDARY CLIPBOARD) |
| 92 | `defvar_bool_byte_boolean_vars::oracle_byte_boolean_vars_matches_gnus_declaration_set` | OK (108 use-default-font-for-symbols load-dangerous-libraries t t t t nil nil nil nil) | OK (116 font-use-system-font load-dangerous-libraries t t t t nil nil nil nil) |
| 93 | `defvar_bool_byte_boolean_vars::oracle_every_defvar_bool_variable_is_bound_and_canonical` | OK (146 (font-use-system-font x-dnd-disable-motif-drag x-dnd-disable-motif-protocol x-dnd-fix-motif-leave x-dnd-preserve-selection-data x-dnd-use-uns… | OK (146 nil nil nil) |
| 94 | `defvar_doc_skip_markers::oracle_defvars_inside_a_dead_preprocessor_branch_are_unbound` | OK ((nil nil) (nil nil nil)) | OK ((nil nil) (t t t)) |
| 95 | `defvar_doc_skip_markers::oracle_platform_duplicated_variables_carry_the_canonical_doc_text` | OK ((font-use-system-font) (next-selection-coding-system . "Coding system for the next communication with other programs.") (selection-coding-system … | OK ((font-use-system-font . "Non-nil means to apply the system defined font dynamically.") (next-selection-coding-system . "Coding system for the nex… |
| 96 | `defvar_int_declarations::oracle_every_gnu_defvar_int_name_is_bound_exactly_where_gnu_binds_it` | OK (74 49 (android-display-planes android-keyboard-bell-duration android-quit-keycode debug-end-pos dos-codepage dos-country-code dos-decimal-point d… | OK (74 52 (android-display-planes android-keyboard-bell-duration android-quit-keycode debug-end-pos dos-codepage dos-country-code dos-decimal-point d… |
| 97 | `defvar_int_declarations::oracle_every_remaining_gnu_defvar_int_is_bound_special_and_typed` | OK (8 (x-color-cache-bucket-size x-mouse-click-focus-ignore-time) nil nil) | OK (8 nil nil nil) |
| 98 | `defvar_int_declarations::oracle_platform_variables_gnu_does_not_declare_here_are_unbound` | OK ((ns-control-modifier ns-command-modifier ns-alternate-modifier ns-function-modifier ns-antialias-text ns-auto-hide-menu-bar ns-confirm-quit ns-us… | OK (nil t) |
| 99 | `defvar_int_declarations::oracle_remaining_defvar_int_initial_values_match_gnu` | ERR (void-variable x-color-cache-bucket-size) | OK (10000 128 500000 0 128 200 t t) |
| 100 | `oracle_divergence::divergence::advice_hooks_deep::divergence_find_file_hook` | OK (t t (find-file-hook find-file-hooks find-tag-hook first-change-hook focus-in-hook focus-out-hook font-lock-mode-hook font-lock-mode-off-hook font… | OK (t t (find-file-hook find-file-hooks find-tag-hook first-change-hook focus-in-hook focus-out-hook font-lock-mode-hook font-lock-mode-off-hook font… |
| 101 | `snarf_documentation_boundp_clause::oracle_the_c_variables_gnu_declares_here_are_declared_here_too` | OK ((long-line-threshold t "Line length above which to use redisplay shortcuts.") (frame-size-history t "History of frame size adjustments.") (intern… | OK ((long-line-threshold t "Line length above which to use redisplay shortcuts.") (frame-size-history t "History of frame size adjustments.") (intern… |
| 102 | `subr_doc_snarf_rules::oracle_defun_heads_a_regex_cannot_parse_still_have_gnus_documentation` | OK ((char-charset . "Return the charset of highest priority that contains CH.") (define-charset-internal . "For internal use only.") (define-coding-s… | OK ((char-charset . "Return the charset of highest priority that contains CH.") (define-charset-internal . "For internal use only.") (define-coding-s… |
| 103 | `subr_doc_snarf_rules::oracle_platform_duplicated_functions_carry_the_canonical_doc_text` | OK ((file-system-info . "Return storage information about the file system FILENAME is on.") (font-get-system-font) (font-get-system-normal-font) (men… | OK ((file-system-info . "Return storage information about the file system FILENAME is on.") (font-get-system-font . "Get the system default fixed wid… |

## D. `x-*`/`ns-*` C function and variable surface (9 tests)

GNU source: under NS, `src/nsfns.m` registers the x-* subset that NS implements
(`x-create-frame:1192`, `x-select-font:1688`, `x-server-vendor:1957`,
`x-display-screens:1985`, `x-open-connection:2111`, ...) with NS error text
("Nextstep windows are not in use or not initialized", `src/nsfns.m:113`), while
`src/xselect.c`/`src/xterm.c` names are absent (`x-get-atom-name`,
`src/xselect.c:3078`; `x-selection-owner-p`, `src/xselect.c:2712`; the
`x-lost-selection-functions`/`x-sent-selection-functions` variables).  NS uses
`src/nsselect.m:397-514` (`ns-own-selection-internal`, ...) instead.
Fix class: per-host function/variable tables.

| # | test | GNU NS | Neomacs |
|---|---|---|---|
| 104 | `oracle_divergence_combo_complex::divergence_combo_complex::case_230::div_cx230_x_get_selection_owns_p` | OK (nil nil) | OK (t nil) |
| 105 | `oracle_divergence_combo_complex::divergence_combo_complex::case_246::div_cx246_clipboard_manager_hooks` | OK (nil nil nil) | OK (nil t t) |
| 106 | `oracle_divergence_combo_complex::divergence_combo_complex::case_246::div_cx246_x_select_request_type` | OK (t nil) | OK (t t) |
| 107 | `oracle_divergence_combo_complex::divergence_combo_complex::case_246::div_cx246_x_selection_availability` | OK (t t nil nil nil) | OK (t t t t t) |
| 108 | `oracle_divergence_combo_complex::divergence_combo_complex::case_416::div_cx416_x_display_color` | ERR (error "Nextstep windows are not in use or not initialized") | ERR (error "X windows are not in use or not initialized") |
| 109 | `oracle_divergence_combo_complex::divergence_combo_complex::case_432::div_cx432_x_atom_ops` | OK (void-function void-function) | OK (void-function error) |
| 110 | `oracle_divergence_combo_complex::divergence_combo_complex::case_432::div_cx432_x_own_get_selection` | OK (void-function void-function) | OK (error wrong-number-of-arguments) |
| 111 | `oracle_divergence_combo_complex::divergence_combo_complex::case_432::div_cx432_x_send_client_message` | OK void-function | OK wrong-number-of-arguments |
| 112 | `oracle_divergence_combo_complex::divergence_combo_complex::case_432::div_cx432_x_window_property` | OK (void-function void-function) | OK (error error) |

## E. NS keymap/binding surface (2 tests)

GNU source: `lisp/term/ns-win.el:160-163` rebinds [home]/[end]/[kp-home]/[kp-end]
to `beginning-of-buffer`/`end-of-buffer` (bindings.el otherwise binds [home] to
`move-beginning-of-line`, `lisp/bindings.el:1396`), and the NS menu event
representation appears in `where-is-internal` output ([8388723] in the
save-buffer menu binding; `src/nsmenu.m`/`src/nsterm.m` event encoding).
Fix class: host-keyed keymaps.

| # | test | GNU NS | Neomacs |
|---|---|---|---|
| 113 | `oracle_divergence::divergence::default_bindings::div_db_function_and_arrow_keys` | OK (left-char right-char previous-line next-line beginning-of-buffer end-of-buffer scroll-down-command scroll-up-command) | OK (left-char right-char previous-line next-line move-beginning-of-line move-end-of-line scroll-down-command scroll-up-command) |
| 114 | `oracle_divergence::divergence::eieio_keymap_real::divergence_where_is_internal` | OK (([24 6] [open] [menu-bar file new-file]) ([24 19] [8388723] [menu-bar file save-buffer]) nil) | OK (([24 6] [open] [menu-bar file new-file]) ([24 19] [menu-bar file save-buffer]) nil) |

## F. X11-only pointer variables (1 tests)

GNU source: `x-pointer-shape` is a `DEFVAR_LISP` in `src/xfns.c:10327` (X11 only);
`x-sensitive-text-pointer-shape` likewise.  GNU NS leaves both unbound while the
generic selection functions are still fbound; Neomacs binds them on mac.
Fix class: per-host variable table.

| # | test | GNU NS | Neomacs |
|---|---|---|---|
| 115 | `oracle_divergence::divergence::frame_font_display::divergence_pointer_shape` | OK (nil nil t t) | OK (t t t t) |

## G. Process-filter delivery race (1 test; OPEN product investigation)

`case_007` (and the pre-NS `case_010` from the same batch) exercises a
process-filter setup.  Neo returned `(nil nil)` once in the 6,613-test
concurrent Live run where GNU returns `(t nil)`: Neo's first filter did not
observe the child's output.  The test is NOT weakened (no retry, timeout or
expectation change); the row is an open product investigation.

Evidence so far:
- GNU contract: process output is read only from the event-loop/wait paths
  and the filter is looked up at delivery time (`read_process_output`,
  src/process.c:6237, called from src/process.c:5579/6016;
  `set-process-filter`, src/process.c:1359, only stores it), and
  `status_notify` drains the remaining output before reporting the status
  (src/process.c around 7894/7923).
- Neomacs implements the same contract in the same shape: the process
  status/output code cites those GNU lines
  (system/process/status_notify.rs), and the helper's documented
  already-terminal drain works in a direct probe -- a child that exits
  before the filter is installed still delivers its pending output to the
  filter on the subsequent `accept-process-output`, identically under GNU
  (NS) and Neomacs.
- Reproduction attempts: 9/9 isolated runs (3 Live NS, 3 Verify NS, 3
  Verify batch); 64/64 concurrent nextest runs (16 parallel x 4 batches);
  the full combo family is re-run repeatedly to try to recapture.  No
  failure yet.
- Remaining uncertainty: a load-dependent ordering in the wait service
  (output-fd readiness vs child-exit status) that has not been reproduced.
  Fix class: product, but no change is made until a recurrence is captured
  (AGENTS.md: reproduce before fixing).  If it recurs, capture the exact
  status/fd ordering and fix the product; the test stays as is.
