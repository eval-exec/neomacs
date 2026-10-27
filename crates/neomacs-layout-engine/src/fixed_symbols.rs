//! Fixed GNU spellings shared by the layout engine's per-redisplay probes.
//!
//! Each handle is an [`WellKnown`]: a compile-time fixed `SymId` assigned by
//! the symbol registry constructor, exactly like GNU's `Qfoo` statics. It
//! stores no variable, property or face value, and reading it takes no lock
//! and no epoch. GNU keeps these names as DEFSYM/DEFVAR statics (`Qdisplay`,
//! `Qinvisible`, `Vfontification_functions`, ...); the probes below rebuilt
//! and re-interned the printed spelling at every call, i.e. on every
//! keystroke's redisplay. Callers still read the live value/plist at the
//! original boundary.

use neovm_core::emacs_core::intern::WellKnown;

// Text and overlay properties (GNU DEFSYM constants).
pub(crate) static DISPLAY: WellKnown = WellKnown::Display;
pub(crate) static COMPOSITION: WellKnown = WellKnown::Composition;
pub(crate) static FACE: WellKnown = WellKnown::Face;
pub(crate) static FONT_LOCK_FACE: WellKnown = WellKnown::FontLockFace;
pub(crate) static MOUSE_FACE: WellKnown = WellKnown::MouseFace;
pub(crate) static LINE_HEIGHT: WellKnown = WellKnown::LineHeight;
pub(crate) static LINE_SPACING: WellKnown = WellKnown::LineSpacing;
pub(crate) static PRIORITY: WellKnown = WellKnown::Priority;
pub(crate) static CATEGORY: WellKnown = WellKnown::Category;
pub(crate) static INVISIBLE: WellKnown = WellKnown::Invisible;
pub(crate) static LINE_PREFIX: WellKnown = WellKnown::LinePrefix;
pub(crate) static WRAP_PREFIX: WellKnown = WellKnown::WrapPrefix;
pub(crate) static WINDOW_PROPERTY: WellKnown = WellKnown::Window;
pub(crate) static CURSOR_PROPERTY: WellKnown = WellKnown::Cursor;
pub(crate) static KEYMAP_PROPERTY: WellKnown = WellKnown::Keymap;
pub(crate) static LOCAL_MAP_PROPERTY: WellKnown = WellKnown::LocalMap;
pub(crate) static HELP_ECHO_PROPERTY: WellKnown = WellKnown::HelpEcho;
pub(crate) static FONTIFIED_PROPERTY: WellKnown = WellKnown::Fontified;

// Layout variables (`DEFVAR_LISP` / `DEFVAR_PER_BUFFER`).
pub(crate) static COMPOSITION_FUNCTION_TABLE: WellKnown = WellKnown::CompositionFunctionTable;
pub(crate) static RESIZE_MINI_WINDOWS: WellKnown = WellKnown::ResizeMiniWindows;
pub(crate) static REDISPLAY_ADHOC_SCROLL_IN_RESIZE_MINI_WINDOWS: WellKnown =
    WellKnown::RedisplayAdhocScrollInResizeMiniWindows;
pub(crate) static FONTIFICATION_FUNCTIONS: WellKnown = WellKnown::FontificationFunctions;
pub(crate) static MAX_MINI_WINDOW_HEIGHT: WellKnown = WellKnown::MaxMiniWindowHeight;
pub(crate) static OVERLAY_ARROW_VARIABLE_LIST: WellKnown = WellKnown::OverlayArrowVariableList;

// Overlay-arrow spellings (`Qoverlay_arrow_string` and friends).
pub(crate) static OVERLAY_ARROW_STRING: WellKnown = WellKnown::OverlayArrowString;
pub(crate) static OVERLAY_ARROW_BITMAP: WellKnown = WellKnown::OverlayArrowBitmap;
pub(crate) static OVERLAY_ARROW: WellKnown = WellKnown::OverlayArrow;

// Face names and bridge-side cursor/window-policy spellings.
pub(crate) static DEFAULT_FACE: WellKnown = WellKnown::Default;
pub(crate) static X_CURSOR_FORE_PIXEL: WellKnown = WellKnown::XCursorForePixel;
pub(crate) static BAR: WellKnown = WellKnown::Bar;
pub(crate) static MODE_LINE_IN_NON_SELECTED_WINDOWS: WellKnown =
    WellKnown::ModeLineInNonSelectedWindows;
pub(crate) static SCROLL_MINIBUFFER_CONSERVATIVELY: WellKnown =
    WellKnown::ScrollMinibufferConservatively;
pub(crate) static NEOMACS_CURSOR_EFFECT: WellKnown = WellKnown::NeomacsCursorEffect;
pub(crate) static NOBREAK_CHAR_ASCII_DISPLAY: WellKnown = WellKnown::NobreakCharAsciiDisplay;
pub(crate) static X_STRETCH_CURSOR: WellKnown = WellKnown::XStretchCursor;
pub(crate) static FRINGE_INDICATOR_ALIST: WellKnown = WellKnown::FringeIndicatorAlist;
pub(crate) static USE_DEFAULT_FONT_FOR_SYMBOLS: WellKnown = WellKnown::UseDefaultFontForSymbols;
pub(crate) static CHAR_SCRIPT_TABLE: WellKnown = WellKnown::CharScriptTable;
pub(crate) static FRAME_ALPHA_LOWER_LIMIT: WellKnown = WellKnown::FrameAlphaLowerLimit;
pub(crate) static X_SCROLL_EVENT_DELTA_FACTOR: WellKnown = WellKnown::XScrollEventDeltaFactor;
pub(crate) static TAB_BAR_BUTTON_MARGIN: WellKnown = WellKnown::TabBarButtonMargin;
pub(crate) static TAB_BAR_BUTTON_RELIEF: WellKnown = WellKnown::TabBarButtonRelief;

// Fixed face names resolved by the chrome and frame-parameter paths.
pub(crate) static MODE_LINE_FACE: WellKnown = WellKnown::ModeLine;
pub(crate) static HEADER_LINE_FACE: WellKnown = WellKnown::HeaderLine;
pub(crate) static FRINGE_FACE: WellKnown = WellKnown::Fringe;
pub(crate) static TOOL_BAR_FACE: WellKnown = WellKnown::ToolBar;
pub(crate) static MENU_FACE: WellKnown = WellKnown::Menu;
pub(crate) static CHILD_FRAME_BORDER_FACE: WellKnown = WellKnown::ChildFrameBorder;
pub(crate) static TAB_BAR_FACE: WellKnown = WellKnown::TabBar;
pub(crate) static TAB_LINE_FACE: WellKnown = WellKnown::TabLine;

// Frame parameters read on every layout pass (GNU frame parameter DEFSYMs).
pub(crate) static WIDTH_PARAM: WellKnown = WellKnown::Width;
pub(crate) static CURSOR_TYPE_PARAM: WellKnown = WellKnown::CursorType;
pub(crate) static BACKGROUND_COLOR_PARAM: WellKnown = WellKnown::BackgroundColor;
pub(crate) static FOREGROUND_COLOR_PARAM: WellKnown = WellKnown::ForegroundColor;
pub(crate) static MOUSE_COLOR_PARAM: WellKnown = WellKnown::MouseColor;
pub(crate) static CURSOR_COLOR_PARAM: WellKnown = WellKnown::CursorColor;
pub(crate) static LEFT_FRINGE_PARAM: WellKnown = WellKnown::LeftFringe;
pub(crate) static RIGHT_FRINGE_PARAM: WellKnown = WellKnown::RightFringe;
