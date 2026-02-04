/* Definitions for Neomacs GPU-accelerated display backend.
   Copyright (C) 2024-2026 Free Software Foundation, Inc.

This file is part of GNU Emacs.

GNU Emacs is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

GNU Emacs is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.  */

#ifndef _NEOMACSTERM_H_
#define _NEOMACSTERM_H_

#include "dispextern.h"
#include "frame.h"
#include "character.h"
#include "font.h"
#include "keyboard.h"  /* For union buffered_input_event */

#ifdef HAVE_NEOMACS

/* GTK4 headers */
#include <gtk/gtk.h>

/* Cairo is used for font rendering with ftcrfont */
#include <cairo.h>

/* Include the generated Rust FFI header */
#include "neomacs_display.h"

/* Color conversion macros */
#define RGB_TO_ULONG(r, g, b) (((r) << 16) | ((g) << 8) | (b))
#define ARGB_TO_ULONG(a, r, g, b) (((a) << 24) | ((r) << 16) | ((g) << 8) | (b))
#define ALPHA_FROM_ULONG(color) ((color) >> 24)
#define RED_FROM_ULONG(color)   (((color) >> 16) & 0xff)
#define GREEN_FROM_ULONG(color) (((color) >> 8) & 0xff)
#define BLUE_FROM_ULONG(color)  ((color) & 0xff)

/* Forward declarations */
struct neomacs_display_info;
struct neomacs_output;

/* Bitmap record for image caching */
struct neomacs_bitmap_record
{
  char *file;
  int refcount;
  int height, width, depth;
  cairo_pattern_t *pattern;
};

/* Scroll bar structure - one per scroll bar widget */
struct scroll_bar
{
  /* These fields are shared by all vectors.  */
  union vectorlike_header header;

  /* The window we're a scroll bar for.  */
  Lisp_Object window;

  /* The next and previous in the chain of scroll bars in this frame.  */
  Lisp_Object next, prev;

  /* Fields from `x_window' down will not be traced by the GC.  */

  /* The window representing this scroll bar (GTK widget handle).  */
  Window x_window;

  /* The position and size of the scroll bar in pixels, relative to the frame.  */
  int top, left, width, height;

  /* The starting and ending positions of the handle, relative to the
     handle area (zero is the top position).  */
  int start, end;

  /* If the scroll bar handle is currently being dragged by the user,
     this is the number of pixels from the top of the handle to the
     place where the user grabbed it.  If not being dragged, this is -1.  */
  int dragging;

  /* True if the scroll bar is horizontal.  */
  bool horizontal;
};

/* Turning a lisp vector value into a pointer to a struct scroll_bar.  */
#define XSCROLL_BAR(vec) ((struct scroll_bar *) XVECTOR (vec))

/* Neomacs display info structure - one per display connection */
struct neomacs_display_info
{
  /* Chain of all neomacs_display_info structures */
  struct neomacs_display_info *next;

  /* The generic display parameters corresponding to this display */
  struct terminal *terminal;

  /* Handle to the Rust display engine */
  struct NeomacsDisplay *display_handle;

  /* This is a cons cell of the form (NAME . FONT-LIST-CACHE).  */
  Lisp_Object name_list_element;

  /* Reference count for frames using this display */
  int reference_count;

  /* Display dimensions */
  int width;
  int height;

  /* Bits per pixel */
  int n_planes;

  /* DPI resolution of this screen */
  double resx, resy;

  /* Root window ID */
  Window root_window;

  /* Root window background pixel */
  unsigned long background_pixel;

  /* Default face colors */
  unsigned long black_pixel;
  unsigned long white_pixel;

  /* Mouse highlight info */
  Mouse_HLInfo mouse_highlight;

  /* Minimum width/height of text area */
  int smallest_char_width;
  int smallest_font_height;

  /* Mask of things that cause the mouse to be grabbed */
  int grabbed;

  /* Xism - resource database placeholder */
  XrmDatabase rdb;

  /* Number of fonts loaded */
  int n_fonts;

  /* Bitmap storage for images */
  struct neomacs_bitmap_record *bitmaps;
  ptrdiff_t bitmaps_size;
  ptrdiff_t bitmaps_last;

  /* Number of planes for image support */
  int n_image_planes;

  /* Focus management */
  struct frame *focus_frame;
  struct frame *x_focus_frame;  /* Alias for X compatibility */
  struct frame *highlight_frame;
  struct frame *x_highlight_frame;  /* Alias for X compatibility */
  struct frame *last_mouse_frame;
  struct frame *last_mouse_motion_frame;

  /* Last mouse position */
  int last_mouse_motion_x;
  int last_mouse_motion_y;

  /* Last user interaction time (for focus stealing prevention) */
  guint32 last_user_time;

  /* Whether the display supports ARGB visuals */
  bool_bf supports_argb : 1;

  /* File descriptor for the display connection (for select/poll) */
  int connection;

  /* GDK Display handle */
  GdkDisplay *gdpy;
};

/* Neomacs output data - one per frame */
struct neomacs_output
{
  /* Pointer back to display info */
  struct neomacs_display_info *display_info;

  /* Winit window identifier */
  uint32_t window_id;

  /* Placeholder for the widget hierarchy (GTK fallback) */
  void *widget;
  void *container;
  void *drawing_area;

  /* Input method context */
  void *im_context;

  /* Whether using GPU-accelerated widget */
  int use_gpu_widget;

  /* Cairo surface for double buffering */
  cairo_surface_t *cr_surface;
  cairo_t *cr_context;

  /* Window IDs - for X compatibility */
  Window window_desc;
  Window parent_desc;
  char explicit_parent;

  /* The frame's current background color */
  unsigned long background_pixel;
  unsigned long foreground_pixel;

  /* Cursor colors */
  unsigned long cursor_pixel;
  unsigned long cursor_foreground_pixel;

  /* Cursors for various mouse positions */
  Emacs_Cursor current_cursor;
  Emacs_Cursor text_cursor;
  Emacs_Cursor nontext_cursor;
  Emacs_Cursor modeline_cursor;
  Emacs_Cursor hand_cursor;
  Emacs_Cursor hourglass_cursor;
  Emacs_Cursor horizontal_drag_cursor;
  Emacs_Cursor vertical_drag_cursor;
  Emacs_Cursor left_edge_cursor;
  Emacs_Cursor top_left_corner_cursor;
  Emacs_Cursor top_edge_cursor;
  Emacs_Cursor top_right_corner_cursor;
  Emacs_Cursor right_edge_cursor;
  Emacs_Cursor bottom_right_corner_cursor;
  Emacs_Cursor bottom_edge_cursor;
  Emacs_Cursor bottom_left_corner_cursor;

  /* Frame geometry */
  int left_pos;
  int top_pos;
  int pixel_width;
  int pixel_height;

  /* Border width */
  int border_width;

  /* Title bar height */
  int title_bar_height;

  /* Menu bar height */
  int menu_bar_height;

  /* Tool bar height */
  int tool_bar_height;

  /* Internal border width */
  int internal_border_width;

  /* Font info */
  struct font *font;
  int baseline_offset;

  /* If a fontset is specified for this frame instead of font, this
     value contains an ID of the fontset, else -1.  */
  int fontset;

  /* Whether frame is visible */
  bool_bf visible : 1;

  /* Whether frame is iconified */
  bool_bf iconified : 1;

  /* Whether frame needs to be redrawn */
  bool_bf needs_exposure : 1;
};

/* Get display info from frame */
#define FRAME_NEOMACS_DISPLAY_INFO(f) \
  ((f)->output_data.neomacs->display_info)

/* For compatibility with X code */
#define FRAME_X_OUTPUT(f)         ((f)->output_data.neomacs)
#define FRAME_OUTPUT_DATA(f)      FRAME_X_OUTPUT (f)
#define FRAME_DISPLAY_INFO(f)     (FRAME_X_OUTPUT (f)->display_info)

/* Get display handle from frame */
#define FRAME_NEOMACS_DISPLAY(f) \
  (FRAME_NEOMACS_DISPLAY_INFO(f)->display_handle)

/* Get output data from frame */
#define FRAME_NEOMACS_OUTPUT(f) ((f)->output_data.neomacs)

/* Frame is a neomacs frame */
#define FRAME_NEOMACS_P(f) ((f)->output_method == output_neomacs)

/* Check if frame has window */
#define FRAME_NEOMACS_WINDOW(f) (FRAME_NEOMACS_OUTPUT(f)->widget)

/* Get pixel dimensions */
#define FRAME_NEOMACS_PIXEL_WIDTH(f) (FRAME_NEOMACS_OUTPUT(f)->pixel_width)
#define FRAME_NEOMACS_PIXEL_HEIGHT(f) (FRAME_NEOMACS_OUTPUT(f)->pixel_height)

/* Default colors */
#define BLACK_PIX_DEFAULT(f) 0x000000
#define WHITE_PIX_DEFAULT(f) 0xFFFFFF

/* Font macros */
#define FRAME_FONT(f) (FRAME_X_OUTPUT(f)->font)
#define FRAME_FONTSET(f) (FRAME_X_OUTPUT(f)->fontset)
#define FRAME_BASELINE_OFFSET(f) (FRAME_X_OUTPUT(f)->baseline_offset)

/* Native window handle */
#define FRAME_NATIVE_WINDOW(f) (FRAME_X_OUTPUT(f)->window_desc)
#define FRAME_X_WINDOW(f) (FRAME_X_OUTPUT(f)->window_desc)

/* Cairo context for drawing */
#define FRAME_CR_SURFACE(f) (FRAME_X_OUTPUT(f)->cr_surface)
#define FRAME_CR_CONTEXT(f) (FRAME_X_OUTPUT(f)->cr_context)

/* External declarations */
extern struct neomacs_display_info *neomacs_display_list;

/* For X compatibility - used by frame.c and others */
#define x_display_list neomacs_display_list

extern void neomacs_delete_terminal (struct terminal *);
extern struct terminal *neomacs_create_terminal (struct neomacs_display_info *);
extern bool neomacs_defined_color (struct frame *, const char *, Emacs_Color *, bool, bool);
extern struct neomacs_display_info *neomacs_open_display (const char *);

/* Frame creation */
extern void neomacs_default_font_parameter (struct frame *, Lisp_Object);
extern struct frame *neomacs_create_frame (Lisp_Object, struct neomacs_display_info *);

/* Font handling */
extern Lisp_Object neomacs_new_font (struct frame *, Lisp_Object, int);

/* Input handling */
extern int neomacs_read_socket (struct terminal *, struct input_event *);

/* Display update hooks */
extern void neomacs_update_begin (struct frame *);
extern void neomacs_update_end (struct frame *);
extern void neomacs_flush_display (struct frame *);

/* Text drawing */
extern void neomacs_draw_glyph_string (struct glyph_string *);
extern void neomacs_clear_frame_area (struct frame *, int, int, int, int);
extern void neomacs_draw_fringe_bitmap (struct window *, struct glyph_row *, struct draw_fringe_bitmap_params *);

/* Cursor */
extern void neomacs_draw_window_cursor (struct window *, struct glyph_row *, int, int, enum text_cursor_kinds, int, bool, bool);

/* Scrolling */
extern void neomacs_scroll_run (struct window *, struct run *);

/* Expose */
extern void neomacs_expose_frame (struct frame *);

/* Mouse */
extern void neomacs_frame_up_to_date (struct frame *);
extern void neomacs_focus_frame (struct frame *, bool);

/* Event queue for input events */
extern void neomacs_evq_enqueue (union buffered_input_event *ev);

/* Cairo integration for font rendering (ftcrfont.c) */
typedef struct _cairo cairo_t;  /* Forward declaration */
extern cairo_t *neomacs_begin_cr_clip (struct frame *f);
extern void neomacs_end_cr_clip (struct frame *f);
extern void neomacs_set_cr_source_with_color (struct frame *f, unsigned long color, bool check_alpha);

/* Display and terminal */
extern struct neomacs_display_info *neomacs_open_display (const char *);
extern struct terminal *neomacs_create_terminal (struct neomacs_display_info *);
extern void neomacs_delete_terminal (struct terminal *);

/* Lisp symbols */
extern void syms_of_neomacsterm (void);
extern void syms_of_neomacsfns (void);

/* Toolbar support */
extern void update_frame_tool_bar (struct frame *f);
extern void free_frame_tool_bar (struct frame *f);

/* Tab bar support */
extern void neomacs_change_tab_bar_height (struct frame *f, int height);

/* Threaded mode support */
extern int neomacs_display_init_threaded_mode (int width, int height, const char *title);
extern int neomacs_display_is_threaded (void);
extern void neomacs_display_shutdown_threaded_mode (void);

/* Note: x_create_gc and x_free_gc are defined as static functions in xfaces.c */

#endif /* HAVE_NEOMACS */

#endif /* _NEOMACSTERM_H_ */
