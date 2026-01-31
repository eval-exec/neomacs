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
#include "buffer.h"
#include "coding.h"
#include "window.h"
#include "keyboard.h"
#include "termhooks.h"
#include "termchar.h"
#include "font.h"
#include "pdumper.h"
#include "fontset.h"

/* List of Neomacs display info structures */
struct neomacs_display_info *neomacs_display_list = NULL;

/* Forward declarations */
extern frame_parm_handler neomacs_frame_parm_handlers[];

/* Prototypes for internal functions */
static void neomacs_initialize_display_info (struct neomacs_display_info *);
static const char *neomacs_get_string_resource (void *, const char *, const char *);
static void neomacs_make_frame_visible_invisible (struct frame *, bool);
static void neomacs_after_update_window_line (struct window *, struct glyph_row *);
static void neomacs_update_window_begin (struct window *);
static void neomacs_update_window_end (struct window *, bool, bool);

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
  .draw_vertical_window_border = NULL,
  .draw_window_divider = NULL,
  .shift_glyphs_for_insert = NULL,
  .show_hourglass = NULL,
  .hide_hourglass = NULL,
  .default_font_parameter = NULL,
};


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
  static bool gtk_initialized = false;

  /* Initialize GTK if not already done */
  if (!gtk_initialized)
    {
      gtk_init ();
      gtk_initialized = true;
    }

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
  terminal->update_begin_hook = neomacs_update_begin;
  terminal->update_end_hook = neomacs_update_end;
  terminal->defined_color_hook = neomacs_defined_color;
  terminal->get_string_resource_hook = neomacs_get_string_resource;
  terminal->set_new_font_hook = neomacs_new_font;
  terminal->read_socket_hook = neomacs_read_socket;
  terminal->frame_visible_invisible_hook = neomacs_make_frame_visible_invisible;

  /* Register the display connection fd for event handling */
  if (dpyinfo->connection >= 0)
    add_keyboard_wait_descriptor (dpyinfo->connection);

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
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (dpyinfo && dpyinfo->display_handle)
    neomacs_display_end_frame (dpyinfo->display_handle);

  /* Queue a redraw of the drawing area */
  if (output && output->drawing_area)
    {
      /* For GPU widget, also set the scene */
      if (output->use_gpu_widget && dpyinfo && dpyinfo->display_handle)
        {
          neomacs_display_render_to_widget (dpyinfo->display_handle, output->drawing_area);
        }
      
      gtk_widget_queue_draw (GTK_WIDGET (output->drawing_area));
    }
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
    gtk_widget_queue_draw (GTK_WIDGET (output->drawing_area));
}

/* Make frame visible or invisible */
static void
neomacs_make_frame_visible_invisible (struct frame *f, bool visible)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);

  if (!output || !output->widget)
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
          /* Always call begin_row - pass X position for glyph replacement */
          int row_y = s->row->y;
          
          neomacs_display_begin_row (dpyinfo->display_handle,
                                     row_y,
                                     s->x,  /* Starting X position for this glyph string */
                                     s->row->height,
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
              
              /* Get font weight and slant from lface attributes */
              int font_weight = 400;  /* normal */
              int is_italic = 0;
              
              /* Check face's lface for weight (bold) */
              Lisp_Object weight_attr = face->lface[LFACE_WEIGHT_INDEX];
              if (!NILP (weight_attr) && SYMBOLP (weight_attr))
                {
                  int weight_numeric = FONT_WEIGHT_NAME_NUMERIC (weight_attr);
                  if (weight_numeric > 0)
                    font_weight = weight_numeric;
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
                  underline_style = 1;  /* line */
                  if (face->underline == FACE_UNDERLINE_WAVE)
                    underline_style = 2;
                  /* Use foreground as underline color (there's no separate underline_color field) */
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
                  if (face->box_color_defaulted_p == 0)
                    box_color = ((RED_FROM_ULONG(face->box_color) << 16) |
                                 (GREEN_FROM_ULONG(face->box_color) << 8) |
                                 BLUE_FROM_ULONG(face->box_color));
                }
              
              neomacs_display_set_face (dpyinfo->display_handle,
                                        face_id,
                                        fg_rgb,
                                        bg_rgb,
                                        font_weight,
                                        is_italic,
                                        underline_style,
                                        underline_color,
                                        box_type,
                                        box_color,
                                        box_line_width);
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
            case STRETCH_GLYPH:
              neomacs_display_add_stretch_glyph (dpyinfo->display_handle,
                                                  s->first_glyph->pixel_width,
                                                  s->row->height,
                                                  (uint32_t) face_id);
              break;
            case IMAGE_GLYPH:
              /* TODO: Handle image glyphs */
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
      return;  /* Don't do Cairo drawing for GPU widget */
    }

  /* For Cairo mode, we need the context */
  if (!output->cr_context)
    return;

  cr = output->cr_context;

  /* Forward glyph data to Rust scene graph if available */
  if (dpyinfo && dpyinfo->display_handle && s->first_glyph && s->row)
    {
      /* Always call begin_row - pass X position for glyph replacement */
      int row_y = s->row->y;
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

/* Clear a rectangle on the frame */
void
neomacs_clear_frame_area (struct frame *f, int x, int y, int width, int height)
{
  struct neomacs_output *output = FRAME_NEOMACS_OUTPUT (f);
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

  /* Get cursor dimensions */
  int char_width = cursor_width > 0 ? cursor_width : FRAME_COLUMN_WIDTH (f);
  int char_height = FRAME_LINE_HEIGHT (f);

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
      
      neomacs_display_set_cursor (dpyinfo->display_handle,
                                  (float) x, (float) y,
                                  (float) char_width, (float) char_height,
                                  style, rgba, 1);
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
      cairo_rectangle (cr, x, y, char_width, char_height);
      cairo_fill (cr);
      break;

    case BAR_CURSOR:
      /* Vertical bar cursor */
      cairo_rectangle (cr, x, y, 2, char_height);
      cairo_fill (cr);
      break;

    case HBAR_CURSOR:
      /* Horizontal bar cursor */
      cairo_rectangle (cr, x, y + char_height - 2, char_width, 2);
      cairo_fill (cr);
      break;

    case HOLLOW_BOX_CURSOR:
      /* Hollow box cursor */
      cairo_set_line_width (cr, 1.0);
      cairo_rectangle (cr, x + 0.5, y + 0.5, char_width - 1, char_height - 1);
      cairo_stroke (cr);
      break;

    case NO_CURSOR:
      break;
    }
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

  return font_object;
}


/* ============================================================================
 * Input Event Handling
 * ============================================================================ */

/* Read socket events for the Neomacs terminal.
   This processes GTK4 events and converts them to Emacs events.  */
int
neomacs_read_socket (struct terminal *terminal, struct input_event *hold_quit)
{
  GMainContext *context;
  int count = 0;
  static int call_count = 0;

  call_count++;
  if (call_count <= 5 || (call_count % 100 == 0))
    if (0) fprintf (stderr, "DEBUG: neomacs_read_socket called, count=%d\n", call_count);

  /* Get the default GLib main context */
  context = g_main_context_default ();

  block_input ();

  /* Process pending GTK events using iteration
     - FALSE means don't block if no events */
  while (g_main_context_iteration (context, FALSE))
    {
      /* Events were processed - increment count to tell Emacs we did something */
      count++;
    }

  unblock_input ();

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
    
  /* TODO: Get EGL display from GTK4 - for now pass NULL to use default */
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
  defsubr (&Sneomacs_video_floating);
  defsubr (&Sneomacs_video_floating_clear);

  /* Image functions */
  defsubr (&Sneomacs_image_load);
  defsubr (&Sneomacs_image_size);
  defsubr (&Sneomacs_image_free);
  defsubr (&Sneomacs_image_floating);
  defsubr (&Sneomacs_image_floating_clear);

  /* WebKit browser functions */
  defsubr (&Sneomacs_webkit_init);
  defsubr (&Sneomacs_webkit_create);
  defsubr (&Sneomacs_webkit_destroy);
  defsubr (&Sneomacs_webkit_load_uri);
  defsubr (&Sneomacs_webkit_go_back);
  defsubr (&Sneomacs_webkit_go_forward);
  defsubr (&Sneomacs_webkit_reload);
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

  DEFSYM (Qneomacs, "neomacs");
  DEFSYM (Qvideo, "video");
  DEFSYM (Qwebkit, "webkit");
  DEFSYM (QCid, ":id");

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
