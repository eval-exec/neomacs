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

/* Forward declarations */
static void neomacs_set_title (struct frame *f);
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
      FRAME_NEOMACS_OUTPUT (f)->foreground_pixel = fg;
      update_face_from_frame_parameter (f, Qforeground_color, arg);
    }
}

static void
neomacs_set_background_color (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  unsigned long bg;

  if (STRINGP (arg))
    {
      bg = x_decode_color (f, arg, WHITE_PIX_DEFAULT (f));
      FRAME_NEOMACS_OUTPUT (f)->background_pixel = bg;
      update_face_from_frame_parameter (f, Qbackground_color, arg);
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
neomacs_set_internal_border_width (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  /* Internal border width - not implemented yet */
}

static void
neomacs_set_child_frame_border_width (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  /* Child frame border width - not implemented yet */
}

static void
neomacs_explicitly_set_name (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  neomacs_set_title (f);
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

static void
neomacs_set_scroll_bar_foreground (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Scroll bar foreground - not implemented yet */
}

static void
neomacs_set_scroll_bar_background (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Scroll bar background - not implemented yet */
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
  /* Undecorated - not implemented yet */
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
  /* No focus on map - not implemented yet */
}

static void
neomacs_set_no_accept_focus (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* No accept focus - not implemented yet */
}

static void
neomacs_set_z_group (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Z group - not implemented yet */
}

static void
neomacs_set_override_redirect (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Override redirect - not implemented yet */
}

static void
neomacs_set_alpha_background (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Alpha background - not implemented yet */
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
    NULL,				/* title - set via set_name */
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
  if (0) fprintf (stderr, "DEBUG: IM commit: '%s'\n", str);
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

  if (0) fprintf (stderr, "DEBUG: Key pressed: keyval=%u (0x%x), keycode=%u, state=%u\n",
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
      if (0) fprintf (stderr, "DEBUG: Enqueueing ASCII key event: code=%u, modifiers=%d\n", c, inev.ie.modifiers);
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

if (0) fprintf (stderr, "DEBUG: Focus entered frame %p, widget=%p (focusable=%d, has_focus=%d)\n", 
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
if (0) fprintf (stderr, "DEBUG: Set highlight_frame to %p\n", (void *) f);
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

if (0) fprintf (stderr, "DEBUG: Focus left frame %p\n", (void *) f);

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
      button = gdk_button_event_get_button (event);

      /* Deduplicate - multiple controllers may receive same event */
      if (time == mouse_drag_state.last_press_time)
        return FALSE;
      mouse_drag_state.last_press_time = time;

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
  struct input_event ie;
  GdkEvent *event;

  if (!FRAME_LIVE_P (f))
    return FALSE;

  event = gtk_event_controller_get_current_event (GTK_EVENT_CONTROLLER (controller));
  if (!event)
    return FALSE;

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

/* Create GTK4 widgets for a frame */
static void
neomacs_create_frame_widgets (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  GtkWidget *window, *drawing_area;
  GtkEventController *key_controller, *focus_controller, *motion_controller, *scroll_controller;
  int use_gpu_widget = 1;  /* Always use GPU widget when Neomacs is available */

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
          fprintf (stderr, "Failed to create NeomacsWidget, falling back to DrawingArea\n");
          use_gpu_widget = 0;
        }
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
  if (0) fprintf (stderr, "DEBUG: Calling gtk_window_present\n");
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
  {
    Lisp_Object font = gui_display_get_arg (dpyinfo, parms, Qfont, "font", "Font",
					    RES_TYPE_STRING);
    if (!FONTP (font) && !STRINGP (font))
      {
	const char *names[] = {
	  "monospace-10",
	  "Monospace-10",
	  "fixed",
	  NULL
	};
	for (int i = 0; names[i]; i++)
	  {
	    font = font_open_by_name (f, build_unibyte_string (names[i]));
	    if (!NILP (font))
	      break;
	  }
      }
    if (!NILP (font))
      gui_default_parameter (f, parms, Qfont, font, "font", "Font",
			     RES_TYPE_STRING);
  }

  /* Set foreground and background colors - REQUIRED for face realization */
  /* Use black on white (standard Emacs default) */
  gui_default_parameter (f, parms, Qforeground_color, build_string ("black"),
			 "foreground", "Foreground", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qbackground_color, build_string ("white"),
			 "background", "Background", RES_TYPE_STRING);

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
     CRITICAL: This sets glyphs_initialized_p which is required for redisplay! */
  adjust_frame_size (f, FRAME_COLS (f) * FRAME_COLUMN_WIDTH (f),
		     FRAME_LINES (f) * FRAME_LINE_HEIGHT (f), 5, true,
		     Qx_create_frame_1);

  /* Initialize cursor */
  FRAME_NEOMACS_OUTPUT (f)->cursor_pixel = dpyinfo->black_pixel;
  FRAME_NEOMACS_OUTPUT (f)->cursor_foreground_pixel = dpyinfo->white_pixel;

  /* Store in frame list */
  Vframe_list = Fcons (frame, Vframe_list);

  /* Create GTK4 widgets */
  block_input ();
  neomacs_create_frame_widgets (f);
  unblock_input ();

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

static void
neomacs_set_title (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  const char *title;

  if (FRAME_ICONIFIED_P (f))
    return;

  if (!STRINGP (f->title))
    title = "Emacs";
  else
    title = SSDATA (f->title);

  if (output && output->widget)
    {
      block_input ();
      gtk_window_set_title (GTK_WINDOW (output->widget), title);
      unblock_input ();
    }
}


/* ============================================================================
 * Scroll Bar Functions (stubs)
 * ============================================================================ */

DEFUN ("x-scroll-bar-foreground", Fx_scroll_bar_foreground,
       Sx_scroll_bar_foreground, 1, 1, 0,
       doc: /* Return the foreground color of scroll bars on FRAME.  */)
  (Lisp_Object frame)
{
  return Qnil;
}

DEFUN ("x-scroll-bar-background", Fx_scroll_bar_background,
       Sx_scroll_bar_background, 1, 1, 0,
       doc: /* Return the background color of scroll bars on FRAME.  */)
  (Lisp_Object frame)
{
  return Qnil;
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

  /* Connection functions */
  defsubr (&Sx_open_connection);
  defsubr (&Sx_close_connection);

  /* Scroll bar functions */
  defsubr (&Sx_scroll_bar_foreground);
  defsubr (&Sx_scroll_bar_background);

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
