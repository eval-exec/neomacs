//! GNU-style DEFSYM table: fixed spellings with fixed `SymId`s.
//!
//! GNU holds every well-known symbol as a static `Lisp_Object` (`Qfoo`),
//! created once at startup and never looked up again. This table is the
//! Neo equivalent: `SymbolRegistry::new` seeds every spelling below, in
//! declaration order, immediately after the nil/t/unbound seeds, and asserts
//! the assigned id equals [`WellKnown::id`]. A use site is therefore a
//! constant id read: no registry lock, no name hash, no epoch check.
//!
//! The table is one list: the enum variants and [`NAMES`] are expanded from
//! the same macro invocation, so order and spellings cannot drift. The
//! compile-time checks below reject duplicate spellings, keyword spellings,
//! and the three constructor seeds.
//!
//! After `(unintern "foo")` the constant still names the original symbol (as
//! in GNU); the registry merely stops mapping the spelling to it, and Lisp
//! interning the spelling again allocates a new symbol.

use super::SymId;

/// First `SymId` assigned to a table spelling; nil/t/unbound occupy 0..3.
pub const FIRST_ID: u32 = 3;

macro_rules! well_known_table {
    ($($variant:ident = $spelling:literal),* $(,)?) => {
        /// A fixed GNU spelling with a compile-time `SymId`.
        #[repr(u32)]
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, strum::EnumCount, strum::VariantArray,
        )]
        pub enum WellKnown {
            $($variant),*
        }

        /// Every spelling, in variant order.
        pub const NAMES: &[&str] = &[$($spelling),*];

        impl WellKnown {
            /// Total table entries.
            pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

            /// Every variant in declaration order.
            pub const VARIANTS: &'static [Self] = <Self as strum::VariantArray>::VARIANTS;

            /// The process-lifetime identity seeded at registry construction.
            #[inline]
            pub const fn id(self) -> SymId {
                SymId(FIRST_ID + self as u32)
            }

            /// The spelling this identity was seeded from.
            #[inline]
            pub const fn name(self) -> &'static str {
                NAMES[self as usize]
            }

            /// Whether `id` is one of the constructor-seeded table
            /// identities. GNU's initial obarray always contains every
            /// DEFSYM, so source-surface cleanup must never unintern one.
            #[inline]
            pub const fn is_table_id(id: SymId) -> bool {
                id.0 >= FIRST_ID && id.0 - FIRST_ID < Self::COUNT as u32
            }
        }

        const _: () = {
            assert!(WellKnown::COUNT == NAMES.len());
            assert!(WellKnown::COUNT <= (u32::MAX - FIRST_ID) as usize);
            assert!(names_are_unique(NAMES));
            assert!(names_exclude_seeds_and_keywords(NAMES));
        };
    };
}

/// Byte equality usable in const evaluation.
const fn str_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut index = 0;
    while index < a.len() {
        if a[index] != b[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn names_are_unique(names: &[&str]) -> bool {
    let mut left = 0;
    while left < names.len() {
        let mut right = left + 1;
        while right < names.len() {
            if str_eq(names[left], names[right]) {
                return false;
            }
            right += 1;
        }
        left += 1;
    }
    true
}

const fn names_exclude_seeds_and_keywords(names: &[&str]) -> bool {
    let mut index = 0;
    while index < names.len() {
        let name = names[index];
        if str_eq(name, "nil") || str_eq(name, "t") || str_eq(name, "unbound") {
            return false;
        }
        let bytes = name.as_bytes();
        if !bytes.is_empty() && bytes[0] == b':' {
            return false;
        }
        index += 1;
    }
    true
}

well_known_table! {
    AbbrevMode = "abbrev-mode",
    Alpha = "alpha",
    AlphaBackground = "alpha-background",
    AutoHscrollMode = "auto-hscroll-mode",
    AutoLower = "auto-lower",
    AutoRaise = "auto-raise",
    AutoSaveHook = "auto-save-hook",
    AutoSaveInterval = "auto-save-interval",
    AutoSaveTimeout = "auto-save-timeout",
    BackgroundColor = "background-color",
    Bar = "bar",
    BarfIfBufferReadOnly = "barf-if-buffer-read-only",
    BorderColor = "border-color",
    BorderWidth = "border-width",
    BordersRespectAlphaBackground = "borders-respect-alpha-background",
    BottomDividerWidth = "bottom-divider-width",
    BufferFileCodingSystem = "buffer-file-coding-system",
    BufferPosition = "buffer-position",
    Category = "category",
    CharScriptTable = "char-script-table",
    ChildFrameBorder = "child-frame-border",
    ChildFrameBorderWidth = "child-frame-border-width",
    ClearMessageFunction = "clear-message-function",
    CommandExecute = "command-execute",
    CommandRemapping = "command-remapping",
    Composition = "composition",
    CompositionFunctionTable = "composition-function-table",
    CurrentPrefixArg = "current-prefix-arg",
    Cursor = "cursor",
    CursorColor = "cursor-color",
    CursorType = "cursor-type",
    DeactivateMark = "deactivate-mark",
    DebugOnEvent = "debug-on-event",
    Default = "default",
    DefaultDirectory = "default-directory",
    DelayedWarningsHook = "delayed-warnings-hook",
    DelayedWarningsList = "delayed-warnings-list",
    DisablePointAdjustment = "disable-point-adjustment",
    Display = "display",
    DoAutoSave = "do-auto-save",
    EchoKeystrokes = "echo-keystrokes",
    EchoKeystrokesHelp = "echo-keystrokes-help",
    EldocLastMessage = "eldoc-last-message",
    EscapeGlyph = "escape-glyph",
    Eval = "eval",
    ExecutingKbdMacro = "executing-kbd-macro",
    Exit = "exit",
    ExpandAbbrev = "expand-abbrev",
    Explicit = "explicit",
    Face = "face",
    FaceAlias = "face-alias",
    FillColumnIndicator = "fill-column-indicator",
    Font = "font",
    FontBackend = "font-backend",
    FontLockFace = "font-lock-face",
    FontParameter = "font-parameter",
    FontificationFunctions = "fontification-functions",
    Fontified = "fontified",
    Fontsize = "fontsize",
    ForegroundColor = "foreground-color",
    FrameAlphaLowerLimit = "frame-alpha-lower-limit",
    Fringe = "fringe",
    FringeIndicatorAlist = "fringe-indicator-alist",
    Fullscreen = "fullscreen",
    FuncallInteractively = "funcall-interactively",
    GlobalDisablePointAdjustment = "global-disable-point-adjustment",
    GlyphlessChar = "glyphless-char",
    HeaderLine = "header-line",
    Height = "height",
    HelpChar = "help-char",
    HelpEcho = "help-echo",
    HelpEventList = "help-event-list",
    HorizontalScrollBars = "horizontal-scroll-bars",
    HscrollMargin = "hscroll-margin",
    HscrollStep = "hscroll-step",
    IconName = "icon-name",
    IconType = "icon-type",
    Identity = "identity",
    ImageScalingFactor = "image-scaling-factor",
    InhibitDebugger = "inhibit-debugger",
    InhibitDoubleBuffering = "inhibit-double-buffering",
    InhibitEolConversion = "inhibit-eol-conversion",
    InhibitMessage = "inhibit-message",
    InhibitQuit = "inhibit-quit",
    InhibitRedisplay = "inhibit-redisplay",
    InputDecodeMap = "input-decode-map",
    InputMethodFunction = "input-method-function",
    InputPendingPFilterEvents = "input-pending-p-filter-events",
    Interactive = "interactive",
    InternalBorderWidth = "internal-border-width",
    InternalTimerStartIdle = "internal-timer-start-idle",
    Invisible = "invisible",
    IsearchMode = "isearch-mode",
    KeyTranslationMap = "key-translation-map",
    KeyboardTranslateTable = "keyboard-translate-table",
    Keymap = "keymap",
    Lambda = "lambda",
    LastCommand = "last-command",
    LastCommandEvent = "last-command-event",
    LastEventFrame = "last-event-frame",
    LastInputEvent = "last-input-event",
    LastKbdMacro = "last-kbd-macro",
    LastNonmenuEvent = "last-nonmenu-event",
    LastPrefixArg = "last-prefix-arg",
    LastRepeatableCommand = "last-repeatable-command",
    LeftFringe = "left-fringe",
    LineHeight = "line-height",
    LinePrefix = "line-prefix",
    LineSpacing = "line-spacing",
    List = "list",
    LocalFunctionKeyMap = "local-function-key-map",
    LocalMap = "local-map",
    Macro = "macro",
    Many = "many",
    MaxImageSize = "max-image-size",
    MaxMiniWindowHeight = "max-mini-window-height",
    MemoryFull = "memory-full",
    Menu = "menu",
    MenuBarLines = "menu-bar-lines",
    MessageLogMax = "message-log-max",
    MessagesBufferName = "messages-buffer-name",
    MinHeight = "min-height",
    MinWidth = "min-width",
    ModeLine = "mode-line",
    ModeLineInNonSelectedWindows = "mode-line-in-non-selected-windows",
    ModeName = "mode-name",
    MouseColor = "mouse-color",
    MouseFace = "mouse-face",
    Name = "name",
    NeomacsScrollDefinition = "neomacs--scroll-definition",
    NeomacsStartupLastCall = "neomacs--startup-last-call",
    NeomacsStartupLastPhase = "neomacs--startup-last-phase",
    NeomacsCompositorScrolling = "neomacs-compositor-scrolling",
    NeomacsCursorEffect = "neomacs-cursor-effect",
    NeovmFrameTextLines = "neovm--frame-text-lines",
    NeovmFrameTotalCols = "neovm--frame-total-cols",
    NeovmFrameTotalLines = "neovm--frame-total-lines",
    NeovmObarrayObject = "neovm--obarray-object",
    NoAcceptFocus = "no-accept-focus",
    NoFocusOnMap = "no-focus-on-map",
    NoSelfInsert = "no-self-insert",
    NoSpecialGlyphs = "no-special-glyphs",
    NobreakCharAsciiDisplay = "nobreak-char-ascii-display",
    NobreakSpace = "nobreak-space",
    NsAppearance = "ns-appearance",
    NsTransparentTitlebar = "ns-transparent-titlebar",
    Obarray = "obarray",
    Object = "object",
    Only = "only",
    OverlayArrow = "overlay-arrow",
    OverlayArrowBitmap = "overlay-arrow-bitmap",
    OverlayArrowString = "overlay-arrow-string",
    OverlayArrowVariableList = "overlay-arrow-variable-list",
    OverrideRedirect = "override-redirect",
    OverridingLocalMap = "overriding-local-map",
    OverridingTerminalLocalMap = "overriding-terminal-local-map",
    OverwriteMode = "overwrite-mode",
    OverwriteModeBinary = "overwrite-mode-binary",
    ParentFrame = "parent-frame",
    PixelScrollPrecision = "pixel-scroll-precision",
    PixelScrollPrecisionInterpolate = "pixel-scroll-precision-interpolate",
    PixelScrollPrecisionLargeScrollHeight = "pixel-scroll-precision-large-scroll-height",
    PixelScrollPrecisionMode = "pixel-scroll-precision-mode",
    PixelScrollPrecisionScrollDown = "pixel-scroll-precision-scroll-down",
    PixelScrollPrecisionScrollDownPage = "pixel-scroll-precision-scroll-down-page",
    PixelScrollPrecisionScrollUp = "pixel-scroll-precision-scroll-up",
    PixelScrollPrecisionScrollUpPage = "pixel-scroll-precision-scroll-up-page",
    PixelScrollPrecisionUseMomentum = "pixel-scroll-precision-use-momentum",
    PosVisibleInWindowP = "pos-visible-in-window-p",
    Position = "position",
    PosnAtPoint = "posn-at-point",
    PosnAtXY = "posn-at-x-y",
    PostCommandHook = "post-command-hook",
    PreCommandHook = "pre-command-hook",
    PreRedisplayFunction = "pre-redisplay-function",
    PrefixArg = "prefix-arg",
    PrefixHelpCommand = "prefix-help-command",
    PrefixNumericValue = "prefix-numeric-value",
    Priority = "priority",
    RealLastCommand = "real-last-command",
    RealThisCommand = "real-this-command",
    RedisplayAdhocScrollInResizeMiniWindows = "redisplay-adhoc-scroll-in-resize-mini-windows",
    RedisplaySkipInitialFrame = "redisplay-skip-initial-frame",
    ResizeMiniFrames = "resize-mini-frames",
    ResizeMiniWindows = "resize-mini-windows",
    RightDividerWidth = "right-divider-width",
    RightFringe = "right-fringe",
    RiskyLocalVariable = "risky-local-variable",
    ScreenGamma = "screen-gamma",
    ScrollBarBackground = "scroll-bar-background",
    ScrollBarForeground = "scroll-bar-foreground",
    ScrollBarHeight = "scroll-bar-height",
    ScrollBarWidth = "scroll-bar-width",
    ScrollMinibufferConservatively = "scroll-minibuffer-conservatively",
    SetWindowStart = "set-window-start",
    SetWindowVscroll = "set-window-vscroll",
    Shaded = "shaded",
    SkipTaskbar = "skip-taskbar",
    SpecialEventMap = "special-event-map",
    Sticky = "sticky",
    SwitchFrame = "switch-frame",
    TabBar = "tab-bar",
    TabBarButtonMargin = "tab-bar-button-margin",
    TabBarButtonRelief = "tab-bar-button-relief",
    TabBarLines = "tab-bar-lines",
    TabLine = "tab-line",
    TextQuotingStyle = "text-quoting-style",
    ThisCommand = "this-command",
    ThisCommandKeysShiftTranslated = "this-command-keys-shift-translated",
    ThisOriginalCommand = "this-original-command",
    TimerIdleList = "timer-idle-list",
    TimerList = "timer-list",
    Title = "title",
    ToolBar = "tool-bar",
    ToolBarLines = "tool-bar-lines",
    ToolBarPosition = "tool-bar-position",
    Tooltip = "tooltip",
    TopLevel = "top-level",
    TrackMouse = "track-mouse",
    TrailingWhitespace = "trailing-whitespace",
    TransientMarkMode = "transient-mark-mode",
    TranslateUpperCaseKeyBindings = "translate-upper-case-key-bindings",
    TruncateLines = "truncate-lines",
    TruncatePartialWidthWindows = "truncate-partial-width-windows",
    Undecorated = "undecorated",
    Undefined = "undefined",
    UndoAutoAddBoundary = "undo-auto--add-boundary",
    UndoAutoLastBoundaryCause = "undo-auto--last-boundary-cause",
    UndoAutoUndoablyChangedBuffers = "undo-auto--undoably-changed-buffers",
    UndoAutoAmalgamate = "undo-auto-amalgamate",
    Unevalled = "unevalled",
    UnreadCommandEvents = "unread-command-events",
    UnreadInputMethodEvents = "unread-input-method-events",
    UnreadPostInputMethodEvents = "unread-post-input-method-events",
    Unsplittable = "unsplittable",
    UseDefaultFontForSymbols = "use-default-font-for-symbols",
    UseFrameSynchronization = "use-frame-synchronization",
    VerticalBorder = "vertical-border",
    VerticalMotion = "vertical-motion",
    VerticalScrollBars = "vertical-scroll-bars",
    Visibility = "visibility",
    WaitForWm = "wait-for-wm",
    WheelDown = "wheel-down",
    WheelUp = "wheel-up",
    WhileNoInputIgnoreEvents = "while-no-input-ignore-events",
    Width = "width",
    Window = "window",
    WindowResizeMiniFrame = "window--resize-mini-frame",
    WindowResizeRootWindowVertically = "window--resize-root-window-vertically",
    WindowBufferChangeFunctions = "window-buffer-change-functions",
    WindowConfigurationChangeHook = "window-configuration-change-hook",
    WindowDivider = "window-divider",
    WindowDividerFirstPixel = "window-divider-first-pixel",
    WindowDividerLastPixel = "window-divider-last-pixel",
    WindowScrollFunctions = "window-scroll-functions",
    WindowSelectionChangeFunctions = "window-selection-change-functions",
    WindowSizeChangeFunctions = "window-size-change-functions",
    WindowStateChangeFunctions = "window-state-change-functions",
    WindowStateChangeHook = "window-state-change-hook",
    WindowSystem = "window-system",
    WindowTextPixelSize = "window-text-pixel-size",
    WordWrap = "word-wrap",
    WrapPrefix = "wrap-prefix",
    XCursorForePixel = "x-cursor-fore-pixel",
    XResourceClass = "x-resource-class",
    XResourceName = "x-resource-name",
    XScrollEventDeltaFactor = "x-scroll-event-delta-factor",
    XStretchCursor = "x-stretch-cursor",
    ZGroup = "z-group",

    // Turn-15: fixed spellings still reached by name on the keystroke rows
    // (typed conversions in the same commit). Appended so earlier ids stay
    // put; the compile-time checks still reject duplicates/keywords/seeds.
    AfterString = "after-string",
    AutoFillChars = "auto-fill-chars",
    AutoSaveNoMessage = "auto-save-no-message",
    BeforeString = "before-string",
    Border = "border",
    BufferInvisibilitySpec = "buffer-invisibility-spec",
    BufferReadOnly = "buffer-read-only",
    BufferUndoList = "buffer-undo-list",
    CloseTab = "close-tab",
    Concat = "concat",
    CopySequence = "copy-sequence",
    Curve = "curve",
    Columns = "columns",
    DisplayLineNumbers = "display-line-numbers",
    EolMnemonicDos = "eol-mnemonic-dos",
    Error = "error",
    EolMnemonicMac = "eol-mnemonic-mac",
    EolMnemonicUndecided = "eol-mnemonic-undecided",
    EolMnemonicUnix = "eol-mnemonic-unix",
    FormatMessage = "format-message",
    GetTextProperty = "get-text-property",
    GotoChar = "goto-char",
    Grave = "grave",
    HandleShiftSelection = "handle-shift-selection",
    HeaderLineActive = "header-line-active",
    HeaderLineFormat = "header-line-format",
    HeaderLineInactive = "header-line-inactive",
    HeaderLineIndentWidth = "header-line-indent-width",
    InhibitReadOnly = "inhibit-read-only",
    InitialWindowSystem = "initial-window-system",
    InhibitXResources = "inhibit-x-resources",
    InsertAndInherit = "insert-and-inherit",
    InternalAutoFill = "internal-auto-fill",
    InternalBorder = "internal-border",
    Margin = "margin",
    MarkActive = "mark-active",
    MenuItem = "menu-item",
    ModeLineActive = "mode-line-active",
    ModeLineFormat = "mode-line-format",
    ModeLineInactive = "mode-line-inactive",
    Mouse = "mouse",
    PostSelfInsertHook = "post-self-insert-hook",
    Propertize = "propertize",
    Quote = "quote",
    RegionExtractFunction = "region-extract-function",
    RunHooks = "run-hooks",
    ScrollBar = "scroll-bar",
    SelectActiveRegions = "select-active-regions",
    SelectionInhibitUpdateCommands = "selection-inhibit-update-commands",
    Straight = "straight",
    TabBarMakeKeymap1 = "tab-bar-make-keymap-1",
    TabLineFormat = "tab-line-format",
    UndoAutoThisCommandAmalgamating = "undo-auto--this-command-amalgamating",
    VisibleCursor = "visible-cursor",
}

#[cfg(test)]
#[path = "tests/well_known_test.rs"]
mod tests;
