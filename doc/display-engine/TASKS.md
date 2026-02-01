# Neomacs - Rust Display/Redisplay Engine

## Project Overview

**Neomacs** is a fork of GNU Emacs with a complete rewrite of the display/redisplay engine in Rust.

### Vision

Replace Emacs's legacy C display engine (`xdisp.c`, `dispnew.c`, ~50K lines) with a modern Rust implementation that is:
- GPU-accelerated (GTK4/GSK/Vulkan)
- Clean architecture (no incremental redisplay complexity)
- Leverages Rust ecosystem (cosmic-text, tree-sitter)
- Supports modern features (smooth scrolling, animations, video, WebKit)

### Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Redisplay Engine** | Pure Rust | Clean break from incremental complexity |
| **Text Layout** | `cosmic-text` | Full Unicode, bidi, shaping, line wrap |
| **Syntax Highlighting** | `tree-sitter` | Incremental parsing, language-agnostic |
| **GPU Rendering** | GTK4/GSK | Vulkan backend, Linux native |
| **Text Rendering** | `cosmic-text` + `swash` | Pure Rust, no Pango dependency |
| **Platform** | Linux (Wayland-first) | Focus on one platform first |
| **Video** | GStreamer (VA-API) | Hardware decode, DMA-BUF |
| **Browser** | WPE WebKit | Lightweight, DMA-BUF |

---

## Architecture

### New Architecture (Rust Redisplay)

```
┌─────────────────────────────────────────────────────────────────┐
│                     EMACS CORE (C)                               │
│  • Lisp interpreter                                              │
│  • Buffer data structure (text storage)                          │
│  • Window configuration (geometry only)                          │
│  • Keymaps, commands, minibuffer                                 │
└─────────────────────────────────┬───────────────────────────────┘
                                  │ FFI (read buffer, window info)
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│              RUST REDISPLAY ENGINE (libneomacs_display)          │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    Buffer Manager                          │ │
│  │  • Read Emacs buffer text via FFI                          │ │
│  │  • Track modifications                                     │ │
│  │  • Sync with cosmic-text Buffer                            │ │
│  └────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                              ▼                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    cosmic-text                             │ │
│  │  • Text shaping (ligatures, kerning)                       │ │
│  │  • Line wrapping (word-aware)                              │ │
│  │  • Bidi support (RTL languages)                            │ │
│  │  • Font fallback                                           │ │
│  │  • Glyph positioning                                       │ │
│  └────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                              ▼                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    tree-sitter                             │ │
│  │  • Incremental syntax parsing                              │ │
│  │  • Language grammars (elisp, rust, etc.)                   │ │
│  │  • Syntax → Face mapping                                   │ │
│  └────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                              ▼                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    Window Manager                          │ │
│  │  • Multiple windows (splits)                               │ │
│  │  • Cursor/point tracking                                   │ │
│  │  • Scrolling                                               │ │
│  │  • Mode line                                               │ │
│  └────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│                              ▼                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    GSK Renderer                            │ │
│  │  • Build render nodes                                      │ │
│  │  • Glyph atlas / texture cache                             │ │
│  │  • GPU compositing                                         │ │
│  └────────────────────────────────────────────────────────────┘ │
│                              │                                   │
│  ┌──────────────┬────────────┴────────────┬──────────────────┐ │
│  │    Video     │        Images           │      WebKit      │ │
│  │  GStreamer   │     (DMA-BUF)           │    WPE WebKit    │ │
│  └──────────────┴─────────────────────────┴──────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    GTK4 Window (Wayland/X11)
                              │
                              ▼
                    Vulkan/GL Compositor
```

### What Stays in C (Emacs Core)

- Lisp interpreter and evaluator
- Buffer data structure (gap buffer)
- Text properties storage
- Overlay storage
- Window tree structure
- Keymap handling
- Minibuffer
- File I/O

### What Moves to Rust (Display Engine)

| Component | From (C) | To (Rust) |
|-----------|----------|-----------|
| Text layout | `xdisp.c` | `cosmic-text` |
| Line wrapping | `xdisp.c` | `cosmic-text` |
| Glyph rendering | `*term.c` | GSK + cosmic-text |
| Face resolution | `xfaces.c` | Rust face cache |
| Syntax highlighting | font-lock.el | `tree-sitter` |
| Cursor drawing | `*term.c` | GSK |
| Scrolling | `dispnew.c` | Rust window manager |
| Redisplay loop | `xdisp.c` | Rust main loop |

---

## Rust Ecosystem

### Core Dependencies

| Crate | Purpose | Version |
|-------|---------|---------|
| `cosmic-text` | Text shaping, layout, rendering | 0.12+ |
| `tree-sitter` | Syntax parsing | 0.22+ |
| `gtk4` | GUI toolkit | 0.9+ |
| `gsk4` | GPU scene graph | 0.9+ |
| `gstreamer` | Video playback | 0.23+ |

### What cosmic-text Provides (FREE)

- ✅ Unicode text shaping (HarfBuzz-like)
- ✅ Font discovery and fallback
- ✅ Line wrapping (word-aware, character-aware)
- ✅ Bidirectional text (Arabic, Hebrew)
- ✅ Complex scripts (Devanagari, Thai, CJK)
- ✅ Variable-width font support
- ✅ Ligatures and kerning
- ✅ Color emoji
- ✅ Text selection/cursor positioning

### What tree-sitter Provides

- ✅ Incremental parsing (fast on edits)
- ✅ Error recovery (partial parse on syntax errors)
- ✅ Language grammars (100+ languages available)
- ✅ Query system (find syntax patterns)
- ✅ Highlighting queries (syntax → colors)

---

## Implementation Plan

### Phase 1: Foundation (Week 1-2) 🔥 CURRENT

#### 1.1 cosmic-text Integration
- [ ] Create `redisplay/` module structure
- [ ] Set up `FontSystem` with system fonts
- [ ] Create `WindowBuffer` wrapping `cosmic_text::Buffer`
- [ ] Test rendering with hardcoded text
- [ ] Verify line wrapping works
- [ ] Integrate with existing GSK renderer

**Files to create:**
```
rust/neomacs-display/src/redisplay/
├── mod.rs           # Module exports
├── buffer.rs        # WindowBuffer (cosmic-text wrapper)
├── font.rs          # FontSystem management
└── layout.rs        # Layout helpers
```

**Test milestone:** Render "Hello World\nLine 2" with automatic line wrap

#### 1.2 Emacs Buffer FFI
- [ ] Add `neomacs_buffer_text()` - read buffer substring
- [ ] Add `neomacs_buffer_size()` - get buffer size
- [ ] Add `neomacs_buffer_modified_tick()` - detect changes
- [ ] Add `neomacs_window_buffer()` - get window's buffer
- [ ] Add `neomacs_window_start()` - get display start position
- [ ] Add `neomacs_window_point()` - get cursor position
- [ ] Add `neomacs_window_dimensions()` - get pixel size

**Files to modify:**
```
src/neomacsterm.c    # Add FFI functions
src/neomacsfns.c     # Add FFI functions
```

**Test milestone:** Display actual *scratch* buffer content

### Phase 2: Window & Cursor (Week 3)

#### 2.1 Window Management
- [ ] Create `Window` struct tracking Emacs window
- [ ] Handle multiple windows (C-x 2, C-x 3)
- [ ] Handle window deletion (C-x 0, C-x 1)
- [ ] Render window borders/dividers
- [ ] Track window-start for scrolling

#### 2.2 Cursor Rendering
- [ ] Map Emacs point → screen (x, y)
- [ ] Render box cursor
- [ ] Render bar cursor
- [ ] Render hollow cursor
- [ ] Cursor blinking (optional)

**Test milestone:** C-x 3 splits window, both show content, cursor visible

### Phase 3: Faces & Styles (Week 4)

#### 3.1 Face System
- [ ] Create `FaceCache` mapping face-id → style
- [ ] Add FFI to read face attributes from Emacs
- [ ] Map Emacs face → `cosmic_text::Attrs`
- [ ] Support foreground/background colors
- [ ] Support bold/italic/underline

#### 3.2 Mode Line
- [ ] Render mode line with mode-line face
- [ ] Parse mode-line-format (basic)
- [ ] Show buffer name, major mode, position

**Test milestone:** Mode line visible with different colors

### Phase 4: Syntax Highlighting (Week 5-6)

#### 4.1 tree-sitter Integration
- [ ] Add `tree-sitter` dependency
- [ ] Create `SyntaxHighlighter` struct
- [ ] Load grammar based on major mode
- [ ] Parse buffer content
- [ ] Map tree-sitter nodes → faces

#### 4.2 Incremental Updates
- [ ] Track buffer modifications
- [ ] Update tree-sitter tree incrementally
- [ ] Re-highlight only changed regions

**Test milestone:** emacs-lisp-mode shows colored keywords, strings, comments

### Phase 5: Scrolling & Input (Week 7)

#### 5.1 Scrolling
- [ ] Handle C-v (scroll-up)
- [ ] Handle M-v (scroll-down)
- [ ] Handle mouse wheel
- [ ] Smooth scroll animation (optional)
- [ ] Keep cursor visible after scroll

#### 5.2 Input Handling
- [ ] Ensure keyboard input reaches Emacs
- [ ] Mouse click → set point
- [ ] Mouse drag → select region

**Test milestone:** Can scroll through large file, click to position cursor

### Phase 6: Advanced Features (Week 8+)

#### 6.1 Selection & Region
- [ ] Highlight active region
- [ ] Support transient-mark-mode

#### 6.2 Overlays (Basic)
- [ ] Read overlay data from Emacs
- [ ] Render overlay faces

#### 6.3 Line Numbers
- [ ] display-line-numbers-mode support
- [ ] Relative line numbers

#### 6.4 Fringe
- [ ] Fringe area rendering
- [ ] Basic fringe bitmaps

---

## Current Status (2026-02-01)

### Working Features ✅

The **hybrid rendering approach** is now working:

1. **Window operations work correctly:**
   - `C-x 2` (split-window-below) ✅
   - `C-x 3` (split-window-right) ✅  
   - `C-x 0` (delete-window) ✅
   - No display artifacts after window changes

2. **Text rendering works:**
   - cosmic-text renders text via GSK
   - Mode line displays correctly
   - Cursor visible and tracks point

### Key Fix: Full Frame Redraw

The fix was simple: instead of Emacs's incremental redisplay, we force full window content each frame:

**C side (`neomacsterm.c`):**
```c
void neomacs_update_begin (struct frame *f) {
  // Mark all windows as needing full redisplay
  mark_window_display_accurate (FRAME_ROOT_WINDOW (f), false);
  // ... then clear and rebuild in Rust
}
```

**Rust side (`ffi.rs`):**
```rust
fn begin_frame() {
  // Clear ALL glyphs - Emacs will resend everything
  display.frame_glyphs.glyphs.clear();
}
```

This is the **Neovide approach**: clear and rebuild each frame, no incremental complexity.

### What's Next

The current hybrid approach (Emacs xdisp.c → glyphs → Rust renders) works. 

The full Rust redisplay rewrite (Phase 1-6 below) is **optional** - pursue it for:
- Better long-term maintainability
- Features that require understanding buffer content (e.g., custom line wrapping)
- Eliminating xdisp.c complexity entirely

---

## Progress Tracking

### Milestones

| Milestone | Target | Status |
|-----------|--------|--------|
| **M1: Text Renders** | Week 2 | ✅ Done |
| **M2: Real Buffer** | Week 2 | ✅ Done (via xdisp.c) |
| **M3: Multi-Window** | Week 3 | ✅ Done |
| **M4: Cursor Works** | Week 3 | ✅ Done |
| **M5: Faces/Colors** | Week 4 | ⏳ Pending |
| **M6: Syntax Colors** | Week 6 | ⏳ Pending |
| **M7: Scrolling** | Week 7 | ⏳ Pending |
| **MVP Complete** | Week 7 | ⏳ Pending |

### Success Criteria for MVP

- [x] Open any text file, content displays correctly
- [ ] Line wrapping works at window edge
- [x] Cursor visible and tracks point
- [x] C-x 2 / C-x 3 window splits work
- [x] C-x 0 delete window works (no artifacts)
- [ ] C-v / M-v scrolling works
- [ ] Basic syntax highlighting (elisp at minimum)
- [x] Mode line shows buffer name and mode

---

## Technical Notes

### cosmic-text Usage

```rust
use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, 
    Metrics, Shaping, Style, Weight,
};

// Initialize once
let mut font_system = FontSystem::new();

// Per-window buffer
let metrics = Metrics::new(14.0, 18.0); // font_size, line_height
let mut buffer = Buffer::new(&mut font_system, metrics);
buffer.set_size(&mut font_system, Some(width), Some(height));

// Set text with attributes
let attrs = Attrs::new()
    .family(Family::Monospace)
    .color(Color::rgb(0, 0, 0));
buffer.set_text(&mut font_system, text, attrs, Shaping::Advanced);

// Iterate glyphs for rendering
for run in buffer.layout_runs() {
    for glyph in run.glyphs {
        // glyph.x, glyph.y - position
        // glyph.cache_key - for texture lookup
        // glyph.color_opt - color
    }
}
```

### tree-sitter Usage

```rust
use tree_sitter::{Parser, Language};

// Load grammar
let mut parser = Parser::new();
parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();

// Parse
let tree = parser.parse(source_code, None).unwrap();

// Walk for highlighting
let mut cursor = tree.walk();
// ... traverse nodes, map to faces
```

### FFI Pattern

```rust
// Rust side
#[no_mangle]
pub unsafe extern "C" fn neomacs_buffer_text(
    buf: *mut c_void,
    start: usize,
    end: usize,
    out: *mut u8,
    out_len: usize,
) -> usize {
    // Read text from Emacs buffer into out
}

// C side (neomacsterm.c)
size_t
neomacs_buffer_text(void *buf, size_t start, size_t end, 
                    char *out, size_t out_len)
{
    struct buffer *b = buf;
    // Copy text from buffer to out
}
```

---

## Open Questions

1. **Text properties**: How much to support initially?
2. **Overlays**: Full support or basic only?
3. **Bidi**: cosmic-text handles it, but need to test
4. **Performance**: Profile cosmic-text on large files
5. **Font config**: How to read Emacs font settings?

---

## References

- [cosmic-text docs](https://docs.rs/cosmic-text/latest/cosmic_text/)
- [tree-sitter docs](https://tree-sitter.github.io/tree-sitter/)
- [Emacs Internals - Display](https://www.gnu.org/software/emacs/manual/html_node/elisp/Display.html)
- [Neovide architecture](https://github.com/neovide/neovide) (inspiration)
- [Zed GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (inspiration)
