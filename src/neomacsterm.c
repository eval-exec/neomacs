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

#include "lisp.h"
#include "blockinput.h"
#include "sysselect.h"
#include "neomacsterm.h"
#include "buffer.h"
#include "coding.h"
#include "window.h"
#include "keyboard.h"
#include "termhooks.h"
#include "termchar.h"
#include "font.h"
#include "pdumper.h"

/* List of Neomacs display info structures */
struct neomacs_display_info *neomacs_display_list = NULL;

/* The redisplay interface for Neomacs frames */
static struct redisplay_interface neomacs_redisplay_interface;

/* Prototypes for internal functions */
static void neomacs_initialize_display_info (struct neomacs_display_info *);


/* ============================================================================
 * Display Initialization
 * ============================================================================ */

/* Initialize the Neomacs display subsystem */
void
neomacs_term_init (void)
{
  /* Initialize the Rust display engine */
  /* This will be called once at Emacs startup */
}

/* Create a new Neomacs display connection */
struct neomacs_display_info *
neomacs_open_display (const char *display_name)
{
  struct neomacs_display_info *dpyinfo;

  dpyinfo = xzalloc (sizeof *dpyinfo);
  neomacs_initialize_display_info (dpyinfo);

  /* Initialize the Rust display engine */
  dpyinfo->display_handle = neomacs_display_init (BACKEND_TYPE_GTK4);

  if (!dpyinfo->display_handle)
    {
      xfree (dpyinfo);
      error ("Failed to initialize Neomacs display engine");
    }

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
  dpyinfo->background_pixel = 0xffffff;
  dpyinfo->smallest_char_width = 8;
  dpyinfo->smallest_font_height = 16;
  dpyinfo->supports_argb = true;
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

/* Create a terminal for a Neomacs display */
struct terminal *
neomacs_create_terminal (struct neomacs_display_info *dpyinfo)
{
  struct terminal *terminal;

  terminal = create_terminal (output_neomacs, &neomacs_redisplay_interface);

  terminal->display_info.neomacs = dpyinfo;
  dpyinfo->terminal = terminal;

  terminal->name = xstrdup ("neomacs");

  /* Set up terminal hooks */
  terminal->delete_terminal_hook = neomacs_delete_terminal;
  terminal->update_begin_hook = neomacs_update_begin;
  terminal->update_end_hook = neomacs_update_end;
  terminal->defined_color_hook = neomacs_defined_color;

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

  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_begin_frame (dpyinfo->display_handle);
}

/* Called at the end of updating a frame */
void
neomacs_update_end (struct frame *f)
{
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_end_frame (dpyinfo->display_handle);
}

/* Flush pending output to display */
void
neomacs_flush_display (struct frame *f)
{
  /* The Rust backend handles flushing internally */
}


/* ============================================================================
 * Color Support
 * ============================================================================ */

/* Check if a color name is valid and return RGB values */
bool
neomacs_defined_color (struct frame *f, const char *color_name,
                       Emacs_Color *color_def, bool alloc, bool makeIndex)
{
  /* Simple color name parsing - expand as needed */
  if (!color_name || !color_def)
    return false;

  /* Try to parse common color names */
  if (strcmp (color_name, "black") == 0)
    {
      color_def->red = color_def->green = color_def->blue = 0;
      color_def->pixel = 0x000000;
      return true;
    }
  if (strcmp (color_name, "white") == 0)
    {
      color_def->red = color_def->green = color_def->blue = 65535;
      color_def->pixel = 0xffffff;
      return true;
    }

  /* Try to parse #RRGGBB format */
  if (color_name[0] == '#' && strlen (color_name) == 7)
    {
      unsigned int r, g, b;
      if (sscanf (color_name + 1, "%2x%2x%2x", &r, &g, &b) == 3)
        {
          color_def->red = r * 257;   /* Scale 0-255 to 0-65535 */
          color_def->green = g * 257;
          color_def->blue = b * 257;
          color_def->pixel = RGB_TO_ULONG (r, g, b);
          return true;
        }
    }

  return false;
}


/* ============================================================================
 * Text Drawing
 * ============================================================================ */

/* Draw a glyph string */
void
neomacs_draw_glyph_string (struct glyph_string *s)
{
  struct frame *f = s->f;
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!dpyinfo || !dpyinfo->display_handle)
    return;

  /* Get face colors */
  unsigned long fg = s->face->foreground;
  unsigned long bg = s->face->background;

  /* Convert Emacs glyphs to Neomacs format */
  /* For now, iterate through glyph string and add characters */
  for (int i = 0; i < s->nchars; i++)
    {
      struct glyph *g = s->first_glyph + i;

      switch (g->type)
        {
        case CHAR_GLYPH:
          neomacs_display_add_char_glyph (dpyinfo->display_handle,
                                          g->u.ch,
                                          s->face->id,
                                          g->pixel_width,
                                          FONT_BASE (s->font),
                                          FONT_DESCENT (s->font));
          break;

        case STRETCH_GLYPH:
          neomacs_display_add_stretch_glyph (dpyinfo->display_handle,
                                             g->pixel_width,
                                             FRAME_LINE_HEIGHT (f),
                                             s->face->id);
          break;

        case IMAGE_GLYPH:
          /* TODO: Handle image glyphs */
          break;

        default:
          break;
        }
    }
}

/* Clear a rectangle on the frame */
void
neomacs_clear_frame_area (struct frame *f, int x, int y, int width, int height)
{
  /* This would clear an area - for now, the Rust backend handles clearing */
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
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!dpyinfo || !dpyinfo->display_handle || !on_p)
    return;

  /* Convert cursor type to Neomacs style */
  int style = 0;  /* Box cursor */
  switch (cursor_type)
    {
    case DEFAULT_CURSOR:
    case FILLED_BOX_CURSOR:
      style = 0;
      break;
    case BAR_CURSOR:
      style = 1;
      break;
    case HBAR_CURSOR:
      style = 2;
      break;
    case HOLLOW_BOX_CURSOR:
      style = 3;
      break;
    case NO_CURSOR:
      return;
    }

  /* Get cursor colors */
  unsigned long cursor_color = FRAME_NEOMACS_OUTPUT (f)->cursor_pixel;

  /* Set cursor in current window */
  int char_width = cursor_width > 0 ? cursor_width : FRAME_COLUMN_WIDTH (f);
  int char_height = FRAME_LINE_HEIGHT (f);

  neomacs_display_set_cursor (dpyinfo->display_handle,
                              (float) x, (float) y,
                              (float) char_width, (float) char_height,
                              style, cursor_color, active_p ? 1 : 0);
}


/* ============================================================================
 * Scrolling
 * ============================================================================ */

/* Scroll the contents of a window */
void
neomacs_scroll_run (struct window *w, struct run *run)
{
  struct frame *f = XFRAME (w->frame);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (!dpyinfo || !dpyinfo->display_handle)
    return;

  /* For smooth scrolling, use the animation API */
  /* neomacs_display_smooth_scroll (dpyinfo->display_handle, ...); */
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

/* Current Cairo context for drawing - thread-local for safety */
static cairo_t *neomacs_current_cr = NULL;

/* Begin Cairo clip region for drawing.  Returns a Cairo context.  */
cairo_t *
neomacs_begin_cr_clip (struct frame *f)
{
  /* For now, we return a placeholder - actual implementation will get
     the Cairo context from the GTK4 drawing area via the Rust FFI.  */
  /* TODO: Get Cairo context from Rust display engine */
  return neomacs_current_cr;
}

/* End Cairo clip region.  */
void
neomacs_end_cr_clip (struct frame *f)
{
  /* Restore previous clip region if needed */
}

/* Set Cairo source color for drawing.  */
void
neomacs_set_cr_source_with_color (struct frame *f, unsigned long color,
                                   bool check_alpha)
{
  if (!neomacs_current_cr)
    return;

  /* Extract RGB components from unsigned long color (0xAARRGGBB format) */
  double r = ((color >> 16) & 0xff) / 255.0;
  double g = ((color >> 8) & 0xff) / 255.0;
  double b = (color & 0xff) / 255.0;

  cairo_set_source_rgb (neomacs_current_cr, r, g, b);
}


/* ============================================================================
 * Redisplay Interface
 * ============================================================================ */

/* Initialize the redisplay interface */
static void
neomacs_setup_redisplay_interface (void)
{
  neomacs_redisplay_interface.produce_glyphs = NULL;  /* Use default */
  neomacs_redisplay_interface.write_glyphs = NULL;    /* Use default */
  neomacs_redisplay_interface.insert_glyphs = NULL;   /* Use default */
  neomacs_redisplay_interface.clear_end_of_line = NULL;
  neomacs_redisplay_interface.scroll_run_hook = neomacs_scroll_run;
  neomacs_redisplay_interface.after_update_window_line_hook = NULL;
  neomacs_redisplay_interface.update_window_begin_hook = NULL;
  neomacs_redisplay_interface.update_window_end_hook = NULL;
  neomacs_redisplay_interface.flush_display = neomacs_flush_display;
  neomacs_redisplay_interface.clear_window_mouse_face = NULL;
  neomacs_redisplay_interface.get_glyph_overhangs = NULL;
  neomacs_redisplay_interface.fix_overlapping_area = NULL;
  neomacs_redisplay_interface.draw_fringe_bitmap = neomacs_draw_fringe_bitmap;
  neomacs_redisplay_interface.define_fringe_bitmap = NULL;
  neomacs_redisplay_interface.destroy_fringe_bitmap = NULL;
  neomacs_redisplay_interface.compute_glyph_string_overhangs = NULL;
  neomacs_redisplay_interface.draw_glyph_string = neomacs_draw_glyph_string;
  neomacs_redisplay_interface.clear_frame_area = neomacs_clear_frame_area;
  neomacs_redisplay_interface.clear_under_internal_border = NULL;
  neomacs_redisplay_interface.draw_window_cursor = neomacs_draw_window_cursor;
  neomacs_redisplay_interface.draw_vertical_window_border = NULL;
  neomacs_redisplay_interface.draw_window_divider = NULL;
  neomacs_redisplay_interface.shift_glyphs_for_insert = NULL;
  neomacs_redisplay_interface.show_hourglass = NULL;
  neomacs_redisplay_interface.hide_hourglass = NULL;
}


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
 * Miscellaneous Functions
 * ============================================================================ */

/* Called from frame.c to get display info for x-get-resource.  */
struct neomacs_display_info *
check_x_display_info (Lisp_Object frame)
{
  struct frame *f;

  if (NILP (frame))
    f = SELECTED_FRAME ();
  else
    {
      CHECK_FRAME (frame);
      f = XFRAME (frame);
    }

  if (!FRAME_NEOMACS_P (f))
    error ("Frame is not a Neomacs frame");

  return FRAME_NEOMACS_DISPLAY_INFO (f);
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
 * Initialization
 * ============================================================================ */

void
syms_of_neomacsterm (void)
{
  /* Set up redisplay interface */
  neomacs_setup_redisplay_interface ();

  defsubr (&Sneomacs_available_p);
  defsubr (&Sneomacs_display_list);
  defsubr (&Sx_hide_tip);
  defsubr (&Sxw_display_color_p);
  defsubr (&Sx_display_grayscale_p);

  DEFSYM (Qneomacs, "neomacs");
}

#endif /* HAVE_NEOMACS */
