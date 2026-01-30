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
#include "buffer.h"
#include "window.h"
#include "keyboard.h"
#include "frame.h"
#include "termhooks.h"
#include "coding.h"
#include "font.h"

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
  /* Tab bar lines - not implemented yet */
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

  /* Update frame dimensions */
  int old_cols = FRAME_COLS (f);
  int old_rows = FRAME_LINES (f);
  int new_cols = width / FRAME_COLUMN_WIDTH (f);
  int new_rows = height / FRAME_LINE_HEIGHT (f);

  if (new_cols != old_cols || new_rows != old_rows)
    {
      change_frame_size (f, new_cols, new_rows, false, true, false);
    }
}

/* Callback for GTK4 drawing area draw */
static void
neomacs_draw_cb (GtkDrawingArea *area, cairo_t *cr,
                 int width, int height, gpointer user_data)
{
  struct frame *f = (struct frame *) user_data;
  
  if (!FRAME_NEOMACS_P (f))
    return;

  /* For now, fill with background color */
  struct face *face = FACE_FROM_ID (f, DEFAULT_FACE_ID);
  if (face)
    {
      unsigned long bg = face->background;
      double r = RED_FROM_ULONG (bg) / 255.0;
      double g = GREEN_FROM_ULONG (bg) / 255.0;
      double b = BLUE_FROM_ULONG (bg) / 255.0;
      cairo_set_source_rgb (cr, r, g, b);
      cairo_paint (cr);
    }

  /* Mark frame for redisplay */
  SET_FRAME_GARBAGED (f);
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

/* Create GTK4 widgets for a frame */
static void
neomacs_create_frame_widgets (struct frame *f)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
  GtkWidget *window, *drawing_area;

  /* Create main window */
  window = gtk_window_new ();
  gtk_window_set_title (GTK_WINDOW (window), "Emacs");
  gtk_window_set_default_size (GTK_WINDOW (window), 
                               FRAME_PIXEL_WIDTH (f),
                               FRAME_PIXEL_HEIGHT (f));

  /* Create drawing area */
  drawing_area = gtk_drawing_area_new ();
  gtk_drawing_area_set_content_width (GTK_DRAWING_AREA (drawing_area),
                                      FRAME_PIXEL_WIDTH (f));
  gtk_drawing_area_set_content_height (GTK_DRAWING_AREA (drawing_area),
                                       FRAME_PIXEL_HEIGHT (f));

  /* Connect callbacks */
  gtk_drawing_area_set_draw_func (GTK_DRAWING_AREA (drawing_area),
                                  neomacs_draw_cb, f, NULL);
  g_signal_connect (drawing_area, "resize",
                    G_CALLBACK (neomacs_resize_cb), f);
  g_signal_connect (window, "close-request",
                    G_CALLBACK (neomacs_close_request_cb), f);

  /* Set up widget hierarchy */
  gtk_window_set_child (GTK_WINDOW (window), drawing_area);

  /* Store in output structure */
  output->widget = window;
  output->drawing_area = drawing_area;
  output->window_desc = (Window) (intptr_t) window;

  /* Show the window */
  gtk_window_present (GTK_WINDOW (window));
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
  gui_default_parameter (f, parms, Qforeground_color, build_string ("black"),
			 "foreground", "Foreground", RES_TYPE_STRING);
  gui_default_parameter (f, parms, Qbackground_color, build_string ("white"),
			 "background", "Background", RES_TYPE_STRING);

  /* Initialize cursor */
  FRAME_NEOMACS_OUTPUT (f)->cursor_pixel = dpyinfo->black_pixel;
  FRAME_NEOMACS_OUTPUT (f)->cursor_foreground_pixel = dpyinfo->white_pixel;

  /* Store in frame list */
  Vframe_list = Fcons (frame, Vframe_list);

  /* Create GTK4 widgets */
  block_input ();
  neomacs_create_frame_widgets (f);
  unblock_input ();

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
