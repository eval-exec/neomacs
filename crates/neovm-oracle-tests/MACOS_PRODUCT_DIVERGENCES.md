# macOS product divergences (GNU 31.1 oracle)

Tests whose Neomacs answer on macOS differs from the pinned GNU 31.1 macOS
reference.  Their families stay out of the macOS oracle-parity `-E` selection
until a product fix makes the Neomacs feature/face/frame tables follow the
host build; the test expectations themselves are already GNU-correct on both
references (or, for the B1 symbol pair, the GNU payload differs between the
two references and is recorded here).

Measurement: `NEOVM_ORACLE_MODE=live` on the macmini against GNU 31.1
`a360712c9d27` built with the macOS batch profile (`--without-x --without-ns`,
no image/font backends, kqueue, ACL), with Neomacs built by
`cargo xtask fresh-build --release` exactly as `neovm-core-tests-macos` does
(the 1,652-file byte-compiled Lisp tree).  Reproduce a row with
`NEOVM_ORACLE_MODE=live cargo nextest run -p neovm-oracle-tests --test oracle --lib
-E 'test(=NAME)'` and the same pinned-oracle environment.

Values are the `OK`/`ERR` payloads; long payloads are truncated at 170
characters with `…` (newlines shown as `\n`).  64 rows are the B3 Live-sweep
vectors, 3 are the B2 product-blocked (a) groups, 2 are the B1 groups, and 2
are the B1 symbol pair.

| # | family / test | GNU 31.1 macOS | Neomacs macOS | source |
|---|---|---|---|---|
| 1 | `defvar_int_declarations::oracle_platform_variables_gnu_does_not_declare_here_are_unbound` | OK (nil nil) | OK (nil t) | B3 live sweep |
| 2 | `cus_start_platform_declarations::oracle_x_platform_defaults_have_gnus_types_not_just_gnus_printed_form` | ERR (void-variable x-bitmap-file-path) | OK (t t t t SECONDARY CLIPBOARD) | B3 live sweep |
| 3 | `cus_start_platform_declarations::oracle_cus_start_platform_names_gnu_binds_here_carry_gnus_declaration` | ERR (void-variable x-bitmap-file-path) | OK ((window-combination-limit t window-size t t nil) (void-text-area-pointer t arrow t t nil) (x-bitmap-file-path t (".") t t nil) (x-gtk-use-system-tooltips t t t t nil… | B3 live sweep |
| 4 | `fontset_repertory::oracle_fontset_encoding_and_repertory_have_distinct_query_roles` | ERR (void-function set-fontset-font) | OK ((("Issue514 Repertory") ("Issue514 Repertory") nil) (nil nil nil) (("Issue514 Repertory") ("Issue514 Repertory") nil) (("Issue514 Repertory") ("Issue514 Repertory") … | B3 live sweep |
| 5 | `cus_start_platform_declarations::oracle_platform_names_bind_dynamically_under_let_like_gnu` | ERR (void-variable x-bitmap-file-path) | OK ((x-bitmap-file-path . probe-marker) (x-scroll-event-delta-factor . probe-marker) (x-auto-preserve-selections . probe-marker) (vertical-centering-font-regexp . probe-… | B3 live sweep |
| 6 | `defvar_int_declarations::oracle_every_remaining_gnu_defvar_int_is_bound_special_and_typed` | OK (8 (x-color-cache-bucket-size x-mouse-click-focus-ignore-time) nil nil) | OK (8 nil nil nil) | B3 live sweep |
| 7 | `defvar_doc_skip_markers::oracle_defvars_inside_a_dead_preprocessor_branch_are_unbound` | OK ((nil nil) (nil nil nil)) | OK ((nil nil) (t t t)) | B3 live sweep |
| 8 | `defvar_int_declarations::oracle_remaining_defvar_int_initial_values_match_gnu` | ERR (void-variable x-color-cache-bucket-size) | OK (10000 128 500000 0 128 200 t t) | B3 live sweep |
| 9 | `defvar_int_declarations::oracle_every_gnu_defvar_int_name_is_bound_exactly_where_gnu_binds_it` | OK (74 49 (android-display-planes android-keyboard-bell-duration android-quit-keycode debug-end-pos dos-codepage dos-country-code dos-decimal-point dos-hyper-key dos-key… | OK (74 52 (android-display-planes android-keyboard-bell-duration android-quit-keycode debug-end-pos dos-codepage dos-country-code dos-decimal-point dos-hyper-key dos-key… | B3 live sweep |
| 10 | `cus_start_platform_declarations::oracle_x_gtk_use_system_tooltips_is_an_alias_onto_the_defvar_bool` | ERR (void-variable x-gtk-use-system-tooltips) | OK (use-system-tooltips t (t t) (nil nil) (t) t) | B3 live sweep |
| 11 | `defvar_doc_skip_markers::oracle_platform_duplicated_variables_carry_the_canonical_doc_text` | OK ((font-use-system-font) (next-selection-coding-system . "Coding system for the next communication with other programs.") (selection-coding-system . "Coding system for… | OK ((font-use-system-font . "Non-nil means to apply the system defined font dynamically.") (next-selection-coding-system . "Coding system for the next communication with… | B3 live sweep |
| 12 | `remember_mouse_glyph_semantics::oracle_remember_mouse_glyph_validates_frame_and_coordinates` | OK ((void-function remember-mouse-glyph) (void-function remember-mouse-glyph) (void-function remember-mouse-glyph)) | OK ((wrong-type-argument frame-live-p "not-a-frame") (error "Window system frame should be used") (error "Window system frame should be used")) | B3 live sweep |
| 13 | `fontset_repertory::oracle_fontset_family_only_registry_preserves_query_encoding` | ERR (void-function set-fontset-font) | OK ((Issue514\ Family\ Only nil nil nil) (Issue514\ Family\ Only iso10646-1 ("Issue514 Family Only" . "iso10646-1") nil)) | B3 live sweep |
| 14 | `subr_doc_snarf_rules::oracle_platform_duplicated_functions_carry_the_canonical_doc_text` | OK ((file-system-info . "Return storage information about the file system FILENAME is on.") (font-get-system-font) (font-get-system-normal-font) (menu-or-popup-active-p)… | OK ((file-system-info . "Return storage information about the file system FILENAME is on.") (font-get-system-font . "Get the system default fixed width font.") (font-get… | B3 live sweep |
| 15 | `subr_doc_snarf_rules::oracle_defun_heads_a_regex_cannot_parse_still_have_gnus_documentation` | OK ((char-charset . "Return the charset of highest priority that contains CH.") (define-charset-internal . "For internal use only.") (define-coding-system-internal . "Fo… | OK ((char-charset . "Return the charset of highest priority that contains CH.") (define-charset-internal . "For internal use only.") (define-coding-system-internal . "Fo… | B3 live sweep |
| 16 | `fontset_repertory::oracle_fontset_range_script_and_charset_targets_preserve_metadata` | ERR (void-function set-fontset-font) | OK (((19968 . 20223) ("Issue514 Target" . "iso10646-1") nil nil) (han ("Issue514 Target" . "iso10646-1") ("Issue514 Target" . "iso10646-1") nil) (japanese-jisx0208 ("Iss… | B3 live sweep |
| 17 | `window_system_common_preload::oracle_term_common_win_is_preloaded_with_gnu_docstrings` | OK (((x-alternatives-map nil nil) (x-colors nil nil) (x-display-name nil nil) (emacs-save-session-functions nil nil)) nil nil nil) | OK (((x-alternatives-map t "Keymap of possible alternative meanings for some keys.") (x-colors t "List of basic colors available on color displays.") (x-display-name t "… | B3 live sweep |
| 18 | `oracle_divergence::divergence::advice_hooks_deep::divergence_find_file_hook` | OK (t t (find-file-hook find-file-hooks find-tag-hook first-change-hook focus-in-hook focus-out-hook font-lock-mode-hook font-lock-mode-off-hook font-lock-mode-on-hook g… | OK (t t (find-file-hook find-file-hooks find-tag-hook first-change-hook focus-in-hook focus-out-hook font-lock-mode-hook font-lock-mode-off-hook font-lock-mode-on-hook g… | B3 live sweep |
| 19 | `oracle_divergence::divergence::display_table_glyph::divergence_face_fonts` | OK (nil t t) | OK (t t t) | B3 live sweep |
| 20 | `oracle_divergence::divergence::face_color_font::divergence_font_match` | OK (nil nil nil nil t) | OK (nil nil nil t t) | B3 live sweep |
| 21 | `oracle_divergence::divergence::frame_display_info::divergence_x_display_info` | OK (nil nil nil nil nil) | OK (t t t t t) | B3 live sweep |
| 22 | `oracle_divergence::divergence::frame_font_display::divergence_cursor_appearance` | OK (t nil t t nil) | OK (t t t t nil) | B3 live sweep |
| 23 | `oracle_divergence::divergence::frame_font_display::divergence_font_spec` | OK (t t t nil t) | OK (t t t t t) | B3 live sweep |
| 24 | `oracle_divergence::divergence::frame_font_display::divergence_frame_font` | OK ("tty" "unspecified-fg" "unspecified-bg" nil nil) | OK ("tty" "unspecified-fg" "unspecified-bg" "white" nil) | B3 live sweep |
| 25 | `oracle_divergence::divergence::frame_font_display::divergence_pointer_shape` | OK (nil nil t t) | OK (t t t t) | B3 live sweep |
| 26 | `oracle_divergence::divergence::image_operations::divergence_image_cache_functions` | OK (nil nil nil nil) | OK (t t t nil) | B3 live sweep |
| 27 | `oracle_divergence::divergence::image_operations::divergence_image_formatting` | OK (nil nil nil nil) | OK (nil t t t) | B3 live sweep |
| 28 | `oracle_divergence::divergence::image_operations::divergence_image_scaling` | OK (nil nil nil) | OK (t nil t) | B3 live sweep |
| 29 | `oracle_divergence::divergence::image_operations::divergence_image_size_pixel` | OK (nil nil nil) | OK (nil nil t) | B3 live sweep |
| 30 | `oracle_divergence::divergence::image_operations::divergence_xpm_colors` | ERR (void-variable image-types) | OK ((xpm xbm pbm) nil (pbm)) | B3 live sweep |
| 31 | `oracle_divergence::divergence::image_operations::divergence_fringe_bitmaps` | OK (t nil nil nil) | OK (t t t t) | B3 live sweep |
| 32 | `oracle_divergence::divergence::image_svg_animation::divergence_animated_svg_explicit_nil_preserves_nonzero_index_matches_gnu` | ERR (error "Invalid image type ‘svg’") | OK (image svg nil 19 nil) | B3 live sweep |
| 33 | `oracle_divergence::divergence::image_operations::divergence_image_properties` | OK (nil nil nil nil) | OK (t t nil nil) | B3 live sweep |
| 34 | `oracle_divergence::divergence::image_operations::divergence_image_magick` | ERR (void-variable image-types) | OK (nil nil nil) | B3 live sweep |
| 35 | `oracle_divergence::divergence::image_svg_animation::divergence_animated_svg_static_by_default_matches_gnu` | ERR (error "Invalid image type ‘svg’") | OK (image svg nil 0 nil) | B3 live sweep |
| 36 | `oracle_divergence::divergence::image_svg_deep::divergence_image_cache` | OK (nil nil nil nil) | OK (t t t nil) | B3 live sweep |
| 37 | `oracle_divergence::divergence::image_svg_deep::divergence_image_map` | OK (nil nil nil nil) | OK (nil t nil nil) | B3 live sweep |
| 38 | `oracle_divergence::divergence::image_svg_deep::divergence_image_create` | OK (t t nil nil) | OK (t t t t) | B3 live sweep |
| 39 | `oracle_divergence::divergence::image_svg_deep::divergence_image_transforms` | OK (nil nil nil nil) | OK (t nil nil t) | B3 live sweep |
| 40 | `oracle_divergence::divergence::image_svg_deep::divergence_image_types` | ERR (void-variable image-types) | OK (t t t t) | B3 live sweep |
| 41 | `oracle_divergence::divergence::image_svg_deep::divergence_image_properties` | OK (nil t nil nil) | OK (t t nil t) | B3 live sweep |
| 42 | `oracle_divergence::divergence::image_widget_display::divergence_image_functions` | OK (t t t t nil) | OK (t t t t t) | B3 live sweep |
| 43 | `oracle_divergence::divergence::image_widget_display::divergence_tool_bar` | OK (t t nil nil) | OK (t t t t) | B3 live sweep |
| 44 | `oracle_divergence::divergence::image_widget_display::divergence_image_cache` | ERR (void-variable image-cache-eviction-delay) | OK (t t) | B3 live sweep |
| 45 | `oracle_divergence::divergence::image_widget_display::divergence_image_types` | ERR (void-variable image-types) | OK (t t (png gif tiff jpeg xpm xbm pbm) (svg webp png gif tiff jpeg xpm xbm pbm)) | B3 live sweep |
| 46 | `oracle_divergence::divergence::register_narrow_misc::divergence_fringe_indicator` | ERR (void-variable overflow-newline-into-fringe) | OK (t t t t) | B3 live sweep |
| 47 | `oracle_divergence::divergence::ui_interaction::divergence_drag_n_drop` | OK (nil nil) | OK (t t) | B3 live sweep |
| 48 | `oracle_divergence::divergence::ui_interaction::divergence_scroll_bar` | OK (nil nil nil) | OK (t t t) | B3 live sweep |
| 49 | `oracle_divergence::divergence::ui_interaction::divergence_tool_bar` | OK (t t nil) | OK (t t t) | B3 live sweep |
| 50 | `oracle_divergence::divergence::window_config_deep::divergence_window_margins` | ERR (void-variable fringe-mode) | OK (t t t t t (nil no-fringe minimal default)) | B3 live sweep |
| 51 | `oracle_divergence_combo_complex::divergence_combo_complex::case_012::div_cx12_font_info_basic` | OK (:errored void-function) | OK (:errored wrong-type-argument) | B3 live sweep |
| 52 | `oracle_divergence_combo_complex::divergence_combo_complex::case_012::div_cx12_fontset_info` | OK (t nil nil) | OK (t t t) | B3 live sweep |
| 53 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_fringe_bitmap_availability_matrix` | OK (:errored void-function) | OK ((left-arrow 3) (right-arrow 4) (up-arrow 5) (down-arrow 6) (filled-square 20) (hollow-square 21) (left-triangle 10) (right-triangle 11) (question-mark 1) (exclamatio… | B3 live sweep |
| 54 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_image_animated_p_predicate` | OK (:errored error) | OK (t :err) | B3 live sweep |
| 55 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_image_format_available_p_per_format` | OK (nil t nil nil nil) | OK (t t t t t) | B3 live sweep |
| 56 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_image_type_availability_matrix` | OK (nil nil nil nil nil nil nil) | OK (t t t t t t t) | B3 live sweep |
| 57 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_image_scale_factor_and_multipliers` | OK (:errored error) | OK (t 2) | B3 live sweep |
| 58 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_image_metadata_creation` | OK (:errored error) | OK (t image png "non-existent.png") | B3 live sweep |
| 59 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_put_text_property_with_image_display` | ERR (error "Invalid image type ‘png’") | OK ((image :type png :file "non-existent.png" :scale default) nil nil) | B3 live sweep |
| 60 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_image_with_marker_overlay_narrow_mega` | OK (:errored error) | OK (:errored args-out-of-range) | B3 live sweep |
| 61 | `oracle_divergence_combo_complex::divergence_combo_complex::case_124::div_cx124_event_basic_type_resolution` | OK ((97 97) (C-a nil) ((control 97) nil) (M-a nil) (C-M-a nil) (return return) (C-return nil) (M-return nil) (mouse-1 mouse-1) (M-mouse-1 mouse-1) (C-down-mouse-1 mouse-… | OK ((97 97) (C-a nil) ((control 97) nil) (M-a nil) (C-M-a nil) (return return) (C-return nil) (M-return return) (mouse-1 mouse-1) (M-mouse-1 mouse-1) (C-down-mouse-1 mou… | B3 live sweep |
| 62 | `oracle_divergence_combo_complex::divergence_combo_complex::case_124::div_cx124_mouse_wheel_availability` | OK (nil nil nil nil) | OK (t t t t) | B3 live sweep |
| 63 | `oracle_divergence_combo_complex::divergence_combo_complex::case_124::div_cx124_tool_bar_availability` | OK (t t nil t) | OK (t t t t) | B3 live sweep |
| 64 | `oracle_divergence_combo_complex::divergence_combo_complex::case_098::div_cx98_svg_creation_simple` | OK (:errored error) | OK (t image svg) | B3 live sweep |
| 65 | `oracle_divergence::divergence::face_attributes_matrix::div_face_attr_fill_column_indicator` | OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . normal) (:slant . normal) (:underline) (:overline) (:exten… | OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . normal) (:slant . normal) (:underline) (:overline) (:exten… | B2 |
| 66 | `oracle_divergence::divergence::window_frame::div_wf_frame_parameters_common` | OK ("F1" 80 25 "unspecified-fg" "unspecified-bg" nil nil nil "tty" t 1 nil nil t nil nil nil dark mono) | OK ("F1" 80 25 "unspecified-fg" "unspecified-bg" "white" nil nil "tty" t 1 nil nil t nil nil nil dark mono) | B2 |
| 67 | `oracle_divergence_combo_complex::divergence_combo_complex::case_123::div_cx123_frame_font_query` | OK ("tty" nil nil (font . "tty")) | OK ("tty" nil 0 (font . "tty")) | B2 |
| 68 | `defvar_bool_byte_boolean_vars::oracle_byte_boolean_vars_matches_gnus_declaration_set` | OK (96 query-all-font-backends load-dangerous-libraries t t t t nil nil nil nil) | OK (116 font-use-system-font load-dangerous-libraries t t t t nil nil nil nil) | B1 |
| 69 | `snarf_documentation_boundp_clause::oracle_the_c_variables_gnu_declares_here_are_declared_here_too` | OK: alternate-fontname-alist, text-conversion-face, x-resource-class, xft-settings, x-max-tooltip-size, x-no-window-manager, x-wait-for-event-timeout are doc-nil/unbound | OK: all seven are bound and documented (Neo documents them) | B1 |
| 70 | `symbol::issue554_final_boundaries::issue554_fringe_alignment_and_frame_symbol_checks_follow_mode` | OK ((t wrong-type-argument) (t void-variable)) | OK ((error wrong-type-argument) (t void-variable)) | B1 symbol |
| 71 | `symbol::issue554_final_boundaries::issue554_image_library_type_follows_eq_policy` | ERR (void-function init-image-library) | OK (nil t) | B1 symbol |
