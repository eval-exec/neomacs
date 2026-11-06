//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

use neovm_text::emacs_core::chartable::{make_char_table_value, make_char_table_with_extra_slots};
use neovm_value::intern::intern;
use neovm_value::symbol::LispVariableLocality;
use neovm_value::value::Value;

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    fn defvar_buffer_local(obarray: &mut neovm_value::symbol::Obarray, name: &str, default: Value) {
        obarray.define_lisp_variable(name, default, LispVariableLocality::BufferLocalIfSet);
    }

    obarray.seed_value("inhibit-redisplay", Value::NIL);
    obarray.make_special("inhibit-redisplay");
    // The five `syms_of_xdisp' DEFVAR_LISPs entry 173's sweep found this port
    // short of.  Each carries GNU's own initializer; none is a nil placeholder
    // standing in for one.
    //
    // xdisp.c:39191 DEFVAR_LISP, `Vdebug_on_message = Qnil'.
    obarray.define_special_variable("debug-on-message", Value::NIL);
    // xdisp.c:38549 DEFVAR_LISP, `Vdisplay_pixels_per_inch = make_float (72.0)'
    // -- a float, not the fixnum 72: `default_pixels_per_inch_x' reads it with
    // `XFLOATINT' after a `NUMBERP' test, and `frame-char-width' arithmetic
    // divides by it.
    obarray.define_special_variable("display-pixels-per-inch", Value::make_float(72.0));
    // xdisp.c:38910 DEFVAR_LISP, `Vmenu_updating_frame = Qnil'.
    obarray.define_special_variable("menu-updating-frame", Value::NIL);
    // xdisp.c:39225 / 39230 DEFVAR_LISP, both `Fmake_hash_table (0, NULL)' --
    // an ordinary `eql' table, which `redisplay_internal' fills with cause
    // counters when `redisplay--variables' is instrumented.  An empty table is
    // not the same value as nil: `puthash' on nil signals.
    obarray.define_special_variable(
        "redisplay--all-windows-cause",
        Value::hash_table(neovm_value::value::HashTableTest::Eql),
    );
    obarray.define_special_variable(
        "redisplay--mode-lines-cause",
        Value::hash_table(neovm_value::value::HashTableTest::Eql),
    );
    // GNU xdisp.c `DEFVAR_LISP ("special-mirror-table", Vspecial_mirror_table)`:
    // a char-table of characters bidi display mirrors specially (paired
    // punctuation such as ¶<->‹). GNU inits it to an empty char-table
    // (`Vspecial_mirror_table = Fmake_char_table (Qnil, Qnil)`);
    // international/characters.el populates it and the redisplay bidi path reads
    // it via CHAR_TABLE_REF. New in 31.0.90 (absent from the 705c0e3 base), so
    // it must be defined for characters.el to load.
    obarray.seed_value(
        "special-mirror-table",
        Value::make_char_table(Value::NIL, Value::NIL, 0),
    );
    obarray.make_special("special-mirror-table");
    obarray.seed_value("blink-matching-delay", Value::fixnum(1));
    obarray.seed_value("blink-matching-paren", Value::T);
    obarray.seed_value("mouse-autoselect-window", Value::NIL);
    // xdisp.c:38695-38795 tab/tool bar DEFVARs (values are GNU's C inits:
    // DEFAULT_TAB_BAR_BUTTON_MARGIN 1 / _RELIEF 1, DEFAULT_TOOL_BAR_BUTTON_MARGIN 4
    // / _RELIEF 1, DEFAULT_TOOL_BAR_LABEL_SIZE 14 -- dispextern.h:3419-3499).
    obarray.define_special_variable("auto-resize-tab-bars", Value::T);
    obarray.define_special_variable("auto-resize-tool-bars", Value::T);
    obarray.define_special_variable("tab-bar-border", Value::symbol("internal-border-width"));
    obarray.define_special_variable("tab-bar-button-margin", Value::fixnum(1));
    obarray.define_int_variable("tab-bar-button-relief", 1);
    obarray.define_special_variable("tool-bar-border", Value::symbol("internal-border-width"));
    obarray.define_special_variable("tool-bar-button-margin", Value::fixnum(4));
    obarray.define_int_variable("tool-bar-button-relief", 1);
    obarray.define_int_variable("tool-bar-max-label-size", 14);
    obarray.seed_value("tool-bar-style", Value::NIL);
    obarray.seed_value("global-font-lock-mode", Value::NIL);
    // GNU xdisp.c registers these as DEFVAR_LISP/INT/BOOL variables and
    // calls Fmake_variable_buffer_local for the variables documented as
    // buffer-local. In particular, `display-line-numbers-mode' relies on
    // `display-line-numbers' being local-if-set so enabling it in one buffer
    // does not mutate the global default.
    defvar_buffer_local(obarray, "wrap-prefix", Value::NIL);
    defvar_buffer_local(obarray, "line-prefix", Value::NIL);
    defvar_buffer_local(obarray, "display-line-numbers", Value::NIL);
    defvar_buffer_local(obarray, "display-line-numbers-width", Value::NIL);
    obarray.seed_value("display-line-numbers-current-absolute", Value::T);
    obarray.make_special("display-line-numbers-current-absolute");
    defvar_buffer_local(obarray, "display-line-numbers-widen", Value::NIL);
    // `display-line-numbers-offset' is BOTH, and in this order
    // (`src/xdisp.c:38999-39005'): `DEFVAR_INT' first, then
    // `Fmake_variable_buffer_local'.  `make_blv' copies the descriptor into the
    // BLV (`src/data.c:2112-2140'), so the integer rule applies to a per-buffer
    // binding as well as to the default -- `(setq-local
    // display-line-numbers-offset "x")' is `(wrong-type-argument integerp "x")'
    // in GNU.  Registering it as a plain buffer-local cell got the locality and
    // dropped the type.
    obarray.define_int_variable("display-line-numbers-offset", 0);
    obarray.make_buffer_local("display-line-numbers-offset", true);
    defvar_buffer_local(obarray, "display-fill-column-indicator", Value::NIL);
    // GNU `src/xdisp.c:38644-38652` defines this with DEFVAR_LISP,
    // initializes it to Qt, then calls Fmake_variable_buffer_local.
    defvar_buffer_local(obarray, "display-fill-column-indicator-column", Value::T);
    defvar_buffer_local(
        obarray,
        "display-fill-column-indicator-character",
        Value::NIL,
    );
    // xdisp.c:38558 DEFVAR_LISP, make_fixnum (50).
    obarray.define_special_variable("truncate-partial-width-windows", Value::fixnum(50));
    obarray.seed_value("line-number-display-limit", Value::NIL);
    // xdisp.c:38514 / 38521 DEFVAR_INT, init 0.
    obarray.define_int_variable("scroll-step", 0);
    obarray.define_int_variable("scroll-conservatively", 0);
    // xdisp.c:38535 DEFVAR_INT, init 0.
    obarray.define_int_variable("scroll-margin", 0);
    // xdisp.c:38541 DEFVAR_LISP, make_float (0.25) -- a float, not a fixnum.
    obarray.define_special_variable("maximum-scroll-margin", Value::make_float(0.25));
    // xdisp.c:38875 DEFVAR_INT, init 5.
    obarray.define_int_variable("hscroll-margin", 5);
    // xdisp.c:38880 DEFVAR_LISP, make_fixnum (0).
    obarray.define_special_variable("hscroll-step", Value::fixnum(0));
    obarray.seed_value("auto-hscroll-mode", Value::T);
    // xdisp.c:38479 DEFVAR_LISP, init Qarrow.
    obarray.define_special_variable("void-text-area-pointer", Value::symbol("arrow"));
    // `inhibit-message' and every other GNU `DEFVAR_BOOL' variable are
    // registered from `defvar_bool::GNU_BOOL_VARIABLES', which is where the
    // declaration's `byte-boolean-vars' visibility lives too.
    obarray.seed_value("make-cursor-line-fully-visible", Value::T);
    // GNU `src/xdisp.c:38708` (`DEFVAR_BOOL ("inhibit-try-cursor-movement", ...)`)
    // controls the `try_cursor_movement` redisplay optimization. neomacs has
    // no equivalent optimization (the layout engine recomputes per frame),
    // so this knob is currently inert — but the symbol must exist so Lisp
    // code that does `(boundp 'inhibit-try-cursor-movement)` or
    // `(setq inhibit-try-cursor-movement ...)` does not raise void-variable.
    // Cursor audit Finding 7 in `drafts/cursor-audit.md`.
    obarray.define_special_variable("inhibit-try-cursor-movement", Value::NIL);
    obarray.seed_value("show-trailing-whitespace", Value::NIL);
    obarray.make_special("show-trailing-whitespace");
    obarray.make_buffer_local("show-trailing-whitespace", true);
    // `syms_of_xdisp` calls `Fmake_variable_buffer_local' on this one right
    // after its `DEFVAR_BOOL' (`src/xdisp.c:38731-38735'); the declaration
    // itself lives in `defvar_bool::GNU_BOOL_VARIABLES', which has already
    // run, so the BLV inherits the Boolean forwarder the way `make_blv' does.
    obarray.make_buffer_local("make-window-start-visible", true);
    obarray.seed_value("show-paren-context-when-offscreen", Value::NIL);
    // xdisp.c:38443 DEFVAR_LISP, init Qt.
    obarray.define_special_variable("nobreak-char-display", Value::T);
    // GNU inits this to `(overlay-arrow-position)` (xdisp.c: `Voverlay_arrow_variable_list
    // = list1 (intern_c_string ("overlay-arrow-position"))`), so the plain
    // `overlay-arrow-position` marker (used by e.g. gud) is scanned by redisplay.
    obarray.define_lisp_variable(
        "overlay-arrow-variable-list",
        Value::cons(Value::symbol("overlay-arrow-position"), Value::NIL),
        LispVariableLocality::Global,
    );
    obarray.define_lisp_variable(
        "overlay-arrow-string",
        Value::string("=>"),
        LispVariableLocality::Global,
    );
    obarray.define_lisp_variable(
        "overlay-arrow-position",
        Value::NIL,
        LispVariableLocality::Global,
    );
    // Mirror GNU Emacs: set char-table-extra-slots property for all subtypes
    // that need extra slots. Fmake_char_table reads this property to allocate
    // the correct number of extra slots.
    // See: casetab.c:249, category.c:426, character.c:1143, coding.c:11737,
    //      fontset.c:2158-2160, xdisp.c:31594, keymap.c:3346, syntax.c:3659
    obarray
        .put_property("case-table", "char-table-extra-slots", Value::fixnum(3))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("category-table", "char-table-extra-slots", Value::fixnum(2))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property(
            "char-script-table",
            "char-table-extra-slots",
            Value::fixnum(1),
        )
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property(
            "translation-table",
            "char-table-extra-slots",
            Value::fixnum(2),
        )
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("fontset", "char-table-extra-slots", Value::fixnum(8))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("fontset-info", "char-table-extra-slots", Value::fixnum(1))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property(
            "glyphless-char-display",
            "char-table-extra-slots",
            Value::fixnum(1),
        )
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("keymap", "char-table-extra-slots", Value::fixnum(0))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray
        .put_property("syntax-table", "char-table-extra-slots", Value::fixnum(0))
        .expect("char-table-extra-slots plist should always be valid during init");
    obarray.seed_value(
        "char-script-table",
        make_char_table_with_extra_slots(Value::symbol("char-script-table"), Value::NIL, 1),
    );
    // GNU DEFVAR_LISP (src/character.c:1138): a special, so a lexical-binding
    // `let` of it binds dynamically and internal matcher reads see it.
    obarray.make_special("char-script-table");
    // GNU's C default for `pre-redisplay-function` is `ignore` (xdisp.c:39133),
    // NOT nil. simple.el upgrades it to `redisplay--pre-redisplay-functions`
    // (the driver of the `pre-redisplay-functions` hook) ONLY when it still
    // equals `ignore` (simple.el:7352). Initialising it to nil here made that
    // guard fail, so the driver was never installed and `pre-redisplay-functions`
    // (hl-line with sticky 'window, the region overlay, …) never ran.
    obarray.define_special_variable("pre-redisplay-function", Value::symbol("ignore"));
    // xdisp.c:38835 DEFVAR_LISP: contrary to the docstring, GNU initializes
    // this to nil so loadup.el does not try to resize windows before
    // window.el is loaded; loadup.el:142 assigns `grow-only' right after.
    obarray.define_special_variable("resize-mini-windows", Value::NIL);
    // xdisp.c:38387 DEFVAR_LISP, build_string ("*Messages*").
    obarray.define_special_variable("messages-buffer-name", Value::string("*Messages*"));
    // xdisp.c:38602 DEFVAR_INT, init 200.
    obarray.define_int_variable("line-number-display-limit-width", 200);
    // xdisp.c:39086 / 39092 DEFVAR_INT, init 2 / 1.
    obarray.define_int_variable("overline-margin", 2);
    obarray.define_int_variable("underline-minimum-offset", 1);
    // xdisp.c:39108 DEFVAR_LISP, make_fixnum (DEFAULT_HOURGLASS_DELAY) = 1.
    obarray.define_special_variable("hourglass-delay", Value::fixnum(1));
    // xdisp.c:38827 DEFVAR_LISP, make_float (0.25).
    obarray.define_special_variable("max-mini-window-height", Value::make_float(0.25));
    // Do NOT pre-bind the *plural* `pre-redisplay-functions`: it is a pure lisp
    // defvar (simple.el) whose default is `(redisplay--update-region-highlight)`
    // — the function that creates the active-region highlight overlay. Binding it
    // to nil here shadowed that defvar, so the region overlay was never created
    // (and `global-hl-line-mode` then `add-hook`'d onto an empty list). Leaving
    // it unbound lets simple.el install GNU's default.
    obarray.define_int_variable("display-line-numbers-major-tick", 0);
    obarray.define_int_variable("display-line-numbers-minor-tick", 0);
    // xdisp.c:39295 DEFVAR_INT, init 0 ("The default value is zero, which
    // disables this feature").
    obarray.define_int_variable("max-redisplay-ticks", 0);
    // GNU `src/xdisp.c:38428-38438` defines this with DEFVAR_LISP,
    // initializes it to nil, then calls Fmake_variable_buffer_local.
    // `jit-lock.el` installs `jit-lock-function` here buffer-locally.
    {
        let id = intern("fontification-functions");
        obarray.seed_value("fontification-functions", Value::NIL);
        obarray.make_special("fontification-functions");
        obarray.make_symbol_localized(id, Value::NIL);
        obarray.set_blv_local_if_set(id, true);
    }

    // auto-fill-chars: a char-table for characters which invoke auto-filling.
    // Official Emacs (character.c) creates it with sub-type `auto-fill-chars`
    // and sets space and newline to t.
    let auto_fill = make_char_table_value(Value::symbol("auto-fill-chars"), Value::NIL);
    // Set space and newline entries to t.  We use set-char-table-range
    // via the underlying data: store single-char entries.
    use neovm_text::emacs_core::chartable::ct_set_single;
    ct_set_single(&auto_fill, ' ' as i64, Value::T);
    ct_set_single(&auto_fill, '\n' as i64, Value::T);
    // character.c:1104 DEFVAR_LISP -- special like every C DEFVAR.
    obarray.define_special_variable("auto-fill-chars", auto_fill);

    // char-width-table: a char-table for character display widths.
    // GNU character.c:1112-1119 seeds default 1 plus C1/raw-byte width 4.
    // Include characters.el Unicode ranges for contexts before Lisp bootstrap.
    obarray.seed_value(
        "char-width-table",
        neovm_text::encoding::default_char_width_table(),
    );

    // translation-table-vector: vector recording all translation tables.
    // Official Emacs (character.c) creates a 16-element nil vector.
    obarray.seed_value(
        "translation-table-vector",
        Value::vector(vec![Value::NIL; 16]),
    );

    // translation-hash-table-vector: vector of translation hash tables.
    // Official Emacs (ccl.c:2382 DEFVAR_LISP) initializes to nil.
    obarray.define_special_variable("translation-hash-table-vector", Value::NIL);

    // printable-chars: a char-table of printable characters.
    // Official Emacs (character.c) creates it with default t.
    obarray.seed_value(
        "printable-chars",
        make_char_table_value(Value::symbol("printable-chars"), Value::T),
    );

    // default-process-coding-system: cons of coding systems for process I/O.
    // Official Emacs (coding.c:12139 DEFVAR_LISP) initializes to nil.
    obarray.define_special_variable("default-process-coding-system", Value::NIL);

    // ambiguous-width-chars: char-table for characters whose width can be 1 or 2.
    // Official Emacs (character.c) creates empty char-table; populated by characters.el.
    obarray.seed_value(
        "ambiguous-width-chars",
        make_char_table_value(Value::NIL, Value::NIL),
    );

    // text-property-default-nonsticky: alist of properties vs non-stickiness.
    // The effective GNU default is assembled by two C files, so it is kept in
    // one place -- see `default_text_property_nonsticky_alist'.
    obarray.seed_value(
        "text-property-default-nonsticky",
        neovm_edit::emacs_core::textprop::default_text_property_nonsticky_alist(),
    );
}
