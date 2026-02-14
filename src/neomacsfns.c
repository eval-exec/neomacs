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

#define NLOG_MODULE "frame"
#include "neomacs_log.h"

#include <math.h>

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

/* Return the pixel color value for color COLOR_NAME on frame F.
   If F is a monochrome frame, return MONO_COLOR regardless of what
   ARG says.  Signal an error if color can't be allocated.  */
static int
x_decode_color (struct frame *f, Lisp_Object color_name, int mono_color)
{
  Emacs_Color cdef;

  CHECK_STRING (color_name);

  /* Return MONO_COLOR for monochrome frames.  */
  if (FRAME_DISPLAY_INFO (f)->n_planes == 1)
    return mono_color;

  /* neomacs_defined_color handles CSS color names, #RGB, #RRGGBB,
     rgb(), etc. via parse_color_spec + X11 named color lookup.  */
  if (neomacs_defined_color (f, SSDATA (color_name), &cdef, true, false))
    return cdef.pixel;

  signal_error ("Undefined color", color_name);
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
  unsigned long pixel;

  CHECK_STRING (arg);
  pixel = x_decode_color (f, arg, BLACK_PIX_DEFAULT (f));
  f->output_data.neomacs->border_pixel = pixel;
}

static void
neomacs_set_menu_bar_lines (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  int nlines;
  int olines = FRAME_MENU_BAR_LINES (f);

  if (FRAME_MINIBUF_ONLY_P (f) || FRAME_PARENT_FRAME (f))
    return;

  if (TYPE_RANGED_FIXNUMP (int, value))
    nlines = XFIXNUM (value);
  else
    nlines = 0;

  /* Make sure we redisplay all windows in this frame.  */
  fset_redisplay (f);

  /* Neomacs uses Emacs-internal text-rendered menu bar.  */
  FRAME_MENU_BAR_LINES (f) = nlines;
  FRAME_MENU_BAR_HEIGHT (f) = nlines * FRAME_LINE_HEIGHT (f);

  if (nlines != olines)
    {
      adjust_frame_size (f, -1, -1, 3, true, Qmenu_bar_lines);
      adjust_frame_glyphs (f);
      SET_FRAME_GARBAGED (f);
    }
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

static void neomacs_change_tool_bar_height (struct frame *f, int height);

static void
neomacs_set_tool_bar_lines (struct frame *f, Lisp_Object value, Lisp_Object oldval)
{
  int nlines;

  if (FRAME_MINIBUF_ONLY_P (f))
    return;

  if (RANGED_FIXNUMP (0, value, INT_MAX))
    nlines = XFIXNAT (value);
  else
    nlines = 0;

  /* Neomacs uses Emacs-internal tool bar (text-rendered in frame).  */
  neomacs_change_tool_bar_height (f, nlines * FRAME_LINE_HEIGHT (f));
}

/* Set the pixel height of the tool bar of frame F to HEIGHT.  */
static void
neomacs_change_tool_bar_height (struct frame *f, int height)
{
  int unit = FRAME_LINE_HEIGHT (f);
  int old_height = FRAME_TOOL_BAR_HEIGHT (f);
  int lines = (height + unit - 1) / unit;
  Lisp_Object fullscreen = get_frame_param (f, Qfullscreen);

  /* Make sure we redisplay all windows in this frame.  */
  fset_redisplay (f);

  FRAME_TOOL_BAR_HEIGHT (f) = height;
  FRAME_TOOL_BAR_LINES (f) = lines;
  store_frame_param (f, Qtool_bar_lines, make_fixnum (lines));

  if (FRAME_NEOMACS_WINDOW (f) && FRAME_TOOL_BAR_HEIGHT (f) == 0)
    {
      clear_frame (f);
      clear_current_matrices (f);
    }

  if ((height < old_height) && WINDOWP (f->tool_bar_window))
    clear_glyph_matrix (XWINDOW (f->tool_bar_window)->current_matrix);

  if (!f->tool_bar_resized)
    {
      if (NILP (fullscreen) || EQ (fullscreen, Qfullwidth))
	adjust_frame_size (f, FRAME_TEXT_WIDTH (f), FRAME_TEXT_HEIGHT (f),
			   1, false, Qtool_bar_lines);
      else
	adjust_frame_size (f, -1, -1, 4, false, Qtool_bar_lines);

      f->tool_bar_resized = f->tool_bar_redisplayed;
    }
  else
    adjust_frame_size (f, -1, -1, 3, false, Qtool_bar_lines);

  adjust_frame_glyphs (f);
  SET_FRAME_GARBAGED (f);
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
  if (STRINGP (arg))
    {
      if (STRINGP (oldval) && EQ (Fstring_equal (oldval, arg), Qt))
	return;
    }
  else if (!STRINGP (oldval) && NILP (oldval) == NILP (arg))
    return;

  fset_icon_name (f, arg);
}

static void
neomacs_set_icon_type (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  /* Wayland doesn't use bitmap icons (icons come from desktop files),
     so this is intentionally a no-op.  Just store if provided.  */
  if (STRINGP (arg))
    {
      if (STRINGP (oldval) && EQ (Fstring_equal (oldval, arg), Qt))
	return;
    }
  else if (!STRINGP (oldval) && NILP (oldval) == NILP (arg))
    return;
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
  /* Sticky (show on all workspaces) requires compositor protocol support
     (e.g., wl_surface_set_sticky or _NET_WM_STATE_STICKY on X11).
     Not available through winit currently.  */
}

static void
neomacs_set_tool_bar_position (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  if (!EQ (arg, Qtop) && !EQ (arg, Qbottom))
    error ("Tool bar position must be either `top' or `bottom'");

  if (EQ (arg, oldval))
    return;

  fset_tool_bar_position (f, arg);
  adjust_frame_size (f, -1, -1, 3, false, Qtool_bar_position);
  adjust_frame_glyphs (f);
  SET_FRAME_GARBAGED (f);
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
  struct frame *p = NULL;

  if (!NILP (arg))
    {
      CHECK_FRAME (arg);
      p = XFRAME (arg);

      if (!FRAME_LIVE_P (p))
	{
	  store_frame_param (f, Qparent_frame, oldval);
	  error ("Parent frame is not live");
	}

      if (!FRAME_NEOMACS_P (p))
	{
	  store_frame_param (f, Qparent_frame, oldval);
	  error ("Parent frame is not a neomacs frame");
	}
    }

  if (p != FRAME_PARENT_FRAME (f))
    {
      /* If unparenting (was a child, now becoming root), remove from render. */
      if (FRAME_PARENT_FRAME (f) && !p)
        {
          struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
          if (dpyinfo && dpyinfo->display_handle)
            neomacs_display_remove_child_frame (dpyinfo->display_handle,
                                                (uint64_t)(uintptr_t) f);
        }
      fset_parent_frame (f, arg);
    }
}

static void
neomacs_set_skip_taskbar (struct frame *f, Lisp_Object arg, Lisp_Object oldval)
{
  if (!EQ (arg, oldval))
    {
      FRAME_SKIP_TASKBAR (f) = !NILP (arg);
      /* Actual WM hint would require compositor protocol support.  */
    }
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
  /* Let gui_set_alpha_background handle validation, face recomputation,
     and SET_FRAME_GARBAGED.  */
  gui_set_alpha_background (f, arg, oldval);

  /* Send the computed alpha to the GPU renderer for background transparency.  */
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_set_background_alpha (dpyinfo->display_handle,
					  (float) f->alpha_background);
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
 * Window Management
 * ============================================================================ */

/* Callback for GPU widget resize (called from Rust via FFI) */
static void
neomacs_widget_resize_cb (void *user_data, int width, int height)
{
  struct frame *f = (struct frame *) user_data;

  if (!f || !FRAME_LIVE_P (f) || !FRAME_NEOMACS_P (f))
    return;

  /* Child frames are composited overlays — ignore OS window resize. */
  if (FRAME_PARENT_FRAME (f))
    return;

  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);

  /* Update Rust display engine with new size */
  if (dpyinfo && dpyinfo->display_handle)
    {
      neomacs_display_resize (dpyinfo->display_handle, width, height);
    }

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

/* Create winit window for a frame via Rust backend */
static void
neomacs_create_frame_widgets (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);

  /* Initialize window_id to 0 (invalid) */
  output->window_id = 0;

  if (!dpyinfo || !dpyinfo->display_handle)
    {
      nlog_fatal ("winit backend not initialized");
      emacs_abort ();
    }

  uint32_t window_id = neomacs_display_create_window (
    dpyinfo->display_handle,
    FRAME_PIXEL_WIDTH (f),
    FRAME_PIXEL_HEIGHT (f),
    "Emacs"
  );

  if (window_id == 0)
    {
      nlog_fatal ("Failed to create winit window");
      emacs_abort ();
    }

  /* Successfully created winit window */
  output->window_id = window_id;
  output->window_desc = (Window) window_id;

  /* Child frames share the parent's winit window and are composited as
     overlays.  Don't overwrite the global resize callback (it must
     point to the root frame) and don't show a separate OS window.  */
  if (!FRAME_PARENT_FRAME (f))
    {
      neomacs_display_set_resize_callback (neomacs_widget_resize_cb, f);
      neomacs_display_show_window (dpyinfo->display_handle, window_id, true);
    }

  output->widget = NULL;
  output->drawing_area = NULL;
  output->use_gpu_widget = 1;

  /* Initialize cursor types for mouse-highlight.
     Values match Rust CursorIcon mapping:
     1=arrow, 2=text/ibeam, 3=hand/pointer,
     5=h-resize, 6=v-resize, 7=hourglass */
  output->text_cursor             = (Emacs_Cursor)(intptr_t) 2;
  output->nontext_cursor          = (Emacs_Cursor)(intptr_t) 1;
  output->modeline_cursor         = (Emacs_Cursor)(intptr_t) 1;
  output->hand_cursor             = (Emacs_Cursor)(intptr_t) 3;
  output->hourglass_cursor        = (Emacs_Cursor)(intptr_t) 7;
  output->horizontal_drag_cursor  = (Emacs_Cursor)(intptr_t) 5;
  output->vertical_drag_cursor    = (Emacs_Cursor)(intptr_t) 6;
  output->current_cursor          = (Emacs_Cursor)(intptr_t) 1;
  output->left_edge_cursor        = (Emacs_Cursor)(intptr_t) 5;
  output->top_left_corner_cursor  = (Emacs_Cursor)(intptr_t) 8;
  output->top_edge_cursor         = (Emacs_Cursor)(intptr_t) 6;
  output->top_right_corner_cursor = (Emacs_Cursor)(intptr_t) 9;
  output->right_edge_cursor       = (Emacs_Cursor)(intptr_t) 5;
  output->bottom_right_corner_cursor = (Emacs_Cursor)(intptr_t) 11;
  output->bottom_edge_cursor      = (Emacs_Cursor)(intptr_t) 6;
  output->bottom_left_corner_cursor  = (Emacs_Cursor)(intptr_t) 10;

  nlog_info ("Created winit window with id %u", window_id);
}


/* ============================================================================
 * Frame Creation
 * ============================================================================ */

/* Handler for signals raised during Fx_create_frame.
   FRAME is the frame which is partially constructed.  */
static Lisp_Object
unwind_create_frame (Lisp_Object frame)
{
  struct frame *f = XFRAME (frame);

  /* If frame is already dead, nothing to do.  This can happen if the
     display is disconnected after the frame has become official, but
     before Fx_create_frame removes the unwind protect.  */
  if (!FRAME_LIVE_P (f))
    return Qnil;

  /* If frame is ``official'' (already in Vframe_list), nothing to do.  */
  if (NILP (Fmemq (frame, Vframe_list)))
    {
      /* Frame never became official - clean up allocated resources.  */
      struct neomacs_display_info *dpyinfo
	= FRAME_NEOMACS_DISPLAY_INFO (f);
      if (dpyinfo)
	dpyinfo->reference_count--;
      free_glyphs (f);
      return Qt;
    }

  return Qnil;
}

static void
do_unwind_create_frame (Lisp_Object frame)
{
  unwind_create_frame (frame);
}

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

  /* Use this general default value to start with
     until we know if this frame has a specified name.  */
  Vx_resource_name = Vinvocation_name;

  /* Get display info */
  tem = gui_display_get_arg (dpyinfo, parms, Qterminal, 0, 0,
                             RES_TYPE_NUMBER);
  if (BASE_EQ (tem, Qunbound))
    tem = gui_display_get_arg (dpyinfo, parms, Qdisplay, 0, 0,
                               RES_TYPE_STRING);
  dpyinfo = check_neomacs_display_info (tem);
  kb = dpyinfo->terminal->kboard;

  if (!dpyinfo->terminal->name)
    error ("Terminal is not live, can't create new frames on it");

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
  FRAME_FONTSET (f) = -1;
  FRAME_NEOMACS_OUTPUT (f)->display_info = dpyinfo;
  dpyinfo->reference_count++;

  /* With FRAME_DISPLAY_INFO set up, this unwind-protect is safe.  */
  record_unwind_protect (do_unwind_create_frame, frame);

  /* Initialize color slots to -1 so we won't try to free colors
     we haven't allocated if x_decode_color signals an error.  */
  {
    Lisp_Object black = build_string ("black");

    FRAME_FOREGROUND_PIXEL (f) = -1;
    FRAME_BACKGROUND_PIXEL (f) = -1;
    FRAME_NEOMACS_OUTPUT (f)->cursor_pixel = -1;
    FRAME_NEOMACS_OUTPUT (f)->cursor_foreground_pixel = -1;
    FRAME_NEOMACS_OUTPUT (f)->border_pixel = -1;

    FRAME_FOREGROUND_PIXEL (f)
      = x_decode_color (f, black, BLACK_PIX_DEFAULT (f));
    FRAME_BACKGROUND_PIXEL (f)
      = x_decode_color (f, build_string ("white"),
			WHITE_PIX_DEFAULT (f));
    FRAME_NEOMACS_OUTPUT (f)->cursor_pixel
      = x_decode_color (f, black, BLACK_PIX_DEFAULT (f));
    FRAME_NEOMACS_OUTPUT (f)->cursor_foreground_pixel
      = x_decode_color (f, build_string ("white"),
			WHITE_PIX_DEFAULT (f));
    FRAME_NEOMACS_OUTPUT (f)->border_pixel
      = x_decode_color (f, black, BLACK_PIX_DEFAULT (f));
    FRAME_NEOMACS_OUTPUT (f)->foreground_pixel
      = FRAME_FOREGROUND_PIXEL (f);
    FRAME_NEOMACS_OUTPUT (f)->background_pixel
      = FRAME_BACKGROUND_PIXEL (f);
  }

  /* Set parent-frame EARLY so that adjust_frame_size / set_window_size
     can detect child frames before resizing the OS window.  Official
     Emacs (xfns.c) does this at the same stage.  */
  {
    Lisp_Object parent_frame = gui_display_get_arg (dpyinfo, parms,
                                                     Qparent_frame,
                                                     NULL, NULL,
                                                     RES_TYPE_SYMBOL);
    if (!BASE_EQ (parent_frame, Qunbound) && !NILP (parent_frame))
      {
        /* Validate that the parent is a live neomacs frame */
        struct frame *p = decode_live_frame (parent_frame);
        if (!FRAME_NEOMACS_P (p))
          error ("Parent frame is not a neomacs frame");
        fset_parent_frame (f, parent_frame);
        store_frame_param (f, Qparent_frame, parent_frame);
      }
  }

  /* Initialize frame dimensions */
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

  /* Set the name; the functions to which we pass f expect the name to
     be set.  */
  if (BASE_EQ (name, Qunbound) || NILP (name))
    {
      fset_name (f, Vinvocation_name);
      f->explicit_name = false;
    }
  else
    {
      fset_name (f, name);
      f->explicit_name = true;
      specbind (Qx_resource_name, name);
    }

  /* Set up font - required before face realization */
  neomacs_default_font_parameter (f, parms);

  /* Validate that we got a font.  */
  if (!FRAME_FONT (f))
    {
      delete_frame (frame, Qnoelisp);
      error ("Invalid frame font");
    }

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

  /* Call adjust_frame_size to initialize glyph matrices.
     CRITICAL: This sets glyphs_initialized_p which is required for redisplay!

     Root frames: size to match the pre-existing winit window (created by
     init_threaded), since in neomacs the window exists before the frame.

     Child frames: size to match the user-specified width/height parameters
     (in character units, converted to pixels using the frame's font).  */
  {
    int win_w, win_h;
    if (FRAME_PARENT_FRAME (f))
      {
        /* Child frames use the caller-specified character dimensions. */
        win_w = width * FRAME_COLUMN_WIDTH (f);
        win_h = height * FRAME_LINE_HEIGHT (f);
      }
    else
      {
        /* Root frames match the pre-existing winit window. */
        win_w = dpyinfo->init_window_width;
        win_h = dpyinfo->init_window_height;
      }
    int text_width = win_w
      - FRAME_LEFT_FRINGE_WIDTH (f) - FRAME_RIGHT_FRINGE_WIDTH (f)
      - FRAME_SCROLL_BAR_AREA_WIDTH (f)
      - 2 * FRAME_INTERNAL_BORDER_WIDTH (f);
    int text_height = win_h
      - 2 * FRAME_INTERNAL_BORDER_WIDTH (f);
    if (text_width < 0) text_width = 0;
    if (text_height < 0) text_height = 0;
    adjust_frame_size (f, text_width, text_height, 5, true,
		       Qx_create_frame_1);
  }

  /* Menu, tab, tool bar lines.
     Default to 0 (hidden) — these are not yet fully functional.  */
  gui_default_parameter (f, parms, Qmenu_bar_lines,
			 make_fixnum (0),
			 NULL, NULL, RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qtab_bar_lines,
			 make_fixnum (0),
			 NULL, NULL, RES_TYPE_NUMBER);
  gui_default_parameter (f, parms, Qtool_bar_lines,
			 make_fixnum (0),
			 NULL, NULL, RES_TYPE_NUMBER);


  gui_default_parameter (f, parms, Qbuffer_predicate, Qnil,
			 "bufferPredicate", "BufferPredicate",
			 RES_TYPE_SYMBOL);
  gui_default_parameter (f, parms, Qtitle, Qnil,
			 "title", "Title", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qinhibit_double_buffering, Qnil,
			 "inhibitDoubleBuffering", "InhibitDoubleBuffering",
			 RES_TYPE_BOOLEAN);

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
  FRAME_NEOMACS_OUTPUT (f)->left_edge_cursor = (Emacs_Cursor) 5;
  FRAME_NEOMACS_OUTPUT (f)->right_edge_cursor = (Emacs_Cursor) 5;
  FRAME_NEOMACS_OUTPUT (f)->top_edge_cursor = (Emacs_Cursor) 6;
  FRAME_NEOMACS_OUTPUT (f)->bottom_edge_cursor = (Emacs_Cursor) 6;
  FRAME_NEOMACS_OUTPUT (f)->top_left_corner_cursor = (Emacs_Cursor) 8;
  FRAME_NEOMACS_OUTPUT (f)->top_right_corner_cursor = (Emacs_Cursor) 9;
  FRAME_NEOMACS_OUTPUT (f)->bottom_left_corner_cursor = (Emacs_Cursor) 10;
  FRAME_NEOMACS_OUTPUT (f)->bottom_right_corner_cursor = (Emacs_Cursor) 11;

  tem = gui_display_get_arg (dpyinfo, parms, Qunsplittable, 0, 0,
			     RES_TYPE_BOOLEAN);
  f->no_split = minibuffer_only || EQ (tem, Qt);

  /* Now consider frame official.  */
  f->terminal->reference_count++;
  Vframe_list = Fcons (frame, Vframe_list);

  /* Create winit window */
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

  /* Initialize `default-minibuffer-frame' in case this is the first
     frame on this terminal.  */
  if (FRAME_HAS_MINIBUF_P (f)
      && (!FRAMEP (KVAR (kb, Vdefault_minibuffer_frame))
	  || !FRAME_LIVE_P (XFRAME (KVAR (kb,
					   Vdefault_minibuffer_frame)))))
    kset_default_minibuffer_frame (kb, frame);

  /* All remaining specified parameters, which have not been "used" by
     gui_display_get_arg and friends, now go in the misc. alist of the
     frame.  */
  for (tem = parms; CONSP (tem); tem = XCDR (tem))
    if (CONSP (XCAR (tem)) && !NILP (XCAR (XCAR (tem))))
      fset_param_alist (f, Fcons (XCAR (tem), f->param_alist));

  /* Make sure windows on this frame appear in calls to next-window
     and similar functions.  */
  Vwindow_list = Qnil;

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
  struct neomacs_display_info *dpyinfo
    = check_neomacs_display_info (terminal);
  int planes = dpyinfo->n_planes > 0 ? dpyinfo->n_planes : 24;
  return make_fixnum (1 << min (planes, 24));
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
  int n = neomacs_display_get_monitor_count ();
  return make_fixnum (n > 0 ? n : 1);
}

DEFUN ("x-display-mm-height", Fx_display_mm_height,
       Sx_display_mm_height, 0, 1, 0,
       doc: /* Return the height of the Neomacs display in millimeters.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo
    = check_neomacs_display_info (terminal);
  int height_mm = 0;
  int n = neomacs_display_get_monitor_count ();

  for (int i = 0; i < n; i++)
    {
      struct NeomacsMonitorInfo info;
      if (neomacs_display_get_monitor_info (i, &info) && info.height_mm > height_mm)
        height_mm = info.height_mm;
    }

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
  int n = neomacs_display_get_monitor_count ();

  for (int i = 0; i < n; i++)
    {
      struct NeomacsMonitorInfo info;
      if (neomacs_display_get_monitor_info (i, &info) && info.width_mm > width_mm)
        width_mm = info.width_mm;
    }

  /* Fallback: estimate from pixel width assuming ~96 DPI */
  if (width_mm == 0 && dpyinfo->width > 0)
    width_mm = (int) (dpyinfo->width * 25.4 / 96.0);

  return make_fixnum (width_mm);
}

DEFUN ("x-server-max-request-size", Fx_server_max_request_size,
       Sx_server_max_request_size, 0, 1, 0,
       doc: /* Return the maximum request size for the Neomacs display.
GPU rendering has no protocol size limit; returns a large value.  */)
  (Lisp_Object terminal)
{
  check_neomacs_display_info (terminal);
  /* No protocol request size limit for GPU rendering.
     Return 65535 (same as X11 default) for compatibility.  */
  return make_fixnum (65535);
}

DEFUN ("x-display-backing-store", Fx_display_backing_store,
       Sx_display_backing_store, 0, 1, 0,
       doc: /* Return the backing store capability of the Neomacs display.
GPU rendering always redraws fully; returns `always'.  */)
  (Lisp_Object terminal)
{
  check_neomacs_display_info (terminal);
  /* GPU rendering effectively provides "always" backing store since
     the entire frame is re-rendered each display cycle.  */
  return intern ("always");
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
  struct frame *f = decode_window_system_frame (frame);

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

  /* Get tooltip colors from the "tooltip" face.  */
  float fg_r = 0.9f, fg_g = 0.9f, fg_b = 0.9f;
  float bg_r = 0.15f, bg_g = 0.15f, bg_b = 0.18f;

  int tooltip_face_id = lookup_named_face (NULL, f, intern ("tooltip"), false);
  if (tooltip_face_id >= 0)
    {
      struct face *tooltip_face = FACE_FROM_ID_OR_NULL (f, tooltip_face_id);
      if (tooltip_face)
        {
          unsigned long fg = tooltip_face->foreground;
          unsigned long bg = tooltip_face->background;
          fg_r = ((fg >> 16) & 0xff) / 255.0f;
          fg_g = ((fg >> 8) & 0xff) / 255.0f;
          fg_b = (fg & 0xff) / 255.0f;
          bg_r = ((bg >> 16) & 0xff) / 255.0f;
          bg_g = ((bg >> 8) & 0xff) / 255.0f;
          bg_b = (bg & 0xff) / 255.0f;
        }
    }

  /* Get mouse position from display info.  */
  struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
  float tip_x = (float) dpyinfo->last_mouse_motion_x + XFIXNUM (dx);
  float tip_y = (float) dpyinfo->last_mouse_motion_y + XFIXNUM (dy);

  /* Show GPU overlay tooltip.  */
  const char *text = SSDATA (ENCODE_UTF_8 (string));
  if (dpyinfo->display_handle)
    neomacs_display_show_tooltip (dpyinfo->display_handle,
                                  tip_x, tip_y, text,
                                  fg_r, fg_g, fg_b,
                                  bg_r, bg_g, bg_b);
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
      /* Hide GPU overlay tooltip.  */
      struct frame *f = SELECTED_FRAME ();
      if (FRAME_NEOMACS_P (f))
        {
          struct neomacs_display_info *dpyinfo = FRAME_DISPLAY_INFO (f);
          if (dpyinfo->display_handle)
            neomacs_display_hide_tooltip (dpyinfo->display_handle);
        }
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
See `neomacs-frame-geometry' for the list of returned attributes.  */)
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

DEFUN ("neomacs-set-mouse-absolute-pixel-position",
       Fneomacs_set_mouse_absolute_pixel_position,
       Sneomacs_set_mouse_absolute_pixel_position, 2, 2, 0,
       doc: /* Move mouse pointer to absolute pixel position (X, Y).
The coordinates X and Y are interpreted in pixels relative to a position
\(0, 0) of the selected frame's display.  */)
  (Lisp_Object x, Lisp_Object y)
{
  struct frame *f = SELECTED_FRAME ();
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  struct neomacs_display_info *dpyinfo = FRAME_NEOMACS_DISPLAY_INFO (f);
  if (!dpyinfo || !output)
    return Qnil;

  int abs_x = check_integer_range (x, INT_MIN, INT_MAX);
  int abs_y = check_integer_range (y, INT_MIN, INT_MAX);

  /* Convert absolute screen coordinates to frame-relative coordinates.  */
  int rel_x = abs_x - output->left_pos;
  int rel_y = abs_y - output->top_pos;

  if (dpyinfo->display_handle)
    neomacs_display_warp_mouse (dpyinfo->display_handle, rel_x, rel_y);

  dpyinfo->last_mouse_motion_x = abs_x;
  dpyinfo->last_mouse_motion_y = abs_y;

  return Qnil;
}

DEFUN ("neomacs-display-monitor-attributes-list",
       Fneomacs_display_monitor_attributes_list,
       Sneomacs_display_monitor_attributes_list,
       0, 1, 0,
       doc: /* Return a list of physical monitor attributes on TERMINAL.
The optional argument TERMINAL specifies which display to ask about.
TERMINAL should be a terminal object, a frame or a display name (a string).
If omitted or nil, that stands for the selected frame's display.

Internal use only, use `display-monitor-attributes-list' instead.  */)
  (Lisp_Object terminal)
{
  struct neomacs_display_info *dpyinfo
    = check_neomacs_display_info (terminal);
  Lisp_Object attributes_list = Qnil;
  int n_monitors, primary_monitor = 0;
  Lisp_Object monitor_frames, rest, frame;
  static const char *source = "winit";
  struct MonitorInfo *monitors;

  block_input ();
  n_monitors = neomacs_display_get_monitor_count ();
  if (n_monitors == 0)
    {
      unblock_input ();
      return Qnil;
    }

  monitor_frames = make_nil_vector (n_monitors);
  monitors = xzalloc (n_monitors * sizeof *monitors);

  /* Collect frames per monitor.  Since neomacs uses a single winit
     window, all frames are effectively on the same monitor.  Assign
     all non-tooltip frames to the primary monitor.  */
  FOR_EACH_FRAME (rest, frame)
    {
      struct frame *f = XFRAME (frame);
      if (FRAME_NEOMACS_P (f)
	  && FRAME_DISPLAY_INFO (f) == dpyinfo
	  && !FRAME_TOOLTIP_P (f))
	ASET (monitor_frames, 0,
	      Fcons (frame, AREF (monitor_frames, 0)));
    }

  for (int i = 0; i < n_monitors; i++)
    {
      struct NeomacsMonitorInfo nmi;
      struct MonitorInfo *mi = &monitors[i];

      if (!neomacs_display_get_monitor_info (i, &nmi))
        continue;

      mi->geom.x = nmi.x;
      mi->geom.y = nmi.y;
      mi->geom.width = nmi.width;
      mi->geom.height = nmi.height;
      /* winit doesn't provide workarea; use geometry.  */
      mi->work = mi->geom;
      mi->mm_width = nmi.width_mm;
      mi->mm_height = nmi.height_mm;

      const char *name = neomacs_display_get_monitor_name (i);
      if (name)
	dupstring (&mi->name, name);
    }

  attributes_list = make_monitor_attribute_list (monitors,
						 n_monitors,
						 primary_monitor,
						 monitor_frames,
						 source);
  free_monitors (monitors, n_monitors);
  unblock_input ();

  return attributes_list;
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
 * Primary Selection Functions
 * ============================================================================ */

DEFUN ("neomacs-primary-selection-set", Fneomacs_primary_selection_set,
       Sneomacs_primary_selection_set, 1, 1, 0,
       doc: /* Set the primary selection to TEXT.
Returns t on success, nil on failure.  */)
  (Lisp_Object text)
{
  CHECK_STRING (text);
  const char *str = SSDATA (text);
  int result = neomacs_primary_selection_set_text (str);
  return result == 0 ? Qt : Qnil;
}

DEFUN ("neomacs-primary-selection-get", Fneomacs_primary_selection_get,
       Sneomacs_primary_selection_get, 0, 0, 0,
       doc: /* Get text from the primary selection.
Returns a string, or nil if the selection is empty or an error occurred.  */)
  (void)
{
  char *text = neomacs_primary_selection_get_text ();
  if (!text)
    return Qnil;
  Lisp_Object result = build_string (text);
  neomacs_clipboard_free_text (text);
  return result;
}

/* ============================================================================
 * Core backend query
 * ============================================================================ */

#ifndef NEOVM_CORE_BACKEND_NAME
#define NEOVM_CORE_BACKEND_NAME "emacs-c"
#endif

DEFUN ("neomacs-core-backend", Fneomacs_core_backend,
       Sneomacs_core_backend, 0, 0, 0,
       doc: /* Return the compile-time NeoVM core backend.
The return value is a string: either \"emacs-c\" or \"rust\".  */)
  (void)
{
  return build_string (NEOVM_CORE_BACKEND_NAME);
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
  defsubr (&Sneomacs_set_mouse_absolute_pixel_position);
  defsubr (&Sneomacs_display_monitor_attributes_list);

  /* Connection functions */
  defsubr (&Sx_open_connection);
  defsubr (&Sx_close_connection);

  /* Scroll bar functions */
  defsubr (&Sx_scroll_bar_foreground);
  defsubr (&Sx_scroll_bar_background);

  /* Clipboard functions */
  defsubr (&Sneomacs_clipboard_set);
  defsubr (&Sneomacs_clipboard_get);

  /* Primary selection functions */
  defsubr (&Sneomacs_primary_selection_set);
  defsubr (&Sneomacs_primary_selection_get);

  /* Core backend query */
  defsubr (&Sneomacs_core_backend);

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
