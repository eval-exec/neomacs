/* Neomacs GPU-accelerated display backend implementation.
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

#include <config.h>

#ifdef HAVE_NEOMACS

#include <dlfcn.h>
#include <string.h>
#include <gtk/gtk.h>

/* Forward declarations for Wayland types (to avoid including wayland headers) */
struct wl_display;

#include "lisp.h"
#include "blockinput.h"
#include "sysselect.h"
#include "neomacsterm.h"
#include "neomacs_display.h"
#include "buffer.h"
#include "coding.h"
#include "window.h"
#include "keyboard.h"
#include "termhooks.h"
#include "termchar.h"
#include "font.h"
#include "dispextern.h"  /* For mark_window_display_accurate, set_window_update_flags */
#include "pdumper.h"
#include "fontset.h"
#include "composite.h"  /* For composition_gstring_from_id, LGSTRING_GLYPH, etc. */
#include "process.h"  /* For add_read_fd, delete_read_fd */

/* List of Neomacs display info structures */
struct neomacs_display_info *neomacs_display_list = NULL;

/* GPU image ID cache - maps Emacs image pointer to GPU image ID */
#define IMAGE_CACHE_SIZE 256
struct neomacs_image_cache_entry {
  struct image *emacs_img;  /* Emacs image (key) */
  uint32_t gpu_id;          /* GPU image ID */
};
static struct neomacs_image_cache_entry neomacs_image_cache[IMAGE_CACHE_SIZE];
static int neomacs_image_cache_count = 0;

/* Forward declarations */
static void neomacs_set_window_size (struct frame *f, bool change_gravity,
                                     int width, int height);
static void neomacs_set_vertical_scroll_bar (struct window *w, int portion,
                                             int whole, int position);
static void neomacs_set_horizontal_scroll_bar (struct window *w, int portion,
                                               int whole, int position);
static void neomacs_condemn_scroll_bars (struct frame *frame);
static void neomacs_redeem_scroll_bar (struct window *w);
static void neomacs_judge_scroll_bars (struct frame *f);
static void neomacs_clear_frame (struct frame *f);
static uint32_t neomacs_get_or_load_image (struct neomacs_display_info *dpyinfo,
                                           struct image *img);

/* Event queue for buffering input events from GTK callbacks */
struct neomacs_event_queue_t
{
  union buffered_input_event *q;
  int nr, cap;
};

static struct neomacs_event_queue_t neomacs_event_q;

/* Enqueue an event from GTK callback to be processed by read_socket */
void
neomacs_evq_enqueue (union buffered_input_event *ev)
{
  struct neomacs_event_queue_t *evq = &neomacs_event_q;
  struct frame *frame;
  struct neomacs_display_info *dpyinfo;

  if (evq->cap == 0)
    {
      evq->cap = 4;
      evq->q = xmalloc (sizeof *evq->q * evq->cap);
    }

  if (evq->nr >= evq->cap)
    {
      evq->cap += evq->cap / 2;
      evq->q = xrealloc (evq->q, sizeof *evq->q * evq->cap);
    }

  evq->q[evq->nr++] = *ev;

  /* Update last user time */
  frame = NULL;
  if (WINDOWP (ev->ie.frame_or_window))
    frame = WINDOW_XFRAME (XWINDOW (ev->ie.frame_or_window));
  if (FRAMEP (ev->ie.frame_or_window))
    frame = XFRAME (ev->ie.frame_or_window);

  if (frame)
    {
      dpyinfo = FRAME_DISPLAY_INFO (frame);
      if (dpyinfo && dpyinfo->last_user_time < ev->ie.timestamp)
        dpyinfo->last_user_time = ev->ie.timestamp;
    }

  /* Signal Emacs that input is available */
  raise (SIGIO);
}

/* Flush events from queue to Emacs keyboard buffer */
static int
neomacs_evq_flush (struct input_event *hold_quit)
{
  struct neomacs_event_queue_t *evq = &neomacs_event_q;
  int n = 0;

  while (evq->nr > 0)
    {
      /* kbd_buffer_store_buffered_event may do longjmp, so
         we need to shift event queue first.  Bug#52941 */
      union buffered_input_event ev = evq->q[0];
      int i;
      for (i = 1; i < evq->nr; i++)
        evq->q[i - 1] = evq->q[i];
      evq->nr--;

      kbd_buffer_store_buffered_event (&ev, hold_quit);
      n++;
    }

  return n;
}

/* Forward declarations */
extern frame_parm_handler neomacs_frame_parm_handlers[];

/* Prototypes for internal functions */
static void neomacs_initialize_display_info (struct neomacs_display_info *);
static const char *neomacs_get_string_resource (void *, const char *, const char *);
static void neomacs_make_frame_visible_invisible (struct frame *, bool);
static void neomacs_after_update_window_line (struct window *, struct glyph_row *);
static void neomacs_update_window_begin (struct window *);
static void neomacs_update_window_end (struct window *, bool, bool);
static void neomacs_draw_vertical_window_border (struct window *, int, int, int);

/* The redisplay interface for Neomacs frames - statically initialized.
   All function pointers are declared extern in neomacsterm.h */
static struct redisplay_interface neomacs_redisplay_interface = {
  .frame_parm_handlers = neomacs_frame_parm_handlers,
  .produce_glyphs = gui_produce_glyphs,
  .write_glyphs = gui_write_glyphs,
  .insert_glyphs = gui_insert_glyphs,
  .clear_end_of_line = gui_clear_end_of_line,
  .scroll_run_hook = neomacs_scroll_run,
  .after_update_window_line_hook = neomacs_after_update_window_line,
  .update_window_begin_hook = neomacs_update_window_begin,
  .update_window_end_hook = neomacs_update_window_end,
  .flush_display = neomacs_flush_display,
  .clear_window_mouse_face = gui_clear_window_mouse_face,
  .get_glyph_overhangs = gui_get_glyph_overhangs,
  .fix_overlapping_area = gui_fix_overlapping_area,
  .draw_fringe_bitmap = neomacs_draw_fringe_bitmap,
  .define_fringe_bitmap = NULL,
  .destroy_fringe_bitmap = NULL,
  .compute_glyph_string_overhangs = NULL,
  .draw_glyph_string = neomacs_draw_glyph_string,
  .define_frame_cursor = NULL,
  .clear_frame_area = neomacs_clear_frame_area,
  .clear_under_internal_border = NULL,
  .draw_window_cursor = neomacs_draw_window_cursor,
  .draw_vertical_window_border = neomacs_draw_vertical_window_border,
  .draw_window_divider = NULL,
  .shift_glyphs_for_insert = NULL,
  .show_hourglass = NULL,
  .hide_hourglass = NULL,
  .default_font_parameter = NULL,
};


/* ============================================================================
 * Display Initialization
 * ============================================================================ */

/* Forward declaration for wakeup handler (defined in Threaded Mode Support section) */
static void neomacs_display_wakeup_handler (int fd, void *data);
/* Forward declarations for image.c terminal hooks */
static void neomacs_query_colors (struct frame *, Emacs_Color *, int);
static void neomacs_query_frame_background_color (struct frame *, Emacs_Color *);
static void neomacs_free_pixmap (struct frame *, Emacs_Pixmap);

/* Create a new Neomacs display connection */
struct neomacs_display_info *
neomacs_open_display (const char *display_name)
{
  struct neomacs_display_info *dpyinfo;
  static bool gtk_initialized = false;

  /* Initialize GTK if not already done */
  if (!gtk_initialized)
    {
      gtk_init ();
      gtk_initialized = true;
    }

  dpyinfo = xzalloc (sizeof *dpyinfo);
  neomacs_initialize_display_info (dpyinfo);

  /* Initialize the Rust display engine in threaded mode */
  int wakeup_fd = neomacs_display_init_threaded (dpyinfo->width, dpyinfo->height, "Emacs");

  if (wakeup_fd < 0)
    {
      xfree (dpyinfo);
      error ("Failed to initialize Neomacs threaded display engine");
    }

  /* Get display handle for frame operations */
  dpyinfo->display_handle = neomacs_display_get_threaded_handle ();
  if (!dpyinfo->display_handle)
    {
      xfree (dpyinfo);
      error ("Failed to get threaded display handle");
    }

  /* Set background color */
  neomacs_display_set_background (dpyinfo->display_handle, dpyinfo->background_pixel);

  /* Store the wakeup fd for event loop integration */
  dpyinfo->connection = wakeup_fd;

  /* Register wakeup handler with Emacs event loop */
  add_read_fd (wakeup_fd, neomacs_display_wakeup_handler, dpyinfo);

  /* Add to display list */
  dpyinfo->next = neomacs_display_list;
  neomacs_display_list = dpyinfo;

  return dpyinfo;
}

/* Initialize display info defaults */
static void
neomacs_initialize_display_info (struct neomacs_display_info *dpyinfo)
{
  dpyinfo->reference_count = 0;
  dpyinfo->width = 800;
  dpyinfo->height = 600;
  dpyinfo->n_planes = 24;
  dpyinfo->black_pixel = 0x000000;
  dpyinfo->white_pixel = 0xffffff;
  dpyinfo->background_pixel = 0x000000;  /* Default to black for dark theme */
  dpyinfo->smallest_char_width = 8;
  dpyinfo->smallest_font_height = 16;
  dpyinfo->supports_argb = true;
  dpyinfo->connection = -1;
  dpyinfo->gdpy = NULL;

  /* Initialize DPI to a reasonable default - required for font sizing */
  dpyinfo->resx = 96;
  dpyinfo->resy = 96;

  /* Get the GDK display */
  GdkDisplay *gdpy = gdk_display_get_default ();
  if (!gdpy)
    {
      /* No display yet, try to open default */
      gdpy = gdk_display_open (NULL);
    }
  dpyinfo->gdpy = gdpy;

  if (gdpy)
    {
      /* Get the display connection fd for event handling.
         This depends on whether we're running on X11 or Wayland.
         Use dlsym to avoid compile-time dependencies on X11/Wayland headers.  */
      void *handle = dlopen (NULL, RTLD_LAZY);
      const char *type_name = G_OBJECT_TYPE_NAME (gdpy);

      if (0) fprintf (stderr, "DEBUG: GDK display type: %s\n", type_name ? type_name : "NULL");

      if (handle && type_name)
	{
	  /* Check for X11 display */
	  if (strstr (type_name, "X11"))
	    {
	      void *(*get_xdisplay) (GdkDisplay *) = dlsym (handle, "gdk_x11_display_get_xdisplay");
	      int (*conn_number) (void *) = dlsym (handle, "XConnectionNumber");
	      if (0) fprintf (stderr, "DEBUG: X11: get_xdisplay=%p, conn_number=%p\n",
		       (void *) get_xdisplay, (void *) conn_number);
	      if (get_xdisplay && conn_number)
		{
		  void *xdpy = get_xdisplay (gdpy);
		  if (0) fprintf (stderr, "DEBUG: X11: xdpy=%p\n", xdpy);
		  if (xdpy)
		    dpyinfo->connection = conn_number (xdpy);
		}
	    }
	  /* Check for Wayland display */
	  else if (strstr (type_name, "Wayland"))
	    {
	      struct wl_display *(*get_wl_display) (GdkDisplay *)
		= dlsym (handle, "gdk_wayland_display_get_wl_display");
	      int (*get_fd) (struct wl_display *) = dlsym (handle, "wl_display_get_fd");
	      if (0) fprintf (stderr, "DEBUG: Wayland: get_wl_display=%p, get_fd=%p\n",
		       (void *) get_wl_display, (void *) get_fd);
	      if (get_wl_display && get_fd)
		{
		  struct wl_display *wl_dpy = get_wl_display (gdpy);
		  if (0) fprintf (stderr, "DEBUG: Wayland: wl_dpy=%p\n", (void *) wl_dpy);
		  if (wl_dpy)
		    dpyinfo->connection = get_fd (wl_dpy);
		}
	    }
	}

      if (0) fprintf (stderr, "DEBUG: Display connection fd: %d\n", dpyinfo->connection);
    }
}


/* ============================================================================
 * Terminal Creation and Deletion
 * ============================================================================ */

/* Delete a Neomacs terminal */
void
neomacs_delete_terminal (struct terminal *terminal)
{
  struct neomacs_display_info *dpyinfo = terminal->display_info.neomacs;

  if (!dpyinfo)
    return;

  /* Shutdown the Rust display engine */
  if (dpyinfo->display_handle)
    {
      neomacs_display_shutdown (dpyinfo->display_handle);
      dpyinfo->display_handle = NULL;
    }

  /* Remove from display list */
  if (dpyinfo == neomacs_display_list)
    neomacs_display_list = dpyinfo->next;
  else
    {
      struct neomacs_display_info *tail;
      for (tail = neomacs_display_list; tail; tail = tail->next)
        if (tail->next == dpyinfo)
          {
            tail->next = dpyinfo->next;
            break;
          }
    }

  xfree (dpyinfo);
}

/* Stub for popup menus - not yet implemented for Neomacs */
static Lisp_Object
neomacs_menu_show (struct frame *f, int x, int y, int menuflags,
                   Lisp_Object title, const char **error_name)
{
  /* For now, just return Qnil to indicate no selection */
  *error_name = "Popup menus not yet implemented for Neomacs";
  return Qnil;
}

/* Create a terminal for a Neomacs display */
struct terminal *
neomacs_create_terminal (struct neomacs_display_info *dpyinfo)
{
  struct terminal *terminal;

  terminal = create_terminal (output_neomacs, &neomacs_redisplay_interface);

  terminal->display_info.neomacs = dpyinfo;
  dpyinfo->terminal = terminal;

  terminal->name = xstrdup ("neomacs");

  /* Set up keyboard for this terminal */
  terminal->kboard = allocate_kboard (Qneomacs);
  /* Don't let the initial kboard remain current longer than necessary. */
  if (current_kboard == initial_kboard)
    current_kboard = terminal->kboard;
  terminal->kboard->reference_count++;

  /* Set up terminal hooks */
  terminal->delete_terminal_hook = neomacs_delete_terminal;
  terminal->clear_frame_hook = neomacs_clear_frame;
  terminal->update_begin_hook = neomacs_update_begin;
  terminal->update_end_hook = neomacs_update_end;
  terminal->defined_color_hook = neomacs_defined_color;
  terminal->query_colors = neomacs_query_colors;
  terminal->query_frame_background_color = neomacs_query_frame_background_color;
  terminal->free_pixmap = neomacs_free_pixmap;
  terminal->get_string_resource_hook = neomacs_get_string_resource;
  terminal->set_new_font_hook = neomacs_new_font;
  terminal->read_socket_hook = neomacs_read_socket;
  terminal->frame_visible_invisible_hook = neomacs_make_frame_visible_invisible;
  terminal->menu_show_hook = neomacs_menu_show;
  terminal->change_tab_bar_height_hook = neomacs_change_tab_bar_height;
  terminal->set_window_size_hook = neomacs_set_window_size;

  /* Scroll bar hooks */
  terminal->set_vertical_scroll_bar_hook = neomacs_set_vertical_scroll_bar;
  terminal->set_horizontal_scroll_bar_hook = neomacs_set_horizontal_scroll_bar;
  terminal->condemn_scroll_bars_hook = neomacs_condemn_scroll_bars;
  terminal->redeem_scroll_bar_hook = neomacs_redeem_scroll_bar;
  terminal->judge_scroll_bars_hook = neomacs_judge_scroll_bars;

  /* Register the display connection fd for event handling */
  if (dpyinfo->connection >= 0)
    {
      add_keyboard_wait_descriptor (dpyinfo->connection);
if (0) fprintf (stderr, "DEBUG: Registered fd %d with add_keyboard_wait_descriptor\n",
               dpyinfo->connection);
    }
  else
if (0) fprintf (stderr, "DEBUG: WARNING: No valid connection fd to register!\n");

  /* More hooks would be set up here... */

  return terminal;
}


/* ============================================================================
 * Frame Update Hooks
 * ============================================================================ */

/* Called at the start of updating a frame */
void
neomacs_update_begin (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  /* Don't mark windows inaccurate - let Emacs do incremental updates.
     Our GPU backend accumulates glyphs and handles overlaps.  */

  if (dpyinfo && dpyinfo->display_handle)
    {
      /* Clear all cursors at frame start to prevent ghost cursors
         when focus changes between windows */
      neomacs_display_clear_all_cursors (dpyinfo->display_handle);

      /* No need to clear borders here — begin_frame calls clear_all()
         which clears everything, and neomacs_extract_full_frame()
         re-adds borders for all leaf windows each frame. */

      /* Use window-targeted begin_frame if we have a winit window */
      struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
      if (output && output->window_id > 0)
        neomacs_display_begin_frame_window (dpyinfo->display_handle, output->window_id,
                                            (float) FRAME_COLUMN_WIDTH (f),
                                            (float) FRAME_LINE_HEIGHT (f),
                                            FRAME_FONT (f) ? (float) FRAME_FONT (f)->pixel_size : 14.0f);
      else
        neomacs_display_begin_frame (dpyinfo->display_handle);
    }
}

/* ============================================================================
 * Matrix Walker: Full-Frame Glyph Extraction
 * ============================================================================ */

/* Convert Emacs-internal font weight (normal=80, bold=200) to CSS/OpenType
   weight scale (normal=400, bold=700) that cosmic-text expects. */
static int
emacs_weight_to_css (int emacs_weight)
{
  /* Emacs weight table (from font.c):
       0=thin, 20=ultra-light, 40=extra-light, 50=light, 55=semi-light,
       80=normal, 100=medium, 180=semi-bold, 200=bold, 205=extra-bold,
       210=ultra-bold */
  if (emacs_weight <= 0)   return 100;  /* Thin */
  if (emacs_weight <= 20)  return 200;  /* Ultra/Extra-light */
  if (emacs_weight <= 50)  return 300;  /* Light */
  if (emacs_weight <= 55)  return 350;  /* Semi-light */
  if (emacs_weight <= 80)  return 400;  /* Normal */
  if (emacs_weight <= 100) return 500;  /* Medium */
  if (emacs_weight <= 180) return 600;  /* Semi-bold */
  if (emacs_weight <= 200) return 700;  /* Bold */
  if (emacs_weight <= 205) return 800;  /* Extra-bold */
  return 900;                           /* Ultra-bold/Black */
}

/* Helper: resolve face and send it to Rust via set_face FFI.
   This mirrors what neomacs_draw_glyph_string does for face setup. */
static void
neomacs_send_face (void *handle, struct frame *f, struct face *face)
{
  if (!face)
    return;

  unsigned long fg = face->foreground;
  unsigned long bg = face->background;

  if (face->foreground_defaulted_p)
    fg = FRAME_FOREGROUND_PIXEL (f);
  if (face->background_defaulted_p)
    bg = FRAME_BACKGROUND_PIXEL (f);

  uint32_t fg_rgb = ((RED_FROM_ULONG (fg) << 16) |
                     (GREEN_FROM_ULONG (fg) << 8) |
                     BLUE_FROM_ULONG (fg));
  uint32_t bg_rgb = ((RED_FROM_ULONG (bg) << 16) |
                     (GREEN_FROM_ULONG (bg) << 8) |
                     BLUE_FROM_ULONG (bg));

  /* Get the actual font family from the realized font object, not the
     logical face.  face->lface[LFACE_FAMILY_INDEX] is the requested family
     (e.g. "Hack"), but Emacs's fontset system may select a different font
     for certain characters (e.g. "Noto Color Emoji" for emoji).  The
     realized font in face->font has the actual family used.  */
  const char *font_family = NULL;
  if (face->font)
    {
      Lisp_Object family_attr = face->font->props[FONT_FAMILY_INDEX];
      if (!NILP (family_attr) && SYMBOLP (family_attr))
        font_family = SSDATA (SYMBOL_NAME (family_attr));
    }
  if (!font_family && face->lface != NULL)
    {
      Lisp_Object family_attr = face->lface[LFACE_FAMILY_INDEX];
      if (!NILP (family_attr) && STRINGP (family_attr))
        font_family = SSDATA (family_attr);
    }

  int font_weight = 400;
  Lisp_Object weight_attr = face->lface[LFACE_WEIGHT_INDEX];
  if (!NILP (weight_attr) && SYMBOLP (weight_attr))
    {
      int w = FONT_WEIGHT_NAME_NUMERIC (weight_attr);
      if (w > 0) font_weight = emacs_weight_to_css (w);
    }

  int is_italic = 0;
  Lisp_Object slant_attr = face->lface[LFACE_SLANT_INDEX];
  if (!NILP (slant_attr) && SYMBOLP (slant_attr))
    {
      int s = FONT_SLANT_NAME_NUMERIC (slant_attr);
      if (s != 100) is_italic = 1;
    }

  int underline_style = 0;
  uint32_t underline_color = fg_rgb;
  if (face->underline != FACE_NO_UNDERLINE)
    {
      switch (face->underline)
        {
        case FACE_UNDERLINE_SINGLE: underline_style = 1; break;
        case FACE_UNDERLINE_WAVE: underline_style = 2; break;
        case FACE_UNDERLINE_DOUBLE_LINE: underline_style = 3; break;
        case FACE_UNDERLINE_DOTS: underline_style = 4; break;
        case FACE_UNDERLINE_DASHES: underline_style = 5; break;
        default: underline_style = 1; break;
        }
      if (!face->underline_defaulted_p)
        underline_color = ((RED_FROM_ULONG (face->underline_color) << 16) |
                           (GREEN_FROM_ULONG (face->underline_color) << 8) |
                           BLUE_FROM_ULONG (face->underline_color));
    }

  int box_type = 0;
  uint32_t box_color = fg_rgb;
  int box_line_width = 0;
  if (face->box != FACE_NO_BOX)
    {
      box_type = 1;
      box_line_width = eabs (face->box_vertical_line_width);
      if (box_line_width == 0) box_line_width = 1;
      /* Always use face->box_color: the face realization code sets it to the
         correct value (either foreground when defaulted, or the user-specified
         color from :box (:color ...)).  box_color_defaulted_p may not be
         cleared even when a custom color is specified (xfaces.c bug). */
      box_color = ((RED_FROM_ULONG (face->box_color) << 16) |
                   (GREEN_FROM_ULONG (face->box_color) << 8) |
                   BLUE_FROM_ULONG (face->box_color));
    }

  int box_corner_radius = 0;
  if (face->box != FACE_NO_BOX)
    box_corner_radius = face->box_corner_radius;

  int strike_through = face->strike_through_p ? 1 : 0;
  uint32_t strike_through_color = fg_rgb;
  if (strike_through && !face->strike_through_color_defaulted_p)
    strike_through_color = ((RED_FROM_ULONG (face->strike_through_color) << 16) |
                            (GREEN_FROM_ULONG (face->strike_through_color) << 8) |
                            BLUE_FROM_ULONG (face->strike_through_color));

  int overline = face->overline_p ? 1 : 0;
  uint32_t overline_color = fg_rgb;
  if (overline && !face->overline_color_defaulted_p)
    overline_color = ((RED_FROM_ULONG (face->overline_color) << 16) |
                      (GREEN_FROM_ULONG (face->overline_color) << 8) |
                      BLUE_FROM_ULONG (face->overline_color));

  int font_size = 14;
  if (face->font)
    font_size = face->font->pixel_size;

  neomacs_display_set_face (handle, face->id,
                            fg_rgb, bg_rgb, font_family,
                            font_weight, is_italic, font_size,
                            underline_style, underline_color,
                            box_type, box_color, box_line_width,
                            box_corner_radius,
                            strike_through, strike_through_color,
                            overline, overline_color);
}

/* Callback for foreach_window: extract all visible glyphs from a window's
   current_matrix and send them to the Rust display engine via FFI. */
static bool
neomacs_extract_window_glyphs (struct window *w, void *user_data)
{
  struct frame *f = XFRAME (w->frame);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  void *handle = dpyinfo->display_handle;
  struct glyph_matrix *matrix = w->current_matrix;

  if (!matrix || !handle)
    return true;  /* continue iterating */

  /* Add this window to the scene */
  int win_x = WINDOW_LEFT_EDGE_X (w);
  int win_y = WINDOW_TOP_EDGE_Y (w);
  int win_w = WINDOW_PIXEL_WIDTH (w);
  int win_h = WINDOW_PIXEL_HEIGHT (w);
  unsigned long bg = FRAME_BACKGROUND_PIXEL (f);
  int selected = (w == XWINDOW (f->selected_window)) ? 1 : 0;

  neomacs_display_add_window (handle,
                              (intptr_t) w,
                              (float) win_x, (float) win_y,
                              (float) win_w, (float) win_h,
                              (uint32_t) bg, selected);

  /* Per-window metadata for animation detection */
  {
    uintptr_t buf_id = 0;
    ptrdiff_t win_start = 0;
    if (BUFFERP (w->contents))
      {
        buf_id = (uintptr_t) XBUFFER (w->contents);
        if (MARKERP (w->start))
          win_start = marker_position (w->start);
      }
    neomacs_display_add_window_info (
        handle,
        (int64_t)(intptr_t) w,
        (uint64_t) buf_id,
        (int64_t) win_start,
        (float) win_x, (float) win_y,
        (float) win_w, (float) win_h,
        (float) WINDOW_MODE_LINE_HEIGHT (w),
        selected);
  }

  /* Walk all rows in current_matrix */
  for (int row_idx = 0; row_idx < matrix->nrows; row_idx++)
    {
      struct glyph_row *row = &matrix->rows[row_idx];
      if (!row->enabled_p)
        continue;

      /* Convert window-relative Y to frame-absolute Y */
      int frame_y = WINDOW_TO_FRAME_PIXEL_Y (w, row->y);
      int is_mode_line = row->mode_line_p;
      int is_tab_line = row->tab_line_p;
      int last_face_id = -1;

      /* Walk glyphs in all 3 areas: LEFT_MARGIN, TEXT, RIGHT_MARGIN */
      for (int area = LEFT_MARGIN_AREA; area < LAST_AREA; area++)
        {
          struct glyph *glyph = row->glyphs[area];
          struct glyph *end = glyph + row->used[area];
          int glyph_x;

          /* Calculate starting X for this area */
          if (area == TEXT_AREA)
            glyph_x = window_box_left (w, TEXT_AREA);
          else if (area == LEFT_MARGIN_AREA)
            glyph_x = window_box_left (w, LEFT_MARGIN_AREA);
          else
            glyph_x = window_box_left (w, RIGHT_MARGIN_AREA);

          while (glyph < end)
            {
              /* Resolve the face for this glyph */
              struct face *face = FACE_FROM_ID_OR_NULL (f, glyph->face_id);
              if (!face)
                face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);

              /* Send face if it changed */
              if (face && (int) glyph->face_id != last_face_id)
                {
                  neomacs_send_face (handle, f, face);
                  last_face_id = glyph->face_id;
                }

              /* Set up row context via begin_row */
              neomacs_display_begin_row (handle,
                                         frame_y,
                                         glyph_x,
                                         row->height,
                                         row->ascent,
                                         is_mode_line || is_tab_line ? 1 : 0,
                                         0);

              switch (glyph->type)
                {
                case CHAR_GLYPH:
                  {
                    unsigned int charcode = glyph->u.ch;
                    int ascent_val = face && face->font ? FONT_BASE (face->font) : row->ascent;
                    int descent_val = face && face->font ? FONT_DESCENT (face->font) : 0;
                    neomacs_display_add_char_glyph (handle, charcode,
                                                    glyph->face_id,
                                                    glyph->pixel_width,
                                                    ascent_val, descent_val);
                  }
                  break;

                case COMPOSITE_GLYPH:
                  {
                    /* For automatic compositions (e.g. emoji), extract characters
                       from the composition using lgstring (grapheme cluster string). */
                    if (glyph->u.cmp.automatic)
                      {
                        int cmp_id = glyph->u.cmp.id;
                        int cmp_from = glyph->slice.cmp.from;
                        Lisp_Object gstring = composition_gstring_from_id (cmp_id);

                        if (!NILP (gstring))
                          {
                            Lisp_Object glyph_obj = LGSTRING_GLYPH (gstring, cmp_from);
                            if (!NILP (glyph_obj))
                              {
                                unsigned int charcode = LGLYPH_CHAR (glyph_obj);

                                /* Use face_for_char to resolve the correct font face
                                   for this character.  For emoji, this finds the emoji
                                   font (e.g. Noto Color Emoji) via fontset fallback,
                                   rather than using the base text font (e.g. Hack).  */
                                int char_face_id = face_for_char (f, face, charcode, -1, Qnil);
                                struct face *char_face = FACE_FROM_ID_OR_NULL (f, char_face_id);
                                if (!char_face)
                                  char_face = face;

                                if (char_face_id != last_face_id)
                                  {
                                    neomacs_send_face (handle, f, char_face);
                                    last_face_id = char_face_id;
                                  }

                                int ascent_val = char_face->font ? FONT_BASE (char_face->font) : row->ascent;
                                int descent_val = char_face->font ? FONT_DESCENT (char_face->font) : 0;
                                neomacs_display_add_char_glyph (handle, charcode,
                                                                (uint32_t) char_face_id,
                                                                glyph->pixel_width,
                                                                ascent_val, descent_val);
                              }
                          }
                      }
                    else
                      {
                        /* Non-automatic (static) composition - use composition table */
                        int cmp_id = glyph->u.cmp.id;
                        struct composition *cmp = composition_table[cmp_id];
                        if (cmp && cmp->glyph_len > 0)
                          {
                            unsigned int charcode = COMPOSITION_GLYPH (cmp, 0);

                            int char_face_id = face_for_char (f, face, charcode, -1, Qnil);
                            struct face *char_face = FACE_FROM_ID_OR_NULL (f, char_face_id);
                            if (!char_face)
                              char_face = face;

                            if (char_face_id != last_face_id)
                              {
                                neomacs_send_face (handle, f, char_face);
                                last_face_id = char_face_id;
                              }

                            int ascent_val = char_face->font ? FONT_BASE (char_face->font) : row->ascent;
                            int descent_val = char_face->font ? FONT_DESCENT (char_face->font) : 0;
                            neomacs_display_add_char_glyph (handle, charcode,
                                                            (uint32_t) char_face_id,
                                                            glyph->pixel_width,
                                                            ascent_val, descent_val);
                          }
                      }
                  }
                  break;

                case GLYPHLESS_GLYPH:
                  {
                    neomacs_display_add_char_glyph (handle, ' ',
                                                    glyph->face_id,
                                                    glyph->pixel_width,
                                                    row->ascent, 0);
                  }
                  break;

                case STRETCH_GLYPH:
                  neomacs_display_add_stretch_glyph (handle,
                                                      glyph->pixel_width,
                                                      row->height,
                                                      glyph->face_id);
                  break;

                case IMAGE_GLYPH:
                  {
                    /* Look up the image and get its GPU ID */
                    struct image *img = IMAGE_FROM_ID (f, glyph->u.img_id);
                    if (img)
                      {
                        uint32_t gpu_id = neomacs_get_or_load_image (dpyinfo, img);
                        if (gpu_id != 0)
                          {
                            int img_w = img->width > 0 ? img->width : glyph->pixel_width;
                            int img_h = img->height > 0 ? img->height : (glyph->ascent + glyph->descent);
                            neomacs_display_add_image_glyph (handle, gpu_id,
                                                              img_w, img_h);
                          }
                      }
                  }
                  break;

#ifdef HAVE_NEOMACS
                case VIDEO_GLYPH:
                  {
                    int glyph_height = glyph->ascent + glyph->descent;
                    neomacs_display_add_video_glyph (handle,
                                                      glyph->u.video_id,
                                                      glyph->pixel_width,
                                                      glyph_height > 0 ? glyph_height : row->height);
                  }
                  break;

                case WEBKIT_GLYPH:
                  {
                    int glyph_height = glyph->ascent + glyph->descent;
                    neomacs_display_add_wpe_glyph (handle,
                                                    glyph->u.webkit_id,
                                                    glyph->pixel_width,
                                                    glyph_height > 0 ? glyph_height : row->height);
                  }
                  break;
#endif

                default:
                  /* XWIDGET_GLYPH etc. - skip */
                  break;
                }

              glyph_x += glyph->pixel_width;
              glyph++;
            }
        }
    }

  return true;  /* continue iterating to next window */
}

/* Walk current_matrix for ALL windows in the frame and extract complete
   glyph data.  Called from neomacs_update_end after Emacs has finished
   all window updates.  This replaces the incremental glyph accumulation
   model with a full-frame snapshot. */
static void
neomacs_extract_full_frame (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (!dpyinfo || !dpyinfo->display_handle)
    return;

  /* Only for GPU widget mode */
  if (!output || !output->use_gpu_widget)
    return;

  /* Sync the frame background color from the default face.  When themes
     change (e.g. load-theme wombat), the default face background updates
     but FRAME_BACKGROUND_PIXEL and the renderer's clear color don't.
     Update them here so LoadOp::Clear uses the correct color.  */
  {
    struct face *default_face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
    if (default_face && !default_face->background_defaulted_p)
      {
        unsigned long new_bg = default_face->background;
        if (new_bg != FRAME_BACKGROUND_PIXEL (f))
          {
            FRAME_BACKGROUND_PIXEL (f) = new_bg;
            uint32_t bg_rgb = ((RED_FROM_ULONG (new_bg) << 16) |
                               (GREEN_FROM_ULONG (new_bg) << 8) |
                               BLUE_FROM_ULONG (new_bg));
            neomacs_display_set_background (dpyinfo->display_handle, bg_rgb);
          }
      }
  }

  /* Walk all leaf windows and extract their glyphs from current_matrix.
     We can't use foreach_window (it's static in window.c), so traverse
     the window tree ourselves. */
  {
    /* Simple iterative tree walk using Emacs window tree structure */
    struct window *root = XWINDOW (FRAME_ROOT_WINDOW (f));
    /* Stack-based traversal: find all leaf windows */
    struct window *stack[64];
    int sp = 0;
    stack[sp++] = root;

    while (sp > 0)
      {
        struct window *w = stack[--sp];
        if (WINDOWP (w->contents))
          {
            /* Internal window - push children */
            struct window *child = XWINDOW (w->contents);
            while (child)
              {
                if (sp < 64)
                  stack[sp++] = child;
                child = NILP (child->next) ? NULL : XWINDOW (child->next);
              }
          }
        else
          {
            /* Leaf window - extract glyphs */
            neomacs_extract_window_glyphs (w, NULL);
          }
      }
  }

  /* The tab-bar window is a pseudo-window NOT part of the root window tree.
     Extract it separately so tab-bar items are rendered. */
  if (WINDOWP (f->tab_bar_window))
    {
      struct window *tw = XWINDOW (f->tab_bar_window);
      if (tw->current_matrix)
        neomacs_extract_window_glyphs (tw, NULL);
    }

  /* The minibuffer/echo area window is NOT part of the root window tree.
     Extract it separately so echo area text is rendered. */
  {
    Lisp_Object mini = FRAME_MINIBUF_WINDOW (f);
    if (!NILP (mini))
      {
        struct window *mw = XWINDOW (mini);
        if (FRAME_HAS_MINIBUF_P (f)
            && mw->current_matrix)
          neomacs_extract_window_glyphs (mw, NULL);
      }
  }

  /* Draw vertical window borders for all leaf windows.
     Borders are cleared by begin_frame/clear_all each frame,
     so we must re-add them here as part of the full-frame extraction. */
  if (!FRAME_HAS_VERTICAL_SCROLL_BARS (f) && !FRAME_RIGHT_DIVIDER_WIDTH (f))
    {
      struct window *bstack[64];
      int bsp = 0;
      bstack[bsp++] = XWINDOW (FRAME_ROOT_WINDOW (f));

      while (bsp > 0)
        {
          struct window *w = bstack[--bsp];
          if (WINDOWP (w->contents))
            {
              struct window *child = XWINDOW (w->contents);
              while (child)
                {
                  if (bsp < 64)
                    bstack[bsp++] = child;
                  child = NILP (child->next) ? NULL : XWINDOW (child->next);
                }
            }
          else
            {
              /* Leaf window — draw its vertical borders */
              gui_draw_vertical_border (w);
            }
        }
    }
}

/* Called at the end of updating a frame */
void
neomacs_update_end (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (dpyinfo && dpyinfo->display_handle)
    {
      /* Extract complete frame from current_matrix (matrix-based full-frame rendering).
         This walks ALL windows and sends ALL visible glyphs to Rust. */
      neomacs_extract_full_frame (f);

      /* Signal end of frame to Rust (sends frame to render thread) */
      if (output && output->window_id > 0)
        neomacs_display_end_frame_window (dpyinfo->display_handle, output->window_id);
      else
        neomacs_display_end_frame (dpyinfo->display_handle);
    }

  /* Queue a redraw of the drawing area */
  if (output && output->drawing_area)
    {
      if (output->use_gpu_widget && dpyinfo && dpyinfo->display_handle)
        neomacs_display_render_to_widget (dpyinfo->display_handle, output->drawing_area);

      gtk_widget_queue_draw (GTK_WIDGET (output->drawing_area));
    }
}

/* Called at the beginning of updating a window */
static void
neomacs_update_window_begin (struct window *w)
{
  struct frame *f = XFRAME (WINDOW_FRAME (w));
  struct neomacs_display_info *dpyinfo;
  struct neomacs_output *output;

  if (!FRAME_NEOMACS_P (f))
    return;

  dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  output = FRAME_NEOMACS_OUTPUT (f);

  /* Disable matrix-level scroll optimization.  With full-frame rendering
     from current_matrix, we don't need scroll optimization.  Keeping this
     ensures Emacs redraws all changed rows rather than trying to reuse them. */
  if (w->desired_matrix)
    w->desired_matrix->no_scrolling_p = true;

  /* Add this window to the Rust scene graph */
  if (dpyinfo && dpyinfo->display_handle)
    {
      int x = WINDOW_LEFT_EDGE_X (w);
      int y = WINDOW_TOP_EDGE_Y (w);
      int width = WINDOW_PIXEL_WIDTH (w);
      int height = WINDOW_PIXEL_HEIGHT (w);
      unsigned long bg = FRAME_BACKGROUND_PIXEL (f);
      int selected = (w == XWINDOW (f->selected_window)) ? 1 : 0;

      neomacs_display_add_window (dpyinfo->display_handle,
                                  (intptr_t) w,  /* window_id */
                                  (float) x, (float) y,
                                  (float) width, (float) height,
                                  (uint32_t) bg,
                                  selected);

      /* With full-frame rendering, the entire glyph buffer is cleared at
         begin_frame and rebuilt from current_matrix.  No incremental cleanup
         needed. */
    }
}

/* Called at the end of updating a window */
static void
neomacs_update_window_end (struct window *w, bool cursor_on_p,
                           bool mouse_face_overwritten_p)
{
  /* Nothing special needed for now */
}

/* Flush pending output to display */
void
neomacs_flush_display (struct frame *f)
{
  struct neomacs_output *output;

  if (!FRAME_NEOMACS_P (f))
    return;

  output = FRAME_NEOMACS_OUTPUT (f);

  /* Queue a redraw of the drawing area */
  if (output && output->drawing_area)
    {
      gtk_widget_queue_draw (GTK_WIDGET (output->drawing_area));
    }
}

/* Set window size (called from set-frame-size and similar) */
static void
neomacs_set_window_size (struct frame *f, bool change_gravity,
                         int width, int height)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!output || !output->widget)
    return;


  block_input ();

  /* Update frame's pixel dimensions */
  output->pixel_width = width;
  output->pixel_height = height;

  /* For GPU widget, also update the glyph buffer dimensions */
  if (output->use_gpu_widget && dpyinfo && dpyinfo->display_handle)
    {
      neomacs_display_resize (dpyinfo->display_handle, width, height);
      /* Clear glyph buffer to force full redraw after resize */
      neomacs_display_clear_all_glyphs (dpyinfo->display_handle);
    }

  /* Set the widget size request - this tells GTK the minimum size */
  if (output->drawing_area)
    {
      gtk_widget_set_size_request (GTK_WIDGET (output->drawing_area),
                                   width, height);
    }

  /* For GTK4 top-level windows, set default size and queue resize */
  if (GTK_IS_WINDOW (output->widget))
    {
      GtkWindow *window = GTK_WINDOW (output->widget);

      /* Set the default size */
      gtk_window_set_default_size (window, width, height);

      /* For GTK4, we need to also set resizable and then queue resize */
      gtk_window_set_resizable (window, TRUE);
      gtk_widget_queue_resize (GTK_WIDGET (window));
    }

  /* Mark frame for redisplay */
  SET_FRAME_GARBAGED (f);

  unblock_input ();
}

/* ============================================================================
 * Scroll Bar Support (Stubs)
 * ============================================================================
 *
 * Note: Full scroll bar support requires creating GtkScrollbar widgets
 * and integrating them with the NeomacsWidget. These stub implementations
 * allow Emacs to function without scroll bars while that work is completed.
 */

/* Set vertical scroll bar for window W */
static void
neomacs_set_vertical_scroll_bar (struct window *w, int portion, int whole,
                                 int position)
{
  /* TODO: Create/update GTK scroll bar widget
     For now, this is a stub that allows Emacs to function without scroll bars */
}

/* Set horizontal scroll bar for window W */
static void
neomacs_set_horizontal_scroll_bar (struct window *w, int portion, int whole,
                                   int position)
{
  /* TODO: Create/update GTK scroll bar widget
     For now, this is a stub */
}

/* Mark all scroll bars on FRAME for deletion */
static void
neomacs_condemn_scroll_bars (struct frame *frame)
{
  /* Move all scroll bars to condemned list.
     Since we don't create scroll bars yet, just clear the lists. */
  if (!NILP (FRAME_SCROLL_BARS (frame)))
    {
      /* Prepend to condemned list */
      if (!NILP (FRAME_CONDEMNED_SCROLL_BARS (frame)))
        {
          Lisp_Object last = FRAME_SCROLL_BARS (frame);
          while (!NILP (XSCROLL_BAR (last)->next))
            last = XSCROLL_BAR (last)->next;
          XSCROLL_BAR (last)->next = FRAME_CONDEMNED_SCROLL_BARS (frame);
          XSCROLL_BAR (FRAME_CONDEMNED_SCROLL_BARS (frame))->prev = last;
        }
      fset_condemned_scroll_bars (frame, FRAME_SCROLL_BARS (frame));
      fset_scroll_bars (frame, Qnil);
    }
}

/* Un-mark WINDOW's scroll bar for deletion */
static void
neomacs_redeem_scroll_bar (struct window *w)
{
  struct scroll_bar *bar;
  Lisp_Object barobj;
  struct frame *f;

  /* If window has no scroll bar, nothing to redeem */
  if (NILP (w->vertical_scroll_bar) && NILP (w->horizontal_scroll_bar))
    return;

  f = XFRAME (WINDOW_FRAME (w));

  /* Redeem vertical scroll bar */
  if (!NILP (w->vertical_scroll_bar) && WINDOW_HAS_VERTICAL_SCROLL_BAR (w))
    {
      bar = XSCROLL_BAR (w->vertical_scroll_bar);

      /* Unlink from condemned list */
      if (NILP (bar->prev))
        {
          if (EQ (FRAME_SCROLL_BARS (f), w->vertical_scroll_bar))
            goto horizontal;  /* Not condemned */
          else if (EQ (FRAME_CONDEMNED_SCROLL_BARS (f), w->vertical_scroll_bar))
            fset_condemned_scroll_bars (f, bar->next);
        }
      else
        XSCROLL_BAR (bar->prev)->next = bar->next;

      if (!NILP (bar->next))
        XSCROLL_BAR (bar->next)->prev = bar->prev;

      /* Add back to active list */
      bar->next = FRAME_SCROLL_BARS (f);
      bar->prev = Qnil;
      XSETVECTOR (barobj, bar);
      fset_scroll_bars (f, barobj);
      if (!NILP (bar->next))
        XSCROLL_BAR (bar->next)->prev = barobj;
    }

 horizontal:
  /* Redeem horizontal scroll bar */
  if (!NILP (w->horizontal_scroll_bar) && WINDOW_HAS_HORIZONTAL_SCROLL_BAR (w))
    {
      bar = XSCROLL_BAR (w->horizontal_scroll_bar);

      if (NILP (bar->prev))
        {
          if (EQ (FRAME_SCROLL_BARS (f), w->horizontal_scroll_bar))
            return;  /* Not condemned */
          else if (EQ (FRAME_CONDEMNED_SCROLL_BARS (f), w->horizontal_scroll_bar))
            fset_condemned_scroll_bars (f, bar->next);
        }
      else
        XSCROLL_BAR (bar->prev)->next = bar->next;

      if (!NILP (bar->next))
        XSCROLL_BAR (bar->next)->prev = bar->prev;

      bar->next = FRAME_SCROLL_BARS (f);
      bar->prev = Qnil;
      XSETVECTOR (barobj, bar);
      fset_scroll_bars (f, barobj);
      if (!NILP (bar->next))
        XSCROLL_BAR (bar->next)->prev = barobj;
    }
}

/* Destroy all condemned scroll bars on FRAME */
static void
neomacs_judge_scroll_bars (struct frame *f)
{
  Lisp_Object bar, next;

  bar = FRAME_CONDEMNED_SCROLL_BARS (f);
  fset_condemned_scroll_bars (f, Qnil);

  for (; !NILP (bar); bar = next)
    {
      struct scroll_bar *b = XSCROLL_BAR (bar);

      /* TODO: Actually destroy the GTK scroll bar widget here
         For now, just unlink the scroll bar */

      next = b->next;
      b->next = b->prev = Qnil;
    }
}

/* Make frame visible or invisible */
static void
neomacs_make_frame_visible_invisible (struct frame *f, bool visible)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!output)
    return;

  /* Handle winit windows (no GTK widget) */
  if (output->window_id > 0 && dpyinfo && dpyinfo->display_handle)
    {
      neomacs_display_show_window (dpyinfo->display_handle, output->window_id, visible);
      if (visible)
        {
          SET_FRAME_VISIBLE (f, 1);
          SET_FRAME_ICONIFIED (f, false);
        }
      else
        {
          SET_FRAME_VISIBLE (f, 0);
        }
      return;
    }

  /* GTK widget path */
  if (!output->widget)
    return;

  if (visible)
    {
      /* Show the window and mark frame visible */
      gtk_widget_set_visible (GTK_WIDGET (output->widget), TRUE);
      SET_FRAME_VISIBLE (f, 1);
      SET_FRAME_ICONIFIED (f, false);
    }
  else
    {
      /* Hide the window */
      gtk_widget_set_visible (GTK_WIDGET (output->widget), FALSE);
      SET_FRAME_VISIBLE (f, 0);
    }
}

/* Get a string resource value (for X resources / defaults) */
static const char *
neomacs_get_string_resource (void *rdb, const char *name, const char *class)
{
  /* Neomacs doesn't support X resources, return NULL */
  return NULL;
}


/* ============================================================================
 * Color Support
 * ============================================================================ */

/* Check if a color name is valid and return RGB values */
bool
neomacs_defined_color (struct frame *f, const char *color_name,
                       Emacs_Color *color_def, bool alloc, bool makeIndex)
{
  /* Use GDK to parse color names - supports all CSS color names and formats */
  if (!color_name || !color_def)
    return false;

  GdkRGBA rgba;
  if (gdk_rgba_parse (&rgba, color_name))
    {
      color_def->red = rgba.red * 65535;
      color_def->green = rgba.green * 65535;
      color_def->blue = rgba.blue * 65535;
      color_def->pixel = ((color_def->red >> 8) << 16
                          | (color_def->green >> 8) << 8
                          | (color_def->blue >> 8) << 0);
      return true;
    }

  return false;
}

/* Convert pixel values back to 16-bit RGB components.
   Pixel format: (R << 16) | (G << 8) | B, with 8-bit components.
   Called by image.c for PBM/XPM/etc. color handling under USE_CAIRO. */
static void
neomacs_query_colors (struct frame *f, Emacs_Color *colors, int ncolors)
{
  for (int i = 0; i < ncolors; i++)
    {
      unsigned long pixel = colors[i].pixel;
      colors[i].red   = ((pixel >> 16) & 0xff) * 257;
      colors[i].green = ((pixel >>  8) & 0xff) * 257;
      colors[i].blue  = ((pixel >>  0) & 0xff) * 257;
    }
}

/* Get the frame background color as an Emacs_Color. */
static void
neomacs_query_frame_background_color (struct frame *f, Emacs_Color *bgcolor)
{
  bgcolor->pixel = FRAME_BACKGROUND_PIXEL (f);
  neomacs_query_colors (f, bgcolor, 1);
}

/* Free a Cairo-style pixmap (Emacs_Pix_Container). */
static void
neomacs_free_pixmap (struct frame *f, Emacs_Pixmap pixmap)
{
  if (pixmap)
    {
      xfree (pixmap->data);
      xfree (pixmap);
    }
}


/* ============================================================================
 * Cairo Helpers for Font Drivers
 * ============================================================================ */

/* Begin Cairo drawing with clipping */
cairo_t *
neomacs_begin_cr_clip (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (!output || !output->cr_context)
    return NULL;

  cairo_save (output->cr_context);
  return output->cr_context;
}

/* End Cairo drawing */
void
neomacs_end_cr_clip (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (output && output->cr_context)
    cairo_restore (output->cr_context);
}

/* Set Cairo source color */
void
neomacs_set_cr_source_with_color (struct frame *f, unsigned long color,
				  bool stipple_p)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (!output || !output->cr_context)
    return;

  double r = RED_FROM_ULONG (color) / 255.0;
  double g = GREEN_FROM_ULONG (color) / 255.0;
  double b = BLUE_FROM_ULONG (color) / 255.0;

  cairo_set_source_rgb (output->cr_context, r, g, b);
}


/* ============================================================================
 * Text Drawing
 * ============================================================================ */

/* Draw a glyph string */
void
neomacs_draw_glyph_string (struct glyph_string *s)
{
  struct frame *f = s->f;
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  cairo_t *cr;

  if (!output)
    return;

  /* For GPU widget mode, we don't need Cairo context - just forward to Rust */
  if (output->use_gpu_widget)
    {
      /* Forward glyph data to Rust scene graph */
      if (dpyinfo && dpyinfo->display_handle && s->first_glyph && s->row)
        {
          /* s->y is already frame-relative (set via WINDOW_TO_FRAME_PIXEL_Y in xdisp.c),
             so we use it directly without adding window_top again */
          int glyph_y = s->y;

          neomacs_display_begin_row (dpyinfo->display_handle,
                                     glyph_y,
                                     s->x,  /* Starting X position for this glyph string */
                                     s->height,  /* Use glyph string height */
                                     s->row->ascent,
                                     s->row->mode_line_p ? 1 : 0,
                                     0);  /* header_line not in glyph_row */

          /* Add glyphs to Rust scene graph */
          int face_id = s->face ? s->face->id : 0;

          /* Register face with colors/attributes if we have face info */
          if (s->face)
            {
              struct face *face = s->face;
              unsigned long fg = face->foreground;
              unsigned long bg = face->background;

              /* If foreground is defaulted, use frame foreground */
              if (face->foreground_defaulted_p)
                fg = FRAME_FOREGROUND_PIXEL(s->f);
              if (face->background_defaulted_p)
                bg = FRAME_BACKGROUND_PIXEL(s->f);

              /* Convert Emacs colors to 0xRRGGBB format */
              uint32_t fg_rgb = ((RED_FROM_ULONG(fg) << 16) |
                                (GREEN_FROM_ULONG(fg) << 8) |
                                BLUE_FROM_ULONG(fg));
              uint32_t bg_rgb = ((RED_FROM_ULONG(bg) << 16) |
                                (GREEN_FROM_ULONG(bg) << 8) |
                                BLUE_FROM_ULONG(bg));

              /* Get font family from lface */
              const char *font_family = NULL;  /* NULL means default */
              if (face->lface != NULL) {
                Lisp_Object family_attr = face->lface[LFACE_FAMILY_INDEX];
                if (!NILP (family_attr) && STRINGP (family_attr))
                  font_family = SSDATA (family_attr);
              }

              /* Get font weight and slant from lface attributes */
              int font_weight = 400;  /* normal */
              int is_italic = 0;

              /* Check face's lface for weight (bold) */
              Lisp_Object weight_attr = face->lface[LFACE_WEIGHT_INDEX];
              if (!NILP (weight_attr) && SYMBOLP (weight_attr))
                {
                  int weight_numeric = FONT_WEIGHT_NAME_NUMERIC (weight_attr);
                  if (weight_numeric > 0)
                    font_weight = emacs_weight_to_css (weight_numeric);
                }

              /* Check face's lface for slant (italic) */
              Lisp_Object slant_attr = face->lface[LFACE_SLANT_INDEX];
              if (!NILP (slant_attr) && SYMBOLP (slant_attr))
                {
                  int slant_numeric = FONT_SLANT_NAME_NUMERIC (slant_attr);
                  /* Normal slant is 100, italic is 200, oblique is around 110 */
                  if (slant_numeric != 100)
                    is_italic = 1;
                }

              /* Check for underline */
              int underline_style = 0;
              uint32_t underline_color = fg_rgb;
              if (face->underline != FACE_NO_UNDERLINE)
                {
                  switch (face->underline)
                    {
                    case FACE_UNDERLINE_SINGLE: underline_style = 1; break;
                    case FACE_UNDERLINE_WAVE: underline_style = 2; break;
                    case FACE_UNDERLINE_DOUBLE_LINE: underline_style = 3; break;
                    case FACE_UNDERLINE_DOTS: underline_style = 4; break;
                    case FACE_UNDERLINE_DASHES: underline_style = 5; break;
                    default: underline_style = 1; break;
                    }
                  if (!face->underline_defaulted_p)
                    underline_color = ((RED_FROM_ULONG(face->underline_color) << 16) |
                                       (GREEN_FROM_ULONG(face->underline_color) << 8) |
                                       BLUE_FROM_ULONG(face->underline_color));
                }

              /* Check for box */
              int box_type = 0;
              uint32_t box_color = fg_rgb;
              int box_line_width = 0;
              if (face->box != FACE_NO_BOX)
                {
                  box_type = 1;  /* line */
                  box_line_width = eabs(face->box_vertical_line_width);
                  if (box_line_width == 0)
                    box_line_width = 1;
                  box_color = ((RED_FROM_ULONG(face->box_color) << 16) |
                               (GREEN_FROM_ULONG(face->box_color) << 8) |
                               BLUE_FROM_ULONG(face->box_color));
                }

              int box_corner_radius = 0;
              if (face->box != FACE_NO_BOX)
                box_corner_radius = face->box_corner_radius;

              /* Check for strike-through */
              int strike_through = face->strike_through_p ? 1 : 0;
              uint32_t strike_through_color = fg_rgb;
              if (strike_through && !face->strike_through_color_defaulted_p)
                strike_through_color = ((RED_FROM_ULONG(face->strike_through_color) << 16) |
                                        (GREEN_FROM_ULONG(face->strike_through_color) << 8) |
                                        BLUE_FROM_ULONG(face->strike_through_color));

              /* Check for overline */
              int overline = face->overline_p ? 1 : 0;
              uint32_t overline_color = fg_rgb;
              if (overline && !face->overline_color_defaulted_p)
                overline_color = ((RED_FROM_ULONG(face->overline_color) << 16) |
                                  (GREEN_FROM_ULONG(face->overline_color) << 8) |
                                  BLUE_FROM_ULONG(face->overline_color));

              /* Get font size from the face's font (for text-scale-increase support) */
              int font_size = 14;  /* default */
              if (face->font)
                font_size = face->font->pixel_size;

              neomacs_display_set_face (dpyinfo->display_handle,
                                        face_id,
                                        fg_rgb,
                                        bg_rgb,
                                        font_family,
                                        font_weight,
                                        is_italic,
                                        font_size,
                                        underline_style,
                                        underline_color,
                                        box_type,
                                        box_color,
                                        box_line_width,
                                        box_corner_radius,
                                        strike_through,
                                        strike_through_color,
                                        overline,
                                        overline_color);
            }

          switch (s->first_glyph->type)
            {
            case CHAR_GLYPH:
              /* Forward character glyphs - get character from glyph->u.ch */
              {
                struct glyph *glyph = s->first_glyph;
                for (int i = 0; i < s->nchars && glyph; i++, glyph++)
                  {
                    /* Get the actual Unicode character from the glyph */
                    unsigned int charcode = glyph->u.ch;
                    int char_width = glyph->pixel_width;
                    neomacs_display_add_char_glyph (dpyinfo->display_handle,
                                                    charcode,
                                                    (uint32_t) face_id,
                                                    char_width,
                                                    FONT_BASE (s->font),
                                                    FONT_DESCENT (s->font));
                  }
              }
              break;
            case COMPOSITE_GLYPH:
            case GLYPHLESS_GLYPH:
              /* For composite/glyphless, use char2b if available */
              if (s->char2b)
                {
                  for (int i = 0; i < s->nchars; i++)
                    {
                      unsigned int charcode = s->char2b[i];
                      int char_width = s->width / (s->nchars > 0 ? s->nchars : 1);
                      neomacs_display_add_char_glyph (dpyinfo->display_handle,
                                                      charcode,
                                                      (uint32_t) face_id,
                                                      char_width,
                                                      s->font ? FONT_BASE (s->font) : s->height,
                                                      s->font ? FONT_DESCENT (s->font) : 0);
                    }
                }
              break;
            case STRETCH_GLYPH:
              neomacs_display_add_stretch_glyph (dpyinfo->display_handle,
                                                  s->first_glyph->pixel_width,
                                                  s->row->height,
                                                  (uint32_t) face_id);
              break;
            case IMAGE_GLYPH:
              /* Handle image glyphs via GPU rendering */
              if (s->img)
                {
                  uint32_t gpu_id = neomacs_get_or_load_image (dpyinfo, s->img);
                  if (gpu_id != 0)
                    {
                      /* Calculate image position and dimensions */
                      int img_x = s->x;
                      int img_y = s->ybase - image_ascent (s->img, s->face, &s->slice);

                      /* Adjust for box line if present */
                      if (s->face->box != FACE_NO_BOX
                          && s->first_glyph->left_box_line_p
                          && s->slice.x == 0)
                        img_x += max (s->face->box_vertical_line_width, 0);

                      /* Adjust for margins */
                      if (s->slice.x == 0)
                        img_x += s->img->hmargin;
                      if (s->slice.y == 0)
                        img_y += s->img->vmargin;

                      neomacs_display_add_image_glyph (dpyinfo->display_handle,
                                                       gpu_id,
                                                       s->slice.width,
                                                       s->slice.height);
                    }
                }
              break;
            case VIDEO_GLYPH:
              {
                /* Video glyphs use glyph dimensions (ascent + descent) */
                int glyph_height = s->first_glyph->ascent + s->first_glyph->descent;
                neomacs_display_add_video_glyph (dpyinfo->display_handle,
                                                  s->first_glyph->u.video_id,
                                                  s->first_glyph->pixel_width,
                                                  glyph_height > 0 ? glyph_height : s->height);
              }
              break;
            case WEBKIT_GLYPH:
              /* Handle WebKit glyphs - use glyph dimensions */
              {
                int glyph_height = s->first_glyph->ascent + s->first_glyph->descent;
                neomacs_display_add_wpe_glyph (dpyinfo->display_handle,
                                                s->first_glyph->u.webkit_id,
                                                s->first_glyph->pixel_width,
                                                glyph_height > 0 ? glyph_height : s->row->height);
              }
              break;
            default:
              break;
            }
        }
      return;  /* Don't do Cairo drawing for GPU widget */
    }

  /* For Cairo mode, we need the context */
  if (!output->cr_context)
    return;

  cr = output->cr_context;

  /* Forward glyph data to Rust scene graph if available */
  if (dpyinfo && dpyinfo->display_handle && s->first_glyph && s->row)
    {
      /* Calculate absolute Y position: window top edge + row Y within window */
      struct window *w = XWINDOW (s->f->selected_window);
      if (s->w)
        w = s->w;  /* Use the actual window from glyph_string if available */
      int window_top = WINDOW_TOP_EDGE_Y (w);
      int row_y = window_top + s->row->y;

      neomacs_display_begin_row (dpyinfo->display_handle,
                                 row_y,
                                 s->x,  /* Starting X position for this glyph string */
                                 s->row->height,
                                 s->row->ascent,
                                 s->row->mode_line_p ? 1 : 0,
                                 0);  /* header_line not in glyph_row */

      /* Add glyphs to Rust scene graph */
      int face_id = s->face ? s->face->id : 0;

      switch (s->first_glyph->type)
        {
        case CHAR_GLYPH:
          /* Forward character glyphs - get character from glyph->u.ch */
          {
            struct glyph *glyph = s->first_glyph;
            for (int i = 0; i < s->nchars && glyph; i++, glyph++)
              {
                /* Get the actual Unicode character from the glyph */
                unsigned int charcode = glyph->u.ch;
                int char_width = glyph->pixel_width;
                neomacs_display_add_char_glyph (dpyinfo->display_handle,
                                                charcode,
                                                face_id,
                                                char_width,
                                                s->font ? FONT_BASE (s->font) : s->height,
                                                s->font ? FONT_DESCENT (s->font) : 0);
              }
          }
          break;

        case COMPOSITE_GLYPH:
        case GLYPHLESS_GLYPH:
          /* For composite/glyphless, use char2b if available */
          if (s->char2b)
            {
              for (int i = 0; i < s->nchars; i++)
                {
                  unsigned int charcode = s->char2b[i];
                  int char_width = s->width / (s->nchars > 0 ? s->nchars : 1);
                  neomacs_display_add_char_glyph (dpyinfo->display_handle,
                                                  charcode,
                                                  face_id,
                                                  char_width,
                                                  s->font ? FONT_BASE (s->font) : s->height,
                                                  s->font ? FONT_DESCENT (s->font) : 0);
                }
            }
          break;

        case STRETCH_GLYPH:
          neomacs_display_add_stretch_glyph (dpyinfo->display_handle,
                                             s->width,
                                             s->height,
                                             face_id);
          break;

        case IMAGE_GLYPH:
          /* TODO: Forward image glyph */
          break;

        case VIDEO_GLYPH:
          /* Handle video glyphs */
          neomacs_display_add_video_glyph (dpyinfo->display_handle,
                                           s->first_glyph->u.video_id,
                                           s->first_glyph->pixel_width,
                                           s->row->height);
          break;

        case WEBKIT_GLYPH:
          /* Handle WebKit glyphs */
          neomacs_display_add_wpe_glyph (dpyinfo->display_handle,
                                          s->first_glyph->u.webkit_id,
                                          s->first_glyph->pixel_width,
                                          s->row->height);
          break;

        default:
          break;
        }
    }

  /* Continue with Cairo rendering (keeping existing behavior) */

  /* Get face colors */
  unsigned long fg = s->face->foreground;
  unsigned long bg = s->face->background;

  /* Draw background if needed */
  if (!s->background_filled_p)
    {
      double r = RED_FROM_ULONG (bg) / 255.0;
      double g = GREEN_FROM_ULONG (bg) / 255.0;
      double b = BLUE_FROM_ULONG (bg) / 255.0;

      cairo_set_source_rgb (cr, r, g, b);
      cairo_rectangle (cr, s->x, s->y, s->background_width, s->height);
      cairo_fill (cr);
      s->background_filled_p = true;
    }

  /* Draw the foreground (text) */
  switch (s->first_glyph->type)
    {
    case CHAR_GLYPH:
    case COMPOSITE_GLYPH:
    case GLYPHLESS_GLYPH:
      {
	/* Set foreground color */
	double r = RED_FROM_ULONG (fg) / 255.0;
	double g = GREEN_FROM_ULONG (fg) / 255.0;
	double b = BLUE_FROM_ULONG (fg) / 255.0;
	cairo_set_source_rgb (cr, r, g, b);

	/* Draw using the font */
	if (s->font && s->nchars > 0 && s->char2b)
	  {
	    /* Use font's draw method if available */
	    struct font *font = s->font;

	    if (font->driver && font->driver->draw)
	      {
		font->driver->draw (s, 0, s->nchars, s->x, s->ybase, false);
	      }
	    else
	      {
		/* Fallback: Draw using Cairo text rendering */
		/* Convert char2b to string */
		char buf[256];
		int len = 0;
		for (int i = 0; i < s->nchars && len < 255; i++)
		  {
		    unsigned int c = s->char2b[i];
		    if (c < 128)
		      buf[len++] = (char) c;
		    else if (c < 0x800 && len < 254)
		      {
			buf[len++] = 0xC0 | (c >> 6);
			buf[len++] = 0x80 | (c & 0x3F);
		      }
		    /* Skip other multibyte for now */
		  }
		buf[len] = '\0';

		if (len > 0)
		  {
		    cairo_select_font_face (cr, "monospace",
					   CAIRO_FONT_SLANT_NORMAL,
					   CAIRO_FONT_WEIGHT_NORMAL);
		    cairo_set_font_size (cr, FRAME_LINE_HEIGHT (f) * 0.8);
		    cairo_move_to (cr, s->x, s->ybase);
		    cairo_show_text (cr, buf);
		  }
	      }
	  }
      }
      break;

    case STRETCH_GLYPH:
      /* Stretch glyphs are just background - already drawn above */
      break;

    case IMAGE_GLYPH:
      /* TODO: Implement image glyph drawing */
      break;

    case VIDEO_GLYPH:
      /* Video glyph rendering is handled by Rust - no Cairo fallback needed */
      break;

    case WEBKIT_GLYPH:
      /* WebKit glyph rendering is handled by Rust - no Cairo fallback needed */
      break;

    default:
      break;
    }
}

/* Called after updating a window line */
static void
neomacs_after_update_window_line (struct window *w, struct glyph_row *desired_row)
{
  struct frame *f;
  int width, height;

  eassert (w);

  if (!desired_row->mode_line_p && !w->pseudo_window_p)
    desired_row->redraw_fringe_bitmaps_p = 1;

  /* When a window has disappeared, make sure that no rest of
     full-width rows stays visible in the internal border.  */
  if (windows_or_buffers_changed
      && desired_row->full_width_p
      && (f = XFRAME (w->frame),
	  width = FRAME_INTERNAL_BORDER_WIDTH (f),
	  width != 0) && (height = desired_row->visible_height, height > 0))
    {
      int y = WINDOW_TO_FRAME_PIXEL_Y (w, max (0, desired_row->y));

      block_input ();
      neomacs_clear_frame_area (f, 0, y, width, height);
      neomacs_clear_frame_area (f,
			       FRAME_PIXEL_WIDTH (f) - width, y, width, height);
      unblock_input ();
    }
}

/* Clear entire frame (used on full redisplay) */
static void
neomacs_clear_frame (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct face *face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);

  if (!face)
    return;

  mark_window_cursors_off (XWINDOW (FRAME_ROOT_WINDOW (f)));

  block_input ();

  /* Clear borders since window configuration is likely changing */
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_clear_all_borders (dpyinfo->display_handle);

  neomacs_clear_frame_area (f, 0, 0, FRAME_PIXEL_WIDTH (f), FRAME_PIXEL_HEIGHT (f));
  unblock_input ();
}

/* Clear a rectangle on the frame */
void
neomacs_clear_frame_area (struct frame *f, int x, int y, int width, int height)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  /* For GPU widget mode, clear the region in the glyph buffer */
  if (output && output->use_gpu_widget && dpyinfo && dpyinfo->display_handle)
    {
      neomacs_display_clear_area (dpyinfo->display_handle, x, y, width, height);
      return;
    }

  /* Cairo path for non-GPU widget mode */
  cairo_t *cr;

  if (!output || !output->cr_context)
    return;

  cr = output->cr_context;

  /* Get background color */
  unsigned long bg = output->background_pixel;
  double r = RED_FROM_ULONG (bg) / 255.0;
  double g = GREEN_FROM_ULONG (bg) / 255.0;
  double b = BLUE_FROM_ULONG (bg) / 255.0;

  cairo_set_source_rgb (cr, r, g, b);
  cairo_rectangle (cr, x, y, width, height);
  cairo_fill (cr);
}

/* Get GPU image ID for an Emacs image, loading it if necessary */
static uint32_t
neomacs_get_or_load_image (struct neomacs_display_info *dpyinfo, struct image *img)
{
  int i;

  if (!dpyinfo || !dpyinfo->display_handle || !img)
    return 0;

  /* Check cache first */
  for (i = 0; i < neomacs_image_cache_count; i++)
    {
      if (neomacs_image_cache[i].emacs_img == img)
        {
          uint32_t cached_id = neomacs_image_cache[i].gpu_id;
          /* If dimensions aren't set yet, try to get them now (async load may have completed) */
          if (img->width == 0 || img->height == 0)
            {
              int actual_w, actual_h;
              if (neomacs_display_get_image_size (dpyinfo->display_handle, cached_id,
                                                   &actual_w, &actual_h) == 0)
                {
                  img->width = actual_w;
                  img->height = actual_h;
                }
            }
          return cached_id;
        }
    }

  /* Not in cache - load the image */
  uint32_t gpu_id = 0;

  /* Try to load from pixmap data if available (Emacs decoded it) */
  if (img->pixmap && img->pixmap->data)
    {
      /* Emacs Cairo uses ARGB32 or RGB24 format */
      int width = img->pixmap->width;
      int height = img->pixmap->height;
      int stride = img->pixmap->bytes_per_line;
      unsigned char *data = (unsigned char *) img->pixmap->data;

      /* Check if image has alpha (mask or ARGB32 bits_per_pixel) */
      if (img->mask || img->pixmap->bits_per_pixel == 32)
        {
          gpu_id = neomacs_display_load_image_argb32 (dpyinfo->display_handle,
                                                       data, width, height, stride);
        }
      else
        {
          gpu_id = neomacs_display_load_image_rgb24 (dpyinfo->display_handle,
                                                      data, width, height, stride);
        }
    }
  else if (CONSP (img->spec))
    {
      /* Pixmap not available - Emacs couldn't decode (missing libpng/libjpeg etc.)
         or this is a neomacs image type. Try to load via gdk-pixbuf.
         Image spec format: (image :type TYPE :file PATH :width W :height H ...) */

      /* First check for :neomacs-id (pre-loaded by neomacs-insert-image) */
      Lisp_Object neomacs_id_sym = intern (":neomacs-id");
      Lisp_Object neomacs_id = plist_get (XCDR (img->spec), neomacs_id_sym);
      if (FIXNUMP (neomacs_id))
        {
          /* Image was already loaded by neomacs-insert-image */
          gpu_id = (uint32_t) XFIXNUM (neomacs_id);
        }
      else
        {
          /* Try to load from :file or :data */
          Lisp_Object file = plist_get (XCDR (img->spec), QCfile);
          Lisp_Object data = plist_get (XCDR (img->spec), QCdata);

          /* Check for dimension constraints */
          Lisp_Object max_width = plist_get (XCDR (img->spec), QCmax_width);
          Lisp_Object max_height = plist_get (XCDR (img->spec), QCmax_height);
          Lisp_Object width = plist_get (XCDR (img->spec), QCwidth);
          Lisp_Object height = plist_get (XCDR (img->spec), QCheight);
          Lisp_Object scale = plist_get (XCDR (img->spec), QCscale);

          int mw = FIXNUMP (max_width) ? XFIXNUM (max_width) : 0;
          int mh = FIXNUMP (max_height) ? XFIXNUM (max_height) : 0;
          int tw = FIXNUMP (width) ? XFIXNUM (width) : 0;  /* target width */
          int th = FIXNUMP (height) ? XFIXNUM (height) : 0; /* target height */
          double sc = NUMBERP (scale) ? XFLOATINT (scale) : 1.0;

          if (STRINGP (file))
            {
              const char *path = SSDATA (file);

              if (mw > 0 || mh > 0)
                gpu_id = neomacs_display_load_image_file_scaled (dpyinfo->display_handle,
                                                                  path, mw, mh);
              else
                gpu_id = neomacs_display_load_image_file (dpyinfo->display_handle, path);
            }
          else if (STRINGP (data))
            {
              /* Inline image data */
              const unsigned char *bytes = (const unsigned char *) SDATA (data);
              ptrdiff_t len = SBYTES (data);

              if (mw > 0 || mh > 0)
                gpu_id = neomacs_display_load_image_data_scaled (dpyinfo->display_handle,
                                                                  bytes, len, mw, mh);
              else
                gpu_id = neomacs_display_load_image_data (dpyinfo->display_handle, bytes, len);
            }

          if (gpu_id != 0)
            {
              /* Get actual dimensions from GPU cache */
              int actual_w, actual_h;
              if (neomacs_display_get_image_size (dpyinfo->display_handle, gpu_id,
                                                   &actual_w, &actual_h) == 0)
                {
                  /* Apply :scale if specified */
                  if (sc != 1.0 && sc > 0)
                    {
                      actual_w = (int)(actual_w * sc);
                      actual_h = (int)(actual_h * sc);
                    }

                  /* Compute final dimensions respecting :width/:height with aspect ratio */
                  if (tw > 0 && th > 0)
                    {
                      img->width = tw;
                      img->height = th;
                    }
                  else if (tw > 0)
                    {
                      img->width = tw;
                      img->height = (int)((double)tw * actual_h / actual_w);
                    }
                  else if (th > 0)
                    {
                      img->height = th;
                      img->width = (int)((double)th * actual_w / actual_h);
                    }
                  else
                    {
                      img->width = actual_w;
                      img->height = actual_h;
                    }
                }
            }
        }  /* end else (load from file/data) */
    }

  if (gpu_id == 0)
    {
      /* Only warn if we actually tried to load something */
      if (img->width > 0 && img->height > 0)
        fprintf (stderr, "neomacs: Failed to load image %dx%d\n",
                 img->width, img->height);
      return 0;
    }

  /* Add to cache */
  if (neomacs_image_cache_count < IMAGE_CACHE_SIZE)
    {
      neomacs_image_cache[neomacs_image_cache_count].emacs_img = img;
      neomacs_image_cache[neomacs_image_cache_count].gpu_id = gpu_id;
      neomacs_image_cache_count++;
    }

  return gpu_id;
}

/* Draw vertical window border - used for horizontal splits (C-x 3) */
static void
neomacs_draw_vertical_window_border (struct window *w, int x, int y0, int y1)
{
  struct frame *f = XFRAME (WINDOW_FRAME (w));
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct face *face;
  unsigned long fg;

  if (!output)
    return;

  /* Get the vertical border face color, or fall back to black */
  face = FACE_FROM_ID_OR_NULL (f, VERTICAL_BORDER_FACE_ID);
  if (face)
    fg = face->foreground;
  else
    fg = 0;  /* Black */

  /* Use GPU path if available */
  if (dpyinfo && dpyinfo->display_handle)
    {
      /* Convert to ARGB format with full opacity */
      uint32_t color = (0xFF << 24) | (fg & 0xFFFFFF);
      neomacs_display_draw_border (dpyinfo->display_handle,
                                   x, y0, 1, y1 - y0, color);
    }
  else if (output->cr_context)
    {
      /* Fallback to Cairo */
      cairo_t *cr = output->cr_context;
      double r = RED_FROM_ULONG (fg) / 255.0;
      double g = GREEN_FROM_ULONG (fg) / 255.0;
      double b = BLUE_FROM_ULONG (fg) / 255.0;
      cairo_set_source_rgb (cr, r, g, b);
      cairo_rectangle (cr, x, y0, 1, y1 - y0);
      cairo_fill (cr);
    }
}

/* Draw fringe bitmap */
void
neomacs_draw_fringe_bitmap (struct window *w, struct glyph_row *row,
                            struct draw_fringe_bitmap_params *p)
{
  /* TODO: Implement fringe bitmap drawing */
}


/* ============================================================================
 * Cursor Drawing
 * ============================================================================ */

/* Draw the cursor */
void
neomacs_draw_window_cursor (struct window *w, struct glyph_row *row,
                            int x, int y, enum text_cursor_kinds cursor_type,
                            int cursor_width, bool on_p, bool active_p)
{
  struct frame *f = XFRAME (w->frame);
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!output || !on_p)
    return;

  /* Get the glyph at the cursor position for proper dimensions */
  struct glyph *cursor_glyph = get_phys_cursor_glyph (w);
  if (!cursor_glyph)
    return;

  int frame_x, frame_y, cursor_h;
  get_phys_cursor_geometry (w, row, cursor_glyph, &frame_x, &frame_y, &cursor_h);
  int cursor_w = w->phys_cursor_width;  /* Set by get_phys_cursor_geometry */

  /* For bar cursor, use the explicit cursor_width if provided */
  if (cursor_type == BAR_CURSOR && cursor_width > 0)
    cursor_w = cursor_width;

  /* Get cursor color - default to a visible color */
  unsigned long cursor_color = output->cursor_pixel;
  if (cursor_color == 0)
    cursor_color = 0x00FF00;  /* Green for visibility */

  /* Use GPU path if available */
  if (dpyinfo && dpyinfo->display_handle)
    {
      int style;
      switch (cursor_type)
        {
        case NO_CURSOR:
          /* Don't draw any cursor */
          return;
        case DEFAULT_CURSOR:
        case FILLED_BOX_CURSOR:
          style = 0;  /* Box */
          break;
        case BAR_CURSOR:
          style = 1;  /* Bar */
          break;
        case HBAR_CURSOR:
          style = 2;  /* Underline */
          break;
        case HOLLOW_BOX_CURSOR:
          style = 3;  /* Hollow */
          break;
        default:
          style = 0;
          break;
        }

      /* Convert color to RGBA format (0xAARRGGBB) */
      uint32_t rgba = 0xFF000000 | (cursor_color & 0xFFFFFF);

      /* Use frame-absolute coordinates for cursor position */
      neomacs_display_set_cursor (dpyinfo->display_handle,
                                  (int)(intptr_t) w,
                                  (float) frame_x, (float) frame_y,
                                  (float) cursor_w, (float) cursor_h,
                                  style, rgba, 1);

      /* For filled box cursor, compute inverse video colors so the
         character under the cursor remains visible.  */
      if (style == 0)
        {
          unsigned long cursor_fg;
          struct face *face = FACE_FROM_ID_OR_NULL (f, cursor_glyph->face_id);
          if (face)
            {
              cursor_fg = face->background;
              /* If face bg == cursor color, text would be invisible;
                 fall back to face foreground.  */
              if (cursor_fg == cursor_color)
                cursor_fg = face->foreground;
            }
          else
            cursor_fg = FRAME_BACKGROUND_PIXEL (f);

          uint32_t cursor_fg_rgba = 0xFF000000 | (cursor_fg & 0xFFFFFF);
          neomacs_display_set_cursor_inverse (dpyinfo->display_handle,
                                              (float) frame_x, (float) frame_y,
                                              (float) cursor_w, (float) cursor_h,
                                              rgba, cursor_fg_rgba);
        }

      neomacs_display_reset_cursor_blink (dpyinfo->display_handle);
      return;
    }

  /* Fallback to Cairo */
  cairo_t *cr = output->cr_context;
  if (!cr)
    return;

  double r = RED_FROM_ULONG (cursor_color) / 255.0;
  double g = GREEN_FROM_ULONG (cursor_color) / 255.0;
  double b = BLUE_FROM_ULONG (cursor_color) / 255.0;

  cairo_set_source_rgb (cr, r, g, b);

  switch (cursor_type)
    {
    case DEFAULT_CURSOR:
    case FILLED_BOX_CURSOR:
      /* Filled box cursor */
      cairo_rectangle (cr, x, y, cursor_w, cursor_h);
      cairo_fill (cr);
      break;

    case BAR_CURSOR:
      /* Vertical bar cursor */
      cairo_rectangle (cr, x, y, 2, cursor_h);
      cairo_fill (cr);
      break;

    case HBAR_CURSOR:
      /* Horizontal bar cursor */
      cairo_rectangle (cr, x, y + cursor_h - 2, cursor_w, 2);
      cairo_fill (cr);
      break;

    case HOLLOW_BOX_CURSOR:
      /* Hollow box cursor */
      cairo_set_line_width (cr, 1.0);
      cairo_rectangle (cr, x + 0.5, y + 0.5, cursor_w - 1, cursor_h - 1);
      cairo_stroke (cr);
      break;

    case NO_CURSOR:
      break;
    }
}


/* ============================================================================
 * Scrolling
 * ============================================================================ */

/* Scroll the contents of a window.
   For GPU-accelerated rendering with a glyph buffer, we need to clear
   the window's text area when scrolling.  Unlike X11 or Haiku which can
   do hardware blitting (copy regions), our glyph buffer approach requires
   removing stale glyphs before Emacs redraws at new positions.

   Without this clear, remove_overlapping() only removes glyphs at exact Y
   positions (within 1px), leaving old glyphs at different Y positions which
   causes visual corruption (e.g., "End of image 4" appearing where
   "End of image 2" should be after scrolling). */
void
neomacs_scroll_run (struct window *w, struct run *run)
{
  struct frame *f = XFRAME (w->frame);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  int x, y, width, height, from_y, to_y, bottom_y;

  if (!dpyinfo || !dpyinfo->display_handle)
    return;

  /* With full-frame rendering from current_matrix, scroll blitting is no
     longer needed.  The entire frame is rebuilt each update.  However, we
     still send the command for compatibility (it's a no-op on the render
     thread). */
  if (output && output->use_gpu_widget)
    {
      /* Get frame-relative bounding box of the text display area of W,
         without mode lines.  Include in this box the left and right
         fringe of W. */
      window_box (w, ANY_AREA, &x, &y, &width, &height);

      /* Convert window-relative Y to frame-relative Y */
      from_y = WINDOW_TO_FRAME_PIXEL_Y (w, run->current_y);
      to_y = WINDOW_TO_FRAME_PIXEL_Y (w, run->desired_y);
      bottom_y = y + height;

      /* Calculate actual height to copy, avoiding mode line */
      if (to_y < from_y)
        {
          /* Scrolling up.  Make sure we don't copy part of the mode
             line at the bottom.  */
          if (from_y + run->height > bottom_y)
            height = bottom_y - from_y;
          else
            height = run->height;
        }
      else
        {
          /* Scrolling down.  Make sure we don't copy over the mode line
             at the bottom.  */
          if (to_y + run->height > bottom_y)
            height = bottom_y - to_y;
          else
            height = run->height;
        }

      /* Get background color from frame's default face */
      Lisp_Object bg = Qnil;
      struct face *face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
      float bg_r = 1.0f, bg_g = 1.0f, bg_b = 1.0f;
      if (face)
        {
          unsigned long pixel = face->background;
          /* Convert pixel value to RGB (assuming 24-bit color) */
          bg_r = ((pixel >> 16) & 0xFF) / 255.0f;
          bg_g = ((pixel >> 8) & 0xFF) / 255.0f;
          bg_b = (pixel & 0xFF) / 255.0f;
        }

      /* Perform the scroll blit in the GPU pixel buffer */
      neomacs_display_scroll_blit (dpyinfo->display_handle,
                                   x, y, width, height,
                                   from_y, to_y,
                                   bg_r, bg_g, bg_b);
    }
}


/* ============================================================================
 * Exposure Handling
 * ============================================================================ */

/* Handle expose event - redraw the frame */
void
neomacs_expose_frame (struct frame *f)
{
  if (!FRAME_NEOMACS_P (f))
    return;

  /* Mark frame as needing redisplay */
  SET_FRAME_GARBAGED (f);
}

/* Called when frame is fully up to date */
void
neomacs_frame_up_to_date (struct frame *f)
{
  /* Nothing special needed */
}


/* ============================================================================
 * Focus Management
 * ============================================================================ */

/* Change focus to frame */
void
neomacs_focus_frame (struct frame *f, bool raise_flag)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!dpyinfo)
    return;

  dpyinfo->focus_frame = f;
}


/* ============================================================================
 * Cairo Integration for Font Rendering (ftcrfont.c)
 * ============================================================================ */

#include <cairo.h>


/* ============================================================================
 * Font Handling
 * ============================================================================ */

/* Set a new font for frame F.  This is called when a frame's font
   parameter is changed.  */
Lisp_Object
neomacs_new_font (struct frame *f, Lisp_Object font_object, int fontset)
{
  struct font *font = XFONT_OBJECT (font_object);
  int font_ascent, font_descent;

  if (fontset < 0)
    fontset = fontset_from_font (font_object);
  FRAME_FONTSET (f) = fontset;

  if (FRAME_FONT (f) == font)
    {
      /* This font is already set in frame F.  There's nothing more to do.  */
      return font_object;
    }

  FRAME_FONT (f) = font;

  FRAME_BASELINE_OFFSET (f) = font->baseline_offset;
  FRAME_COLUMN_WIDTH (f) = font->average_width;
  get_font_ascent_descent (font, &font_ascent, &font_descent);
  FRAME_LINE_HEIGHT (f) = font_ascent + font_descent;

  /* We could use a more elaborate calculation here.  */
  FRAME_TAB_BAR_HEIGHT (f) = FRAME_TAB_BAR_LINES (f) * FRAME_LINE_HEIGHT (f);

  /* Compute the scroll bar width in character columns.  */
  if (FRAME_CONFIG_SCROLL_BAR_WIDTH (f) > 0)
    {
      int wid = FRAME_COLUMN_WIDTH (f);
      FRAME_CONFIG_SCROLL_BAR_COLS (f)
	= (FRAME_CONFIG_SCROLL_BAR_WIDTH (f) + wid - 1) / wid;
    }
  else
    {
      int wid = FRAME_COLUMN_WIDTH (f);
      FRAME_CONFIG_SCROLL_BAR_COLS (f) = (14 + wid - 1) / wid;
    }

  /* Compute the scroll bar height in character lines.  */
  if (FRAME_CONFIG_SCROLL_BAR_HEIGHT (f) > 0)
    {
      int height = FRAME_LINE_HEIGHT (f);
      FRAME_CONFIG_SCROLL_BAR_LINES (f)
	= (FRAME_CONFIG_SCROLL_BAR_HEIGHT (f) + height - 1) / height;
    }
  else
    {
      int height = FRAME_LINE_HEIGHT (f);
      FRAME_CONFIG_SCROLL_BAR_LINES (f) = (14 + height - 1) / height;
    }

  /* Invalidate face cache since fonts changed.  */
  if (FRAME_FACE_CACHE (f))
    {
      struct face_cache *c = FRAME_FACE_CACHE (f);
      if (c)
	c->used = 0;
    }

  /* Now make the frame display the given font.
     This triggers proper redisplay after dynamic font changes.  */
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  if (output != NULL)
    adjust_frame_size (f, FRAME_COLS (f) * FRAME_COLUMN_WIDTH (f),
		       FRAME_LINES (f) * FRAME_LINE_HEIGHT (f), 3,
		       false, Qfont);

  return font_object;
}


/* ============================================================================
 * Input Event Handling
 * ============================================================================ */

/* Read socket events for the Neomacs terminal.
   In threaded mode, events are delivered via the wakeup handler
   (neomacs_display_wakeup_handler) which calls neomacs_display_drain_input.
   This function just flushes any queued events to Emacs.  */
int
neomacs_read_socket (struct terminal *terminal, struct input_event *hold_quit)
{
  int count;

  /* Flush queued events to Emacs */
  count = neomacs_evq_flush (hold_quit);
  return count;
}


/* ============================================================================
 * Redisplay Interface
 * ============================================================================ */


/* ============================================================================
 * Lisp Interface
 * ============================================================================ */

DEFUN ("neomacs-available-p", Fneomacs_available_p, Sneomacs_available_p, 0, 0, 0,
       doc: /* Return t if Neomacs display backend is available.  */)
  (void)
{
  return Qt;
}

DEFUN ("neomacs-display-list", Fneomacs_display_list, Sneomacs_display_list, 0, 0, 0,
       doc: /* Return a list of all Neomacs display connections.  */)
  (void)
{
  Lisp_Object result = Qnil;
  struct neomacs_display_info *dpyinfo;

  for (dpyinfo = neomacs_display_list; dpyinfo; dpyinfo = dpyinfo->next)
    {
      if (dpyinfo->terminal)
        result = Fcons (make_fixnum (dpyinfo->terminal->id), result);
    }

  return result;
}

DEFUN ("x-hide-tip", Fx_hide_tip, Sx_hide_tip, 0, 0, 0,
       doc: /* Hide the current tooltip window, if there is any.
Value is t if tooltip was open, nil otherwise.  */)
  (void)
{
  /* TODO: Implement tooltip hiding */
  return Qnil;
}

DEFUN ("xw-display-color-p", Fxw_display_color_p, Sxw_display_color_p, 0, 1, 0,
       doc: /* Return t if the display supports color.  */)
  (Lisp_Object terminal)
{
  /* Neomacs always supports full color via GTK4 */
  return Qt;
}

DEFUN ("x-display-grayscale-p", Fx_display_grayscale_p, Sx_display_grayscale_p, 0, 1, 0,
       doc: /* Return t if the display can show shades of gray.  */)
  (Lisp_Object terminal)
{
  /* Neomacs displays support both color and grayscale */
  return Qnil;  /* Return nil meaning we support full color, not just grayscale */
}


/* ============================================================================
 * Video Playback API
 * ============================================================================ */

DEFUN ("neomacs-video-load", Fneomacs_video_load, Sneomacs_video_load, 1, 1, 0,
       doc: /* Load a video from URI.
Returns video ID on success, nil on failure.
URI can be a file path or a URL.  */)
  (Lisp_Object uri)
{
  CHECK_STRING (uri);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *uri_str = SSDATA (uri);
  uint32_t video_id = neomacs_display_load_video (dpyinfo->display_handle, uri_str);

  if (video_id == 0)
    return Qnil;

  return make_fixnum (video_id);
}

DEFUN ("neomacs-video-play", Fneomacs_video_play, Sneomacs_video_play, 1, 1, 0,
       doc: /* Start playing video with VIDEO-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object video_id)
{
  CHECK_FIXNUM (video_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_video_play (dpyinfo->display_handle,
                                           (uint32_t) XFIXNUM (video_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-video-pause", Fneomacs_video_pause, Sneomacs_video_pause, 1, 1, 0,
       doc: /* Pause video with VIDEO-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object video_id)
{
  CHECK_FIXNUM (video_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_video_pause (dpyinfo->display_handle,
                                            (uint32_t) XFIXNUM (video_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-video-stop", Fneomacs_video_stop, Sneomacs_video_stop, 1, 1, 0,
       doc: /* Stop video with VIDEO-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object video_id)
{
  CHECK_FIXNUM (video_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_video_stop (dpyinfo->display_handle,
                                           (uint32_t) XFIXNUM (video_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-video-floating", Fneomacs_video_floating, Sneomacs_video_floating, 5, 5, 0,
       doc: /* Display VIDEO-ID as a floating layer at X, Y with WIDTH and HEIGHT.
The video is rendered on top of the frame content at a fixed screen position.  */)
  (Lisp_Object video_id, Lisp_Object x, Lisp_Object y,
   Lisp_Object width, Lisp_Object height)
{
  CHECK_FIXNUM (video_id);
  CHECK_FIXNUM (x);
  CHECK_FIXNUM (y);
  CHECK_FIXNUM (width);
  CHECK_FIXNUM (height);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_set_floating_video (dpyinfo->display_handle,
                                      (uint32_t) XFIXNUM (video_id),
                                      (int) XFIXNUM (x),
                                      (int) XFIXNUM (y),
                                      (int) XFIXNUM (width),
                                      (int) XFIXNUM (height));
  return Qt;
}

DEFUN ("neomacs-video-floating-clear", Fneomacs_video_floating_clear, Sneomacs_video_floating_clear, 1, 1, 0,
       doc: /* Remove floating video layer for VIDEO-ID.  */)
  (Lisp_Object video_id)
{
  CHECK_FIXNUM (video_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_clear_floating_video (dpyinfo->display_handle,
                                        (uint32_t) XFIXNUM (video_id));
  return Qt;
}

DEFUN ("neomacs-video-set-loop", Fneomacs_video_set_loop, Sneomacs_video_set_loop, 2, 2, 0,
       doc: /* Set loop mode for VIDEO-ID.
LOOP-COUNT can be:
  nil or 0 - no looping
  t or -1  - infinite loop
  positive integer - loop that many times  */)
  (Lisp_Object video_id, Lisp_Object loop_count)
{
  CHECK_FIXNUM (video_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int count = 0;
  if (NILP (loop_count))
    count = 0;  /* No loop */
  else if (EQ (loop_count, Qt))
    count = -1; /* Infinite loop */
  else if (FIXNUMP (loop_count))
    count = XFIXNUM (loop_count);
  else
    count = -1; /* Default to infinite for truthy values */

  int result = neomacs_display_video_set_loop (dpyinfo->display_handle,
                                               (uint32_t) XFIXNUM (video_id),
                                               count);
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-video-update", Fneomacs_video_update, Sneomacs_video_update, 1, 1, 0,
       doc: /* Update video state for VIDEO-ID.
Checks for end-of-stream and handles looping.
Should be called periodically for proper loop handling.  */)
  (Lisp_Object video_id)
{
  CHECK_FIXNUM (video_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_video_update (dpyinfo->display_handle,
                                             (uint32_t) XFIXNUM (video_id));
  return result == 0 ? Qt : Qnil;
}


/* ============================================================================
 * Image API
 * ============================================================================ */

DEFUN ("neomacs-image-load", Fneomacs_image_load, Sneomacs_image_load, 1, 1, 0,
       doc: /* Load an image from PATH.
Returns image ID on success, nil on failure.  */)
  (Lisp_Object path)
{
  CHECK_STRING (path);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *path_str = SSDATA (path);
  uint32_t image_id = neomacs_display_load_image (dpyinfo->display_handle, path_str);

  if (image_id == 0)
    return Qnil;

  return make_fixnum (image_id);
}

DEFUN ("neomacs-image-size", Fneomacs_image_size, Sneomacs_image_size, 1, 1, 0,
       doc: /* Get size of image with IMAGE-ID.
Returns (width . height) on success, nil on failure.  */)
  (Lisp_Object image_id)
{
  CHECK_FIXNUM (image_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int width = 0, height = 0;
  int result = neomacs_display_get_image_size (dpyinfo->display_handle,
                                                (uint32_t) XFIXNUM (image_id),
                                                &width, &height);

  if (result != 0)
    return Qnil;

  return Fcons (make_fixnum (width), make_fixnum (height));
}

DEFUN ("neomacs-image-free", Fneomacs_image_free, Sneomacs_image_free, 1, 1, 0,
       doc: /* Free image with IMAGE-ID from cache.
Returns t on success, nil on failure.  */)
  (Lisp_Object image_id)
{
  CHECK_FIXNUM (image_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_free_image (dpyinfo->display_handle,
                                           (uint32_t) XFIXNUM (image_id));

  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-image-floating", Fneomacs_image_floating, Sneomacs_image_floating, 5, 5, 0,
       doc: /* Show image IMAGE-ID as a floating layer at position (X, Y) with size (WIDTH, HEIGHT).
The image will be rendered on top of the frame content at a fixed screen position.  */)
  (Lisp_Object image_id, Lisp_Object x, Lisp_Object y, Lisp_Object width, Lisp_Object height)
{
  CHECK_FIXNUM (image_id);
  CHECK_FIXNUM (x);
  CHECK_FIXNUM (y);
  CHECK_FIXNUM (width);
  CHECK_FIXNUM (height);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_set_floating_image (dpyinfo->display_handle,
                                      (uint32_t) XFIXNUM (image_id),
                                      (int) XFIXNUM (x),
                                      (int) XFIXNUM (y),
                                      (int) XFIXNUM (width),
                                      (int) XFIXNUM (height));
  return Qt;
}

DEFUN ("neomacs-image-floating-clear", Fneomacs_image_floating_clear, Sneomacs_image_floating_clear, 1, 1, 0,
       doc: /* Remove the floating layer for image IMAGE-ID.  */)
  (Lisp_Object image_id)
{
  CHECK_FIXNUM (image_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_clear_floating_image (dpyinfo->display_handle,
                                        (uint32_t) XFIXNUM (image_id));
  return Qt;
}

DEFUN ("neomacs-insert-image", Fneomacs_insert_image, Sneomacs_insert_image, 1, 3, 0,
       doc: /* Insert image from FILE at point as an inline image.
Optional MAX-WIDTH and MAX-HEIGHT limit the image dimensions (scales to fit).
The image is loaded via GPU and displayed inline with text.
Returns the neomacs image ID on success, nil on failure.

This function bypasses Emacs's native image library requirements,
using gdk-pixbuf to load PNG, JPEG, GIF, WebP, SVG, and other formats.  */)
  (Lisp_Object file, Lisp_Object max_width, Lisp_Object max_height)
{
  CHECK_STRING (file);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    {
      error ("No neomacs display available");
      return Qnil;
    }

  const char *path = SSDATA (file);
  int mw = FIXNUMP (max_width) ? XFIXNUM (max_width) : 0;
  int mh = FIXNUMP (max_height) ? XFIXNUM (max_height) : 0;

  /* Load image via GPU backend using direct GdkTexture path */
  uint32_t image_id;
  if (mw > 0 || mh > 0)
    image_id = neomacs_display_load_image_file_direct_scaled (dpyinfo->display_handle, path, mw, mh);
  else
    image_id = neomacs_display_load_image_file_direct (dpyinfo->display_handle, path);

  if (image_id == 0)
    {
      error ("Failed to load image: %s", path);
      return Qnil;
    }

  /* Get actual image dimensions */
  int width, height;
  if (neomacs_display_get_image_size (dpyinfo->display_handle, image_id, &width, &height) != 0)
    {
      error ("Failed to get image size");
      return Qnil;
    }

  /* Create image spec for display property
     Format: (image :type neomacs :neomacs-id ID :width W :height H :file PATH) */
  Lisp_Object neomacs_id_sym = intern (":neomacs-id");

  /* Build the plist manually */
  Lisp_Object plist = Qnil;
  plist = Fcons (file, plist);
  plist = Fcons (QCfile, plist);
  plist = Fcons (make_fixnum (height), plist);
  plist = Fcons (QCheight, plist);
  plist = Fcons (make_fixnum (width), plist);
  plist = Fcons (QCwidth, plist);
  plist = Fcons (make_fixnum (image_id), plist);
  plist = Fcons (neomacs_id_sym, plist);
  plist = Fcons (Qneomacs, plist);
  plist = Fcons (QCtype, plist);

  /* Final spec is (image :type neomacs ...) */
  Lisp_Object spec = Fcons (Qimage, plist);

  /* Just return the spec - let elisp do the insertion */
  return spec;
}

DEFUN ("neomacs-insert-image-data", Fneomacs_insert_image_data, Sneomacs_insert_image_data, 1, 3, 0,
       doc: /* Insert image from DATA (a string of image bytes) at point as an inline image.
Optional MAX-WIDTH and MAX-HEIGHT limit the image dimensions (scales to fit).
The image format is auto-detected (PNG, JPEG, GIF, WebP, SVG, etc.).
Returns the neomacs image spec on success, nil on failure.

This function bypasses Emacs's native image library requirements,
using gdk-pixbuf to decode the image data.  */)
  (Lisp_Object data, Lisp_Object max_width, Lisp_Object max_height)
{
  CHECK_STRING (data);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    {
      error ("No neomacs display available");
      return Qnil;
    }

  const unsigned char *bytes = SDATA (data);
  ptrdiff_t len = SBYTES (data);
  int mw = FIXNUMP (max_width) ? XFIXNUM (max_width) : 0;
  int mh = FIXNUMP (max_height) ? XFIXNUM (max_height) : 0;

  /* Load image via GPU backend */
  uint32_t image_id;
  if (mw > 0 || mh > 0)
    image_id = neomacs_display_load_image_data_scaled (dpyinfo->display_handle,
                                                        bytes, len, mw, mh);
  else
    image_id = neomacs_display_load_image_data (dpyinfo->display_handle, bytes, len);

  if (image_id == 0)
    {
      error ("Failed to load image from data");
      return Qnil;
    }

  /* Get actual image dimensions */
  int width, height;
  if (neomacs_display_get_image_size (dpyinfo->display_handle, image_id, &width, &height) != 0)
    {
      error ("Failed to get image size");
      return Qnil;
    }

  /* Create image spec for display property
     Format: (image :type neomacs :neomacs-id ID :width W :height H) */
  Lisp_Object neomacs_id_sym = intern (":neomacs-id");

  /* Build the plist manually */
  Lisp_Object plist = Qnil;
  plist = Fcons (make_fixnum (height), plist);
  plist = Fcons (QCheight, plist);
  plist = Fcons (make_fixnum (width), plist);
  plist = Fcons (QCwidth, plist);
  plist = Fcons (make_fixnum (image_id), plist);
  plist = Fcons (neomacs_id_sym, plist);
  plist = Fcons (Qneomacs, plist);
  plist = Fcons (QCtype, plist);

  /* Final spec is (image :type neomacs ...) */
  Lisp_Object spec = Fcons (Qimage, plist);

  return spec;
}


/* ============================================================================
 * WebKit API
 * ============================================================================ */

DEFUN ("neomacs-webkit-init", Fneomacs_webkit_init, Sneomacs_webkit_init, 0, 0, 0,
       doc: /* Initialize WebKit subsystem.
Must be called before creating WebKit views.
Returns t on success, nil on failure.  */)
  (void)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  /* Pass NULL - Rust side will try to get EGL display */
  int result = neomacs_display_webkit_init (dpyinfo->display_handle, NULL);
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-create", Fneomacs_webkit_create, Sneomacs_webkit_create, 2, 2, 0,
       doc: /* Create a new WebKit view with WIDTH and HEIGHT.
Returns view ID on success, nil on failure.  */)
  (Lisp_Object width, Lisp_Object height)
{
  CHECK_FIXNUM (width);
  CHECK_FIXNUM (height);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  uint32_t view_id = neomacs_display_webkit_create (dpyinfo->display_handle,
                                                    (int) XFIXNUM (width),
                                                    (int) XFIXNUM (height));

  if (view_id == 0)
    return Qnil;

  return make_fixnum (view_id);
}

DEFUN ("neomacs-webkit-destroy", Fneomacs_webkit_destroy, Sneomacs_webkit_destroy, 1, 1, 0,
       doc: /* Destroy WebKit view with VIEW-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_destroy (dpyinfo->display_handle,
                                               (uint32_t) XFIXNUM (view_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-load-uri", Fneomacs_webkit_load_uri, Sneomacs_webkit_load_uri, 2, 2, 0,
       doc: /* Load URI in WebKit view VIEW-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id, Lisp_Object uri)
{
  CHECK_FIXNUM (view_id);
  CHECK_STRING (uri);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *uri_str = SSDATA (uri);
  int result = neomacs_display_webkit_load_uri (dpyinfo->display_handle,
                                                (uint32_t) XFIXNUM (view_id),
                                                uri_str);
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-go-back", Fneomacs_webkit_go_back, Sneomacs_webkit_go_back, 1, 1, 0,
       doc: /* Go back in WebKit view VIEW-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_go_back (dpyinfo->display_handle,
                                               (uint32_t) XFIXNUM (view_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-go-forward", Fneomacs_webkit_go_forward, Sneomacs_webkit_go_forward, 1, 1, 0,
       doc: /* Go forward in WebKit view VIEW-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_go_forward (dpyinfo->display_handle,
                                                  (uint32_t) XFIXNUM (view_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-reload", Fneomacs_webkit_reload, Sneomacs_webkit_reload, 1, 1, 0,
       doc: /* Reload WebKit view VIEW-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_reload (dpyinfo->display_handle,
                                              (uint32_t) XFIXNUM (view_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-resize", Fneomacs_webkit_resize, Sneomacs_webkit_resize, 3, 3, 0,
       doc: /* Resize WebKit view VIEW-ID to WIDTH x HEIGHT pixels.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id, Lisp_Object width, Lisp_Object height)
{
  CHECK_FIXNUM (view_id);
  CHECK_FIXNUM (width);
  CHECK_FIXNUM (height);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_resize (dpyinfo->display_handle,
                                              (uint32_t) XFIXNUM (view_id),
                                              (int) XFIXNUM (width),
                                              (int) XFIXNUM (height));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-execute-js", Fneomacs_webkit_execute_js, Sneomacs_webkit_execute_js, 2, 2, 0,
       doc: /* Execute JavaScript SCRIPT in WebKit view VIEW-ID.
Returns t on success, nil on failure.  */)
  (Lisp_Object view_id, Lisp_Object script)
{
  CHECK_FIXNUM (view_id);
  CHECK_STRING (script);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *script_str = SSDATA (script);
  int result = neomacs_display_webkit_execute_js (dpyinfo->display_handle,
                                                  (uint32_t) XFIXNUM (view_id),
                                                  script_str);
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-floating", Fneomacs_webkit_floating, Sneomacs_webkit_floating, 5, 5, 0,
       doc: /* Display WebKit view VIEW-ID as a floating layer at X, Y with WIDTH and HEIGHT.
The browser view is rendered on top of the frame content at a fixed screen position.  */)
  (Lisp_Object view_id, Lisp_Object x, Lisp_Object y,
   Lisp_Object width, Lisp_Object height)
{
  CHECK_FIXNUM (view_id);
  CHECK_FIXNUM (x);
  CHECK_FIXNUM (y);
  CHECK_FIXNUM (width);
  CHECK_FIXNUM (height);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_set_floating_webkit (dpyinfo->display_handle,
                                       (uint32_t) XFIXNUM (view_id),
                                       (int) XFIXNUM (x),
                                       (int) XFIXNUM (y),
                                       (int) XFIXNUM (width),
                                       (int) XFIXNUM (height));
  return Qt;
}

DEFUN ("neomacs-webkit-floating-clear", Fneomacs_webkit_floating_clear, Sneomacs_webkit_floating_clear, 1, 1, 0,
       doc: /* Remove floating WebKit layer for VIEW-ID.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_hide_floating_webkit (dpyinfo->display_handle,
                                        (uint32_t) XFIXNUM (view_id));
  return Qt;
}

DEFUN ("neomacs-webkit-send-key", Fneomacs_webkit_send_key, Sneomacs_webkit_send_key, 4, 5, 0,
       doc: /* Send keyboard event to WebKit VIEW-ID.
KEY-CODE is the XKB keysym.
HARDWARE-KEY-CODE is the physical scancode.
PRESSED is non-nil for key down, nil for key up.
MODIFIERS is a bitmask: ctrl=1, shift=2, alt=4, meta=8.  */)
  (Lisp_Object view_id, Lisp_Object key_code, Lisp_Object hardware_key_code,
   Lisp_Object pressed, Lisp_Object modifiers)
{
  CHECK_FIXNUM (view_id);
  CHECK_FIXNUM (key_code);
  CHECK_FIXNUM (hardware_key_code);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  uint32_t mods = NILP (modifiers) ? 0 : (uint32_t) XFIXNUM (modifiers);

  neomacs_display_webkit_send_key (dpyinfo->display_handle,
                                   (uint32_t) XFIXNUM (view_id),
                                   (uint32_t) XFIXNUM (key_code),
                                   (uint32_t) XFIXNUM (hardware_key_code),
                                   !NILP (pressed) ? 1 : 0,
                                   mods);
  return Qt;
}

DEFUN ("neomacs-webkit-send-pointer", Fneomacs_webkit_send_pointer, Sneomacs_webkit_send_pointer, 6, 7, 0,
       doc: /* Send pointer/mouse event to WebKit VIEW-ID.
EVENT-TYPE is 1 for motion, 2 for button.
X and Y are coordinates relative to the view.
BUTTON is the mouse button (1=left, 2=middle, 3=right).
STATE is button state (1=pressed, 0=released).
MODIFIERS is a bitmask.  */)
  (Lisp_Object view_id, Lisp_Object event_type, Lisp_Object x, Lisp_Object y,
   Lisp_Object button, Lisp_Object state, Lisp_Object modifiers)
{
  CHECK_FIXNUM (view_id);
  CHECK_FIXNUM (event_type);
  CHECK_FIXNUM (x);
  CHECK_FIXNUM (y);
  CHECK_FIXNUM (button);
  CHECK_FIXNUM (state);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  uint32_t mods = NILP (modifiers) ? 0 : (uint32_t) XFIXNUM (modifiers);

  neomacs_display_webkit_send_pointer (dpyinfo->display_handle,
                                       (uint32_t) XFIXNUM (view_id),
                                       (uint32_t) XFIXNUM (event_type),
                                       (int) XFIXNUM (x),
                                       (int) XFIXNUM (y),
                                       (uint32_t) XFIXNUM (button),
                                       (uint32_t) XFIXNUM (state),
                                       mods);
  return Qt;
}

DEFUN ("neomacs-webkit-send-scroll", Fneomacs_webkit_send_scroll, Sneomacs_webkit_send_scroll, 5, 5, 0,
       doc: /* Send scroll event to WebKit VIEW-ID at position X, Y.
DELTA-X is horizontal scroll amount (positive = right).
DELTA-Y is vertical scroll amount (positive = down).  */)
  (Lisp_Object view_id, Lisp_Object x, Lisp_Object y,
   Lisp_Object delta_x, Lisp_Object delta_y)
{
  CHECK_FIXNUM (view_id);
  CHECK_FIXNUM (x);
  CHECK_FIXNUM (y);
  CHECK_FIXNUM (delta_x);
  CHECK_FIXNUM (delta_y);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_webkit_send_scroll (dpyinfo->display_handle,
                                      (uint32_t) XFIXNUM (view_id),
                                      (int) XFIXNUM (x),
                                      (int) XFIXNUM (y),
                                      (int) XFIXNUM (delta_x),
                                      (int) XFIXNUM (delta_y));
  return Qt;
}

DEFUN ("neomacs-webkit-click", Fneomacs_webkit_click, Sneomacs_webkit_click, 4, 4, 0,
       doc: /* Click in WebKit VIEW-ID at position X, Y with BUTTON.
BUTTON is 1 for left, 2 for middle, 3 for right.  */)
  (Lisp_Object view_id, Lisp_Object x, Lisp_Object y, Lisp_Object button)
{
  CHECK_FIXNUM (view_id);
  CHECK_FIXNUM (x);
  CHECK_FIXNUM (y);
  CHECK_FIXNUM (button);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_webkit_click (dpyinfo->display_handle,
                                (uint32_t) XFIXNUM (view_id),
                                (int) XFIXNUM (x),
                                (int) XFIXNUM (y),
                                (uint32_t) XFIXNUM (button));
  return Qt;
}

DEFUN ("neomacs-webkit-get-title", Fneomacs_webkit_get_title, Sneomacs_webkit_get_title, 1, 1, 0,
       doc: /* Get the title of WebKit VIEW-ID.
Returns the page title as a string, or nil if not available.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  char *title = neomacs_display_webkit_get_title (dpyinfo->display_handle,
                                                   (uint32_t) XFIXNUM (view_id));
  if (!title)
    return Qnil;

  Lisp_Object result = build_string (title);
  neomacs_display_webkit_free_string (title);
  return result;
}

DEFUN ("neomacs-webkit-get-url", Fneomacs_webkit_get_url, Sneomacs_webkit_get_url, 1, 1, 0,
       doc: /* Get the current URL of WebKit VIEW-ID.
Returns the URL as a string, or nil if not available.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  char *url = neomacs_display_webkit_get_url (dpyinfo->display_handle,
                                               (uint32_t) XFIXNUM (view_id));
  if (!url)
    return Qnil;

  Lisp_Object result = build_string (url);
  neomacs_display_webkit_free_string (url);
  return result;
}

DEFUN ("neomacs-webkit-get-progress", Fneomacs_webkit_get_progress, Sneomacs_webkit_get_progress, 1, 1, 0,
       doc: /* Get the loading progress of WebKit VIEW-ID.
Returns a float from 0.0 to 1.0, or nil if view not found.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  double progress = neomacs_display_webkit_get_progress (dpyinfo->display_handle,
                                                          (uint32_t) XFIXNUM (view_id));
  if (progress < 0)
    return Qnil;

  return make_float (progress);
}

DEFUN ("neomacs-webkit-loading-p", Fneomacs_webkit_loading_p, Sneomacs_webkit_loading_p, 1, 1, 0,
       doc: /* Return non-nil if WebKit VIEW-ID is currently loading.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int loading = neomacs_display_webkit_is_loading (dpyinfo->display_handle,
                                                    (uint32_t) XFIXNUM (view_id));
  return loading == 1 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-update", Fneomacs_webkit_update, Sneomacs_webkit_update, 1, 1, 0,
       doc: /* Process pending events for WebKit VIEW-ID.
Pumps the GLib main context to handle WebKit events (including frame rendering).
Returns t on success, nil if view not found.  */)
  (Lisp_Object view_id)
{
  CHECK_FIXNUM (view_id);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_update (dpyinfo->display_handle,
                                               (uint32_t) XFIXNUM (view_id));
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-webkit-update-all", Fneomacs_webkit_update_all, Sneomacs_webkit_update_all, 0, 0, 0,
       doc: /* Process pending events for all WebKit views.
Pumps the GLib main context to handle WebKit events (including frame rendering).
Call this once per frame/redraw cycle to keep WebKit views rendering.
Returns t on success, nil on failure.  */)
  (void)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_webkit_update_all (dpyinfo->display_handle);
  return result == 0 ? Qt : Qnil;
}

/* C callback that calls the Lisp function for new window requests */
static bool
neomacs_webkit_new_window_callback (uint32_t view_id, const char *url, const char *frame_name)
{
  if (NILP (Vneomacs_webkit_new_window_function))
    return false;

  /* Call the Lisp function safely */
  Lisp_Object result = safe_calln (Vneomacs_webkit_new_window_function,
                                   make_fixnum (view_id),
                                   url ? build_string (url) : Qnil,
                                   frame_name ? build_string (frame_name) : Qnil);

  return !NILP (result);
}

DEFUN ("neomacs-webkit-set-new-window-function", Fneomacs_webkit_set_new_window_function,
       Sneomacs_webkit_set_new_window_function, 1, 1, 0,
       doc: /* Set FUNCTION as the handler for WebKit new window requests.
FUNCTION is called with three arguments: (VIEW-ID URL FRAME-NAME)
when a link with target=\"_blank\" is clicked or window.open() is called.

If FUNCTION returns non-nil, the new window request is handled by Emacs
(typically by opening URL in a new buffer). If it returns nil, the request
is ignored.

Pass nil to clear the handler.  */)
  (Lisp_Object function)
{
  Vneomacs_webkit_new_window_function = function;

  /* Register or clear the C callback */
  if (NILP (function))
    {
      neomacs_display_webkit_set_new_window_callback (NULL);
    }
  else
    {
      neomacs_display_webkit_set_new_window_callback (neomacs_webkit_new_window_callback);
    }

  return Qt;
}

/* C callback for page load events */
static void
neomacs_webkit_load_callback_impl (uint32_t view_id, int load_event, const char *uri)
{
  if (NILP (Vneomacs_webkit_load_callback))
    return;

  /* load_event: 0=started, 1=redirected, 2=committed, 3=finished, 4=failed */
  Lisp_Object event_sym;
  switch (load_event)
    {
    case 0: event_sym = intern ("started"); break;
    case 1: event_sym = intern ("redirected"); break;
    case 2: event_sym = intern ("committed"); break;
    case 3: event_sym = intern ("finished"); break;
    case 4: event_sym = intern ("failed"); break;
    default: event_sym = intern ("unknown"); break;
    }

  safe_calln (Vneomacs_webkit_load_callback,
              make_fixnum (view_id),
              event_sym,
              uri ? build_string (uri) : Qnil);
}

DEFUN ("neomacs-webkit-set-load-callback", Fneomacs_webkit_set_load_callback,
       Sneomacs_webkit_set_load_callback, 1, 1, 0,
       doc: /* Set FUNCTION as callback for WebKit page load events.
FUNCTION is called with three arguments: (VIEW-ID EVENT URI)
where EVENT is one of: started, redirected, committed, finished, failed.
Pass nil to clear the callback.  */)
  (Lisp_Object function)
{
  Vneomacs_webkit_load_callback = function;

  /* Register or clear the C callback */
  if (NILP (function))
    neomacs_display_webkit_set_load_callback (NULL);
  else
    neomacs_display_webkit_set_load_callback (neomacs_webkit_load_callback_impl);

  return Qt;
}

DEFUN ("neomacs-insert-webkit", Fneomacs_insert_webkit, Sneomacs_insert_webkit, 3, 4, 0,
       doc: /* Create webkit view and return display spec for inline display.
URI is the URL to load (or nil for blank).
WIDTH and HEIGHT are the display dimensions.
Optional LOAD-P if non-nil means load URI immediately.
Returns a display spec suitable for use with `insert' and `propertize':
  (webkit :id ID :width W :height H)

Example usage:
  (insert (propertize \" \" 'display (neomacs-insert-webkit \"https://example.com\" 400 300)))

To update an existing webkit view, use `neomacs-webkit-load-uri' with the ID.  */)
  (Lisp_Object uri, Lisp_Object width, Lisp_Object height, Lisp_Object load_p)
{
  CHECK_FIXNUM (width);
  CHECK_FIXNUM (height);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    {
      error ("No neomacs display available");
      return Qnil;
    }

  int w = XFIXNUM (width);
  int h = XFIXNUM (height);

  /* Create webkit view */
  uint32_t view_id = neomacs_display_webkit_create (dpyinfo->display_handle, w, h);
  if (view_id == 0)
    {
      error ("Failed to create WebKit view");
      return Qnil;
    }

  /* Load URI if provided and load_p is non-nil */
  if (!NILP (uri) && !NILP (load_p))
    {
      CHECK_STRING (uri);
      const char *uri_str = SSDATA (uri);
      neomacs_display_webkit_load_uri (dpyinfo->display_handle, view_id, uri_str);
    }

  /* Create display spec: (webkit :id ID :width W :height H) */
  Lisp_Object plist = Qnil;
  plist = Fcons (make_fixnum (h), plist);
  plist = Fcons (QCheight, plist);
  plist = Fcons (make_fixnum (w), plist);
  plist = Fcons (QCwidth, plist);
  plist = Fcons (make_fixnum (view_id), plist);
  plist = Fcons (QCid, plist);

  Lisp_Object spec = Fcons (Qwebkit, plist);
  return spec;
}


/* ============================================================================
 * Animation API
 * ============================================================================ */

DEFUN ("neomacs-set-animation-option", Fneomacs_set_animation_option, Sneomacs_set_animation_option, 2, 2, 0,
       doc: /* Set OPTION to VALUE for Neomacs animation system.
OPTION is a string naming the option:
  \"animation\" - master enable/disable (\"t\" or \"nil\")
  \"cursor-animation\" - enable cursor animation (\"t\" or \"nil\")
  \"cursor-animation-mode\" - cursor effect mode (\"smooth\", \"railgun\", \"torpedo\", \"pixiedust\", \"sonicboom\", \"ripple\", \"wireframe\", \"none\")
  \"cursor-animation-speed\" - cursor animation speed (1-100)
  \"cursor-glow\" - enable cursor glow (\"t\" or \"nil\")
  \"cursor-particle-count\" - number of particles (1-100)
  \"buffer-transition\" - enable buffer transitions (\"t\" or \"nil\")
  \"buffer-transition-effect\" - transition effect (\"crossfade\", \"slide-left\", \"slide-right\", \"slide-up\", \"slide-down\", \"scale-fade\", \"push\", \"blur\", \"page-curl\", \"none\")
  \"buffer-transition-duration\" - duration in milliseconds
VALUE is a string with the new value.
Returns t on success, nil on failure.  */)
  (Lisp_Object option, Lisp_Object value)
{
  CHECK_STRING (option);
  CHECK_STRING (value);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *opt = SSDATA (option);
  const char *val = SSDATA (value);

  int result = neomacs_display_set_animation_option (dpyinfo->display_handle, opt, val);
  return result ? Qt : Qnil;
}

DEFUN ("neomacs-get-animation-option", Fneomacs_get_animation_option, Sneomacs_get_animation_option, 1, 1, 0,
       doc: /* Get the value of animation OPTION.
OPTION is a string naming the option (see `neomacs-set-animation-option').
Returns the current value as a string, or nil if the option is unknown.  */)
  (Lisp_Object option)
{
  CHECK_STRING (option);

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *opt = SSDATA (option);
  char *value = neomacs_display_get_animation_option (dpyinfo->display_handle, opt);

  if (!value)
    return Qnil;

  Lisp_Object result = build_string (value);
  neomacs_display_free_string (value);
  return result;
}

DEFUN ("neomacs-start-buffer-transition", Fneomacs_start_buffer_transition, Sneomacs_start_buffer_transition, 1, 2, 0,
       doc: /* Start a buffer transition animation with EFFECT.
EFFECT is a string naming the effect:
  \"crossfade\" - simple fade between buffers
  \"slide-left\" - slide horizontally (new buffer comes from right)
  \"slide-right\" - slide horizontally (new buffer comes from left)
  \"slide-up\" - slide vertically (new buffer comes from bottom)
  \"slide-down\" - slide vertically (new buffer comes from top)
  \"scale-fade\" - scale and fade
  \"push\" - new buffer pushes over old
  \"blur\" - blur transition
  \"page-curl\" - 3D book page turn effect
  \"none\" - no animation (instant switch)
Optional DURATION is the animation duration in milliseconds (default 300).
Returns t on success, nil on failure.  */)
  (Lisp_Object effect, Lisp_Object duration)
{
  CHECK_STRING (effect);

  int duration_ms = 300;  /* Default */
  if (!NILP (duration))
    {
      CHECK_FIXNUM (duration);
      duration_ms = XFIXNUM (duration);
      if (duration_ms < 0)
        duration_ms = 0;
      if (duration_ms > 5000)
        duration_ms = 5000;  /* Cap at 5 seconds */
    }

  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  const char *eff = SSDATA (effect);
  int result = neomacs_display_start_buffer_transition (dpyinfo->display_handle, eff, duration_ms);
  return result ? Qt : Qnil;
}

DEFUN ("neomacs-animation-active-p", Fneomacs_animation_active_p, Sneomacs_animation_active_p, 0, 0, 0,
       doc: /* Return non-nil if any animation is currently active.
This includes cursor animation and buffer transition animation.  */)
  (void)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int active = neomacs_display_animation_active (dpyinfo->display_handle);
  return active ? Qt : Qnil;
}

DEFUN ("neomacs-prepare-buffer-transition", Fneomacs_prepare_buffer_transition, Sneomacs_prepare_buffer_transition, 0, 0, 0,
       doc: /* Prepare for buffer transition by capturing current frame.
Call this BEFORE switching buffers to capture the "old" frame.
The captured frame will be used as the starting point for the transition animation.
Returns t on success, nil on failure.  */)
  (void)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_prepare_buffer_transition (dpyinfo->display_handle);
  return result ? Qt : Qnil;
}

DEFUN ("neomacs-trigger-buffer-transition", Fneomacs_trigger_buffer_transition, Sneomacs_trigger_buffer_transition, 0, 0, 0,
       doc: /* Trigger buffer transition animation after buffer has changed.
Call this AFTER switching buffers to start the transition animation.
The animation will transition from the previously captured frame to the new content.
Returns t if animation started, nil otherwise.  */)
  (void)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int result = neomacs_display_trigger_buffer_transition (dpyinfo->display_handle);
  return result ? Qt : Qnil;
}

DEFUN ("neomacs-has-transition-snapshot-p", Fneomacs_has_transition_snapshot_p, Sneomacs_has_transition_snapshot_p, 0, 0, 0,
       doc: /* Return non-nil if a transition snapshot is ready.
This indicates that `neomacs-prepare-buffer-transition' has been called
and a frame has been captured.  */)
  (void)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int has_snapshot = neomacs_display_has_transition_snapshot (dpyinfo->display_handle);
  return has_snapshot ? Qt : Qnil;
}


DEFUN ("neomacs-set-cursor-blink", Fneomacs_set_cursor_blink, Sneomacs_set_cursor_blink, 1, 2, 0,
       doc: /* Configure cursor blinking in the render thread.
ENABLED non-nil enables blinking, nil disables it.
Optional INTERVAL is the blink interval in seconds (default 0.5).  */)
  (Lisp_Object enabled, Lisp_Object interval)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int blink_enabled = !NILP (enabled);
  int interval_ms = 500;
  if (!NILP (interval) && NUMBERP (interval))
    interval_ms = (int)(XFLOATINT (interval) * 1000);

  neomacs_display_set_cursor_blink (dpyinfo->display_handle,
                                    blink_enabled, interval_ms);
  return blink_enabled ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-animation", Fneomacs_set_cursor_animation, Sneomacs_set_cursor_animation, 1, 2, 0,
       doc: /* Configure cursor animation (smooth motion) in the render thread.
ENABLED non-nil enables smooth cursor animation, nil disables it.
Optional SPEED is the exponential interpolation rate (default 15.0).
Higher values make the cursor move faster.  */)
  (Lisp_Object enabled, Lisp_Object speed)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int anim_enabled = !NILP (enabled);
  float anim_speed = 15.0f;
  if (!NILP (speed) && NUMBERP (speed))
    anim_speed = (float) XFLOATINT (speed);

  neomacs_display_set_cursor_animation (dpyinfo->display_handle,
                                         anim_enabled, anim_speed);
  return anim_enabled ? Qt : Qnil;
}

DEFUN ("neomacs-set-animation-config", Fneomacs_set_animation_config, Sneomacs_set_animation_config, 8, MANY, 0,
       doc: /* Configure all animation settings in the render thread.
Arguments: CURSOR-ENABLED CURSOR-SPEED CURSOR-STYLE CURSOR-DURATION
           CROSSFADE-ENABLED CROSSFADE-DURATION SCROLL-ENABLED SCROLL-DURATION
           &optional SCROLL-EFFECT SCROLL-EASING TRAIL-SIZE
           CROSSFADE-EFFECT CROSSFADE-EASING

CURSOR-ENABLED non-nil enables smooth cursor animation.
CURSOR-SPEED is the exponential interpolation rate (default 15.0).
CURSOR-STYLE is a symbol naming the animation style:
  `exponential'  - smooth deceleration, no fixed duration (uses CURSOR-SPEED)
  `spring'       - critically-damped spring, Neovide-like feel
  `ease-out-quad'  - gentle deceleration curve
  `ease-out-cubic' - stronger deceleration curve
  `ease-out-expo'  - sharp deceleration curve
  `ease-in-out-cubic' - smooth S-curve acceleration + deceleration
  `linear'       - constant speed, uniform motion
CURSOR-DURATION is duration in milliseconds for non-exponential styles (default 150).
CROSSFADE-ENABLED non-nil enables buffer-switch crossfade.
CROSSFADE-DURATION is duration in milliseconds (default 200).
SCROLL-ENABLED non-nil enables scroll slide animation.
SCROLL-DURATION is duration in milliseconds (default 150).
SCROLL-EFFECT is a symbol (or integer index) selecting the scroll animation effect:
  `slide'                - content slides in scroll direction (default)
  `crossfade'            - alpha blend between old and new
  `scale-zoom'           - destination zooms from 95% to 100%
  `fade-edges'           - lines fade at viewport edges
  `cascade'              - lines drop in with stagger delay
  `parallax'             - layers scroll at different speeds
  `tilt'                 - subtle 3D perspective tilt
  `page-curl'            - page turning effect
  `card-flip'            - card flips around X-axis
  `cylinder-roll'        - content wraps around cylinder
  `wobbly'               - jelly-like deformation
  `wave'                 - sine-wave distortion
  `per-line-spring'      - each line springs independently
  `liquid'               - noise-based fluid distortion
  `motion-blur'          - vertical blur during scroll
  `chromatic-aberration' - RGB channel separation
  `ghost-trails'         - semi-transparent afterimages
  `color-temperature'    - warm/cool tint by direction
  `crt-scanlines'        - retro scanline overlay
  `depth-of-field'       - center sharp, edges dim
  `typewriter-reveal'    - lines appear left-to-right
SCROLL-EASING is a symbol (or integer index) selecting the scroll easing function:
  `ease-out-quad'        - standard deceleration (default)
  `ease-out-cubic'       - stronger deceleration
  `spring'               - critically damped spring with overshoot
  `linear'               - constant speed
  `ease-in-out-cubic'    - smooth S-curve
Optional TRAIL-SIZE (0.0-1.0) controls the spring cursor trail effect (default 0.7).
  0.0 means no trail (all corners move together like a rigid rectangle).
  0.7 is the default with a visible trailing stretch effect.
  1.0 is maximum trail where leading corners snap almost immediately.
CROSSFADE-EFFECT is a symbol (or integer index) selecting the buffer-switch
  transition effect.  Accepts the same symbols as SCROLL-EFFECT.
  Default is `crossfade' (simple alpha blend).
CROSSFADE-EASING is a symbol (or integer index) selecting the easing function
  for buffer-switch transitions.  Accepts the same symbols as SCROLL-EASING.
  Default is `ease-out-quad'.
usage: (neomacs-set-animation-config CURSOR-ENABLED CURSOR-SPEED CURSOR-STYLE CURSOR-DURATION CROSSFADE-ENABLED CROSSFADE-DURATION SCROLL-ENABLED SCROLL-DURATION &optional SCROLL-EFFECT SCROLL-EASING TRAIL-SIZE CROSSFADE-EFFECT CROSSFADE-EASING)  */)
  (ptrdiff_t nargs, Lisp_Object *args)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  /* Required args: 0-7 */
  Lisp_Object cursor_enabled = args[0];
  Lisp_Object cursor_speed = args[1];
  Lisp_Object cursor_style = args[2];
  Lisp_Object cursor_duration = args[3];
  Lisp_Object crossfade_enabled = args[4];
  Lisp_Object crossfade_duration = args[5];
  Lisp_Object scroll_enabled = args[6];
  Lisp_Object scroll_duration = args[7];

  /* Optional args: 8-12 */
  Lisp_Object scroll_effect = nargs > 8 ? args[8] : Qnil;
  Lisp_Object scroll_easing = nargs > 9 ? args[9] : Qnil;
  Lisp_Object trail_size = nargs > 10 ? args[10] : Qnil;
  Lisp_Object crossfade_effect = nargs > 11 ? args[11] : Qnil;
  Lisp_Object crossfade_easing = nargs > 12 ? args[12] : Qnil;

  int ce = !NILP (cursor_enabled);
  float cs = 15.0f;
  if (NUMBERP (cursor_speed))
    cs = (float) XFLOATINT (cursor_speed);

  /* Map symbol to style ID */
  uint8_t cst = 1; /* default: spring */
  if (SYMBOLP (cursor_style))
    {
      if (EQ (cursor_style, Qexponential))
        cst = 0;
      else if (EQ (cursor_style, Qspring))
        cst = 1;
      else if (EQ (cursor_style, Qease_out_quad))
        cst = 2;
      else if (EQ (cursor_style, Qease_out_cubic))
        cst = 3;
      else if (EQ (cursor_style, Qease_out_expo))
        cst = 4;
      else if (EQ (cursor_style, Qease_in_out_cubic))
        cst = 5;
      else if (EQ (cursor_style, Qlinear))
        cst = 6;
    }
  else if (FIXNUMP (cursor_style))
    cst = (uint8_t) XFIXNUM (cursor_style);

  uint32_t cd = 150;
  if (NUMBERP (cursor_duration))
    cd = (uint32_t) XFIXNUM (cursor_duration);

  int cfe = !NILP (crossfade_enabled);
  uint32_t cfd = 200;
  if (NUMBERP (crossfade_duration))
    cfd = (uint32_t) XFIXNUM (crossfade_duration);

  int se = !NILP (scroll_enabled);
  uint32_t sd = 150;
  if (NUMBERP (scroll_duration))
    sd = (uint32_t) XFIXNUM (scroll_duration);

  /* Map symbol or integer to scroll effect ID */
  uint32_t seff = 0;  /* default: slide */
  if (SYMBOLP (scroll_effect))
    {
      if (EQ (scroll_effect, Qslide))                    seff = 0;
      else if (EQ (scroll_effect, Qcrossfade))            seff = 1;
      else if (EQ (scroll_effect, Qscale_zoom))           seff = 2;
      else if (EQ (scroll_effect, Qfade_edges))           seff = 3;
      else if (EQ (scroll_effect, Qcascade))              seff = 4;
      else if (EQ (scroll_effect, Qparallax))             seff = 5;
      else if (EQ (scroll_effect, Qtilt))                 seff = 6;
      else if (EQ (scroll_effect, Qpage_curl))            seff = 7;
      else if (EQ (scroll_effect, Qcard_flip))            seff = 8;
      else if (EQ (scroll_effect, Qcylinder_roll))        seff = 9;
      else if (EQ (scroll_effect, Qwobbly))               seff = 10;
      else if (EQ (scroll_effect, Qwave))                 seff = 11;
      else if (EQ (scroll_effect, Qper_line_spring))      seff = 12;
      else if (EQ (scroll_effect, Qliquid))               seff = 13;
      else if (EQ (scroll_effect, Qmotion_blur))          seff = 14;
      else if (EQ (scroll_effect, Qchromatic_aberration)) seff = 15;
      else if (EQ (scroll_effect, Qghost_trails))         seff = 16;
      else if (EQ (scroll_effect, Qcolor_temperature))    seff = 17;
      else if (EQ (scroll_effect, Qcrt_scanlines))        seff = 18;
      else if (EQ (scroll_effect, Qdepth_of_field))       seff = 19;
      else if (EQ (scroll_effect, Qtypewriter_reveal))    seff = 20;
    }
  else if (FIXNUMP (scroll_effect))
    seff = (uint32_t) XFIXNUM (scroll_effect);

  /* Map symbol or integer to scroll easing ID */
  uint32_t seas = 0;  /* default: ease-out-quad */
  if (SYMBOLP (scroll_easing))
    {
      if (EQ (scroll_easing, Qease_out_quad))           seas = 0;
      else if (EQ (scroll_easing, Qease_out_cubic))     seas = 1;
      else if (EQ (scroll_easing, Qspring))              seas = 2;
      else if (EQ (scroll_easing, Qlinear))              seas = 3;
      else if (EQ (scroll_easing, Qease_in_out_cubic))  seas = 4;
    }
  else if (FIXNUMP (scroll_easing))
    seas = (uint32_t) XFIXNUM (scroll_easing);

  float ts = 0.7f;
  if (NUMBERP (trail_size))
    ts = (float) XFLOATINT (trail_size);

  /* Map symbol or integer to crossfade effect ID (same enum as scroll effect) */
  uint32_t ceff = 1;  /* default: crossfade (index 1 in ScrollEffect) */
  if (SYMBOLP (crossfade_effect))
    {
      if (EQ (crossfade_effect, Qslide))                    ceff = 0;
      else if (EQ (crossfade_effect, Qcrossfade))            ceff = 1;
      else if (EQ (crossfade_effect, Qscale_zoom))           ceff = 2;
      else if (EQ (crossfade_effect, Qfade_edges))           ceff = 3;
      else if (EQ (crossfade_effect, Qcascade))              ceff = 4;
      else if (EQ (crossfade_effect, Qparallax))             ceff = 5;
      else if (EQ (crossfade_effect, Qtilt))                 ceff = 6;
      else if (EQ (crossfade_effect, Qpage_curl))            ceff = 7;
      else if (EQ (crossfade_effect, Qcard_flip))            ceff = 8;
      else if (EQ (crossfade_effect, Qcylinder_roll))        ceff = 9;
      else if (EQ (crossfade_effect, Qwobbly))               ceff = 10;
      else if (EQ (crossfade_effect, Qwave))                 ceff = 11;
      else if (EQ (crossfade_effect, Qper_line_spring))      ceff = 12;
      else if (EQ (crossfade_effect, Qliquid))               ceff = 13;
      else if (EQ (crossfade_effect, Qmotion_blur))          ceff = 14;
      else if (EQ (crossfade_effect, Qchromatic_aberration)) ceff = 15;
      else if (EQ (crossfade_effect, Qghost_trails))         ceff = 16;
      else if (EQ (crossfade_effect, Qcolor_temperature))    ceff = 17;
      else if (EQ (crossfade_effect, Qcrt_scanlines))        ceff = 18;
      else if (EQ (crossfade_effect, Qdepth_of_field))       ceff = 19;
      else if (EQ (crossfade_effect, Qtypewriter_reveal))    ceff = 20;
    }
  else if (FIXNUMP (crossfade_effect))
    ceff = (uint32_t) XFIXNUM (crossfade_effect);

  /* Map symbol or integer to crossfade easing ID (same enum as scroll easing) */
  uint32_t ceas = 0;  /* default: ease-out-quad */
  if (SYMBOLP (crossfade_easing))
    {
      if (EQ (crossfade_easing, Qease_out_quad))           ceas = 0;
      else if (EQ (crossfade_easing, Qease_out_cubic))     ceas = 1;
      else if (EQ (crossfade_easing, Qspring))              ceas = 2;
      else if (EQ (crossfade_easing, Qlinear))              ceas = 3;
      else if (EQ (crossfade_easing, Qease_in_out_cubic))  ceas = 4;
    }
  else if (FIXNUMP (crossfade_easing))
    ceas = (uint32_t) XFIXNUM (crossfade_easing);

  neomacs_display_set_animation_config (dpyinfo->display_handle,
                                         ce, cs, cst, cd, cfe, cfd, se, sd,
                                         seff, seas, ts, ceff, ceas);
  return Qt;
}


/* ============================================================================
 * Terminal Emulator (neo-term) Functions
 * ============================================================================ */

DEFUN ("neomacs-terminal-create", Fneomacs_terminal_create, Sneomacs_terminal_create, 3, 4, 0,
       doc: /* Create a GPU-accelerated terminal with COLS columns and ROWS rows.
MODE is 0 for Window, 1 for Inline, 2 for Floating.
Optional SHELL is the shell program path (nil means default).
Returns terminal ID on success, nil on failure.  */)
  (Lisp_Object cols, Lisp_Object rows, Lisp_Object mode, Lisp_Object shell)
{
  CHECK_FIXNUM (cols);
  CHECK_FIXNUM (rows);
  CHECK_FIXNUM (mode);

  const char *shell_str = NULL;
  if (!NILP (shell))
    {
      CHECK_STRING (shell);
      shell_str = SSDATA (shell);
    }

  uint32_t id = neomacs_display_terminal_create (
    (uint16_t) XFIXNUM (cols),
    (uint16_t) XFIXNUM (rows),
    (uint8_t) XFIXNUM (mode),
    shell_str);

  if (id == 0)
    return Qnil;

  return make_fixnum (id);
}

DEFUN ("neomacs-terminal-write", Fneomacs_terminal_write, Sneomacs_terminal_write, 2, 2, 0,
       doc: /* Write STRING to terminal TERMINAL-ID.
STRING is sent as keyboard input to the terminal's PTY.  */)
  (Lisp_Object terminal_id, Lisp_Object string)
{
  CHECK_FIXNUM (terminal_id);
  CHECK_STRING (string);

  neomacs_display_terminal_write (
    (uint32_t) XFIXNUM (terminal_id),
    (const uint8_t *) SDATA (string),
    SBYTES (string));

  return Qt;
}

DEFUN ("neomacs-terminal-resize", Fneomacs_terminal_resize, Sneomacs_terminal_resize, 3, 3, 0,
       doc: /* Resize terminal TERMINAL-ID to COLS columns and ROWS rows.  */)
  (Lisp_Object terminal_id, Lisp_Object cols, Lisp_Object rows)
{
  CHECK_FIXNUM (terminal_id);
  CHECK_FIXNUM (cols);
  CHECK_FIXNUM (rows);

  neomacs_display_terminal_resize (
    (uint32_t) XFIXNUM (terminal_id),
    (uint16_t) XFIXNUM (cols),
    (uint16_t) XFIXNUM (rows));

  return Qt;
}

DEFUN ("neomacs-terminal-destroy", Fneomacs_terminal_destroy, Sneomacs_terminal_destroy, 1, 1, 0,
       doc: /* Destroy terminal TERMINAL-ID.  */)
  (Lisp_Object terminal_id)
{
  CHECK_FIXNUM (terminal_id);

  neomacs_display_terminal_destroy ((uint32_t) XFIXNUM (terminal_id));

  return Qt;
}

DEFUN ("neomacs-terminal-set-float", Fneomacs_terminal_set_float, Sneomacs_terminal_set_float, 4, 4, 0,
       doc: /* Set floating position and opacity for terminal TERMINAL-ID.
X and Y are the screen coordinates, OPACITY is 0.0 to 1.0.  */)
  (Lisp_Object terminal_id, Lisp_Object x, Lisp_Object y, Lisp_Object opacity)
{
  CHECK_FIXNUM (terminal_id);
  CHECK_NUMBER (x);
  CHECK_NUMBER (y);
  CHECK_NUMBER (opacity);

  neomacs_display_terminal_set_float (
    (uint32_t) XFIXNUM (terminal_id),
    (float) XFLOATINT (x),
    (float) XFLOATINT (y),
    (float) XFLOATINT (opacity));

  return Qt;
}

DEFUN ("neomacs-terminal-get-text", Fneomacs_terminal_get_text, Sneomacs_terminal_get_text, 1, 1, 0,
       doc: /* Get visible text from terminal TERMINAL-ID.
Returns a string, or nil if the terminal is not found.  */)
  (Lisp_Object terminal_id)
{
  CHECK_FIXNUM (terminal_id);

  char *text = neomacs_display_terminal_get_text ((uint32_t) XFIXNUM (terminal_id));
  if (!text)
    return Qnil;

  Lisp_Object result = build_string (text);
  free (text);
  return result;
}


/* ============================================================================
 * Miscellaneous Functions
 * ============================================================================ */

/* Find display info for a given display name.  */
static struct neomacs_display_info *
neomacs_display_info_for_name (Lisp_Object name)
{
  struct neomacs_display_info *dpyinfo;

  CHECK_STRING (name);

  for (dpyinfo = neomacs_display_list; dpyinfo; dpyinfo = dpyinfo->next)
    {
      if (dpyinfo->name_list_element
          && !NILP (Fstring_equal (XCAR (dpyinfo->name_list_element), name)))
        return dpyinfo;
    }

  /* If display not found, try to open it.  For now, just use first display.  */
  if (neomacs_display_list)
    return neomacs_display_list;

  error ("Cannot connect to Neomacs display: %s", SDATA (name));
}

/* Called from frame.c to get display info for x-get-resource.  */
struct neomacs_display_info *
check_x_display_info (Lisp_Object frame)
{
  struct neomacs_display_info *dpyinfo = NULL;

  if (NILP (frame))
    {
      struct frame *sf = XFRAME (selected_frame);

      if (FRAME_NEOMACS_P (sf) && FRAME_LIVE_P (sf))
        dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (sf);
      else if (x_display_list != NULL)
        dpyinfo = x_display_list;
      else
        error ("Neomacs frames are not in use or not initialized");
    }
  else if (TERMINALP (frame))
    {
      struct terminal *t = decode_live_terminal (frame);

      if (t->type != output_neomacs)
        error ("Terminal %d is not a Neomacs display", t->id);

      dpyinfo = t->display_info.neomacs;
    }
  else if (STRINGP (frame))
    dpyinfo = neomacs_display_info_for_name (frame);
  else
    {
      struct frame *f;
      CHECK_FRAME (frame);
      f = XFRAME (frame);

      if (!FRAME_NEOMACS_P (f))
        error ("Frame is not a Neomacs frame");

      dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
    }

  return dpyinfo;
}

/* Get a human-readable name for a keysym.  */
char *
get_keysym_name (int keysym)
{
  /* For GTK4, we could use gdk_keyval_name, but for now return NULL */
  /* This function is used for debugging and error messages */
  return NULL;
}

/* Set mouse pixel position on frame F.  */
void
frame_set_mouse_pixel_position (struct frame *f, int pix_x, int pix_y)
{
  /* TODO: Implement with GTK4 */
}


/* ============================================================================
 * Toolbar Support
 * ============================================================================ */

/* Update the tool bar for frame F.  Currently a stub.  */
void
update_frame_tool_bar (struct frame *f)
{
  /* TODO: Implement tool bar update via Rust/GTK4 */
}

/* Free the tool bar resources for frame F.  Currently a stub.  */
void
free_frame_tool_bar (struct frame *f)
{
  /* TODO: Implement tool bar cleanup */
}


/* ============================================================================
 * Menu Bar Support
 * ============================================================================ */

/* Update the menu bar for frame F.  Currently a stub.  */
void
set_frame_menubar (struct frame *f, bool deep_p)
{
  /* TODO: Implement menu bar via Rust/GTK4 */
}

/* Free the menu bar resources for frame F.  Currently a stub.  */
void
free_frame_menubar (struct frame *f)
{
  /* TODO: Implement menu bar cleanup */
}


/* ============================================================================
 * Threaded Mode Support
 * ============================================================================ */

/* Threaded mode state */
static int threaded_mode_active = 0;
static int wakeup_fd = -1;

/* Forward declaration */
static void neomacs_display_wakeup_handler (int fd, void *data);

/* Handler called when wakeup_fd is readable */
static void
neomacs_display_wakeup_handler (int fd, void *data)
{
  struct NeomacsInputEvent events[64];
  int count;

  /* Drain input events from render thread */
  count = neomacs_display_drain_input (events, 64);

  /* Process events */
  for (int i = 0; i < count; i++)
    {
      struct NeomacsInputEvent *ev = &events[i];
      union buffered_input_event inev;
      struct frame *f = SELECTED_FRAME ();
      bool frame_matched = false;
      Lisp_Object tail, frame;

      /* Find frame by window_id */
      FOR_EACH_FRAME (tail, frame)
        {
          struct frame *tf = XFRAME (frame);
          if (FRAME_NEOMACS_P (tf)
              && FRAME_NEOMACS_OUTPUT (tf)->window_id == ev->windowId)
            {
              f = tf;
              frame_matched = true;
              break;
            }
        }

      /* Fallback for legacy/invalid window ids: route to any Neomacs frame
         instead of potentially sending events to a non-Neomacs frame.  */
      if (!frame_matched && !FRAME_NEOMACS_P (f))
        {
          FOR_EACH_FRAME (tail, frame)
            {
              struct frame *tf = XFRAME (frame);
              if (FRAME_NEOMACS_P (tf))
                {
                  f = tf;
                  break;
                }
            }
        }

      EVENT_INIT (inev.ie);
      inev.ie.timestamp = ev->timestamp;

      switch (ev->kind)
        {
        case NEOMACS_EVENT_KEY_PRESS:
          if (ev->keysym < 0x100)
            inev.ie.kind = ASCII_KEYSTROKE_EVENT;
          else
            inev.ie.kind = NON_ASCII_KEYSTROKE_EVENT;
          inev.ie.code = ev->keysym;
          inev.ie.modifiers = 0;
          if (ev->modifiers & NEOMACS_SHIFT_MASK) inev.ie.modifiers |= shift_modifier;
          if (ev->modifiers & NEOMACS_CTRL_MASK) inev.ie.modifiers |= ctrl_modifier;
          if (ev->modifiers & NEOMACS_META_MASK) inev.ie.modifiers |= meta_modifier;
          if (ev->modifiers & NEOMACS_SUPER_MASK) inev.ie.modifiers |= super_modifier;
          XSETFRAME (inev.ie.frame_or_window, f);
          neomacs_evq_enqueue (&inev);
          break;

        case NEOMACS_EVENT_MOUSE_PRESS:
        case NEOMACS_EVENT_MOUSE_RELEASE:
          {
            /* Check if click is on a webkit view (floating or inline) */
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (ev->kind == NEOMACS_EVENT_MOUSE_PRESS
                && dpyinfo && dpyinfo->display_handle)
              {
                uint32_t webkit_id = 0;
                int rel_x = 0, rel_y = 0;
                if (neomacs_display_webkit_at_position (dpyinfo->display_handle,
                                                         ev->x, ev->y,
                                                         &webkit_id, &rel_x, &rel_y))
                  {
                    neomacs_display_webkit_click (dpyinfo->display_handle,
                                                  webkit_id, rel_x, rel_y, ev->button);
                    /* Store the clicked webkit view ID so Elisp can
                       auto-enter input mode for keyboard forwarding.  */
                    Vneomacs_webkit_clicked_view_id = make_fixnum (webkit_id);
                    /* Fall through to also generate Emacs mouse event,
                       so Elisp can detect the click position.  */
                  }
              }

            /* Check if click is on the tab-bar pseudo-window */
            Lisp_Object tab_bar_arg = Qnil;
            if (WINDOWP (f->tab_bar_window)
                && WINDOW_TOTAL_LINES (XWINDOW (f->tab_bar_window)))
              {
                Lisp_Object window
                  = window_from_coordinates (f, ev->x, ev->y, 0, true, true, true);
                if (EQ (window, f->tab_bar_window))
                  {
                    int emacs_modifiers = 0;
                    if (ev->modifiers & NEOMACS_SHIFT_MASK) emacs_modifiers |= shift_modifier;
                    if (ev->modifiers & NEOMACS_CTRL_MASK) emacs_modifiers |= ctrl_modifier;
                    if (ev->modifiers & NEOMACS_META_MASK) emacs_modifiers |= meta_modifier;
                    tab_bar_arg = handle_tab_bar_click
                      (f, ev->x, ev->y,
                       ev->kind == NEOMACS_EVENT_MOUSE_PRESS, emacs_modifiers);
                  }
              }

            inev.ie.kind = MOUSE_CLICK_EVENT;
            inev.ie.code = ev->button - 1;  /* Emacs buttons are 0-indexed */
            inev.ie.modifiers = (ev->kind == NEOMACS_EVENT_MOUSE_PRESS) ? down_modifier : up_modifier;
            if (ev->modifiers & NEOMACS_SHIFT_MASK) inev.ie.modifiers |= shift_modifier;
            if (ev->modifiers & NEOMACS_CTRL_MASK) inev.ie.modifiers |= ctrl_modifier;
            if (ev->modifiers & NEOMACS_META_MASK) inev.ie.modifiers |= meta_modifier;
            XSETINT (inev.ie.x, ev->x);
            XSETINT (inev.ie.y, ev->y);
            if (!NILP (tab_bar_arg))
              inev.ie.arg = tab_bar_arg;
            XSETFRAME (inev.ie.frame_or_window, f);
            neomacs_evq_enqueue (&inev);
          }
          break;

        case NEOMACS_EVENT_SCROLL:
          {
            /* Check if scrolling over a webkit view */
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (dpyinfo && dpyinfo->display_handle)
              {
                uint32_t webkit_id = 0;
                int rel_x = 0, rel_y = 0;
                if (neomacs_display_webkit_at_position (dpyinfo->display_handle,
                                                         ev->x, ev->y,
                                                         &webkit_id, &rel_x, &rel_y))
                  {
                    /* Forward scroll to webkit view.
                       Scale deltas: winit LineDelta is ~1.0 per notch,
                       WPE expects pixel-like values. */
                    int sdx = (int)(ev->scrollDeltaX * 53);
                    int sdy = (int)(ev->scrollDeltaY * 53);
                    neomacs_display_webkit_send_scroll (dpyinfo->display_handle,
                                                        webkit_id, rel_x, rel_y,
                                                        sdx, sdy);
                    break;
                  }
              }

            float dx = ev->scrollDeltaX;
            float dy = ev->scrollDeltaY;
            float abs_dx = dx < 0 ? -dx : dx;
            float abs_dy = dy < 0 ? -dy : dy;

            /* Determine primary axis and generate appropriate event */
            if (abs_dy >= abs_dx)
              {
                if (dy != 0.0f)
                  {
                    inev.ie.kind = WHEEL_EVENT;
                    /* Positive deltaY = scroll up in winit */
                    inev.ie.modifiers |= (dy > 0) ? up_modifier : down_modifier;
                    inev.ie.arg = list3 (Qnil,
                                         make_float ((double) -dx * 100),
                                         make_float ((double) -dy * 100));
                    if (ev->modifiers & NEOMACS_SHIFT_MASK) inev.ie.modifiers |= shift_modifier;
                    if (ev->modifiers & NEOMACS_CTRL_MASK) inev.ie.modifiers |= ctrl_modifier;
                    if (ev->modifiers & NEOMACS_META_MASK) inev.ie.modifiers |= meta_modifier;
                    XSETINT (inev.ie.x, ev->x);
                    XSETINT (inev.ie.y, ev->y);
                    XSETFRAME (inev.ie.frame_or_window, f);
                    neomacs_evq_enqueue (&inev);
                  }
              }
            else
              {
                if (dx != 0.0f)
                  {
                    inev.ie.kind = HORIZ_WHEEL_EVENT;
                    inev.ie.modifiers |= (dx > 0) ? up_modifier : down_modifier;
                    inev.ie.arg = list3 (Qnil,
                                         make_float ((double) -dx * 100),
                                         make_float ((double) -dy * 100));
                    if (ev->modifiers & NEOMACS_SHIFT_MASK) inev.ie.modifiers |= shift_modifier;
                    if (ev->modifiers & NEOMACS_CTRL_MASK) inev.ie.modifiers |= ctrl_modifier;
                    if (ev->modifiers & NEOMACS_META_MASK) inev.ie.modifiers |= meta_modifier;
                    XSETINT (inev.ie.x, ev->x);
                    XSETINT (inev.ie.y, ev->y);
                    XSETFRAME (inev.ie.frame_or_window, f);
                    neomacs_evq_enqueue (&inev);
                  }
              }
          }
          break;

        case NEOMACS_EVENT_RESIZE:
          {
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            int new_width = ev->width;
            int new_height = ev->height;

            /* Update dpyinfo dimensions */
            if (dpyinfo)
              {
                dpyinfo->width = new_width;
                dpyinfo->height = new_height;
              }

            /* Update the Rust display handle */
            if (dpyinfo && dpyinfo->display_handle)
              neomacs_display_resize (dpyinfo->display_handle, new_width, new_height);

            /* Update the Emacs frame size - queue for later since we're in event handler */
            if (FRAME_PIXEL_WIDTH (f) != new_width
                || FRAME_PIXEL_HEIGHT (f) != new_height)
              {
                change_frame_size (f, new_width, new_height, false, true, false);
                SET_FRAME_GARBAGED (f);
              }
          }
          break;

        case NEOMACS_EVENT_CLOSE_REQUEST:
          inev.ie.kind = DELETE_WINDOW_EVENT;
          XSETFRAME (inev.ie.frame_or_window, f);
          neomacs_evq_enqueue (&inev);
          break;

        case NEOMACS_EVENT_FOCUS_IN:
          {
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (dpyinfo)
              {
                dpyinfo->focus_frame = f;
                dpyinfo->x_focus_frame = f;
                dpyinfo->highlight_frame = f;
                dpyinfo->x_highlight_frame = f;
              }
            inev.ie.kind = FOCUS_IN_EVENT;
            XSETFRAME (inev.ie.frame_or_window, f);
            neomacs_evq_enqueue (&inev);
          }
          break;

        case NEOMACS_EVENT_FOCUS_OUT:
          {
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (dpyinfo && dpyinfo->highlight_frame == f)
              {
                dpyinfo->highlight_frame = NULL;
                dpyinfo->x_highlight_frame = NULL;
              }
            inev.ie.kind = FOCUS_OUT_EVENT;
            XSETFRAME (inev.ie.frame_or_window, f);
            neomacs_evq_enqueue (&inev);
          }
          break;

        case NEOMACS_EVENT_IMAGE_DIMENSIONS_READY:
          /* Image dimensions are now available in the shared map.
             Trigger a redisplay so Emacs can pick them up. */
          {
            /* Mark frame for redisplay */
            SET_FRAME_GARBAGED (f);
            /* Force redisplay to pick up new image dimensions */
            windows_or_buffers_changed = 1;
          }
          break;

        case NEOMACS_EVENT_TERMINAL_EXITED:
          {
            Lisp_Object handler = intern ("neo-term--handle-exit");
            if (!NILP (Ffboundp (handler)))
              safe_calln (Fsymbol_function (handler), make_fixnum (ev->keysym));
          }
          break;

        default:
          break;
        }
    }
}

/* Initialize display in threaded mode */
int
neomacs_display_init_threaded_mode (int width, int height, const char *title)
{
  int fd = neomacs_display_init_threaded (width, height, title);
  if (fd < 0)
    return -1;

  wakeup_fd = fd;
  threaded_mode_active = 1;

  /* Add wakeup_fd to Emacs's file descriptor set */
  add_read_fd (wakeup_fd, neomacs_display_wakeup_handler, NULL);

  return 0;
}

/* Check if threaded mode is active */
int
neomacs_display_is_threaded (void)
{
  return threaded_mode_active;
}

/* Shutdown threaded mode */
void
neomacs_display_shutdown_threaded_mode (void)
{
  if (!threaded_mode_active)
    return;

  /* Remove wakeup_fd from Emacs's fd set */
  if (wakeup_fd >= 0)
    delete_read_fd (wakeup_fd);

  neomacs_display_shutdown_threaded ();

  wakeup_fd = -1;
  threaded_mode_active = 0;
}


/* ============================================================================
 * Initialization
 * ============================================================================ */

void
syms_of_neomacsterm (void)
{
  /* Redisplay interface is now statically initialized */

  defsubr (&Sneomacs_available_p);
  defsubr (&Sneomacs_display_list);
  defsubr (&Sx_hide_tip);
  defsubr (&Sxw_display_color_p);
  defsubr (&Sx_display_grayscale_p);

  /* Video playback API */
  defsubr (&Sneomacs_video_load);
  defsubr (&Sneomacs_video_play);
  defsubr (&Sneomacs_video_pause);
  defsubr (&Sneomacs_video_stop);
  defsubr (&Sneomacs_video_set_loop);
  defsubr (&Sneomacs_video_update);
  defsubr (&Sneomacs_video_floating);
  defsubr (&Sneomacs_video_floating_clear);

  /* Image functions */
  defsubr (&Sneomacs_image_load);
  defsubr (&Sneomacs_image_size);
  defsubr (&Sneomacs_image_free);
  defsubr (&Sneomacs_image_floating);
  defsubr (&Sneomacs_image_floating_clear);
  defsubr (&Sneomacs_insert_image);
  defsubr (&Sneomacs_insert_image_data);

  /* WebKit browser functions */
  defsubr (&Sneomacs_webkit_init);
  defsubr (&Sneomacs_webkit_create);
  defsubr (&Sneomacs_webkit_destroy);
  defsubr (&Sneomacs_webkit_load_uri);
  defsubr (&Sneomacs_webkit_go_back);
  defsubr (&Sneomacs_webkit_go_forward);
  defsubr (&Sneomacs_webkit_reload);
  defsubr (&Sneomacs_webkit_resize);
  defsubr (&Sneomacs_webkit_execute_js);
  defsubr (&Sneomacs_webkit_floating);
  defsubr (&Sneomacs_webkit_floating_clear);
  defsubr (&Sneomacs_webkit_send_key);
  defsubr (&Sneomacs_webkit_send_pointer);
  defsubr (&Sneomacs_webkit_send_scroll);
  defsubr (&Sneomacs_webkit_click);
  defsubr (&Sneomacs_webkit_get_title);
  defsubr (&Sneomacs_webkit_get_url);
  defsubr (&Sneomacs_webkit_get_progress);
  defsubr (&Sneomacs_webkit_loading_p);
  defsubr (&Sneomacs_webkit_update);
  defsubr (&Sneomacs_webkit_update_all);
  defsubr (&Sneomacs_webkit_set_new_window_function);
  defsubr (&Sneomacs_webkit_set_load_callback);
  defsubr (&Sneomacs_insert_webkit);

  /* Animation API */
  defsubr (&Sneomacs_set_animation_option);
  defsubr (&Sneomacs_get_animation_option);
  defsubr (&Sneomacs_start_buffer_transition);
  defsubr (&Sneomacs_animation_active_p);
  defsubr (&Sneomacs_prepare_buffer_transition);
  defsubr (&Sneomacs_trigger_buffer_transition);
  defsubr (&Sneomacs_has_transition_snapshot_p);

  /* Cursor blink */
  defsubr (&Sneomacs_set_cursor_blink);
  defsubr (&Sneomacs_set_cursor_animation);
  defsubr (&Sneomacs_set_animation_config);

  /* Terminal emulator (neo-term) */
  defsubr (&Sneomacs_terminal_create);
  defsubr (&Sneomacs_terminal_write);
  defsubr (&Sneomacs_terminal_resize);
  defsubr (&Sneomacs_terminal_destroy);
  defsubr (&Sneomacs_terminal_set_float);
  defsubr (&Sneomacs_terminal_get_text);

  DEFSYM (Qneomacs, "neomacs");
  /* Qvideo and Qwebkit are defined in xdisp.c for use in VIDEOP/WEBKITP */
  DEFSYM (QCid, ":id");

  /* Cursor animation style symbols */
  DEFSYM (Qexponential, "exponential");
  DEFSYM (Qspring, "spring");
  DEFSYM (Qease_out_quad, "ease-out-quad");
  DEFSYM (Qease_out_cubic, "ease-out-cubic");
  DEFSYM (Qease_out_expo, "ease-out-expo");
  DEFSYM (Qease_in_out_cubic, "ease-in-out-cubic");
  DEFSYM (Qlinear, "linear");

  /* Scroll effect symbols */
  DEFSYM (Qslide, "slide");
  DEFSYM (Qcrossfade, "crossfade");
  DEFSYM (Qscale_zoom, "scale-zoom");
  DEFSYM (Qfade_edges, "fade-edges");
  DEFSYM (Qcascade, "cascade");
  DEFSYM (Qparallax, "parallax");
  DEFSYM (Qtilt, "tilt");
  DEFSYM (Qpage_curl, "page-curl");
  DEFSYM (Qcard_flip, "card-flip");
  DEFSYM (Qcylinder_roll, "cylinder-roll");
  DEFSYM (Qwobbly, "wobbly");
  DEFSYM (Qwave, "wave");
  DEFSYM (Qper_line_spring, "per-line-spring");
  DEFSYM (Qliquid, "liquid");
  DEFSYM (Qmotion_blur, "motion-blur");
  DEFSYM (Qchromatic_aberration, "chromatic-aberration");
  DEFSYM (Qghost_trails, "ghost-trails");
  DEFSYM (Qcolor_temperature, "color-temperature");
  DEFSYM (Qcrt_scanlines, "crt-scanlines");
  DEFSYM (Qdepth_of_field, "depth-of-field");
  DEFSYM (Qtypewriter_reveal, "typewriter-reveal");

  /* WebKit new window callback */
  DEFVAR_LISP ("neomacs-webkit-new-window-function", Vneomacs_webkit_new_window_function,
    doc: /* Function called when WebKit requests a new window.
The function is called with three arguments: VIEW-ID, URL, and FRAME-NAME.
If it returns non-nil, Emacs handles the request (e.g., opens URL in new buffer).
If nil, the request is ignored. */);
  Vneomacs_webkit_new_window_function = Qnil;

  /* WebKit page load callback */
  DEFVAR_LISP ("neomacs-webkit-load-callback", Vneomacs_webkit_load_callback,
    doc: /* Function called when WebKit page load events occur.
The function is called with three arguments: VIEW-ID, EVENT, URI.
EVENT is one of: started, redirected, committed, finished, failed. */);
  Vneomacs_webkit_load_callback = Qnil;

  /* WebKit clicked view ID — set when a mouse click lands on a webkit view */
  DEFVAR_LISP ("neomacs-webkit-clicked-view-id", Vneomacs_webkit_clicked_view_id,
    doc: /* View ID of the last WebKit view that was clicked.
Set automatically when a mouse click lands on an inline or floating
WebKit view.  Used by `neomacs-webkit-interaction-mode' to auto-enter
keyboard input forwarding.  Set to nil to clear. */);
  Vneomacs_webkit_clicked_view_id = Qnil;

  /* Required variables for cus-start */
  DEFVAR_BOOL ("x-use-underline-position-properties",
	       x_use_underline_position_properties,
     doc: /* SKIP: real doc in xterm.c.  */);
  x_use_underline_position_properties = 1;

  DEFVAR_BOOL ("x-underline-at-descent-line",
	       x_underline_at_descent_line,
     doc: /* SKIP: real doc in xterm.c.  */);
  x_underline_at_descent_line = 0;

  DEFVAR_LISP ("x-toolkit-scroll-bars", Vx_toolkit_scroll_bars,
     doc: /* SKIP: real doc in xterm.c.  */);
  Vx_toolkit_scroll_bars = intern_c_string ("gtk");

  DEFVAR_LISP ("x-ctrl-keysym", Vx_ctrl_keysym,
	       doc: /* SKIP: real doc in xterm.c.  */);
  Vx_ctrl_keysym = Qnil;
  DEFVAR_LISP ("x-alt-keysym", Vx_alt_keysym,
	       doc: /* SKIP: real doc in xterm.c.  */);
  Vx_alt_keysym = Qnil;
  DEFVAR_LISP ("x-hyper-keysym", Vx_hyper_keysym,
	       doc: /* SKIP: real doc in xterm.c.  */);
  Vx_hyper_keysym = Qnil;
  DEFVAR_LISP ("x-meta-keysym", Vx_meta_keysym,
	       doc: /* SKIP: real doc in xterm.c.  */);
  Vx_meta_keysym = Qnil;
  DEFVAR_LISP ("x-super-keysym", Vx_super_keysym,
	       doc: /* SKIP: real doc in xterm.c.  */);
  Vx_super_keysym = Qnil;

  /* Tell Emacs about this window system */
  Fprovide (Qneomacs, Qnil);
}

#endif /* HAVE_NEOMACS */
