//! Integration tests for threaded display mode.
//!
//! Tests the two-thread architecture communication infrastructure.

use std::thread;
use std::time::Duration;

use neomacs_display::thread_comm::{InputEvent, RenderCommand, ThreadComms};
use neomacs_display::core::frame_glyphs::FrameGlyphBuffer;
use neomacs_display::core::types::Color;

#[test]
fn test_thread_comms_creation() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");

    // Verify we can access channels before split
    assert!(comms.frame_rx.is_empty());
    assert!(comms.cmd_rx.is_empty());
    assert!(comms.input_rx.is_empty());
}

#[test]
fn test_thread_comms_split() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Verify both sides have access to their channels
    assert!(emacs.input_rx.is_empty());
    assert!(render.cmd_rx.is_empty());
    assert!(render.frame_rx.is_empty());
}

#[test]
fn test_wakeup_pipe() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Wake up from render side
    render.wakeup.wake();

    // Clear from emacs side (should not block)
    emacs.wakeup_clear.clear();

    // Multiple wakes and clears should work
    render.wakeup.wake();
    render.wakeup.wake();
    emacs.wakeup_clear.clear();
}

#[test]
fn test_wakeup_fd_valid() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, _render) = comms.split();

    // The read_fd should be a valid file descriptor (non-negative)
    assert!(emacs.wakeup_read_fd >= 0);
}

#[test]
fn test_command_channel() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Send shutdown command
    emacs.cmd_tx.send(RenderCommand::Shutdown).unwrap();

    // Receive on render side
    let cmd = render.cmd_rx.recv().unwrap();
    assert!(matches!(cmd, RenderCommand::Shutdown));
}

#[test]
fn test_command_channel_webkit_create() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    emacs
        .cmd_tx
        .send(RenderCommand::WebKitCreate {
            id: 42,
            width: 800,
            height: 600,
        })
        .unwrap();

    let cmd = render.cmd_rx.recv().unwrap();
    match cmd {
        RenderCommand::WebKitCreate { id, width, height } => {
            assert_eq!(id, 42);
            assert_eq!(width, 800);
            assert_eq!(height, 600);
        }
        _ => panic!("Expected WebKitCreate command"),
    }
}

#[test]
fn test_command_channel_video_commands() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Send a series of video commands
    emacs
        .cmd_tx
        .send(RenderCommand::VideoCreate {
            id: 1,
            path: "/path/to/video.mp4".to_string(),
        })
        .unwrap();
    emacs.cmd_tx.send(RenderCommand::VideoPlay { id: 1 }).unwrap();
    emacs.cmd_tx.send(RenderCommand::VideoPause { id: 1 }).unwrap();
    emacs.cmd_tx.send(RenderCommand::VideoDestroy { id: 1 }).unwrap();

    // Verify all commands arrive in order
    match render.cmd_rx.recv().unwrap() {
        RenderCommand::VideoCreate { id, path } => {
            assert_eq!(id, 1);
            assert_eq!(path, "/path/to/video.mp4");
        }
        _ => panic!("Expected VideoCreate"),
    }

    assert!(matches!(
        render.cmd_rx.recv().unwrap(),
        RenderCommand::VideoPlay { id: 1 }
    ));
    assert!(matches!(
        render.cmd_rx.recv().unwrap(),
        RenderCommand::VideoPause { id: 1 }
    ));
    assert!(matches!(
        render.cmd_rx.recv().unwrap(),
        RenderCommand::VideoDestroy { id: 1 }
    ));
}

#[test]
fn test_input_event_channel() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Send input event from render side
    render.send_input(InputEvent::Key {
        keysym: 0xff0d, // Enter
        modifiers: 0,
        pressed: true,
    });

    // Receive on emacs side
    let event = emacs.input_rx.recv().unwrap();
    match event {
        InputEvent::Key {
            keysym,
            modifiers,
            pressed,
        } => {
            assert_eq!(keysym, 0xff0d);
            assert_eq!(modifiers, 0);
            assert!(pressed);
        }
        _ => panic!("Expected Key event"),
    }
}

#[test]
fn test_input_event_mouse() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    render.send_input(InputEvent::MouseButton {
        button: 1,
        x: 100.0,
        y: 200.0,
        pressed: true,
        modifiers: 0,
        target_frame_id: 0,
    });

    render.send_input(InputEvent::MouseMove {
        x: 150.0,
        y: 250.0,
        modifiers: 0,
        target_frame_id: 0,
    });

    render.send_input(InputEvent::MouseScroll {
        delta_x: 0.0,
        delta_y: -3.0,
        x: 150.0,
        y: 250.0,
        modifiers: 0,
        pixel_precise: false,
        target_frame_id: 0,
    });

    // Verify all events
    match emacs.input_rx.recv().unwrap() {
        InputEvent::MouseButton {
            button, x, y, pressed, ..
        } => {
            assert_eq!(button, 1);
            assert_eq!(x, 100.0);
            assert_eq!(y, 200.0);
            assert!(pressed);
        }
        _ => panic!("Expected MouseButton event"),
    }

    match emacs.input_rx.recv().unwrap() {
        InputEvent::MouseMove { x, y, .. } => {
            assert_eq!(x, 150.0);
            assert_eq!(y, 250.0);
        }
        _ => panic!("Expected MouseMove event"),
    }

    match emacs.input_rx.recv().unwrap() {
        InputEvent::MouseScroll { delta_y, .. } => {
            assert_eq!(delta_y, -3.0);
        }
        _ => panic!("Expected MouseScroll event"),
    }
}

#[test]
fn test_input_event_window() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    render.send_input(InputEvent::WindowResize {
        width: 1920,
        height: 1080,
        emacs_frame_id: 0,
    });

    render.send_input(InputEvent::WindowFocus { focused: true, emacs_frame_id: 0 });

    render.send_input(InputEvent::WindowClose { emacs_frame_id: 0 });

    match emacs.input_rx.recv().unwrap() {
        InputEvent::WindowResize { width, height, .. } => {
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
        }
        _ => panic!("Expected WindowResize event"),
    }

    match emacs.input_rx.recv().unwrap() {
        InputEvent::WindowFocus { focused, .. } => {
            assert!(focused);
        }
        _ => panic!("Expected WindowFocus event"),
    }

    assert!(matches!(
        emacs.input_rx.recv().unwrap(),
        InputEvent::WindowClose { .. }
    ));
}

#[test]
fn test_frame_channel() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Create a frame buffer
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.background = Color::rgb(0.1, 0.1, 0.1);

    // Add some glyphs
    frame.set_face(0, Color::WHITE, None, 400, false, 0, None, 0, None, 0, None);
    frame.add_char('H', 0.0, 0.0, 10.0, 20.0, 16.0, false);
    frame.add_char('i', 10.0, 0.0, 10.0, 20.0, 16.0, false);

    // Send frame
    emacs.frame_tx.send(frame).unwrap();

    // Receive on render side
    let received = render.frame_rx.recv().unwrap();
    assert_eq!(received.width, 800.0);
    assert_eq!(received.height, 600.0);
    assert_eq!(received.glyphs.len(), 2);
}

#[test]
fn test_frame_channel_unbounded() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Frame channel is unbounded — can send multiple frames without blocking
    let frame1 = FrameGlyphBuffer::with_size(800.0, 600.0);
    let frame2 = FrameGlyphBuffer::with_size(800.0, 600.0);
    let frame3 = FrameGlyphBuffer::with_size(800.0, 600.0);

    emacs.frame_tx.send(frame1).unwrap();
    emacs.frame_tx.send(frame2).unwrap();
    emacs.frame_tx.send(frame3).unwrap();

    // All three should be receivable
    let _ = render.frame_rx.recv().unwrap();
    let _ = render.frame_rx.recv().unwrap();
    let _ = render.frame_rx.recv().unwrap();
}

#[test]
fn test_cross_thread_communication() {
    let comms = ThreadComms::new().expect("Failed to create ThreadComms");
    let (emacs, render) = comms.split();

    // Spawn a "render thread" that sends events
    let render_handle = thread::spawn(move || {
        // Simulate render thread sending events
        for i in 0..10 {
            render.send_input(InputEvent::Key {
                keysym: 0x61 + i, // 'a' through 'j'
                modifiers: 0,
                pressed: true,
            });
        }

        // Wait for shutdown command
        loop {
            if let Ok(cmd) = render.cmd_rx.recv_timeout(Duration::from_millis(100)) {
                if matches!(cmd, RenderCommand::Shutdown) {
                    break;
                }
            }
        }
    });

    // Emacs side: receive all events
    let mut received = 0;
    while let Ok(_event) = emacs.input_rx.recv_timeout(Duration::from_millis(100)) {
        received += 1;
        if received >= 10 {
            break;
        }
    }

    assert_eq!(received, 10);

    // Send shutdown
    emacs.cmd_tx.send(RenderCommand::Shutdown).unwrap();

    // Join render thread
    render_handle.join().unwrap();
}

#[test]
fn test_frame_glyph_buffer_operations() {
    let mut buffer = FrameGlyphBuffer::new();

    // Test initial state
    assert!(buffer.is_empty());
    assert_eq!(buffer.len(), 0);

    // Begin frame
    buffer.begin_frame(1024.0, 768.0, Color::BLACK);
    assert_eq!(buffer.width, 1024.0);
    assert_eq!(buffer.height, 768.0);

    // Add background
    buffer.add_background(0.0, 0.0, 500.0, 300.0, Color::rgb(0.2, 0.2, 0.2));
    assert_eq!(buffer.len(), 1);

    // Set face and add chars
    buffer.set_face_with_font(
        1,
        Color::WHITE,
        Some(Color::BLACK),
        "monospace",
        400,     // font_weight
        false,   // italic
        14.0,    // font_size
        0,       // underline
        None,    // underline_color
        0,       // strike_through
        None,    // strike_through_color
        0,       // overline
        None,    // overline_color
    );

    buffer.add_char('T', 0.0, 0.0, 10.0, 20.0, 16.0, false);
    buffer.add_char('e', 10.0, 0.0, 10.0, 20.0, 16.0, false);
    buffer.add_char('s', 20.0, 0.0, 10.0, 20.0, 16.0, false);
    buffer.add_char('t', 30.0, 0.0, 10.0, 20.0, 16.0, false);

    assert_eq!(buffer.len(), 5); // 1 background + 4 chars

    // Add cursor
    buffer.add_cursor(1, 40.0, 0.0, 2.0, 20.0, 2, Color::WHITE);
    assert_eq!(buffer.len(), 6);

    // Verify font family lookup
    assert_eq!(buffer.get_face_font(1), "monospace");
    assert_eq!(buffer.get_face_font(999), "monospace"); // default
}

#[test]
fn test_frame_glyph_buffer_append_model() {
    let mut buffer = FrameGlyphBuffer::new();
    buffer.begin_frame(800.0, 600.0, Color::BLACK);

    // Set face
    buffer.set_face(0, Color::WHITE, None, 400, false, 0, None, 0, None, 0, None);

    // Full-frame rebuild: add_char always appends (no overlap tracking)
    buffer.add_char('A', 10.0, 10.0, 10.0, 20.0, 16.0, false);
    assert_eq!(buffer.len(), 1);

    buffer.add_char('B', 20.0, 10.0, 10.0, 20.0, 16.0, false);
    assert_eq!(buffer.len(), 2);

    // Verify both chars present
    if let neomacs_display::core::frame_glyphs::FrameGlyph::Char { char, .. } = &buffer.glyphs[0] {
        assert_eq!(*char, 'A');
    } else {
        panic!("Expected Char glyph");
    }
    if let neomacs_display::core::frame_glyphs::FrameGlyph::Char { char, .. } = &buffer.glyphs[1] {
        assert_eq!(*char, 'B');
    } else {
        panic!("Expected Char glyph");
    }
}

#[test]
fn test_frame_glyph_buffer_clear_all() {
    let mut buffer = FrameGlyphBuffer::new();
    buffer.begin_frame(800.0, 600.0, Color::BLACK);

    buffer.set_face(0, Color::WHITE, None, 400, false, 0, None, 0, None, 0, None);

    // Add some chars
    buffer.add_char('A', 0.0, 0.0, 10.0, 20.0, 16.0, false);
    buffer.add_char('B', 10.0, 0.0, 10.0, 20.0, 16.0, false);
    buffer.add_char('C', 20.0, 0.0, 10.0, 20.0, 16.0, false);
    buffer.add_char('D', 0.0, 30.0, 10.0, 20.0, 16.0, false); // Different row

    assert_eq!(buffer.len(), 4);

    // clear_all removes all glyphs (full-frame rebuild model)
    buffer.clear_all();

    assert_eq!(buffer.len(), 0);
}

#[test]
fn test_frame_glyph_buffer_cursor_append() {
    let mut buffer = FrameGlyphBuffer::new();
    buffer.begin_frame(800.0, 600.0, Color::BLACK);

    // Full-frame rebuild: add_cursor always appends
    buffer.add_cursor(1, 10.0, 10.0, 2.0, 20.0, 0, Color::WHITE);
    buffer.add_cursor(2, 100.0, 100.0, 2.0, 20.0, 0, Color::WHITE);

    assert_eq!(buffer.len(), 2);

    // Adding another cursor appends (frame is rebuilt each time)
    buffer.add_cursor(3, 200.0, 200.0, 2.0, 20.0, 0, Color::WHITE);
    assert_eq!(buffer.len(), 3);
}

// Test that requires windowing system - mark as ignored for CI
#[test]
#[ignore = "Requires display server (X11/Wayland)"]
#[cfg(feature = "winit-backend")]
fn test_render_thread_lifecycle() {
    use neomacs_display::render_thread::RenderThread;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;

    let comms = ThreadComms::new().expect("Failed to create comms");
    let (emacs, render) = comms.split();

    let image_dimensions = Arc::new(Mutex::new(HashMap::new()));
    let shared_monitors = Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));

    // Spawn render thread
    let rt = RenderThread::spawn(
        render, 800, 600, "Test Window".to_string(),
        image_dimensions,
        shared_monitors,
        #[cfg(feature = "neo-term")]
        Arc::new(Mutex::new(HashMap::new())),
    );

    // Give it time to start
    thread::sleep(Duration::from_millis(200));

    // Send shutdown
    emacs.cmd_tx.send(RenderCommand::Shutdown).unwrap();

    // Join - should complete without hanging
    rt.join();
}

// Test render thread with frame data - also requires display
#[test]
#[ignore = "Requires display server (X11/Wayland)"]
#[cfg(feature = "winit-backend")]
fn test_render_thread_with_frames() {
    use neomacs_display::render_thread::RenderThread;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;

    let comms = ThreadComms::new().expect("Failed to create comms");
    let (emacs, render) = comms.split();

    let image_dimensions = Arc::new(Mutex::new(HashMap::new()));
    let shared_monitors = Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));

    let rt = RenderThread::spawn(
        render, 800, 600, "Test Frame Render".to_string(),
        image_dimensions,
        shared_monitors,
        #[cfg(feature = "neo-term")]
        Arc::new(Mutex::new(HashMap::new())),
    );

    // Wait for window to be ready
    thread::sleep(Duration::from_millis(300));

    // Send a few frames
    for i in 0..5 {
        let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
        frame.background = Color::rgb(0.1 * i as f32, 0.1, 0.1);
        frame.set_face(0, Color::WHITE, None, 400, false, 0, None, 0, None, 0, None);
        frame.add_char('X', 100.0, 100.0, 10.0, 20.0, 16.0, false);

        if emacs.frame_tx.try_send(frame).is_err() {
            // Channel full, that's OK
        }

        thread::sleep(Duration::from_millis(50));
    }

    // Shutdown
    emacs.cmd_tx.send(RenderCommand::Shutdown).unwrap();
    rt.join();
}
