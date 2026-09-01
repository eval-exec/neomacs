use super::*;

#[test]
fn test_scene_creation() {
    let scene = Scene::new(800.0, 600.0);
    assert_eq!(scene.width, 800.0);
    assert_eq!(scene.height, 600.0);
}

#[test]
fn test_dirty_region() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.mark_region_dirty(Rect::new(10.0, 10.0, 100.0, 100.0));
    scene.mark_region_dirty(Rect::new(50.0, 50.0, 100.0, 100.0));

    let dirty = scene.dirty.unwrap();
    assert_eq!(dirty.x, 10.0);
    assert_eq!(dirty.y, 10.0);
    assert_eq!(dirty.right(), 150.0);
    assert_eq!(dirty.bottom(), 150.0);
}

// ---------------------------------------------------------------
// Scene creation and modification
// ---------------------------------------------------------------

#[test]
fn test_scene_defaults() {
    let scene = Scene::new(1920.0, 1080.0);
    assert_eq!(scene.background, Color::BLACK);
    assert!(scene.windows.is_empty());
    assert!(scene.root.is_none());
    assert!(scene.dirty.is_none());
    assert!(scene.faces.is_empty());
    assert!(scene.floating_videos.is_empty());
    assert!(scene.floating_images.is_empty());
    assert!(scene.borders.is_empty());
}

#[test]
fn test_scene_clear_resets_windows_and_root_but_preserves_faces() {
    let mut scene = Scene::new(800.0, 600.0);
    // Add a face
    let face = Face::new(FaceId::new(1));
    scene.set_face(face);
    // Add a window
    scene.windows.push(make_window(42, 0.0, 0.0, 400.0, 300.0));
    // Build the scene graph
    scene.build();
    assert!(scene.root.is_some());
    assert_eq!(scene.windows.len(), 1);

    scene.clear();

    assert!(scene.windows.is_empty());
    assert!(scene.root.is_none());
    // Faces should be preserved
    assert!(scene.get_face(FaceId::new(1)).is_some());
    // clear marks the scene dirty
    assert!(scene.dirty.is_some());
}

#[test]
fn test_scene_set_and_get_face() {
    let mut scene = Scene::new(800.0, 600.0);
    let mut face = Face::new(FaceId::new(5));
    face.font_size = 16.0;
    face.font_family = "Iosevka".to_string();
    scene.set_face(face);

    let retrieved = scene.get_face(FaceId::new(5)).unwrap();
    assert_eq!(retrieved.id, FaceId::new(5));
    assert_eq!(retrieved.font_size, 16.0);
    assert_eq!(retrieved.font_family, "Iosevka");
    // Non-existent face
    assert!(scene.get_face(FaceId::new(99)).is_none());
}

#[test]
fn test_scene_set_face_overwrites() {
    let mut scene = Scene::new(800.0, 600.0);
    let mut face = Face::new(FaceId::new(1));
    face.font_size = 12.0;
    scene.set_face(face);

    let mut updated = Face::new(FaceId::new(1));
    updated.font_size = 20.0;
    scene.set_face(updated);

    let f = scene.get_face(FaceId::new(1)).unwrap();
    assert_eq!(f.font_size, 20.0);
}

// ---------------------------------------------------------------
// Node creation (text, rect, image, etc.)
// ---------------------------------------------------------------

#[test]
fn test_node_color_rect() {
    let bounds = Rect::new(10.0, 20.0, 100.0, 50.0);
    let node = Node::color_rect(bounds, Color::RED);
    assert_eq!(node.bounds, bounds);
    assert_eq!(node.opacity, 1.0);
    assert!(node.transform.is_none());
    assert!(node.clip.is_none());
    match node.kind {
        NodeKind::ColorRect { color } => assert_eq!(color, Color::RED),
        _ => panic!("Expected ColorRect"),
    }
}

#[test]
fn test_node_text_run() {
    let bounds = Rect::new(0.0, 0.0, 200.0, 16.0);
    let node = Node::text_run("hello world".into(), FaceId::new(3), 5.0, 10.0, bounds);
    match &node.kind {
        NodeKind::TextRun {
            text,
            face_id,
            x,
            y,
        } => {
            assert_eq!(text, "hello world");
            assert_eq!(*face_id, FaceId::new(3));
            assert_eq!(*x, 5.0);
            assert_eq!(*y, 10.0);
        }
        _ => panic!("Expected TextRun"),
    }
    assert_eq!(node.bounds, bounds);
}

#[test]
fn test_node_image() {
    let bounds = Rect::new(0.0, 0.0, 64.0, 64.0);
    let node = Node::image(ImageId::new(42), bounds);
    match &node.kind {
        NodeKind::Image { image_id } => assert_eq!(image_id.get(), 42),
        _ => panic!("Expected Image"),
    }
}

#[test]
fn test_node_video() {
    let bounds = Rect::new(0.0, 0.0, 320.0, 240.0);
    let node = Node::video(VideoId::new(7), bounds);
    match &node.kind {
        NodeKind::Video { video_id } => assert_eq!(video_id.get(), 7),
        _ => panic!("Expected Video"),
    }
}

#[test]
fn test_node_cursor_creation() {
    let bounds = Rect::new(100.0, 50.0, 8.0, 16.0);
    let node = Node::cursor(SceneCursorStyle::Bar, Color::WHITE, bounds);
    match &node.kind {
        NodeKind::Cursor {
            style,
            color,
            blink_on,
        } => {
            assert_eq!(*style, SceneCursorStyle::Bar);
            assert_eq!(*color, Color::WHITE);
            assert!(*blink_on);
        }
        _ => panic!("Expected Cursor"),
    }
    assert_eq!(node.bounds, bounds);
}

// ---------------------------------------------------------------
// Node builder methods (with_opacity, with_transform, with_clip)
// ---------------------------------------------------------------

#[test]
fn test_node_builder_methods() {
    let bounds = Rect::new(0.0, 0.0, 50.0, 50.0);
    let clip = Rect::new(5.0, 5.0, 40.0, 40.0);
    let transform = Transform::translate(10.0, 20.0);
    let node = Node::color_rect(bounds, Color::GREEN)
        .with_opacity(0.5)
        .with_transform(transform)
        .with_clip(clip);

    assert_eq!(node.opacity, 0.5);
    assert_eq!(node.transform, Some(transform));
    assert_eq!(node.clip, Some(clip));
}

// ---------------------------------------------------------------
// Node tree building and traversal
// ---------------------------------------------------------------

#[test]
fn test_container_with_children() {
    let child1 = Node::color_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::RED);
    let child2 = Node::color_rect(Rect::new(10.0, 0.0, 10.0, 10.0), Color::BLUE);
    let container = Node::container(vec![child1, child2]);

    match &container.kind {
        NodeKind::Container { children } => {
            assert_eq!(children.len(), 2);
            // First child is red
            match &children[0].kind {
                NodeKind::ColorRect { color } => assert_eq!(*color, Color::RED),
                _ => panic!("Expected ColorRect"),
            }
            // Second child is blue
            match &children[1].kind {
                NodeKind::ColorRect { color } => assert_eq!(*color, Color::BLUE),
                _ => panic!("Expected ColorRect"),
            }
        }
        _ => panic!("Expected Container"),
    }
}

#[test]
fn test_container_with_transform() {
    let child = Node::color_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::RED);
    let transform = Transform::translate(100.0, 200.0);
    let container = Node::container_with_transform(vec![child], transform);

    assert_eq!(container.transform, Some(transform));
    match &container.kind {
        NodeKind::Container { children } => assert_eq!(children.len(), 1),
        _ => panic!("Expected Container"),
    }
}

#[test]
fn test_nested_containers() {
    let leaf = Node::color_rect(Rect::new(0.0, 0.0, 5.0, 5.0), Color::WHITE);
    let inner = Node::container(vec![leaf]);
    let outer = Node::container(vec![inner]);

    match &outer.kind {
        NodeKind::Container { children } => {
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::Container {
                    children: inner_children,
                } => {
                    assert_eq!(inner_children.len(), 1);
                    match &inner_children[0].kind {
                        NodeKind::ColorRect { color } => assert_eq!(*color, Color::WHITE),
                        _ => panic!("Expected ColorRect leaf"),
                    }
                }
                _ => panic!("Expected inner Container"),
            }
        }
        _ => panic!("Expected outer Container"),
    }
}

// ---------------------------------------------------------------
// Dirty region tracking and clearing
// ---------------------------------------------------------------

#[test]
fn test_mark_dirty_covers_full_scene() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.mark_dirty();
    let dirty = scene.dirty.unwrap();
    assert_eq!(dirty.x, 0.0);
    assert_eq!(dirty.y, 0.0);
    assert_eq!(dirty.width, 800.0);
    assert_eq!(dirty.height, 600.0);
}

#[test]
fn test_clear_dirty() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.mark_dirty();
    assert!(scene.dirty.is_some());
    scene.clear_dirty();
    assert!(scene.dirty.is_none());
}

#[test]
fn test_mark_region_dirty_first_region() {
    let mut scene = Scene::new(800.0, 600.0);
    assert!(scene.dirty.is_none());
    scene.mark_region_dirty(Rect::new(50.0, 60.0, 70.0, 80.0));
    let dirty = scene.dirty.unwrap();
    assert_eq!(dirty.x, 50.0);
    assert_eq!(dirty.y, 60.0);
    assert_eq!(dirty.width, 70.0);
    assert_eq!(dirty.height, 80.0);
}

#[test]
fn test_mark_region_dirty_disjoint_union() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.mark_region_dirty(Rect::new(0.0, 0.0, 10.0, 10.0));
    scene.mark_region_dirty(Rect::new(200.0, 300.0, 50.0, 50.0));
    let dirty = scene.dirty.unwrap();
    // Union bounding box
    assert_eq!(dirty.x, 0.0);
    assert_eq!(dirty.y, 0.0);
    assert_eq!(dirty.right(), 250.0);
    assert_eq!(dirty.bottom(), 350.0);
}

// ---------------------------------------------------------------
// Window / scene relationship
// ---------------------------------------------------------------

/// Helper to create a basic WindowScene
fn make_window(id: i32, x: f32, y: f32, w: f32, h: f32) -> WindowScene {
    WindowScene {
        window_id: DisplayWindowId::new(id as i64),
        bounds: Rect::new(x, y, w, h),
        background: Color::BLACK,
        cursor: None,
        scroll_offset: 0.0,
        selected: false,
        mode_line_height: 20,
        header_line_height: 0,
        last_frame_touched: 0,
    }
}

#[test]
fn test_scene_build_with_no_windows() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.background = Color::WHITE;
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            // Only the background rect
            assert_eq!(children.len(), 1);
            match &children[0].kind {
                NodeKind::ColorRect { color } => assert_eq!(*color, Color::WHITE),
                _ => panic!("Expected background ColorRect"),
            }
        }
        _ => panic!("Expected root Container"),
    }
}

#[test]
fn test_scene_build_with_windows() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.windows.push(make_window(1, 0.0, 0.0, 400.0, 600.0));
    scene.windows.push(make_window(2, 400.0, 0.0, 400.0, 600.0));
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            // 1 background + 2 windows = 3 children
            assert_eq!(children.len(), 3);
            // children[1] and children[2] should be window containers with transforms
            for i in 1..=2 {
                assert!(
                    children[i].transform.is_some(),
                    "Window node must have transform"
                );
                assert!(children[i].clip.is_some(), "Window node must have clip");
            }
        }
        _ => panic!("Expected root Container"),
    }
}

#[test]
fn test_scene_build_window_transform_and_clip() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.windows.push(make_window(1, 50.0, 30.0, 300.0, 200.0));
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            let win_node = &children[1]; // first window node (after background)
            // Transform should translate to window position
            let transform = win_node.transform.unwrap();
            // translate(50.0, 30.0 - scroll_offset=0.0)
            assert_eq!(transform, Transform::translate(50.0, 30.0));
            // Clip should match window bounds
            let clip = win_node.clip.unwrap();
            assert_eq!(clip, Rect::new(50.0, 30.0, 300.0, 200.0));
        }
        _ => panic!("Expected root Container"),
    }
}

#[test]
fn test_scene_build_window_with_scroll_offset() {
    let mut scene = Scene::new(800.0, 600.0);
    let mut win = make_window(1, 0.0, 0.0, 400.0, 300.0);
    win.scroll_offset = 25.0;
    scene.windows.push(win);
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            let transform = children[1].transform.unwrap();
            // translate(0.0, 0.0 - 25.0) = translate(0.0, -25.0)
            assert_eq!(transform, Transform::translate(0.0, -25.0));
        }
        _ => panic!("Expected root Container"),
    }
}

// ---------------------------------------------------------------
// Cursor scene updates
// ---------------------------------------------------------------

#[test]
fn test_scene_build_window_with_visible_cursor() {
    let mut scene = Scene::new(800.0, 600.0);
    let mut win = make_window(1, 0.0, 0.0, 400.0, 300.0);
    win.cursor = Some(CursorState {
        x: 100.0,
        y: 50.0,
        width: 8.0,
        height: 16.0,
        style: SceneCursorStyle::Box,
        color: Color::GREEN,
        visible: true,
    });
    scene.windows.push(win);
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            let win_node = &children[1];
            match &win_node.kind {
                NodeKind::Container {
                    children: win_children,
                } => {
                    // background rect + cursor = 2
                    assert_eq!(win_children.len(), 2);
                    match &win_children[1].kind {
                        NodeKind::Cursor {
                            style,
                            color,
                            blink_on,
                        } => {
                            assert_eq!(*style, SceneCursorStyle::Box);
                            assert_eq!(*color, Color::GREEN);
                            assert!(*blink_on);
                        }
                        _ => panic!("Expected Cursor node"),
                    }
                    // Cursor bounds
                    assert_eq!(win_children[1].bounds, Rect::new(100.0, 50.0, 8.0, 16.0));
                }
                _ => panic!("Expected window Container"),
            }
        }
        _ => panic!("Expected root Container"),
    }
}

#[test]
fn test_scene_build_window_with_invisible_cursor() {
    let mut scene = Scene::new(800.0, 600.0);
    let mut win = make_window(1, 0.0, 0.0, 400.0, 300.0);
    win.cursor = Some(CursorState {
        x: 100.0,
        y: 50.0,
        width: 8.0,
        height: 16.0,
        style: SceneCursorStyle::Bar,
        color: Color::WHITE,
        visible: false,
    });
    scene.windows.push(win);
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            let win_node = &children[1];
            match &win_node.kind {
                NodeKind::Container {
                    children: win_children,
                } => {
                    // Only background rect, no cursor
                    assert_eq!(win_children.len(), 1);
                }
                _ => panic!("Expected window Container"),
            }
        }
        _ => panic!("Expected root Container"),
    }
}

#[test]
fn test_cursor_style_default() {
    assert_eq!(SceneCursorStyle::default(), SceneCursorStyle::Box);
}

#[test]
fn test_all_cursor_styles() {
    let styles = [
        SceneCursorStyle::Box,
        SceneCursorStyle::Bar,
        SceneCursorStyle::Underline,
        SceneCursorStyle::Hollow,
    ];
    let bounds = Rect::new(0.0, 0.0, 8.0, 16.0);
    for style in &styles {
        let node = Node::cursor(*style, Color::WHITE, bounds);
        match &node.kind {
            NodeKind::Cursor { style: s, .. } => assert_eq!(s, style),
            _ => panic!("Expected Cursor"),
        }
    }
}

// ---------------------------------------------------------------
// Layer ordering (borders on top)
// ---------------------------------------------------------------

#[test]
fn test_scene_build_borders_on_top_of_windows() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.windows.push(make_window(1, 0.0, 0.0, 400.0, 600.0));
    scene.add_border(400.0, 0.0, 2.0, 600.0, Color::WHITE);
    scene.build();

    let root = scene.root.as_ref().unwrap();
    match &root.kind {
        NodeKind::Container { children } => {
            // 1 background + 1 window + 1 border = 3
            assert_eq!(children.len(), 3);
            // Last child must be the border
            match &children[2].kind {
                NodeKind::ColorRect { color } => assert_eq!(*color, Color::WHITE),
                _ => panic!("Expected border ColorRect as last child"),
            }
            assert_eq!(children[2].bounds, Rect::new(400.0, 0.0, 2.0, 600.0));
        }
        _ => panic!("Expected root Container"),
    }
}

#[test]
fn test_scene_clear_borders() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.add_border(0.0, 0.0, 2.0, 600.0, Color::WHITE);
    scene.add_border(400.0, 0.0, 2.0, 600.0, Color::RED);
    assert_eq!(scene.borders.len(), 2);

    scene.clear_borders();
    assert!(scene.borders.is_empty());
}

// ---------------------------------------------------------------
// Floating video / image management
// ---------------------------------------------------------------

#[test]
fn test_floating_video_add_remove_clear() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.add_floating_video(VideoId::new(1), 10.0, 20.0, 320.0, 240.0);
    scene.add_floating_video(VideoId::new(2), 400.0, 20.0, 320.0, 240.0);
    assert_eq!(scene.floating_videos.len(), 2);

    scene.remove_floating_video(VideoId::new(1));
    assert_eq!(scene.floating_videos.len(), 1);
    assert_eq!(scene.floating_videos[0].video_id.get(), 2);

    scene.clear_floating_videos();
    assert!(scene.floating_videos.is_empty());
}

#[test]
fn test_floating_image_add_remove_clear() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.add_floating_image(ImageId::new(10), 0.0, 0.0, 128.0, 128.0);
    assert_eq!(scene.floating_images.len(), 1);
    assert_eq!(scene.floating_images[0].image_id.get(), 10);

    scene.remove_floating_image(ImageId::new(10));
    assert!(scene.floating_images.is_empty());

    scene.add_floating_image(ImageId::new(20), 0.0, 0.0, 64.0, 64.0);
    scene.add_floating_image(ImageId::new(21), 64.0, 0.0, 64.0, 64.0);
    scene.clear_floating_images();
    assert!(scene.floating_images.is_empty());
}

#[test]
fn test_remove_nonexistent_floating_is_noop() {
    let mut scene = Scene::new(800.0, 600.0);
    scene.add_floating_video(VideoId::new(1), 0.0, 0.0, 100.0, 100.0);
    // Removing a non-existent ID should not panic or change existing entries
    scene.remove_floating_video(VideoId::new(999));
    assert_eq!(scene.floating_videos.len(), 1);
    scene.remove_floating_image(ImageId::new(999));
}

// ---------------------------------------------------------------
// Mutations mark dirty
// ---------------------------------------------------------------

#[test]
fn test_mutations_mark_dirty() {
    let mut scene = Scene::new(800.0, 600.0);

    // add_floating_video marks dirty
    scene.add_floating_video(VideoId::new(1), 0.0, 0.0, 100.0, 100.0);
    assert!(scene.dirty.is_some());
    scene.clear_dirty();

    // remove_floating_video marks dirty
    scene.remove_floating_video(VideoId::new(1));
    assert!(scene.dirty.is_some());
    scene.clear_dirty();

    // add_floating_image marks dirty
    scene.add_floating_image(ImageId::new(1), 0.0, 0.0, 100.0, 100.0);
    assert!(scene.dirty.is_some());
    scene.clear_dirty();

    // add_border marks dirty
    scene.add_border(0.0, 0.0, 2.0, 600.0, Color::WHITE);
    assert!(scene.dirty.is_some());
    scene.clear_dirty();

    // clear marks dirty
    scene.clear();
    assert!(scene.dirty.is_some());
}

// ---------------------------------------------------------------
// WindowScene selected field
// ---------------------------------------------------------------

#[test]
fn test_window_selected_flag() {
    let mut win = make_window(1, 0.0, 0.0, 400.0, 300.0);
    assert!(!win.selected);
    win.selected = true;
    assert!(win.selected);
}
