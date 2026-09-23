use super::*;

#[test]
fn text_window_append_surface_request_reserves_right_columns() {
    let tab_stops = vec![4, 12];
    let surface =
        TextWindowAppendSurfaceRequest::new(20.0, 200.0, 16.0, true, true, 8.0, 6, &tab_stops)
            .into_surface();

    assert_eq!(surface.content_x(), 20.0);
    // GNU xdisp reserves both the continuation glyph and a non-rightmost
    // terminal window's border before it consumes body text.  The marker
    // must not replace the final source character at its display column.
    assert_eq!(surface.right_edge(), 188.0);
    assert_eq!(surface.full_text_right_edge(), 204.0);
}
