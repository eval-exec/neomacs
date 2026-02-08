/* Functions for the Neomacs GPU-accelerated display backend.
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

#define NLOG_MODULE "frame"
#include "neomacs_log.h"

#include <math.h>
#include <gtk/gtk.h>

#include "lisp.h"
#include "blockinput.h"
#include "neomacsterm.h"
#include "neomacs_display.h"
#include "buffer.h"
#include "window.h"
#include "keyboard.h"
#include "frame.h"
#include "termhooks.h"
#include "coding.h"
#include "font.h"
#include "dispextern.h"

/* GTK4 objects for each frame */
struct neomacs_frame_data
{
  GtkWidget *window;
  GtkWidget *drawing_area;
  int width;
  int height;
};

/* Tooltip state (shared between x-show-tip and x-hide-tip) */
static Lisp_Object tip_timer;
static Lisp_Object tip_last_frame;
static Lisp_Object tip_last_string;
static Lisp_Object tip_last_parms;
static bool tip_showing;

/* Forward declarations */
static void neomacs_set_title_handler (struct frame *, Lisp_Object, Lisp_Object);
static struct neomacs_display_info *check_neomacs_display_info (Lisp_Object);
static int x_decode_color (struct frame *f, Lisp_Object color_name, int mono_color);

/* ============================================================================
 * Color decoding
 * ============================================================================ */

/* Given a color name, return the color value for it.
   MONO_COLOR is returned if the color can't be found. */
static int
x_decode_color (struct frame *f, Lisp_Object color_name, int mono_color)
{
  Emacs_Color cdef;

  CHECK_STRING (color_name);

  /* Just return mono_color for now - proper color parsing would need
     to use GDK or similar. */
  if (NILP (color_name) || !STRINGP (color_name))
    return mono_color;

  /* Try to parse as #RRGGBB */
  const char *name = SSDATA (color_name);
  if (name[0] == '#' && strlen (name) == 7)
    {
      unsigned int r, g, b;
      if (sscanf (name + 1, "%02x%02x%02x", &r, &g, &b) == 3)
	return (r << 16) | (g << 8) | b;
    }

  /* Simple color name lookup */
  if (strcmp (name, "black") == 0)
    return 0x000000;
  if (strcmp (name, "white") == 0)
    return 0xFFFFFF;
  if (strcmp (name, "red") == 0)
    return 0xFF0000;
  if (strcmp (name, "green") == 0)
    return 0x00FF00;
  if (strcmp (name, "blue") == 0)
    return 0x0000FF;
  if (strcmp (name, "yellow") == 0)
    return 0xFFFF00;
  if (strcmp (name, "cyan") == 0)
    return 0x00FFFF;
  if (strcmp (name, "magenta") == 0)
    return 0xFF00FF;
  if (strcmp (name, "gray") == 0 || strcmp (name, "grey") == 0)
    return 0x808080;

  return mono_color;
}

/* ============================================================================
 * Frame parameter handlers
 * ============================================================================ */

static void
neomacs_set_foreground_color (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  unsigned long fg;

  if (STRINGP (arg))
    {
      fg = x_decode_color (f, arg, BLACK_PIX_DEFAULT (f));
      FRAME_FOREGROUND_PIXEL (f) = fg;
      FRAME_NEOMACS_OUTPUT (f)->foreground_pixel = fg;
      update_face_from_frame_parameter (f, Qforeground_color, arg);

      if (FRAME_VISIBLE_P (f))
        redraw_frame (f);
    }
}

static void
neomacs_set_background_color (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  unsigned long bg;

  if (STRINGP (arg))
    {
      bg = x_decode_color (f, arg, WHITE_PIX_DEFAULT (f));

      /* Update both the frame's background pixel and the output struct.
         FRAME_BACKGROUND_PIXEL(f) is used by face code for defaulted
         backgrounds (face->background_defaulted_p).  */
      FRAME_BACKGROUND_PIXEL (f) = bg;
      FRAME_NEOMACS_OUTPUT (f)->background_pixel = bg;

      /* Send new background to the renderer for LoadOp::Clear.  */
      struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
      if (dpyinfo && dpyinfo->display_handle)
        {
          uint32_t bg_rgb = ((RED_FROM_ULONG (bg) << 16) |
                             (GREEN_FROM_ULONG (bg) << 8) |
                             BLUE_FROM_ULONG (bg));
          neomacs_display_set_background (dpyinfo->display_handle, bg_rgb);
        }

      update_face_from_frame_parameter (f, Qbackground_color, arg);

      if (FRAME_VISIBLE_P (f))
        redraw_frame (f);
    }
}

static void
neomacs_set_cursor_color (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Set cursor color */
  if (STRINGP (arg))
    FRAME_NEOMACS_OUTPUT (f)->cursor_pixel = x_decode_color (f, arg, BLACK_PIX_DEFAULT (f));
}

static void
neomacs_set_cursor_type (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  set_frame_cursor_types (f, arg);
}

static void
neomacs_set_mouse_color (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Mouse color - not used in Neomacs currently */
}

static void
neomacs_set_border_color (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Border color - not used in Neomacs currently */
}

static void
neomacs_set_menu_bar_lines (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  /* Menu bar lines - not implemented yet */
}

static void
neomacs_set_tab_bar_lines (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  int nlines;

  if (FRAME_MINIBUF_ONLY_P (f))
    return;

  if (RANGED_FIXNUMP (0, value, INT_MAX))
    nlines = XFIXNAT (value);
  else
    nlines = 0;

  neomacs_change_tab_bar_height (f, nlines * FRAME_LINE_HEIGHT (f));
}

/* Set the pixel height of the tab bar of frame F to HEIGHT.  */
void
neomacs_change_tab_bar_height (struct frame *f, int height)
{
  int unit = FRAME_LINE_HEIGHT (f);
  int old_height = FRAME_TAB_BAR_HEIGHT (f);
  int lines = height / unit;

  /* Even if HEIGHT is less than unit (e.g., tab bar face not as tall),
     ensure at least 1 line if height is nonzero.  */
  if (lines == 0 && height != 0)
    lines = 1;

  /* Make sure we redisplay all windows in this frame.  */
  fset_redisplay (f);

  /* Recalculate tab bar and frame text sizes.  */
  FRAME_TAB_BAR_HEIGHT (f) = height;
  FRAME_TAB_BAR_LINES (f) = lines;
  store_frame_param (f, Qtab_bar_lines, make_fixnum (lines));

  /* Clear frame when tab bar height changes - need full redraw */
  if (FRAME_NEOMACS_WINDOW (f) && height != old_height)
    {
      /* Clear GPU renderer's glyph buffer to force full redraw */
      struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
      if (dpyinfo && dpyinfo->display_handle)
        neomacs_display_clear_all_glyphs (dpyinfo->display_handle);

      clear_frame (f);
      clear_current_matrices (f);
    }

  /* Clear old tab bar contents if shrinking */
  if ((height < old_height) && WINDOWP (f->tab_bar_window))
    clear_glyph_matrix (XWINDOW (f->tab_bar_window)->current_matrix);

  if (!f->tab_bar_resized)
    {
      Lisp_Object fullscreen = get_frame_param (f, Qfullscreen);

      /* As long as tab_bar_resized is false, try to change F's native height.  */
      if (NILP (fullscreen) || EQ (fullscreen, Qfullwidth))
	adjust_frame_size (f, FRAME_TEXT_WIDTH (f), FRAME_TEXT_HEIGHT (f),
			   1, false, Qtab_bar_lines);
      else
	adjust_frame_size (f, -1, -1, 4, false, Qtab_bar_lines);

      f->tab_bar_resized = f->tab_bar_redisplayed;

      /* After frame size adjustment, resize GTK widget to match new frame size */
      struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
      if (output && output->drawing_area)
        {
          int new_width = FRAME_PIXEL_WIDTH (f);
          int new_height = FRAME_PIXEL_HEIGHT (f);
          gtk_widget_set_size_request (GTK_WIDGET (output->drawing_area),
                                       new_width, new_height);
          if (GTK_IS_WINDOW (output->widget))
            {
              gtk_window_set_default_size (GTK_WINDOW (output->widget),
                                           new_width, new_height);
              gtk_widget_queue_resize (GTK_WIDGET (output->widget));
            }
        }
    }
  else
    /* Any other change may leave the native size of F alone.  */
    adjust_frame_size (f, -1, -1, 3, false, Qtab_bar_lines);

  /* adjust_frame_size might not have done anything, garbage frame here.  */
  adjust_frame_glyphs (f);
  SET_FRAME_GARBAGED (f);
}

static void
neomacs_set_tool_bar_lines (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  /* Tool bar lines - not implemented yet */
}

static void
neomacs_set_internal_border_width (struct frame *f, Lisp_Object value,
                                    Lisp_Object oldval)
{
  int border = check_int_nonnegative (value);

  if (border != FRAME_INTERNAL_BORDER_WIDTH (f))
    {
      f->internal_border_width = border;
      if (FRAME_NEOMACS_P (f))
	{
	  adjust_frame_size (f, -1, -1, 3, false,
			     Qinternal_border_width);
	  SET_FRAME_GARBAGED (f);
	}
    }
}

static void
neomacs_set_child_frame_border_width (struct frame *f, Lisp_Object value,
                                       Lisp_Object oldval)
{
  int border;

  if (NILP (value))
    border = -1;
  else if (RANGED_FIXNUMP (0, value, INT_MAX))
    border = XFIXNAT (value);
  else
    signal_error ("Invalid child frame border width", value);

  if (border != FRAME_CHILD_FRAME_BORDER_WIDTH (f))
    {
      f->child_frame_border_width = border;
      if (FRAME_NEOMACS_P (f))
	{
	  adjust_frame_size (f, -1, -1, 3, false,
			     Qchild_frame_border_width);
	  SET_FRAME_GARBAGED (f);
	}
    }
}

static void
neomacs_explicitly_set_name (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  neomacs_set_name (f, arg, true);
}

static void
neomacs_set_icon_name (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Icon name - not implemented yet */
}

static void
neomacs_set_icon_type (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Icon type - not implemented yet */
}

/* Change the title of frame F to NAME.
   If NAME is nil, use the frame name as the title.  */
static void
neomacs_set_title_handler (struct frame *f, Lisp_Object name,
                           Lisp_Object old_name)
{
  if (EQ (name, f->title))
    return;

  update_mode_lines = 22;
  fset_title (f, name);

  if (NILP (name))
    name = f->name;
  else
    CHECK_STRING (name);

  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->display_handle)
    {
      Lisp_Object encoded = ENCODE_UTF_8 (name);
      neomacs_display_set_title (dpyinfo->display_handle,
                                 SSDATA (encoded));
    }
}

static void
neomacs_set_scroll_bar_foreground (struct frame *f, Lisp_Object arg,
                                    Lisp_Object oldval)
{
  if (FRAME_TOOLTIP_P (f))
    return;
  update_face_from_frame_parameter (f, Qscroll_bar_foreground, arg);
}

static void
neomacs_set_scroll_bar_background (struct frame *f, Lisp_Object arg,
                                    Lisp_Object oldval)
{
  if (FRAME_TOOLTIP_P (f))
    return;
  update_face_from_frame_parameter (f, Qscroll_bar_background, arg);
}

static void
neomacs_set_sticky (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Sticky - not implemented yet */
}

static void
neomacs_set_tool_bar_position (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Tool bar position - not implemented yet */
}

static void
neomacs_set_undecorated (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  if (!EQ (arg, oldval))
    {
      FRAME_UNDECORATED (f) = NILP (arg) ? false : true;
      struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
      if (dpyinfo && dpyinfo->display_handle)
	neomacs_display_set_decorated (dpyinfo->display_handle,
				       NILP (arg) ? 1 : 0);
    }
}

static void
neomacs_set_parent_frame (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Parent frame - not implemented yet */
}

static void
neomacs_set_skip_taskbar (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Skip taskbar - not implemented yet */
}

static void
neomacs_set_no_focus_on_map (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  if (!EQ (arg, oldval))
    FRAME_NO_FOCUS_ON_MAP (f) = !NILP (arg);
}

static void
neomacs_set_no_accept_focus (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  if (!EQ (arg, oldval))
    FRAME_NO_ACCEPT_FOCUS (f) = !NILP (arg);
}

static void
neomacs_set_z_group (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Z group (always-on-top, etc.) - not supported by winit yet */
}

static void
neomacs_set_override_redirect (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Override redirect - not applicable to GPU rendering */
}

static void
neomacs_set_alpha_background (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  double alpha = 1.0;

  if (FLOATP (arg))
    {
      alpha = XFLOAT_DATA (arg);
      if (alpha < 0.0 || alpha > 1.0)
	args_out_of_range (make_float (0.0), make_float (1.0));
    }
  else if (FIXNUMP (arg))
    {
      EMACS_INT ialpha = XFIXNUM (arg);
      if (ialpha < 0 || ialpha > 100)
	args_out_of_range (make_fixnum (0), make_fixnum (100));
      alpha = ialpha / 100.0;
    }

  f->alpha_background = alpha;

  if (FRAME_TERMINAL (f)->set_frame_alpha_hook)
    FRAME_TERMINAL (f)->set_frame_alpha_hook (f);
}

/* Frame parameter handlers table - must match the order in frame.c frame_parms table */
frame_parm_handler neomacs_frame_parm_handlers[] =
  {
    gui_set_autoraise,			/* autoraise */
    gui_set_autolower,			/* autolower */
    neomacs_set_background_color,	/* background-color */
    neomacs_set_border_color,		/* border-color */
    gui_set_border_width,		/* border-width */
    neomacs_set_cursor_color,		/* cursor-color */
    neomacs_set_cursor_type,		/* cursor-type */
    gui_set_font,			/* font */
    neomacs_set_foreground_color,	/* foreground-color */
    neomacs_set_icon_name,		/* icon-name */
    neomacs_set_icon_type,		/* icon-type */
    neomacs_set_child_frame_border_width, /* child-frame-border-width */
    neomacs_set_internal_border_width,	/* internal-border-width */
    gui_set_right_divider_width,	/* right-divider-width */
    gui_set_bottom_divider_width,	/* bottom-divider-width */
    neomacs_set_menu_bar_lines,		/* menu-bar-lines */
    neomacs_set_mouse_color,		/* mouse-color */
    neomacs_explicitly_set_name,	/* name */
    gui_set_scroll_bar_width,		/* scroll-bar-width */
    gui_set_scroll_bar_height,		/* scroll-bar-height */
    neomacs_set_title_handler,		/* title */
    gui_set_unsplittable,		/* unsplittable */
    gui_set_vertical_scroll_bars,	/* vertical-scroll-bars */
    gui_set_horizontal_scroll_bars,	/* horizontal-scroll-bars */
    gui_set_visibility,			/* visibility */
    neomacs_set_tab_bar_lines,		/* tab-bar-lines */
    neomacs_set_tool_bar_lines,		/* tool-bar-lines */
    neomacs_set_scroll_bar_foreground,	/* scroll-bar-foreground */
    neomacs_set_scroll_bar_background,	/* scroll-bar-background */
    gui_set_screen_gamma,		/* screen-gamma */
    gui_set_line_spacing,		/* line-spacing */
    gui_set_left_fringe,		/* left-fringe */
    gui_set_right_fringe,		/* right-fringe */
    NULL,				/* wait-for-wm */
    gui_set_fullscreen,			/* fullscreen */
    gui_set_font_backend,		/* font-backend */
    gui_set_alpha,			/* alpha */
    neomacs_set_sticky,			/* sticky */
    neomacs_set_tool_bar_position,	/* tool-bar-position */
    NULL,				/* inhibit-double-buffering */
    neomacs_set_undecorated,		/* undecorated */
    neomacs_set_parent_frame,		/* parent-frame */
    neomacs_set_skip_taskbar,		/* skip-taskbar */
    neomacs_set_no_focus_on_map,	/* no-focus-on-map */
    neomacs_set_no_accept_focus,	/* no-accept-focus */
    neomacs_set_z_group,		/* z-group */
    neomacs_set_override_redirect,	/* override-redirect */
    gui_set_no_special_glyphs,		/* no-special-glyphs */
    neomacs_set_alpha_background,	/* alpha-background */
    gui_set_borders_respect_alpha_background, /* borders-respect-alpha-background */
    NULL,				/* use-frame-synchronization */
  };

/* ============================================================================
 * Display Info Utilities
 * ============================================================================ */

/* Get or create display info for a frame or display specifier.  */
static struct neomacs_display_info *
check_neomacs_display_info (Lisp_Object object)
{
  struct frame *f;
  struct neomacs_display_info *dpyinfo;

  if (NILP (object))
    {
      f = SELECTED_FRAME ();
      if (FRAME_NEOMACS_P (f))
        return FRAME_NEOMACS_DISPLAY_INFO (f);

      /* No display yet, create one */
      dpyinfo = neomacs_display_list;
      if (dpyinfo)
        return dpyinfo;

      /* Initialize a new display */
      return neomacs_open_display (NULL);
    }
  else if (FRAMEP (object))
    {
      f = XFRAME (object);
      if (!FRAME_NEOMACS_P (f))
        error ("Not a Neomacs frame");
      return FRAME_NEOMACS_DISPLAY_INFO (f);
    }
  else if (STRINGP (object))
    {
      /* Open a new display with the given name */
      return neomacs_open_display (SSDATA (object));
    }
  else if (TERMINALP (object))
    {
      struct terminal *t = decode_live_terminal (object);
      if (t->type != output_neomacs)
        error ("Not a Neomacs terminal");
      return t->display_info.neomacs;
    }

  /* Default: return first available display */
  return neomacs_display_list;
}


/* ============================================================================
 * GTK4 Window Management
 * ============================================================================ */

/* Ensure the frame has a Cairo surface of the right size */
static void
neomacs_ensure_cr_surface (struct frame *f, int width, int height)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  /* Check if we need to recreate the surface */
  if (output->cr_surface)
    {
      int cur_width = cairo_image_surface_get_width (output->cr_surface);
      int cur_height = cairo_image_surface_get_height (output->cr_surface);
      if (cur_width == width && cur_height == height)
	return;  /* Surface is the right size */

      /* Destroy old context and surface */
      if (output->cr_context)
	{
	  cairo_destroy (output->cr_context);
	  output->cr_context = NULL;
	}
      cairo_surface_destroy (output->cr_surface);
      output->cr_surface = NULL;
    }

  /* Create new surface - use RGB24 (no alpha) for opaque rendering */
  output->cr_surface = cairo_image_surface_create (CAIRO_FORMAT_RGB24,
						   width, height);
  output->cr_context = cairo_create (output->cr_surface);

  /* Fill with background color */
  unsigned long bg = output->background_pixel;
  double r = RED_FROM_ULONG (bg) / 255.0;
  double g = GREEN_FROM_ULONG (bg) / 255.0;
  double b = BLUE_FROM_ULONG (bg) / 255.0;
  cairo_set_source_rgb (output->cr_context, r, g, b);
  cairo_paint (output->cr_context);
}

/* Callback for GTK4 drawing area resize */
static void
neomacs_resize_cb (GtkDrawingArea *area, int width, int height,
                   gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;

  if (!FRAME_NEOMACS_P (f))
    return;

  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_resize (dpyinfo->display_handle, width, height);

  /* Ensure we have a surface of the right size */
  neomacs_ensure_cr_surface (f, width, height);

  /* Update frame dimensions */
  int old_cols = FRAME_COLS (f);
  int old_rows = FRAME_LINES (f);
  int new_cols = width / FRAME_COLUMN_WIDTH (f);
  int new_rows = height / FRAME_LINE_HEIGHT (f);

  if (new_cols != old_cols || new_rows != old_rows)
    {
      /* change_frame_size expects PIXEL dimensions, not character dimensions */
      change_frame_size (f, width, height, false, true, false);
    }
}

/* Callback for GPU widget resize (called from Rust via FFI) */
static void
neomacs_widget_resize_cb (void *user_data, int width, int height)
{
  struct frame *f = (struct frame *) user_data;

  if (!f || !FRAME_LIVE_P (f) || !FRAME_NEOMACS_P (f))
    return;

  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  /* Update Rust display engine with new size */
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_resize (dpyinfo->display_handle, width, height);

  /* Avoid division by zero */
  int col_width = FRAME_COLUMN_WIDTH (f);
  int line_height = FRAME_LINE_HEIGHT (f);
  if (col_width <= 0) col_width = 8;
  if (line_height <= 0) line_height = 16;

  int new_cols = width / col_width;
  int new_rows = height / line_height;

  if (new_cols > 0 && new_rows > 0)
    {
      /* Update pixel dimensions */
      FRAME_PIXEL_WIDTH (f) = width;
      FRAME_PIXEL_HEIGHT (f) = height;

      /* Change frame size - pass PIXEL dimensions, not character dimensions */
      change_frame_size (f, width, height, false, true, false);

      /* Reallocate glyph matrices for new size */
      adjust_frame_glyphs (f);

      /* Clear old glyph data that was built with wrong size */
      clear_current_matrices (f);

      /* Force full redisplay to update content */
      SET_FRAME_GARBAGED (f);

      /* Also mark windows as needing update */
      mark_window_display_accurate (FRAME_ROOT_WINDOW (f), false);

      /* Signal that windows changed - forces redisplay cycle */
      windows_or_buffers_changed = 63;
    }
}

/* Callback for GTK4 drawing area draw */
static void
neomacs_draw_cb (GtkDrawingArea *area, cairo_t *cr,
                 int width, int height, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct neomacs_output *output;
  struct neomacs_display_info *dpyinfo;

  if (!FRAME_NEOMACS_P (f))
    return;

  output = FRAME_NEOMACS_OUTPUT (f);
  dpyinfo = FRAME_DISPLAY_INFO (f);

  /* Use SOURCE operator to replace pixels (ignores destination alpha) */
  cairo_set_operator (cr, CAIRO_OPERATOR_SOURCE);

  /* Try Rust rendering first */
  if (dpyinfo && dpyinfo->display_handle)
    {
      /* Have Rust render the scene graph directly to the widget's Cairo context */
      neomacs_display_render_to_cairo (dpyinfo->display_handle, cr);
      return;
    }

  /* Fallback: use C Cairo backing surface */
  neomacs_ensure_cr_surface (f, width, height);

  if (output->cr_surface)
    {
      cairo_set_source_surface (cr, output->cr_surface, 0, 0);
      cairo_paint (cr);
    }
  else
    {
      /* Fallback: draw background */
      double r = 1.0, g = 1.0, b = 1.0;
      if (FRAME_FACE_CACHE (f))
	{
	  struct face *face = FACE_FROM_ID (f, DEFAULT_FACE_ID);
	  if (face)
	    {
	      unsigned long bg = face->background;
	      r = RED_FROM_ULONG (bg) / 255.0;
	      g = GREEN_FROM_ULONG (bg) / 255.0;
	      b = BLUE_FROM_ULONG (bg) / 255.0;
	    }
	}
      cairo_set_source_rgb (cr, r, g, b);
      cairo_paint (cr);
    }
}

/* Convert GDK keyval to Emacs keysym */
static unsigned int
neomacs_translate_key (guint keyval, GdkModifierType state)
{
  /* Map some common keys */
  switch (keyval)
    {
    case GDK_KEY_Return:
    case GDK_KEY_KP_Enter:
      return 0x0D;  /* Return */
    case GDK_KEY_Tab:
    case GDK_KEY_KP_Tab:
    case GDK_KEY_ISO_Left_Tab:
      return 0x09;  /* Tab */
    case GDK_KEY_BackSpace:
      return 0;  /* Handle as function key, not ASCII 0x08 which conflicts with C-h */
    case GDK_KEY_Escape:
      return 0x1B;  /* Escape */
    case GDK_KEY_Delete:
      return 0;  /* Handle as function key */
    default:
      if (keyval >= 0x20 && keyval <= 0x7E)
	return keyval;  /* Printable ASCII */
      else if (keyval >= GDK_KEY_space && keyval <= GDK_KEY_asciitilde)
	return keyval - GDK_KEY_space + ' ';
      return 0;
    }
}

/* Callback for GTK4 key press events */
/* IM context commit callback */
static void
neomacs_im_commit_cb (GtkIMContext *ctx, const char *str, gpointer user_data)
{
  nlog_debug ("IM commit: '%s'", str);
}

static gboolean
neomacs_key_pressed_cb (GtkEventControllerKey *controller,
			guint keyval, guint keycode,
			GdkModifierType state, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  union buffered_input_event inev;
  unsigned int c;

  if (!FRAME_LIVE_P (f))
    return FALSE;

  nlog_debug ("Key pressed: keyval=%u (0x%x), keycode=%u, state=%u",
	     keyval, keyval, keycode, state);

  /* Ignore modifier-only key presses - they are only used as modifiers */
  switch (keyval)
    {
    case GDK_KEY_Shift_L:
    case GDK_KEY_Shift_R:
    case GDK_KEY_Control_L:
    case GDK_KEY_Control_R:
    case GDK_KEY_Caps_Lock:
    case GDK_KEY_Shift_Lock:
    case GDK_KEY_Meta_L:
    case GDK_KEY_Meta_R:
    case GDK_KEY_Alt_L:
    case GDK_KEY_Alt_R:
    case GDK_KEY_Super_L:
    case GDK_KEY_Super_R:
    case GDK_KEY_Hyper_L:
    case GDK_KEY_Hyper_R:
    case GDK_KEY_ISO_Lock:
    case GDK_KEY_ISO_Level2_Latch:
    case GDK_KEY_ISO_Level3_Shift:
    case GDK_KEY_ISO_Level3_Latch:
    case GDK_KEY_ISO_Level3_Lock:
    case GDK_KEY_ISO_Level5_Shift:
    case GDK_KEY_ISO_Level5_Latch:
    case GDK_KEY_ISO_Level5_Lock:
      return FALSE;  /* Let GTK handle modifier keys, don't send as events */
    }

  c = neomacs_translate_key (keyval, state);
  if (c == 0 && keyval > 0x7F)
    {
      /* Function key or special key - convert to symbol */
      EVENT_INIT (inev.ie);
      inev.ie.kind = NON_ASCII_KEYSTROKE_EVENT;
      inev.ie.code = keyval;
      inev.ie.modifiers = 0;
      if (state & GDK_CONTROL_MASK)
	inev.ie.modifiers |= ctrl_modifier;
      if (state & GDK_ALT_MASK)
	inev.ie.modifiers |= meta_modifier;
      if (state & GDK_SHIFT_MASK)
	inev.ie.modifiers |= shift_modifier;
      if (state & GDK_SUPER_MASK)
	inev.ie.modifiers |= super_modifier;
      inev.ie.timestamp = 0;
      XSETFRAME (inev.ie.frame_or_window, f);
      neomacs_evq_enqueue (&inev);
      return TRUE;
    }
  else if (c != 0)
    {
      /* ASCII character */
      EVENT_INIT (inev.ie);
      inev.ie.kind = ASCII_KEYSTROKE_EVENT;
      inev.ie.code = c;
      inev.ie.modifiers = 0;
      if (state & GDK_CONTROL_MASK)
	inev.ie.modifiers |= ctrl_modifier;
      if (state & GDK_ALT_MASK)
	inev.ie.modifiers |= meta_modifier;
      inev.ie.timestamp = 0;
      XSETFRAME (inev.ie.frame_or_window, f);
      nlog_debug ("Enqueueing ASCII key event: code=%u, modifiers=%d", c, inev.ie.modifiers);
      neomacs_evq_enqueue (&inev);
      return TRUE;
    }

  return FALSE;
}

/* Callback for focus enter */
static void
neomacs_focus_enter_cb (GtkEventControllerFocus *controller, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct input_event ie;
  struct neomacs_display_info *dpyinfo;
  GtkWidget *widget = gtk_event_controller_get_widget (GTK_EVENT_CONTROLLER (controller));

  if (!FRAME_LIVE_P (f))
    return;

  nlog_debug ("Focus entered frame %p, widget=%p (focusable=%d, has_focus=%d)",
	     (void *) f, (void *) widget,
	     gtk_widget_get_focusable (widget),
	     gtk_widget_has_focus (widget));

  /* Set this frame as the focus and highlight frame */
  dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo)
    {
      dpyinfo->focus_frame = f;
      dpyinfo->x_focus_frame = f;
      dpyinfo->highlight_frame = f;
      dpyinfo->x_highlight_frame = f;
      nlog_debug ("Set highlight_frame to %p", (void *) f);
    }

  /* Send focus-in event to Emacs */
  EVENT_INIT (ie);
  ie.kind = FOCUS_IN_EVENT;
  XSETFRAME (ie.frame_or_window, f);
  kbd_buffer_store_event (&ie);
}

/* Callback for focus leave */
static void
neomacs_focus_leave_cb (GtkEventControllerFocus *controller, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct input_event ie;
  struct neomacs_display_info *dpyinfo;

  if (!FRAME_LIVE_P (f))
    return;

  nlog_debug ("Focus left frame %p", (void *) f);

  /* Clear highlight frame if it was this frame */
  dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->highlight_frame == f)
    {
      dpyinfo->highlight_frame = NULL;
      dpyinfo->x_highlight_frame = NULL;
    }

  /* Send focus-out event to Emacs */
  EVENT_INIT (ie);
  ie.kind = FOCUS_OUT_EVENT;
  XSETFRAME (ie.frame_or_window, f);
  kbd_buffer_store_event (&ie);
}

/* Callback for widget realize - grab focus when ready */
static void
neomacs_realize_cb (GtkWidget *widget, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct neomacs_output *output;

  if (!FRAME_LIVE_P (f))
    return;

  output = FRAME_NEOMACS_OUTPUT (f);
  if (!output)
    return;

  /* Grab focus on the drawing area once widget is realized */
  if (output->drawing_area)
    gtk_widget_grab_focus (output->drawing_area);
}

/* Callback for GTK4 window close request */
static gboolean
neomacs_close_request_cb (GtkWindow *window, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;

  if (FRAME_LIVE_P (f))
    {
      /* Send delete event to Emacs */
      struct input_event ie;
      EVENT_INIT (ie);
      ie.kind = DELETE_WINDOW_EVENT;
      XSETFRAME (ie.frame_or_window, f);
      kbd_buffer_store_event (&ie);
    }

  return TRUE;  /* Prevent immediate close, let Emacs handle it */
}

/* State for tracking mouse drag */
static struct {
  bool button_pressed;
  int button;  /* Which button is pressed (1-based) */
  double start_x, start_y;  /* Where the press started */
  double last_x, last_y;    /* Last reported position during drag */
  struct frame *frame;
  guint32 last_press_time;  /* For deduplication */
  guint32 last_release_time;  /* For deduplication */
} mouse_drag_state = { false, 0, 0, 0, 0, 0, NULL, 0, 0 };

/* Unified legacy event callback - handles all mouse button and motion events */
static gboolean
neomacs_legacy_event_cb (GtkEventControllerLegacy *controller,
                         GdkEvent *event, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct input_event ie;
  GdkEventType event_type;
  guint button;
  double x, y;
  GdkModifierType state;
  guint32 time;

  if (!FRAME_LIVE_P (f))
    return FALSE;

  event_type = gdk_event_get_event_type (event);
  gdk_event_get_position (event, &x, &y);
  state = gdk_event_get_modifier_state (event);
  time = gdk_event_get_time (event);

  /* Handle button press */
  if (event_type == GDK_BUTTON_PRESS)
    {
      struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
      button = gdk_button_event_get_button (event);

      /* Deduplicate - multiple controllers may receive same event */
      if (time == mouse_drag_state.last_press_time)
        return FALSE;
      mouse_drag_state.last_press_time = time;

      /* Check if click is on a floating webkit view */
      if (dpyinfo && dpyinfo->display_handle)
        {
          uint32_t webkit_id = 0;
          int rel_x = 0, rel_y = 0;
          if (neomacs_display_webkit_at_position (dpyinfo->display_handle,
                                                   (int) x, (int) y,
                                                   &webkit_id, &rel_x, &rel_y))
            {
              /* Forward click to webkit view */
              neomacs_display_webkit_click (dpyinfo->display_handle,
                                            webkit_id, rel_x, rel_y, button);
              return TRUE;  /* Event handled by webkit */
            }
        }

      /* Track drag state */
      mouse_drag_state.button_pressed = true;
      mouse_drag_state.button = button;
      mouse_drag_state.start_x = x;
      mouse_drag_state.start_y = y;
      mouse_drag_state.last_x = x;
      mouse_drag_state.last_y = y;
      mouse_drag_state.frame = f;

      /* Send press event to Emacs */
      EVENT_INIT (ie);
      ie.kind = MOUSE_CLICK_EVENT;
      ie.code = button - 1;
      ie.timestamp = time;
      ie.modifiers = down_modifier;

      if (state & GDK_SHIFT_MASK)
        ie.modifiers |= shift_modifier;
      if (state & GDK_CONTROL_MASK)
        ie.modifiers |= ctrl_modifier;
      if (state & GDK_ALT_MASK)
        ie.modifiers |= meta_modifier;

      XSETINT (ie.x, (int) x);
      XSETINT (ie.y, (int) y);
      XSETFRAME (ie.frame_or_window, f);
      ie.arg = Qnil;

      kbd_buffer_store_event (&ie);

      /* GTK4 doesn't deliver release events to custom NeomacsWidget reliably.
         Send synthetic release immediately - this breaks drag-to-select but
         makes basic clicking work. TODO: Implement proper drag support. */
      EVENT_INIT (ie);
      ie.kind = MOUSE_CLICK_EVENT;
      ie.code = button - 1;
      ie.timestamp = time + 50;  /* Slightly later to not trigger double-click */
      ie.modifiers = up_modifier;

      if (state & GDK_SHIFT_MASK)
        ie.modifiers |= shift_modifier;
      if (state & GDK_CONTROL_MASK)
        ie.modifiers |= ctrl_modifier;
      if (state & GDK_ALT_MASK)
        ie.modifiers |= meta_modifier;

      XSETINT (ie.x, (int) x);
      XSETINT (ie.y, (int) y);
      XSETFRAME (ie.frame_or_window, f);
      ie.arg = Qnil;

      kbd_buffer_store_event (&ie);

      /* Reset drag state */
      mouse_drag_state.button_pressed = false;

      return FALSE;
    }

  /* Handle button release - since we send synthetic release on press,
     this just handles cleanup if GTK4 does eventually send a release */
  if (event_type == GDK_BUTTON_RELEASE)
    {
      /* Deduplicate */
      if (time == mouse_drag_state.last_release_time)
        return FALSE;
      mouse_drag_state.last_release_time = time;

      /* Don't send event - we already sent synthetic release on press */
      return FALSE;
    }

  return FALSE;
}

/* Callback for GTK4 mouse motion (non-drag) */
static void
neomacs_motion_cb (GtkEventControllerMotion *controller,
                   double x, double y, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct neomacs_display_info *dpyinfo;

  if (!FRAME_LIVE_P (f))
    return;

  dpyinfo = FRAME_DISPLAY_INFO (f);
  if (!dpyinfo)
    return;

  /* Check if hovering over a floating webkit view - forward motion */
  if (dpyinfo->display_handle)
    {
      uint32_t webkit_id = 0;
      int rel_x = 0, rel_y = 0;
      if (neomacs_display_webkit_at_position (dpyinfo->display_handle,
                                               (int) x, (int) y,
                                               &webkit_id, &rel_x, &rel_y))
        {
          /* Forward motion to webkit (event_type=1 for motion) */
          neomacs_display_webkit_send_pointer (dpyinfo->display_handle,
                                               webkit_id,
                                               1,  /* motion */
                                               rel_x, rel_y,
                                               0,  /* no button */
                                               0,  /* no state */
                                               0); /* no modifiers */
          /* Don't update Emacs mouse tracking for webkit areas */
          return;
        }
    }

  /* Update last mouse position - used by Emacs for tracking */
  f->mouse_moved = true;
  dpyinfo->last_mouse_frame = f;
}

/* Callback for GTK4 scroll events (mouse wheel) */
static gboolean
neomacs_scroll_cb (GtkEventControllerScroll *controller,
                   double dx, double dy, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  struct neomacs_display_info *dpyinfo;
  struct input_event ie;
  GdkEvent *event;
  double scroll_x = 0, scroll_y = 0;

  if (!FRAME_LIVE_P (f))
    return FALSE;

  dpyinfo = FRAME_DISPLAY_INFO (f);
  event = gtk_event_controller_get_current_event (GTK_EVENT_CONTROLLER (controller));
  if (!event)
    return FALSE;

  /* Get mouse position for webkit check */
  gdk_event_get_position (event, &scroll_x, &scroll_y);

  /* Check if scrolling over a floating webkit view */
  if (dpyinfo && dpyinfo->display_handle)
    {
      uint32_t webkit_id = 0;
      int rel_x = 0, rel_y = 0;
      if (neomacs_display_webkit_at_position (dpyinfo->display_handle,
                                               (int) scroll_x, (int) scroll_y,
                                               &webkit_id, &rel_x, &rel_y))
        {
          /* Forward scroll to webkit */
          neomacs_display_webkit_send_scroll (dpyinfo->display_handle,
                                              webkit_id,
                                              rel_x, rel_y,
                                              (int) (dx * 50),  /* Scale for reasonable scroll speed */
                                              (int) (dy * 50));
          return TRUE;  /* Event handled by webkit */
        }
    }

  EVENT_INIT (ie);
  ie.timestamp = gdk_event_get_time (event);
  XSETFRAME (ie.frame_or_window, f);
  ie.arg = Qnil;

  /* Use 0,0 as position - will be updated by next motion event */
  XSETINT (ie.x, 0);
  XSETINT (ie.y, 0);

  if (dy != 0)
    {
      /* Vertical scroll - button 4 (up) or 5 (down) */
      ie.kind = MOUSE_CLICK_EVENT;
      ie.code = dy < 0 ? 3 : 4;  /* Button 4 or 5, 0-indexed */
      ie.modifiers = down_modifier;
      kbd_buffer_store_event (&ie);

      /* Also send release */
      ie.modifiers = up_modifier;
      kbd_buffer_store_event (&ie);
    }

  if (dx != 0)
    {
      /* Horizontal scroll - button 6 (left) or 7 (right) */
      ie.kind = MOUSE_CLICK_EVENT;
      ie.code = dx < 0 ? 5 : 6;  /* Button 6 or 7, 0-indexed */
      ie.modifiers = down_modifier;
      kbd_buffer_store_event (&ie);

      ie.modifiers = up_modifier;
      kbd_buffer_store_event (&ie);
    }

  return TRUE;
}

/* Create winit window for a frame via Rust backend */
static void
neomacs_create_frame_widgets (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  int use_winit = 1;  /* Try winit by default */
  int use_gtk_fallback = 0;

  /* Allow forcing GTK fallback via environment variable */
  const char *winit_env = getenv ("NEOMACS_USE_WINIT");
  if (winit_env && strcmp (winit_env, "0") == 0)
    use_winit = 0;

  /* Initialize window_id to 0 (invalid) */
  output->window_id = 0;

  /* Try to create winit window via Rust backend */
  if (use_winit && dpyinfo && dpyinfo->display_handle)
    {
      uint32_t window_id = neomacs_display_create_window (
        dpyinfo->display_handle,
        FRAME_PIXEL_WIDTH (f),
        FRAME_PIXEL_HEIGHT (f),
        "Emacs"
      );

      if (window_id != 0)
        {
          /* Successfully created winit window */
          output->window_id = window_id;
          output->window_desc = (Window) window_id;

          /* Set up resize callback for the window */
          neomacs_display_set_resize_callback (neomacs_widget_resize_cb, f);

          /* In threaded mode, window creation happens asynchronously in render thread */

          /* Show the window */
          neomacs_display_show_window (dpyinfo->display_handle, window_id, true);

          /* Mark that we're using winit, not GTK widgets */
          output->widget = NULL;
          output->drawing_area = NULL;
          output->use_gpu_widget = 1;

          nlog_info ("Created winit window with id %u", window_id);
          return;
        }
      else
        {
          nlog_fatal ("Failed to create winit window");
          emacs_abort ();
        }
    }
  else
    {
      nlog_fatal ("winit backend not initialized");
      emacs_abort ();
    }

  /* GTK fallback path - only used if winit is disabled or fails */
  if (use_gtk_fallback)
    {
      GtkWidget *window, *drawing_area = NULL;
      GtkEventController *key_controller, *focus_controller, *motion_controller, *scroll_controller;
      int use_gpu_widget = 1;

      /* Allow disabling GPU widget via environment variable */
      const char *gpu_env = getenv ("NEOMACS_GPU_WIDGET");
      if (gpu_env && strcmp (gpu_env, "0") == 0)
        use_gpu_widget = 0;

      /* Create main window */
      window = gtk_window_new ();

      gtk_window_set_title (GTK_WINDOW (window), "Emacs");
      gtk_window_set_default_size (GTK_WINDOW (window),
                                   FRAME_PIXEL_WIDTH (f),
                                   FRAME_PIXEL_HEIGHT (f));

      /* Create drawing area - either standard DrawingArea or GPU NeomacsWidget */
      if (use_gpu_widget && dpyinfo && dpyinfo->display_handle)
        {
          /* Use GPU-accelerated NeomacsWidget */
          drawing_area = (GtkWidget *) neomacs_display_create_widget ();
          if (!drawing_area)
            {
              nlog_warn ("Failed to create NeomacsWidget, falling back to DrawingArea");
              use_gpu_widget = 0;
            }
        }
      else
        {
          /* dpyinfo not available, fall back to DrawingArea */
          use_gpu_widget = 0;
        }

      if (!use_gpu_widget)
        {
          /* Use standard DrawingArea with Cairo rendering */
          drawing_area = gtk_drawing_area_new ();
          gtk_drawing_area_set_content_width (GTK_DRAWING_AREA (drawing_area),
                                              FRAME_PIXEL_WIDTH (f));
          gtk_drawing_area_set_content_height (GTK_DRAWING_AREA (drawing_area),
                                               FRAME_PIXEL_HEIGHT (f));
          /* Connect draw callback for Cairo rendering */
          gtk_drawing_area_set_draw_func (GTK_DRAWING_AREA (drawing_area),
                                          neomacs_draw_cb, f, NULL);
        }
      else
        {
          /* Set size for GPU widget */
          gtk_widget_set_size_request (drawing_area,
                                       FRAME_PIXEL_WIDTH (f),
                                       FRAME_PIXEL_HEIGHT (f));
        }

      /* Make widget opaque - disable transparency */
      gtk_widget_set_opacity (drawing_area, 1.0);
      gtk_widget_set_opacity (window, 1.0);

      /* Add CSS to set a solid background (black for dark theme) */
      {
        GtkCssProvider *css_provider = gtk_css_provider_new ();
        gtk_css_provider_load_from_string (css_provider,
          "window, drawingarea { background-color: black; }");
        gtk_style_context_add_provider_for_display (
          gdk_display_get_default (),
          GTK_STYLE_PROVIDER (css_provider),
          GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
        g_object_unref (css_provider);
      }

      /* Make window and drawing area focusable to receive keyboard events */
      gtk_widget_set_focusable (window, TRUE);
      gtk_widget_set_can_focus (window, TRUE);
      gtk_widget_set_focusable (drawing_area, TRUE);
      gtk_widget_set_can_focus (drawing_area, TRUE);

      /* Connect callbacks - resize handling differs between widget types */
      if (!use_gpu_widget)
        {
          /* DrawingArea has its own resize signal */
          g_signal_connect (drawing_area, "resize",
                            G_CALLBACK (neomacs_resize_cb), f);
        }
      else
        {
          /* GPU widget: use Rust callback mechanism since NeomacsWidget doesn't have resize signal */
          neomacs_display_set_resize_callback (neomacs_widget_resize_cb, f);
        }
      g_signal_connect (window, "close-request",
                        G_CALLBACK (neomacs_close_request_cb), f);

      /* Add keyboard event controller on window with CAPTURE phase */
      key_controller = gtk_event_controller_key_new ();
      gtk_event_controller_set_propagation_phase (key_controller, GTK_PHASE_CAPTURE);
      g_signal_connect (key_controller, "key-pressed",
                        G_CALLBACK (neomacs_key_pressed_cb), f);
      gtk_widget_add_controller (window, key_controller);

      /* Add key controller to drawing area with BUBBLE phase */
      {
        GtkEventController *da_key_controller = gtk_event_controller_key_new ();
        gtk_event_controller_set_propagation_phase (da_key_controller, GTK_PHASE_BUBBLE);
        g_signal_connect (da_key_controller, "key-pressed",
                          G_CALLBACK (neomacs_key_pressed_cb), f);
        gtk_widget_add_controller (drawing_area, da_key_controller);
      }

      /* Also add key controller to drawing area with CAPTURE phase */
      {
        GtkEventController *da_key_controller2 = gtk_event_controller_key_new ();
        gtk_event_controller_set_propagation_phase (da_key_controller2, GTK_PHASE_CAPTURE);
        g_signal_connect (da_key_controller2, "key-pressed",
                          G_CALLBACK (neomacs_key_pressed_cb), f);
        gtk_widget_add_controller (drawing_area, da_key_controller2);
      }

      /* Create IM context for input method support */
      {
        GtkIMContext *imc = gtk_im_multicontext_new ();
        g_signal_connect (imc, "commit", G_CALLBACK (neomacs_im_commit_cb), f);
        gtk_im_context_set_client_widget (imc, drawing_area);
        gtk_im_context_focus_in (imc);
        output->im_context = imc;
      }

      /* Add focus event controller to window */
      focus_controller = gtk_event_controller_focus_new ();
      g_signal_connect (focus_controller, "enter",
                        G_CALLBACK (neomacs_focus_enter_cb), f);
      g_signal_connect (focus_controller, "leave",
                        G_CALLBACK (neomacs_focus_leave_cb), f);
      gtk_widget_add_controller (window, focus_controller);

      /* Also add focus controller to drawing area */
      {
        GtkEventController *da_focus_controller = gtk_event_controller_focus_new ();
        g_signal_connect (da_focus_controller, "enter",
                          G_CALLBACK (neomacs_focus_enter_cb), f);
        g_signal_connect (da_focus_controller, "leave",
                          G_CALLBACK (neomacs_focus_leave_cb), f);
        gtk_widget_add_controller (drawing_area, da_focus_controller);
      }

      /* Add legacy event controller for ALL mouse button events.
         We need controllers on BOTH window and drawing_area because GTK4 only
         delivers release events to window, but press events to drawing_area.
         Deduplication is handled in the callback using event timestamps. */
      {
        GtkEventController *legacy_controller = gtk_event_controller_legacy_new ();
        g_signal_connect (legacy_controller, "event",
                          G_CALLBACK (neomacs_legacy_event_cb), f);
        gtk_widget_add_controller (drawing_area, legacy_controller);

        GtkEventController *window_legacy_controller = gtk_event_controller_legacy_new ();
        g_signal_connect (window_legacy_controller, "event",
                          G_CALLBACK (neomacs_legacy_event_cb), f);
        gtk_widget_add_controller (window, window_legacy_controller);
      }

      /* Add motion controller for mouse tracking (non-button motion) */
      motion_controller = gtk_event_controller_motion_new ();
      g_signal_connect (motion_controller, "motion",
                        G_CALLBACK (neomacs_motion_cb), f);
      gtk_widget_add_controller (drawing_area, motion_controller);

      /* Add scroll controller for mouse wheel */
      scroll_controller = gtk_event_controller_scroll_new (GTK_EVENT_CONTROLLER_SCROLL_BOTH_AXES);
      g_signal_connect (scroll_controller, "scroll",
                        G_CALLBACK (neomacs_scroll_cb), f);
      gtk_widget_add_controller (drawing_area, scroll_controller);

      /* Connect realize callback to grab focus when widget is ready */
      g_signal_connect (drawing_area, "realize",
                        G_CALLBACK (neomacs_realize_cb), f);

      /* Set up widget hierarchy */
      gtk_window_set_child (GTK_WINDOW (window), drawing_area);

      /* Store in output structure */
      output->widget = window;
      output->drawing_area = drawing_area;
      output->window_desc = (Window) (intptr_t) window;
      output->use_gpu_widget = use_gpu_widget;

      /* Create initial Cairo surface (only needed for non-GPU mode) */
      if (!use_gpu_widget)
        neomacs_ensure_cr_surface (f, FRAME_PIXEL_WIDTH (f), FRAME_PIXEL_HEIGHT (f));

      /* Show the window and grab focus */
      nlog_debug ("Calling gtk_window_present");
      gtk_window_present (GTK_WINDOW (window));

      /* Force window to be realized and shown */
      gtk_widget_set_visible (window, TRUE);
      gtk_widget_realize (window);

      /* Initialize Rust renderer's Pango context now that widget is realized */
      {
        struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
        if (dpyinfo && dpyinfo->display_handle)
          {
            PangoContext *pango_ctx = gtk_widget_get_pango_context (drawing_area);
            if (pango_ctx)
              neomacs_display_init_pango (dpyinfo->display_handle, pango_ctx);

            /* For GPU widget, also initialize with widget reference for video callbacks */
            if (use_gpu_widget)
              neomacs_display_widget_init_pango (dpyinfo->display_handle, drawing_area);
          }
      }

      /* Process events to ensure window is mapped */
      while (g_main_context_iteration (NULL, FALSE))
        ;

      /* Ensure the drawing area gets keyboard focus */
      gtk_widget_grab_focus (drawing_area);

      /* Also set as the default focus widget */
      gtk_window_set_focus (GTK_WINDOW (window), drawing_area);
    }
}


/* ============================================================================
 * Frame Creation
 * ============================================================================ */

DEFUN ("x-create-frame", Fx_create_frame, Sx_create_frame, 1, 1, 0,
       doc: /* Create a new Neomacs frame.
PARMS is an alist of frame parameters.
If the parameters specify a display, that display is used.  */)
  (Lisp_Object parms)
{
  struct frame *f;
  Lisp_Object frame, tem;
  Lisp_Object name;
  bool minibuffer_only = false;
  specpdl_ref count = SPECPDL_INDEX ();
  struct neomacs_display_info *dpyinfo = NULL;
  struct kboard *kb;

  parms = Fcopy_alist (parms);

  /* Get display info */
  tem = gui_display_get_arg (dpyinfo, parms, Qterminal, 0, 0,
                             RES_TYPE_NUMBER);
  if (BASE_EQ (tem, Qunbound))
    tem = gui_display_get_arg (dpyinfo, parms, Qdisplay, 0, 0,
                               RES_TYPE_STRING);
  dpyinfo = check_neomacs_display_info (tem);
  kb = dpyinfo->terminal->kboard;

  /* Get frame name */
  name = gui_display_get_arg (dpyinfo, parms, Qname, "name", "Name",
                              RES_TYPE_STRING);
  if (!STRINGP (name) && !BASE_EQ (name, Qunbound) && !NILP (name))
    error ("Invalid frame name--not a string or nil");

  /* Check minibuffer parameter */
  tem = gui_display_get_arg (dpyinfo, parms, Qminibuffer, "minibuffer",
                             "Minibuffer", RES_TYPE_SYMBOL);
  if (EQ (tem, Qnone) || NILP (tem))
    f = make_frame_without_minibuffer (Qnil, kb, Qnil);
  else if (EQ (tem, Qonly))
    {
      f = make_minibuffer_frame ();
      minibuffer_only = true;
    }
  else if (WINDOWP (tem))
    f = make_frame_without_minibuffer (tem, kb, Qnil);
  else
    f = make_frame (true);

  XSETFRAME (frame, f);

  /* Set frame type */
  f->terminal = dpyinfo->terminal;
  f->output_method = output_neomacs;
  f->output_data.neomacs = xzalloc (sizeof (struct neomacs_output));
  FRAME_NEOMACS_OUTPUT (f)->display_info = dpyinfo;
  dpyinfo->reference_count++;

  /* Initialize frame pixels - standard Emacs: black text on white background */
  /* This is critical because face realization uses FRAME_FOREGROUND_PIXEL */
  f->foreground_pixel = 0x000000;  /* black text */
  f->background_pixel = 0xffffff;  /* white background */
  FRAME_NEOMACS_OUTPUT (f)->foreground_pixel = 0x000000;  /* black text */
  FRAME_NEOMACS_OUTPUT (f)->background_pixel = 0xffffff;  /* white background */

  /* Initialize frame dimensions */
  FRAME_FONTSET (f) = -1;
  f->border_width = 0;
  f->internal_border_width = 0;

  /* Register font drivers for this frame */
  register_font_driver (&ftcrfont_driver, f);
#ifdef HAVE_HARFBUZZ
  register_font_driver (&ftcrhbfont_driver, f);
#endif

  /* Enable font backends - this must be done before opening fonts */
  gui_default_parameter (f, parms, Qfont_backend, Qnil,
			 "fontBackend", "FontBackend", RES_TYPE_STRING);

  /* Set default dimensions */
  int width = 80;
  int height = 36;
  tem = gui_display_get_arg (dpyinfo, parms, Qwidth, "width", "Width",
                             RES_TYPE_NUMBER);
  if (!BASE_EQ (tem, Qunbound))
    width = XFIXNUM (tem);
  tem = gui_display_get_arg (dpyinfo, parms, Qheight, "height", "Height",
                             RES_TYPE_NUMBER);
  if (!BASE_EQ (tem, Qunbound))
    height = XFIXNUM (tem);

  /* Set up default font */
  FRAME_NEOMACS_OUTPUT (f)->fontset = -1;

  /* Calculate pixel dimensions (estimate until we have real font) */
  int char_width = 8;
  int char_height = 16;
  f->text_cols = width;
  f->text_lines = height;
  FRAME_PIXEL_WIDTH (f) = width * char_width;
  FRAME_PIXEL_HEIGHT (f) = height * char_height;

  /* Set frame name */
  if (STRINGP (name))
    Fmodify_frame_parameters (frame,
                              list1 (Fcons (Qname, name)));

  /* Set up font - required before face realization */
  neomacs_default_font_parameter (f, parms);

  /* Border and divider parameters */
  gui_default_parameter (f, parms, Qborder_width, make_fixnum (0),
			 "borderWidth", "BorderWidth", RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qinternal_border_width, make_fixnum (0),
			 "internalBorderWidth", "internalBorderWidth",
			 RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qchild_frame_border_width, Qnil,
			 "childFrameBorderWidth", "childFrameBorderWidth",
			 RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qright_divider_width, make_fixnum (0),
			 NULL, NULL, RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qbottom_divider_width, make_fixnum (0),
			 NULL, NULL, RES_TYPE_NUMBER);

  /* Scroll bar parameters */
  gui_default_parameter (f, parms, Qvertical_scroll_bars, Qright,
			 "verticalScrollBars", "ScrollBars", RES_TYPE_SYMBOL);
  gui_default_parameter (f, parms, Qhorizontal_scroll_bars, Qnil,
			 "horizontalScrollBars", "ScrollBars",
			 RES_TYPE_SYMBOL);

  /* Color parameters - REQUIRED for face realization */
  gui_default_parameter (f, parms, Qforeground_color, build_string ("black"),
			 "foreground", "Foreground", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qbackground_color, build_string ("white"),
			 "background", "Background", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qmouse_color, build_string ("black"),
			 "pointerColor", "Foreground", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qborder_color, build_string ("black"),
			 "borderColor", "BorderColor", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qscreen_gamma, Qnil,
			 "screenGamma", "ScreenGamma", RES_TYPE_FLOAT);
  gui_default_parameter (f, parms, Qline_spacing, Qnil,
			 "lineSpacing", "LineSpacing", RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qleft_fringe, Qnil,
			 "leftFringe", "LeftFringe", RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qright_fringe, Qnil,
			 "rightFringe", "RightFringe", RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qno_special_glyphs, Qnil,
			 NULL, NULL, RES_TYPE_BOOLEAN);
  gui_default_parameter (f, parms, Qscroll_bar_foreground, Qnil,
			 "scrollBarForeground", "ScrollBarForeground",
			 RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qscroll_bar_background, Qnil,
			 "scrollBarBackground", "ScrollBarBackground",
			 RES_TYPE_STRING);

  /* Initialize faces - MUST be done before adjust_frame_size */
  init_frame_faces (f);

  /* Process min-width and min-height parameters before adjust_frame_size */
  tem = gui_display_get_arg (dpyinfo, parms, Qmin_width, NULL, NULL,
			     RES_TYPE_NUMBER);
  if (FIXNUMP (tem))
    store_frame_param (f, Qmin_width, tem);
  tem = gui_display_get_arg (dpyinfo, parms, Qmin_height, NULL, NULL,
			     RES_TYPE_NUMBER);
  if (FIXNUMP (tem))
    store_frame_param (f, Qmin_height, tem);

  /* Call adjust_frame_size - this initializes glyph matrices.
     CRITICAL: This sets glyphs_initialized_p which is required for redisplay!
     Use the display dimensions (dpyinfo->width/height) as the target size
     so the frame matches the actual window created by the render thread.
     Note: adjust_frame_size expects TEXT pixel dimensions, so we need to
     convert from native (window) pixel dimensions. */
  {
    int native_width = dpyinfo->width;
    int native_height = dpyinfo->height;
    /* Convert native pixels to text pixels by subtracting decorations */
    int text_width = FRAME_PIXEL_TO_TEXT_WIDTH (f, native_width);
    int text_height = FRAME_PIXEL_TO_TEXT_HEIGHT (f, native_height);
    adjust_frame_size (f, text_width, text_height, 5, true,
		       Qx_create_frame_1);
  }

  /* Menu, tab, tool bar lines */
  gui_default_parameter (f, parms, Qmenu_bar_lines,
			 NILP (Vmenu_bar_mode)
			 ? make_fixnum (0) : make_fixnum (1),
			 NULL, NULL, RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qtab_bar_lines,
			 NILP (Vtab_bar_mode)
			 ? make_fixnum (0) : make_fixnum (1),
			 NULL, NULL, RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qtool_bar_lines,
			 NILP (Vtool_bar_mode)
			 ? make_fixnum (0) : make_fixnum (1),
			 NULL, NULL, RES_TYPE_NUMBER);

  gui_default_parameter (f, parms, Qbuffer_predicate, Qnil,
			 "bufferPredicate", "BufferPredicate",
			 RES_TYPE_SYMBOL);
  gui_default_parameter (f, parms, Qtitle, Qnil,
			 "title", "Title", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qinhibit_double_buffering, Qnil,
			 "inhibitDoubleBuffering", "InhibitDoubleBuffering",
			 RES_TYPE_BOOLEAN);

  /* Initialize cursor */
  FRAME_NEOMACS_OUTPUT (f)->cursor_pixel = dpyinfo->black_pixel;
  FRAME_NEOMACS_OUTPUT (f)->cursor_foreground_pixel = dpyinfo->white_pixel;

  /* Initialize mouse pointer cursor shapes.
     Values are integer constants matching Rust CursorIcon mapping:
     1=default/arrow, 2=text/ibeam, 3=hand/pointer,
     4=crosshair, 5=h-resize, 6=v-resize, 7=hourglass */
  FRAME_NEOMACS_OUTPUT (f)->text_cursor = (Emacs_Cursor) 2;
  FRAME_NEOMACS_OUTPUT (f)->nontext_cursor = (Emacs_Cursor) 1;
  FRAME_NEOMACS_OUTPUT (f)->hand_cursor = (Emacs_Cursor) 3;
  FRAME_NEOMACS_OUTPUT (f)->modeline_cursor = (Emacs_Cursor) 6;
  FRAME_NEOMACS_OUTPUT (f)->hourglass_cursor = (Emacs_Cursor) 7;
  FRAME_NEOMACS_OUTPUT (f)->horizontal_drag_cursor = (Emacs_Cursor) 5;
  FRAME_NEOMACS_OUTPUT (f)->vertical_drag_cursor = (Emacs_Cursor) 6;

  /* Now consider frame official.  */
  f->terminal->reference_count++;
  Vframe_list = Fcons (frame, Vframe_list);

  /* Create GTK4 widgets */
  block_input ();
  neomacs_create_frame_widgets (f);
  unblock_input ();

  /* Post-widget parameters */
  gui_default_parameter (f, parms, Qicon_type, Qt,
			 "bitmapIcon", "BitmapIcon", RES_TYPE_BOOLEAN);
  gui_default_parameter (f, parms, Qauto_raise, Qnil,
			 "autoRaise", "AutoRaiseLower", RES_TYPE_BOOLEAN);
  gui_default_parameter (f, parms, Qauto_lower, Qnil,
			 "autoLower", "AutoRaiseLower", RES_TYPE_BOOLEAN);
  gui_default_parameter (f, parms, Qcursor_type, Qbox,
			 "cursorType", "CursorType", RES_TYPE_SYMBOL);
  gui_default_parameter (f, parms, Qscroll_bar_width, Qnil,
			 "scrollBarWidth", "ScrollBarWidth",
			 RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qscroll_bar_height, Qnil,
			 "scrollBarHeight", "ScrollBarHeight",
			 RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qalpha, Qnil,
			 "alpha", "Alpha", RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qalpha_background, Qnil,
			 "alphaBackground", "AlphaBackground",
			 RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qno_focus_on_map, Qnil,
			 NULL, NULL, RES_TYPE_BOOLEAN);
  gui_default_parameter (f, parms, Qno_accept_focus, Qnil,
			 NULL, NULL, RES_TYPE_BOOLEAN);

  /* Allow frame to be resized now.  */
  f->can_set_window_size = true;

  /* Final frame size adjustment */
  adjust_frame_size (f, FRAME_TEXT_WIDTH (f), FRAME_TEXT_HEIGHT (f),
		     0, true, Qx_create_frame_2);

  /* Process fullscreen parameter */
  gui_default_parameter (f, parms, Qfullscreen, Qnil,
			 "fullscreen", "Fullscreen", RES_TYPE_SYMBOL);

  /* Make the frame visible */
  gui_default_parameter (f, parms, Qvisibility, Qt,
			 "visibility", "Visibility", RES_TYPE_SYMBOL);

  return unbind_to (count, frame);
}


/* ============================================================================
 * Display Functions
 * ============================================================================ */

DEFUN ("x-display-pixel-width", Fx_display_pixel_width,
       Sx_display_pixel_width, 0, 1, 0,
       doc: /* Return width in pixels of the Neomacs display.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo = check_neomacs_display_info (terminal);
  return make_fixnum (dpyinfo->width);
}

DEFUN ("x-display-pixel-height", Fx_display_pixel_height,
       Sx_display_pixel_height, 0, 1, 0,
       doc: /* Return height in pixels of the Neomacs display.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo = check_neomacs_display_info (terminal);
  return make_fixnum (dpyinfo->height);
}

DEFUN ("x-display-planes", Fx_display_planes, Sx_display_planes, 0, 1, 0,
       doc: /* Return the number of bitplanes of the Neomacs display.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo = check_neomacs_display_info (terminal);
  return make_fixnum (dpyinfo->n_planes);
}

DEFUN ("x-display-color-cells", Fx_display_color_cells,
       Sx_display_color_cells, 0, 1, 0,
       doc: /* Return number of color cells of the Neomacs display.  */)
  (Lisp_Object terminal)
{
  /* 24-bit color = 16 million colors */
  return make_fixnum (16777216);
}

DEFUN ("x-display-visual-class", Fx_display_visual_class,
       Sx_display_visual_class, 0, 1, 0,
       doc: /* Return the visual class of the Neomacs display.  */)
  (Lisp_Object terminal)
{
  return intern ("true-color");
}

DEFUN ("xw-color-values", Fxw_color_values, Sxw_color_values, 1, 2, 0,
       doc: /* Internal function called by `color-values'.
Return a list of (RED GREEN BLUE) for COLOR on FRAME.
Each value is in the range 0 to 65535 inclusive.  */)
  (Lisp_Object color, Lisp_Object frame)
{
  Emacs_Color col;
  struct frame *f = decode_window_system_frame (frame);

  CHECK_STRING (color);

  if (neomacs_defined_color (f, SSDATA (color), &col, false, false))
    return list3i (col.red, col.green, col.blue);
  else
    return Qnil;
}

DEFUN ("xw-color-defined-p", Fxw_color_defined_p, Sxw_color_defined_p, 1, 2, 0,
       doc: /* Internal function called by `color-defined-p'.
Return non-nil if COLOR is supported on FRAME.  */)
  (Lisp_Object color, Lisp_Object frame)
{
  Emacs_Color col;
  struct frame *f = decode_window_system_frame (frame);

  CHECK_STRING (color);

  if (neomacs_defined_color (f, SSDATA (color), &col, false, false))
    return Qt;
  else
    return Qnil;
}

DEFUN ("x-display-screens", Fx_display_screens, Sx_display_screens, 0, 1, 0,
       doc: /* Return the number of screens on the Neomacs display.  */)
  (Lisp_Object terminal)
{
  check_neomacs_display_info (terminal);
  return make_fixnum (1);
}

DEFUN ("x-display-mm-height", Fx_display_mm_height,
       Sx_display_mm_height, 0, 1, 0,
       doc: /* Return the height of the Neomacs display in millimeters.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo
    = check_neomacs_display_info (terminal);
  int height_mm = 0;

  block_input ();
  GdkDisplay *gdpy = dpyinfo->gdpy;
  if (gdpy)
    {
      GListModel *monitors = gdk_display_get_monitors (gdpy);
      guint n = g_list_model_get_n_items (monitors);
      for (guint i = 0; i < n; i++)
        {
          GdkMonitor *monitor = GDK_MONITOR (g_list_model_get_item (monitors, i));
          int mm = gdk_monitor_get_height_mm (monitor);
          if (mm > height_mm)
            height_mm = mm;
          g_object_unref (monitor);
        }
    }
  unblock_input ();

  /* Fallback: estimate from pixel height assuming ~96 DPI */
  if (height_mm == 0 && dpyinfo->height > 0)
    height_mm = (int) (dpyinfo->height * 25.4 / 96.0);

  return make_fixnum (height_mm);
}

DEFUN ("x-display-mm-width", Fx_display_mm_width,
       Sx_display_mm_width, 0, 1, 0,
       doc: /* Return the width of the Neomacs display in millimeters.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo
    = check_neomacs_display_info (terminal);
  int width_mm = 0;

  block_input ();
  GdkDisplay *gdpy = dpyinfo->gdpy;
  if (gdpy)
    {
      GListModel *monitors = gdk_display_get_monitors (gdpy);
      guint n = g_list_model_get_n_items (monitors);
      for (guint i = 0; i < n; i++)
        {
          GdkMonitor *monitor = GDK_MONITOR (g_list_model_get_item (monitors, i));
          int mm = gdk_monitor_get_width_mm (monitor);
          if (mm > width_mm)
            width_mm = mm;
          g_object_unref (monitor);
        }
    }
  unblock_input ();

  /* Fallback: estimate from pixel width assuming ~96 DPI */
  if (width_mm == 0 && dpyinfo->width > 0)
    width_mm = (int) (dpyinfo->width * 25.4 / 96.0);

  return make_fixnum (width_mm);
}

DEFUN ("x-server-max-request-size", Fx_server_max_request_size,
       Sx_server_max_request_size, 0, 1, 0,
       doc: /* Return the maximum request size for the Neomacs display.
Not applicable to GPU-rendered displays; returns nil.  */)
  (Lisp_Object terminal)
{
  check_neomacs_display_info (terminal);
  return Qnil;
}

DEFUN ("x-display-backing-store", Fx_display_backing_store,
       Sx_display_backing_store, 0, 1, 0,
       doc: /* Return the backing store capability of the Neomacs display.
Not applicable to GPU-rendered displays; returns nil.  */)
  (Lisp_Object terminal)
{
  check_neomacs_display_info (terminal);
  return Qnil;
}

DEFUN ("x-display-save-under", Fx_display_save_under,
       Sx_display_save_under, 0, 1, 0,
       doc: /* Return the save-under capability of the Neomacs display.
Not applicable to GPU-rendered displays; returns nil.  */)
  (Lisp_Object terminal)
{
  check_neomacs_display_info (terminal);
  return Qnil;
}

DEFUN ("x-show-tip", Fx_show_tip, Sx_show_tip, 1, 6, 0,
       doc: /* SKIP: real doc in xfns.c.  */)
  (Lisp_Object string, Lisp_Object frame, Lisp_Object parms,
   Lisp_Object timeout, Lisp_Object dx, Lisp_Object dy)
{
  specpdl_ref count = SPECPDL_INDEX ();
  specbind (Qinhibit_redisplay, Qt);

  CHECK_STRING (string);
  if (SCHARS (string) == 0)
    string = make_unibyte_string (" ", 1);

  if (NILP (frame))
    frame = selected_frame;
  decode_window_system_frame (frame);

  if (NILP (timeout))
    timeout = Vx_show_tooltip_timeout;
  CHECK_FIXNAT (timeout);

  if (NILP (dx))
    dx = make_fixnum (5);
  else
    CHECK_FIXNUM (dx);

  if (NILP (dy))
    dy = make_fixnum (-10);
  else
    CHECK_FIXNUM (dy);

  /* Hide any existing tooltip first.  */
  Fx_hide_tip ();

  tip_last_frame = frame;
  tip_last_string = string;
  tip_last_parms = parms;

  /* Display tooltip text via echo area message.
     This is a temporary implementation until GPU overlay tooltips
     are added.  */
  message_with_string ("%s", string, false);
  tip_showing = true;

  /* Let the tip disappear after timeout seconds.  */
  tip_timer = calln (intern ("run-at-time"), timeout, Qnil,
                     intern ("x-hide-tip"));

  return unbind_to (count, Qnil);
}

DEFUN ("x-hide-tip", Fx_hide_tip, Sx_hide_tip, 0, 0, 0,
       doc: /* Hide the current tooltip window, if there is any.
Value is t if tooltip was open, nil otherwise.  */)
  (void)
{
  Lisp_Object was_open = Qnil;

  if (!NILP (tip_timer))
    {
      calln (intern ("cancel-timer"), tip_timer);
      tip_timer = Qnil;
    }

  if (tip_showing)
    {
      /* Clear echo area.  */
      message1 (NULL);
      tip_showing = false;
      was_open = Qt;
    }

  tip_last_frame = Qnil;

  return was_open;
}

/* ============================================================================
 * Frame Geometry Functions
 * ============================================================================ */

static Lisp_Object
neomacs_frame_geometry (Lisp_Object frame, Lisp_Object attribute)
{
  struct frame *f = decode_live_frame (frame);
  int border = f->border_width;
  int native_width = FRAME_PIXEL_WIDTH (f);
  int native_height = FRAME_PIXEL_HEIGHT (f);
  int outer_width = native_width + 2 * border;
  int outer_height = native_height + 2 * border;
  int left_pos = f->left_pos;
  int top_pos = f->top_pos;
  int native_left = left_pos + border;
  int native_top = top_pos + border;
  int native_right = left_pos + outer_width - border;
  int native_bottom = top_pos + outer_height - border;
  int internal_border_width = FRAME_INTERNAL_BORDER_WIDTH (f);
  int tab_bar_height = FRAME_TAB_BAR_HEIGHT (f);
  int tab_bar_width = (tab_bar_height
                       ? native_width - 2 * internal_border_width : 0);
  int tool_bar_height = FRAME_TOOL_BAR_HEIGHT (f);
  int tool_bar_width = (tool_bar_height
                        ? outer_width - 2 * internal_border_width : 0);

  if (EQ (attribute, Qouter_edges))
    return list4 (make_fixnum (left_pos), make_fixnum (top_pos),
                  make_fixnum (left_pos + outer_width),
                  make_fixnum (top_pos + outer_height));
  else if (EQ (attribute, Qnative_edges))
    return list4 (make_fixnum (native_left), make_fixnum (native_top),
                  make_fixnum (native_right), make_fixnum (native_bottom));
  else if (EQ (attribute, Qinner_edges))
    return list4 (make_fixnum (native_left + internal_border_width),
                  make_fixnum (native_top
                               + tab_bar_height
                               + FRAME_TOOL_BAR_TOP_HEIGHT (f)
                               + internal_border_width),
                  make_fixnum (native_right - internal_border_width),
                  make_fixnum (native_bottom - internal_border_width
                               - FRAME_TOOL_BAR_BOTTOM_HEIGHT (f)));
  else
    return
      list (Fcons (Qouter_position,
                   Fcons (make_fixnum (left_pos),
                          make_fixnum (top_pos))),
            Fcons (Qouter_size,
                   Fcons (make_fixnum (outer_width),
                          make_fixnum (outer_height))),
            Fcons (Qexternal_border_size,
                   Fcons (make_fixnum (border), make_fixnum (border))),
            Fcons (Qtitle_bar_size,
                   Fcons (make_fixnum (0), make_fixnum (0))),
            Fcons (Qmenu_bar_external, Qnil),
            Fcons (Qmenu_bar_size,
                   Fcons (make_fixnum (0), make_fixnum (0))),
            Fcons (Qtab_bar_size,
                   Fcons (make_fixnum (tab_bar_width),
                          make_fixnum (tab_bar_height))),
            Fcons (Qtool_bar_external,
                   FRAME_EXTERNAL_TOOL_BAR (f) ? Qt : Qnil),
            Fcons (Qtool_bar_position, FRAME_TOOL_BAR_POSITION (f)),
            Fcons (Qtool_bar_size,
                   Fcons (make_fixnum (tool_bar_width),
                          make_fixnum (tool_bar_height))),
            Fcons (Qinternal_border_width,
                   make_fixnum (internal_border_width)));
}

DEFUN ("neomacs-frame-geometry", Fneomacs_frame_geometry,
       Sneomacs_frame_geometry, 0, 1, 0,
       doc: /* Return geometric attributes of FRAME.
FRAME must be a live frame and defaults to the selected one.
See `pgtk-frame-geometry' for the list of returned attributes.  */)
  (Lisp_Object frame)
{
  return neomacs_frame_geometry (frame, Qnil);
}

DEFUN ("neomacs-frame-edges", Fneomacs_frame_edges,
       Sneomacs_frame_edges, 0, 2, 0,
       doc: /* Return edge coordinates of FRAME.
FRAME must be a live frame and defaults to the selected one.
TYPE can be `outer-edges', `native-edges', or `inner-edges'.  */)
  (Lisp_Object frame, Lisp_Object type)
{
  return neomacs_frame_geometry (frame, ((EQ (type, Qouter_edges)
                                          || EQ (type, Qinner_edges))
                                         ? type : Qnative_edges));
}

DEFUN ("neomacs-mouse-absolute-pixel-position",
       Fneomacs_mouse_absolute_pixel_position,
       Sneomacs_mouse_absolute_pixel_position, 0, 0, 0,
       doc: /* Return absolute position of mouse cursor in pixels.
The position is returned as a cons cell (X . Y).  */)
  (void)
{
  struct frame *f = SELECTED_FRAME ();
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (!dpyinfo)
    return Fcons (make_fixnum (0), make_fixnum (0));

  return Fcons (make_fixnum (dpyinfo->last_mouse_motion_x),
                make_fixnum (dpyinfo->last_mouse_motion_y));
}

DEFUN ("x-open-connection", Fx_open_connection, Sx_open_connection, 1, 3, 0,
       doc: /* Open a connection to a Neomacs display.
DISPLAY is the name of the display.  Optional second arg
XRM-STRING is a string of resources.  Optional third arg MUST-SUCCEED
is ignored.  */)
  (Lisp_Object display, Lisp_Object xrm_string, Lisp_Object must_succeed)
{
  struct neomacs_display_info *dpyinfo;
  Lisp_Object display_name = Qnil;

  if (!NILP (display))
    CHECK_STRING (display);
  else
    display = build_string (":0");

  block_input ();
  dpyinfo = neomacs_open_display (SSDATA (display));
  unblock_input ();

  if (!dpyinfo)
    {
      if (!NILP (must_succeed))
        error ("Cannot open Neomacs display");
      return Qnil;
    }

  /* Set up name_list_element for x-display-list */
  dpyinfo->name_list_element = Fcons (display, Qnil);

  /* Create terminal */
  struct terminal *terminal = neomacs_create_terminal (dpyinfo);
  if (!terminal)
    {
      error ("Cannot create Neomacs terminal");
      return Qnil;
    }

  return Qnil;
}

DEFUN ("x-close-connection", Fx_close_connection, Sx_close_connection, 1, 1, 0,
       doc: /* Close the connection to the Neomacs display.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo = check_neomacs_display_info (terminal);

  if (dpyinfo->reference_count > 0)
    error ("Display still has frames");

  neomacs_delete_terminal (dpyinfo->terminal);
  return Qnil;
}


/* ============================================================================
 * Frame Functions
 * ============================================================================ */

DEFUN ("x-display-list", Fx_display_list, Sx_display_list, 0, 0, 0,
       doc: /* Return the list of Neomacs displays.  */)
  (void)
{
  Lisp_Object result = Qnil;
  struct neomacs_display_info *dpyinfo;

  for (dpyinfo = neomacs_display_list; dpyinfo; dpyinfo = dpyinfo->next)
    if (dpyinfo->name_list_element)
      result = Fcons (XCAR (dpyinfo->name_list_element), result);

  return result;
}


/* ============================================================================
 * Set Frame Title
 * ============================================================================ */

/* Set up the default font for frame F from parameters PARMS.
   Called during frame creation and when font-backend changes.  */
void
neomacs_default_font_parameter (struct frame *f, Lisp_Object parms)
{
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  Lisp_Object font_param
    = gui_display_get_arg (dpyinfo, parms, Qfont, NULL, NULL,
                           RES_TYPE_STRING);
  Lisp_Object font = Qnil;
  if (BASE_EQ (font_param, Qunbound))
    font_param = Qnil;

  if (NILP (font))
    font = !NILP (font_param) ? font_param
      : gui_display_get_arg (dpyinfo, parms, Qfont, "font", "Font",
                             RES_TYPE_STRING);

  if (!FONTP (font) && !STRINGP (font))
    {
      const char *names[] = {
        "monospace-10",
        "Monospace-10",
        "-misc-fixed-medium-r-normal-*-*-140-*-*-c-*-iso8859-1",
        "fixed",
        NULL
      };
      for (int i = 0; names[i]; i++)
        {
          font = font_open_by_name (f, build_unibyte_string (names[i]));
          if (!NILP (font))
            break;
        }
      if (NILP (font))
        error ("No suitable font was found");
    }
  else if (STRINGP (font))
    {
      Lisp_Object opened = font_open_by_name (f, font);
      if (!NILP (opened))
        font = opened;
    }

  gui_default_parameter (f, parms, Qfont, font, "font", "Font",
                         RES_TYPE_STRING);
}


/* ============================================================================
 * Scroll Bar Functions
 * ============================================================================ */

DEFUN ("x-scroll-bar-foreground", Fx_scroll_bar_foreground,
       Sx_scroll_bar_foreground, 1, 1, 0,
       doc: /* Return the foreground color of scroll bars on FRAME.  */)
  (Lisp_Object frame)
{
  decode_window_system_frame (frame);
  return Fframe_parameter (frame, Qscroll_bar_foreground);
}

DEFUN ("x-scroll-bar-background", Fx_scroll_bar_background,
       Sx_scroll_bar_background, 1, 1, 0,
       doc: /* Return the background color of scroll bars on FRAME.  */)
  (Lisp_Object frame)
{
  decode_window_system_frame (frame);
  return Fframe_parameter (frame, Qscroll_bar_background);
}


/* ============================================================================
 * Clipboard Functions
 * ============================================================================ */

DEFUN ("neomacs-clipboard-set", Fneomacs_clipboard_set,
       Sneomacs_clipboard_set, 1, 1, 0,
       doc: /* Set the system clipboard to TEXT.
Returns t on success, nil on failure.  */)
  (Lisp_Object text)
{
  CHECK_STRING (text);
  const char *str = SSDATA (text);
  int result = neomacs_clipboard_set_text (str);
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-clipboard-get", Fneomacs_clipboard_get,
       Sneomacs_clipboard_get, 0, 0, 0,
       doc: /* Get text from the system clipboard.
Returns a string, or nil if the clipboard is empty or an error occurred.  */)
  (void)
{
  char *text = neomacs_clipboard_get_text ();
  if (!text)
    return Qnil;
  Lisp_Object result = build_string (text);
  neomacs_clipboard_free_text (text);
  return result;
}

/* ============================================================================
 * Initialization
 * ============================================================================ */

void
syms_of_neomacsfns (void)
{
  /* Frame creation */
  defsubr (&Sx_create_frame);

  /* Display functions */
  defsubr (&Sx_display_pixel_width);
  defsubr (&Sx_display_pixel_height);
  defsubr (&Sx_display_planes);
  defsubr (&Sx_display_color_cells);
  defsubr (&Sx_display_visual_class);
  defsubr (&Sxw_color_values);
  defsubr (&Sx_display_list);

  /* Color functions */
  defsubr (&Sxw_color_defined_p);

  /* Additional display functions */
  defsubr (&Sx_display_screens);
  defsubr (&Sx_display_mm_height);
  defsubr (&Sx_display_mm_width);
  defsubr (&Sx_server_max_request_size);
  defsubr (&Sx_display_backing_store);
  defsubr (&Sx_display_save_under);

  /* Tooltip functions */
  defsubr (&Sx_show_tip);
  defsubr (&Sx_hide_tip);

  /* Frame geometry functions */
  defsubr (&Sneomacs_frame_geometry);
  defsubr (&Sneomacs_frame_edges);
  defsubr (&Sneomacs_mouse_absolute_pixel_position);

  /* Connection functions */
  defsubr (&Sx_open_connection);
  defsubr (&Sx_close_connection);

  /* Scroll bar functions */
  defsubr (&Sx_scroll_bar_foreground);
  defsubr (&Sx_scroll_bar_background);

  /* Clipboard functions */
  defsubr (&Sneomacs_clipboard_set);
  defsubr (&Sneomacs_clipboard_get);

  /* Tooltip state GC roots */
  staticpro (&tip_timer);
  tip_timer = Qnil;
  staticpro (&tip_last_frame);
  tip_last_frame = Qnil;
  staticpro (&tip_last_string);
  tip_last_string = Qnil;
  staticpro (&tip_last_parms);
  tip_last_parms = Qnil;

  /* Symbols */
  DEFSYM (Qdisplay, "display");
  DEFSYM (Qname, "name");
  DEFSYM (Qminibuffer, "minibuffer");
  DEFSYM (Qterminal, "terminal");
  DEFSYM (Qwidth, "width");
  DEFSYM (Qheight, "height");
  DEFSYM (Qnone, "none");
  DEFSYM (Qonly, "only");
}

#endif /* HAVE_NEOMACS */
