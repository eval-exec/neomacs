//! Fixed GNU spellings shared by the layout engine's per-redisplay probes.
//!
//! Each handle is an [`EpochCachedSymbol`]: it resolves its canonical symbol
//! identity once per registry epoch and stores no variable, property or face
//! value. GNU keeps these names as DEFSYM/DEFVAR statics (`Qdisplay`,
//! `Qinvisible`, `Vfontification_functions`, ...); the probes below rebuilt
//! and re-interned the printed spelling at every call, i.e. on every
//! keystroke's redisplay. Callers still read the live value/plist at the
//! original boundary.

use neovm_core::emacs_core::intern::EpochCachedSymbol;

// Text and overlay properties (GNU DEFSYM constants).
pub(crate) static DISPLAY: EpochCachedSymbol = EpochCachedSymbol::new("display");
pub(crate) static COMPOSITION: EpochCachedSymbol = EpochCachedSymbol::new("composition");
pub(crate) static FACE: EpochCachedSymbol = EpochCachedSymbol::new("face");
pub(crate) static FONT_LOCK_FACE: EpochCachedSymbol = EpochCachedSymbol::new("font-lock-face");
pub(crate) static MOUSE_FACE: EpochCachedSymbol = EpochCachedSymbol::new("mouse-face");
pub(crate) static LINE_HEIGHT: EpochCachedSymbol = EpochCachedSymbol::new("line-height");
pub(crate) static LINE_SPACING: EpochCachedSymbol = EpochCachedSymbol::new("line-spacing");
pub(crate) static PRIORITY: EpochCachedSymbol = EpochCachedSymbol::new("priority");
pub(crate) static CATEGORY: EpochCachedSymbol = EpochCachedSymbol::new("category");
pub(crate) static INVISIBLE: EpochCachedSymbol = EpochCachedSymbol::new("invisible");
pub(crate) static LINE_PREFIX: EpochCachedSymbol = EpochCachedSymbol::new("line-prefix");
pub(crate) static WRAP_PREFIX: EpochCachedSymbol = EpochCachedSymbol::new("wrap-prefix");
pub(crate) static WINDOW_PROPERTY: EpochCachedSymbol = EpochCachedSymbol::new("window");
pub(crate) static CURSOR_PROPERTY: EpochCachedSymbol = EpochCachedSymbol::new("cursor");
pub(crate) static KEYMAP_PROPERTY: EpochCachedSymbol = EpochCachedSymbol::new("keymap");
pub(crate) static LOCAL_MAP_PROPERTY: EpochCachedSymbol = EpochCachedSymbol::new("local-map");
pub(crate) static HELP_ECHO_PROPERTY: EpochCachedSymbol = EpochCachedSymbol::new("help-echo");
pub(crate) static FONTIFIED_PROPERTY: EpochCachedSymbol = EpochCachedSymbol::new("fontified");

// Layout variables (`DEFVAR_LISP` / `DEFVAR_PER_BUFFER`).
pub(crate) static COMPOSITION_FUNCTION_TABLE: EpochCachedSymbol =
    EpochCachedSymbol::new("composition-function-table");
pub(crate) static RESIZE_MINI_WINDOWS: EpochCachedSymbol =
    EpochCachedSymbol::new("resize-mini-windows");
pub(crate) static REDISPLAY_ADHOC_SCROLL_IN_RESIZE_MINI_WINDOWS: EpochCachedSymbol =
    EpochCachedSymbol::new("redisplay-adhoc-scroll-in-resize-mini-windows");
pub(crate) static FONTIFICATION_FUNCTIONS: EpochCachedSymbol =
    EpochCachedSymbol::new("fontification-functions");
pub(crate) static MAX_MINI_WINDOW_HEIGHT: EpochCachedSymbol =
    EpochCachedSymbol::new("max-mini-window-height");
pub(crate) static OVERLAY_ARROW_VARIABLE_LIST: EpochCachedSymbol =
    EpochCachedSymbol::new("overlay-arrow-variable-list");

// Overlay-arrow spellings (`Qoverlay_arrow_string` and friends).
pub(crate) static OVERLAY_ARROW_STRING: EpochCachedSymbol =
    EpochCachedSymbol::new("overlay-arrow-string");
pub(crate) static OVERLAY_ARROW_BITMAP: EpochCachedSymbol =
    EpochCachedSymbol::new("overlay-arrow-bitmap");
pub(crate) static OVERLAY_ARROW: EpochCachedSymbol = EpochCachedSymbol::new("overlay-arrow");

// Face names and bridge-side cursor/window-policy spellings.
pub(crate) static DEFAULT_FACE: EpochCachedSymbol = EpochCachedSymbol::new("default");
pub(crate) static X_CURSOR_FORE_PIXEL: EpochCachedSymbol =
    EpochCachedSymbol::new("x-cursor-fore-pixel");
pub(crate) static BAR: EpochCachedSymbol = EpochCachedSymbol::new("bar");
pub(crate) static MODE_LINE_IN_NON_SELECTED_WINDOWS: EpochCachedSymbol =
    EpochCachedSymbol::new("mode-line-in-non-selected-windows");
pub(crate) static SCROLL_MINIBUFFER_CONSERVATIVELY: EpochCachedSymbol =
    EpochCachedSymbol::new("scroll-minibuffer-conservatively");
pub(crate) static NEOMACS_CURSOR_EFFECT: EpochCachedSymbol =
    EpochCachedSymbol::new("neomacs-cursor-effect");
pub(crate) static NOBREAK_CHAR_ASCII_DISPLAY: EpochCachedSymbol =
    EpochCachedSymbol::new("nobreak-char-ascii-display");
pub(crate) static X_STRETCH_CURSOR: EpochCachedSymbol = EpochCachedSymbol::new("x-stretch-cursor");
pub(crate) static FRINGE_INDICATOR_ALIST: EpochCachedSymbol =
    EpochCachedSymbol::new("fringe-indicator-alist");
pub(crate) static USE_DEFAULT_FONT_FOR_SYMBOLS: EpochCachedSymbol =
    EpochCachedSymbol::new("use-default-font-for-symbols");
pub(crate) static CHAR_SCRIPT_TABLE: EpochCachedSymbol =
    EpochCachedSymbol::new("char-script-table");
pub(crate) static FRAME_ALPHA_LOWER_LIMIT: EpochCachedSymbol =
    EpochCachedSymbol::new("frame-alpha-lower-limit");
pub(crate) static X_SCROLL_EVENT_DELTA_FACTOR: EpochCachedSymbol =
    EpochCachedSymbol::new("x-scroll-event-delta-factor");
pub(crate) static TAB_BAR_BUTTON_MARGIN: EpochCachedSymbol =
    EpochCachedSymbol::new("tab-bar-button-margin");
pub(crate) static TAB_BAR_BUTTON_RELIEF: EpochCachedSymbol =
    EpochCachedSymbol::new("tab-bar-button-relief");

// Fixed face names resolved by the chrome and frame-parameter paths.
pub(crate) static MODE_LINE_FACE: EpochCachedSymbol = EpochCachedSymbol::new("mode-line");
pub(crate) static HEADER_LINE_FACE: EpochCachedSymbol = EpochCachedSymbol::new("header-line");
pub(crate) static FRINGE_FACE: EpochCachedSymbol = EpochCachedSymbol::new("fringe");
pub(crate) static TOOL_BAR_FACE: EpochCachedSymbol = EpochCachedSymbol::new("tool-bar");
pub(crate) static MENU_FACE: EpochCachedSymbol = EpochCachedSymbol::new("menu");
pub(crate) static CHILD_FRAME_BORDER_FACE: EpochCachedSymbol =
    EpochCachedSymbol::new("child-frame-border");
pub(crate) static TAB_BAR_FACE: EpochCachedSymbol = EpochCachedSymbol::new("tab-bar");
pub(crate) static TAB_LINE_FACE: EpochCachedSymbol = EpochCachedSymbol::new("tab-line");

// Frame parameters read on every layout pass (GNU frame parameter DEFSYMs).
pub(crate) static WIDTH_PARAM: EpochCachedSymbol = EpochCachedSymbol::new("width");
pub(crate) static CURSOR_TYPE_PARAM: EpochCachedSymbol = EpochCachedSymbol::new("cursor-type");
pub(crate) static BACKGROUND_COLOR_PARAM: EpochCachedSymbol =
    EpochCachedSymbol::new("background-color");
pub(crate) static FOREGROUND_COLOR_PARAM: EpochCachedSymbol =
    EpochCachedSymbol::new("foreground-color");
pub(crate) static MOUSE_COLOR_PARAM: EpochCachedSymbol = EpochCachedSymbol::new("mouse-color");
pub(crate) static CURSOR_COLOR_PARAM: EpochCachedSymbol = EpochCachedSymbol::new("cursor-color");
pub(crate) static LEFT_FRINGE_PARAM: EpochCachedSymbol = EpochCachedSymbol::new("left-fringe");
pub(crate) static RIGHT_FRINGE_PARAM: EpochCachedSymbol = EpochCachedSymbol::new("right-fringe");
