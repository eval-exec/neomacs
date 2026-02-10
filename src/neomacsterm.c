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

#define NLOG_MODULE "display"
#include "neomacs_log.h"

#include <dlfcn.h>
#include <string.h>
#include <stdint.h>
#include <signal.h>
#include <xkbcommon/xkbcommon.h>

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
#include "intervals.h"  /* For TEXT_PROP_MEANS_INVISIBLE */
#include "process.h"  /* For add_read_fd, delete_read_fd */
#include "termopts.h"  /* For interrupt_input */
#include "menu.h"  /* For MENU_KEYMAPS, MENU_FOR_CLICK, menu_items macros */

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

/* Track popup/menu activation for tooltip/auto-select suppression */
static int neomacs_popup_activated_flag;

/* Forward declarations */
static void neomacs_extract_full_frame (struct frame *f);
static void neomacs_define_frame_cursor (struct frame *f, Emacs_Cursor cursor);
static void neomacs_show_hourglass (struct frame *f);
static void neomacs_hide_hourglass (struct frame *f);
static void neomacs_compute_glyph_string_overhangs (struct glyph_string *s);
static void neomacs_clear_under_internal_border (struct frame *f);
static void neomacs_display_wakeup_handler (int fd, void *data);

/* Rust layout engine FFI entry point (defined in layout/engine.rs via ffi.rs) */
extern void neomacs_rust_layout_frame (void *display_handle, void *frame_ptr,
                                       float width, float height,
                                       float char_width, float char_height,
                                       float font_pixel_size,
                                       uint32_t background,
                                       uint32_t vertical_border_fg,
                                       int right_divider_width,
                                       int bottom_divider_width,
                                       uint32_t divider_fg,
                                       uint32_t divider_first_fg,
                                       uint32_t divider_last_fg);

static void neomacs_set_window_size (struct frame *f, bool change_gravity,
                                     int width, int height);
static void neomacs_set_vertical_scroll_bar (struct window *w, int portion,
                                             int whole, int position);
static void neomacs_set_horizontal_scroll_bar (struct window *w, int portion,
                                               int whole, int position);
static void neomacs_condemn_scroll_bars (struct frame *frame);
static void neomacs_redeem_scroll_bar (struct window *w);
static void neomacs_judge_scroll_bars (struct frame *f);
static void neomacs_set_scroll_bar_default_width (struct frame *f);
static void neomacs_set_scroll_bar_default_height (struct frame *f);
static void neomacs_clear_frame (struct frame *f);
static void neomacs_mouse_position (struct frame **fp, int insist,
                                    Lisp_Object *bar_window,
                                    enum scroll_bar_part *part,
                                    Lisp_Object *x, Lisp_Object *y,
                                    Time *timestamp);
static void neomacs_ring_bell (struct frame *f);
static void neomacs_toggle_invisible_pointer (struct frame *f, bool invisible);
static void neomacs_frame_up_to_date (struct frame *f);
static void neomacs_frame_rehighlight (struct neomacs_display_info *dpyinfo);
static void neomacs_frame_rehighlight_hook (struct frame *f);
static void neomacs_frame_raise_lower (struct frame *f, bool raise_flag);
static void neomacs_fullscreen_hook (struct frame *f);
static void neomacs_iconify_frame (struct frame *f);
static void neomacs_implicit_set_name (struct frame *f, Lisp_Object arg,
                                       Lisp_Object oldval);
static void neomacs_set_frame_offset (struct frame *f, int xoff, int yoff,
                                      int change_gravity);
static void neomacs_delete_frame (struct frame *f);
static void neomacs_focus_frame (struct frame *f, bool noactivate);
static Lisp_Object neomacs_get_focus_frame (struct frame *frame);
static void neomacs_buffer_flipping_unblocked (struct frame *f);
static void neomacs_set_frame_alpha (struct frame *f);
static bool neomacs_bitmap_icon (struct frame *f, Lisp_Object file);
static uint32_t neomacs_get_or_load_image (struct neomacs_display_info *dpyinfo,
                                           struct image *img);

/* Event queue for buffering input events from render thread callbacks */
struct neomacs_event_queue_t
{
  union buffered_input_event *q;
  int nr, cap;
};

static struct neomacs_event_queue_t neomacs_event_q;

/* Enqueue an event from render thread callback to be processed by read_socket */
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

  /* With interrupt_input = false (polling mode), we do NOT raise(SIGIO).
     The wakeup pipe write from the render thread is sufficient to wake
     pselect() in xg_select.  raise(SIGIO) would set pending_signals,
     causing an infinite busy loop: pending_signals → process_pending_signals
     → handle_async_input → gobble_input → read_socket → wakeup_handler
     → neomacs_evq_enqueue → raise(SIGIO) → pending_signals ...
     Events are flushed to kbd_buffer by gobble_input() after
     wait_reading_process_output returns, or by the fd callback.  */
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

  if (n > 0)
    nlog_debug ("neomacs_evq_flush: flushed %d events to kbd_buffer", n);
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
static void neomacs_draw_window_divider (struct window *, int, int, int, int);
static void neomacs_define_fringe_bitmap (int, unsigned short *, int, int);
static void neomacs_destroy_fringe_bitmap (int);

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
  .define_fringe_bitmap = neomacs_define_fringe_bitmap,
  .destroy_fringe_bitmap = neomacs_destroy_fringe_bitmap,
  .compute_glyph_string_overhangs = neomacs_compute_glyph_string_overhangs,
  .draw_glyph_string = neomacs_draw_glyph_string,
  .define_frame_cursor = neomacs_define_frame_cursor,
  .clear_frame_area = neomacs_clear_frame_area,
  .clear_under_internal_border = neomacs_clear_under_internal_border,
  .draw_window_cursor = neomacs_draw_window_cursor,
  .draw_vertical_window_border = neomacs_draw_vertical_window_border,
  .draw_window_divider = neomacs_draw_window_divider,
  .shift_glyphs_for_insert = NULL,
  .show_hourglass = neomacs_show_hourglass,
  .hide_hourglass = neomacs_hide_hourglass,
  .default_font_parameter = neomacs_default_font_parameter,
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

/* Initialize display info defaults */
static void
neomacs_initialize_display_info (struct neomacs_display_info *dpyinfo)
{
  dpyinfo->reference_count = 0;
  dpyinfo->n_planes = 24;
  dpyinfo->black_pixel = 0x000000;
  dpyinfo->white_pixel = 0xffffff;
  dpyinfo->background_pixel = 0x000000;  /* Default to black for dark theme */
  dpyinfo->smallest_char_width = 8;
  dpyinfo->smallest_font_height = 16;
  dpyinfo->supports_argb = true;
  dpyinfo->connection = -1;

  /* Initialize DPI to a reasonable default - required for font sizing */
  dpyinfo->resx = 96;
  dpyinfo->resy = 96;

  /* Default display dimensions (overridden after render thread init) */
  dpyinfo->width = 1920;
  dpyinfo->height = 1080;
}

/* Query monitor info from winit (via Rust FFI) and update dpyinfo. */
static void
neomacs_query_monitors (struct neomacs_display_info *dpyinfo)
{
  int n = neomacs_display_wait_for_monitors ();
  if (n <= 0)
    {
      nlog_debug ("No monitors found, using defaults");
      return;
    }

  int max_width = 0, max_height = 0;
  double max_scale = 1.0;
  for (int i = 0; i < n; i++)
    {
      struct NeomacsMonitorInfo info;
      if (neomacs_display_get_monitor_info (i, &info))
        {
          /* Monitor dimensions from winit are in physical pixels.
             Convert to logical pixels using the monitor's scale factor
             so that Emacs frame dimensions match the renderer's logical
             coordinate space. */
          double scale = info.scale > 0.0 ? info.scale : 1.0;
          int logical_x = (int) (info.x / scale);
          int logical_y = (int) (info.y / scale);
          int logical_w = (int) (info.width / scale);
          int logical_h = (int) (info.height / scale);
          int right = logical_x + logical_w;
          int bottom = logical_y + logical_h;
          if (right > max_width)
            max_width = right;
          if (bottom > max_height)
            max_height = bottom;
          if (scale > max_scale)
            max_scale = scale;
        }
    }
  if (max_width > 0 && max_height > 0)
    {
      dpyinfo->width = max_width;
      dpyinfo->height = max_height;
      nlog_debug ("Display size from winit (logical): %dx%d (scale=%.1f)",
                  dpyinfo->width, dpyinfo->height, max_scale);
    }
}

/* Create a new Neomacs display connection */
struct neomacs_display_info *
neomacs_open_display (const char *display_name)
{
  struct neomacs_display_info *dpyinfo;

  dpyinfo = xzalloc (sizeof *dpyinfo);
  neomacs_initialize_display_info (dpyinfo);

  /* Initialize the Rust display engine in threaded mode.
     This spawns the render thread with winit event loop.  */
  int wakeup_fd = neomacs_display_init_threaded (dpyinfo->width, dpyinfo->height, "Emacs");

  if (wakeup_fd < 0)
    {
      xfree (dpyinfo);
      error ("Failed to initialize Neomacs threaded display engine");
    }

  /* Query actual monitor dimensions from winit */
  neomacs_query_monitors (dpyinfo);

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

  /* Update render thread with actual display dimensions if they changed */
  if (dpyinfo->display_handle)
    neomacs_display_resize (dpyinfo->display_handle, dpyinfo->width, dpyinfo->height);

  /* Add to display list */
  dpyinfo->next = neomacs_display_list;
  neomacs_display_list = dpyinfo;

  return dpyinfo;
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

  /* Unregister the connection fd from the event loop.  */
  if (dpyinfo->connection >= 0)
    {
      delete_read_fd (dpyinfo->connection);
      delete_keyboard_wait_descriptor (dpyinfo->connection);
    }

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

/* Popup menu implementation using GPU-rendered overlay in the render thread.
   Parses Emacs's menu_items array, sends items to Rust render thread,
   and blocks until the user makes a selection or cancels. */
static Lisp_Object
neomacs_menu_show (struct frame *f, int x, int y, int menuflags,
                   Lisp_Object title, const char **error_name)
{
  int i;
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  *error_name = NULL;

  if (menu_items_used <= MENU_ITEMS_PANE_LENGTH)
    {
      *error_name = "Empty menu";
      return Qnil;
    }

  if (!dpyinfo || !dpyinfo->display_handle)
    {
      *error_name = "No display connection";
      return Qnil;
    }

  /* Count items, tracking submenu depth to include all levels */
  int item_count = 0;
  int depth = 0;
  i = 0;
  while (i < menu_items_used)
    {
      if (NILP (AREF (menu_items, i)))
        { depth++; i++; continue; } /* submenu push */
      else if (EQ (AREF (menu_items, i), Qlambda))
        { if (depth > 0) depth--; i++; continue; } /* submenu pop */
      else if (EQ (AREF (menu_items, i), Qquote))
        { i += 1; continue; } /* nil item (dialog only) */
      else if (EQ (AREF (menu_items, i), Qt))
        {
          /* Pane header — count as separator if not first pane at depth 0 */
          if (depth == 0 && item_count > 0)
            item_count++; /* separator between panes */
          i += MENU_ITEMS_PANE_LENGTH;
        }
      else
        {
          item_count++;
          i += MENU_ITEMS_ITEM_LENGTH;
        }
    }

  if (item_count == 0)
    {
      *error_name = "Empty menu";
      return Qnil;
    }

  /* Allocate C menu items array */
  struct CPopupMenuItem *c_items = xmalloc (item_count * sizeof *c_items);
  memset (c_items, 0, item_count * sizeof *c_items);

  /* Map from c_items index -> menu_items index (for finding the selection) */
  int *item_indices = xmalloc (item_count * sizeof *item_indices);
  int ci = 0;
  bool first_pane = true;
  depth = 0;

  i = 0;
  while (i < menu_items_used)
    {
      if (NILP (AREF (menu_items, i)))
        { depth++; i++; continue; } /* submenu push */
      else if (EQ (AREF (menu_items, i), Qlambda))
        { if (depth > 0) depth--; i++; continue; } /* submenu pop */
      else if (EQ (AREF (menu_items, i), Qquote))
        { i += 1; continue; }
      else if (EQ (AREF (menu_items, i), Qt))
        {
          /* Pane header — insert separator between panes at top level */
          if (depth == 0 && !first_pane && ci < item_count)
            {
              c_items[ci].label = "";
              c_items[ci].shortcut = "";
              c_items[ci].enabled = 0;
              c_items[ci].separator = 1;
              c_items[ci].submenu = 0;
              c_items[ci].depth = 0;
              item_indices[ci] = -1;
              ci++;
            }
          if (depth == 0)
            first_pane = false;
          i += MENU_ITEMS_PANE_LENGTH;
        }
      else
        {
          /* Regular menu item */
          Lisp_Object item_name = AREF (menu_items, i + MENU_ITEMS_ITEM_NAME);
          Lisp_Object enable = AREF (menu_items, i + MENU_ITEMS_ITEM_ENABLE);
          Lisp_Object descrip = AREF (menu_items, i + MENU_ITEMS_ITEM_EQUIV_KEY);
          Lisp_Object def = AREF (menu_items, i + MENU_ITEMS_ITEM_DEFINITION);

          /* Detect if next element is a submenu push marker */
          int next_i = i + MENU_ITEMS_ITEM_LENGTH;
          int has_submenu = (next_i < menu_items_used
                             && NILP (AREF (menu_items, next_i))) ? 1 : 0;

          if (ci < item_count)
            {
              c_items[ci].label = STRINGP (item_name) ? SSDATA (item_name) : "";
              c_items[ci].shortcut = STRINGP (descrip) ? SSDATA (descrip) : "";
              c_items[ci].enabled = !NILP (enable) && !NILP (def) ? 1 : 0;
              c_items[ci].separator = (STRINGP (item_name)
                                       && !strcmp (SSDATA (item_name), "--")) ? 1 : 0;
              c_items[ci].submenu = has_submenu;
              c_items[ci].depth = depth;
              item_indices[ci] = i;
              ci++;
            }
          i += MENU_ITEMS_ITEM_LENGTH;
        }
    }
  item_count = ci; /* Actual count after building */

  /* Send menu to render thread */
  const char *title_str = NULL;
  if (!NILP (title) && STRINGP (title))
    title_str = SSDATA (title);

  /* Resolve menu face colors for themed popup rendering */
  uint32_t menu_fg = 0, menu_bg = 0;
  {
    int face_id = lookup_named_face (NULL, f, Qmenu, false);
    if (face_id >= 0)
      {
        struct face *mface = FACE_FROM_ID (f, face_id);
        if (mface)
          {
            unsigned long fg = mface->foreground;
            unsigned long bg = mface->background;
            menu_fg = ((RED_FROM_ULONG (fg) << 16)
                       | (GREEN_FROM_ULONG (fg) << 8)
                       | BLUE_FROM_ULONG (fg));
            menu_bg = ((RED_FROM_ULONG (bg) << 16)
                       | (GREEN_FROM_ULONG (bg) << 8)
                       | BLUE_FROM_ULONG (bg));
          }
      }
  }

  neomacs_popup_activated_flag = 1;
  neomacs_display_show_popup_menu (dpyinfo->display_handle,
                                   x, y, c_items, item_count,
                                   title_str, menu_fg, menu_bg);

  /* Block waiting for menu selection event from render thread.
     We poll the input event queue until we get a MenuSelection event.
     Give up after 30 seconds to prevent indefinite hang. */
  int selection = -2; /* -2 = still waiting, -1 = cancelled, >= 0 = item index */
  NeomacsInputEvent events[16];
  int max_iterations = 600; /* 600 * 50ms = 30 seconds */

  while (selection == -2 && max_iterations-- > 0)
    {
      /* Wait for events (use a short timeout to stay responsive) */
      struct timespec timeout = { 0, 50000000 }; /* 50ms */
      fd_set readfds;
      FD_ZERO (&readfds);
      FD_SET (dpyinfo->connection, &readfds);
      pselect (dpyinfo->connection + 1, &readfds,
               NULL, NULL, &timeout, NULL);

      int n = neomacs_display_drain_input (events, 16);
      for (int j = 0; j < n; j++)
        {
          if (events[j].kind == NEOMACS_EVENT_MENU_SELECTION)
            {
              selection = events[j].x; /* x field contains the selection index */
            }
          else if (events[j].kind == NEOMACS_EVENT_RESIZE)
            {
              /* Handle resize during menu display */
              int new_width = events[j].width;
              int new_height = events[j].height;
              if (new_width > 0 && new_height > 0)
                {
                  change_frame_size (f, new_width, new_height, false, true, false);
                }
            }
          /* Discard other events while menu is active */
        }
    }

  /* Menu selection done — clear popup flag */
  neomacs_popup_activated_flag = 0;

  /* Clean up */
  Lisp_Object result = Qnil;

  if (selection >= 0 && selection < item_count
      && item_indices[selection] >= 0)
    {
      int mi = item_indices[selection];
      Lisp_Object entry = AREF (menu_items, mi + MENU_ITEMS_ITEM_VALUE);

      if (menuflags & MENU_KEYMAPS)
        {
          /* Return (prefix . entry) for keymaps */
          Lisp_Object prefix = Qnil;
          /* Find the pane prefix for this item */
          int k = 0;
          while (k < mi)
            {
              if (EQ (AREF (menu_items, k), Qt))
                {
                  prefix = AREF (menu_items, k + MENU_ITEMS_PANE_PREFIX);
                  k += MENU_ITEMS_PANE_LENGTH;
                }
              else if (NILP (AREF (menu_items, k))
                       || EQ (AREF (menu_items, k), Qlambda)
                       || EQ (AREF (menu_items, k), Qquote))
                k++;
              else
                k += MENU_ITEMS_ITEM_LENGTH;
            }
          entry = list1 (entry);
          if (!NILP (prefix))
            entry = Fcons (prefix, entry);
        }
      result = entry;
    }
  xfree (c_items);
  xfree (item_indices);

  if (selection == -1 && !(menuflags & MENU_FOR_CLICK))
    {
      /* User cancelled — equivalent to C-g for non-click menus */
      quit ();
    }

  return result;
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
  terminal->set_scroll_bar_default_width_hook = neomacs_set_scroll_bar_default_width;
  terminal->set_scroll_bar_default_height_hook = neomacs_set_scroll_bar_default_height;
  terminal->mouse_position_hook = neomacs_mouse_position;

  /* Frame management hooks */
  terminal->ring_bell_hook = neomacs_ring_bell;
  terminal->toggle_invisible_pointer_hook = neomacs_toggle_invisible_pointer;
  terminal->frame_up_to_date_hook = neomacs_frame_up_to_date;
  terminal->frame_rehighlight_hook = neomacs_frame_rehighlight_hook;
  terminal->frame_raise_lower_hook = neomacs_frame_raise_lower;
  terminal->fullscreen_hook = neomacs_fullscreen_hook;
  terminal->iconify_frame_hook = neomacs_iconify_frame;
  terminal->implicit_set_name_hook = neomacs_implicit_set_name;
  terminal->set_frame_offset_hook = neomacs_set_frame_offset;
  terminal->delete_frame_hook = neomacs_delete_frame;
  terminal->focus_frame_hook = neomacs_focus_frame;
  terminal->get_focus_frame = neomacs_get_focus_frame;
  terminal->buffer_flipping_unblocked_hook = neomacs_buffer_flipping_unblocked;
  terminal->set_frame_alpha_hook = neomacs_set_frame_alpha;
  terminal->set_bitmap_icon_hook = neomacs_bitmap_icon;

  /* Register the display connection fd for event handling */
  if (dpyinfo->connection >= 0)
    {
      add_keyboard_wait_descriptor (dpyinfo->connection);
      nlog_debug ("Registered fd %d with add_keyboard_wait_descriptor",
		  dpyinfo->connection);
    }
  else
    nlog_warn ("No valid connection fd to register");

  /* Neomacs uses a wakeup pipe + fd callback for event delivery,
     not SIGIO on the display connection.  Set interrupt_input = false
     so that gobble_input() is called after wait_reading_process_output
     returns, ensuring read_socket_hook (neomacs_read_socket) runs and
     flushes events from neomacs_evq to kbd_buffer.  */
  interrupt_input = false;

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
  if (!font_family)
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
  int font_ascent = 0;
  int font_descent = 0;
  int ul_position = 1;
  int ul_thickness = 1;
  if (face->font)
    {
      font_size = face->font->pixel_size;
      font_ascent = FONT_BASE (face->font);
      font_descent = FONT_DESCENT (face->font);
      if (face->font->underline_position > 0)
        ul_position = face->font->underline_position;
      if (face->font->underline_thickness > 0)
        ul_thickness = face->font->underline_thickness;
    }

  neomacs_display_set_face (handle, face->id,
                            fg_rgb, bg_rgb, font_family,
                            font_weight, is_italic, font_size,
                            underline_style, underline_color,
                            box_type, box_color, box_line_width,
                            box_corner_radius,
                            strike_through, strike_through_color,
                            overline, overline_color,
                            font_ascent, font_descent,
                            ul_position, ul_thickness);
}

/* Callback for foreach_window: extract all visible glyphs from a window's
   current_matrix and send them to the Rust display engine via FFI. */
static bool
neomacs_extract_window_glyphs (struct window *w, void *user_data)
{
  struct frame *f = XFRAME (w->frame);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (!dpyinfo)
    return true;
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
    ptrdiff_t win_end = 0;
    ptrdiff_t buf_size = 0;
    if (BUFFERP (w->contents))
      {
        struct buffer *buf = XBUFFER (w->contents);
        buf_id = (uintptr_t) buf;
        if (MARKERP (w->start))
          win_start = marker_position (w->start);
        win_end = w->window_end_pos;
        buf_size = BUF_Z (buf);
      }
    int is_mini = MINI_WINDOW_P (w) ? 1 : 0;
    /* Get window-specific char height (respects text-scale-adjust) */
    float w_char_height = (float) FRAME_LINE_HEIGHT (f);
    {
      int def_face_id = lookup_basic_face (w, f, DEFAULT_FACE_ID);
      struct face *wface = FACE_FROM_ID_OR_NULL (f, def_face_id);
      if (wface && wface->font)
        {
          int asc, desc;
          get_font_ascent_descent (wface->font, &asc, &desc);
          w_char_height = (float) (asc + desc);
        }
    }
    const char *buf_fname = NULL;
    int buf_modified = 0;
    if (BUFFERP (w->contents))
      {
        struct buffer *b = XBUFFER (w->contents);
        Lisp_Object fn = BVAR (b, filename);
        if (STRINGP (fn))
          buf_fname = SSDATA (fn);
        buf_modified = (BUF_SAVE_MODIFF (b) < BUF_MODIFF (b)) ? 1 : 0;
      }
    neomacs_display_add_window_info (
        handle,
        (int64_t)(intptr_t) w,
        (uint64_t) buf_id,
        (int64_t) win_start,
        (int64_t) win_end,
        (int64_t) buf_size,
        (float) win_x, (float) win_y,
        (float) win_w, (float) win_h,
        (float) WINDOW_MODE_LINE_HEIGHT (w),
        selected, is_mini, w_char_height,
        buf_fname, buf_modified);
  }

  /* Check mouse-face highlight for this window */
  Mouse_HLInfo *hlinfo = MOUSE_HL_INFO (f);
  bool has_mouse_face = false;
  int mf_beg_row = -1, mf_beg_col = -1;
  int mf_end_row = -1, mf_end_col = -1;
  int mf_face_id = 0;

  if (!NILP (hlinfo->mouse_face_window)
      && XWINDOW (hlinfo->mouse_face_window) == w
      && hlinfo->mouse_face_beg_row >= 0
      && !hlinfo->mouse_face_hidden)
    {
      has_mouse_face = true;
      mf_beg_row = hlinfo->mouse_face_beg_row;
      mf_beg_col = hlinfo->mouse_face_beg_col;
      mf_end_row = hlinfo->mouse_face_end_row;
      mf_end_col = hlinfo->mouse_face_end_col;
      mf_face_id = hlinfo->mouse_face_face_id;
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
          int text_col = 0;  /* Column index within TEXT_AREA */

          /* Calculate starting X for this area */
          if (area == TEXT_AREA)
            glyph_x = window_box_left (w, TEXT_AREA);
          else if (area == LEFT_MARGIN_AREA)
            glyph_x = window_box_left (w, LEFT_MARGIN_AREA);
          else
            glyph_x = window_box_left (w, RIGHT_MARGIN_AREA);

          while (glyph < end)
            {
              /* Check if this glyph is in the mouse-face highlight region.
                 Mouse-face beg/end col are relative to TEXT_AREA glyphs. */
              bool in_mouse_face = false;
              if (has_mouse_face && area == TEXT_AREA && !is_mode_line)
                {
                  if (row_idx > mf_beg_row && row_idx < mf_end_row)
                    in_mouse_face = true;
                  else if (row_idx == mf_beg_row && row_idx == mf_end_row)
                    in_mouse_face = (text_col >= mf_beg_col
                                     && text_col < mf_end_col);
                  else if (row_idx == mf_beg_row)
                    in_mouse_face = (text_col >= mf_beg_col);
                  else if (row_idx == mf_end_row)
                    in_mouse_face = (text_col < mf_end_col);
                }

              /* Determine effective face: mouse-face overrides normal */
              int effective_face_id = in_mouse_face
                ? mf_face_id : (int) glyph->face_id;
              struct face *face
                = FACE_FROM_ID_OR_NULL (f, effective_face_id);
              if (!face)
                face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);

              /* Send face if it changed */
              if (face && effective_face_id != last_face_id)
                {
                  neomacs_send_face (handle, f, face);
                  last_face_id = effective_face_id;
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
                    /* For automatic compositions (e.g. emoji, combining chars),
                       iterate through all sub-glyphs and apply positioning
                       offsets from the lgstring (grapheme cluster string). */
                    if (glyph->u.cmp.automatic)
                      {
                        int cmp_id = glyph->u.cmp.id;
                        int cmp_from = glyph->slice.cmp.from;
                        int cmp_to = glyph->slice.cmp.to;
                        Lisp_Object gstring = composition_gstring_from_id (cmp_id);

                        if (!NILP (gstring))
                          {
                            int total_width = glyph->pixel_width;
                            int width_used = 0;

                            for (int gi = cmp_from; gi < cmp_to; gi++)
                              {
                                Lisp_Object glyph_obj = LGSTRING_GLYPH (gstring, gi);
                                if (NILP (glyph_obj))
                                  break;

                                unsigned int charcode = LGLYPH_CHAR (glyph_obj);

                                /* Resolve correct font face for this character
                                   (e.g. emoji font via fontset fallback). */
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

                                /* Check for positioning adjustments (xoff, yoff, wadjust).
                                   Combining characters have offsets relative to the base. */
                                int glyph_width;
                                if (!NILP (LGLYPH_ADJUSTMENT (glyph_obj)))
                                  {
                                    /* Glyph has explicit positioning — render as
                                       zero-width so the renderer positions it
                                       using its own shaping.  The character is
                                       sent with the total composition width only
                                       for the first sub-glyph. */
                                    glyph_width = LGLYPH_WADJUST (glyph_obj);
                                  }
                                else
                                  {
                                    glyph_width = LGLYPH_WIDTH (glyph_obj);
                                  }

                                /* For the first glyph, use remaining total width
                                   if this is the only glyph.  For multi-glyph
                                   compositions, use the individual glyph widths. */
                                int effective_width;
                                if (gi == cmp_from && cmp_to - cmp_from == 1)
                                  effective_width = total_width;
                                else
                                  effective_width = glyph_width;

                                neomacs_display_add_char_glyph (handle, charcode,
                                                                (uint32_t) char_face_id,
                                                                effective_width,
                                                                ascent_val, descent_val);
                                width_used += effective_width;
                              }
                          }
                      }
                    else
                      {
                        /* Non-automatic (static) composition - use composition table.
                           Render all glyphs in the composition with their offsets. */
                        int cmp_id = glyph->u.cmp.id;
                        struct composition *cmp = composition_table[cmp_id];
                        if (cmp && cmp->glyph_len > 0)
                          {
                            int total_width = glyph->pixel_width;

                            for (int gi = 0; gi < cmp->glyph_len; gi++)
                              {
                                unsigned int charcode = COMPOSITION_GLYPH (cmp, gi);

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

                                /* First glyph gets total width, subsequent are
                                   zero-width (rendered atop the first). */
                                int effective_width = (gi == 0) ? total_width : 0;

                                neomacs_display_add_char_glyph (handle, charcode,
                                                                (uint32_t) char_face_id,
                                                                effective_width,
                                                                ascent_val, descent_val);
                              }
                          }
                      }
                  }
                  break;

                case GLYPHLESS_GLYPH:
                  {
                    enum glyphless_display_method method
                      = glyph->u.glyphless.method;
                    unsigned int ch = glyph->u.glyphless.ch;
                    int gw = glyph->pixel_width;

                    if (method == GLYPHLESS_DISPLAY_THIN_SPACE)
                      {
                        /* Just emit a thin space.  */
                        neomacs_display_add_char_glyph (handle, ' ',
                                                        glyph->face_id,
                                                        gw,
                                                        row->ascent, 0);
                      }
                    else
                      {
                        /* For HEX_CODE, ACRONYM, EMPTY_BOX: draw a box
                           border and the text (if any) inside it.  */
                        unsigned long fg_pixel = face
                          ? face->foreground : 0;
                        uint32_t fg_rgb
                          = (uint32_t) (fg_pixel & 0xFFFFFF);

                        /* Draw box outline (1px border).  */
                        int bx = glyph_x + WINDOW_LEFT_EDGE_X (w);
                        int by = frame_y;
                        int bw = gw;
                        int bh = glyph->ascent + glyph->descent;
                        if (bh <= 0)
                          bh = row->height;

                        /* Top edge */
                        neomacs_display_draw_border (handle,
                                                     bx, by, bw, 1,
                                                     fg_rgb);
                        /* Bottom edge */
                        neomacs_display_draw_border (handle,
                                                     bx, by + bh - 1,
                                                     bw, 1, fg_rgb);
                        /* Left edge */
                        neomacs_display_draw_border (handle,
                                                     bx, by, 1, bh,
                                                     fg_rgb);
                        /* Right edge */
                        neomacs_display_draw_border (handle,
                                                     bx + bw - 1, by,
                                                     1, bh, fg_rgb);

                        if (method == GLYPHLESS_DISPLAY_HEX_CODE)
                          {
                            /* Emit hex digits as characters.  */
                            char buf[7];
                            sprintf (buf, "%0*X",
                                     ch < 0x10000 ? 4 : 6, ch);
                            int len = strlen (buf);
                            int char_w = gw / (len > 0 ? len : 1);

                            for (int k = 0; k < len; k++)
                              neomacs_display_add_char_glyph (
                                handle, (unsigned int) buf[k],
                                glyph->face_id, char_w,
                                row->ascent, 0);
                          }
                        else if (method == GLYPHLESS_DISPLAY_ACRONYM)
                          {
                            /* Look up acronym from char table.  */
                            const char *str = NULL;
                            if (CHAR_TABLE_P (Vglyphless_char_display)
                                && (CHAR_TABLE_EXTRA_SLOTS (
                                      XCHAR_TABLE (
                                        Vglyphless_char_display))
                                    >= 1))
                              {
                                Lisp_Object acronym
                                  = (!glyph->u.glyphless.for_no_font
                                     ? CHAR_TABLE_REF (
                                         Vglyphless_char_display, ch)
                                     : XCHAR_TABLE (
                                         Vglyphless_char_display)
                                         ->extras[0]);
                                if (CONSP (acronym))
                                  acronym = XCAR (acronym);
                                if (STRINGP (acronym))
                                  str = SSDATA (acronym);
                              }
                            if (str)
                              {
                                int len = strlen (str);
                                int char_w
                                  = gw / (len > 0 ? len : 1);
                                for (int k = 0; k < len; k++)
                                  neomacs_display_add_char_glyph (
                                    handle,
                                    (unsigned int) (unsigned char) str[k],
                                    glyph->face_id, char_w,
                                    row->ascent, 0);
                              }
                            else
                              {
                                /* Fallback: empty box.  */
                                neomacs_display_add_char_glyph (
                                  handle, ' ', glyph->face_id,
                                  gw, row->ascent, 0);
                              }
                          }
                        else
                          {
                            /* EMPTY_BOX: just the box, no text.  */
                            neomacs_display_add_char_glyph (
                              handle, ' ', glyph->face_id,
                              gw, row->ascent, 0);
                          }
                      }
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
              if (area == TEXT_AREA)
                text_col++;
              glyph++;
            }
        }
    }

  return true;  /* continue iterating to next window */
}

/* ============================================================================
 * Rust Layout Engine FFI Helpers
 * ============================================================================
 *
 * These functions are called by the Rust layout engine to read Emacs data
 * structures during layout computation.  They run on the Emacs thread.
 */

/* Global flag: whether to use the Rust display engine */
bool use_rust_display_engine = true;

/* Get a character at a character position in a buffer.
   Returns the Unicode codepoint, or -1 if out of range. */
int
neomacs_layout_char_at (void *buffer_ptr, int64_t charpos)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  if (charpos < BEGV || charpos >= ZV)
    {
      set_buffer_internal_1 (old);
      return -1;
    }

  ptrdiff_t bytepos = CHAR_TO_BYTE (charpos);
  int ch = FETCH_CHAR (bytepos);

  set_buffer_internal_1 (old);
  return ch;
}

/* Copy buffer text as UTF-8 into the provided buffer.
   `from` and `to` are character positions.
   Returns number of bytes written, or -1 on error. */
int64_t
neomacs_layout_buffer_text (void *buffer_ptr, int64_t from, int64_t to,
                            uint8_t *out_buf, int64_t out_buf_len)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf || !out_buf || out_buf_len <= 0)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t begv = BEGV;
  ptrdiff_t zv = ZV;

  if (from < begv) from = begv;
  if (to > zv) to = zv;
  if (from >= to)
    {
      set_buffer_internal_1 (old);
      return 0;
    }

  int64_t written = 0;
  ptrdiff_t pos = from;

  while (pos < to && written < out_buf_len - 4)  /* leave room for max UTF-8 char */
    {
      ptrdiff_t bytepos = CHAR_TO_BYTE (pos);
      int ch = FETCH_CHAR (bytepos);

      /* Encode as UTF-8 */
      if (ch < 0x80)
        {
          out_buf[written++] = (uint8_t) ch;
        }
      else if (ch < 0x800)
        {
          out_buf[written++] = (uint8_t) (0xC0 | (ch >> 6));
          out_buf[written++] = (uint8_t) (0x80 | (ch & 0x3F));
        }
      else if (ch < 0x10000)
        {
          out_buf[written++] = (uint8_t) (0xE0 | (ch >> 12));
          out_buf[written++] = (uint8_t) (0x80 | ((ch >> 6) & 0x3F));
          out_buf[written++] = (uint8_t) (0x80 | (ch & 0x3F));
        }
      else
        {
          out_buf[written++] = (uint8_t) (0xF0 | (ch >> 18));
          out_buf[written++] = (uint8_t) (0x80 | ((ch >> 12) & 0x3F));
          out_buf[written++] = (uint8_t) (0x80 | ((ch >> 6) & 0x3F));
          out_buf[written++] = (uint8_t) (0x80 | (ch & 0x3F));
        }

      pos++;
    }

  set_buffer_internal_1 (old);
  return written;
}

/* Get buffer narrowing bounds. */
void
neomacs_layout_buffer_bounds (void *buffer_ptr, int64_t *begv, int64_t *zv)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    {
      *begv = 1;
      *zv = 1;
      return;
    }

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);
  *begv = BEGV;
  *zv = ZV;
  set_buffer_internal_1 (old);
}

/* Get buffer point position. */
int64_t
neomacs_layout_buffer_point (void *buffer_ptr)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 1;
  return BUF_PT (buf);
}

/* Check if buffer uses multibyte encoding. */
int
neomacs_layout_buffer_multibyte_p (void *buffer_ptr)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 0;
  return !NILP (BVAR (buf, enable_multibyte_characters));
}

/* Get buffer-local tab-width. */
int
neomacs_layout_buffer_tab_width (void *buffer_ptr)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 8;
  Lisp_Object tw = BVAR (buf, tab_width);
  return FIXNATP (tw) ? XFIXNAT (tw) : 8;
}

/* Get buffer-local truncate-lines setting. */
int
neomacs_layout_buffer_truncate_lines (void *buffer_ptr)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 0;
  return !NILP (BVAR (buf, truncate_lines));
}

/* Get number of leaf windows in the frame. */
int
neomacs_layout_frame_window_count (void *frame_ptr)
{
  struct frame *f = (struct frame *) frame_ptr;
  if (!f)
    return 0;

  /* Count leaf windows using same traversal as neomacs_extract_full_frame */
  int count = 0;
  struct window *stack[64];
  int sp = 0;
  stack[sp++] = XWINDOW (FRAME_ROOT_WINDOW (f));

  while (sp > 0)
    {
      struct window *w = stack[--sp];
      if (WINDOWP (w->contents))
        {
          struct window *child = XWINDOW (w->contents);
          while (child)
            {
              if (sp < 64)
                stack[sp++] = child;
              child = NILP (child->next) ? NULL : XWINDOW (child->next);
            }
        }
      else
        count++;
    }

  /* Also count minibuffer */
  Lisp_Object mini = FRAME_MINIBUF_WINDOW (f);
  if (!NILP (mini) && FRAME_HAS_MINIBUF_P (f))
    count++;

  return count;
}

/* FFI struct matching Rust WindowParamsFFI.
   Must be kept in sync with layout/emacs_ffi.rs. */
struct neomacs_window_params_ffi {
  int64_t window_id;
  uint64_t buffer_id;
  void *window_ptr;
  void *buffer_ptr;
  float x, y, width, height;
  float text_x, text_y, text_width, text_height;
  int selected;
  int is_minibuffer;
  int64_t window_start;
  int64_t point;
  int64_t buffer_zv;
  int64_t buffer_begv;
  int hscroll;
  int truncate_lines;
  int word_wrap;
  int tab_width;
  uint32_t default_fg;
  uint32_t default_bg;
  float char_width, char_height;
  float font_pixel_size;
  float font_ascent;
  float mode_line_height;
  float header_line_height;
  float tab_line_height;
  uint8_t cursor_type;
  int cursor_bar_width;
  /* Fringe widths in pixels */
  float left_fringe_width;
  float right_fringe_width;
  /* indicate-empty-lines: 0=off, 1=left, 2=right */
  int indicate_empty_lines;
  /* show-trailing-whitespace */
  int show_trailing_whitespace;
  /* trailing-whitespace face background color (sRGB) */
  uint32_t trailing_ws_bg;
  /* fill-column-indicator column (0 = off) */
  int fill_column_indicator;
  /* fill-column-indicator character (0 = use default '|') */
  int fill_column_indicator_char;
  /* fill-column-indicator face foreground (sRGB) */
  uint32_t fill_column_indicator_fg;
  /* Extra line spacing in pixels */
  float extra_line_spacing;
  /* Whether to show cursor in non-selected windows */
  int cursor_in_non_selected;
  /* selective-display: 0=off, >0=hide lines indented more than N columns */
  int selective_display;
  /* escape-glyph face foreground color for control chars */
  uint32_t escape_glyph_fg;
  /* nobreak-char-display: 0=off, 1=highlight, 2=escape notation */
  int nobreak_char_display;
  /* nobreak-char face foreground color */
  uint32_t nobreak_char_fg;
  /* glyphless-char face foreground color */
  uint32_t glyphless_char_fg;
  /* wrap-prefix: string rendered at start of continuation lines */
  uint8_t wrap_prefix[128];
  int wrap_prefix_len;
  /* line-prefix: string rendered at start of all visual lines */
  uint8_t line_prefix[128];
  int line_prefix_len;
  /* Margin widths in pixels */
  float left_margin_width;
  float right_margin_width;
  /* Buffer file name (NULL if no file) */
  const char *buffer_file_name;
  /* Whether the buffer has unsaved modifications */
  int modified;
};

/* Get window parameters for the Nth leaf window.
   Returns 0 on success, -1 on error. */
int
neomacs_layout_get_window_params (void *frame_ptr, int window_index,
                                  struct neomacs_window_params_ffi *params)
{
  struct frame *f = (struct frame *) frame_ptr;
  if (!f || !params)
    return -1;

  /* Find the Nth leaf window */
  int count = 0;
  struct window *found = NULL;
  struct window *stack[64];
  int sp = 0;
  stack[sp++] = XWINDOW (FRAME_ROOT_WINDOW (f));

  while (sp > 0 && !found)
    {
      struct window *w = stack[--sp];
      if (WINDOWP (w->contents))
        {
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
          if (count == window_index)
            found = w;
          count++;
        }
    }

  /* Check minibuffer */
  if (!found)
    {
      Lisp_Object mini = FRAME_MINIBUF_WINDOW (f);
      if (!NILP (mini) && FRAME_HAS_MINIBUF_P (f))
        {
          if (count == window_index)
            found = XWINDOW (mini);
        }
    }

  if (!found)
    return -1;

  struct window *w = found;

  /* Fill params */
  params->window_id = (int64_t)(intptr_t) w;
  params->window_ptr = (void *) w;

  if (BUFFERP (w->contents))
    {
      struct buffer *buf = XBUFFER (w->contents);
      params->buffer_id = (uint64_t)(uintptr_t) buf;
      params->buffer_ptr = (void *) buf;
      params->point = BUF_PT (buf);
      params->buffer_begv = BUF_BEGV (buf);
      params->buffer_zv = BUF_ZV (buf);
      params->tab_width = NILP (BVAR (buf, tab_width)) ? 8
                          : (FIXNATP (BVAR (buf, tab_width))
                             ? XFIXNAT (BVAR (buf, tab_width)) : 8);
      params->truncate_lines = !NILP (BVAR (buf, truncate_lines));
      params->word_wrap = !NILP (BVAR (buf, word_wrap));
      Lisp_Object fn = BVAR (buf, filename);
      params->buffer_file_name = STRINGP (fn) ? SSDATA (fn) : NULL;
      params->modified = (BUF_SAVE_MODIFF (buf) < BUF_MODIFF (buf)) ? 1 : 0;
    }
  else
    {
      params->buffer_id = 0;
      params->buffer_ptr = NULL;
      params->point = 1;
      params->buffer_begv = 1;
      params->buffer_zv = 1;
      params->tab_width = 8;
      params->truncate_lines = 0;
      params->word_wrap = 0;
      params->buffer_file_name = NULL;
      params->modified = 0;
    }

  params->x = (float) WINDOW_LEFT_EDGE_X (w);
  params->y = (float) WINDOW_TOP_EDGE_Y (w);
  params->width = (float) WINDOW_PIXEL_WIDTH (w);
  params->height = (float) WINDOW_PIXEL_HEIGHT (w);

  /* Text area bounds */
  params->text_x = (float) window_box_left (w, TEXT_AREA);
  params->text_y = (float) WINDOW_TOP_EDGE_Y (w);
  params->text_width = (float) window_box_width (w, TEXT_AREA);
  params->text_height = (float) WINDOW_PIXEL_HEIGHT (w);

  /* Margin areas */
  params->left_margin_width = (float) WINDOW_LEFT_MARGIN_WIDTH (w);
  params->right_margin_width = (float) WINDOW_RIGHT_MARGIN_WIDTH (w);

  params->selected = (w == XWINDOW (f->selected_window)) ? 1 : 0;
  params->is_minibuffer = MINI_WINDOW_P (w) ? 1 : 0;

  if (MARKERP (w->start))
    params->window_start = marker_position (w->start);
  else
    params->window_start = 1;

  params->hscroll = w->hscroll;

  /* Face colors */
  struct face *default_face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
  if (default_face)
    {
      unsigned long fg = default_face->foreground_defaulted_p
        ? FRAME_FOREGROUND_PIXEL (f) : default_face->foreground;
      unsigned long bg = default_face->background_defaulted_p
        ? FRAME_BACKGROUND_PIXEL (f) : default_face->background;
      params->default_fg = (uint32_t) ((RED_FROM_ULONG (fg) << 16)
                                       | (GREEN_FROM_ULONG (fg) << 8)
                                       | BLUE_FROM_ULONG (fg));
      params->default_bg = (uint32_t) ((RED_FROM_ULONG (bg) << 16)
                                       | (GREEN_FROM_ULONG (bg) << 8)
                                       | BLUE_FROM_ULONG (bg));
    }
  else
    {
      params->default_fg = 0x00FFFFFF;
      params->default_bg = 0x00000000;
    }

  /* Character cell dimensions.
     Use the window's own default face font if available (respects
     text-scale-mode via face-remapping-alist), otherwise fall back
     to the frame font.  */
  {
    int def_face_id = lookup_basic_face (w, f, DEFAULT_FACE_ID);
    struct face *wface = FACE_FROM_ID_OR_NULL (f, def_face_id);
    if (wface && wface->font)
      {
        params->char_width = (float) wface->font->average_width;
        int asc, desc;
        get_font_ascent_descent (wface->font, &asc, &desc);
        params->char_height = (float) (asc + desc);
        params->font_pixel_size = (float) wface->font->pixel_size;
        params->font_ascent = (float) asc;
      }
    else
      {
        params->char_width = (float) FRAME_COLUMN_WIDTH (f);
        params->char_height = (float) FRAME_LINE_HEIGHT (f);
        params->font_pixel_size = FRAME_FONT (f)
          ? (float) FRAME_FONT (f)->pixel_size : 14.0f;
        params->font_ascent = FRAME_FONT (f)
          ? (float) FONT_BASE (FRAME_FONT (f)) : 12.0f;
      }
  }

  /* Special line heights */
  params->mode_line_height = (float) WINDOW_MODE_LINE_HEIGHT (w);
  params->header_line_height = (float) WINDOW_HEADER_LINE_HEIGHT (w);
  params->tab_line_height = (float) WINDOW_TAB_LINE_HEIGHT (w);

  /* Cursor type — read from phys_cursor_type (set by C display engine).
     get_window_cursor_type() is static in xdisp.c so we use the cached value. */
  switch (w->phys_cursor_type)
    {
    case FILLED_BOX_CURSOR: params->cursor_type = 0; break;
    case BAR_CURSOR: params->cursor_type = 1; break;
    case HBAR_CURSOR: params->cursor_type = 2; break;
    case HOLLOW_BOX_CURSOR: params->cursor_type = 3; break;
    default: params->cursor_type = 0; break;
    }
  params->cursor_bar_width = w->phys_cursor_width > 0 ? w->phys_cursor_width : 2;

  /* Fringe widths */
  params->left_fringe_width = (float) WINDOW_LEFT_FRINGE_WIDTH (w);
  params->right_fringe_width = (float) WINDOW_RIGHT_FRINGE_WIDTH (w);

  /* indicate-empty-lines */
  if (BUFFERP (w->contents))
    {
      Lisp_Object iel = BVAR (XBUFFER (w->contents), indicate_empty_lines);
      if (NILP (iel))
        params->indicate_empty_lines = 0;
      else if (EQ (iel, Qright))
        params->indicate_empty_lines = 2;
      else
        params->indicate_empty_lines = 1; /* t or Qleft -> left */
    }
  else
    params->indicate_empty_lines = 0;

  /* show-trailing-whitespace */
  params->show_trailing_whitespace = !NILP (Vshow_trailing_whitespace);
  if (params->show_trailing_whitespace)
    {
      int face_id = lookup_named_face (w, f, Qtrailing_whitespace, false);
      if (face_id >= 0)
        {
          struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
          if (face)
            {
              unsigned long bg = face->background;
              params->trailing_ws_bg
                  = (uint32_t) ((RED_FROM_ULONG (bg) << 16)
                                | (GREEN_FROM_ULONG (bg) << 8)
                                | BLUE_FROM_ULONG (bg));
            }
          else
            params->trailing_ws_bg = 0x00FF0000;
        }
      else
        params->trailing_ws_bg = 0x00FF0000;
    }
  else
    {
      params->trailing_ws_bg = 0;
    }

  /* fill-column-indicator */
  params->fill_column_indicator = 0;
  params->fill_column_indicator_char = 0;
  params->fill_column_indicator_fg = 0;
  if (display_fill_column_indicator && BUFFERP (w->contents))
    {
      struct buffer *buf = XBUFFER (w->contents);
      Lisp_Object col_val = EQ (Vdisplay_fill_column_indicator_column, Qt)
          ? BVAR (buf, fill_column)
          : Vdisplay_fill_column_indicator_column;
      if (FIXNUMP (col_val) && XFIXNUM (col_val) > 0)
        {
          params->fill_column_indicator = (int) XFIXNUM (col_val);
          params->fill_column_indicator_char
              = (CHARACTERP (Vdisplay_fill_column_indicator_character)
                 ? (int) XFIXNAT (Vdisplay_fill_column_indicator_character)
                 : '|');
          /* Resolve fill-column-indicator face */
          int fci_face_id
              = lookup_named_face (w, f, Qfill_column_indicator, false);
          if (fci_face_id >= 0)
            {
              struct face *fci_face
                  = FACE_FROM_ID_OR_NULL (f, fci_face_id);
              if (fci_face)
                {
                  unsigned long fg = fci_face->foreground;
                  params->fill_column_indicator_fg
                      = (uint32_t) ((RED_FROM_ULONG (fg) << 16)
                                    | (GREEN_FROM_ULONG (fg) << 8)
                                    | BLUE_FROM_ULONG (fg));
                }
            }
        }
    }

  /* Extra line spacing: buffer-local or frame-level */
  params->extra_line_spacing = 0.0f;
  if (BUFFERP (w->contents))
    {
      Lisp_Object els = BVAR (XBUFFER (w->contents), extra_line_spacing);
      if (FIXNUMP (els))
        params->extra_line_spacing = (float) XFIXNUM (els);
      else if (FLOATP (els))
        params->extra_line_spacing = (float) XFLOAT_DATA (els);
    }
  if (params->extra_line_spacing == 0.0f)
    params->extra_line_spacing = (float) f->extra_line_spacing;

  /* Cursor in non-selected windows */
  params->cursor_in_non_selected
      = !NILP (BVAR (&buffer_defaults, cursor_in_non_selected_windows));

  /* escape-glyph face for control characters */
  params->escape_glyph_fg = params->default_fg; // fallback
  {
    int eg_face_id = lookup_named_face (w, f, Qescape_glyph, false);
    if (eg_face_id >= 0)
      {
        struct face *eg_face = FACE_FROM_ID_OR_NULL (f, eg_face_id);
        if (eg_face)
          {
            unsigned long fg = eg_face->foreground;
            params->escape_glyph_fg
              = (uint32_t) ((RED_FROM_ULONG (fg) << 16)
                            | (GREEN_FROM_ULONG (fg) << 8)
                            | BLUE_FROM_ULONG (fg));
          }
      }
  }

  /* nobreak-char-display */
  params->nobreak_char_display = 0;
  params->nobreak_char_fg = params->default_fg;
  if (FIXNUMP (Vnobreak_char_display) && XFIXNUM (Vnobreak_char_display) > 0)
    params->nobreak_char_display = (int) XFIXNUM (Vnobreak_char_display);
  else if (!NILP (Vnobreak_char_display))
    params->nobreak_char_display = 1;
  if (params->nobreak_char_display > 0)
    {
      int nb_face_id = lookup_named_face (w, f, Qnobreak_space, false);
      if (nb_face_id >= 0)
        {
          struct face *nb_face = FACE_FROM_ID_OR_NULL (f, nb_face_id);
          if (nb_face)
            {
              unsigned long fg = nb_face->foreground;
              params->nobreak_char_fg
                = (uint32_t) ((RED_FROM_ULONG (fg) << 16)
                              | (GREEN_FROM_ULONG (fg) << 8)
                              | BLUE_FROM_ULONG (fg));
            }
        }
    }

  /* glyphless-char face for unrenderable characters */
  params->glyphless_char_fg = params->default_fg;
  {
    int gc_face_id = lookup_named_face (w, f, Qglyphless_char, false);
    if (gc_face_id >= 0)
      {
        struct face *gc_face = FACE_FROM_ID_OR_NULL (f, gc_face_id);
        if (gc_face)
          {
            unsigned long fg = gc_face->foreground;
            params->glyphless_char_fg
              = (uint32_t) ((RED_FROM_ULONG (fg) << 16)
                            | (GREEN_FROM_ULONG (fg) << 8)
                            | BLUE_FROM_ULONG (fg));
          }
      }
  }

  /* selective-display: when a fixnum, hide lines indented deeper */
  params->selective_display = 0;
  if (BUFFERP (w->contents))
    {
      Lisp_Object sd = BVAR (XBUFFER (w->contents), selective_display);
      if (FIXNUMP (sd) && XFIXNUM (sd) > 0)
        params->selective_display = (int) XFIXNUM (sd);
    }

  /* wrap-prefix and line-prefix (global variables, may also be per-char props) */
  params->wrap_prefix_len = 0;
  params->line_prefix_len = 0;
  if (!NILP (Vwrap_prefix) && STRINGP (Vwrap_prefix))
    {
      ptrdiff_t len = SBYTES (Vwrap_prefix);
      ptrdiff_t copy_len = len < 127 ? len : 127;
      memcpy (params->wrap_prefix, SDATA (Vwrap_prefix), copy_len);
      params->wrap_prefix[copy_len] = 0;
      params->wrap_prefix_len = (int) copy_len;
    }
  if (!NILP (Vline_prefix) && STRINGP (Vline_prefix))
    {
      ptrdiff_t len = SBYTES (Vline_prefix);
      ptrdiff_t copy_len = len < 127 ? len : 127;
      memcpy (params->line_prefix, SDATA (Vline_prefix), copy_len);
      params->line_prefix[copy_len] = 0;
      params->line_prefix_len = (int) copy_len;
    }

  return 0;
}

/* Set window_end_pos on an Emacs window. */
void
neomacs_layout_set_window_end (void *window_ptr, int64_t end_pos, int end_vpos)
{
  struct window *w = (struct window *) window_ptr;
  if (!w)
    return;
  w->window_end_pos = BUF_Z (XBUFFER (w->contents)) - end_pos;
  w->window_end_vpos = end_vpos;
  w->window_end_valid = true;
}

/* Set cursor position on an Emacs window. */
void
neomacs_layout_set_cursor (void *window_ptr, int x, int y, int hpos, int vpos)
{
  struct window *w = (struct window *) window_ptr;
  if (!w)
    return;
  w->cursor.x = x;
  w->cursor.y = y;
  w->cursor.hpos = hpos;
  w->cursor.vpos = vpos;
}

/* Trigger fontification at a range (calls fontification-functions).
   Returns 1 if fontification happened, 0 if already fontified. */
int
neomacs_layout_ensure_fontified (void *buffer_ptr, int64_t from, int64_t to)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 0;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  /* Check if text is already fontified */
  Lisp_Object fontified = Fget_text_property (make_fixnum (from),
                                              Qfontified, Qnil);
  if (!NILP (fontified))
    {
      set_buffer_internal_1 (old);
      return 0;
    }

  /* Run fontification-functions */
  if (!NILP (Vfontification_functions))
    {
      safe_calln (XCAR (Vfontification_functions), make_fixnum (from));
    }

  set_buffer_internal_1 (old);
  return 1;
}

/* Check if text at charpos is invisible.
   Returns 0 = visible, 1 = invisible (hidden), 2 = invisible (ellipsis).
   If invisible, *next_visible_out is set to the next visible position.  */
int
neomacs_layout_check_invisible (void *buffer_ptr, void *window_ptr,
                                int64_t charpos, int64_t *next_visible_out)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 0;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t zv = BUF_ZV (buf);
  if (charpos >= zv)
    {
      set_buffer_internal_1 (old);
      return 0;
    }

  /* Get the invisible property at this position.  */
  Lisp_Object prop = Fget_char_property (make_fixnum (charpos),
                                         Qinvisible, Qnil);

  int invis = TEXT_PROP_MEANS_INVISIBLE (prop);
  if (invis == 0)
    {
      /* Text is visible. Set next_visible_out to the position where
         the invisible property next changes so caller knows when to
         re-check.  */
      if (next_visible_out)
        {
          Lisp_Object limit = make_fixnum (zv);
          Lisp_Object next = Fnext_single_char_property_change (
              make_fixnum (charpos), Qinvisible, Qnil, limit);
          *next_visible_out = XFIXNUM (next);
        }
      set_buffer_internal_1 (old);
      return 0;
    }

  /* Find the next visible position efficiently by jumping to
     property change boundaries.  */
  if (next_visible_out)
    {
      Lisp_Object limit = make_fixnum (zv);
      Lisp_Object pos_obj = make_fixnum (charpos);

      /* Jump forward through invisible regions.  */
      while (1)
        {
          /* Find where the invisible property next changes.  */
          Lisp_Object next = Fnext_single_char_property_change (
              pos_obj, Qinvisible, Qnil, limit);
          int64_t next_pos = XFIXNUM (next);
          if (next_pos >= zv)
            {
              *next_visible_out = zv;
              break;
            }
          /* Check if text at the new position is visible.  */
          Lisp_Object p = Fget_char_property (next, Qinvisible, Qnil);
          if (TEXT_PROP_MEANS_INVISIBLE (p) == 0)
            {
              *next_visible_out = next_pos;
              break;
            }
          pos_obj = next;
        }
    }

  set_buffer_internal_1 (old);
  return invis;
}

/* Helper: fill a FaceDataFFI struct from a resolved Emacs face.
   The struct layout must match the Rust FaceDataFFI in emacs_ffi.rs. */
struct FaceDataFFI {
  uint32_t face_id;
  uint32_t fg;
  uint32_t bg;
  const char *font_family;
  int font_weight;
  int italic;
  int font_size;
  int underline_style;
  uint32_t underline_color;
  int strike_through;
  uint32_t strike_through_color;
  int overline;
  uint32_t overline_color;
  int box_type;
  uint32_t box_color;
  int box_line_width;
  int extend;
  float font_char_width;
  float font_ascent;
  float font_space_width;
  int font_is_monospace;
};

static void
fill_face_data (struct frame *f, struct face *face, struct FaceDataFFI *out)
{
  unsigned long fg = face->foreground;
  unsigned long bg = face->background;

  if (face->foreground_defaulted_p)
    fg = FRAME_FOREGROUND_PIXEL (f);
  if (face->background_defaulted_p)
    bg = FRAME_BACKGROUND_PIXEL (f);

  out->face_id = (uint32_t) face->id;
  out->fg = ((RED_FROM_ULONG (fg) << 16) |
             (GREEN_FROM_ULONG (fg) << 8) |
             BLUE_FROM_ULONG (fg));
  out->bg = ((RED_FROM_ULONG (bg) << 16) |
             (GREEN_FROM_ULONG (bg) << 8) |
             BLUE_FROM_ULONG (bg));

  /* Font family from realized font object */
  out->font_family = NULL;
  if (face->font)
    {
      Lisp_Object family_attr = face->font->props[FONT_FAMILY_INDEX];
      if (!NILP (family_attr) && SYMBOLP (family_attr))
        out->font_family = SSDATA (SYMBOL_NAME (family_attr));
    }
  if (!out->font_family)
    {
      Lisp_Object family_attr = face->lface[LFACE_FAMILY_INDEX];
      if (!NILP (family_attr) && STRINGP (family_attr))
        out->font_family = SSDATA (family_attr);
    }

  /* Font weight (CSS scale) */
  out->font_weight = 400;
  Lisp_Object weight_attr = face->lface[LFACE_WEIGHT_INDEX];
  if (!NILP (weight_attr) && SYMBOLP (weight_attr))
    {
      int w = FONT_WEIGHT_NAME_NUMERIC (weight_attr);
      if (w > 0) out->font_weight = emacs_weight_to_css (w);
    }

  /* Italic */
  out->italic = 0;
  Lisp_Object slant_attr = face->lface[LFACE_SLANT_INDEX];
  if (!NILP (slant_attr) && SYMBOLP (slant_attr))
    {
      int s = FONT_SLANT_NAME_NUMERIC (slant_attr);
      if (s != 100) out->italic = 1;
    }

  /* Font pixel size */
  out->font_size = 14;
  if (face->font)
    out->font_size = face->font->pixel_size;

  /* Underline */
  out->underline_style = 0;
  out->underline_color = out->fg;
  if (face->underline != FACE_NO_UNDERLINE)
    {
      switch (face->underline)
        {
        case FACE_UNDERLINE_SINGLE: out->underline_style = 1; break;
        case FACE_UNDERLINE_WAVE: out->underline_style = 2; break;
        case FACE_UNDERLINE_DOUBLE_LINE: out->underline_style = 3; break;
        case FACE_UNDERLINE_DOTS: out->underline_style = 4; break;
        case FACE_UNDERLINE_DASHES: out->underline_style = 5; break;
        default: out->underline_style = 1; break;
        }
      if (!face->underline_defaulted_p)
        out->underline_color = ((RED_FROM_ULONG (face->underline_color) << 16) |
                                (GREEN_FROM_ULONG (face->underline_color) << 8) |
                                BLUE_FROM_ULONG (face->underline_color));
    }

  /* Strike-through */
  out->strike_through = face->strike_through_p ? 1 : 0;
  out->strike_through_color = out->fg;
  if (out->strike_through && !face->strike_through_color_defaulted_p)
    out->strike_through_color = ((RED_FROM_ULONG (face->strike_through_color) << 16) |
                                 (GREEN_FROM_ULONG (face->strike_through_color) << 8) |
                                 BLUE_FROM_ULONG (face->strike_through_color));

  /* Overline */
  out->overline = face->overline_p ? 1 : 0;
  out->overline_color = out->fg;
  if (out->overline && !face->overline_color_defaulted_p)
    out->overline_color = ((RED_FROM_ULONG (face->overline_color) << 16) |
                           (GREEN_FROM_ULONG (face->overline_color) << 8) |
                           BLUE_FROM_ULONG (face->overline_color));

  /* Box */
  out->box_type = 0;
  out->box_color = out->fg;
  out->box_line_width = 0;
  if (face->box != FACE_NO_BOX)
    {
      out->box_type = 1;
      out->box_line_width = eabs (face->box_vertical_line_width);
      if (out->box_line_width == 0) out->box_line_width = 1;
      out->box_color = ((RED_FROM_ULONG (face->box_color) << 16) |
                        (GREEN_FROM_ULONG (face->box_color) << 8) |
                        BLUE_FROM_ULONG (face->box_color));
    }

  /* Extend: face background extends to end of visual line */
  out->extend = FACE_EXTENSIBLE_P (face) ? 1 : 0;

  /* Per-face font metrics for mixed-font rendering */
  if (face->font)
    {
      out->font_char_width = (float) face->font->average_width;
      out->font_ascent = (float) FONT_BASE (face->font);
      out->font_space_width = (float) face->font->space_width;
      out->font_is_monospace = (face->font->average_width == face->font->space_width
                                && face->font->space_width == face->font->max_width)
                               ? 1 : 0;
    }
  else
    {
      out->font_char_width = 0.0f; /* 0 = use window default */
      out->font_ascent = 0.0f;
      out->font_space_width = 0.0f;
      out->font_is_monospace = 1;
    }
}

/* Get the advance width of a single character in a specific face's font.
   Returns the pixel width, or -1.0 if not measurable. */
float
neomacs_layout_char_width (void *window_ptr, int charcode, int face_id)
{
  struct window *w = (struct window *) window_ptr;
  if (!w)
    return -1.0f;

  struct frame *f = XFRAME (w->frame);
  struct face *face = FACE_FROM_ID (f, face_id);
  if (!face || !face->font)
    return -1.0f;

  struct font *font = face->font;
  unsigned code = font->driver->encode_char (font, charcode);
  if (code == FONT_INVALID_CODE)
    return -1.0f;

  struct font_metrics metrics;
  font->driver->text_extents (font, &code, 1, &metrics);
  return (float) metrics.width;
}

/* Fill advance widths for ASCII characters (0-127) in a face's font.
   widths must point to an array of at least 128 floats. */
void
neomacs_layout_fill_ascii_widths (void *window_ptr, int face_id, float *widths)
{
  struct window *w = (struct window *) window_ptr;
  if (!w)
    {
      for (int i = 0; i < 128; i++) widths[i] = -1.0f;
      return;
    }

  struct frame *f = XFRAME (w->frame);
  struct face *face = FACE_FROM_ID (f, face_id);
  if (!face || !face->font)
    {
      for (int i = 0; i < 128; i++) widths[i] = -1.0f;
      return;
    }

  struct font *font = face->font;
  for (int i = 0; i < 128; i++)
    {
      unsigned code = font->driver->encode_char (font, i);
      if (code == FONT_INVALID_CODE)
        {
          widths[i] = -1.0f;
          continue;
        }
      struct font_metrics metrics;
      font->driver->text_extents (font, &code, 1, &metrics);
      widths[i] = (float) metrics.width;
    }
}

/* Get the resolved face at a buffer position for a window.
   Uses face_at_buffer_position() internally.
   Returns the face_id (>= 0) on success, -1 on error.
   If next_check_out is non-NULL, writes the position where the face may change. */
int
neomacs_layout_face_at_pos (void *window_ptr, int64_t charpos,
                            void *face_out, int64_t *next_check_out)
{
  struct window *w = (struct window *) window_ptr;
  if (!w)
    return -1;

  struct frame *f = XFRAME (w->frame);
  if (!f)
    return -1;

  /* Set buffer to window's buffer for face resolution */
  struct buffer *old = current_buffer;
  if (BUFFERP (w->contents))
    set_buffer_internal_1 (XBUFFER (w->contents));

  ptrdiff_t next_check = 0;
  int face_id = face_at_buffer_position (w, (ptrdiff_t) charpos,
                                         &next_check,
                                         (ptrdiff_t) charpos + 100,
                                         false, DEFAULT_FACE_ID,
                                         0);

  struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
  if (!face)
    face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);

  set_buffer_internal_1 (old);

  if (!face)
    return -1;

  fill_face_data (f, face, (struct FaceDataFFI *) face_out);

  if (next_check_out)
    *next_check_out = (int64_t) next_check;

  return face_id;
}

/* Get the default face for a frame. */
int
neomacs_layout_default_face (void *frame_ptr, void *face_out)
{
  struct frame *f = (struct frame *) frame_ptr;
  if (!f)
    return -1;

  struct face *face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
  if (!face)
    return -1;

  fill_face_data (f, face, (struct FaceDataFFI *) face_out);
  return DEFAULT_FACE_ID;
}

/* Get mode-line text for a window as plain UTF-8.
   Returns the number of bytes written, or -1 on error.
   Also fills face_out with the mode-line face (active or inactive). */
int64_t
neomacs_layout_mode_line_text (void *window_ptr, void *frame_ptr,
                               uint8_t *out_buf, int64_t out_buf_len,
                               void *face_out)
{
  struct window *w = (struct window *) window_ptr;
  if (!w || !out_buf || out_buf_len <= 0)
    return -1;

  /* Get frame from window if not provided */
  struct frame *f = frame_ptr ? (struct frame *) frame_ptr : XFRAME (w->frame);

  /* Determine which face to use */
  int selected = (w == XWINDOW (f->selected_window));
  int face_id = selected ? MODE_LINE_ACTIVE_FACE_ID : MODE_LINE_INACTIVE_FACE_ID;

  /* Fill face data */
  if (face_out)
    {
      struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
      if (!face)
        face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
      if (face)
        fill_face_data (f, face, (struct FaceDataFFI *) face_out);
    }

  /* Get mode-line-format from the window's buffer */
  if (!BUFFERP (w->contents))
    return -1;

  struct buffer *buf = XBUFFER (w->contents);
  Lisp_Object format = BVAR (buf, mode_line_format);
  if (NILP (format))
    return 0; /* No mode line */

  /* Set buffer context for format-mode-line */
  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  /* Call (format-mode-line FORMAT FACE WINDOW BUFFER)
     Use the mode-line face to get propertized string with face text properties
     from :propertize specs. */
  Lisp_Object face_sym = selected
    ? Qmode_line_active : Qmode_line_inactive;
  Lisp_Object window_obj;
  XSETWINDOW (window_obj, w);
  Lisp_Object result = Fformat_mode_line (format, face_sym,
                                           window_obj, w->contents);

  set_buffer_internal_1 (old);

  if (!STRINGP (result))
    return -1;

  ptrdiff_t len = SBYTES (result);
  if (len > out_buf_len)
    len = out_buf_len;

  memcpy (out_buf, SDATA (result), len);

  /* Extract face runs from the propertized string.
     Walk character positions and detect face changes.
     Store face runs in the face_runs area (if provided via out_buf after text).
     Format: each run = { uint16_t byte_offset, uint32_t fg, uint32_t bg }
     Total = 10 bytes per run.  Stored after text data if space permits. */
  if (string_intervals (result) && len + 10 <= out_buf_len)
    {
      /* Extract per-character face properties from the string. */
      ptrdiff_t charpos = 0;
      ptrdiff_t nchars = SCHARS (result);
      ptrdiff_t run_offset = len;
      int nruns = 0;
      int max_runs = (int) ((out_buf_len - len) / 10);
      uint32_t prev_fg = 0xFFFFFFFF;
      uint32_t prev_bg = 0xFFFFFFFF;

      while (charpos < nchars && nruns < max_runs)
        {
          /* Get face property at this position */
          Lisp_Object face_prop
            = Fget_text_property (make_fixnum (charpos), Qface, result);

          /* Find next change */
          Lisp_Object next_change
            = Fnext_single_property_change (make_fixnum (charpos), Qface,
                                             result, Qnil);
          ptrdiff_t next_pos = NILP (next_change)
            ? nchars : XFIXNUM (next_change);

          /* Resolve face to colors */
          uint32_t fg = 0, bg = 0;
          if (!NILP (face_prop))
            {
              int rid = lookup_named_face (w, f,
                                           SYMBOLP (face_prop)
                                           ? face_prop : Qdefault,
                                           false);
              if (rid >= 0)
                {
                  struct face *rf = FACE_FROM_ID_OR_NULL (f, rid);
                  if (rf)
                    {
                      unsigned long c = rf->foreground;
                      fg = ((RED_FROM_ULONG (c) << 16)
                            | (GREEN_FROM_ULONG (c) << 8)
                            | BLUE_FROM_ULONG (c));
                      c = rf->background;
                      bg = ((RED_FROM_ULONG (c) << 16)
                            | (GREEN_FROM_ULONG (c) << 8)
                            | BLUE_FROM_ULONG (c));
                    }
                }
            }

          /* Only emit a run if colors changed */
          if (fg != prev_fg || bg != prev_bg)
            {
              /* Get byte offset for this charpos */
              ptrdiff_t byte_off = string_char_to_byte (result, charpos);
              if (byte_off > 0xFFFF) byte_off = 0xFFFF;
              uint16_t boff = (uint16_t) byte_off;
              memcpy (out_buf + run_offset, &boff, 2);
              memcpy (out_buf + run_offset + 2, &fg, 4);
              memcpy (out_buf + run_offset + 6, &bg, 4);
              run_offset += 10;
              nruns++;
              prev_fg = fg;
              prev_bg = bg;
            }

          charpos = next_pos;
        }

      /* Store number of runs as a negative return value offset:
         actual return = text_len | (nruns << 32) packed as int64_t */
      if (nruns > 0)
        return (int64_t) len | ((int64_t) nruns << 32);
    }

  return (int64_t) len;
}

/* Get header-line text for a window as plain UTF-8.
   Returns the number of bytes written, 0 if no header-line, or -1 on error.
   Also fills face_out with the header-line face (active or inactive). */
int64_t
neomacs_layout_header_line_text (void *window_ptr, void *frame_ptr,
                                  uint8_t *out_buf, int64_t out_buf_len,
                                  void *face_out)
{
  struct window *w = (struct window *) window_ptr;
  if (!w || !out_buf || out_buf_len <= 0)
    return -1;

  struct frame *f = frame_ptr ? (struct frame *) frame_ptr : XFRAME (w->frame);

  int selected = (w == XWINDOW (f->selected_window));
  int face_id = selected ? HEADER_LINE_ACTIVE_FACE_ID : HEADER_LINE_INACTIVE_FACE_ID;

  if (face_out)
    {
      struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
      if (!face)
        face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
      if (face)
        fill_face_data (f, face, (struct FaceDataFFI *) face_out);
    }

  if (!BUFFERP (w->contents))
    return -1;

  struct buffer *buf = XBUFFER (w->contents);
  Lisp_Object format = BVAR (buf, header_line_format);
  if (NILP (format))
    return 0;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  Lisp_Object window_obj;
  XSETWINDOW (window_obj, w);
  Lisp_Object result = Fformat_mode_line (format, make_fixnum (0),
                                           window_obj, w->contents);

  set_buffer_internal_1 (old);

  if (!STRINGP (result))
    return -1;

  ptrdiff_t len = SBYTES (result);
  if (len > out_buf_len)
    len = out_buf_len;

  memcpy (out_buf, SDATA (result), len);
  return (int64_t) len;
}

/* Get tab-line text for a window as plain UTF-8.
   Returns number of bytes, 0 if no tab-line, or -1 on error.  */
int64_t
neomacs_layout_tab_line_text (void *window_ptr, void *frame_ptr,
                               uint8_t *out_buf, int64_t out_buf_len,
                               void *face_out)
{
  struct window *w = (struct window *) window_ptr;
  if (!w || !out_buf || out_buf_len <= 0)
    return -1;

  struct frame *f = frame_ptr
    ? (struct frame *) frame_ptr : XFRAME (w->frame);

  if (face_out)
    {
      struct face *face
        = FACE_FROM_ID_OR_NULL (f, TAB_LINE_FACE_ID);
      if (!face)
        face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
      if (face)
        fill_face_data (f, face, (struct FaceDataFFI *) face_out);
    }

  if (!BUFFERP (w->contents))
    return -1;

  struct buffer *buf = XBUFFER (w->contents);

  /* Check both buffer-local and window-parameter formats.  */
  Lisp_Object format = BVAR (buf, tab_line_format);
  Lisp_Object wfmt = window_parameter (w, Qtab_line_format);
  if (!NILP (wfmt) && !EQ (wfmt, Qnone))
    format = wfmt;
  if (NILP (format))
    return 0;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  Lisp_Object window_obj;
  XSETWINDOW (window_obj, w);
  Lisp_Object result = Fformat_mode_line (
      format, make_fixnum (0), window_obj, w->contents);

  set_buffer_internal_1 (old);

  if (!STRINGP (result))
    return -1;

  ptrdiff_t len = SBYTES (result);
  if (len > out_buf_len)
    len = out_buf_len;

  memcpy (out_buf, SDATA (result), len);
  return (int64_t) len;
}

/* FFI struct for line number configuration.
   Matches Rust LineNumberConfigFFI.  */
struct LineNumberConfigFFI {
  int mode;        /* 0=off, 1=absolute, 2=relative, 3=visual */
  int width;       /* Column width for line numbers */
  int offset;      /* display-line-numbers-offset */
  int major_tick;  /* display-line-numbers-major-tick */
  int minor_tick;  /* display-line-numbers-minor-tick */
  int current_absolute; /* show absolute for current line */
  int widen;       /* count from buffer BEG */
};

/* Get line number display configuration for a window.
   Returns 0 on success, -1 on error.  */
int
neomacs_layout_line_number_config (void *window_ptr,
                                   void *buffer_ptr,
                                   int64_t buffer_zv,
                                   int max_rows,
                                   void *config_out)
{
  struct window *w = (struct window *) window_ptr;
  struct buffer *buf = (struct buffer *) buffer_ptr;
  struct LineNumberConfigFFI *cfg
    = (struct LineNumberConfigFFI *) config_out;

  if (!w || !buf || !cfg)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  /* Check if line numbers are enabled.  */
  if (NILP (Vdisplay_line_numbers))
    {
      cfg->mode = 0;
      set_buffer_internal_1 (old);
      return 0;
    }
  else if (EQ (Vdisplay_line_numbers, Qrelative))
    cfg->mode = 2;
  else if (EQ (Vdisplay_line_numbers, Qvisual))
    cfg->mode = 3;
  else
    cfg->mode = 1;

  cfg->offset = display_line_numbers_offset;
  cfg->major_tick = display_line_numbers_major_tick;
  cfg->minor_tick = display_line_numbers_minor_tick;
  cfg->current_absolute
    = !NILP (Vdisplay_line_numbers_current_absolute) ? 1 : 0;
  cfg->widen = display_line_numbers_widen ? 1 : 0;

  /* Calculate width: digits needed for max line number.  */
  ptrdiff_t max_line = buffer_zv;  /* conservative estimate */
  int digits = 1;
  while (max_line >= 10)
    {
      max_line /= 10;
      digits++;
    }

  /* Check fixed width from variable.  */
  if (FIXNATP (Vdisplay_line_numbers_width)
      && XFIXNUM (Vdisplay_line_numbers_width) > digits)
    digits = XFIXNUM (Vdisplay_line_numbers_width);

  /* Add 1 column for space padding after number.  */
  cfg->width = digits + 1;

  set_buffer_internal_1 (old);
  return 0;
}

/* Count the line number at a character position.
   Returns the 1-based line number.
   If widen is nonzero, count from buffer BEG;
   otherwise from BEGV (respects narrowing).  */
int64_t
neomacs_layout_count_line_number (void *buffer_ptr,
                                  int64_t charpos, int widen)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return 1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t start;
  if (widen)
    start = BUF_BEG_BYTE (buf);
  else
    start = BUF_BEGV_BYTE (buf);

  ptrdiff_t pos_byte = buf_charpos_to_bytepos (buf, charpos);
  ptrdiff_t lines = count_lines (start, pos_byte);

  set_buffer_internal_1 (old);
  return (int64_t) (lines + 1);  /* 1-based */
}

/* Resolve the face for a line number and fill FaceDataFFI.
   is_current: nonzero if this is the line containing point.
   lnum: the line number (for tick highlighting).
   Returns 0 on success.  */
int
neomacs_layout_line_number_face (void *window_ptr,
                                  int is_current,
                                  int64_t lnum,
                                  int major_tick,
                                  int minor_tick,
                                  void *face_out)
{
  struct window *w = (struct window *) window_ptr;
  if (!w)
    return -1;

  struct frame *f = XFRAME (w->frame);

  /* Choose face symbol.  */
  Lisp_Object face_name = Qline_number;
  if (is_current)
    face_name = Qline_number_current_line;
  else if (major_tick > 0 && lnum % major_tick == 0)
    face_name = Qline_number_major_tick;
  else if (minor_tick > 0 && lnum % minor_tick == 0)
    face_name = Qline_number_minor_tick;

  int face_id = merge_faces (w, face_name, 0, DEFAULT_FACE_ID);
  struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
  if (!face)
    face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);

  if (face && face_out)
    fill_face_data (f, face, (struct FaceDataFFI *) face_out);

  return 0;
}

/* Get a byte from buffer text at a byte position. */
int
neomacs_layout_buffer_byte_at (void *buffer_ptr, int64_t byte_pos)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  if (!buf)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  if (byte_pos < BUF_BEGV_BYTE (buf) || byte_pos >= BUF_ZV_BYTE (buf))
    {
      set_buffer_internal_1 (old);
      return -1;
    }

  int result = FETCH_BYTE (byte_pos);
  set_buffer_internal_1 (old);
  return result;
}

/* FFI struct for display text property results.
   Layout must match Rust DisplayPropFFI in emacs_ffi.rs. */
struct DisplayPropFFI {
  int type;           /* 0=none, 1=string, 2=space, 3=align-to, 4=image,
                         5=raise, 6=left-fringe, 7=right-fringe, 8=height,
                         9=video, 10=webkit */
  int str_len;        /* bytes of replacement string (type=1) */
  float space_width;  /* width in columns (type=2) */
  float space_height; /* height in pixels (type=2), 0 = use default char_h */
  int64_t covers_to;  /* charpos where display prop region ends */
  float align_to;     /* align-to column (type=3) */
  uint32_t image_gpu_id;  /* GPU image ID (type=4) */
  int image_width;        /* image/video/webkit width in pixels (type=4,9,10) */
  int image_height;       /* image/video/webkit height in pixels (type=4,9,10) */
  int image_hmargin;      /* image horizontal margin in pixels (type=4) */
  int image_vmargin;      /* image vertical margin in pixels (type=4) */
  int image_ascent;       /* image ascent: 0-100 = percent, -1 = centered (type=4) */
  float raise_factor;     /* raise factor (type=5), fraction of line height */
  uint32_t display_fg;    /* display string face fg (type=1), 0=use position face */
  uint32_t display_bg;    /* display string face bg (type=1), 0=use position face */
  int fringe_bitmap_id;   /* fringe bitmap ID (type=6,7) */
  uint32_t fringe_fg;     /* fringe face foreground color (type=6,7) */
  uint32_t fringe_bg;     /* fringe face background color (type=6,7) */
  float height_factor;    /* font height multiplier (0.0=default, >0=scale) */
  uint32_t video_id;      /* video ID (type=9) */
  uint32_t webkit_id;     /* webkit view ID (type=10) */
  int display_nruns;      /* number of face runs in display string (type=1) */
};

/* Check for a 'display text property at charpos.
   Handles:
     - String replacement: (put-text-property ... 'display "text")
     - Space spec: (put-text-property ... 'display (space :width N))
   Writes replacement string into str_buf (for type=1).
   Returns 0 on success. */
int
neomacs_layout_check_display_prop (void *buffer_ptr, void *window_ptr,
                                   int64_t charpos,
                                   uint8_t *str_buf, int str_buf_len,
                                   void *out_ptr)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  struct DisplayPropFFI *out = (struct DisplayPropFFI *) out_ptr;

  out->type = 0;
  out->str_len = 0;
  out->space_width = 0;
  out->space_height = 0;
  out->covers_to = charpos + 1;
  out->align_to = 0;
  out->image_gpu_id = 0;
  out->image_width = 0;
  out->image_height = 0;
  out->image_hmargin = 0;
  out->image_vmargin = 0;
  out->image_ascent = 50;  /* DEFAULT_IMAGE_ASCENT */
  out->display_fg = 0;
  out->display_bg = 0;
  out->raise_factor = 0;
  out->fringe_bitmap_id = 0;
  out->fringe_fg = 0;
  out->fringe_bg = 0;
  out->height_factor = 0;
  out->video_id = 0;
  out->webkit_id = 0;
  out->display_nruns = 0;

  if (!buf)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t zv = BUF_ZV (buf);
  if (charpos >= zv)
    {
      set_buffer_internal_1 (old);
      return 0;
    }

  Lisp_Object pos = make_fixnum (charpos);
  Lisp_Object display_prop = Fget_char_property (pos, Qdisplay, Qnil);

  if (NILP (display_prop))
    {
      /* No display property. Find next change so caller knows when to
         re-check. */
      Lisp_Object limit = make_fixnum (zv);
      Lisp_Object next
          = Fnext_single_char_property_change (pos, Qdisplay, Qnil, limit);
      out->covers_to = FIXNUMP (next) ? XFIXNUM (next) : zv;
      set_buffer_internal_1 (old);
      return 0;
    }

  /* Find end of this display property region. */
  Lisp_Object limit = make_fixnum (zv);
  Lisp_Object next_change
      = Fnext_single_char_property_change (pos, Qdisplay, Qnil, limit);
  out->covers_to = FIXNUMP (next_change) ? XFIXNUM (next_change) : zv;

  if (STRINGP (display_prop))
    {
      /* String replacement: 'display "text" or 'display #("text" 0 4 (face foo)) */
      ptrdiff_t len = SBYTES (display_prop);
      ptrdiff_t copy_len = len < str_buf_len - 1 ? len : str_buf_len - 1;
      memcpy (str_buf, SDATA (display_prop), copy_len);
      str_buf[copy_len] = 0;
      out->type = 1;
      out->str_len = (int) copy_len;

      /* Extract per-character face runs from display string text
         properties.  Face runs are stored after the string text in
         str_buf, 10 bytes each: u16 byte_offset + u32 fg + u32 bg.
         This enables propertized display strings like
         #("text" 0 2 (face bold) 2 4 (face italic)).  */
      if (SCHARS (display_prop) > 0
          && string_intervals (display_prop)
          && copy_len + 10 <= str_buf_len)
        {
          struct window *sw = window_ptr
            ? (struct window *) window_ptr : NULL;
          struct frame *sf = sw ? XFRAME (sw->frame) : NULL;
          if (sf)
            {
              ptrdiff_t fcharpos = 0;
              ptrdiff_t nchars = SCHARS (display_prop);
              ptrdiff_t run_offset = copy_len;
              int nruns = 0;
              int max_runs = (int) ((str_buf_len - copy_len) / 10);
              uint32_t prev_fg = 0xFFFFFFFF;
              uint32_t prev_bg = 0xFFFFFFFF;

              while (fcharpos < nchars && nruns < max_runs)
                {
                  Lisp_Object face_prop
                    = Fget_text_property (make_fixnum (fcharpos),
                                          Qface, display_prop);
                  Lisp_Object next_change
                    = Fnext_single_property_change (
                        make_fixnum (fcharpos), Qface,
                        display_prop, Qnil);
                  ptrdiff_t next_pos = NILP (next_change)
                    ? nchars : XFIXNUM (next_change);

                  uint32_t fg = 0, bg = 0;
                  if (!NILP (face_prop))
                    {
                      int rid = lookup_named_face (
                          sw, sf,
                          SYMBOLP (face_prop) ? face_prop : Qdefault,
                          false);
                      if (rid >= 0)
                        {
                          struct face *rf
                            = FACE_FROM_ID_OR_NULL (sf, rid);
                          if (rf)
                            {
                              unsigned long c = rf->foreground;
                              if (rf->foreground_defaulted_p)
                                c = FRAME_FOREGROUND_PIXEL (sf);
                              fg = ((RED_FROM_ULONG (c) << 16)
                                    | (GREEN_FROM_ULONG (c) << 8)
                                    | BLUE_FROM_ULONG (c));
                              c = rf->background;
                              if (rf->background_defaulted_p)
                                c = FRAME_BACKGROUND_PIXEL (sf);
                              bg = ((RED_FROM_ULONG (c) << 16)
                                    | (GREEN_FROM_ULONG (c) << 8)
                                    | BLUE_FROM_ULONG (c));
                            }
                        }
                    }

                  if (fg != prev_fg || bg != prev_bg)
                    {
                      ptrdiff_t byte_off
                        = string_char_to_byte (display_prop,
                                                fcharpos);
                      uint16_t boff = (uint16_t) byte_off;
                      memcpy (str_buf + run_offset, &boff, 2);
                      memcpy (str_buf + run_offset + 2, &fg, 4);
                      memcpy (str_buf + run_offset + 6, &bg, 4);
                      run_offset += 10;
                      nruns++;
                      prev_fg = fg;
                      prev_bg = bg;
                    }
                  fcharpos = next_pos;
                }
              out->display_nruns = nruns;

              /* Also set display_fg/bg from first face for backward
                 compat (single-face fallback).  */
              if (nruns > 0)
                {
                  uint32_t first_fg, first_bg;
                  memcpy (&first_fg, str_buf + copy_len + 2, 4);
                  memcpy (&first_bg, str_buf + copy_len + 6, 4);
                  if (first_fg != 0)
                    out->display_fg = first_fg;
                  if (first_bg != 0)
                    out->display_bg = first_bg;
                }
            }
        }
      else if (SCHARS (display_prop) > 0)
        {
          /* No intervals — check single face at position 0.  */
          Lisp_Object face_prop = Fget_text_property (
              make_fixnum (0), Qface, display_prop);
          if (!NILP (face_prop) && SYMBOLP (face_prop))
            {
              struct window *sw = window_ptr
                ? (struct window *) window_ptr : NULL;
              struct frame *sf = sw ? XFRAME (sw->frame) : NULL;
              if (sf)
                {
                  int face_id
                    = lookup_named_face (sw, sf, face_prop, false);
                  struct face *face
                    = FACE_FROM_ID_OR_NULL (sf, face_id);
                  if (face)
                    {
                      unsigned long fg = face->foreground;
                      unsigned long bg = face->background;
                      if (face->foreground_defaulted_p)
                        fg = FRAME_FOREGROUND_PIXEL (sf);
                      if (face->background_defaulted_p)
                        bg = FRAME_BACKGROUND_PIXEL (sf);
                      out->display_fg
                        = ((RED_FROM_ULONG (fg) << 16)
                           | (GREEN_FROM_ULONG (fg) << 8)
                           | BLUE_FROM_ULONG (fg));
                      out->display_bg
                        = ((RED_FROM_ULONG (bg) << 16)
                           | (GREEN_FROM_ULONG (bg) << 8)
                           | BLUE_FROM_ULONG (bg));
                    }
                }
            }
        }

      set_buffer_internal_1 (old);
      return 0;
    }

  if (CONSP (display_prop))
    {
      Lisp_Object car = XCAR (display_prop);

      /* Check for (when CONDITION . SPEC) conditional display */
      if (EQ (car, Qwhen) && CONSP (XCDR (display_prop)))
        {
          Lisp_Object condition = XCAR (XCDR (display_prop));
          Lisp_Object rest = XCDR (XCDR (display_prop));

          /* Evaluate condition in a safe way */
          Lisp_Object result = safe_eval (condition);
          if (!NILP (result))
            {
              /* Condition true — recursively process remaining spec.
                 The spec is (SPEC) or SPEC, depending on form. */
              if (CONSP (rest))
                {
                  Lisp_Object inner_spec = XCAR (rest);
                  /* Re-check with inner spec as if it were the
                     display property */
                  if (STRINGP (inner_spec))
                    {
                      ptrdiff_t len = SBYTES (inner_spec);
                      ptrdiff_t copy_len
                        = len < str_buf_len - 1
                          ? len : str_buf_len - 1;
                      memcpy (str_buf, SDATA (inner_spec),
                              copy_len);
                      str_buf[copy_len] = 0;
                      out->type = 1;
                      out->str_len = (int) copy_len;
                      set_buffer_internal_1 (old);
                      return 0;
                    }
                }
            }
          /* Condition false or no spec — skip */
          set_buffer_internal_1 (old);
          return 0;
        }

      if (EQ (car, Qspace))
        {
          Lisp_Object plist = XCDR (display_prop);

          /* Check :align-to first */
          Lisp_Object align_val = Fplist_get (plist, QCalign_to, Qnil);
          if (FIXNUMP (align_val))
            {
              out->align_to = (float) XFIXNUM (align_val);
              out->type = 3;
              set_buffer_internal_1 (old);
              return 0;
            }
          else if (FLOATP (align_val))
            {
              out->align_to = (float) XFLOAT_DATA (align_val);
              out->type = 3;
              set_buffer_internal_1 (old);
              return 0;
            }
          else if (CONSP (align_val))
            {
              /* (N) or (N . pixel) — pixel-based align-to */
              Lisp_Object n = XCAR (align_val);
              float pixel_pos = 0;
              if (FIXNUMP (n))
                pixel_pos = (float) XFIXNUM (n);
              else if (FLOATP (n))
                pixel_pos = (float) XFLOAT_DATA (n);
              struct window *sw = window_ptr ? (struct window *) window_ptr : NULL;
              float col_w = sw ? (float) FRAME_COLUMN_WIDTH (XFRAME (WINDOW_FRAME (sw)))
                               : 8.0f;
              if (col_w > 0)
                out->align_to = pixel_pos / col_w;
              else
                out->align_to = 0;
              out->type = 3;
              set_buffer_internal_1 (old);
              return 0;
            }

          /* Check :relative-width first — width relative to
             the character at this position (typically 1 column) */
          Lisp_Object relw = Fplist_get (plist, QCrelative_width, Qnil);
          if (!NILP (relw))
            {
              float factor = 1.0;
              if (FIXNUMP (relw))
                factor = (float) XFIXNUM (relw);
              else if (FLOATP (relw))
                factor = (float) XFLOAT_DATA (relw);
              /* In monospace, relative-width 1 = 1 column */
              out->space_width = factor;
              out->type = 2;
              set_buffer_internal_1 (old);
              return 0;
            }

          /* Space spec: (space :width N) or (space :width (N)) for pixels */
          Lisp_Object width_val = Fplist_get (plist, QCwidth, Qnil);
          if (FIXNUMP (width_val))
            out->space_width = (float) XFIXNUM (width_val);
          else if (FLOATP (width_val))
            out->space_width = (float) XFLOAT_DATA (width_val);
          else if (CONSP (width_val))
            {
              /* (N) or (N . pixel) — treat car as pixel width,
                 convert to column-equivalent */
              Lisp_Object n = XCAR (width_val);
              float pixel_w = 0;
              if (FIXNUMP (n))
                pixel_w = (float) XFIXNUM (n);
              else if (FLOATP (n))
                pixel_w = (float) XFLOAT_DATA (n);
              struct window *sw = window_ptr ? (struct window *) window_ptr : NULL;
              float col_w = sw ? (float) FRAME_COLUMN_WIDTH (XFRAME (WINDOW_FRAME (sw)))
                               : 8.0f;
              if (col_w > 0)
                out->space_width = pixel_w / col_w;
              else
                out->space_width = 1.0;
            }
          else
            out->space_width = 1.0;

          /* Space height: (space :height N) or (space :height (N)) */
          Lisp_Object height_val = Fplist_get (plist, QCheight, Qnil);
          if (FIXNUMP (height_val))
            {
              struct window *sw = window_ptr
                ? (struct window *) window_ptr : NULL;
              float line_h = sw
                ? (float) FRAME_LINE_HEIGHT (XFRAME (WINDOW_FRAME (sw)))
                : 16.0f;
              out->space_height = (float) XFIXNUM (height_val) * line_h;
            }
          else if (FLOATP (height_val))
            {
              struct window *sw = window_ptr
                ? (struct window *) window_ptr : NULL;
              float line_h = sw
                ? (float) FRAME_LINE_HEIGHT (XFRAME (WINDOW_FRAME (sw)))
                : 16.0f;
              out->space_height = (float) XFLOAT_DATA (height_val) * line_h;
            }
          else if (CONSP (height_val))
            {
              Lisp_Object n = XCAR (height_val);
              if (FIXNUMP (n))
                out->space_height = (float) XFIXNUM (n);
              else if (FLOATP (n))
                out->space_height = (float) XFLOAT_DATA (n);
            }
          /* else space_height stays 0 = use default char_h */

          out->type = 2;
          set_buffer_internal_1 (old);
          return 0;
        }

      /* Check for (raise FACTOR) display property */
      if (EQ (car, Qraise) && CONSP (XCDR (display_prop)))
        {
          Lisp_Object factor = XCAR (XCDR (display_prop));
          if (FIXNUMP (factor))
            out->raise_factor = (float) XFIXNUM (factor);
          else if (FLOATP (factor))
            out->raise_factor = (float) XFLOAT_DATA (factor);
          out->type = 5;
          /* Raise doesn't replace text, so covers_to stays as next change */
          set_buffer_internal_1 (old);
          return 0;
        }

      /* Check for (height FACTOR) display property — font scaling */
      if (EQ (car, Qheight) && CONSP (XCDR (display_prop)))
        {
          Lisp_Object factor = XCAR (XCDR (display_prop));
          if (FIXNUMP (factor))
            out->height_factor = (float) XFIXNUM (factor);
          else if (FLOATP (factor))
            out->height_factor = (float) XFLOAT_DATA (factor);
          /* Height is a non-replacing spec that modifies text;
             use type 8 to signal font scaling. */
          out->type = 8;
          set_buffer_internal_1 (old);
          return 0;
        }

      /* Check for (left-fringe BITMAP [FACE]) or
         (right-fringe BITMAP [FACE]) display property */
      if ((EQ (car, Qleft_fringe)
           || EQ (car, Qright_fringe))
          && CONSP (XCDR (display_prop)))
        {
          Lisp_Object bitmap_spec = XCAR (XCDR (display_prop));
          int bitmap_id = lookup_fringe_bitmap (bitmap_spec);
          if (bitmap_id > 0)
            {
              out->type = EQ (car, Qleft_fringe) ? 6 : 7;
              out->fringe_bitmap_id = bitmap_id;

              /* Resolve fringe face colors */
              struct window *sw
                = window_ptr ? (struct window *) window_ptr : NULL;
              struct frame *sf
                = sw ? XFRAME (sw->frame) : NULL;
              if (sf)
                {
                  int fface_id = FRINGE_FACE_ID;
                  /* Check for optional FACE argument */
                  Lisp_Object rest = XCDR (XCDR (display_prop));
                  if (CONSP (rest) && SYMBOLP (XCAR (rest)))
                    {
                      int nid = lookup_named_face (
                          sw, sf, XCAR (rest), false);
                      if (nid >= 0)
                        fface_id = nid;
                    }

                  struct face *fface
                    = FACE_FROM_ID_OR_NULL (sf, fface_id);
                  if (fface)
                    {
                      unsigned long fg = fface->foreground;
                      unsigned long bg = fface->background;
                      if (fface->foreground_defaulted_p)
                        fg = FRAME_FOREGROUND_PIXEL (sf);
                      if (fface->background_defaulted_p)
                        bg = FRAME_BACKGROUND_PIXEL (sf);
                      out->fringe_fg
                        = ((RED_FROM_ULONG (fg) << 16)
                           | (GREEN_FROM_ULONG (fg) << 8)
                           | BLUE_FROM_ULONG (fg));
                      out->fringe_bg
                        = ((RED_FROM_ULONG (bg) << 16)
                           | (GREEN_FROM_ULONG (bg) << 8)
                           | BLUE_FROM_ULONG (bg));
                    }
                }
              set_buffer_internal_1 (old);
              return 0;
            }
        }

      /* Check for (image ...) display property */
      if (EQ (car, Qimage) && valid_image_p (display_prop))
        {
          struct window *w = window_ptr ? (struct window *) window_ptr : NULL;
          struct frame *f = w ? XFRAME (WINDOW_FRAME (w)) : NULL;
          if (f)
            {
              struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
              if (dpyinfo && dpyinfo->display_handle)
                {
                  /* Get face for this position (needed by lookup_image for fg/bg) */
                  int face_id = DEFAULT_FACE_ID;
                  if (w)
                    {
                      face_id = lookup_named_face (w, f, Qdefault, false);
                      if (face_id < 0)
                        face_id = DEFAULT_FACE_ID;
                    }

                  ptrdiff_t img_id = lookup_image (f, display_prop, face_id);
                  if (img_id >= 0)
                    {
                      struct image *img = IMAGE_FROM_ID (f, img_id);
                      if (img)
                        {
                          prepare_image_for_display (f, img);
                          uint32_t gpu_id = neomacs_get_or_load_image (dpyinfo, img);
                          if (gpu_id != 0)
                            {
                              out->type = 4;
                              out->image_gpu_id = gpu_id;
                              out->image_width = img->width > 0 ? img->width : 100;
                              out->image_height = img->height > 0 ? img->height : 100;
                              out->image_hmargin = img->hmargin;
                              out->image_vmargin = img->vmargin;
                              out->image_ascent = img->ascent;
                              set_buffer_internal_1 (old);
                              return 0;
                            }
                        }
                    }
                }
            }
          set_buffer_internal_1 (old);
          return 0;
        }

      /* Check for (video :id N :width W :height H) display property */
      if (EQ (car, Qvideo))
        {
          Lisp_Object plist = XCDR (display_prop);
          Lisp_Object id_val = Fplist_get (plist, QCid, Qnil);
          Lisp_Object w_val = Fplist_get (plist, QCwidth, Qnil);
          Lisp_Object h_val = Fplist_get (plist, QCheight, Qnil);
          if (FIXNUMP (id_val))
            {
              out->type = 9;
              out->video_id = (uint32_t) XFIXNUM (id_val);
              out->image_width = FIXNUMP (w_val) ? (int) XFIXNUM (w_val) : 640;
              out->image_height = FIXNUMP (h_val) ? (int) XFIXNUM (h_val) : 360;
            }
          set_buffer_internal_1 (old);
          return 0;
        }

      /* Check for (webkit :id N :width W :height H) display property */
      if (EQ (car, Qwebkit))
        {
          Lisp_Object plist = XCDR (display_prop);
          Lisp_Object id_val = Fplist_get (plist, QCid, Qnil);
          Lisp_Object w_val = Fplist_get (plist, QCwidth, Qnil);
          Lisp_Object h_val = Fplist_get (plist, QCheight, Qnil);
          if (FIXNUMP (id_val))
            {
              out->type = 10;
              out->webkit_id = (uint32_t) XFIXNUM (id_val);
              out->image_width = FIXNUMP (w_val) ? (int) XFIXNUM (w_val) : 800;
              out->image_height = FIXNUMP (h_val) ? (int) XFIXNUM (h_val) : 600;
            }
          set_buffer_internal_1 (old);
          return 0;
        }

      /* If car is not a known keyword, treat as list of specs.
         Each element is a single spec like (raise 0.3) or
         (height 0.7). Process all non-replacing specs, stop
         at first replacing spec (string/image/space). */
      if (!EQ (car, Qspace) && !EQ (car, Qimage) && !EQ (car, Qraise)
          && !EQ (car, Qleft_fringe) && !EQ (car, Qright_fringe)
          && !EQ (car, Qvideo) && !EQ (car, Qwebkit)
          && !NILP (car)
          && !(CONSP (car) && EQ (XCAR (car), Qmargin)))
        {
          Lisp_Object tail;
          for (tail = display_prop; CONSP (tail); tail = XCDR (tail))
            {
              Lisp_Object spec = XCAR (tail);

              /* String replacement in list form */
              if (STRINGP (spec))
                {
                  ptrdiff_t len = SBYTES (spec);
                  ptrdiff_t copy_len
                    = len < str_buf_len - 1 ? len : str_buf_len - 1;
                  memcpy (str_buf, SDATA (spec), copy_len);
                  str_buf[copy_len] = 0;
                  out->type = 1;
                  out->str_len = (int) copy_len;
                  set_buffer_internal_1 (old);
                  return 0;
                }

              if (!CONSP (spec))
                continue;

              Lisp_Object scar = XCAR (spec);

              /* (raise FACTOR) — accumulate */
              if (EQ (scar, Qraise) && CONSP (XCDR (spec)))
                {
                  Lisp_Object factor = XCAR (XCDR (spec));
                  if (FIXNUMP (factor))
                    out->raise_factor += (float) XFIXNUM (factor);
                  else if (FLOATP (factor))
                    out->raise_factor += (float) XFLOAT_DATA (factor);
                  continue;
                }

              /* (height FACTOR) — font scaling */
              if (EQ (scar, Qheight) && CONSP (XCDR (spec)))
                {
                  Lisp_Object factor = XCAR (XCDR (spec));
                  if (FIXNUMP (factor))
                    out->height_factor = (float) XFIXNUM (factor);
                  else if (FLOATP (factor))
                    out->height_factor = (float) XFLOAT_DATA (factor);
                  continue;
                }

              /* (space ...) — replacing spec, stop.
                 Extract basic :width for the space. */
              if (EQ (scar, Qspace))
                {
                  Lisp_Object plist = XCDR (spec);
                  Lisp_Object width_val
                    = Fplist_get (plist, QCwidth, Qnil);
                  if (FIXNUMP (width_val))
                    out->space_width
                      = (float) XFIXNUM (width_val);
                  else if (FLOATP (width_val))
                    out->space_width
                      = (float) XFLOAT_DATA (width_val);
                  else
                    out->space_width = 1.0;
                  out->type = 2;
                  set_buffer_internal_1 (old);
                  return 0;
                }
            }
          /* Processed all list elements.
             If any non-replacing effects found, use type 5
             (raise) — the Rust engine checks both raise_factor
             and height_factor regardless of type. */
          if (out->raise_factor != 0.0
              || out->height_factor != 0.0)
            {
              out->type = 5;
              set_buffer_internal_1 (old);
              return 0;
            }
          set_buffer_internal_1 (old);
          return 0;
        }
    }

  /* Vector of display specs: process each element */
  if (VECTORP (display_prop))
    {
      ptrdiff_t vlen = ASIZE (display_prop);
      for (ptrdiff_t vi = 0; vi < vlen; vi++)
        {
          Lisp_Object spec = AREF (display_prop, vi);
          if (!CONSP (spec))
            continue;
          Lisp_Object scar = XCAR (spec);

          if (EQ (scar, Qraise) && CONSP (XCDR (spec)))
            {
              Lisp_Object factor = XCAR (XCDR (spec));
              if (FIXNUMP (factor))
                out->raise_factor += (float) XFIXNUM (factor);
              else if (FLOATP (factor))
                out->raise_factor += (float) XFLOAT_DATA (factor);
            }
          else if (EQ (scar, Qheight) && CONSP (XCDR (spec)))
            {
              Lisp_Object factor = XCAR (XCDR (spec));
              if (FIXNUMP (factor))
                out->height_factor = (float) XFIXNUM (factor);
              else if (FLOATP (factor))
                out->height_factor = (float) XFLOAT_DATA (factor);
            }
        }
      if (out->raise_factor != 0.0 || out->height_factor != 0.0)
        {
          out->type = 5;
          set_buffer_internal_1 (old);
          return 0;
        }
    }

  /* Unrecognized display property form — treat as no display prop */
  set_buffer_internal_1 (old);
  return 0;
}

/* Collect overlay before-string and after-string at a position.
   Before-strings are from overlays that START at charpos.
   After-strings are from overlays that END at charpos.
   Writes concatenated strings into the provided buffers.
   Returns 0 on success. */
int
neomacs_layout_overlay_strings_at (void *buffer_ptr, void *window_ptr,
                                   int64_t charpos,
                                   uint8_t *before_buf, int before_buf_len,
                                   int *before_len_out,
                                   uint8_t *after_buf, int after_buf_len,
                                   int *after_len_out,
                                   void *before_face_out,
                                   void *after_face_out,
                                   int *before_nruns_out,
                                   int *after_nruns_out)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  struct window *w = window_ptr ? (struct window *) window_ptr : NULL;

  *before_len_out = 0;
  *after_len_out = 0;
  *before_nruns_out = 0;
  *after_nruns_out = 0;

  if (!buf)
    return -1;

  struct frame *f = w ? XFRAME (w->frame) : NULL;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t pos = (ptrdiff_t) charpos;

  /* Get all overlays at this position. */
  ptrdiff_t len = 16;
  Lisp_Object *overlay_vec = xmalloc (len * sizeof *overlay_vec);
  ptrdiff_t noverlays = overlays_at (pos, true, &overlay_vec, &len, NULL);



  int before_offset = 0;
  int after_offset = 0;
  bool before_face_set = false;
  bool after_face_set = false;

  for (ptrdiff_t i = 0; i < noverlays; i++)
    {
      Lisp_Object overlay = overlay_vec[i];
      if (!OVERLAYP (overlay))
        continue;

      /* Filter window-specific overlays. */
      Lisp_Object owin = Foverlay_get (overlay, Qwindow);
      if (WINDOWP (owin) && w && XWINDOW (owin) != w)
        continue;

      ptrdiff_t ostart = OVERLAY_START (overlay);
      ptrdiff_t oend = OVERLAY_END (overlay);

      /* Before-string: render at overlay start. */
      if (ostart == pos)
        {
          Lisp_Object bstr = Foverlay_get (overlay, Qbefore_string);
          if (STRINGP (bstr))
            {
              ptrdiff_t slen = SBYTES (bstr);
              ptrdiff_t copy = slen;
              if (before_offset + copy > before_buf_len - 1)
                copy = before_buf_len - 1 - before_offset;
              if (copy > 0)
                {
                  memcpy (before_buf + before_offset, SDATA (bstr), copy);
                  before_offset += (int) copy;
                }

              /* Extract overlay face for before-string. */
              if (!before_face_set && f && before_face_out)
                {
                  Lisp_Object oface = Foverlay_get (overlay, Qface);
                  if (!NILP (oface) && SYMBOLP (oface))
                    {
                      int face_id = lookup_named_face (w, f, oface, false);
                      struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
                      if (face)
                        {
                          fill_face_data (f, face,
                                          (struct FaceDataFFI *) before_face_out);
                          before_face_set = true;
                        }
                    }
                }

              /* Extract per-character face runs from before-string text
                 properties.  Face runs are stored after the string text in
                 before_buf, 10 bytes each: u16 byte_offset + u32 fg + u32 bg. */
              if (SCHARS (bstr) > 0
                  && string_intervals (bstr)
                  && f && copy > 0
                  && copy + 10 <= before_buf_len)
                {
                  ptrdiff_t fcharpos = 0;
                  ptrdiff_t nchars = SCHARS (bstr);
                  ptrdiff_t run_offset = before_offset;
                  int nruns = 0;
                  int max_runs = (int) ((before_buf_len - before_offset) / 10);
                  uint32_t prev_fg = 0xFFFFFFFF;
                  uint32_t prev_bg = 0xFFFFFFFF;

                  while (fcharpos < nchars && nruns < max_runs)
                    {
                      ptrdiff_t endpos;
                      int fid = face_at_string_position (w, bstr, fcharpos,
                                                         0, &endpos,
                                                         DEFAULT_FACE_ID,
                                                         false, 0);
                      struct face *rf = FACE_FROM_ID_OR_NULL (f, fid);

                      uint32_t fg = 0, bg = 0;
                      if (rf)
                        {
                          unsigned long c = rf->foreground;
                          if (rf->foreground_defaulted_p)
                            c = FRAME_FOREGROUND_PIXEL (f);
                          fg = ((RED_FROM_ULONG (c) << 16)
                                | (GREEN_FROM_ULONG (c) << 8)
                                | BLUE_FROM_ULONG (c));
                          c = rf->background;
                          if (rf->background_defaulted_p)
                            c = FRAME_BACKGROUND_PIXEL (f);
                          bg = ((RED_FROM_ULONG (c) << 16)
                                | (GREEN_FROM_ULONG (c) << 8)
                                | BLUE_FROM_ULONG (c));
                        }

                      if (fg != prev_fg || bg != prev_bg)
                        {
                          ptrdiff_t byte_off
                            = string_char_to_byte (bstr, fcharpos);
                          uint16_t boff = (uint16_t) byte_off;
                          memcpy (before_buf + run_offset, &boff, 2);
                          memcpy (before_buf + run_offset + 2, &fg, 4);
                          memcpy (before_buf + run_offset + 6, &bg, 4);
                          run_offset += 10;
                          nruns++;
                          prev_fg = fg;
                          prev_bg = bg;
                        }
                      if (endpos < 0)
                        fcharpos = nchars;  /* constant to end */
                      else
                        fcharpos = endpos > fcharpos ? endpos : fcharpos + 1;
                    }
                  *before_nruns_out = nruns;
                }
            }
        }

      /* After-string: render at overlay end.
         Overlay intervals are half-open [start, end), so the after-string
         belongs after the last character inside the overlay at pos == end - 1.
         Also handle zero-width overlays where ostart == oend == pos.  */
      if (oend == pos + 1 || (ostart == pos && oend == pos))
        {
          Lisp_Object astr = Foverlay_get (overlay, Qafter_string);
          if (STRINGP (astr))
            {
              ptrdiff_t slen = SBYTES (astr);
              ptrdiff_t copy = slen;
              if (after_offset + copy > after_buf_len - 1)
                copy = after_buf_len - 1 - after_offset;
              if (copy > 0)
                {
                  memcpy (after_buf + after_offset, SDATA (astr), copy);
                  after_offset += (int) copy;
                }

              /* Extract overlay face for after-string. */
              if (!after_face_set && f && after_face_out)
                {
                  Lisp_Object oface = Foverlay_get (overlay, Qface);
                  if (!NILP (oface) && SYMBOLP (oface))
                    {
                      int face_id = lookup_named_face (w, f, oface, false);
                      struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);
                      if (face)
                        {
                          fill_face_data (f, face,
                                          (struct FaceDataFFI *) after_face_out);
                          after_face_set = true;
                        }
                    }
                }

              /* Extract per-character face runs from after-string text
                 properties.  Face runs are stored after the string text in
                 after_buf, 10 bytes each: u16 byte_offset + u32 fg + u32 bg. */
              if (SCHARS (astr) > 0
                  && string_intervals (astr)
                  && f && copy > 0
                  && copy + 10 <= after_buf_len)
                {
                  ptrdiff_t fcharpos = 0;
                  ptrdiff_t nchars = SCHARS (astr);
                  ptrdiff_t run_offset = after_offset;
                  int nruns = 0;
                  int max_runs = (int) ((after_buf_len - after_offset) / 10);
                  uint32_t prev_fg = 0xFFFFFFFF;
                  uint32_t prev_bg = 0xFFFFFFFF;

                  while (fcharpos < nchars && nruns < max_runs)
                    {
                      ptrdiff_t endpos;
                      int fid = face_at_string_position (w, astr, fcharpos,
                                                         0, &endpos,
                                                         DEFAULT_FACE_ID,
                                                         false, 0);
                      struct face *rf = FACE_FROM_ID_OR_NULL (f, fid);

                      uint32_t fg = 0, bg = 0;
                      if (rf)
                        {
                          unsigned long c = rf->foreground;
                          if (rf->foreground_defaulted_p)
                            c = FRAME_FOREGROUND_PIXEL (f);
                          fg = ((RED_FROM_ULONG (c) << 16)
                                | (GREEN_FROM_ULONG (c) << 8)
                                | BLUE_FROM_ULONG (c));
                          c = rf->background;
                          if (rf->background_defaulted_p)
                            c = FRAME_BACKGROUND_PIXEL (f);
                          bg = ((RED_FROM_ULONG (c) << 16)
                                | (GREEN_FROM_ULONG (c) << 8)
                                | BLUE_FROM_ULONG (c));
                        }

                      if (fg != prev_fg || bg != prev_bg)
                        {
                          ptrdiff_t byte_off
                            = string_char_to_byte (astr, fcharpos);
                          uint16_t boff = (uint16_t) byte_off;
                          memcpy (after_buf + run_offset, &boff, 2);
                          memcpy (after_buf + run_offset + 2, &fg, 4);
                          memcpy (after_buf + run_offset + 6, &bg, 4);
                          run_offset += 10;
                          nruns++;
                          prev_fg = fg;
                          prev_bg = bg;
                        }
                      if (endpos < 0)
                        fcharpos = nchars;  /* constant to end */
                      else
                        fcharpos = endpos > fcharpos ? endpos : fcharpos + 1;
                    }
                  *after_nruns_out = nruns;
                }
            }
        }
    }

  xfree (overlay_vec);

  if (before_offset > 0)
    before_buf[before_offset] = 0;
  if (after_offset > 0)
    after_buf[after_offset] = 0;

  *before_len_out = before_offset;
  *after_len_out = after_offset;

  set_buffer_internal_1 (old);
  return 0;
}

/* Check if a character should be displayed as a glyphless glyph.
   Looks up Vglyphless_char_display char-table.
   Returns 0 on success, -1 on error.
   method_out: 0=normal, 1=thin_space, 2=empty_box, 3=hex_code,
               4=acronym, 5=zero_width.
   For method=4 (acronym), writes the acronym string into str_buf. */
int
neomacs_layout_check_glyphless (void *frame_ptr, int codepoint,
                                int *method_out,
                                uint8_t *str_buf, int str_buf_len,
                                int *str_len_out)
{
  struct frame *f = frame_ptr ? (struct frame *) frame_ptr : NULL;
  *method_out = 0;
  *str_len_out = 0;

  if (!CHAR_TABLE_P (Vglyphless_char_display))
    return 0;

  if (CHAR_TABLE_EXTRA_SLOTS (XCHAR_TABLE (Vglyphless_char_display)) < 1)
    return 0;

  Lisp_Object glyphless_method;
  if (codepoint >= 0)
    glyphless_method = CHAR_TABLE_REF (Vglyphless_char_display, codepoint);
  else
    glyphless_method = XCHAR_TABLE (Vglyphless_char_display)->extras[0];

  /* Handle (GRAPHICAL . TEXT) cons cells */
  if (CONSP (glyphless_method))
    {
      if (f && FRAME_WINDOW_P (f))
        glyphless_method = XCAR (glyphless_method);
      else
        glyphless_method = XCDR (glyphless_method);
    }

  if (NILP (glyphless_method))
    {
      *method_out = 0;
      return 0;
    }

  if (EQ (glyphless_method, Qthin_space))
    *method_out = 1;
  else if (EQ (glyphless_method, Qempty_box))
    *method_out = 2;
  else if (EQ (glyphless_method, Qhex_code))
    *method_out = 3;
  else if (EQ (glyphless_method, Qzero_width))
    *method_out = 5;
  else if (STRINGP (glyphless_method))
    {
      *method_out = 4;
      ptrdiff_t slen = SBYTES (glyphless_method);
      ptrdiff_t copy = slen < str_buf_len - 1 ? slen : str_buf_len - 1;
      memcpy (str_buf, SDATA (glyphless_method), copy);
      str_buf[copy] = 0;
      *str_len_out = (int) copy;
    }
  else
    *method_out = 2; /* unknown method -> empty-box */

  return 0;
}

/* Collect margin overlay strings at a position.
   Checks overlay before/after-strings for display properties with
   (margin left-margin) or (margin right-margin).
   left_buf/right_buf receive the display text for each margin.
   Returns 0 on success. */
int
neomacs_layout_margin_strings_at (void *buffer_ptr, void *window_ptr,
                                  int64_t charpos,
                                  uint8_t *left_buf, int left_buf_len,
                                  int *left_len_out,
                                  uint8_t *right_buf, int right_buf_len,
                                  int *right_len_out)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  struct window *w = window_ptr ? (struct window *) window_ptr : NULL;

  *left_len_out = 0;
  *right_len_out = 0;

  if (!buf)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t pos = (ptrdiff_t) charpos;
  ptrdiff_t len = 16;
  Lisp_Object *overlay_vec = xmalloc (len * sizeof *overlay_vec);
  ptrdiff_t noverlays = overlays_at (pos, true, &overlay_vec, &len, NULL);

  int left_offset = 0;
  int right_offset = 0;

  for (ptrdiff_t i = 0; i < noverlays; i++)
    {
      Lisp_Object overlay = overlay_vec[i];
      if (!OVERLAYP (overlay))
        continue;

      /* Filter window-specific overlays. */
      Lisp_Object owin = Foverlay_get (overlay, Qwindow);
      if (WINDOWP (owin) && w && XWINDOW (owin) != w)
        continue;

      ptrdiff_t ostart = OVERLAY_START (overlay);

      /* Check before-string and after-string for margin display props. */
      Lisp_Object strings[2];
      strings[0] = (ostart == pos)
        ? Foverlay_get (overlay, Qbefore_string) : Qnil;
      strings[1] = Foverlay_get (overlay, Qafter_string);

      for (int si = 0; si < 2; si++)
        {
          Lisp_Object str = strings[si];
          if (!STRINGP (str) || SCHARS (str) == 0)
            continue;

          /* Check the display property on the FIRST character of the string. */
          Lisp_Object disp
            = Fget_text_property (make_fixnum (0), Qdisplay, str);
          if (NILP (disp))
            continue;

          /* Look for ((margin left-margin) ...) or ((margin right-margin) ...) */
          if (!CONSP (disp) || !CONSP (XCAR (disp)))
            continue;

          Lisp_Object margin_spec = XCAR (disp);
          if (!CONSP (margin_spec))
            continue;

          Lisp_Object margin_sym = XCAR (margin_spec);
          if (!EQ (margin_sym, Qmargin))
            continue;

          Lisp_Object location = Qnil;
          if (CONSP (XCDR (margin_spec)))
            location = XCAR (XCDR (margin_spec));

          /* Get the display value (second element of outer cons). */
          Lisp_Object display_val = XCDR (disp);
          if (CONSP (display_val))
            display_val = XCAR (display_val);

          /* Extract text content. */
          const char *text_data = NULL;
          ptrdiff_t text_len = 0;

          if (STRINGP (display_val))
            {
              text_data = (const char *) SDATA (display_val);
              text_len = SBYTES (display_val);
            }
          else if (STRINGP (str))
            {
              /* Fall back to the overlay string itself. */
              text_data = (const char *) SDATA (str);
              text_len = SBYTES (str);
            }

          if (!text_data || text_len <= 0)
            continue;

          if (EQ (location, Qleft_margin))
            {
              ptrdiff_t copy = text_len;
              if (left_offset + copy > left_buf_len - 1)
                copy = left_buf_len - 1 - left_offset;
              if (copy > 0)
                {
                  memcpy (left_buf + left_offset, text_data, copy);
                  left_offset += (int) copy;
                }
            }
          else if (EQ (location, Qright_margin))
            {
              ptrdiff_t copy = text_len;
              if (right_offset + copy > right_buf_len - 1)
                copy = right_buf_len - 1 - right_offset;
              if (copy > 0)
                {
                  memcpy (right_buf + right_offset, text_data, copy);
                  right_offset += (int) copy;
                }
            }
        }
    }

  xfree (overlay_vec);

  if (left_offset > 0)
    left_buf[left_offset] = 0;
  if (right_offset > 0)
    right_buf[right_offset] = 0;

  *left_len_out = left_offset;
  *right_len_out = right_offset;

  set_buffer_internal_1 (old);
  return 0;
}

/* Check line-height and line-spacing text properties at CHARPOS.
   Returns extra height (in pixels) beyond BASE_HEIGHT that should
   be added to the row containing this position.
   line-height can be: integer (pixel height), float (multiplier).
   line-spacing can be: integer (extra pixels), float (fraction of height). */
int
neomacs_layout_check_line_spacing (void *buffer_ptr, void *window_ptr,
                                   int64_t charpos, float base_height,
                                   float *extra_height_out)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  struct window *w = (struct window *) window_ptr;
  if (!buf || !w || !extra_height_out)
    return -1;

  struct buffer *old = current_buffer;
  set_buffer_internal_1 (buf);

  *extra_height_out = 0.0f;

  ptrdiff_t zv = BUF_ZV (buf);
  if (charpos < BUF_BEGV (buf) || charpos >= zv)
    {
      set_buffer_internal_1 (old);
      return 0;
    }

  Lisp_Object pos = make_fixnum (charpos);

  /* Check line-height text property. */
  Lisp_Object line_height = Fget_char_property (pos, Qline_height, Qnil);
  if (!NILP (line_height))
    {
      if (FIXNUMP (line_height))
        {
          int h = XFIXNUM (line_height);
          if ((float) h > base_height)
            *extra_height_out += (float) h - base_height;
        }
      else if (FLOATP (line_height))
        {
          float h = (float) (XFLOAT_DATA (line_height) * (double) base_height);
          if (h > base_height)
            *extra_height_out += h - base_height;
        }
    }

  /* Check line-spacing text property. */
  Lisp_Object line_spacing = Fget_char_property (pos, Qline_spacing, Qnil);
  if (!NILP (line_spacing))
    {
      if (FIXNUMP (line_spacing))
        {
          int s = XFIXNUM (line_spacing);
          if (s > 0)
            *extra_height_out += (float) s;
        }
      else if (FLOATP (line_spacing))
        {
          float s = (float) (XFLOAT_DATA (line_spacing) * (double) base_height);
          if (s > 0.0f)
            *extra_height_out += s;
        }
    }

  set_buffer_internal_1 (old);
  return 0;
}

/* Check line-prefix and wrap-prefix text properties at a position.
   Returns the prefix as a space width in columns.
   prefix_type: 0=line-prefix, 1=wrap-prefix
   Returns 0 on success, sets *width_out to the prefix width.
   If no prefix text property, *width_out = -1.0 (use window default). */
int
neomacs_layout_check_line_prefix (void *buffer_ptr, void *window_ptr,
                                  int64_t charpos, int prefix_type,
                                  float *width_out)
{
  struct buffer *buf = (struct buffer *) buffer_ptr;
  struct window *w = (struct window *) window_ptr;
  if (!buf || !w || !width_out)
    return -1;

  *width_out = -1.0f; /* no override */

  struct buffer *old_buf = current_buffer;
  set_buffer_internal_1 (buf);

  ptrdiff_t zv = BUF_ZV (buf);
  if (charpos < BUF_BEGV (buf) || charpos >= zv)
    {
      set_buffer_internal_1 (old_buf);
      return 0;
    }

  Lisp_Object pos = make_fixnum (charpos);
  Lisp_Object prop_sym = prefix_type == 0 ? Qline_prefix : Qwrap_prefix;
  Lisp_Object prefix = Fget_char_property (pos, prop_sym, Qnil);

  if (NILP (prefix))
    {
      set_buffer_internal_1 (old_buf);
      return 0;
    }

  /* Evaluate the prefix spec to get a width in columns. */
  if (STRINGP (prefix))
    {
      /* String prefix: width = string length */
      *width_out = (float) SCHARS (prefix);
    }
  else if (CONSP (prefix) && EQ (XCAR (prefix), Qspace))
    {
      /* (space :width N) or (space :width (N)) */
      Lisp_Object plist = XCDR (prefix);
      Lisp_Object width_val = Fplist_get (plist, QCwidth, Qnil);
      if (FIXNUMP (width_val))
        *width_out = (float) XFIXNUM (width_val);
      else if (FLOATP (width_val))
        *width_out = (float) XFLOAT_DATA (width_val);
      else if (CONSP (width_val))
        {
          /* (N) — pixel width, convert to columns */
          Lisp_Object n = XCAR (width_val);
          float pixel_w = 0;
          if (FIXNUMP (n))
            pixel_w = (float) XFIXNUM (n);
          else if (FLOATP (n))
            pixel_w = (float) XFLOAT_DATA (n);
          float col_w = (float) FRAME_COLUMN_WIDTH (
              XFRAME (WINDOW_FRAME (w)));
          if (col_w > 0)
            *width_out = pixel_w / col_w;
        }
      else
        *width_out = 0;
    }

  set_buffer_internal_1 (old_buf);
  return 0;
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
    Lisp_Object root_window = FRAME_ROOT_WINDOW (f);
    if (!WINDOWP (root_window))
      return;
    struct window *root = XWINDOW (root_window);
    /* Stack-based traversal: find all leaf windows */
#define WINDOW_STACK_SIZE 128
    struct window *stack[WINDOW_STACK_SIZE];
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
                if (sp < WINDOW_STACK_SIZE)
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

  /* The menu-bar, tool-bar, and tab-bar windows are pseudo-windows NOT
     part of the root window tree.  Extract them separately so their
     content (menu items, tool bar buttons, tab-bar items) is rendered. */
  if (WINDOWP (f->menu_bar_window))
    {
      struct window *mbw = XWINDOW (f->menu_bar_window);
      if (mbw->current_matrix)
        neomacs_extract_window_glyphs (mbw, NULL);
    }

  if (WINDOWP (f->tool_bar_window))
    {
      struct window *tbw = XWINDOW (f->tool_bar_window);
      if (tbw->current_matrix)
        neomacs_extract_window_glyphs (tbw, NULL);
    }

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
      struct window *bstack[WINDOW_STACK_SIZE];
      int bsp = 0;
      if (WINDOWP (FRAME_ROOT_WINDOW (f)))
        bstack[bsp++] = XWINDOW (FRAME_ROOT_WINDOW (f));

      while (bsp > 0)
        {
          struct window *w = bstack[--bsp];
          if (WINDOWP (w->contents))
            {
              struct window *child = XWINDOW (w->contents);
              while (child)
                {
                  if (bsp < WINDOW_STACK_SIZE)
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

  /* Render GPU scroll bars for all active scroll bars on the frame.
     Walk the frame's scroll bar list and emit FrameGlyph::ScrollBar
     for each one. */
  {
    Lisp_Object bar;
    struct face *sb_face = FACE_FROM_ID_OR_NULL (f, SCROLL_BAR_FACE_ID);
    /* Scroll bar face colors: background for track, foreground for thumb */
    unsigned long track_pixel = sb_face ? sb_face->background
      : FRAME_BACKGROUND_PIXEL (f);
    unsigned long thumb_pixel = sb_face ? sb_face->foreground
      : 0x808080UL;  /* gray fallback */
    uint32_t track_rgb = ((RED_FROM_ULONG (track_pixel) << 16)
                          | (GREEN_FROM_ULONG (track_pixel) << 8)
                          | BLUE_FROM_ULONG (track_pixel));
    uint32_t thumb_rgb = ((RED_FROM_ULONG (thumb_pixel) << 16)
                          | (GREEN_FROM_ULONG (thumb_pixel) << 8)
                          | BLUE_FROM_ULONG (thumb_pixel));

    for (bar = FRAME_SCROLL_BARS (f); !NILP (bar);
         bar = XSCROLL_BAR (bar)->next)
      {
        struct scroll_bar *b = XSCROLL_BAR (bar);
        int thumb_start = b->start;
        int thumb_size = b->end - b->start;
        if (thumb_size < 1)
          thumb_size = 1;

        neomacs_display_add_scroll_bar (
            dpyinfo->display_handle,
            b->horizontal ? 1 : 0,
            b->left, b->top,
            b->width, b->height,
            thumb_start, thumb_size,
            track_rgb, thumb_rgb);
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
      if (use_rust_display_engine)
        {
          /* NEW PATH: Rust layout engine reads buffer data directly via FFI
             and produces FrameGlyphBuffer internally. */
          struct face *default_face = FACE_FROM_ID_OR_NULL (f, DEFAULT_FACE_ID);
          unsigned long bg_pixel = default_face && !default_face->background_defaulted_p
            ? default_face->background : FRAME_BACKGROUND_PIXEL (f);
          uint32_t bg_rgb = ((RED_FROM_ULONG (bg_pixel) << 16)
                             | (GREEN_FROM_ULONG (bg_pixel) << 8)
                             | BLUE_FROM_ULONG (bg_pixel));

          /* Get vertical border face color */
          struct face *vborder_face = FACE_FROM_ID_OR_NULL (f, VERTICAL_BORDER_FACE_ID);
          unsigned long vb_fg = vborder_face ? vborder_face->foreground : 0;
          uint32_t vb_rgb = ((RED_FROM_ULONG (vb_fg) << 16)
                             | (GREEN_FROM_ULONG (vb_fg) << 8)
                             | BLUE_FROM_ULONG (vb_fg));

          /* Window divider face colors */
          int rdw = FRAME_RIGHT_DIVIDER_WIDTH (f);
          int bdw = FRAME_BOTTOM_DIVIDER_WIDTH (f);
          uint32_t div_fg = vb_rgb, div_first_fg = vb_rgb, div_last_fg = vb_rgb;
          if (rdw > 0 || bdw > 0)
            {
              struct face *div_face
                = FACE_FROM_ID_OR_NULL (f, WINDOW_DIVIDER_FACE_ID);
              struct face *div_first
                = FACE_FROM_ID_OR_NULL (f, WINDOW_DIVIDER_FIRST_PIXEL_FACE_ID);
              struct face *div_last
                = FACE_FROM_ID_OR_NULL (f, WINDOW_DIVIDER_LAST_PIXEL_FACE_ID);
              if (div_face)
                {
                  unsigned long c = div_face->foreground;
                  div_fg = ((RED_FROM_ULONG (c) << 16)
                            | (GREEN_FROM_ULONG (c) << 8)
                            | BLUE_FROM_ULONG (c));
                }
              if (div_first)
                {
                  unsigned long c = div_first->foreground;
                  div_first_fg = ((RED_FROM_ULONG (c) << 16)
                                  | (GREEN_FROM_ULONG (c) << 8)
                                  | BLUE_FROM_ULONG (c));
                }
              else
                div_first_fg = div_fg;
              if (div_last)
                {
                  unsigned long c = div_last->foreground;
                  div_last_fg = ((RED_FROM_ULONG (c) << 16)
                                 | (GREEN_FROM_ULONG (c) << 8)
                                 | BLUE_FROM_ULONG (c));
                }
              else
                div_last_fg = div_fg;
            }

          neomacs_rust_layout_frame (
              dpyinfo->display_handle,
              (void *) f,
              (float) FRAME_PIXEL_WIDTH (f),
              (float) FRAME_PIXEL_HEIGHT (f),
              (float) FRAME_COLUMN_WIDTH (f),
              (float) FRAME_LINE_HEIGHT (f),
              FRAME_FONT (f) ? (float) FRAME_FONT (f)->pixel_size : 14.0f,
              bg_rgb,
              vb_rgb,
              rdw, bdw,
              div_fg, div_first_fg, div_last_fg);

          /* Extract menu bar and tool bar from current_matrix.
             Their desired_matrix was populated by
             neomacs_display_menu_and_tool_bar() in redisplay_internal,
             then copied desired→current by update_menu_bar/update_tool_bar
             in update_frame (dispnew.c).  */
          if (WINDOWP (f->menu_bar_window))
            {
              struct window *mbw = XWINDOW (f->menu_bar_window);
              if (mbw->current_matrix)
                neomacs_extract_window_glyphs (mbw, NULL);
            }
          if (WINDOWP (f->tool_bar_window))
            {
              struct window *tbw = XWINDOW (f->tool_bar_window);
              if (tbw->current_matrix)
                neomacs_extract_window_glyphs (tbw, NULL);
            }
        }
      else
        {
          /* LEGACY PATH: Extract from current_matrix (C display engine). */
          neomacs_extract_full_frame (f);
        }

      /* Signal end of frame to Rust (sends frame to render thread) */
      if (output && output->window_id > 0)
        neomacs_display_end_frame_window (dpyinfo->display_handle, output->window_id);
      else
        neomacs_display_end_frame (dpyinfo->display_handle);
    }

  /* Drain any pending events from the render thread to prevent busy loop.
     During startup, the render thread sends resize events that put data
     on the wakeup pipe.  If we don't drain here, xg_select() will see
     the pipe as readable and return immediately forever, causing 100% CPU.
     This is safe to call here because the event handler only sets flags
     (SET_FRAME_GARBAGED, enqueue events) — no immediate redisplay.  */
  if (dpyinfo && dpyinfo->connection >= 0)
    neomacs_display_wakeup_handler (dpyinfo->connection, dpyinfo);
}

/* Called at the beginning of updating a window */
static void
neomacs_update_window_begin (struct window *w)
{
  struct frame *f = XFRAME (WINDOW_FRAME (w));
  struct neomacs_display_info *dpyinfo;

  if (!FRAME_NEOMACS_P (f))
    return;

  dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

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
  /* Rendering is handled by the Rust render thread — nothing to flush. */
}

/* Set window size (called from set-frame-size and similar) */
static void
neomacs_set_window_size (struct frame *f, bool change_gravity,
                         int width, int height)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!output)
    return;

  block_input ();

  /* Clamp to display dimensions so the window doesn't extend beyond
     the screen.  On a real display the WM would do this; without a WM
     (e.g. Xvfb) the window would grow beyond the screen otherwise.
     We clamp output dimensions and the renderer/winit request, but
     leave FRAME_PIXEL_WIDTH/HEIGHT (set by adjust_frame_size) alone
     so that the Resized event from winit will trigger change_frame_size
     to recalculate correctly.  */
  int req_w = width;
  int req_h = height;
  if (dpyinfo)
    {
      if (req_w > dpyinfo->width)
        req_w = dpyinfo->width;
      if (req_h > dpyinfo->height)
        req_h = dpyinfo->height;
    }

  /* Update frame's pixel dimensions (clamped) */
  output->pixel_width = req_w;
  output->pixel_height = req_h;

  /* Update the glyph buffer dimensions (threaded and GPU widget modes) */
  if (dpyinfo && dpyinfo->display_handle)
    {
      neomacs_display_resize (dpyinfo->display_handle, req_w, req_h);
      neomacs_display_clear_all_glyphs (dpyinfo->display_handle);
      /* Request the winit window to resize */
      neomacs_display_request_size (dpyinfo->display_handle, req_w, req_h);
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

/* Create a scroll bar pseudovector for window W */
static struct scroll_bar *
neomacs_scroll_bar_create (struct window *w, int top, int left,
                           int width, int height, bool horizontal)
{
  struct frame *f = XFRAME (w->frame);
  struct scroll_bar *bar
    = ALLOCATE_PSEUDOVECTOR (struct scroll_bar, prev, PVEC_OTHER);
  Lisp_Object barobj;

  XSETWINDOW (bar->window, w);
  bar->top = top;
  bar->left = left;
  bar->width = width;
  bar->height = height;
  bar->start = 0;
  bar->end = 0;
  bar->dragging = -1;
  bar->horizontal = horizontal;

  /* Link into frame's scroll bar list */
  bar->next = FRAME_SCROLL_BARS (f);
  bar->prev = Qnil;
  XSETVECTOR (barobj, bar);
  fset_scroll_bars (f, barobj);
  if (!NILP (bar->next))
    XSCROLL_BAR (bar->next)->prev = barobj;

  return bar;
}

/* Set vertical scroll bar for window W */
static void
neomacs_set_vertical_scroll_bar (struct window *w, int portion, int whole,
                                 int position)
{
  Lisp_Object barobj;
  struct scroll_bar *bar;
  int top, height, left, width;
  int window_y, window_height;

  /* Get window dimensions */
  window_box (w, ANY_AREA, 0, &window_y, 0, &window_height);
  top = window_y;
  height = window_height;
  left = WINDOW_SCROLL_BAR_AREA_X (w);
  width = WINDOW_SCROLL_BAR_AREA_WIDTH (w);

  if (NILP (w->vertical_scroll_bar))
    {
      bar = neomacs_scroll_bar_create (w, top, left, width,
                                       max (height, 1), false);
    }
  else
    {
      bar = XSCROLL_BAR (w->vertical_scroll_bar);
      bar->left = left;
      bar->top = top;
      bar->width = width;
      bar->height = height;
    }

  /* Compute thumb position from portion/whole/position.
     portion = visible part of buffer (in buffer positions)
     whole = total buffer size (in buffer positions)
     position = start of visible region (in buffer positions) */
  if (whole > 0 && height > 0)
    {
      int thumb_size = (int) ((double) portion / whole * height);
      int thumb_start = (int) ((double) position / whole * height);
      /* Enforce minimum thumb size */
      if (thumb_size < 10)
        thumb_size = 10;
      /* Clamp */
      if (thumb_start + thumb_size > height)
        thumb_start = height - thumb_size;
      if (thumb_start < 0)
        thumb_start = 0;
      bar->start = thumb_start;
      bar->end = thumb_start + thumb_size;
    }
  else
    {
      bar->start = 0;
      bar->end = height;
    }

  XSETVECTOR (barobj, bar);
  wset_vertical_scroll_bar (w, barobj);
}

/* Set horizontal scroll bar for window W */
static void
neomacs_set_horizontal_scroll_bar (struct window *w, int portion, int whole,
                                   int position)
{
  Lisp_Object barobj;
  struct scroll_bar *bar;
  int top, height, left, width;
  int window_x, window_width;

  /* Get window dimensions */
  window_box (w, ANY_AREA, &window_x, 0, &window_width, 0);
  left = window_x;
  width = window_width;
  top = WINDOW_SCROLL_BAR_AREA_Y (w);
  height = WINDOW_SCROLL_BAR_AREA_HEIGHT (w);

  if (NILP (w->horizontal_scroll_bar))
    {
      bar = neomacs_scroll_bar_create (w, top, left, max (width, 1),
                                       height, true);
    }
  else
    {
      bar = XSCROLL_BAR (w->horizontal_scroll_bar);
      bar->left = left;
      bar->top = top;
      bar->width = width;
      bar->height = height;
    }

  /* Compute thumb position */
  if (whole > 0 && width > 0)
    {
      int thumb_size = (int) ((double) portion / whole * width);
      int thumb_start = (int) ((double) position / whole * width);
      if (thumb_size < 10)
        thumb_size = 10;
      if (thumb_start + thumb_size > width)
        thumb_start = width - thumb_size;
      if (thumb_start < 0)
        thumb_start = 0;
      bar->start = thumb_start;
      bar->end = thumb_start + thumb_size;
    }
  else
    {
      bar->start = 0;
      bar->end = width;
    }

  XSETVECTOR (barobj, bar);
  wset_horizontal_scroll_bar (w, barobj);
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

      /* Clear the window's reference to this scroll bar */
      if (!NILP (b->window))
        {
          struct window *w = XWINDOW (b->window);
          if (!NILP (w->vertical_scroll_bar)
              && XSCROLL_BAR (w->vertical_scroll_bar) == b)
            wset_vertical_scroll_bar (w, Qnil);
          if (!NILP (w->horizontal_scroll_bar)
              && XSCROLL_BAR (w->horizontal_scroll_bar) == b)
            wset_horizontal_scroll_bar (w, Qnil);
        }

      next = b->next;
      b->next = b->prev = Qnil;
    }
}

/* Set default vertical scroll bar width for frame F.
   GPU-rendered scroll bars use a thin 12px width. */
static void
neomacs_set_scroll_bar_default_width (struct frame *f)
{
  int unit = FRAME_COLUMN_WIDTH (f);
  int width = 12;  /* 12px thin scroll bar for GPU rendering */
  FRAME_CONFIG_SCROLL_BAR_COLS (f) = (width + unit - 1) / unit;
  FRAME_CONFIG_SCROLL_BAR_WIDTH (f)
    = FRAME_CONFIG_SCROLL_BAR_COLS (f) * unit;
}

/* Set default horizontal scroll bar height for frame F. */
static void
neomacs_set_scroll_bar_default_height (struct frame *f)
{
  int height = FRAME_LINE_HEIGHT (f);
  int bar_height = 12;  /* 12px thin scroll bar */
  FRAME_CONFIG_SCROLL_BAR_LINES (f) = (bar_height + height - 1) / height;
  FRAME_CONFIG_SCROLL_BAR_HEIGHT (f) = bar_height;
}

/* Return the current position of the mouse.
   *FP should be a frame which indicates which display to ask about.

   If the mouse movement started in a scroll bar, set *BAR_WINDOW to
   the scroll bar's window, *PART to the part the mouse is in, and
   *X and *Y to the position within the scroll bar.

   Otherwise, set *BAR_WINDOW to Qnil, *PART to scroll_bar_above_handle,
   and *X and *Y to the character cell the mouse is over.

   Set *TIMESTAMP to the server timestamp of the last mouse event.

   Don't store anything if we don't have a valid set of values to report.

   This clears the mouse_moved flag, so we can wait for the next mouse
   movement.  */

static void
neomacs_mouse_position (struct frame **fp, int insist, Lisp_Object *bar_window,
                        enum scroll_bar_part *part, Lisp_Object *x,
                        Lisp_Object *y, Time *timestamp)
{
  struct frame *f1;
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (*fp);

  block_input ();

  Lisp_Object frame, tail;

  /* Clear the mouse-moved flag for every frame on this display.  */
  FOR_EACH_FRAME (tail, frame)
    if (FRAME_NEOMACS_P (XFRAME (frame))
        && FRAME_DISPLAY_INFO (XFRAME (frame)) == dpyinfo)
      XFRAME (frame)->mouse_moved = false;

  dpyinfo->last_mouse_scroll_bar = NULL;

  if (gui_mouse_grabbed (dpyinfo))
    f1 = dpyinfo->last_mouse_frame;
  else
    f1 = dpyinfo->last_mouse_motion_frame;

  if (f1 != NULL && FRAME_NEOMACS_P (f1))
    {
      int win_x = dpyinfo->last_mouse_motion_x;
      int win_y = dpyinfo->last_mouse_motion_y;

      remember_mouse_glyph (f1, win_x, win_y,
                            &dpyinfo->last_mouse_glyph);
      dpyinfo->last_mouse_glyph_frame = f1;

      *bar_window = Qnil;
      *part = 0;
      *fp = f1;
      XSETINT (*x, win_x);
      XSETINT (*y, win_y);
      *timestamp = dpyinfo->last_user_time;
    }

  unblock_input ();
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

/* X11 named color table.  Generated from the standard X11 rgb.txt.
   Values are 8-bit (0-255).  */
struct named_color
{
  const char *name;
  unsigned char r, g, b;
};

static const struct named_color x11_colors[] =
{
  {"alice blue", 240, 248, 255},
  {"aliceblue", 240, 248, 255},
  {"antique white", 250, 235, 215},
  {"antiquewhite", 250, 235, 215},
  {"aqua", 0, 255, 255},
  {"aquamarine", 127, 255, 212},
  {"azure", 240, 255, 255},
  {"beige", 245, 245, 220},
  {"bisque", 255, 228, 196},
  {"black", 0, 0, 0},
  {"blanched almond", 255, 235, 205},
  {"blanchedalmond", 255, 235, 205},
  {"blue", 0, 0, 255},
  {"blue violet", 138, 43, 226},
  {"blueviolet", 138, 43, 226},
  {"brown", 165, 42, 42},
  {"burlywood", 222, 184, 135},
  {"cadet blue", 95, 158, 160},
  {"cadetblue", 95, 158, 160},
  {"chartreuse", 127, 255, 0},
  {"chocolate", 210, 105, 30},
  {"coral", 255, 127, 80},
  {"cornflower blue", 100, 149, 237},
  {"cornflowerblue", 100, 149, 237},
  {"cornsilk", 255, 248, 220},
  {"crimson", 220, 20, 60},
  {"cyan", 0, 255, 255},
  {"dark blue", 0, 0, 139},
  {"dark cyan", 0, 139, 139},
  {"dark goldenrod", 184, 134, 11},
  {"dark gray", 169, 169, 169},
  {"dark green", 0, 100, 0},
  {"dark grey", 169, 169, 169},
  {"dark khaki", 189, 183, 107},
  {"dark magenta", 139, 0, 139},
  {"dark olive green", 85, 107, 47},
  {"dark orange", 255, 140, 0},
  {"dark orchid", 153, 50, 204},
  {"dark red", 139, 0, 0},
  {"dark salmon", 233, 150, 122},
  {"dark sea green", 143, 188, 143},
  {"dark slate blue", 72, 61, 139},
  {"dark slate gray", 47, 79, 79},
  {"dark slate grey", 47, 79, 79},
  {"dark turquoise", 0, 206, 209},
  {"dark violet", 148, 0, 211},
  {"darkblue", 0, 0, 139},
  {"darkcyan", 0, 139, 139},
  {"darkgoldenrod", 184, 134, 11},
  {"darkgray", 169, 169, 169},
  {"darkgreen", 0, 100, 0},
  {"darkgrey", 169, 169, 169},
  {"darkkhaki", 189, 183, 107},
  {"darkmagenta", 139, 0, 139},
  {"darkolivegreen", 85, 107, 47},
  {"darkorange", 255, 140, 0},
  {"darkorchid", 153, 50, 204},
  {"darkred", 139, 0, 0},
  {"darksalmon", 233, 150, 122},
  {"darkseagreen", 143, 188, 143},
  {"darkslateblue", 72, 61, 139},
  {"darkslategray", 47, 79, 79},
  {"darkslategrey", 47, 79, 79},
  {"darkturquoise", 0, 206, 209},
  {"darkviolet", 148, 0, 211},
  {"deep pink", 255, 20, 147},
  {"deep sky blue", 0, 191, 255},
  {"deeppink", 255, 20, 147},
  {"deepskyblue", 0, 191, 255},
  {"dim gray", 105, 105, 105},
  {"dim grey", 105, 105, 105},
  {"dimgray", 105, 105, 105},
  {"dimgrey", 105, 105, 105},
  {"dodger blue", 30, 144, 255},
  {"dodgerblue", 30, 144, 255},
  {"firebrick", 178, 34, 34},
  {"floral white", 255, 250, 240},
  {"floralwhite", 255, 250, 240},
  {"forest green", 34, 139, 34},
  {"forestgreen", 34, 139, 34},
  {"fuchsia", 255, 0, 255},
  {"gainsboro", 220, 220, 220},
  {"ghost white", 248, 248, 255},
  {"ghostwhite", 248, 248, 255},
  {"gold", 255, 215, 0},
  {"goldenrod", 218, 165, 32},
  {"gray", 128, 128, 128},
  {"gray0", 0, 0, 0},
  {"gray1", 3, 3, 3},
  {"gray2", 5, 5, 5},
  {"gray3", 8, 8, 8},
  {"gray4", 10, 10, 10},
  {"gray5", 13, 13, 13},
  {"gray10", 26, 26, 26},
  {"gray15", 38, 38, 38},
  {"gray20", 51, 51, 51},
  {"gray25", 64, 64, 64},
  {"gray30", 77, 77, 77},
  {"gray35", 89, 89, 89},
  {"gray40", 102, 102, 102},
  {"gray45", 115, 115, 115},
  {"gray50", 128, 128, 128},
  {"gray55", 140, 140, 140},
  {"gray60", 153, 153, 153},
  {"gray65", 166, 166, 166},
  {"gray70", 179, 179, 179},
  {"gray75", 191, 191, 191},
  {"gray80", 204, 204, 204},
  {"gray85", 217, 217, 217},
  {"gray90", 229, 229, 229},
  {"gray95", 242, 242, 242},
  {"gray100", 255, 255, 255},
  {"green", 0, 128, 0},
  {"green yellow", 173, 255, 47},
  {"greenyellow", 173, 255, 47},
  {"grey", 128, 128, 128},
  {"grey0", 0, 0, 0},
  {"grey1", 3, 3, 3},
  {"grey2", 5, 5, 5},
  {"grey3", 8, 8, 8},
  {"grey4", 10, 10, 10},
  {"grey5", 13, 13, 13},
  {"grey10", 26, 26, 26},
  {"grey15", 38, 38, 38},
  {"grey20", 51, 51, 51},
  {"grey25", 64, 64, 64},
  {"grey30", 77, 77, 77},
  {"grey35", 89, 89, 89},
  {"grey40", 102, 102, 102},
  {"grey45", 115, 115, 115},
  {"grey50", 128, 128, 128},
  {"grey55", 140, 140, 140},
  {"grey60", 153, 153, 153},
  {"grey65", 166, 166, 166},
  {"grey70", 179, 179, 179},
  {"grey75", 191, 191, 191},
  {"grey80", 204, 204, 204},
  {"grey85", 217, 217, 217},
  {"grey90", 229, 229, 229},
  {"grey95", 242, 242, 242},
  {"grey100", 255, 255, 255},
  {"honeydew", 240, 255, 240},
  {"hot pink", 255, 105, 180},
  {"hotpink", 255, 105, 180},
  {"indian red", 205, 92, 92},
  {"indianred", 205, 92, 92},
  {"indigo", 75, 0, 130},
  {"ivory", 255, 255, 240},
  {"khaki", 240, 230, 140},
  {"lavender", 230, 230, 250},
  {"lavender blush", 255, 240, 245},
  {"lavenderblush", 255, 240, 245},
  {"lawn green", 124, 252, 0},
  {"lawngreen", 124, 252, 0},
  {"lemon chiffon", 255, 250, 205},
  {"lemonchiffon", 255, 250, 205},
  {"light blue", 173, 216, 230},
  {"light coral", 240, 128, 128},
  {"light cyan", 224, 255, 255},
  {"light goldenrod", 238, 221, 130},
  {"light goldenrod yellow", 250, 250, 210},
  {"light gray", 211, 211, 211},
  {"light green", 144, 238, 144},
  {"light grey", 211, 211, 211},
  {"light pink", 255, 182, 193},
  {"light salmon", 255, 160, 122},
  {"light sea green", 32, 178, 170},
  {"light sky blue", 135, 206, 250},
  {"light slate blue", 132, 112, 255},
  {"light slate gray", 119, 136, 153},
  {"light slate grey", 119, 136, 153},
  {"light steel blue", 176, 196, 222},
  {"light yellow", 255, 255, 224},
  {"lightblue", 173, 216, 230},
  {"lightcoral", 240, 128, 128},
  {"lightcyan", 224, 255, 255},
  {"lightgoldenrod", 238, 221, 130},
  {"lightgoldenrodyellow", 250, 250, 210},
  {"lightgray", 211, 211, 211},
  {"lightgreen", 144, 238, 144},
  {"lightgrey", 211, 211, 211},
  {"lightpink", 255, 182, 193},
  {"lightsalmon", 255, 160, 122},
  {"lightseagreen", 32, 178, 170},
  {"lightskyblue", 135, 206, 250},
  {"lightslateblue", 132, 112, 255},
  {"lightslategray", 119, 136, 153},
  {"lightslategrey", 119, 136, 153},
  {"lightsteelblue", 176, 196, 222},
  {"lightyellow", 255, 255, 224},
  {"lime", 0, 255, 0},
  {"lime green", 50, 205, 50},
  {"limegreen", 50, 205, 50},
  {"linen", 250, 240, 230},
  {"magenta", 255, 0, 255},
  {"maroon", 128, 0, 0},
  {"medium aquamarine", 102, 205, 170},
  {"medium blue", 0, 0, 205},
  {"medium orchid", 186, 85, 211},
  {"medium purple", 147, 112, 219},
  {"medium sea green", 60, 179, 113},
  {"medium slate blue", 123, 104, 238},
  {"medium spring green", 0, 250, 154},
  {"medium turquoise", 72, 209, 204},
  {"medium violet red", 199, 21, 133},
  {"mediumaquamarine", 102, 205, 170},
  {"mediumblue", 0, 0, 205},
  {"mediumorchid", 186, 85, 211},
  {"mediumpurple", 147, 112, 219},
  {"mediumseagreen", 60, 179, 113},
  {"mediumslateblue", 123, 104, 238},
  {"mediumspringgreen", 0, 250, 154},
  {"mediumturquoise", 72, 209, 204},
  {"mediumvioletred", 199, 21, 133},
  {"midnight blue", 25, 25, 112},
  {"midnightblue", 25, 25, 112},
  {"mint cream", 245, 255, 250},
  {"mintcream", 245, 255, 250},
  {"misty rose", 255, 228, 225},
  {"mistyrose", 255, 228, 225},
  {"moccasin", 255, 228, 181},
  {"navajo white", 255, 222, 173},
  {"navajowhite", 255, 222, 173},
  {"navy", 0, 0, 128},
  {"navy blue", 0, 0, 128},
  {"navyblue", 0, 0, 128},
  {"old lace", 253, 245, 230},
  {"oldlace", 253, 245, 230},
  {"olive", 128, 128, 0},
  {"olive drab", 107, 142, 35},
  {"olivedrab", 107, 142, 35},
  {"orange", 255, 165, 0},
  {"orange red", 255, 69, 0},
  {"orangered", 255, 69, 0},
  {"orchid", 218, 112, 214},
  {"pale goldenrod", 238, 232, 170},
  {"pale green", 152, 251, 152},
  {"pale turquoise", 175, 238, 238},
  {"pale violet red", 219, 112, 147},
  {"palegoldenrod", 238, 232, 170},
  {"palegreen", 152, 251, 152},
  {"paleturquoise", 175, 238, 238},
  {"palevioletred", 219, 112, 147},
  {"papaya whip", 255, 239, 213},
  {"papayawhip", 255, 239, 213},
  {"peach puff", 255, 218, 185},
  {"peachpuff", 255, 218, 185},
  {"peru", 205, 133, 63},
  {"pink", 255, 192, 203},
  {"plum", 221, 160, 221},
  {"powder blue", 176, 224, 230},
  {"powderblue", 176, 224, 230},
  {"purple", 128, 0, 128},
  {"rebecca purple", 102, 51, 153},
  {"rebeccapurple", 102, 51, 153},
  {"red", 255, 0, 0},
  {"rosy brown", 188, 143, 143},
  {"rosybrown", 188, 143, 143},
  {"royal blue", 65, 105, 225},
  {"royalblue", 65, 105, 225},
  {"saddle brown", 139, 69, 19},
  {"saddlebrown", 139, 69, 19},
  {"salmon", 250, 128, 114},
  {"sandy brown", 244, 164, 96},
  {"sandybrown", 244, 164, 96},
  {"sea green", 46, 139, 87},
  {"seagreen", 46, 139, 87},
  {"seashell", 255, 245, 238},
  {"sienna", 160, 82, 45},
  {"silver", 192, 192, 192},
  {"sky blue", 135, 206, 235},
  {"skyblue", 135, 206, 235},
  {"slate blue", 106, 90, 205},
  {"slate gray", 112, 128, 144},
  {"slate grey", 112, 128, 144},
  {"slateblue", 106, 90, 205},
  {"slategray", 112, 128, 144},
  {"slategrey", 112, 128, 144},
  {"snow", 255, 250, 250},
  {"spring green", 0, 255, 127},
  {"springgreen", 0, 255, 127},
  {"steel blue", 70, 130, 180},
  {"steelblue", 70, 130, 180},
  {"tan", 210, 180, 140},
  {"teal", 0, 128, 128},
  {"thistle", 216, 191, 216},
  {"tomato", 255, 99, 71},
  {"turquoise", 64, 224, 208},
  {"violet", 238, 130, 238},
  {"violet red", 208, 32, 144},
  {"violetred", 208, 32, 144},
  {"wheat", 245, 222, 179},
  {"white", 255, 255, 255},
  {"white smoke", 245, 245, 245},
  {"whitesmoke", 245, 245, 245},
  {"yellow", 255, 255, 0},
  {"yellow green", 154, 205, 50},
  {"yellowgreen", 154, 205, 50},
};

/* Look up a named color in the X11 color table.
   Returns true and sets r, g, b (as 16-bit values) on success.  */
static bool
neomacs_lookup_named_color (const char *name,
                            unsigned short *r, unsigned short *g, unsigned short *b)
{
  int lo = 0, hi = sizeof x11_colors / sizeof x11_colors[0] - 1;

  /* Binary search won't work because entries aren't all in strict order
     (e.g., "dark blue" vs "darkblue").  Use linear search with
     case-insensitive comparison.  */
  for (int i = 0; i <= hi; i++)
    {
      if (!xstrcasecmp (name, x11_colors[i].name))
        {
          *r = x11_colors[i].r * 257;   /* Scale 0-255 to 0-65535 */
          *g = x11_colors[i].g * 257;
          *b = x11_colors[i].b * 257;
          return true;
        }
    }
  return false;
}

/* Check if a color name is valid and return RGB values */
bool
neomacs_defined_color (struct frame *f, const char *color_name,
                       Emacs_Color *color_def, bool alloc, bool makeIndex)
{
  if (!color_name || !color_def)
    return false;

  unsigned short r, g, b;

  /* Try numeric formats first (#RGB, #RRGGBB, rgb:, rgbi:) */
  if (parse_color_spec (color_name, &r, &g, &b))
    {
      color_def->red = r;
      color_def->green = g;
      color_def->blue = b;
      color_def->pixel = ((r >> 8) << 16) | ((g >> 8) << 8) | (b >> 8);
      return true;
    }

  /* Try X11 named colors */
  if (neomacs_lookup_named_color (color_name, &r, &g, &b))
    {
      color_def->red = r;
      color_def->green = g;
      color_def->blue = b;
      color_def->pixel = ((r >> 8) << 16) | ((g >> 8) << 8) | (b >> 8);
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
              {
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

              /* Get font metrics from the face's font */
              int font_size = 14;  /* default */
              int font_ascent = 0;
              int font_descent = 0;
              int ul_position = 1;
              int ul_thickness = 1;
              if (face->font)
                {
                  font_size = face->font->pixel_size;
                  font_ascent = FONT_BASE (face->font);
                  font_descent = FONT_DESCENT (face->font);
                  if (face->font->underline_position > 0)
                    ul_position = face->font->underline_position;
                  if (face->font->underline_thickness > 0)
                    ul_thickness = face->font->underline_thickness;
                }

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
                                        overline_color,
                                        font_ascent,
                                        font_descent,
                                        ul_position,
                                        ul_thickness);
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
          /* Forward image glyph to Rust scene graph */
          if (s->img)
            {
              uint32_t gpu_id = neomacs_get_or_load_image (dpyinfo, s->img);
              if (gpu_id != 0)
                neomacs_display_add_image_glyph (dpyinfo->display_handle,
                                                  gpu_id,
                                                  s->slice.width > 0 ? s->slice.width : s->img->width,
                                                  s->slice.height > 0 ? s->slice.height : s->img->height);
            }
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
      /* Image glyph rendering is handled by Rust GPU pipeline */
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

/* Clear under internal border — draws the internal border face background
   at the four frame edges so the border is visible.  */
static void
neomacs_clear_under_internal_border (struct frame *f)
{
  if (FRAME_INTERNAL_BORDER_WIDTH (f) > 0)
    {
      int border = FRAME_INTERNAL_BORDER_WIDTH (f);
      int width = FRAME_PIXEL_WIDTH (f);
      int height = FRAME_PIXEL_HEIGHT (f);
      int margin = FRAME_TOP_MARGIN_HEIGHT (f);
      int bottom_margin = FRAME_BOTTOM_MARGIN_HEIGHT (f);
      int face_id = (FRAME_PARENT_FRAME (f)
                     ? (!NILP (Vface_remapping_alist)
                        ? lookup_basic_face (NULL, f,
                                             CHILD_FRAME_BORDER_FACE_ID)
                        : CHILD_FRAME_BORDER_FACE_ID)
                     : (!NILP (Vface_remapping_alist)
                        ? lookup_basic_face (NULL, f,
                                             INTERNAL_BORDER_FACE_ID)
                        : INTERNAL_BORDER_FACE_ID));
      struct face *face = FACE_FROM_ID_OR_NULL (f, face_id);

      if (face)
        {
          unsigned long color = face->background;
          uint32_t rgba = 0xFF000000
            | ((color & 0xFF0000))
            | ((color & 0x00FF00))
            | ((color & 0x0000FF));

          struct neomacs_display_info *dpyinfo
            = FRAME_NEOMACS_DISPLAY_INFO (f);
          if (dpyinfo && dpyinfo->display_handle)
            {
              /* Top edge */
              neomacs_display_draw_border (dpyinfo->display_handle,
                                           0, margin, width, border,
                                           rgba);
              /* Left edge */
              neomacs_display_draw_border (dpyinfo->display_handle,
                                           0, 0, border, height,
                                           rgba);
              /* Right edge */
              neomacs_display_draw_border (dpyinfo->display_handle,
                                           width - border, 0, border,
                                           height, rgba);
              /* Bottom edge */
              neomacs_display_draw_border (dpyinfo->display_handle,
                                           0,
                                           height - bottom_margin - border,
                                           width, border, rgba);
            }
        }
    }
}

/* Compute overhangs for glyph string S — needed so Emacs knows
   whether neighbouring strings overlap.  */
static void
neomacs_compute_glyph_string_overhangs (struct glyph_string *s)
{
  if (s->cmp == NULL
      && (s->first_glyph->type == CHAR_GLYPH
          || s->first_glyph->type == COMPOSITE_GLYPH))
    {
      struct font_metrics metrics;

      if (s->first_glyph->type == CHAR_GLYPH)
        {
          unsigned *code = alloca (sizeof (unsigned) * s->nchars);
          struct font *font = s->font;

          for (int i = 0; i < s->nchars; i++)
            code[i] = s->char2b[i];
          font->driver->text_extents (font, code, s->nchars, &metrics);
        }
      else
        {
          Lisp_Object gstring = composition_gstring_from_id (s->cmp_id);
          composition_gstring_width (gstring, s->cmp_from, s->cmp_to,
                                     &metrics);
        }
      s->right_overhang = (metrics.rbearing > metrics.width
                           ? metrics.rbearing - metrics.width : 0);
      s->left_overhang = metrics.lbearing < 0 ? -metrics.lbearing : 0;
    }
  else if (s->cmp)
    {
      s->right_overhang = s->cmp->rbearing - s->cmp->pixel_width;
      s->left_overhang = -s->cmp->lbearing;
    }
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
        nlog_warn ("Failed to load image %dx%d", img->width, img->height);
      return 0;
    }

  /* Add to cache (evict oldest entry if full) */
  if (neomacs_image_cache_count >= IMAGE_CACHE_SIZE)
    {
      /* Free the oldest GPU image */
      neomacs_display_free_image (dpyinfo->display_handle,
                                  neomacs_image_cache[0].gpu_id);
      /* Shift entries down */
      memmove (&neomacs_image_cache[0], &neomacs_image_cache[1],
               (IMAGE_CACHE_SIZE - 1) * sizeof (neomacs_image_cache[0]));
      neomacs_image_cache_count = IMAGE_CACHE_SIZE - 1;
    }
  neomacs_image_cache[neomacs_image_cache_count].emacs_img = img;
  neomacs_image_cache[neomacs_image_cache_count].gpu_id = gpu_id;
  neomacs_image_cache_count++;

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

/* Draw window divider - used for window-divider-mode (Doom Emacs etc.) */
static void
neomacs_draw_window_divider (struct window *w, int x0, int x1, int y0, int y1)
{
  struct frame *f = XFRAME (WINDOW_FRAME (w));
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!output)
    return;

  /* Get divider face colors */
  struct face *face = FACE_FROM_ID_OR_NULL (f, WINDOW_DIVIDER_FACE_ID);
  struct face *face_first
    = FACE_FROM_ID_OR_NULL (f, WINDOW_DIVIDER_FIRST_PIXEL_FACE_ID);
  struct face *face_last
    = FACE_FROM_ID_OR_NULL (f, WINDOW_DIVIDER_LAST_PIXEL_FACE_ID);
  unsigned long color = face ? face->foreground : FRAME_FOREGROUND_PIXEL (f);
  unsigned long color_first = (face_first
                               ? face_first->foreground
                               : FRAME_FOREGROUND_PIXEL (f));
  unsigned long color_last = (face_last
                              ? face_last->foreground
                              : FRAME_FOREGROUND_PIXEL (f));

  if (dpyinfo && dpyinfo->display_handle)
    {
      if (y1 - y0 > x1 - x0 && x1 - x0 > 2)
        {
          /* Vertical divider: first pixel column, middle, last pixel column */
          uint32_t c_first = (0xFF << 24) | (color_first & 0xFFFFFF);
          uint32_t c_mid = (0xFF << 24) | (color & 0xFFFFFF);
          uint32_t c_last = (0xFF << 24) | (color_last & 0xFFFFFF);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x0, y0, 1, y1 - y0, c_first);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x0 + 1, y0, x1 - x0 - 2, y1 - y0, c_mid);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x1 - 1, y0, 1, y1 - y0, c_last);
        }
      else if (x1 - x0 > y1 - y0 && y1 - y0 > 3)
        {
          /* Horizontal divider: first pixel row, middle, last pixel row */
          uint32_t c_first = (0xFF << 24) | (color_first & 0xFFFFFF);
          uint32_t c_mid = (0xFF << 24) | (color & 0xFFFFFF);
          uint32_t c_last = (0xFF << 24) | (color_last & 0xFFFFFF);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x0, y0, x1 - x0, 1, c_first);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x0, y0 + 1, x1 - x0, y1 - y0 - 2, c_mid);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x0, y1 - 1, x1 - x0, 1, c_last);
        }
      else
        {
          /* Small divider: fill with main color */
          uint32_t c = (0xFF << 24) | (color & 0xFFFFFF);
          neomacs_display_draw_border (dpyinfo->display_handle,
                                       x0, y0, x1 - x0, y1 - y0, c);
        }
    }
  else if (output->cr_context)
    {
      /* Fallback to Cairo */
      cairo_t *cr = output->cr_context;
      double r = RED_FROM_ULONG (color) / 255.0;
      double g = GREEN_FROM_ULONG (color) / 255.0;
      double b = BLUE_FROM_ULONG (color) / 255.0;
      cairo_set_source_rgb (cr, r, g, b);
      cairo_rectangle (cr, x0, y0, x1 - x0, y1 - y0);
      cairo_fill (cr);
    }
}

/* Draw fringe bitmap as GPU rectangles.
   Each row of the bitmap is scanned for runs of set bits, and each run
   is emitted as a 1-pixel-tall rectangle via neomacs_display_draw_border.
   Fringe bitmaps are small (8-16px wide, ~8-10px tall) so this is
   efficient for GPU rendering. */
void
neomacs_draw_fringe_bitmap (struct window *w, struct glyph_row *row,
                            struct draw_fringe_bitmap_params *p)
{
  struct frame *f = XFRAME (WINDOW_FRAME (w));
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (!dpyinfo || !dpyinfo->display_handle || !output)
    return;

  struct face *face = p->face;
  if (!face)
    return;

  /* Determine colors */
  unsigned long fg_pixel = p->cursor_p
    ? (p->overlay_p ? face->background : output->cursor_pixel)
    : face->foreground;
  unsigned long bg_pixel = face->background;

  uint32_t fg_rgb = ((RED_FROM_ULONG (fg_pixel) << 16)
                     | (GREEN_FROM_ULONG (fg_pixel) << 8)
                     | BLUE_FROM_ULONG (fg_pixel));
  uint32_t bg_rgb = ((RED_FROM_ULONG (bg_pixel) << 16)
                     | (GREEN_FROM_ULONG (bg_pixel) << 8)
                     | BLUE_FROM_ULONG (bg_pixel));

  /* Step 1: Clear background area if needed */
  if (p->bx >= 0 && !p->overlay_p)
    neomacs_display_draw_border (dpyinfo->display_handle,
                                 p->bx, p->by, p->nx, p->ny, bg_rgb);

  /* Step 2: Draw bitmap pixels.
     p->bits is an array of unsigned short, one per row.
     p->dh is the vertical offset into the array.
     p->wd is the bitmap width, p->h is the bitmap height. */
  if (p->which != 0 && p->bits)
    {
      for (int row_i = 0; row_i < p->h; row_i++)
        {
          unsigned short bits_row = p->bits[p->dh + row_i];
          int screen_y = p->y + row_i;

          /* Scan for horizontal runs of set bits to minimize draw calls */
          int col = 0;
          while (col < p->wd)
            {
              /* Skip unset bits */
              while (col < p->wd && !(bits_row & (1 << col)))
                col++;
              if (col >= p->wd)
                break;

              /* Found a set bit - scan the run */
              int run_start = col;
              while (col < p->wd && (bits_row & (1 << col)))
                col++;

              /* Emit rectangle for this run */
              int run_len = col - run_start;
              int screen_x = p->x + run_start;
              neomacs_display_draw_border (dpyinfo->display_handle,
                                           screen_x, screen_y,
                                           run_len, 1, fg_rgb);
            }
        }
    }
}


/* Define a fringe bitmap.  Neomacs draws fringe bitmaps directly from
   p->bits in neomacs_draw_fringe_bitmap, so no backend-specific caching
   is needed.  This function exists so gui_init_fringe does not bail. */
static void
neomacs_define_fringe_bitmap (int which, unsigned short *bits,
                               int h, int wd)
{
  /* No-op: neomacs renders directly from raw bitmap bits. */
}

/* Destroy a fringe bitmap.  No-op since we don't cache anything. */
static void
neomacs_destroy_fringe_bitmap (int which)
{
  /* No-op: nothing to free. */
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
 * Mouse Pointer Cursor Shape
 * ============================================================================ */

/* Change the mouse pointer cursor shape for a frame.
   Called by gui_define_frame_cursor() when the mouse moves over
   different elements (text, links, mode-line, fringes, etc.).
   Cursor type values (matching Rust CursorIcon mapping):
   1=default/arrow, 2=text/ibeam, 3=hand/pointer,
   4=crosshair, 5=h-resize, 6=v-resize, 7=hourglass */
static void
neomacs_define_frame_cursor (struct frame *f, Emacs_Cursor cursor)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!dpyinfo || !dpyinfo->display_handle)
    return;

  int cursor_type = (int)(intptr_t) cursor;
  neomacs_display_set_mouse_cursor (dpyinfo->display_handle,
                                     cursor_type);
}


/* Show hourglass cursor during long operations. */
static void
neomacs_show_hourglass (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_set_mouse_cursor (dpyinfo->display_handle, 7);
}

/* Restore normal cursor after long operation. */
static void
neomacs_hide_hourglass (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_set_mouse_cursor (dpyinfo->display_handle, 1);
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
static void
neomacs_frame_up_to_date (struct frame *f)
{
  /* Nothing special needed for neomacs.  The frame has already been
     sent to the render thread in update_end.  */
}


/* ============================================================================
 * Focus and Frame Management
 * ============================================================================ */

/* Set frame alpha (whole-window opacity) based on focus state.
   Reads f->alpha[0] (focused) or f->alpha[1] (unfocused),
   applies Vframe_alpha_lower_limit.  Sends the computed alpha to the
   GPU renderer which uses it as the clear-color alpha for transparent
   window compositing (window is created with_transparent(true) and
   pre-multiplied alpha mode).  */
static void
neomacs_set_frame_alpha (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  double alpha = 1.0;
  double alpha_min = 1.0;

  if (dpyinfo->highlight_frame == f)
    alpha = f->alpha[0];
  else
    alpha = f->alpha[1];

  if (alpha < 0.0)
    return;

  if (FLOATP (Vframe_alpha_lower_limit))
    alpha_min = XFLOAT_DATA (Vframe_alpha_lower_limit);
  else if (FIXNUMP (Vframe_alpha_lower_limit))
    alpha_min = (XFIXNUM (Vframe_alpha_lower_limit)) / 100.0;

  if (alpha > 1.0)
    alpha = 1.0;
  else if (alpha < alpha_min && alpha_min <= 1.0)
    alpha = alpha_min;

  if (dpyinfo->display_handle)
    neomacs_display_set_background_alpha (dpyinfo->display_handle,
                                          (float) alpha);
}

/* Set bitmap icon for frame - no-op.
   Wayland (our primary target) doesn't use bitmap icons
   (icons come from desktop files).  PGTK's implementation is
   also disabled (#if 0) for the same reason.  */
static bool
neomacs_bitmap_icon (struct frame *f, Lisp_Object file)
{
  return false;
}

/* Highlight/unhighlight frame (update cursor appearance and alpha).  */
static void
neomacs_frame_highlight (struct frame *f)
{
  gui_update_cursor (f, true);
  neomacs_set_frame_alpha (f);
}

static void
neomacs_frame_unhighlight (struct frame *f)
{
  gui_update_cursor (f, true);
  neomacs_set_frame_alpha (f);
}

/* Recompute which frame should be highlighted based on focus state.  */
static void
neomacs_frame_rehighlight (struct neomacs_display_info *dpyinfo)
{
  struct frame *old_highlight = dpyinfo->highlight_frame;

  if (dpyinfo->x_focus_frame)
    {
      dpyinfo->highlight_frame
        = ((FRAMEP (FRAME_FOCUS_FRAME (dpyinfo->x_focus_frame)))
           ? XFRAME (FRAME_FOCUS_FRAME (dpyinfo->x_focus_frame))
           : dpyinfo->x_focus_frame);
      if (!FRAME_LIVE_P (dpyinfo->highlight_frame))
        {
          fset_focus_frame (dpyinfo->x_focus_frame, Qnil);
          dpyinfo->highlight_frame = dpyinfo->x_focus_frame;
        }
    }
  else
    dpyinfo->highlight_frame = 0;

  if (old_highlight)
    neomacs_frame_unhighlight (old_highlight);
  if (dpyinfo->highlight_frame)
    neomacs_frame_highlight (dpyinfo->highlight_frame);
}

static void
neomacs_frame_rehighlight_hook (struct frame *frame)
{
  neomacs_frame_rehighlight (FRAME_DISPLAY_INFO (frame));
}

/* Change focus to frame F.  */
static void
neomacs_focus_frame (struct frame *f, bool noactivate)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!dpyinfo)
    return;

  dpyinfo->x_focus_frame = f;
  dpyinfo->focus_frame = f;
  neomacs_frame_rehighlight (dpyinfo);
}

/* Return the focused frame for this terminal.  */
static Lisp_Object
neomacs_get_focus_frame (struct frame *frame)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (frame);
  Lisp_Object focus;

  if (!dpyinfo || !dpyinfo->x_focus_frame)
    return Qnil;

  XSETFRAME (focus, dpyinfo->x_focus_frame);
  return focus;
}

/* Raise or lower frame F.  */
static void
neomacs_frame_raise_lower (struct frame *f, bool raise_flag)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (raise_flag)
    {
      if (!FRAME_VISIBLE_P (f))
        neomacs_make_frame_visible_invisible (f, true);

      /* Request window attention (taskbar flash) when raising.
         This is the equivalent of X11's urgency hint.  */
      if (dpyinfo && dpyinfo->display_handle)
        neomacs_display_request_attention (dpyinfo->display_handle, 0);
    }
}

/* Fullscreen handling.  */
static void
neomacs_fullscreen_hook (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (dpyinfo && dpyinfo->display_handle)
    {
      int mode;
      switch (f->want_fullscreen)
        {
        case FULLSCREEN_BOTH:
          mode = 3;
          break;
        case FULLSCREEN_MAXIMIZED:
          mode = 4;
          break;
        case FULLSCREEN_WIDTH:
          mode = 1;
          break;
        case FULLSCREEN_HEIGHT:
          mode = 2;
          break;
        default:
          mode = 0;
          break;
        }
      neomacs_display_set_fullscreen (dpyinfo->display_handle, mode);
    }
  f->want_fullscreen = FULLSCREEN_NONE;
}

/* Iconify (minimize) frame.  */
static void
neomacs_iconify_frame (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_set_minimized (dpyinfo->display_handle, 1);

  SET_FRAME_ICONIFIED (f, true);
  SET_FRAME_VISIBLE (f, 0);
}

/* Set frame name/title — common logic for implicit and explicit.  */
void
neomacs_set_name (struct frame *f, Lisp_Object name, bool explicit_p)
{
  if (explicit_p)
    {
      if (f->explicit_name && NILP (name))
        update_mode_lines = 12;
      f->explicit_name = !NILP (name);
    }
  else if (f->explicit_name)
    return;

  if (NILP (name))
    name = build_string ("Emacs");
  else
    CHECK_STRING (name);

  if (!NILP (Fstring_equal (name, f->name)))
    return;

  fset_name (f, name);

  /* Title overrides name.  */
  if (!NILP (f->title))
    name = f->title;

  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->display_handle)
    {
      Lisp_Object encoded = ENCODE_UTF_8 (name);
      neomacs_display_set_title (dpyinfo->display_handle,
                                 SSDATA (encoded));
    }
}

/* Set frame title implicitly (from buffer name, etc.).  */
static void
neomacs_implicit_set_name (struct frame *f, Lisp_Object arg,
                           Lisp_Object oldval)
{
  neomacs_set_name (f, arg, false);
}

/* Set frame offset/position.  */
static void
neomacs_set_frame_offset (struct frame *f, int xoff, int yoff,
                          int change_gravity)
{
  if (change_gravity > 0)
    {
      f->top_pos = yoff;
      f->left_pos = xoff;
      f->size_hint_flags &= ~(XNegative | YNegative);
      if (xoff < 0)
        f->size_hint_flags |= XNegative;
      if (yoff < 0)
        f->size_hint_flags |= YNegative;
      f->win_gravity = NorthWestGravity;
    }

  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_set_position (dpyinfo->display_handle, xoff, yoff);
}

/* Delete/destroy a frame.  */
static void
neomacs_free_frame_resources (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  Mouse_HLInfo *hlinfo;

  if (!output || !dpyinfo)
    return;

  hlinfo = MOUSE_HL_INFO (f);

  block_input ();

  free_frame_faces (f);

  /* Clean up display info references to this frame.  */
  if (dpyinfo->focus_frame == f)
    dpyinfo->focus_frame = NULL;
  if (dpyinfo->x_focus_frame == f)
    dpyinfo->x_focus_frame = NULL;
  if (dpyinfo->highlight_frame == f)
    dpyinfo->highlight_frame = NULL;
  if (dpyinfo->x_highlight_frame == f)
    dpyinfo->x_highlight_frame = NULL;
  if (dpyinfo->last_mouse_frame == f)
    dpyinfo->last_mouse_frame = NULL;
  if (dpyinfo->last_mouse_motion_frame == f)
    dpyinfo->last_mouse_motion_frame = NULL;
  if (dpyinfo->last_mouse_glyph_frame == f)
    dpyinfo->last_mouse_glyph_frame = NULL;

  if (f == hlinfo->mouse_face_mouse_frame)
    reset_mouse_highlight (hlinfo);

  unblock_input ();
}

static void
neomacs_delete_frame (struct frame *f)
{
  neomacs_free_frame_resources (f);

  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo)
    dpyinfo->reference_count--;
}

/* Ring the bell (visual flash + optional system beep).  */
static void
neomacs_ring_bell (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  /* GPU visual bell flash (brief white overlay that fades out) */
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_visual_bell (dpyinfo->display_handle);

  /* Terminal bell as fallback when visible-bell is not set */
  if (!visible_bell)
    write (STDOUT_FILENO, "\a", 1);
}

/* Toggle invisible mouse pointer.  */
static void
neomacs_toggle_invisible_pointer (struct frame *f, bool invisible)
{
  if (invisible)
    neomacs_display_set_mouse_cursor (
      FRAME_NEOMACS_DISPLAY_INFO (f)->display_handle, 0);
  else
    neomacs_define_frame_cursor (f, FRAME_OUTPUT_DATA (f)->current_cursor);
}

/* Called when buffer flipping becomes unblocked.  */
static void
neomacs_buffer_flipping_unblocked (struct frame *f)
{
  /* Nothing needed — neomacs doesn't use double-buffered flipping
     at the Emacs level (the GPU renderer handles its own
     double-buffering).  */
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

/* Re-send the current frame to the render thread.
   Used after mouse highlight changes to immediately update the display
   without waiting for a full redisplay cycle.  Does NOT clear cursors.
   Only works with the C display engine (legacy path), since the Rust
   layout engine doesn't use current_matrix.  Mouse-face highlighting
   for the Rust engine requires a hit-test infrastructure (TODO).  */
static void
neomacs_resend_frame (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (!dpyinfo || !dpyinfo->display_handle || !output
      || !output->use_gpu_widget)
    return;

  /* The Rust layout engine produces frames independently — resending
     via neomacs_extract_full_frame would produce an empty frame
     (current_matrix is not populated by the Rust path), causing a
     white flash.  Mouse-face highlighting needs a Rust-side hit-test
     infrastructure to determine buffer positions from pixel coordinates.
     Skip the resend until that is implemented.  */
  if (use_rust_display_engine)
    return;

  void *handle = dpyinfo->display_handle;

  /* Begin frame (clears glyph data but not cursors) */
  if (output->window_id > 0)
    neomacs_display_begin_frame_window (
        handle, output->window_id,
        (float) FRAME_COLUMN_WIDTH (f),
        (float) FRAME_LINE_HEIGHT (f),
        FRAME_FONT (f)
          ? (float) FRAME_FONT (f)->pixel_size : 14.0f);
  else
    neomacs_display_begin_frame (handle);

  /* Re-extract with current mouse highlight state */
  neomacs_extract_full_frame (f);

  /* Send to render thread */
  if (output->window_id > 0)
    neomacs_display_end_frame_window (handle, output->window_id);
  else
    neomacs_display_end_frame (handle);
}

/* Read socket events for the Neomacs terminal.
   This is the main event polling entry point, called from read_avail_input().
   We must drain events from the render thread here because the fd callback
   (neomacs_display_wakeup_handler) is only invoked from
   wait_reading_process_output, which may not run during startup or when
   Emacs is waiting for keyboard input via gobble_input().  */
int
neomacs_read_socket (struct terminal *terminal, struct input_event *hold_quit)
{
  int count;

  /* Drain events from the render thread — this is the primary event pump.
     In GUI backends like X11/GTK, read_socket directly reads from the
     display connection.  We do the same by draining our render thread.
     The wakeup_handler processes events and enqueues them, sets frame
     flags like SET_FRAME_GARBAGED for resize events.  The connection fd
     is stored in dpyinfo->connection.  */
  struct neomacs_display_info *dpyinfo = terminal->display_info.neomacs;
  if (dpyinfo && dpyinfo->connection >= 0)
    neomacs_display_wakeup_handler (dpyinfo->connection, dpyinfo);

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

DEFUN ("xw-display-color-p", Fxw_display_color_p, Sxw_display_color_p, 0, 1, 0,
       doc: /* Return t if the display supports color.  */)
  (Lisp_Object terminal)
{
  /* Neomacs always supports full color via wgpu */
  return Qt;
}

DEFUN ("x-display-grayscale-p", Fx_display_grayscale_p, Sx_display_grayscale_p, 0, 1, 0,
       doc: /* Return t if the display can show shades of gray.  */)
  (Lisp_Object terminal)
{
  /* Neomacs displays support both color and grayscale */
  return Qt;
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
 * Rust Display Engine API
 * ============================================================================ */

DEFUN ("neomacs-set-rust-display", Fneomacs_set_rust_display, Sneomacs_set_rust_display, 1, 1, 0,
       doc: /* Enable or disable the Rust layout engine.
When ENABLE is non-nil, use the Rust layout engine instead of the C display engine.
The Rust engine reads buffer data directly via FFI and produces the glyph buffer,
bypassing xdisp.c's matrix extraction.  Default is t (Rust engine).
Set to nil to fall back to the legacy C matrix extraction engine.  */)
  (Lisp_Object enable)
{
  use_rust_display_engine = !NILP (enable);
  nlog_info ("Rust display engine %s", use_rust_display_engine ? "ENABLED" : "disabled");

  /* Force full redisplay */
  SET_FRAME_GARBAGED (SELECTED_FRAME ());
  return enable;
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


DEFUN ("neomacs-set-scroll-indicators", Fneomacs_set_scroll_indicators,
       Sneomacs_set_scroll_indicators, 1, 1, 0,
       doc: /* Enable or disable scroll position indicators and focus ring.
ENABLED non-nil shows scroll indicators and active window focus ring,
nil hides them.  */)
  (Lisp_Object enabled)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_set_scroll_indicators (dpyinfo->display_handle,
                                          !NILP (enabled));
  return !NILP (enabled) ? Qt : Qnil;
}

DEFUN ("neomacs-set-titlebar-height", Fneomacs_set_titlebar_height,
       Sneomacs_set_titlebar_height, 1, 1, 0,
       doc: /* Set the custom title bar height in pixels.
HEIGHT of 0 hides the title bar; positive values set the height.
The title bar is only visible when window decorations are disabled.  */)
  (Lisp_Object height)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  CHECK_FIXNAT (height);
  int h = XFIXNAT (height);
  neomacs_display_set_titlebar_height (dpyinfo->display_handle, h);
  return make_fixnum (h);
}

DEFUN ("neomacs-show-fps", Fneomacs_show_fps,
       Sneomacs_show_fps, 1, 1, 0,
       doc: /* Toggle the FPS counter overlay.
ENABLED non-nil shows the counter, nil hides it.  */)
  (Lisp_Object enabled)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  neomacs_display_set_show_fps (dpyinfo->display_handle,
                                 !NILP (enabled) ? 1 : 0);
  return !NILP (enabled) ? Qt : Qnil;
}

DEFUN ("neomacs-set-corner-radius", Fneomacs_set_corner_radius,
       Sneomacs_set_corner_radius, 1, 1, 0,
       doc: /* Set the corner radius for borderless window rounding.
RADIUS of 0 means square corners; positive values round the corners.
Only visible when window decorations are disabled.  */)
  (Lisp_Object radius)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  CHECK_FIXNAT (radius);
  int r = XFIXNAT (radius);
  neomacs_display_set_corner_radius (dpyinfo->display_handle, r);
  return make_fixnum (r);
}

DEFUN ("neomacs-set-extra-spacing", Fneomacs_set_extra_spacing,
       Sneomacs_set_extra_spacing, 2, 2, 0,
       doc: /* Set extra LINE-SPACING and LETTER-SPACING in pixels.
LINE-SPACING adds vertical space between text rows.
LETTER-SPACING adds horizontal space between characters.
Values of 0 mean no extra spacing (default).  */)
  (Lisp_Object line_spacing, Lisp_Object letter_spacing)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  CHECK_FIXNUM (line_spacing);
  CHECK_FIXNUM (letter_spacing);
  int ls = XFIXNUM (line_spacing);
  int cs = XFIXNUM (letter_spacing);
  neomacs_display_set_extra_spacing (dpyinfo->display_handle, ls, cs);
  return Qt;
}

DEFUN ("neomacs-set-background-gradient",
       Fneomacs_set_background_gradient,
       Sneomacs_set_background_gradient, 2, 2, 0,
       doc: /* Set a background gradient from TOP-COLOR to BOTTOM-COLOR.
Colors are strings like \"#rrggbb\".  Pass nil for either to disable.  */)
  (Lisp_Object top_color, Lisp_Object bottom_color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  if (NILP (top_color) || NILP (bottom_color))
    {
      neomacs_display_set_background_gradient (
        dpyinfo->display_handle, 0, 0, 0, 0, 0, 0, 0);
      return Qnil;
    }

  CHECK_STRING (top_color);
  CHECK_STRING (bottom_color);

  Emacs_Color tc, bc;
  if (!neomacs_defined_color (NULL, SSDATA (top_color), &tc, false, false))
    error ("Undefined color: %s", SSDATA (top_color));
  if (!neomacs_defined_color (NULL, SSDATA (bottom_color), &bc, false, false))
    error ("Undefined color: %s", SSDATA (bottom_color));

  neomacs_display_set_background_gradient (
    dpyinfo->display_handle, 1,
    tc.red >> 8, tc.green >> 8, tc.blue >> 8,
    bc.red >> 8, bc.green >> 8, bc.blue >> 8);
  return Qt;
}

DEFUN ("neomacs-set-line-highlight",
       Fneomacs_set_line_highlight,
       Sneomacs_set_line_highlight, 0, 2, 0,
       doc: /* Configure current line highlight rendering.
ENABLED non-nil enables a subtle background on the cursor line.
Optional COLOR is a color string (default inherits from hl-line face).  */)
  (Lisp_Object enabled, Lisp_Object color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 50, g = 50, b = 80;
  int opacity = 15;

  if (!NILP (color) && STRINGP (color))
    {
      Emacs_Color c;
      if (neomacs_defined_color (NULL, SSDATA (color), &c, false, false))
        {
          r = c.red >> 8;
          g = c.green >> 8;
          b = c.blue >> 8;
          opacity = 20;
        }
    }

  neomacs_display_set_line_highlight (
    dpyinfo->display_handle, on, r, g, b, opacity);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-show-whitespace",
       Fneomacs_set_show_whitespace,
       Sneomacs_set_show_whitespace, 0, 2, 0,
       doc: /* Configure visible whitespace rendering.
ENABLED non-nil shows dots for spaces and arrows for tabs.
Optional COLOR is a color string (default \"gray50\").  */)
  (Lisp_Object enabled, Lisp_Object color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 128, g = 128, b = 128;
  int opacity = 30;

  if (!NILP (color) && STRINGP (color))
    {
      Emacs_Color c;
      if (neomacs_defined_color (NULL, SSDATA (color), &c, false, false))
        {
          r = c.red >> 8;
          g = c.green >> 8;
          b = c.blue >> 8;
          opacity = 35;
        }
    }

  neomacs_display_set_show_whitespace (
    dpyinfo->display_handle, on, r, g, b, opacity);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-inactive-dim",
       Fneomacs_set_inactive_dim,
       Sneomacs_set_inactive_dim, 0, 2, 0,
       doc: /* Configure inactive window dimming.
ENABLED non-nil dims inactive windows with a dark overlay.
Optional OPACITY is a number 0.0-1.0 for dimming strength (default 0.15).  */)
  (Lisp_Object enabled, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 15; /* default 0.15 */

  if (NUMBERP (opacity))
    {
      double val = XFLOATINT (opacity);
      if (val < 0.0) val = 0.0;
      if (val > 1.0) val = 1.0;
      op = (int)(val * 100.0);
    }

  neomacs_display_set_inactive_dim (dpyinfo->display_handle, on, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-mode-line-separator",
       Fneomacs_set_mode_line_separator,
       Sneomacs_set_mode_line_separator, 0, 3, 0,
       doc: /* Configure mode-line separator rendering.
STYLE is a symbol: nil (none), `line' (thin line), `shadow' (shadow effect),
or `gradient' (gradient fade).
Optional COLOR is a color string (default \"black\").
Optional HEIGHT is the separator height in pixels (default 3).  */)
  (Lisp_Object style, Lisp_Object color, Lisp_Object height)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int s = 0;
  if (EQ (style, intern ("line")))
    s = 1;
  else if (EQ (style, intern ("shadow")))
    s = 2;
  else if (EQ (style, intern ("gradient")))
    s = 3;

  int r = 0, g = 0, b = 0;
  if (!NILP (color) && STRINGP (color))
    {
      Emacs_Color c;
      if (neomacs_defined_color (NULL, SSDATA (color), &c, false, false))
        {
          r = c.red >> 8;
          g = c.green >> 8;
          b = c.blue >> 8;
        }
    }

  int h = 3;
  if (FIXNUMP (height))
    h = (int) XFIXNUM (height);
  if (h < 1) h = 1;
  if (h > 20) h = 20;

  neomacs_display_set_mode_line_separator (
    dpyinfo->display_handle, s, r, g, b, h);
  return s > 0 ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-glow",
       Fneomacs_set_cursor_glow,
       Sneomacs_set_cursor_glow, 0, 3, 0,
       doc: /* Configure cursor glow effect.
ENABLED non-nil enables a soft glow around the cursor.
Optional COLOR is a color string (default cursor color).
Optional RADIUS is the glow radius in pixels (default 30).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object radius)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 102, g = 153, b = 255; /* light blue default */
  int rad = 30;
  int opacity = 15; /* 0.15 */

  if (!NILP (color) && STRINGP (color))
    {
      Emacs_Color c;
      if (neomacs_defined_color (NULL, SSDATA (color), &c, false, false))
        {
          r = c.red >> 8;
          g = c.green >> 8;
          b = c.blue >> 8;
        }
    }

  if (FIXNUMP (radius))
    {
      rad = (int) XFIXNUM (radius);
      if (rad < 5) rad = 5;
      if (rad > 100) rad = 100;
    }

  neomacs_display_set_cursor_glow (
    dpyinfo->display_handle, on, r, g, b, rad, opacity);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-pulse",
       Fneomacs_set_cursor_pulse,
       Sneomacs_set_cursor_pulse, 0, 2, 0,
       doc: /* Configure cursor pulse animation.
ENABLED non-nil enables sinusoidal glow modulation on idle cursor.
Optional SPEED is the pulse frequency as integer (100 = 1.0 Hz, default 100).
Requires cursor glow to be enabled for visible effect.  */)
  (Lisp_Object enabled, Lisp_Object speed)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int spd = 100;  /* 1.0 Hz */
  int min_opacity = 30;  /* 0.3 */

  if (FIXNUMP (speed))
    {
      spd = (int) XFIXNUM (speed);
      if (spd < 10) spd = 10;
      if (spd > 500) spd = 500;
    }

  neomacs_display_set_cursor_pulse (
    dpyinfo->display_handle, on, spd, min_opacity);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-focus-mode",
       Fneomacs_set_focus_mode,
       Sneomacs_set_focus_mode, 0, 2, 0,
       doc: /* Configure focus mode (dim text outside current paragraph).
ENABLED non-nil enables focus mode where only the paragraph around the
cursor is fully visible.  Surrounding text is dimmed.
OPACITY is an integer 0-100 for the dimming overlay opacity (default 40).  */)
  (Lisp_Object enabled, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 40;  /* 0.4 default */
  if (FIXNUMP (opacity))
    op = XFIXNUM (opacity);

  neomacs_display_set_focus_mode (
    dpyinfo->display_handle, on, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-minimap",
       Fneomacs_set_minimap,
       Sneomacs_set_minimap, 0, 2, 0,
       doc: /* Configure minimap code overview column.
ENABLED non-nil shows a minimap on the right side of each window.
WIDTH is the minimap column width in pixels (default 80).  */)
  (Lisp_Object enabled, Lisp_Object width)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int w = 80;
  if (FIXNUMP (width))
    w = XFIXNUM (width);

  neomacs_display_set_minimap (
    dpyinfo->display_handle, on, w);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-typing-ripple",
       Fneomacs_set_typing_ripple,
       Sneomacs_set_typing_ripple, 0, 3, 0,
       doc: /* Configure typing ripple effect.
ENABLED non-nil shows expanding ripple circles from cursor on movement.
MAX-RADIUS is the maximum ripple radius in pixels (default 40).
DURATION-MS is the ripple duration in milliseconds (default 300).  */)
  (Lisp_Object enabled, Lisp_Object max_radius, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 40;
  int d = 300;
  if (FIXNUMP (max_radius))
    r = XFIXNUM (max_radius);
  if (FIXNUMP (duration_ms))
    d = XFIXNUM (duration_ms);

  neomacs_display_set_typing_ripple (
    dpyinfo->display_handle, on, r, d);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-search-pulse",
       Fneomacs_set_search_pulse,
       Sneomacs_set_search_pulse, 0, 1, 0,
       doc: /* Configure search highlight pulse effect.
ENABLED non-nil draws a pulsing glow around the current isearch match.
The isearch face is resolved automatically.  */)
  (Lisp_Object enabled)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int fid = 0;
  if (on)
    {
      struct frame *f = SELECTED_FRAME ();
      Lisp_Object isearch_sym = Fintern (build_string ("isearch"), Qnil);
      fid = lookup_named_face (NULL, f, isearch_sym, false);
      if (fid < 0)
        fid = 0;
    }

  neomacs_display_set_search_pulse (
    dpyinfo->display_handle, on, fid);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-zen-mode",
       Fneomacs_set_zen_mode,
       Sneomacs_set_zen_mode, 0, 3, 0,
       doc: /* Configure zen/distraction-free mode.
ENABLED non-nil activates zen mode with centered content.
CONTENT-WIDTH-PCT is the content width as percentage of window (default 60).
MARGIN-OPACITY is 0-100 for margin overlay opacity (default 30).  */)
  (Lisp_Object enabled, Lisp_Object content_width_pct, Lisp_Object margin_opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int cw = 60;
  int mo = 30;
  if (FIXNUMP (content_width_pct))
    cw = XFIXNUM (content_width_pct);
  if (FIXNUMP (margin_opacity))
    mo = XFIXNUM (margin_opacity);

  neomacs_display_set_zen_mode (
    dpyinfo->display_handle, on, cw, mo);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-color-cycle",
       Fneomacs_set_cursor_color_cycle,
       Sneomacs_set_cursor_color_cycle, 0, 4, 0,
       doc: /* Configure cursor color cycling (rainbow hue rotation).
ENABLED non-nil cycles the cursor color through the rainbow.
SPEED is cycles per second * 100 (default 50 = 0.5 cps).
SATURATION is 0-100 (default 80).
LIGHTNESS is 0-100 (default 60).  */)
  (Lisp_Object enabled, Lisp_Object speed, Lisp_Object saturation, Lisp_Object lightness)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int sp = 50;
  int sat = 80;
  int lig = 60;
  if (FIXNUMP (speed))
    sp = XFIXNUM (speed);
  if (FIXNUMP (saturation))
    sat = XFIXNUM (saturation);
  if (FIXNUMP (lightness))
    lig = XFIXNUM (lightness);

  neomacs_display_set_cursor_color_cycle (
    dpyinfo->display_handle, on, sp, sat, lig);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-header-shadow",
       Fneomacs_set_header_shadow,
       Sneomacs_set_header_shadow, 0, 3, 0,
       doc: /* Configure header/mode-line shadow depth effect.
ENABLED non-nil draws gradient shadows below header-line and above mode-line.
INTENSITY is 0-100 for shadow darkness (default 30).
SIZE is the shadow gradient size in pixels (default 6).  */)
  (Lisp_Object enabled, Lisp_Object intensity, Lisp_Object size)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int inten = 30;
  int sz = 6;
  if (FIXNUMP (intensity))
    inten = XFIXNUM (intensity);
  if (FIXNUMP (size))
    sz = XFIXNUM (size);

  neomacs_display_set_header_shadow (
    dpyinfo->display_handle, on, inten, sz);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-line-animation",
       Fneomacs_set_line_animation,
       Sneomacs_set_line_animation, 0, 2, 0,
       doc: /* Configure smooth line insertion/deletion animation.
ENABLED non-nil activates the animation.
DURATION-MS is the animation duration in milliseconds (default 150).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 150;
  if (FIXNUMP (duration_ms))
    dur = XFIXNUM (duration_ms);

  neomacs_display_set_line_animation (
    dpyinfo->display_handle, on, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-vignette",
       Fneomacs_set_vignette,
       Sneomacs_set_vignette, 0, 3, 0,
       doc: /* Configure vignette (edge darkening) effect.
ENABLED non-nil activates vignette overlay on frame edges.
INTENSITY is 0-100 for darkening strength (default 50).
RADIUS is the inset in pixels from each edge (default 80).  */)
  (Lisp_Object enabled, Lisp_Object intensity, Lisp_Object radius)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int inten = 50;
  int rad = 80;
  if (FIXNUMP (intensity))
    inten = XFIXNUM (intensity);
  if (FIXNUMP (radius))
    rad = XFIXNUM (radius);

  neomacs_display_set_vignette (
    dpyinfo->display_handle, on, inten, rad);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-scroll-progress",
       Fneomacs_set_scroll_progress,
       Sneomacs_set_scroll_progress, 0, 6, 0,
       doc: /* Configure scroll progress indicator bar.
ENABLED non-nil shows a thin colored bar at the top of each window
indicating scroll position within the buffer.
HEIGHT is bar height in pixels (default 2).
R, G, B are color components 0-255 (default 102 153 255).
OPACITY is 0-100 (default 80).  */)
  (Lisp_Object enabled, Lisp_Object height, Lisp_Object r, Lisp_Object g,
   Lisp_Object b, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int h = 2;
  int cr = 102, cg = 153, cb = 255;
  int op = 80;
  if (FIXNUMP (height)) h = XFIXNUM (height);
  if (FIXNUMP (r)) cr = XFIXNUM (r);
  if (FIXNUMP (g)) cg = XFIXNUM (g);
  if (FIXNUMP (b)) cb = XFIXNUM (b);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_scroll_progress (
    dpyinfo->display_handle, on, h, cr, cg, cb, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-inactive-tint",
       Fneomacs_set_inactive_tint,
       Sneomacs_set_inactive_tint, 0, 5, 0,
       doc: /* Configure inactive window color tint.
ENABLED non-nil applies a color overlay on inactive (non-selected) windows.
R, G, B are color components 0-255 (default 51 26 0, warm sepia).
OPACITY is 0-100 (default 10).  */)
  (Lisp_Object enabled, Lisp_Object r, Lisp_Object g, Lisp_Object b,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int cr = 51, cg = 26, cb = 0;
  int op = 10;
  if (FIXNUMP (r)) cr = XFIXNUM (r);
  if (FIXNUMP (g)) cg = XFIXNUM (g);
  if (FIXNUMP (b)) cb = XFIXNUM (b);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_inactive_tint (
    dpyinfo->display_handle, on, cr, cg, cb, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-window-glow",
       Fneomacs_set_window_glow,
       Sneomacs_set_window_glow, 0, 6, 0,
       doc: /* Configure active window border glow effect.
ENABLED non-nil draws a soft glow around the selected window border.
R, G, B are color components 0-255 (default 102 153 255, light blue).
RADIUS is glow size in pixels (default 8).
INTENSITY is 0-100 for peak glow opacity (default 40).  */)
  (Lisp_Object enabled, Lisp_Object r, Lisp_Object g, Lisp_Object b,
   Lisp_Object radius, Lisp_Object intensity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int cr = 102, cg = 153, cb = 255;
  int rad = 8;
  int inten = 40;
  if (FIXNUMP (r)) cr = XFIXNUM (r);
  if (FIXNUMP (g)) cg = XFIXNUM (g);
  if (FIXNUMP (b)) cb = XFIXNUM (b);
  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (intensity)) inten = XFIXNUM (intensity);

  neomacs_display_set_window_glow (
    dpyinfo->display_handle, on, cr, cg, cb, rad, inten);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-breadcrumb",
       Fneomacs_set_breadcrumb,
       Sneomacs_set_breadcrumb, 0, 2, 0,
       doc: /* Configure breadcrumb/path bar overlay.
ENABLED non-nil shows the buffer file path as a breadcrumb bar at
the top of each window.
OPACITY is 0-100 for background opacity (default 70).  */)
  (Lisp_Object enabled, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 70;
  if (FIXNUMP (opacity))
    op = XFIXNUM (opacity);

  neomacs_display_set_breadcrumb (
    dpyinfo->display_handle, on, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-title-fade",
       Fneomacs_set_title_fade,
       Sneomacs_set_title_fade, 0, 2, 0,
       doc: /* Configure breadcrumb title fade animation.
ENABLED non-nil crossfades the breadcrumb text when switching buffers
in a window, providing smooth visual feedback.
DURATION-MS is the fade duration in milliseconds (default 300).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 300;
  if (FIXNUMP (duration_ms))
    dur = XFIXNUM (duration_ms);

  neomacs_display_set_title_fade (
    dpyinfo->display_handle, on, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-typing-speed",
       Fneomacs_set_typing_speed,
       Sneomacs_set_typing_speed, 0, 1, 0,
       doc: /* Configure typing speed (WPM) indicator overlay.
ENABLED non-nil shows a live words-per-minute counter in the bottom-right
corner of the active window, calculated from recent keystroke rate.  */)
  (Lisp_Object enabled)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  neomacs_display_set_typing_speed (dpyinfo->display_handle, on);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-border-transition",
       Fneomacs_set_border_transition,
       Sneomacs_set_border_transition, 0, 5, 0,
       doc: /* Configure smooth border color transition on focus change.
ENABLED non-nil draws a colored border around windows that smoothly
fades in/out when focus changes between windows.
R, G, B are the active border color 0-255 (default 102 153 255).
DURATION-MS is the transition duration in milliseconds (default 200).  */)
  (Lisp_Object enabled, Lisp_Object r, Lisp_Object g, Lisp_Object b,
   Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int cr = 102, cg = 153, cb = 255;
  int dur = 200;
  if (FIXNUMP (r)) cr = XFIXNUM (r);
  if (FIXNUMP (g)) cg = XFIXNUM (g);
  if (FIXNUMP (b)) cb = XFIXNUM (b);
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_border_transition (
    dpyinfo->display_handle, on, cr, cg, cb, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-accent-strip",
       Fneomacs_set_accent_strip,
       Sneomacs_set_accent_strip, 0, 2, 0,
       doc: /* Configure buffer-local accent color strip.
ENABLED non-nil renders a thin colored strip on the left edge of each
window, with color derived from the buffer's file extension.
WIDTH is strip width in pixels (default 3).  */)
  (Lisp_Object enabled, Lisp_Object width)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int w = 3;
  if (FIXNUMP (width)) w = XFIXNUM (width);

  neomacs_display_set_accent_strip (dpyinfo->display_handle, on, w);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-frosted-glass",
       Fneomacs_set_frosted_glass,
       Sneomacs_set_frosted_glass, 0, 3, 0,
       doc: /* Configure frosted glass effect on mode-lines.
ENABLED non-nil adds a semi-transparent frosted glass overlay with
noise grain to mode-line areas, creating a blurred glass appearance.
OPACITY is 0-100 for frost intensity (default 30).
BLUR is the blur spread radius in pixels (default 4).  */)
  (Lisp_Object enabled, Lisp_Object opacity, Lisp_Object blur)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 30;
  int bl = 4;
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (FIXNUMP (blur)) bl = XFIXNUM (blur);

  neomacs_display_set_frosted_glass (
    dpyinfo->display_handle, on, op, bl);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-region-glow",
       Fneomacs_set_region_glow,
       Sneomacs_set_region_glow, 0, 4, 0,
       doc: /* Configure selection region glow highlight.
ENABLED non-nil renders a soft glow around the active text selection
region, using the region face background color.
FACE-ID is the numeric face ID of the region face (auto-detected if nil).
RADIUS is glow radius in pixels (default 6).
OPACITY is 0-100 for glow intensity (default 30).  */)
  (Lisp_Object enabled, Lisp_Object face_id, Lisp_Object radius,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int fid = 0;
  int rad = 6;
  int op = 30;

  if (FIXNUMP (face_id))
    fid = XFIXNUM (face_id);
  else if (on)
    {
      /* Auto-detect region face ID */
      Lisp_Object region_sym = intern ("region");
      fid = lookup_named_face (NULL, NULL, region_sym, false);
      if (fid < 0) fid = 0;
    }

  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_region_glow (dpyinfo->display_handle, on, fid, rad, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-idle-dim",
       Fneomacs_set_idle_dim,
       Sneomacs_set_idle_dim, 0, 4, 0,
       doc: /* Configure idle screen dimming after inactivity.
ENABLED non-nil dims the entire frame after a period of no keyboard
or mouse activity, then smoothly restores brightness on input.
DELAY-SECS is seconds of inactivity before dimming (default 60).
OPACITY is 0-100 for dimming darkness (default 40).
FADE-MS is the fade transition duration in milliseconds (default 500).  */)
  (Lisp_Object enabled, Lisp_Object delay_secs, Lisp_Object opacity,
   Lisp_Object fade_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int delay = 60;
  int op = 40;
  int fade = 500;
  if (FIXNUMP (delay_secs)) delay = XFIXNUM (delay_secs);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (FIXNUMP (fade_ms)) fade = XFIXNUM (fade_ms);

  neomacs_display_set_idle_dim (dpyinfo->display_handle, on, delay, op, fade);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-shadow",
       Fneomacs_set_cursor_shadow,
       Sneomacs_set_cursor_shadow, 0, 4, 0,
       doc: /* Configure cursor drop shadow.
ENABLED non-nil renders a soft drop shadow behind the cursor for
a 3D depth effect.
OFFSET-X is horizontal shadow offset in pixels (default 2).
OFFSET-Y is vertical shadow offset in pixels (default 2).
OPACITY is 0-100 for shadow darkness (default 30).  */)
  (Lisp_Object enabled, Lisp_Object offset_x, Lisp_Object offset_y,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int ox = 2, oy = 2, op = 30;
  if (FIXNUMP (offset_x)) ox = XFIXNUM (offset_x);
  if (FIXNUMP (offset_y)) oy = XFIXNUM (offset_y);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_shadow (dpyinfo->display_handle, on, ox, oy, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-focus-ring",
       Fneomacs_set_focus_ring,
       Sneomacs_set_focus_ring, 0, 7, 0,
       doc: /* Configure animated focus ring around selected window.
ENABLED non-nil renders a marching-ants-style dashed border around
the focused window with continuous animation.
R, G, B are 0-255 ring color components (default 102, 153, 255).
OPACITY is 0-100 for ring intensity (default 50).
DASH-LENGTH is the dash segment length in pixels (default 8).
SPEED is the animation speed in pixels per second (default 40).  */)
  (Lisp_Object enabled, Lisp_Object r, Lisp_Object g, Lisp_Object b,
   Lisp_Object opacity, Lisp_Object dash_length, Lisp_Object speed)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int cr = 102, cg = 153, cb = 255;
  int op = 50;
  int dl = 8;
  int sp = 40;
  if (FIXNUMP (r)) cr = XFIXNUM (r);
  if (FIXNUMP (g)) cg = XFIXNUM (g);
  if (FIXNUMP (b)) cb = XFIXNUM (b);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (FIXNUMP (dash_length)) dl = XFIXNUM (dash_length);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);

  neomacs_display_set_focus_ring (dpyinfo->display_handle, on, cr, cg, cb, op, dl, sp);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-window-mode-tint",
       Fneomacs_set_window_mode_tint,
       Sneomacs_set_window_mode_tint, 0, 2, 0,
       doc: /* Configure window background tint based on file type.
ENABLED non-nil applies a subtle background color tint to each window
based on the file extension of its buffer, providing a visual cue for
the type of file being edited.
OPACITY is 0-100 for tint intensity (default 3).  */)
  (Lisp_Object enabled, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 3;
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_window_mode_tint (dpyinfo->display_handle, on, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-window-watermark",
       Fneomacs_set_window_watermark,
       Sneomacs_set_window_watermark, 0, 3, 0,
       doc: /* Configure window watermark for empty buffers.
ENABLED non-nil renders a large faded text showing the buffer name
centered in windows whose buffer has very little content.
OPACITY is 0-100 for watermark visibility (default 8).
THRESHOLD is the maximum buffer size in characters to show watermark (default 10).  */)
  (Lisp_Object enabled, Lisp_Object opacity, Lisp_Object threshold)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 8;
  int thresh = 10;
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (FIXNUMP (threshold)) thresh = XFIXNUM (threshold);

  neomacs_display_set_window_watermark (dpyinfo->display_handle, on, op, thresh);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-text-fade-in",
       Fneomacs_set_text_fade_in,
       Sneomacs_set_text_fade_in, 0, 2, 0,
       doc: /* Configure text fade-in animation for new content.
ENABLED non-nil makes text in a window fade in from transparent when
the buffer content changes (scroll, buffer switch).
DURATION-MS is the fade duration in milliseconds (default 150).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 150;
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_text_fade_in (dpyinfo->display_handle, on, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-scroll-line-spacing",
       Fneomacs_set_scroll_line_spacing,
       Sneomacs_set_scroll_line_spacing, 0, 3, 0,
       doc: /* Configure scroll line spacing animation (accordion effect).
ENABLED non-nil applies a transient accordion-style line spacing
animation when scrolling, where lines at the leading edge briefly
spread apart and then settle back.
MAX-SPACING is maximum extra spacing in pixels (default 6).
DURATION-MS is animation duration in milliseconds (default 200).  */)
  (Lisp_Object enabled, Lisp_Object max_spacing, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int ms = 6, dur = 200;
  if (FIXNUMP (max_spacing)) ms = XFIXNUM (max_spacing);
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_scroll_line_spacing (dpyinfo->display_handle, on, ms, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-mode-line-transition",
       Fneomacs_set_mode_line_transition,
       Sneomacs_set_mode_line_transition, 0, 2, 0,
       doc: /* Configure smooth mode-line content transition.
ENABLED non-nil makes mode-line text fade in when its content changes,
creating a smooth transition effect.
DURATION-MS is the fade duration in milliseconds (default 200).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 200;
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_mode_line_transition (dpyinfo->display_handle, on, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-wake",
       Fneomacs_set_cursor_wake,
       Sneomacs_set_cursor_wake, 0, 3, 0,
       doc: /* Configure cursor wake animation.
ENABLED non-nil makes the cursor briefly scale up (pop) when it
becomes visible after blinking off.
DURATION-MS is the animation duration in milliseconds (default 120).
SCALE-PCT is the initial scale percentage (default 130, meaning 130%).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms, Lisp_Object scale_pct)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 120;
  int scale = 130;
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);
  if (FIXNUMP (scale_pct)) scale = XFIXNUM (scale_pct);

  neomacs_display_set_cursor_wake (dpyinfo->display_handle, on, dur, scale);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-window-content-shadow",
       Fneomacs_set_window_content_shadow,
       Sneomacs_set_window_content_shadow, 0, 3, 0,
       doc: /* Configure window content shadow/depth between split panes.
ENABLED non-nil renders inner shadows at window edges when multiple
windows are visible, creating a depth/raised pane illusion.
SIZE is the shadow spread in pixels (default 6).
OPACITY is 0-100 percentage (default 15).  */)
  (Lisp_Object enabled, Lisp_Object size, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int sz = 6;
  int op = 15;
  if (FIXNUMP (size)) sz = XFIXNUM (size);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_window_content_shadow (dpyinfo->display_handle, on, sz, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-click-halo",
       Fneomacs_set_click_halo,
       Sneomacs_set_click_halo, 0, 4, 0,
       doc: /* Configure cursor click halo effect.
ENABLED non-nil draws an expanding circular halo animation at the
mouse position when a button is clicked.
COLOR is an RGB hex string (default "#6699FF").
DURATION-MS is the animation duration in milliseconds (default 300).
MAX-RADIUS is the maximum halo radius in pixels (default 30).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object duration_ms, Lisp_Object max_radius)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 102, g = 153, b = 255;
  int dur = 300;
  int rad = 30;

  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);
  if (FIXNUMP (max_radius)) rad = XFIXNUM (max_radius);

  neomacs_display_set_click_halo (dpyinfo->display_handle, on, r, g, b, dur, rad);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-scroll-velocity-fade",
       Fneomacs_set_scroll_velocity_fade,
       Sneomacs_set_scroll_velocity_fade, 0, 3, 0,
       doc: /* Configure scroll velocity fade overlay.
ENABLED non-nil darkens window content briefly during fast scrolling,
providing visual feedback of scroll speed that fades when scrolling stops.
MAX-OPACITY is 0-100 percentage at peak velocity (default 15).
FADE-MS is the fade-out duration in milliseconds (default 300).  */)
  (Lisp_Object enabled, Lisp_Object max_opacity, Lisp_Object fade_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 15;
  int fade = 300;
  if (FIXNUMP (max_opacity)) op = XFIXNUM (max_opacity);
  if (FIXNUMP (fade_ms)) fade = XFIXNUM (fade_ms);

  neomacs_display_set_scroll_velocity_fade (dpyinfo->display_handle, on, op, fade);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-minibuffer-highlight",
       Fneomacs_set_minibuffer_highlight,
       Sneomacs_set_minibuffer_highlight, 0, 3, 0,
       doc: /* Configure mini-buffer completion highlight glow.
ENABLED non-nil draws a soft glow overlay around highlighted
completion candidates in the mini-buffer area.
COLOR is an RGB hex string (default "#6699FF").
OPACITY is 0-100 percentage (default 25).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 102, g = 153, b = 255;
  int op = 25;

  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_minibuffer_highlight (dpyinfo->display_handle, on, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-resize-padding",
       Fneomacs_set_resize_padding,
       Sneomacs_set_resize_padding, 0, 3, 0,
       doc: /* Configure smooth window padding transition on resize.
ENABLED non-nil draws a temporary visual padding overlay at window edges
during frame resize that eases to zero, creating a smooth transition.
DURATION-MS is the transition duration in milliseconds (default 200).
MAX-PADDING is the maximum extra padding in pixels (default 12).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms, Lisp_Object max_padding)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 200;
  int pad = 12;
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);
  if (FIXNUMP (max_padding)) pad = XFIXNUM (max_padding);

  neomacs_display_set_resize_padding (dpyinfo->display_handle, on, dur, pad);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-error-pulse",
       Fneomacs_set_cursor_error_pulse,
       Sneomacs_set_cursor_error_pulse, 0, 3, 0,
       doc: /* Configure cursor error pulse animation.
ENABLED non-nil makes the cursor briefly flash a color when Emacs
rings the bell (e.g., on errors like end-of-buffer).
COLOR is an RGB hex string (default "#FF3333").
DURATION-MS is the pulse duration in milliseconds (default 250).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 255, g = 51, b = 51;
  int dur = 250;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_cursor_error_pulse (dpyinfo->display_handle, on, r, g, b, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-crosshair",
       Fneomacs_set_cursor_crosshair,
       Sneomacs_set_cursor_crosshair, 0, 3, 0,
       doc: /* Configure cursor crosshair guide lines.
ENABLED non-nil draws thin semi-transparent horizontal and vertical
guide lines extending from the cursor position across the window.
COLOR is an RGB hex string (default "#808080").
OPACITY is a percentage 0-100 (default 15).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 128, g = 128, b = 128;
  int op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_crosshair (dpyinfo->display_handle, on, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-modified-indicator",
       Fneomacs_set_modified_indicator,
       Sneomacs_set_modified_indicator, 0, 4, 0,
       doc: /* Configure buffer modified border indicator.
ENABLED non-nil draws a colored strip along the left edge of windows
whose buffers have unsaved modifications.
COLOR is an RGB hex string (default "#FF9933").
WIDTH is the strip width in pixels (default 3).
OPACITY is a percentage 0-100 (default 80).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object width, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 255, g = 153, b = 51;
  int w = 3;
  int op = 80;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (width)) w = XFIXNUM (width);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_modified_indicator (dpyinfo->display_handle, on, r, g, b, w, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-stained-glass",
       Fneomacs_set_stained_glass,
       Sneomacs_set_stained_glass, 0, 3, 0,
       doc: /* Configure inactive window stained glass effect.
ENABLED non-nil tints inactive windows with a unique color derived
from each buffer's identity, creating a stained-glass look.
OPACITY is a percentage 0-100 (default 8).
SATURATION is a percentage 0-100 controlling color intensity (default 60).  */)
  (Lisp_Object enabled, Lisp_Object opacity, Lisp_Object saturation)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 8, sat = 60;
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (FIXNUMP (saturation)) sat = XFIXNUM (saturation);

  neomacs_display_set_stained_glass (dpyinfo->display_handle, on, op, sat);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-focus-gradient-border",
       Fneomacs_set_focus_gradient_border,
       Sneomacs_set_focus_gradient_border, 0, 5, 0,
       doc: /* Configure focused window gradient border.
ENABLED non-nil renders a gradient border on the active window that
transitions from TOP-COLOR to BOTTOM-COLOR vertically.
TOP-COLOR and BOTTOM-COLOR are RGB hex strings (defaults "#4D99FF" and "#994DFF").
WIDTH is border width in pixels (default 2).
OPACITY is a percentage 0-100 (default 60).  */)
  (Lisp_Object enabled, Lisp_Object top_color, Lisp_Object bottom_color,
   Lisp_Object width, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int w = 2, op = 60;
  int tr = 0x4D, tg = 0x99, tb = 0xFF;
  int br = 0x99, bg = 0x4D, bb = 0xFF;
  if (FIXNUMP (width)) w = XFIXNUM (width);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (top_color))
    {
      const char *s = SSDATA (top_color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          tr = (hex >> 16) & 0xFF;
          tg = (hex >> 8) & 0xFF;
          tb = hex & 0xFF;
        }
    }
  if (STRINGP (bottom_color))
    {
      const char *s = SSDATA (bottom_color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          br = (hex >> 16) & 0xFF;
          bg = (hex >> 8) & 0xFF;
          bb = hex & 0xFF;
        }
    }

  neomacs_display_set_focus_gradient_border (dpyinfo->display_handle, on, tr, tg, tb, br, bg, bb, w, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-magnetism",
       Fneomacs_set_cursor_magnetism,
       Sneomacs_set_cursor_magnetism, 0, 5, 0,
       doc: /* Configure cursor magnetism effect on jump.
ENABLED non-nil renders collapsing concentric rings at the cursor
destination when it jumps a long distance.
COLOR is an RGB hex string (default "#66B3FF").
RING-COUNT is the number of rings (default 3).
DURATION-MS is animation duration in milliseconds (default 300).
OPACITY is a percentage 0-100 (default 50).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ring_count,
   Lisp_Object duration_ms, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int rc = 3, dm = 300, op = 50;
  int r = 0x66, g = 0xB3, b = 0xFF;
  if (FIXNUMP (ring_count)) rc = XFIXNUM (ring_count);
  if (FIXNUMP (duration_ms)) dm = XFIXNUM (duration_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_cursor_magnetism (dpyinfo->display_handle, on, r, g, b, rc, dm, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-depth-shadow",
       Fneomacs_set_depth_shadow,
       Sneomacs_set_depth_shadow, 0, 5, 0,
       doc: /* Configure window depth shadow layers.
ENABLED non-nil renders multiple shadow layers at window edges,
creating a 3D stacked paper effect.
LAYERS is the number of shadow layers (default 3).
OFFSET is pixels per layer (default 2).
COLOR is an RGB hex string (default "#000000").
OPACITY is a percentage 0-100 (default 15).  */)
  (Lisp_Object enabled, Lisp_Object layers, Lisp_Object offset,
   Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int ly = 3, off = 2, op = 15;
  int r = 0, g = 0, b = 0;
  if (FIXNUMP (layers)) ly = XFIXNUM (layers);
  if (FIXNUMP (offset)) off = XFIXNUM (offset);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_depth_shadow (dpyinfo->display_handle, on, ly, off, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-mode-line-gradient",
       Fneomacs_set_mode_line_gradient,
       Sneomacs_set_mode_line_gradient, 0, 4, 0,
       doc: /* Configure mode-line gradient background.
ENABLED non-nil renders a horizontal gradient across the mode-line
area instead of a flat color.
LEFT-COLOR and RIGHT-COLOR are RGB hex strings (defaults "#334D80" and "#804D33").
OPACITY is a percentage 0-100 (default 30).  */)
  (Lisp_Object enabled, Lisp_Object left_color, Lisp_Object right_color,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int op = 30;
  int lr = 0x33, lg = 0x4D, lb = 0x80;
  int rr = 0x80, rg = 0x4D, rb = 0x33;
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (left_color))
    {
      const char *s = SSDATA (left_color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          lr = (hex >> 16) & 0xFF;
          lg = (hex >> 8) & 0xFF;
          lb = hex & 0xFF;
        }
    }
  if (STRINGP (right_color))
    {
      const char *s = SSDATA (right_color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          rr = (hex >> 16) & 0xFF;
          rg = (hex >> 8) & 0xFF;
          rb = hex & 0xFF;
        }
    }

  neomacs_display_set_mode_line_gradient (dpyinfo->display_handle, on, lr, lg, lb, rr, rg, rb, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-corner-fold",
       Fneomacs_set_corner_fold,
       Sneomacs_set_corner_fold, 0, 4, 0,
       doc: /* Configure window corner fold effect.
ENABLED non-nil renders a small triangular page fold in the top-right
corner of each window, giving a paper-like appearance.
SIZE is fold size in pixels (default 20).
COLOR is an RGB hex string for the fold accent (default "#996633").
OPACITY is a percentage 0-100 (default 50).  */)
  (Lisp_Object enabled, Lisp_Object size, Lisp_Object color,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int sz = 20, op = 50;
  int r = 0x99, g = 0x66, b = 0x33;
  if (FIXNUMP (size)) sz = XFIXNUM (size);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_corner_fold (dpyinfo->display_handle, on, sz, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-frosted-border",
       Fneomacs_set_frosted_border,
       Sneomacs_set_frosted_border, 0, 4, 0,
       doc: /* Configure frosted window border effect.
ENABLED non-nil renders a multi-layered semi-transparent border around
each window, simulating frosted glass.
WIDTH is border width in pixels (default 4).
OPACITY is a percentage 0-100 (default 15).
COLOR is an RGB hex string (default "#FFFFFF").  */)
  (Lisp_Object enabled, Lisp_Object width, Lisp_Object opacity,
   Lisp_Object color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int w = 4, op = 15;
  int r = 0xFF, g = 0xFF, b = 0xFF;
  if (FIXNUMP (width)) w = XFIXNUM (width);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_frosted_border (dpyinfo->display_handle, on, w, op, r, g, b);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-line-number-pulse",
       Fneomacs_set_line_number_pulse,
       Sneomacs_set_line_number_pulse, 0, 4, 0,
       doc: /* Configure line number pulse on cursor line.
ENABLED non-nil renders a pulsing glow on the line number gutter area
of the current cursor line.
COLOR is an RGB hex string (default "#6699FF").
INTENSITY is a percentage 0-100 (default 30).
CYCLE-MS is the pulse cycle duration in milliseconds (default 2000).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object intensity,
   Lisp_Object cycle_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int inten = 30, cm = 2000;
  int r = 0x66, g = 0x99, b = 0xFF;
  if (FIXNUMP (intensity)) inten = XFIXNUM (intensity);
  if (FIXNUMP (cycle_ms)) cm = XFIXNUM (cycle_ms);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_line_number_pulse (dpyinfo->display_handle, on, r, g, b, inten, cm);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-breathing-border",
       Fneomacs_set_breathing_border,
       Sneomacs_set_breathing_border, 0, 5, 0,
       doc: /* Configure window breathing border animation.
ENABLED non-nil animates window borders with a slow sinusoidal opacity
change, creating a gentle breathing effect.
COLOR is an RGB hex string (default "#808080").
MIN-OPACITY is minimum opacity percentage 0-100 (default 5).
MAX-OPACITY is maximum opacity percentage 0-100 (default 30).
CYCLE-MS is the full cycle duration in milliseconds (default 3000).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object min_opacity,
   Lisp_Object max_opacity, Lisp_Object cycle_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int mn = 5, mx = 30, cm = 3000;
  int r = 0x80, g = 0x80, b = 0x80;
  if (FIXNUMP (min_opacity)) mn = XFIXNUM (min_opacity);
  if (FIXNUMP (max_opacity)) mx = XFIXNUM (max_opacity);
  if (FIXNUMP (cycle_ms)) cm = XFIXNUM (cycle_ms);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_breathing_border (dpyinfo->display_handle, on, r, g, b, mn, mx, cm);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-scanlines",
       Fneomacs_set_scanlines,
       Sneomacs_set_scanlines, 0, 4, 0,
       doc: /* Configure window scanline (CRT) effect.
ENABLED non-nil overlays subtle horizontal scanlines across the screen,
creating a retro CRT monitor appearance.
SPACING is pixel distance between lines (default 2).
OPACITY is a percentage 0-100 (default 8).
COLOR is an RGB hex string for scanline color (default "#000000").  */)
  (Lisp_Object enabled, Lisp_Object spacing, Lisp_Object opacity, Lisp_Object color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int sp = 2, op = 8;
  int r = 0, g = 0, b = 0;
  if (FIXNUMP (spacing)) sp = XFIXNUM (spacing);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_scanlines (dpyinfo->display_handle, on, sp, op, r, g, b);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-comet",
       Fneomacs_set_cursor_comet,
       Sneomacs_set_cursor_comet, 0, 5, 0,
       doc: /* Configure cursor comet tail effect.
ENABLED non-nil leaves a fading trail of ghost cursor shapes along
the cursor path as it moves.
TRAIL-LENGTH is the number of ghost copies (default 5).
FADE-MS is the fade-out duration in milliseconds (default 300).
COLOR is an RGB hex string (default "#80B3FF").
OPACITY is a percentage 0-100 (default 60).  */)
  (Lisp_Object enabled, Lisp_Object trail_length, Lisp_Object fade_ms,
   Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int tl = 5, fm = 300, op = 60;
  int r = 0x80, g = 0xB3, b = 0xFF;
  if (FIXNUMP (trail_length)) tl = XFIXNUM (trail_length);
  if (FIXNUMP (fade_ms)) fm = XFIXNUM (fade_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_cursor_comet (dpyinfo->display_handle, on, tl, fm, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-spotlight",
       Fneomacs_set_cursor_spotlight,
       Sneomacs_set_cursor_spotlight, 0, 4, 0,
       doc: /* Configure cursor spotlight/radial gradient effect.
ENABLED non-nil renders a radial gradient overlay centered on the
cursor position, creating a subtle spotlight effect.
RADIUS is the spotlight radius in pixels (default 200).
INTENSITY is a percentage 0-100 (default 15).
COLOR is an RGB hex string (default "#FFFFE6").  */)
  (Lisp_Object enabled, Lisp_Object radius, Lisp_Object intensity,
   Lisp_Object color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int rad = 200, inten = 15;
  int r = 0xFF, g = 0xFF, b = 0xE6;
  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (intensity)) inten = XFIXNUM (intensity);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }

  neomacs_display_set_cursor_spotlight (dpyinfo->display_handle, on, rad, inten, r, g, b);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-particles",
       Fneomacs_set_cursor_particles,
       Sneomacs_set_cursor_particles, 0, 5, 0,
       doc: /* Configure cursor particle trail effect.
ENABLED non-nil emits small colored particles that scatter from the
cursor position when it moves, with physics-based motion and fade-out.
COLOR is an RGB hex string (default "#FF9933").
COUNT is the number of particles per cursor move (default 6).
LIFETIME-MS is the particle lifetime in milliseconds (default 800).
GRAVITY is the downward acceleration in pixels/sec^2 (default 120).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object count,
   Lisp_Object lifetime_ms, Lisp_Object gravity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 255, g = 153, b = 51;
  int cnt = 6, lt = 800, grav = 120;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (count)) cnt = XFIXNUM (count);
  if (FIXNUMP (lifetime_ms)) lt = XFIXNUM (lifetime_ms);
  if (FIXNUMP (gravity)) grav = XFIXNUM (gravity);

  neomacs_display_set_cursor_particles (dpyinfo->display_handle, on, r, g, b, cnt, lt, grav);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-window-border-radius",
       Fneomacs_set_window_border_radius,
       Sneomacs_set_window_border_radius, 0, 5, 0,
       doc: /* Configure per-window rounded border.
ENABLED non-nil draws a rounded rectangle border around each
non-minibuffer window content area.
RADIUS is the corner radius in pixels (default 8).
BORDER-WIDTH is the line width in pixels (default 1).
COLOR is an RGB hex string (default "#808080").
OPACITY is a percentage 0-100 (default 30).  */)
  (Lisp_Object enabled, Lisp_Object radius, Lisp_Object border_width,
   Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int rad = 8, bw = 1;
  int r = 128, g = 128, b = 128;
  int op = 30;
  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (border_width)) bw = XFIXNUM (border_width);
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_window_border_radius (dpyinfo->display_handle, on, rad, bw, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-typing-heatmap",
       Fneomacs_set_typing_heatmap,
       Sneomacs_set_typing_heatmap, 0, 4, 0,
       doc: /* Configure typing heat map overlay.
ENABLED non-nil highlights recently-edited character cells with a
decaying colored overlay, creating a visual heat map of editing activity.
COLOR is an RGB hex string (default "#FF6619").
FADE-MS is the fade-out duration in milliseconds (default 2000).
OPACITY is a percentage 0-100 (default 15).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object fade_ms, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 255, g = 102, b = 25;
  int dur = 2000;
  int op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (fade_ms)) dur = XFIXNUM (fade_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_typing_heatmap (dpyinfo->display_handle, on, r, g, b, dur, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-theme-transition",
       Fneomacs_set_theme_transition,
       Sneomacs_set_theme_transition, 0, 2, 0,
       doc: /* Configure smooth theme transition.
ENABLED non-nil crossfades the entire frame when the background color
changes (e.g. after load-theme).
DURATION-MS is the crossfade duration in milliseconds (default 300).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 300;
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_theme_transition (dpyinfo->display_handle, on, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-edge-snap",
       Fneomacs_set_edge_snap,
       Sneomacs_set_edge_snap, 0, 3, 0,
       doc: /* Configure window edge snap indicator.
ENABLED non-nil flashes a gradient bar at the top or bottom edge of
the selected window when the bell rings at buffer boundaries
(beginning-of-buffer or end-of-buffer).
COLOR is an RGB hex string (default "#FF6633").
DURATION-MS is the flash duration in milliseconds (default 200).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 255, g = 102, b = 51;
  int dur = 200;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_edge_snap (dpyinfo->display_handle, on, r, g, b, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-wrap-indicator",
       Fneomacs_set_wrap_indicator,
       Sneomacs_set_wrap_indicator, 0, 3, 0,
       doc: /* Configure line wrap indicator overlay.
ENABLED non-nil renders a subtle gradient at the right edge of lines
that wrap, providing a visual cue for line wrapping.
COLOR is an RGB hex string (default "#8099CC").
OPACITY is a percentage 0-100 (default 30).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 128, g = 153, b = 204;
  int op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_wrap_indicator (dpyinfo->display_handle, on, r, g, b, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-scroll-momentum",
       Fneomacs_set_scroll_momentum,
       Sneomacs_set_scroll_momentum, 0, 3, 0,
       doc: /* Configure per-window scroll momentum indicator.
ENABLED non-nil shows a brief directional gradient bar at the edge of
each window during scrolling, indicating scroll direction.
FADE-MS is the fade-out duration in milliseconds (default 300).
WIDTH is the bar width in pixels (default 3).  */)
  (Lisp_Object enabled, Lisp_Object fade_ms, Lisp_Object width)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int fade = 300;
  int w = 3;
  if (FIXNUMP (fade_ms)) fade = XFIXNUM (fade_ms);
  if (FIXNUMP (width)) w = XFIXNUM (width);

  neomacs_display_set_scroll_momentum (dpyinfo->display_handle, on, fade, w);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-trail-fade",
       Fneomacs_set_cursor_trail_fade,
       Sneomacs_set_cursor_trail_fade, 0, 3, 0,
       doc: /* Configure cursor trail fade effect.
ENABLED non-nil leaves fading ghost rectangles at previous cursor
positions as the cursor moves, creating a trail effect.
LENGTH is the maximum number of trail positions to keep (default 8).
FADE-MS is the fade duration in milliseconds for each ghost (default 300).  */)
  (Lisp_Object enabled, Lisp_Object length, Lisp_Object fade_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int len = 8;
  int fade = 300;
  if (FIXNUMP (length)) len = XFIXNUM (length);
  if (FIXNUMP (fade_ms)) fade = XFIXNUM (fade_ms);

  neomacs_display_set_cursor_trail_fade (dpyinfo->display_handle, on, len, fade);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-noise-grain",
       Fneomacs_set_noise_grain,
       Sneomacs_set_noise_grain, 0, 3, 0,
       doc: /* Configure noise/film grain texture overlay.
ENABLED non-nil renders a subtle animated grain pattern over the
entire frame, simulating a CRT or film look.
INTENSITY is 0-100 for grain visibility (default 3).
SIZE is the grain cell size in pixels (default 2).  */)
  (Lisp_Object enabled, Lisp_Object intensity, Lisp_Object size)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int inten = 3;
  int sz = 2;
  if (FIXNUMP (intensity)) inten = XFIXNUM (intensity);
  if (FIXNUMP (size)) sz = XFIXNUM (size);

  neomacs_display_set_noise_grain (dpyinfo->display_handle, on, inten, sz);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-matrix-rain",
       Fneomacs_set_matrix_rain,
       Sneomacs_set_matrix_rain, 0, 4, 0,
       doc: /* Configure matrix/digital rain effect.
ENABLED non-nil renders animated vertical columns of green cascading
drops across the frame, reminiscent of the Matrix film.
COLOR is an RGB hex string (default "#00CC33").
SPEED is the fall speed in pixels/sec (default 150).
OPACITY is a percentage 0-100 (default 12).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object speed,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 0x00, g = 0xCC, b = 0x33;
  int col_count = 40, spd = 150, op = 12;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_matrix_rain (dpyinfo->display_handle, on, r, g, b, col_count, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-elastic-snap",
       Fneomacs_set_cursor_elastic_snap,
       Sneomacs_set_cursor_elastic_snap, 0, 3, 0,
       doc: /* Configure cursor elastic snap animation.
ENABLED non-nil makes the cursor overshoot its target position slightly
and bounce back, creating a rubber-band effect.
OVERSHOOT is the overshoot amount as percentage (default 15).
DURATION-MS is the snap animation duration in milliseconds (default 200).  */)
  (Lisp_Object enabled, Lisp_Object overshoot, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int os = 15, dur = 200;
  if (FIXNUMP (overshoot)) os = XFIXNUM (overshoot);
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);

  neomacs_display_set_cursor_elastic_snap (dpyinfo->display_handle, on, os, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-frost-border",
       Fneomacs_set_frost_border_effect,
       Sneomacs_set_frost_border_effect, 0, 4, 0,
       doc: /* Configure window frost/ice border effect.
ENABLED non-nil renders irregular crystalline frost patterns around
the edges of each window, simulating ice crystals.
COLOR is an RGB hex string (default "#B3D9FF").
WIDTH is the frost border width in pixels (default 6).
OPACITY is a percentage 0-100 (default 20).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object width,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 0xB3, g = 0xD9, b = 0xFF;
  int w = 6, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (width)) w = XFIXNUM (width);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_frost_border_effect (dpyinfo->display_handle, on, r, g, b, w, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-ghost",
       Fneomacs_set_cursor_ghost,
       Sneomacs_set_cursor_ghost, 0, 4, 0,
       doc: /* Configure cursor afterimage ghost effect.
ENABLED non-nil draws ghost copies of the cursor at previous positions
that slowly fade and drift upward, creating a spectral echo.
COLOR is an RGB hex string (default "#8080FF").
FADE-MS is the ghost duration in milliseconds (default 600).
OPACITY is a percentage 0-100 (default 40).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object fade_ms,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 0x80, g = 0x80, b = 0xFF;
  int count = 4, fm = 600, drift = 20, op = 40;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (fade_ms)) fm = XFIXNUM (fade_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_ghost (dpyinfo->display_handle, on, r, g, b, count, fm, drift, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-edge-glow",
       Fneomacs_set_edge_glow,
       Sneomacs_set_edge_glow, 0, 4, 0,
       doc: /* Configure window edge glow on scroll boundaries.
ENABLED non-nil shows a soft glow at buffer edges when scrolling reaches
beginning-of-buffer or end-of-buffer.
COLOR is an RGB hex string (default "#6699FF").
HEIGHT is glow height in pixels (default 40).
OPACITY is a percentage 0-100 (default 30).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object height,
   Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 0x66, g = 0x99, b = 0xFF;
  int h = 40, op = 30, fade = 400;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (height)) h = XFIXNUM (height);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_edge_glow (dpyinfo->display_handle, on, r, g, b, h, op, fade);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-rain-effect",
       Fneomacs_set_rain_effect,
       Sneomacs_set_rain_effect, 0, 5, 0,
       doc: /* Configure window rain/drip ambient effect.
ENABLED non-nil renders animated vertical rain drops falling across
the frame as a decorative ambient animation.
COLOR is an RGB hex string (default "#8099CC").
DROP-COUNT is the number of simultaneous drops (default 30).
SPEED is fall speed in pixels/sec (default 120).
OPACITY is a percentage 0-100 (default 15).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object drop_count,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 0x80, g = 0x99, b = 0xCC;
  int dc = 30, spd = 120, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (drop_count)) dc = XFIXNUM (drop_count);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_rain_effect (dpyinfo->display_handle, on, r, g, b, dc, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-ripple-wave",
       Fneomacs_set_cursor_ripple_wave,
       Sneomacs_set_cursor_ripple_wave, 0, 5, 0,
       doc: /* Configure cursor ripple wave effect.
ENABLED non-nil draws expanding concentric ring waves from the cursor
position each time it moves.
COLOR is an RGB hex string (default "#6699FF").
MAX-RADIUS is the maximum wave radius in pixels (default 80).
DURATION-MS is the wave duration in milliseconds (default 500).
OPACITY is a percentage 0-100 (default 30).  */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object max_radius,
   Lisp_Object duration_ms, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 0x66, g = 0x99, b = 0xFF;
  int rc = 3, mr = 80, dur = 500, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (max_radius)) mr = XFIXNUM (max_radius);
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_ripple_wave (dpyinfo->display_handle, on, r, g, b, rc, mr, dur, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-aurora",
       Fneomacs_set_aurora,
       Sneomacs_set_aurora, 0, 5, 0,
       doc: /* Configure window aurora/northern lights effect.
ENABLED non-nil renders animated flowing color bands at the top
of the frame, reminiscent of aurora borealis.
COLOR1 is the primary RGB hex string (default "#33CC66").
COLOR2 is the secondary RGB hex string (default "#4D66E6").
HEIGHT is band height in pixels (default 60).
OPACITY is a percentage 0-100 (default 12).  */)
  (Lisp_Object enabled, Lisp_Object color1, Lisp_Object color2,
   Lisp_Object height, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r1 = 0x33, g1 = 0xCC, b1 = 0x66;
  int r2 = 0x4D, g2 = 0x66, b2 = 0xE6;
  int h = 60, op = 12, spd = 100;
  if (STRINGP (color1))
    {
      const char *s = SSDATA (color1);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r1 = (hex >> 16) & 0xFF;
          g1 = (hex >> 8) & 0xFF;
          b1 = hex & 0xFF;
        }
    }
  if (STRINGP (color2))
    {
      const char *s = SSDATA (color2);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned hex = 0;
          sscanf (s + 1, "%06x", &hex);
          r2 = (hex >> 16) & 0xFF;
          g2 = (hex >> 8) & 0xFF;
          b2 = hex & 0xFF;
        }
    }
  if (FIXNUMP (height)) h = XFIXNUM (height);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_aurora (dpyinfo->display_handle, on, r1, g1, b1, r2, g2, b2, h, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-heat-distortion",
       Fneomacs_set_heat_distortion,
       Sneomacs_set_heat_distortion, 0, 5, 0,
       doc: /* Configure heat distortion/shimmer effect along window edges.
ENABLED non-nil activates the effect.
INTENSITY is 0-100 for distortion strength (default 30).
SPEED is 0-100 for animation speed (default 100).
EDGE-WIDTH is pixel width of affected region (default 30).
OPACITY is 0-100 for peak opacity (default 15). */)
  (Lisp_Object enabled, Lisp_Object intensity, Lisp_Object speed,
   Lisp_Object edge_width, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int intens = 30, spd = 100, ew = 30, op = 15;
  if (FIXNUMP (intensity)) intens = XFIXNUM (intensity);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (edge_width)) ew = XFIXNUM (edge_width);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_heat_distortion (dpyinfo->display_handle, on, intens, spd, ew, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-lighthouse",
       Fneomacs_set_cursor_lighthouse,
       Sneomacs_set_cursor_lighthouse, 0, 7, 0,
       doc: /* Configure cursor lighthouse beam effect.
ENABLED non-nil activates a rotating beam from cursor position.
COLOR is a hex color string like "#RRGGBB" (default "#FFE64D").
BEAM-WIDTH is angular width in degrees (default 15).
ROTATION-SPEED is revolutions per second * 100 (default 50).
BEAM-LENGTH is max beam reach in pixels (default 200).
OPACITY is 0-100 for beam opacity (default 12). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object beam_width,
   Lisp_Object rotation_speed, Lisp_Object beam_length, Lisp_Object opacity,
   Lisp_Object _dummy)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 255, g = 230, b = 77, bw = 15, rs = 50, bl = 200, op = 12;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (beam_width)) bw = XFIXNUM (beam_width);
  if (FIXNUMP (rotation_speed)) rs = XFIXNUM (rotation_speed);
  if (FIXNUMP (beam_length)) bl = XFIXNUM (beam_length);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_lighthouse (dpyinfo->display_handle, on, r, g, b, bw, rs, bl, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-neon-border",
       Fneomacs_set_neon_border,
       Sneomacs_set_neon_border, 0, 7, 0,
       doc: /* Configure neon border glow effect.
ENABLED non-nil activates the neon border.
COLOR is a hex color string like "#RRGGBB" (default "#00FFCC").
INTENSITY is 0-100 for glow intensity (default 60).
FLICKER is 0-100 for flicker amount (default 10).
THICKNESS is border thickness in pixels (default 3).
OPACITY is 0-100 for overall opacity (default 40). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object intensity,
   Lisp_Object flicker, Lisp_Object thickness, Lisp_Object opacity,
   Lisp_Object _dummy)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0, g = 255, b = 204, intens = 60, fl = 10, th = 3, op = 40;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (intensity)) intens = XFIXNUM (intensity);
  if (FIXNUMP (flicker)) fl = XFIXNUM (flicker);
  if (FIXNUMP (thickness)) th = XFIXNUM (thickness);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_neon_border (dpyinfo->display_handle, on, r, g, b, intens, fl, th, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-sonar-ping",
       Fneomacs_set_cursor_sonar_ping,
       Sneomacs_set_cursor_sonar_ping, 0, 7, 0,
       doc: /* Configure cursor sonar ping effect.
ENABLED non-nil activates expanding concentric rings from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#4DB3FF").
RING-COUNT is the number of concentric rings per ping (default 3).
MAX-RADIUS is maximum expansion radius in pixels (default 60).
DURATION-MS is duration of each ping in milliseconds (default 600).
OPACITY is 0-100 for ring opacity (default 25). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ring_count,
   Lisp_Object max_radius, Lisp_Object duration_ms, Lisp_Object opacity,
   Lisp_Object _dummy)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 77, g = 179, b = 255, rc = 3, mr = 60, dur = 600, op = 25;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (ring_count)) rc = XFIXNUM (ring_count);
  if (FIXNUMP (max_radius)) mr = XFIXNUM (max_radius);
  if (FIXNUMP (duration_ms)) dur = XFIXNUM (duration_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_sonar_ping (dpyinfo->display_handle, on, r, g, b, rc, mr, dur, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-lightning-bolt",
       Fneomacs_set_lightning_bolt,
       Sneomacs_set_lightning_bolt, 0, 5, 0,
       doc: /* Configure lightning bolt effect along window borders.
ENABLED non-nil activates random electrical discharge effects.
COLOR is a hex color string like "#RRGGBB" (default "#B3CCFF").
FREQUENCY is bolts per second * 100 (default 100 = 1.0/sec).
INTENSITY is 0-100 for bolt brightness (default 80).
OPACITY is 0-100 for overall opacity (default 30). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object frequency,
   Lisp_Object intensity, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xB3, g = 0xCC, b = 0xFF, freq = 100, intens = 80, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (frequency)) freq = XFIXNUM (frequency);
  if (FIXNUMP (intensity)) intens = XFIXNUM (intensity);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_lightning_bolt (dpyinfo->display_handle, on, r, g, b, freq, intens, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-orbit-particles",
       Fneomacs_set_cursor_orbit_particles,
       Sneomacs_set_cursor_orbit_particles, 0, 6, 0,
       doc: /* Configure cursor orbit particles effect.
ENABLED non-nil activates particles orbiting the cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FFCC4D").
PARTICLE-COUNT is number of orbiting particles (default 6).
ORBIT-RADIUS is orbit radius in pixels (default 25).
SPEED is orbit speed * 100 (default 150 = 1.5 rev/sec).
OPACITY is 0-100 for particle opacity (default 35). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object particle_count,
   Lisp_Object orbit_radius, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xCC, b = 0x4D, pc = 6, orr = 25, spd = 150, op = 35;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (particle_count)) pc = XFIXNUM (particle_count);
  if (FIXNUMP (orbit_radius)) orr = XFIXNUM (orbit_radius);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_orbit_particles (dpyinfo->display_handle, on, r, g, b, pc, orr, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-plasma-border",
       Fneomacs_set_plasma_border,
       Sneomacs_set_plasma_border, 0, 6, 0,
       doc: /* Configure animated plasma border effect.
ENABLED non-nil activates the plasma lava-lamp border.
COLOR1 is the primary RGB hex string (default "#FF3380").
COLOR2 is the secondary RGB hex string (default "#3380FF").
WIDTH is border width in pixels (default 4).
SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 for overall opacity (default 30). */)
  (Lisp_Object enabled, Lisp_Object color1, Lisp_Object color2,
   Lisp_Object width, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r1 = 0xFF, g1 = 0x33, b1 = 0x80;
  int r2 = 0x33, g2 = 0x80, b2 = 0xFF;
  int w = 4, spd = 100, op = 30;
  if (STRINGP (color1))
    {
      const char *s = SSDATA (color1);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r1 = (hex >> 16) & 0xFF;
          g1 = (hex >> 8) & 0xFF;
          b1 = hex & 0xFF;
        }
    }
  if (STRINGP (color2))
    {
      const char *s = SSDATA (color2);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r2 = (hex >> 16) & 0xFF;
          g2 = (hex >> 8) & 0xFF;
          b2 = hex & 0xFF;
        }
    }
  if (FIXNUMP (width)) w = XFIXNUM (width);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_plasma_border (dpyinfo->display_handle, on, r1, g1, b1, r2, g2, b2, w, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-heartbeat",
       Fneomacs_set_cursor_heartbeat,
       Sneomacs_set_cursor_heartbeat, 0, 5, 0,
       doc: /* Configure cursor heartbeat pulse effect.
ENABLED non-nil activates double-pulse rings from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FF4D4D").
BPM is beats per minute (default 72).
MAX-RADIUS is max expansion radius in pixels (default 50).
OPACITY is 0-100 for pulse opacity (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object bpm,
   Lisp_Object max_radius, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0x4D, b = 0x4D, bp = 72, mr = 50, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (bpm)) bp = XFIXNUM (bpm);
  if (FIXNUMP (max_radius)) mr = XFIXNUM (max_radius);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_heartbeat (dpyinfo->display_handle, on, r, g, b, bp, mr, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-topo-contour",
       Fneomacs_set_topo_contour,
       Sneomacs_set_topo_contour, 0, 5, 0,
       doc: /* Configure topographic contour effect.
ENABLED non-nil activates flowing contour lines overlay.
COLOR is a hex color string like "#RRGGBB" (default "#66B380").
SPACING is line spacing in pixels (default 30).
SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 (default 10). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object spacing,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x66, g = 0xB3, b = 0x80, sp = 30, spd = 100, op = 10;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (spacing)) sp = XFIXNUM (spacing);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_topo_contour (dpyinfo->display_handle, on, r, g, b, sp, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-metronome",
       Fneomacs_set_cursor_metronome,
       Sneomacs_set_cursor_metronome, 0, 5, 0,
       doc: /* Configure cursor metronome tick effect.
ENABLED non-nil shows a tick mark on cursor movement.
COLOR is a hex color string like "#RRGGBB" (default "#E58033").
TICK-HEIGHT is tick height in pixels (default 20).
FADE-MS is fade duration in milliseconds (default 300).
OPACITY is 0-100 (default 40). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object tick_height,
   Lisp_Object fade_ms, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xE5, g = 0x80, b = 0x33, th = 20, fm = 300, op = 40;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (tick_height)) th = XFIXNUM (tick_height);
  if (FIXNUMP (fade_ms)) fm = XFIXNUM (fade_ms);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_metronome (dpyinfo->display_handle, on, r, g, b, th, fm, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-constellation",
       Fneomacs_set_constellation,
       Sneomacs_set_constellation, 0, 6, 0,
       doc: /* Configure constellation overlay effect.
ENABLED non-nil activates twinkling star pattern.
COLOR is a hex color string like "#RRGGBB" (default "#B3CCFF").
STAR-COUNT is number of stars (default 50).
CONNECT-DIST is connection distance in pixels (default 80).
TWINKLE-SPEED is twinkle speed * 100 (default 100).
OPACITY is 0-100 (default 15). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object star_count,
   Lisp_Object connect_dist, Lisp_Object twinkle_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xB3, g = 0xCC, b = 0xFF, sc = 50, cd = 80, ts = 100, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (star_count)) sc = XFIXNUM (star_count);
  if (FIXNUMP (connect_dist)) cd = XFIXNUM (connect_dist);
  if (FIXNUMP (twinkle_speed)) ts = XFIXNUM (twinkle_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_constellation (dpyinfo->display_handle, on, r, g, b, sc, cd, ts, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-radar",
       Fneomacs_set_cursor_radar,
       Sneomacs_set_cursor_radar, 0, 5, 0,
       doc: /* Configure cursor radar sweep effect.
ENABLED non-nil activates rotating radar sweep around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#33E566").
RADIUS is sweep radius in pixels (default 40).
SPEED is rotation speed * 100 (default 150).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object radius,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x33, g = 0xE5, b = 0x66, rad = 40, spd = 150, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_radar (dpyinfo->display_handle, on, r, g, b, rad, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-kaleidoscope",
       Fneomacs_set_kaleidoscope,
       Sneomacs_set_kaleidoscope, 0, 5, 0,
       doc: /* Configure kaleidoscope overlay effect.
ENABLED non-nil activates mirrored geometric kaleidoscope patterns.
COLOR is a hex color string like "#RRGGBB" (default "#9933E5").
SEGMENTS is number of mirror segments (default 6).
SPEED is rotation speed * 100 (default 50).
OPACITY is 0-100 (default 10). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object segments,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0x33, b = 0xE5, seg = 6, spd = 50, op = 10;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (segments)) seg = XFIXNUM (segments);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_kaleidoscope (dpyinfo->display_handle, on, r, g, b, seg, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-ripple-ring",
       Fneomacs_set_cursor_ripple_ring,
       Sneomacs_set_cursor_ripple_ring, 0, 6, 0,
       doc: /* Configure cursor ripple ring effect.
ENABLED non-nil activates expanding concentric rings on cursor move.
COLOR is a hex color string like "#RRGGBB" (default "#4DCCE5").
MAX-RADIUS is max ring radius in pixels (default 60).
RING-COUNT is number of concentric rings (default 3).
SPEED is expansion speed * 100 (default 200).
OPACITY is 0-100 (default 25). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object max_radius,
   Lisp_Object ring_count, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0xCC, b = 0xE5, mr = 60, rc = 3, spd = 200, op = 25;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (max_radius)) mr = XFIXNUM (max_radius);
  if (FIXNUMP (ring_count)) rc = XFIXNUM (ring_count);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_ripple_ring (dpyinfo->display_handle, on, r, g, b, mr, rc, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-noise-field",
       Fneomacs_set_noise_field,
       Sneomacs_set_noise_field, 0, 5, 0,
       doc: /* Configure noise field overlay effect.
ENABLED non-nil activates animated noise pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#80B34D").
SCALE is noise cell size in pixels (default 50).
SPEED is animation speed * 100 (default 50).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object scale,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x80, g = 0xB3, b = 0x4D, sc = 50, spd = 50, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (scale)) sc = XFIXNUM (scale);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_noise_field (dpyinfo->display_handle, on, r, g, b, sc, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-scope",
       Fneomacs_set_cursor_scope,
       Sneomacs_set_cursor_scope, 0, 5, 0,
       doc: /* Configure cursor scope effect.
ENABLED non-nil activates scope lines through cursor position.
COLOR is a hex color string like "#RRGGBB" (default "#FFCC33").
THICKNESS is line thickness in pixels (default 1).
GAP is gap around cursor in pixels (default 10).
OPACITY is 0-100 (default 30). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object thickness,
   Lisp_Object gap, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xCC, b = 0x33, th = 1, gp = 10, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (thickness)) th = XFIXNUM (thickness);
  if (FIXNUMP (gap)) gp = XFIXNUM (gap);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_scope (dpyinfo->display_handle, on, r, g, b, th, gp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-spiral-vortex",
       Fneomacs_set_spiral_vortex,
       Sneomacs_set_spiral_vortex, 0, 5, 0,
       doc: /* Configure spiral vortex overlay effect.
ENABLED non-nil activates spiraling pattern from window center.
COLOR is a hex color string like "#RRGGBB" (default "#6633CC").
ARMS is number of spiral arms (default 4).
SPEED is rotation speed * 100 (default 50).
OPACITY is 0-100 (default 10). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object arms,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x66, g = 0x33, b = 0xCC, ar = 4, spd = 50, op = 10;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (arms)) ar = XFIXNUM (arms);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_spiral_vortex (dpyinfo->display_handle, on, r, g, b, ar, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-shockwave",
       Fneomacs_set_cursor_shockwave,
       Sneomacs_set_cursor_shockwave, 0, 5, 0,
       doc: /* Configure cursor shockwave effect.
ENABLED non-nil activates expanding shockwave ring on cursor move.
COLOR is a hex color string like "#RRGGBB" (default "#FF9933").
RADIUS is max expansion radius in pixels (default 80).
DECAY is decay speed * 100 (default 200).
OPACITY is 0-100 (default 30). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object radius,
   Lisp_Object decay, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0x99, b = 0x33, rad = 80, dec = 200, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (decay)) dec = XFIXNUM (decay);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_shockwave (dpyinfo->display_handle, on, r, g, b, rad, dec, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-diamond-lattice",
       Fneomacs_set_diamond_lattice,
       Sneomacs_set_diamond_lattice, 0, 5, 0,
       doc: /* Configure diamond lattice overlay effect.
ENABLED non-nil activates tessellated diamond pattern.
COLOR is a hex color string like "#RRGGBB" (default "#B380E5").
CELL-SIZE is diamond cell size in pixels (default 30).
SHIMMER-SPEED is shimmer animation speed * 100 (default 80).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object cell_size,
   Lisp_Object shimmer_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xB3, g = 0x80, b = 0xE5, cs = 30, ss = 80, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (cell_size)) cs = XFIXNUM (cell_size);
  if (FIXNUMP (shimmer_speed)) ss = XFIXNUM (shimmer_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_diamond_lattice (dpyinfo->display_handle, on, r, g, b, cs, ss, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-gravity-well",
       Fneomacs_set_cursor_gravity_well,
       Sneomacs_set_cursor_gravity_well, 0, 5, 0,
       doc: /* Configure cursor gravity well effect.
ENABLED non-nil activates gravity field lines curving toward cursor.
COLOR is a hex color string like "#RRGGBB" (default "#4D99FF").
FIELD-RADIUS is field radius in pixels (default 80).
LINE-COUNT is number of field lines (default 8).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object field_radius,
   Lisp_Object line_count, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x99, b = 0xFF, fr = 80, lc = 8, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (field_radius)) fr = XFIXNUM (field_radius);
  if (FIXNUMP (line_count)) lc = XFIXNUM (line_count);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_gravity_well (dpyinfo->display_handle, on, r, g, b, fr, lc, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-tessellation",
       Fneomacs_set_tessellation,
       Sneomacs_set_tessellation, 0, 5, 0,
       doc: /* Configure tessellation overlay effect.
ENABLED non-nil activates geometric tessellation pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#8080B3").
TILE-SIZE is tile size in pixels (default 40).
ROTATION is rotation angle * 100 (default 0).
OPACITY is 0-100 (default 4). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object tile_size,
   Lisp_Object rotation, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x80, g = 0x80, b = 0xB3, ts = 40, rot = 0, op = 4;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (tile_size)) ts = XFIXNUM (tile_size);
  if (FIXNUMP (rotation)) rot = XFIXNUM (rotation);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_tessellation (dpyinfo->display_handle, on, r, g, b, ts, rot, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-water-drop",
       Fneomacs_set_cursor_water_drop,
       Sneomacs_set_cursor_water_drop, 0, 5, 0,
       doc: /* Configure cursor water drop effect.
ENABLED non-nil activates concentric ripples expanding from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#4D99E6").
RIPPLE-COUNT is number of ripple rings (default 4).
EXPAND-SPEED is expansion speed * 100 (default 100).
OPACITY is 0-100 (default 15). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ripple_count,
   Lisp_Object expand_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x99, b = 0xE6, rc = 4, es = 100, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (ripple_count)) rc = XFIXNUM (ripple_count);
  if (FIXNUMP (expand_speed)) es = XFIXNUM (expand_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_water_drop (dpyinfo->display_handle, on, r, g, b, rc, es, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-guilloche",
       Fneomacs_set_guilloche,
       Sneomacs_set_guilloche, 0, 5, 0,
       doc: /* Configure guilloche overlay effect.
ENABLED non-nil activates intricate spirograph-like curve pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#9966B3").
CURVE-COUNT is number of overlapping curves (default 8).
WAVE-FREQ is wave frequency * 100 (default 100).
OPACITY is 0-100 (default 5). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object curve_count,
   Lisp_Object wave_freq, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0x66, b = 0xB3, cc = 8, wf = 100, op = 5;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (curve_count)) cc = XFIXNUM (curve_count);
  if (FIXNUMP (wave_freq)) wf = XFIXNUM (wave_freq);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_guilloche (dpyinfo->display_handle, on, r, g, b, cc, wf, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-pixel-dust",
       Fneomacs_set_cursor_pixel_dust,
       Sneomacs_set_cursor_pixel_dust, 0, 5, 0,
       doc: /* Configure cursor pixel dust effect.
ENABLED non-nil activates pixel particles scattering from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#CCCC99").
DUST-COUNT is number of dust particles (default 15).
SCATTER-SPEED is scatter speed * 100 (default 100).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object dust_count,
   Lisp_Object scatter_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xCC, g = 0xCC, b = 0x99, dc = 15, ss = 100, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (dust_count)) dc = XFIXNUM (dust_count);
  if (FIXNUMP (scatter_speed)) ss = XFIXNUM (scatter_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_pixel_dust (dpyinfo->display_handle, on, r, g, b, dc, ss, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-celtic-knot",
       Fneomacs_set_celtic_knot,
       Sneomacs_set_celtic_knot, 0, 5, 0,
       doc: /* Configure celtic knot overlay effect.
ENABLED non-nil activates interlocking loop pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#009933").
KNOT-SCALE is size of each knot cell in pixels (default 60).
WEAVE-SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object knot_scale,
   Lisp_Object weave_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x00, g = 0x99, b = 0x33, ks = 60, ws = 100, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (knot_scale)) ks = XFIXNUM (knot_scale);
  if (FIXNUMP (weave_speed)) ws = XFIXNUM (weave_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_celtic_knot (dpyinfo->display_handle, on, r, g, b, ks, ws, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-candle-flame",
       Fneomacs_set_cursor_candle_flame,
       Sneomacs_set_cursor_candle_flame, 0, 5, 0,
       doc: /* Configure cursor candle flame effect.
ENABLED non-nil activates flickering flame above cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FFB333").
FLAME-HEIGHT is height in pixels (default 20).
FLICKER-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object flame_height,
   Lisp_Object flicker_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xB3, b = 0x33, fh = 20, fs = 100, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (flame_height)) fh = XFIXNUM (flame_height);
  if (FIXNUMP (flicker_speed)) fs = XFIXNUM (flicker_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_candle_flame (dpyinfo->display_handle, on, r, g, b, fh, fs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-argyle-pattern",
       Fneomacs_set_argyle_pattern,
       Sneomacs_set_argyle_pattern, 0, 5, 0,
       doc: /* Configure argyle pattern overlay effect.
ENABLED non-nil activates diamond checker with diagonal lines.
COLOR is a hex color string like "#RRGGBB" (default "#804D4D").
DIAMOND-SIZE is size of each diamond in pixels (default 30).
LINE-WIDTH is width of diagonal lines in pixels (default 1).
OPACITY is 0-100 (default 5). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object diamond_size,
   Lisp_Object line_width, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x80, g = 0x4D, b = 0x4D, ds = 30, lw = 1, op = 5;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (diamond_size)) ds = XFIXNUM (diamond_size);
  if (FIXNUMP (line_width)) lw = XFIXNUM (line_width);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_argyle_pattern (dpyinfo->display_handle, on, r, g, b, ds, lw, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-moth-flame",
       Fneomacs_set_cursor_moth_flame,
       Sneomacs_set_cursor_moth_flame, 0, 5, 0,
       doc: /* Configure cursor moth flame effect.
ENABLED non-nil activates moth-like particles orbiting cursor.
COLOR is a hex color string like "#RRGGBB" (default "#CCB380").
MOTH-COUNT is number of moths (default 5).
ORBIT-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 18). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object moth_count,
   Lisp_Object orbit_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xCC, g = 0xB3, b = 0x80, mc = 5, os = 100, op = 18;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (moth_count)) mc = XFIXNUM (moth_count);
  if (FIXNUMP (orbit_speed)) os = XFIXNUM (orbit_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_moth_flame (dpyinfo->display_handle, on, r, g, b, mc, os, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-basket-weave",
       Fneomacs_set_basket_weave,
       Sneomacs_set_basket_weave, 0, 5, 0,
       doc: /* Configure basket weave overlay effect.
ENABLED non-nil activates interlocking strip pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#8C6640").
STRIP-WIDTH is width of each strip in pixels (default 6).
STRIP-SPACING is spacing between strips in pixels (default 20).
OPACITY is 0-100 (default 5). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object strip_width,
   Lisp_Object strip_spacing, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x8C, g = 0x66, b = 0x40, sw = 6, ss = 20, op = 5;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (strip_width)) sw = XFIXNUM (strip_width);
  if (FIXNUMP (strip_spacing)) ss = XFIXNUM (strip_spacing);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_basket_weave (dpyinfo->display_handle, on, r, g, b, sw, ss, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-sparkler",
       Fneomacs_set_cursor_sparkler,
       Sneomacs_set_cursor_sparkler, 0, 5, 0,
       doc: /* Configure cursor sparkler effect.
ENABLED non-nil activates radiating sparks around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FFD94D").
SPARK-COUNT is number of sparks (default 12).
BURN-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 25). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object spark_count,
   Lisp_Object burn_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xD9, b = 0x4D, sc = 12, bs = 100, op = 25;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (spark_count)) sc = XFIXNUM (spark_count);
  if (FIXNUMP (burn_speed)) bs = XFIXNUM (burn_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_sparkler (dpyinfo->display_handle, on, r, g, b, sc, bs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-fish-scale",
       Fneomacs_set_fish_scale,
       Sneomacs_set_fish_scale, 0, 5, 0,
       doc: /* Configure fish scale overlay effect.
ENABLED non-nil activates tessellated semicircular scale pattern.
COLOR is a hex color string like "#RRGGBB" (default "#4D99B3").
SCALE-SIZE is size of each scale in pixels (default 16).
ROW-OFFSET is offset between rows * 100 (default 50).
OPACITY is 0-100 (default 4). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object scale_size,
   Lisp_Object row_offset, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x99, b = 0xB3, sz = 16, ro = 50, op = 4;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (scale_size)) sz = XFIXNUM (scale_size);
  if (FIXNUMP (row_offset)) ro = XFIXNUM (row_offset);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_fish_scale (dpyinfo->display_handle, on, r, g, b, sz, ro, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-plasma-ball",
       Fneomacs_set_cursor_plasma_ball,
       Sneomacs_set_cursor_plasma_ball, 0, 5, 0,
       doc: /* Configure cursor plasma ball effect.
ENABLED non-nil activates electric tendrils around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#B34DFF").
TENDRIL-COUNT is number of tendrils (default 6).
ARC-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object tendril_count,
   Lisp_Object arc_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xB3, g = 0x4D, b = 0xFF, tc = 6, as_ = 100, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (tendril_count)) tc = XFIXNUM (tendril_count);
  if (FIXNUMP (arc_speed)) as_ = XFIXNUM (arc_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_plasma_ball (dpyinfo->display_handle, on, r, g, b, tc, as_, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-trefoil-knot",
       Fneomacs_set_trefoil_knot,
       Sneomacs_set_trefoil_knot, 0, 5, 0,
       doc: /* Configure trefoil knot overlay effect.
ENABLED non-nil activates animated trefoil knot pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#6699E6").
KNOT-SIZE is size in pixels (default 80).
ROTATION-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object knot_size,
   Lisp_Object rotation_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x66, g = 0x99, b = 0xE6, ks = 80, rs = 100, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (knot_size)) ks = XFIXNUM (knot_size);
  if (FIXNUMP (rotation_speed)) rs = XFIXNUM (rotation_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_trefoil_knot (dpyinfo->display_handle, on, r, g, b, ks, rs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-quill-pen",
       Fneomacs_set_cursor_quill_pen,
       Sneomacs_set_cursor_quill_pen, 0, 5, 0,
       doc: /* Configure cursor quill pen effect.
ENABLED non-nil activates ink drips and flourishes around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#4D260D").
TRAIL-LENGTH is number of ink drips (default 8).
INK-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object trail_length,
   Lisp_Object ink_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x26, b = 0x0D, tl = 8, is = 100, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (trail_length)) tl = XFIXNUM (trail_length);
  if (FIXNUMP (ink_speed)) is = XFIXNUM (ink_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_quill_pen (dpyinfo->display_handle, on, r, g, b, tl, is, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-herringbone-pattern",
       Fneomacs_set_herringbone_pattern,
       Sneomacs_set_herringbone_pattern, 0, 5, 0,
       doc: /* Configure herringbone pattern overlay effect.
ENABLED non-nil activates V-shaped weave pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#998066").
TILE-WIDTH is width of each tile in pixels (default 20).
TILE-HEIGHT is height of each tile in pixels (default 10).
OPACITY is 0-100 (default 5). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object tile_width,
   Lisp_Object tile_height, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0x80, b = 0x66, tw = 20, th = 10, op = 5;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (tile_width)) tw = XFIXNUM (tile_width);
  if (FIXNUMP (tile_height)) th = XFIXNUM (tile_height);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_herringbone_pattern (dpyinfo->display_handle, on, r, g, b, tw, th, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-aurora-borealis",
       Fneomacs_set_cursor_aurora_borealis,
       Sneomacs_set_cursor_aurora_borealis, 0, 5, 0,
       doc: /* Configure cursor aurora borealis effect.
ENABLED non-nil activates shimmering aurora bands above cursor.
COLOR is a hex color string like "#RRGGBB" (default "#33E680").
BAND-COUNT is number of aurora bands (default 5).
SHIMMER-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 15). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object band_count,
   Lisp_Object shimmer_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x33, g = 0xE6, b = 0x80, bc = 5, ss = 100, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (band_count)) bc = XFIXNUM (band_count);
  if (FIXNUMP (shimmer_speed)) ss = XFIXNUM (shimmer_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_aurora_borealis (dpyinfo->display_handle, on, r, g, b, bc, ss, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-target-reticle",
       Fneomacs_set_target_reticle,
       Sneomacs_set_target_reticle, 0, 5, 0,
       doc: /* Configure target reticle overlay effect.
ENABLED non-nil activates animated reticle rings overlay.
COLOR is a hex color string like "#RRGGBB" (default "#33CC33").
RING-COUNT is number of concentric rings (default 3).
PULSE-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ring_count,
   Lisp_Object pulse_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x33, g = 0xCC, b = 0x33, rc = 3, ps = 100, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (ring_count)) rc = XFIXNUM (ring_count);
  if (FIXNUMP (pulse_speed)) ps = XFIXNUM (pulse_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_target_reticle (dpyinfo->display_handle, on, r, g, b, rc, ps, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-feather",
       Fneomacs_set_cursor_feather,
       Sneomacs_set_cursor_feather, 0, 5, 0,
       doc: /* Configure cursor feather effect.
ENABLED non-nil activates floating feather wisps around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#E6D9B3").
FEATHER-COUNT is number of feather wisps (default 4).
DRIFT-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 18). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object feather_count,
   Lisp_Object drift_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xE6, g = 0xD9, b = 0xB3, fc = 4, ds = 100, op = 18;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (feather_count)) fc = XFIXNUM (feather_count);
  if (FIXNUMP (drift_speed)) ds = XFIXNUM (drift_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_feather (dpyinfo->display_handle, on, r, g, b, fc, ds, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-plaid-pattern",
       Fneomacs_set_plaid_pattern,
       Sneomacs_set_plaid_pattern, 0, 5, 0,
       doc: /* Configure plaid pattern overlay effect.
ENABLED non-nil activates crossing bands pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#B34D4D").
BAND-WIDTH is width of each band in pixels (default 4).
BAND-SPACING is spacing between bands in pixels (default 30).
OPACITY is 0-100 (default 5). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object band_width,
   Lisp_Object band_spacing, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xB3, g = 0x4D, b = 0x4D, bw = 4, bs = 30, op = 5;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (band_width)) bw = XFIXNUM (band_width);
  if (FIXNUMP (band_spacing)) bs = XFIXNUM (band_spacing);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_plaid_pattern (dpyinfo->display_handle, on, r, g, b, bw, bs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-stardust",
       Fneomacs_set_cursor_stardust,
       Sneomacs_set_cursor_stardust, 0, 5, 0,
       doc: /* Configure cursor stardust effect.
ENABLED non-nil activates falling stardust particles around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FFE680").
PARTICLE-COUNT is number of particles (default 20).
FALL-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object particle_count,
   Lisp_Object fall_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xE6, b = 0x80, pc = 20, fs = 100, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (particle_count)) pc = XFIXNUM (particle_count);
  if (FIXNUMP (fall_speed)) fs = XFIXNUM (fall_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_stardust (dpyinfo->display_handle, on, r, g, b, pc, fs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-brick-wall",
       Fneomacs_set_brick_wall,
       Sneomacs_set_brick_wall, 0, 5, 0,
       doc: /* Configure brick wall overlay effect.
ENABLED non-nil activates offset brick pattern overlay.
COLOR is a hex color string like "#RRGGBB" (default "#996644").
BRICK-WIDTH is width of each brick in pixels (default 40).
BRICK-HEIGHT is height of each brick in pixels (default 20).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object brick_width,
   Lisp_Object brick_height, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0x66, b = 0x44, bw = 40, bh = 20, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (brick_width)) bw = XFIXNUM (brick_width);
  if (FIXNUMP (brick_height)) bh = XFIXNUM (brick_height);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_brick_wall (dpyinfo->display_handle, on, r, g, b, bw, bh, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-compass-needle",
       Fneomacs_set_cursor_compass_needle,
       Sneomacs_set_cursor_compass_needle, 0, 5, 0,
       doc: /* Configure cursor compass needle effect.
ENABLED non-nil activates spinning needle around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FF4D4D").
NEEDLE-LENGTH is length in pixels (default 20).
SPIN-SPEED is speed * 100 (default 200).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object needle_length,
   Lisp_Object spin_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0x4D, b = 0x4D, nl = 20, ss = 200, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (needle_length)) nl = XFIXNUM (needle_length);
  if (FIXNUMP (spin_speed)) ss = XFIXNUM (spin_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_compass_needle (dpyinfo->display_handle, on, r, g, b, nl, ss, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-sine-wave",
       Fneomacs_set_sine_wave,
       Sneomacs_set_sine_wave, 0, 6, 0,
       doc: /* Configure sine wave overlay effect.
ENABLED non-nil activates flowing wave line pattern.
COLOR is a hex color string like "#RRGGBB" (default "#4DB3FF").
AMPLITUDE is wave height in pixels (default 20).
WAVELENGTH is distance between peaks in pixels (default 80).
SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object amplitude,
   Lisp_Object wavelength, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0xB3, b = 0xFF, am = 20, wl = 80, sp = 100, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (amplitude)) am = XFIXNUM (amplitude);
  if (FIXNUMP (wavelength)) wl = XFIXNUM (wavelength);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_sine_wave (dpyinfo->display_handle, on, r, g, b, am, wl, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-galaxy",
       Fneomacs_set_cursor_galaxy,
       Sneomacs_set_cursor_galaxy, 0, 5, 0,
       doc: /* Configure cursor galaxy effect.
ENABLED non-nil activates spiraling star particles around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#CCCCFF").
STAR-COUNT is number of stars (default 30).
RADIUS is galaxy radius in pixels (default 30).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object star_count,
   Lisp_Object radius, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xCC, g = 0xCC, b = 0xFF, sc = 30, rd = 30, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (star_count)) sc = XFIXNUM (star_count);
  if (FIXNUMP (radius)) rd = XFIXNUM (radius);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_galaxy (dpyinfo->display_handle, on, r, g, b, sc, rd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-rotating-gear",
       Fneomacs_set_rotating_gear,
       Sneomacs_set_rotating_gear, 0, 5, 0,
       doc: /* Configure rotating gear overlay effect.
ENABLED non-nil activates interlocking gear cogs pattern.
COLOR is a hex color string like "#RRGGBB" (default "#99B3CC").
GEAR-SIZE is size of each gear in pixels (default 40).
ROTATION-SPEED is speed * 100 (default 50).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object gear_size,
   Lisp_Object rotation_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0xB3, b = 0xCC, gs = 40, rs = 50, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (gear_size)) gs = XFIXNUM (gear_size);
  if (FIXNUMP (rotation_speed)) rs = XFIXNUM (rotation_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_rotating_gear (dpyinfo->display_handle, on, r, g, b, gs, rs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-prism",
       Fneomacs_set_cursor_prism,
       Sneomacs_set_cursor_prism, 0, 5, 0,
       doc: /* Configure cursor prism effect.
ENABLED non-nil activates rainbow light refraction from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FFFFFF").
RAY-COUNT is number of prismatic rays (default 7).
SPREAD is ray spread distance in pixels (default 30).
OPACITY is 0-100 (default 15). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ray_count,
   Lisp_Object spread, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xFF, b = 0xFF, rc = 7, sp = 30, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (ray_count)) rc = XFIXNUM (ray_count);
  if (FIXNUMP (spread)) sp = XFIXNUM (spread);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_prism (dpyinfo->display_handle, on, r, g, b, rc, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-crosshatch-pattern",
       Fneomacs_set_crosshatch_pattern,
       Sneomacs_set_crosshatch_pattern, 0, 6, 0,
       doc: /* Configure crosshatch pattern overlay effect.
ENABLED non-nil activates intersecting diagonal line pattern.
COLOR is a hex color string like "#RRGGBB" (default "#809966").
LINE-SPACING is gap between lines in pixels (default 20).
ANGLE is line angle in degrees (default 45).
SPEED is animation speed * 100 (default 30).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object line_spacing,
   Lisp_Object angle, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x80, g = 0x99, b = 0x66, ls = 20, an = 45, sp = 30, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (line_spacing)) ls = XFIXNUM (line_spacing);
  if (FIXNUMP (angle)) an = XFIXNUM (angle);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_crosshatch_pattern (dpyinfo->display_handle, on, r, g, b, ls, an, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-moth",
       Fneomacs_set_cursor_moth,
       Sneomacs_set_cursor_moth, 0, 5, 0,
       doc: /* Configure cursor moth effect.
ENABLED non-nil activates fluttering wing shapes around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#E6CC80").
MOTH-COUNT is number of moths (default 5).
WING-SIZE is wing size in pixels (default 8).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object moth_count,
   Lisp_Object wing_size, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xE6, g = 0xCC, b = 0x80, mc = 5, ws = 8, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (moth_count)) mc = XFIXNUM (moth_count);
  if (FIXNUMP (wing_size)) ws = XFIXNUM (wing_size);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_moth (dpyinfo->display_handle, on, r, g, b, mc, ws, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-concentric-rings",
       Fneomacs_set_concentric_rings,
       Sneomacs_set_concentric_rings, 0, 5, 0,
       doc: /* Configure concentric rings overlay effect.
ENABLED non-nil activates expanding circular wave rings.
COLOR is a hex color string like "#RRGGBB" (default "#6699FF").
RING-SPACING is gap between rings in pixels (default 30).
EXPANSION-SPEED is speed * 100 (default 100).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ring_spacing,
   Lisp_Object expansion_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x66, g = 0x99, b = 0xFF, rs = 30, es = 100, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (ring_spacing)) rs = XFIXNUM (ring_spacing);
  if (FIXNUMP (expansion_speed)) es = XFIXNUM (expansion_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_concentric_rings (dpyinfo->display_handle, on, r, g, b, rs, es, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-flame",
       Fneomacs_set_cursor_flame,
       Sneomacs_set_cursor_flame, 0, 5, 0,
       doc: /* Configure cursor flame effect.
ENABLED non-nil activates rising flame particles from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#FF6633").
PARTICLE-COUNT is number of flame particles (default 12).
HEIGHT is flame height in pixels (default 40).
OPACITY is 0-100 (default 15). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object particle_count,
   Lisp_Object height, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0x66, b = 0x33, pc = 12, ht = 40, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (particle_count)) pc = XFIXNUM (particle_count);
  if (FIXNUMP (height)) ht = XFIXNUM (height);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_flame (dpyinfo->display_handle, on, r, g, b, pc, ht, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-zigzag-pattern",
       Fneomacs_set_zigzag_pattern,
       Sneomacs_set_zigzag_pattern, 0, 6, 0,
       doc: /* Configure zigzag pattern overlay effect.
ENABLED non-nil activates animated zigzag/sawtooth wave lines.
COLOR is a hex color string like "#RRGGBB" (default "#99CC66").
AMPLITUDE is wave height in pixels (default 15).
FREQUENCY is wave frequency * 100 (default 50).
SPEED is animation speed * 100 (default 80).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object amplitude,
   Lisp_Object frequency, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0xCC, b = 0x66, am = 15, fr = 50, sp = 80, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (amplitude)) am = XFIXNUM (amplitude);
  if (FIXNUMP (frequency)) fr = XFIXNUM (frequency);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_zigzag_pattern (dpyinfo->display_handle, on, r, g, b, am, fr, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-crystal",
       Fneomacs_set_cursor_crystal,
       Sneomacs_set_cursor_crystal, 0, 5, 0,
       doc: /* Configure cursor crystal effect.
ENABLED non-nil activates rotating crystal facets around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#AADDFF").
FACET-COUNT is number of crystal facets (default 6).
RADIUS is crystal radius in pixels (default 25).
OPACITY is 0-100 (default 12). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object facet_count,
   Lisp_Object radius, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xAA, g = 0xDD, b = 0xFF, fc = 6, rd = 25, op = 12;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (facet_count)) fc = XFIXNUM (facet_count);
  if (FIXNUMP (radius)) rd = XFIXNUM (radius);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_crystal (dpyinfo->display_handle, on, r, g, b, fc, rd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-moire-pattern",
       Fneomacs_set_moire_pattern,
       Sneomacs_set_moire_pattern, 0, 6, 0,
       doc: /* Configure moiré pattern overlay effect.
ENABLED non-nil activates overlapping grid interference pattern.
COLOR is a hex color string like "#RRGGBB" (default "#8080CC").
LINE-SPACING is spacing between lines in pixels (default 8).
ANGLE-OFFSET is angle between grids in degrees (default 5).
SPEED is rotation speed * 100 (default 30).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object line_spacing,
   Lisp_Object angle_offset, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x80, g = 0x80, b = 0xCC, ls = 8, ao = 5, sp = 30, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (line_spacing)) ls = XFIXNUM (line_spacing);
  if (FIXNUMP (angle_offset)) ao = XFIXNUM (angle_offset);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_moire_pattern (dpyinfo->display_handle, on, r, g, b, ls, ao, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-lightning",
       Fneomacs_set_cursor_lightning,
       Sneomacs_set_cursor_lightning, 0, 5, 0,
       doc: /* Configure cursor lightning effect.
ENABLED non-nil activates electric bolt arcs from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#99CCFF").
BOLT-COUNT is number of lightning bolts (default 4).
MAX-LENGTH is maximum bolt length in pixels (default 50).
OPACITY is 0-100 (default 40). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object bolt_count,
   Lisp_Object max_length, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0xCC, b = 0xFF, bc = 4, ml = 50, op = 40;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (bolt_count)) bc = XFIXNUM (bolt_count);
  if (FIXNUMP (max_length)) ml = XFIXNUM (max_length);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_lightning (dpyinfo->display_handle, on, r, g, b, bc, ml, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-dot-matrix",
       Fneomacs_set_dot_matrix,
       Sneomacs_set_dot_matrix, 0, 5, 0,
       doc: /* Configure dot matrix overlay effect.
ENABLED non-nil activates retro LED-style pulsing dot grid.
COLOR is a hex color string like "#RRGGBB" (default "#4DFF4D").
DOT-SPACING is spacing between dots in pixels (default 12).
PULSE-SPEED is pulse animation speed * 100 (default 100).
OPACITY is 0-100 (default 6). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object dot_spacing,
   Lisp_Object pulse_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0xFF, b = 0x4D, ds = 12, ps = 100, op = 6;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (dot_spacing)) ds = XFIXNUM (dot_spacing);
  if (FIXNUMP (pulse_speed)) ps = XFIXNUM (pulse_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_dot_matrix (dpyinfo->display_handle, on, r, g, b, ds, ps, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-snowflake",
       Fneomacs_set_cursor_snowflake,
       Sneomacs_set_cursor_snowflake, 0, 5, 0,
       doc: /* Configure cursor snowflake effect.
ENABLED non-nil activates crystalline snowflakes drifting from cursor.
COLOR is a hex color string like "#RRGGBB" (default "#CCE5FF").
COUNT is number of snowflakes (default 8).
FALL-SPEED is fall speed in pixels per second (default 30).
OPACITY is 0-100 (default 30). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object count,
   Lisp_Object fall_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xCC, g = 0xE5, b = 0xFF, ct = 8, fs = 30, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (count)) ct = XFIXNUM (count);
  if (FIXNUMP (fall_speed)) fs = XFIXNUM (fall_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_snowflake (dpyinfo->display_handle, on, r, g, b, ct, fs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-sunburst-pattern",
       Fneomacs_set_sunburst_pattern,
       Sneomacs_set_sunburst_pattern, 0, 5, 0,
       doc: /* Configure sunburst pattern overlay effect.
ENABLED non-nil activates radial ray pattern from window center.
COLOR is a hex color string like "#RRGGBB" (default "#FFCC4D").
RAY-COUNT is number of rays (default 12).
SPEED is rotation speed * 100 (default 50).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object ray_count,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xCC, b = 0x4D, rc = 12, sp = 50, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (ray_count)) rc = XFIXNUM (ray_count);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_sunburst_pattern (dpyinfo->display_handle, on, r, g, b, rc, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-firework",
       Fneomacs_set_cursor_firework,
       Sneomacs_set_cursor_firework, 0, 5, 0,
       doc: /* Configure cursor firework effect.
ENABLED non-nil activates firework particle burst on cursor movement.
COLOR is a hex color string like "#RRGGBB" (default "#FF9933").
PARTICLE-COUNT is number of particles (default 16).
BURST-RADIUS is burst radius in pixels (default 60).
OPACITY is 0-100 (default 30). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object particle_count,
   Lisp_Object burst_radius, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0x99, b = 0x33, pc = 16, br = 60, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (particle_count)) pc = XFIXNUM (particle_count);
  if (FIXNUMP (burst_radius)) br = XFIXNUM (burst_radius);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_firework (dpyinfo->display_handle, on, r, g, b, pc, br, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-honeycomb-dissolve",
       Fneomacs_set_honeycomb_dissolve,
       Sneomacs_set_honeycomb_dissolve, 0, 5, 0,
       doc: /* Configure honeycomb dissolve overlay effect.
ENABLED non-nil activates hexagonal cells fading in and out.
COLOR is a hex color string like "#RRGGBB" (default "#CC9933").
CELL-SIZE is hexagonal cell size in pixels (default 30).
DISSOLVE-SPEED is dissolve animation speed * 100 (default 80).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object cell_size,
   Lisp_Object dissolve_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xCC, g = 0x99, b = 0x33, cs = 30, ds = 80, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (cell_size)) cs = XFIXNUM (cell_size);
  if (FIXNUMP (dissolve_speed)) ds = XFIXNUM (dissolve_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_honeycomb_dissolve (dpyinfo->display_handle, on, r, g, b, cs, ds, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-tornado",
       Fneomacs_set_cursor_tornado,
       Sneomacs_set_cursor_tornado, 0, 5, 0,
       doc: /* Configure cursor tornado effect.
ENABLED non-nil activates spinning funnel vortex around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#80B3FF").
RADIUS is tornado radius in pixels (default 40).
PARTICLE-COUNT is number of particles (default 12).
OPACITY is 0-100 (default 25). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object radius,
   Lisp_Object particle_count, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x80, g = 0xB3, b = 0xFF, rd = 40, pc = 12, op = 25;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (radius)) rd = XFIXNUM (radius);
  if (FIXNUMP (particle_count)) pc = XFIXNUM (particle_count);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_tornado (dpyinfo->display_handle, on, r, g, b, rd, pc, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-wave-interference",
       Fneomacs_set_wave_interference,
       Sneomacs_set_wave_interference, 0, 6, 0,
       doc: /* Configure wave interference overlay effect.
ENABLED non-nil activates multi-source wave interference pattern.
COLOR is a hex color string like "#RRGGBB" (default "#4D99E5").
WAVELENGTH is wave wavelength in pixels (default 60).
SOURCE-COUNT is number of wave sources (default 3).
SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 (default 10). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object wavelength,
   Lisp_Object source_count, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x99, b = 0xE5, wl = 60, sc = 3, sp = 100, op = 10;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (wavelength)) wl = XFIXNUM (wavelength);
  if (FIXNUMP (source_count)) sc = XFIXNUM (source_count);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_wave_interference (dpyinfo->display_handle, on, r, g, b, wl, sc, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-portal",
       Fneomacs_set_cursor_portal,
       Sneomacs_set_cursor_portal, 0, 5, 0,
       doc: /* Configure cursor portal effect.
ENABLED non-nil activates swirling portal ring around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#9966FF").
RADIUS is portal radius in pixels (default 30).
SPEED is swirl animation speed * 100 (default 200).
OPACITY is 0-100 (default 25). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object radius,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x99, g = 0x66, b = 0xFF, rd = 30, sp = 200, op = 25;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (radius)) rd = XFIXNUM (radius);
  if (FIXNUMP (speed)) sp = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_portal (dpyinfo->display_handle, on, r, g, b, rd, sp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-chevron-pattern",
       Fneomacs_set_chevron_pattern,
       Sneomacs_set_chevron_pattern, 0, 5, 0,
       doc: /* Configure chevron pattern overlay effect.
ENABLED non-nil activates V-shaped repeating chevron pattern.
COLOR is a hex color string like "#RRGGBB" (default "#4DE5B0").
SPACING is chevron spacing in pixels (default 40).
SPEED is scroll speed * 100 (default 50).
OPACITY is 0-100 (default 8). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object spacing,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0xE5, b = 0xB0, sp_val = 40, spd = 50, op = 8;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (spacing)) sp_val = XFIXNUM (spacing);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_chevron_pattern (dpyinfo->display_handle, on, r, g, b, sp_val, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-bubble",
       Fneomacs_set_cursor_bubble,
       Sneomacs_set_cursor_bubble, 0, 5, 0,
       doc: /* Configure cursor bubble effect.
ENABLED non-nil activates bubbles rising from cursor position.
COLOR is a hex color string like "#RRGGBB" (default "#66CCFF").
COUNT is number of bubbles (default 6).
RISE-SPEED is rise speed in pixels per second (default 40).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object count,
   Lisp_Object rise_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x66, g = 0xCC, b = 0xFF, ct = 6, rs = 40, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (count)) ct = XFIXNUM (count);
  if (FIXNUMP (rise_speed)) rs = XFIXNUM (rise_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_bubble (dpyinfo->display_handle, on, r, g, b, ct, rs, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-hex-grid",
       Fneomacs_set_hex_grid,
       Sneomacs_set_hex_grid, 0, 5, 0,
       doc: /* Configure hex grid overlay effect.
ENABLED non-nil activates honeycomb grid pattern.
COLOR is a hex color string like "#RRGGBB" (default "#4D99E5").
CELL-SIZE is hexagon cell size in pixels (default 40).
PULSE-SPEED is pulse animation speed * 100 (default 100).
OPACITY is 0-100 (default 10). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object cell_size,
   Lisp_Object pulse_speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x99, b = 0xE5, cs = 40, ps = 100, op = 10;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (cell_size)) cs = XFIXNUM (cell_size);
  if (FIXNUMP (pulse_speed)) ps = XFIXNUM (pulse_speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_hex_grid (dpyinfo->display_handle, on, r, g, b, cs, ps, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-sparkle-burst",
       Fneomacs_set_cursor_sparkle_burst,
       Sneomacs_set_cursor_sparkle_burst, 0, 5, 0,
       doc: /* Configure cursor sparkle burst effect.
ENABLED non-nil activates sparkle particles on keystroke.
COLOR is a hex color string like "#RRGGBB" (default "#FFD94D").
PARTICLE-COUNT is particles per burst (default 12).
BURST-RADIUS is burst radius in pixels (default 30).
OPACITY is 0-100 (default 40). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object particle_count,
   Lisp_Object burst_radius, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xFF, g = 0xD9, b = 0x4D, pc = 12, br = 30, op = 40;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (particle_count)) pc = XFIXNUM (particle_count);
  if (FIXNUMP (burst_radius)) br = XFIXNUM (burst_radius);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_sparkle_burst (dpyinfo->display_handle, on, r, g, b, pc, br, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-circuit-trace",
       Fneomacs_set_circuit_trace,
       Sneomacs_set_circuit_trace, 0, 5, 0,
       doc: /* Configure circuit board trace effect.
ENABLED non-nil activates pulsing circuit traces along edges.
COLOR is a hex color string like "#RRGGBB" (default "#33CC66").
TRACE-WIDTH is trace width in pixels (default 2).
SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 (default 20). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object trace_width,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x33, g = 0xCC, b = 0x66, tw = 2, spd = 100, op = 20;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (trace_width)) tw = XFIXNUM (trace_width);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_circuit_trace (dpyinfo->display_handle, on, r, g, b, tw, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-compass",
       Fneomacs_set_cursor_compass,
       Sneomacs_set_cursor_compass, 0, 5, 0,
       doc: /* Configure cursor compass rose effect.
ENABLED non-nil activates spinning compass rose around cursor.
COLOR is a hex color string like "#RRGGBB" (default "#E59933").
SIZE is compass size in pixels (default 20).
SPEED is rotation speed * 100 (default 100).
OPACITY is 0-100 (default 25). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object size,
   Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xE5, g = 0x99, b = 0x33, sz = 20, spd = 100, op = 25;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (size)) sz = XFIXNUM (size);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_compass (dpyinfo->display_handle, on, r, g, b, sz, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-warp-grid",
       Fneomacs_set_warp_grid,
       Sneomacs_set_warp_grid, 0, 6, 0,
       doc: /* Configure warp/distortion grid effect.
ENABLED non-nil activates animated distortion grid overlay.
COLOR is a hex color string like "#RRGGBB" (default "#4D80E5").
DENSITY is grid cells across width (default 20).
AMPLITUDE is distortion in pixels (default 5).
SPEED is animation speed * 100 (default 100).
OPACITY is 0-100 (default 15). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object density,
   Lisp_Object amplitude, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0x4D, g = 0x80, b = 0xE5, den = 20, amp = 5, spd = 100, op = 15;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (density)) den = XFIXNUM (density);
  if (FIXNUMP (amplitude)) amp = XFIXNUM (amplitude);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_warp_grid (dpyinfo->display_handle, on, r, g, b, den, amp, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-dna-helix",
       Fneomacs_set_cursor_dna_helix,
       Sneomacs_set_cursor_dna_helix, 0, 6, 0,
       doc: /* Configure cursor DNA helix trail effect.
ENABLED non-nil activates double-helix spirals around cursor.
COLOR1 is primary strand hex string (default "#4DE580").
COLOR2 is secondary strand hex string (default "#804DE5").
RADIUS is helix radius in pixels (default 12).
SPEED is rotation speed * 100 (default 150).
OPACITY is 0-100 (default 30). */)
  (Lisp_Object enabled, Lisp_Object color1, Lisp_Object color2,
   Lisp_Object radius, Lisp_Object speed, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r1 = 0x4D, g1 = 0xE5, b1 = 0x80;
  int r2 = 0x80, g2 = 0x4D, b2 = 0xE5;
  int rad = 12, spd = 150, op = 30;
  if (STRINGP (color1))
    {
      const char *s = SSDATA (color1);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r1 = (hex >> 16) & 0xFF;
          g1 = (hex >> 8) & 0xFF;
          b1 = hex & 0xFF;
        }
    }
  if (STRINGP (color2))
    {
      const char *s = SSDATA (color2);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r2 = (hex >> 16) & 0xFF;
          g2 = (hex >> 8) & 0xFF;
          b2 = hex & 0xFF;
        }
    }
  if (FIXNUMP (radius)) rad = XFIXNUM (radius);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_dna_helix (dpyinfo->display_handle, on, r1, g1, b1, r2, g2, b2, rad, spd, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-prism-edge",
       Fneomacs_set_prism_edge,
       Sneomacs_set_prism_edge, 0, 5, 0,
       doc: /* Configure prism/rainbow edge effect.
ENABLED non-nil activates animated rainbow spectrum at frame edges.
WIDTH is spectrum band width in pixels (default 6).
SPEED is animation speed * 100 (default 100).
SATURATION is color saturation 0-100 (default 80).
OPACITY is 0-100 (default 25). */)
  (Lisp_Object enabled, Lisp_Object width, Lisp_Object speed,
   Lisp_Object saturation, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int w = 6, spd = 100, sat = 80, op = 25;
  if (FIXNUMP (width)) w = XFIXNUM (width);
  if (FIXNUMP (speed)) spd = XFIXNUM (speed);
  if (FIXNUMP (saturation)) sat = XFIXNUM (saturation);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_prism_edge (dpyinfo->display_handle, on, w, spd, sat, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-pendulum",
       Fneomacs_set_cursor_pendulum,
       Sneomacs_set_cursor_pendulum, 0, 5, 0,
       doc: /* Configure cursor pendulum swing effect.
ENABLED non-nil activates pendulum arc on cursor movement.
COLOR is a hex color string like "#RRGGBB" (default "#E5B34D").
ARC-LENGTH is arc length in pixels (default 40).
DAMPING is damping factor 0-100 (default 50).
OPACITY is 0-100 (default 30). */)
  (Lisp_Object enabled, Lisp_Object color, Lisp_Object arc_length,
   Lisp_Object damping, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;
  int on = !NILP (enabled);
  int r = 0xE5, g = 0xB3, b = 0x4D, arc = 40, damp = 50, op = 30;
  if (STRINGP (color))
    {
      const char *s = SSDATA (color);
      if (s[0] == '#' && strlen (s) == 7)
        {
          unsigned int hex;
          sscanf (s + 1, "%06x", &hex);
          r = (hex >> 16) & 0xFF;
          g = (hex >> 8) & 0xFF;
          b = hex & 0xFF;
        }
    }
  if (FIXNUMP (arc_length)) arc = XFIXNUM (arc_length);
  if (FIXNUMP (damping)) damp = XFIXNUM (damping);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);

  neomacs_display_set_cursor_pendulum (dpyinfo->display_handle, on, r, g, b, arc, damp, op);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-padding-gradient",
       Fneomacs_set_padding_gradient,
       Sneomacs_set_padding_gradient, 0, 6, 0,
       doc: /* Configure window padding gradient for depth effect.
ENABLED non-nil renders a subtle gradient at the inner edges of each
window, blending from a configurable edge color inward, creating a
sense of depth.
R, G, B are the edge color 0-255 (default 0 0 0 for dark shading).
OPACITY is 0-100 for peak edge opacity (default 15).
WIDTH is the gradient width in pixels (default 8).  */)
  (Lisp_Object enabled, Lisp_Object r, Lisp_Object g, Lisp_Object b,
   Lisp_Object opacity, Lisp_Object width)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int cr = 0, cg = 0, cb = 0;
  int op = 15;
  int w = 8;
  if (FIXNUMP (r)) cr = XFIXNUM (r);
  if (FIXNUMP (g)) cg = XFIXNUM (g);
  if (FIXNUMP (b)) cb = XFIXNUM (b);
  if (FIXNUMP (opacity)) op = XFIXNUM (opacity);
  if (FIXNUMP (width)) w = XFIXNUM (width);

  neomacs_display_set_padding_gradient (
    dpyinfo->display_handle, on, cr, cg, cb, op, w);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-cursor-size-transition",
       Fneomacs_set_cursor_size_transition,
       Sneomacs_set_cursor_size_transition, 0, 2, 0,
       doc: /* Configure smooth cursor size transition.
ENABLED non-nil smoothly animates the cursor width and height when
text-scale-adjust changes the font size, rather than snapping instantly.
DURATION-MS is the transition duration in milliseconds (default 150).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 150;
  if (FIXNUMP (duration_ms))
    dur = XFIXNUM (duration_ms);

  neomacs_display_set_cursor_size_transition (
    dpyinfo->display_handle, on, dur);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-window-switch-fade",
       Fneomacs_set_window_switch_fade,
       Sneomacs_set_window_switch_fade, 0, 3, 0,
       doc: /* Configure window switch highlight fade effect.
ENABLED non-nil flashes a brief white overlay on the newly selected window.
DURATION-MS is the fade duration in milliseconds (default 200).
INTENSITY is 0-100 for overlay brightness (default 15).  */)
  (Lisp_Object enabled, Lisp_Object duration_ms, Lisp_Object intensity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int dur = 200;
  int inten = 15;
  if (FIXNUMP (duration_ms))
    dur = XFIXNUM (duration_ms);
  if (FIXNUMP (intensity))
    inten = XFIXNUM (intensity);

  neomacs_display_set_window_switch_fade (
    dpyinfo->display_handle, on, dur, inten);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-background-pattern",
       Fneomacs_set_background_pattern,
       Sneomacs_set_background_pattern, 0, 4, 0,
       doc: /* Configure background pattern rendering.
STYLE is the pattern type: 0=none, 1=dots, 2=grid, 3=crosshatch.
SPACING is the pixel distance between pattern elements (default 20).
COLOR is a color string for the pattern (default \"gray50\").
OPACITY is 0-100 for pattern opacity (default 5).  */)
  (Lisp_Object style, Lisp_Object spacing, Lisp_Object color, Lisp_Object opacity)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int s = 0;
  if (FIXNUMP (style))
    s = XFIXNUM (style);
  int sp = 20;
  if (FIXNUMP (spacing))
    sp = XFIXNUM (spacing);
  int op = 5;
  if (FIXNUMP (opacity))
    op = XFIXNUM (opacity);

  int cr = 128, cg = 128, cb = 128;
  if (STRINGP (color))
    {
      Emacs_Color ec;
      if (neomacs_defined_color (NULL, SSDATA (color), &ec, false, false))
        {
          cr = ec.red >> 8;
          cg = ec.green >> 8;
          cb = ec.blue >> 8;
        }
    }

  neomacs_display_set_background_pattern (
    dpyinfo->display_handle, s, sp, cr, cg, cb, op);
  return s > 0 ? Qt : Qnil;
}

DEFUN ("neomacs-set-indent-guides",
       Fneomacs_set_indent_guides,
       Sneomacs_set_indent_guides, 0, 2, 0,
       doc: /* Configure indent guide rendering.
ENABLED non-nil enables indent guides.
Optional COLOR is a color string for the guides (default \"gray30\").  */)
  (Lisp_Object enabled, Lisp_Object color)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);
  int r = 77, g = 77, b = 77;  /* gray30 default */
  int opacity = 30;

  if (!NILP (color) && STRINGP (color))
    {
      Emacs_Color c;
      if (neomacs_defined_color (NULL, SSDATA (color), &c, false, false))
        {
          r = c.red >> 8;
          g = c.green >> 8;
          b = c.blue >> 8;
          opacity = 40;
        }
    }

  neomacs_display_set_indent_guides (
    dpyinfo->display_handle, on, r, g, b, opacity);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-indent-guide-rainbow",
       Fneomacs_set_indent_guide_rainbow,
       Sneomacs_set_indent_guide_rainbow, 0, 2, 0,
       doc: /* Configure rainbow indent guide colors.
ENABLED non-nil enables rainbow coloring (cycles colors by depth level).
Optional COLORS is a list of color strings (up to 6).
Default palette: red, orange, yellow, green, cyan, purple.  */)
  (Lisp_Object enabled, Lisp_Object colors)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int on = !NILP (enabled);

  /* Default rainbow palette */
  int cr[6][4] = {
    {228, 66, 66, 50},   /* red */
    {228, 160, 66, 50},  /* orange */
    {228, 228, 66, 50},  /* yellow */
    {66, 228, 66, 50},   /* green */
    {66, 228, 228, 50},  /* cyan */
    {160, 66, 228, 50},  /* purple */
  };
  int num_colors = 6;

  if (!NILP (colors) && CONSP (colors))
    {
      num_colors = 0;
      Lisp_Object tail;
      for (tail = colors; CONSP (tail) && num_colors < 6; tail = XCDR (tail))
        {
          Lisp_Object c = XCAR (tail);
          if (STRINGP (c))
            {
              Emacs_Color ec;
              if (neomacs_defined_color (NULL, SSDATA (c), &ec, false, false))
                {
                  cr[num_colors][0] = ec.red >> 8;
                  cr[num_colors][1] = ec.green >> 8;
                  cr[num_colors][2] = ec.blue >> 8;
                  cr[num_colors][3] = 50;
                  num_colors++;
                }
            }
        }
      if (num_colors == 0)
        num_colors = 6; /* fall back to defaults */
    }

  neomacs_display_set_indent_guide_rainbow (
    dpyinfo->display_handle, on, num_colors,
    cr[0][0], cr[0][1], cr[0][2], cr[0][3],
    cr[1][0], cr[1][1], cr[1][2], cr[1][3],
    cr[2][0], cr[2][1], cr[2][2], cr[2][3],
    cr[3][0], cr[3][1], cr[3][2], cr[3][3],
    cr[4][0], cr[4][1], cr[4][2], cr[4][3],
    cr[5][0], cr[5][1], cr[5][2], cr[5][3]);
  return on ? Qt : Qnil;
}

DEFUN ("neomacs-set-scroll-bar-config",
       Fneomacs_set_scroll_bar_config,
       Sneomacs_set_scroll_bar_config, 0, 4, 0,
       doc: /* Configure GPU scroll bar appearance.
Optional WIDTH is the scroll bar width in pixels (default 12).
Optional THUMB-RADIUS is the thumb corner radius ratio 0-100 (default 40).
Optional TRACK-OPACITY is the track opacity 0-100 (default 60).
Optional HOVER-BRIGHTNESS is the hover brightness 0-200 (default 140).  */)
  (Lisp_Object width, Lisp_Object thumb_radius,
   Lisp_Object track_opacity, Lisp_Object hover_brightness)
{
  struct neomacs_display_info *dpyinfo = neomacs_display_list;
  if (!dpyinfo || !dpyinfo->display_handle)
    return Qnil;

  int w = 0;
  int tr = 40;
  int to = 60;
  int hb = 140;

  if (!NILP (width) && NUMBERP (width))
    w = (int) XFIXNUM (width);
  if (!NILP (thumb_radius) && NUMBERP (thumb_radius))
    tr = (int) XFIXNUM (thumb_radius);
  if (!NILP (track_opacity) && NUMBERP (track_opacity))
    to = (int) XFIXNUM (track_opacity);
  if (!NILP (hover_brightness) && NUMBERP (hover_brightness))
    hb = (int) XFIXNUM (hover_brightness);

  /* Update the C-side default scroll bar width for new frames/windows */
  if (w > 0)
    {
      struct frame *f = SELECTED_FRAME ();
      if (f)
        {
          int unit = FRAME_COLUMN_WIDTH (f);
          FRAME_CONFIG_SCROLL_BAR_WIDTH (f) = w;
          FRAME_CONFIG_SCROLL_BAR_COLS (f) = (w + unit - 1) / unit;
        }
    }

  neomacs_display_set_scroll_bar_config (
    dpyinfo->display_handle, w, tr, to, hb);
  return Qt;
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
  static char name_buf[64];
  if (xkb_keysym_get_name (keysym, name_buf, sizeof name_buf) > 0)
    return name_buf;
  return NULL;
}

/* Set mouse pixel position on frame F.  */
void
frame_set_mouse_pixel_position (struct frame *f, int pix_x, int pix_y)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (!dpyinfo)
    return;

  /* Update tracked position so mouse_position_hook returns correct values.  */
  dpyinfo->last_mouse_motion_x = pix_x;
  dpyinfo->last_mouse_motion_y = pix_y;
  dpyinfo->last_mouse_motion_frame = f;

  if (dpyinfo->display_handle)
    neomacs_display_warp_mouse (dpyinfo->display_handle, pix_x, pix_y);
}


/* ============================================================================
 * Menu Bar / Tool Bar Support
 * ============================================================================ */

/* Neomacs uses Emacs-internal text-rendered menu and tool bars (not external
   toolkit widgets).  HAVE_EXT_MENU_BAR and HAVE_EXT_TOOL_BAR are NOT defined
   for neomacs, so xdisp.c handles menu/tool bar rendering as text in the
   frame's display matrix.  We still provide set_frame_menubar/free_frame_menubar
   since frame.h declares them unconditionally.  */

void
set_frame_menubar (struct frame *f, bool deep_p)
{
  /* Internal menu bar is rendered by xdisp.c via update_mode_line.
     Nothing needed here.  */
}

void
free_frame_menubar (struct frame *f)
{
  /* No external menu bar resources to free.  */
}

int
popup_activated (void)
{
  return neomacs_popup_activated_flag;
}

DEFUN ("menu-or-popup-active-p", Fmenu_or_popup_active_p,
       Smenu_or_popup_active_p, 0, 0, 0,
       doc: /* Return t if a menu or popup dialog is active.  */)
  (void)
{
  return popup_activated () ? Qt : Qnil;
}


/* ============================================================================
 * Threaded Mode Support
 * ============================================================================ */

/* Threaded mode state */
static int threaded_mode_active = 0;
static int wakeup_fd = -1;


/* Resolve a safe target frame for an input event from the render thread.  */
static struct frame *
neomacs_event_target_frame (uint32_t window_id)
{
  Lisp_Object tail, frame;

  /* First try an explicit window mapping when one is provided.  */
  if (window_id != 0)
    {
      FOR_EACH_FRAME (tail, frame)
        {
          struct frame *tf = XFRAME (frame);
          if (FRAME_LIVE_P (tf)
              && FRAME_NEOMACS_P (tf)
              && FRAME_NEOMACS_OUTPUT (tf)->window_id == window_id)
            return tf;
        }
    }

  /* Fallback to selected frame if it is live and a Neomacs frame.  */
  {
    struct frame *selected = SELECTED_FRAME ();
    if (selected && FRAME_LIVE_P (selected) && FRAME_NEOMACS_P (selected))
      return selected;
  }

  /* Last fallback: any live Neomacs frame.  */
  FOR_EACH_FRAME (tail, frame)
    {
      struct frame *tf = XFRAME (frame);
      if (FRAME_LIVE_P (tf) && FRAME_NEOMACS_P (tf))
        return tf;
    }

  return NULL;
}

/* Handler called when wakeup_fd is readable */
static void
neomacs_display_wakeup_handler (int fd, void *data)
{
  struct NeomacsInputEvent events[64];
  int count;

  /* Drain input events from render thread */
  count = neomacs_display_drain_input (events, 64);
  nlog_debug ("wakeup_handler: drained %d events from fd=%d", count, fd);

  /* Process events */
  for (int i = 0; i < count; i++)
    {
      struct NeomacsInputEvent *ev = &events[i];
      union buffered_input_event inev;
      struct frame *f = neomacs_event_target_frame (ev->windowId);

      if (!f)
        continue;

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
          nlog_debug ("KEY_PRESS: keysym=0x%x mods=0x%x", ev->keysym, ev->modifiers);
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

            /* Check if click is on a scroll bar */
            {
              Lisp_Object click_window
                = window_from_coordinates (f, ev->x, ev->y, 0, true, true, true);
              bool scroll_bar_handled = false;

              if (WINDOWP (click_window))
                {
                  struct window *cw = XWINDOW (click_window);

                  /* Check vertical scroll bar */
                  if (!NILP (cw->vertical_scroll_bar)
                      && WINDOW_HAS_VERTICAL_SCROLL_BAR (cw))
                    {
                      struct scroll_bar *bar
                        = XSCROLL_BAR (cw->vertical_scroll_bar);
                      int sb_left = bar->left;
                      int sb_right = sb_left + bar->width;

                      /* Is the click in the scroll bar area? */
                      if (ev->x >= sb_left && ev->x < sb_right
                          && ev->y >= bar->top
                          && ev->y < bar->top + bar->height)
                        {
                          int y_in_bar = ev->y - bar->top;
                          enum scroll_bar_part part;

                          if (y_in_bar < bar->start)
                            part = scroll_bar_above_handle;
                          else if (y_in_bar < bar->end)
                            part = scroll_bar_handle;
                          else
                            part = scroll_bar_below_handle;

                          if (ev->kind == NEOMACS_EVENT_MOUSE_PRESS)
                            bar->dragging = y_in_bar - bar->start;
                          else
                            {
                              part = scroll_bar_end_scroll;
                              bar->dragging = -1;
                            }

                          inev.ie.kind = SCROLL_BAR_CLICK_EVENT;
                          inev.ie.code = ev->button - 1;
                          inev.ie.part = part;
                          inev.ie.modifiers = (ev->kind == NEOMACS_EVENT_MOUSE_PRESS)
                            ? down_modifier : up_modifier;
                          if (ev->modifiers & NEOMACS_SHIFT_MASK)
                            inev.ie.modifiers |= shift_modifier;
                          if (ev->modifiers & NEOMACS_CTRL_MASK)
                            inev.ie.modifiers |= ctrl_modifier;
                          if (ev->modifiers & NEOMACS_META_MASK)
                            inev.ie.modifiers |= meta_modifier;
                          XSETINT (inev.ie.x, y_in_bar);
                          XSETINT (inev.ie.y, bar->height);
                          inev.ie.frame_or_window = cw->vertical_scroll_bar;
                          neomacs_evq_enqueue (&inev);
                          scroll_bar_handled = true;
                        }
                    }
                }

              if (!scroll_bar_handled)
                {
                  inev.ie.kind = MOUSE_CLICK_EVENT;
                  inev.ie.code = ev->button - 1;
                  inev.ie.modifiers = (ev->kind == NEOMACS_EVENT_MOUSE_PRESS)
                    ? down_modifier : up_modifier;
                  if (ev->modifiers & NEOMACS_SHIFT_MASK)
                    inev.ie.modifiers |= shift_modifier;
                  if (ev->modifiers & NEOMACS_CTRL_MASK)
                    inev.ie.modifiers |= ctrl_modifier;
                  if (ev->modifiers & NEOMACS_META_MASK)
                    inev.ie.modifiers |= meta_modifier;
                  XSETINT (inev.ie.x, ev->x);
                  XSETINT (inev.ie.y, ev->y);
                  if (!NILP (tab_bar_arg))
                    inev.ie.arg = tab_bar_arg;
                  XSETFRAME (inev.ie.frame_or_window, f);
                  neomacs_evq_enqueue (&inev);
                }
            }
          }
          break;

        case NEOMACS_EVENT_SCROLL:
          {
            /* Check if scrolling over a webkit view */
            struct neomacs_display_info *dpyinfo
              = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (dpyinfo && dpyinfo->display_handle)
              {
                uint32_t webkit_id = 0;
                int rel_x = 0, rel_y = 0;
                if (neomacs_display_webkit_at_position (
                      dpyinfo->display_handle,
                      ev->x, ev->y,
                      &webkit_id, &rel_x, &rel_y))
                  {
                    int sdx, sdy;
                    if (ev->pixelPrecise)
                      {
                        /* Pixel deltas from touchpad — already
                           in logical pixels, pass directly. */
                        sdx = (int) ev->scrollDeltaX;
                        sdy = (int) ev->scrollDeltaY;
                      }
                    else
                      {
                        /* Line deltas from mouse wheel —
                           convert to ~pixels for WPE. */
                        sdx = (int)(ev->scrollDeltaX * 53);
                        sdy = (int)(ev->scrollDeltaY * 53);
                      }
                    neomacs_display_webkit_send_scroll (
                      dpyinfo->display_handle,
                      webkit_id, rel_x, rel_y, sdx, sdy);
                    break;
                  }
              }

            float dx = ev->scrollDeltaX;
            float dy = ev->scrollDeltaY;
            float abs_dx = dx < 0 ? -dx : dx;
            float abs_dy = dy < 0 ? -dy : dy;

            /* Convert deltas to pixel amounts for Emacs.
               pixel_precise: touchpad deltas already in pixels.
               Otherwise: line deltas × line height. */
            double px_dx, px_dy;
            if (ev->pixelPrecise)
              {
                px_dx = (double) -dx;
                px_dy = (double) -dy;
              }
            else
              {
                int lh = FRAME_LINE_HEIGHT (f);
                px_dx = (double) -dx * lh;
                px_dy = (double) -dy * lh;
              }

            /* Determine primary axis and generate event */
            if (abs_dy >= abs_dx)
              {
                if (dy != 0.0f)
                  {
                    inev.ie.kind = WHEEL_EVENT;
                    inev.ie.modifiers
                      |= (dy > 0) ? up_modifier : down_modifier;
                    inev.ie.arg = list3 (Qnil,
                                         make_float (px_dx),
                                         make_float (px_dy));
                    if (ev->modifiers & NEOMACS_SHIFT_MASK)
                      inev.ie.modifiers |= shift_modifier;
                    if (ev->modifiers & NEOMACS_CTRL_MASK)
                      inev.ie.modifiers |= ctrl_modifier;
                    if (ev->modifiers & NEOMACS_META_MASK)
                      inev.ie.modifiers |= meta_modifier;
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
                    inev.ie.modifiers
                      |= (dx > 0) ? up_modifier : down_modifier;
                    inev.ie.arg = list3 (Qnil,
                                         make_float (px_dx),
                                         make_float (px_dy));
                    if (ev->modifiers & NEOMACS_SHIFT_MASK)
                      inev.ie.modifiers |= shift_modifier;
                    if (ev->modifiers & NEOMACS_CTRL_MASK)
                      inev.ie.modifiers |= ctrl_modifier;
                    if (ev->modifiers & NEOMACS_META_MASK)
                      inev.ie.modifiers |= meta_modifier;
                    XSETINT (inev.ie.x, ev->x);
                    XSETINT (inev.ie.y, ev->y);
                    XSETFRAME (inev.ie.frame_or_window, f);
                    neomacs_evq_enqueue (&inev);
                  }
              }
          }
          break;

        case NEOMACS_EVENT_MOUSE_MOVE:
          {
            struct neomacs_display_info *dpyinfo
              = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (dpyinfo)
              {
                dpyinfo->last_mouse_motion_frame = f;
                dpyinfo->last_mouse_motion_x = ev->x;
                dpyinfo->last_mouse_motion_y = ev->y;
              }

            /* Check if we're dragging a scroll bar thumb */
            {
              bool scroll_drag_handled = false;
              Lisp_Object bar_obj;

              for (bar_obj = FRAME_SCROLL_BARS (f);
                   !scroll_drag_handled && VECTORP (bar_obj);
                   bar_obj = XSCROLL_BAR (bar_obj)->next)
                {
                  struct scroll_bar *bar = XSCROLL_BAR (bar_obj);
                  if (bar->dragging >= 0 && !bar->horizontal)
                    {
                      int y_in_bar = ev->y - bar->top;
                      int new_start = y_in_bar - bar->dragging;
                      int thumb_size = bar->end - bar->start;
                      if (new_start < 0)
                        new_start = 0;
                      if (new_start + thumb_size > bar->height)
                        new_start = bar->height - thumb_size;

                      EVENT_INIT (inev.ie);
                      inev.ie.kind = SCROLL_BAR_CLICK_EVENT;
                      inev.ie.code = 0;
                      inev.ie.part = scroll_bar_handle;
                      inev.ie.modifiers = 0;
                      XSETINT (inev.ie.x, new_start);
                      XSETINT (inev.ie.y, bar->height);
                      inev.ie.frame_or_window = bar_obj;
                      neomacs_evq_enqueue (&inev);
                      scroll_drag_handled = true;
                    }
                }

              if (scroll_drag_handled)
                break;
            }

            /* Check if mouse has moved off the glyph it was on. */
            if (dpyinfo)
              {
                NativeRectangle *r = &dpyinfo->last_mouse_glyph;
                if (f != dpyinfo->last_mouse_glyph_frame
                    || ev->x < r->x
                    || ev->x >= r->x + (int) r->width
                    || ev->y < r->y
                    || ev->y >= r->y + (int) r->height)
                  {
                    Lisp_Object previous_help_echo_string
                      = help_echo_string;
                    help_echo_string = Qnil;

                    f->mouse_moved = true;
                    dpyinfo->last_mouse_scroll_bar = NULL;
                    note_mouse_highlight (f, ev->x, ev->y);
                    remember_mouse_glyph (
                        f, ev->x, ev->y, r);
                    dpyinfo->last_mouse_glyph_frame = f;

                    /* Generate HELP_EVENT if help-echo changed. */
                    if (!NILP (help_echo_string)
                        || !NILP (previous_help_echo_string))
                      {
                        Lisp_Object frame;
                        XSETFRAME (frame, f);
                        if (!NILP (help_echo_string))
                          gen_help_event (help_echo_string, frame,
                                         help_echo_window,
                                         help_echo_object,
                                         help_echo_pos);
                        else
                          gen_help_event (Qnil, frame, Qnil, Qnil, 0);
                      }

                    /* Re-send frame so mouse-face highlight is
                       immediately visible without waiting for
                       a full redisplay cycle.  */
                    neomacs_resend_frame (f);
                  }
              }
          }
          break;

        case NEOMACS_EVENT_RESIZE:
          {
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            int new_width = ev->width;
            int new_height = ev->height;

            nlog_info ("RESIZE: %dx%d -> %dx%d",
                       FRAME_PIXEL_WIDTH (f), FRAME_PIXEL_HEIGHT (f),
                       new_width, new_height);

            /* Update the Rust display handle */
            if (dpyinfo && dpyinfo->display_handle)
              neomacs_display_resize (dpyinfo->display_handle, new_width, new_height);

            if (FRAME_PIXEL_WIDTH (f) != new_width
                || FRAME_PIXEL_HEIGHT (f) != new_height)
              change_frame_size (f, new_width, new_height, false, true, false);

            SET_FRAME_GARBAGED (f);
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
                neomacs_frame_rehighlight (dpyinfo);
              }
            inev.ie.kind = FOCUS_IN_EVENT;
            XSETFRAME (inev.ie.frame_or_window, f);
            neomacs_evq_enqueue (&inev);
          }
          break;

        case NEOMACS_EVENT_FOCUS_OUT:
          {
            struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
            if (dpyinfo)
              {
                if (dpyinfo->x_focus_frame == f)
                  dpyinfo->x_focus_frame = NULL;
                if (dpyinfo->focus_frame == f)
                  dpyinfo->focus_frame = NULL;
                neomacs_frame_rehighlight (dpyinfo);
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

        case NEOMACS_EVENT_FILE_DROP:
          {
            /* Retrieve dropped file paths from Rust */
            char *paths[32];
            int nfiles = neomacs_display_get_dropped_files (paths, 32);

            if (nfiles > 0)
              {
                /* Build a Lisp list of file:// URLs */
                Lisp_Object files = Qnil;
                for (int j = nfiles - 1; j >= 0; j--)
                  {
                    /* Construct file:// URL */
                    int len = strlen (paths[j]);
                    char *url = alloca (len + 8);
                    snprintf (url, len + 8, "file://%s", paths[j]);
                    files = Fcons (build_string (url), files);
                    neomacs_display_free_dropped_path (paths[j]);
                  }

                EVENT_INIT (inev.ie);
                inev.ie.kind = DRAG_N_DROP_EVENT;
                inev.ie.modifiers = 0;
                inev.ie.arg = files;
                XSETINT (inev.ie.x, ev->x);
                XSETINT (inev.ie.y, ev->y);
                XSETFRAME (inev.ie.frame_or_window, f);
                neomacs_evq_enqueue (&inev);
              }
          }
          break;

        case NEOMACS_EVENT_TERMINAL_TITLE_CHANGED:
          {
            uint32_t term_id = ev->keysym;
            char *title
              = neomacs_display_get_terminal_title (term_id);
            if (title)
              {
                Lisp_Object handler
                  = intern ("neo-term--handle-title-changed");
                if (!NILP (Ffboundp (handler)))
                  safe_calln (Fsymbol_function (handler),
                              make_fixnum (term_id),
                              build_string (title));
                neomacs_display_free_dropped_path (title);
              }
          }
          break;

        default:
          break;
        }
    }

  /* Flush events to kbd_buffer.  When wakeup_handler fires as an fd
     callback (during wait_reading_process_output), events need to be
     in kbd_buffer for detect_input_pending to find them.  When called
     from neomacs_read_socket (via gobble_input), the read_socket path
     also calls neomacs_evq_flush, making this a harmless no-op.  */
  neomacs_evq_flush (NULL);
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
  defsubr (&Sxw_display_color_p);
  defsubr (&Sx_display_grayscale_p);
  defsubr (&Smenu_or_popup_active_p);

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

  /* Scroll indicators */
  defsubr (&Sneomacs_set_scroll_indicators);

  /* Title bar */
  defsubr (&Sneomacs_set_titlebar_height);

  /* FPS counter */
  defsubr (&Sneomacs_show_fps);

  /* Corner radius */
  defsubr (&Sneomacs_set_corner_radius);
  defsubr (&Sneomacs_set_extra_spacing);
  defsubr (&Sneomacs_set_background_gradient);
  defsubr (&Sneomacs_set_scroll_bar_config);
  defsubr (&Sneomacs_set_indent_guides);
  defsubr (&Sneomacs_set_indent_guide_rainbow);
  defsubr (&Sneomacs_set_line_highlight);
  defsubr (&Sneomacs_set_show_whitespace);
  defsubr (&Sneomacs_set_inactive_dim);
  defsubr (&Sneomacs_set_mode_line_separator);
  defsubr (&Sneomacs_set_cursor_glow);
  defsubr (&Sneomacs_set_cursor_pulse);
  defsubr (&Sneomacs_set_focus_mode);
  defsubr (&Sneomacs_set_minimap);
  defsubr (&Sneomacs_set_typing_ripple);
  defsubr (&Sneomacs_set_search_pulse);
  defsubr (&Sneomacs_set_background_pattern);
  defsubr (&Sneomacs_set_zen_mode);
  defsubr (&Sneomacs_set_vignette);
  defsubr (&Sneomacs_set_line_animation);
  defsubr (&Sneomacs_set_header_shadow);
  defsubr (&Sneomacs_set_cursor_color_cycle);
  defsubr (&Sneomacs_set_window_switch_fade);
  defsubr (&Sneomacs_set_breadcrumb);
  defsubr (&Sneomacs_set_title_fade);
  defsubr (&Sneomacs_set_typing_speed);
  defsubr (&Sneomacs_set_border_transition);
  defsubr (&Sneomacs_set_accent_strip);
  defsubr (&Sneomacs_set_frosted_glass);
  defsubr (&Sneomacs_set_cursor_size_transition);
  defsubr (&Sneomacs_set_padding_gradient);
  defsubr (&Sneomacs_set_noise_grain);
  defsubr (&Sneomacs_set_idle_dim);
  defsubr (&Sneomacs_set_cursor_shadow);
  defsubr (&Sneomacs_set_focus_ring);
  defsubr (&Sneomacs_set_window_mode_tint);
  defsubr (&Sneomacs_set_window_watermark);
  defsubr (&Sneomacs_set_cursor_trail_fade);
  defsubr (&Sneomacs_set_scroll_line_spacing);
  defsubr (&Sneomacs_set_text_fade_in);
  defsubr (&Sneomacs_set_mode_line_transition);
  defsubr (&Sneomacs_set_cursor_wake);
  defsubr (&Sneomacs_set_scroll_momentum);
  defsubr (&Sneomacs_set_wrap_indicator);
  defsubr (&Sneomacs_set_cursor_error_pulse);
  defsubr (&Sneomacs_set_window_content_shadow);
  defsubr (&Sneomacs_set_resize_padding);
  defsubr (&Sneomacs_set_minibuffer_highlight);
  defsubr (&Sneomacs_set_scroll_velocity_fade);
  defsubr (&Sneomacs_set_click_halo);
  defsubr (&Sneomacs_set_edge_snap);
  defsubr (&Sneomacs_set_cursor_crosshair);
  defsubr (&Sneomacs_set_modified_indicator);
  defsubr (&Sneomacs_set_theme_transition);
  defsubr (&Sneomacs_set_typing_heatmap);
  defsubr (&Sneomacs_set_window_border_radius);
  defsubr (&Sneomacs_set_cursor_particles);
  defsubr (&Sneomacs_set_stained_glass);
  defsubr (&Sneomacs_set_scanlines);
  defsubr (&Sneomacs_set_cursor_comet);
  defsubr (&Sneomacs_set_cursor_spotlight);
  defsubr (&Sneomacs_set_corner_fold);
  defsubr (&Sneomacs_set_frosted_border);
  defsubr (&Sneomacs_set_line_number_pulse);
  defsubr (&Sneomacs_set_breathing_border);
  defsubr (&Sneomacs_set_focus_gradient_border);
  defsubr (&Sneomacs_set_cursor_magnetism);
  defsubr (&Sneomacs_set_depth_shadow);
  defsubr (&Sneomacs_set_matrix_rain);
  defsubr (&Sneomacs_set_cursor_elastic_snap);
  defsubr (&Sneomacs_set_frost_border_effect);
  defsubr (&Sneomacs_set_cursor_ghost);
  defsubr (&Sneomacs_set_edge_glow);
  defsubr (&Sneomacs_set_rain_effect);
  defsubr (&Sneomacs_set_cursor_ripple_wave);
  defsubr (&Sneomacs_set_aurora);
  defsubr (&Sneomacs_set_heat_distortion);
  defsubr (&Sneomacs_set_cursor_lighthouse);
  defsubr (&Sneomacs_set_neon_border);
  defsubr (&Sneomacs_set_cursor_sonar_ping);
  defsubr (&Sneomacs_set_lightning_bolt);
  defsubr (&Sneomacs_set_cursor_orbit_particles);
  defsubr (&Sneomacs_set_plasma_border);
  defsubr (&Sneomacs_set_cursor_heartbeat);
  defsubr (&Sneomacs_set_topo_contour);
  defsubr (&Sneomacs_set_cursor_metronome);
  defsubr (&Sneomacs_set_constellation);
  defsubr (&Sneomacs_set_cursor_radar);
  defsubr (&Sneomacs_set_kaleidoscope);
  defsubr (&Sneomacs_set_cursor_ripple_ring);
  defsubr (&Sneomacs_set_noise_field);
  defsubr (&Sneomacs_set_cursor_scope);
  defsubr (&Sneomacs_set_spiral_vortex);
  defsubr (&Sneomacs_set_cursor_shockwave);
  defsubr (&Sneomacs_set_diamond_lattice);
  defsubr (&Sneomacs_set_cursor_gravity_well);
  defsubr (&Sneomacs_set_tessellation);
  defsubr (&Sneomacs_set_cursor_water_drop);
  defsubr (&Sneomacs_set_guilloche);
  defsubr (&Sneomacs_set_cursor_pixel_dust);
  defsubr (&Sneomacs_set_celtic_knot);
  defsubr (&Sneomacs_set_cursor_candle_flame);
  defsubr (&Sneomacs_set_argyle_pattern);
  defsubr (&Sneomacs_set_cursor_moth_flame);
  defsubr (&Sneomacs_set_basket_weave);
  defsubr (&Sneomacs_set_cursor_sparkler);
  defsubr (&Sneomacs_set_fish_scale);
  defsubr (&Sneomacs_set_cursor_plasma_ball);
  defsubr (&Sneomacs_set_trefoil_knot);
  defsubr (&Sneomacs_set_cursor_quill_pen);
  defsubr (&Sneomacs_set_herringbone_pattern);
  defsubr (&Sneomacs_set_cursor_aurora_borealis);
  defsubr (&Sneomacs_set_target_reticle);
  defsubr (&Sneomacs_set_cursor_feather);
  defsubr (&Sneomacs_set_plaid_pattern);
  defsubr (&Sneomacs_set_cursor_stardust);
  defsubr (&Sneomacs_set_brick_wall);
  defsubr (&Sneomacs_set_cursor_compass_needle);
  defsubr (&Sneomacs_set_sine_wave);
  defsubr (&Sneomacs_set_cursor_galaxy);
  defsubr (&Sneomacs_set_rotating_gear);
  defsubr (&Sneomacs_set_cursor_prism);
  defsubr (&Sneomacs_set_crosshatch_pattern);
  defsubr (&Sneomacs_set_cursor_moth);
  defsubr (&Sneomacs_set_concentric_rings);
  defsubr (&Sneomacs_set_cursor_flame);
  defsubr (&Sneomacs_set_zigzag_pattern);
  defsubr (&Sneomacs_set_cursor_crystal);
  defsubr (&Sneomacs_set_moire_pattern);
  defsubr (&Sneomacs_set_cursor_lightning);
  defsubr (&Sneomacs_set_dot_matrix);
  defsubr (&Sneomacs_set_cursor_snowflake);
  defsubr (&Sneomacs_set_sunburst_pattern);
  defsubr (&Sneomacs_set_cursor_firework);
  defsubr (&Sneomacs_set_honeycomb_dissolve);
  defsubr (&Sneomacs_set_cursor_tornado);
  defsubr (&Sneomacs_set_wave_interference);
  defsubr (&Sneomacs_set_cursor_portal);
  defsubr (&Sneomacs_set_chevron_pattern);
  defsubr (&Sneomacs_set_cursor_bubble);
  defsubr (&Sneomacs_set_hex_grid);
  defsubr (&Sneomacs_set_cursor_sparkle_burst);
  defsubr (&Sneomacs_set_circuit_trace);
  defsubr (&Sneomacs_set_cursor_compass);
  defsubr (&Sneomacs_set_warp_grid);
  defsubr (&Sneomacs_set_cursor_dna_helix);
  defsubr (&Sneomacs_set_prism_edge);
  defsubr (&Sneomacs_set_cursor_pendulum);
  defsubr (&Sneomacs_set_mode_line_gradient);
  defsubr (&Sneomacs_set_region_glow);
  defsubr (&Sneomacs_set_window_glow);
  defsubr (&Sneomacs_set_scroll_progress);
  defsubr (&Sneomacs_set_inactive_tint);

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
  Vx_toolkit_scroll_bars = Qnil;

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

  /* Rust display engine toggle */
  defsubr (&Sneomacs_set_rust_display);

  /* Tell Emacs about this window system */
  Fprovide (Qneomacs, Qnil);
}

#endif /* HAVE_NEOMACS */
