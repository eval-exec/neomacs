//! Native menu interaction and compositor placement contracts.

use super::*;

// Coordinates below are compositor screenshot pixels, not menu layout telemetry.
#[derive(Clone, Copy, Debug)]
struct ScreenRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}
impl ScreenRect {
    fn right(self) -> u32 {
        self.x + self.width
    }
    fn bottom(self) -> u32 {
        self.y + self.height
    }
}

fn capture(env: &[(String, String)], artifacts: &Path, name: &str) -> image::RgbImage {
    let path = artifacts.join(format!("{name}.png"));
    let output = Command::new("import")
        .args(["-window", "root"])
        .arg(&path)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    image::open(path).unwrap().to_rgb8()
}

fn stable_screen(env: &[(String, String)], artifacts: &Path, name: &str) -> image::RgbImage {
    // These fixtures disable cursor blinking. Observe the whole compositor,
    // including edge popups outside the old top-left screenshot crop.
    thread::sleep(Duration::from_millis(150));
    let mut previous = None;
    wait_for("stable compositor screenshot", artifacts, || {
        let pixels = capture(env, artifacts, name);
        let stable = previous.as_ref() == Some(&pixels);
        previous = Some(pixels.clone());
        stable.then_some(pixels)
    })
}

fn changed_rectangle(
    before: &image::RgbImage,
    after: &image::RgbImage,
    region: ScreenRect,
) -> Option<ScreenRect> {
    let (mut left, mut top, mut right, mut bottom) = (region.right(), region.bottom(), 0, 0);
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            if before
                .get_pixel(x, y)
                .0
                .iter()
                .zip(after.get_pixel(x, y).0)
                .any(|(a, b)| a.abs_diff(b) > 8)
            {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    (right > left && bottom > top).then(|| ScreenRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn highlighted_row(before: &image::RgbImage, after: &image::RgbImage, panel: ScreenRect) -> u32 {
    // A row highlight changes most of the panel width. A moving native cursor
    // may change a few pixels elsewhere, and must not become the row anchor.
    (panel.y..panel.bottom())
        .find(|&y| {
            let changed = (panel.x..panel.right())
                .filter(|&x| {
                    before
                        .get_pixel(x, y)
                        .0
                        .iter()
                        .zip(after.get_pixel(x, y).0)
                        .any(|(a, b)| a.abs_diff(b) > 8)
                })
                .count();
            changed > panel.width as usize / 2
        })
        .expect("visible highlighted parent row")
}

fn click_at(env: &[(String, String)], window: &str, x: u32, y: u32) {
    input(
        env,
        &[
            "mousemove",
            "--window",
            window,
            &x.to_string(),
            &y.to_string(),
            "click",
            "1",
        ],
    );
}
fn events(artifacts: &Path) -> Vec<String> {
    fs::read(artifacts.join("events.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[test]
fn dropdown_is_below_heading_and_mouse_selection_runs_once() {
    check_dropdown(1);
}

#[test]
fn hidpi_dropdown_is_below_heading_and_mouse_selection_runs_once() {
    check_dropdown(2);
}

use super::popup_trace::{self, ParentRect};

fn configured_popups(artifacts: &Path) -> Vec<ParentRect> {
    popup_trace::configured(&fs::read_to_string(artifacts.join("wayland.log")).unwrap())
}

fn check_dropdown(scale: u32) {
    with_native_menu_at_scale(
        "menu-interaction.el",
        scale,
        |env, window, ready, artifacts, editor, _| {
            thread::sleep(Duration::from_millis(200));
            let before = capture(env, artifacts, "closed");
            click_at(
                env,
                window,
                (16 + ready.char_width * 6) * scale,
                ready.char_height * scale / 2,
            );
            wait_for("popup", artifacts, || {
                (popup_requests(artifacts) > 0).then_some(())
            });
            let _ = stable_screen(env, artifacts, "open");
            let after = capture(env, artifacts, "dropdown");
            let bounds = changed_rectangle(
                &before,
                &after,
                ScreenRect {
                    x: 0,
                    y: ready.char_height * scale,
                    width: 600,
                    height: 400,
                },
            )
            .expect("visible dropdown");
            let positions = configured_popups(artifacts);
            let position = positions.first().expect("compositor popup configuration");
            assert_eq!(
                (position.x, position.y),
                (8, ready.char_height as i32),
                "popup must not overlap heading: {position:?}"
            );
            assert!((bounds.width as i32 - position.width * scale as i32).abs() <= 1);
            assert!((bounds.height as i32 - position.height * scale as i32).abs() <= 1);
            // The fixture's first heading begins at the frame's 8px menu margin.
            assert_eq!(
                bounds.x,
                8 * scale,
                "dropdown must align with heading: {bounds:?}; {}",
                artifacts.display()
            );
            assert_eq!(
                bounds.y,
                ready.char_height * scale,
                "dropdown must touch menu-bar bottom: {bounds:?}; {}",
                artifacts.display()
            );
            click_at(env, window, bounds.x + 30 * scale, bounds.y + 10 * scale);
            wait_for("one command", artifacts, || {
                alive(editor, "selecting item", artifacts);
                (events(artifacts) == ["ready", "choose"]).then_some(())
            });
            input(env, &["type", "--clearmodifiers", "menu-focus"]);
            wait_for("editor focus after selection", artifacts, || {
                (fs::read_to_string(artifacts.join("text")).ok().as_deref() == Some("menu-focus"))
                    .then_some(())
            });
            assert_eq!(events(artifacts), ["ready", "choose"]);
        },
    );
}

#[test]
fn nested_menu_touches_parent_row_and_mouse_can_cross_into_it() {
    check_nested(1);
}

#[test]
fn hidpi_nested_menu_touches_parent_row_and_mouse_can_cross_into_it() {
    check_nested(2);
}

fn check_nested(scale: u32) {
    with_native_menu_at_scale(
        "menu-interaction.el",
        scale,
        |env, window, ready, artifacts, editor, _| {
            thread::sleep(Duration::from_millis(200));
            let closed = capture(env, artifacts, "closed");
            click_at(env, window, 30 * scale, ready.char_height * scale / 2);
            let _ = stable_screen(env, artifacts, "root");
            let root_image = capture(env, artifacts, "root-full");
            let root = changed_rectangle(
                &closed,
                &root_image,
                ScreenRect {
                    x: 0,
                    y: ready.char_height * scale,
                    width: 600,
                    height: 400,
                },
            )
            .expect("visible root menu");
            // Second row: one 4px panel inset followed by one text row + spacing.
            let row_top = root.y + (4 + ready.char_height + 3) * scale;
            input(
                env,
                &[
                    "mousemove",
                    "--window",
                    window,
                    &(root.x + 30 * scale).to_string(),
                    &(row_top + 8 * scale).to_string(),
                ],
            );
            wait_for("nested popup", artifacts, || {
                (popup_requests(artifacts) >= 2).then_some(())
            });
            let _ = stable_screen(env, artifacts, "nested");
            let nested_image = capture(env, artifacts, "nested-full");
            let positions = configured_popups(artifacts);
            let configured_parent = &positions[0];
            let configured_child = &positions[1];
            // The screenshot search excludes the parent. Independently check
            // the accepted origin so an overlapping child cannot pass merely
            // because that search clipped away the overlap.
            assert_eq!(
                configured_child.x, configured_parent.width,
                "submenu must not overlap parent: {positions:?}"
            );
            let highlighted = highlighted_row(
                &root_image,
                &nested_image,
                ScreenRect {
                    x: root.x + 1,
                    y: root.y + 1,
                    width: root.width - 2,
                    height: root.height - 2,
                },
            );
            assert_eq!(
                configured_child.y * scale as i32,
                (highlighted - root.y) as i32,
                "submenu must align with the rendered parent row: {positions:?}"
            );
            let child = changed_rectangle(
                &root_image,
                &nested_image,
                ScreenRect {
                    x: root.right(),
                    y: ready.char_height * scale,
                    width: 800 - root.right(),
                    height: 400,
                },
            )
            .expect("visible child menu");
            assert_eq!(
                child.x,
                root.right(),
                "submenu must touch parent: {root:?}, {child:?}; {}",
                artifacts.display()
            );
            assert_eq!(
                child.y,
                highlighted,
                "submenu must align with parent row: {child:?}; {}",
                artifacts.display()
            );
            // Traverse the boundary in small steps: a gap must not dismiss the child.
            for x in (root.x + 30 * scale..child.x + 30 * scale).step_by(4) {
                input(
                    env,
                    &[
                        "mousemove",
                        "--window",
                        window,
                        &x.to_string(),
                        &(row_top + 10 * scale).to_string(),
                    ],
                );
            }
            click_at(env, window, child.x + 30 * scale, child.y + 10 * scale);
            wait_for("nested command", artifacts, || {
                alive(editor, "crossing into submenu", artifacts);
                (events(artifacts) == ["ready", "nested"]).then_some(())
            });
        },
    );
}

#[derive(Clone, Copy, Debug)]
enum Dismissal {
    Escape,
    OutsideClick,
}

#[test]
fn cancel_menu_restores_typing_without_running_a_command() {
    with_native_menu(
        "menu-interaction.el",
        |env, window, ready, artifacts, _, _| {
            for dismissal in [Dismissal::Escape, Dismissal::OutsideClick] {
                input(env, &["mousemove", "--window", window, "500", "400"]);
                let closed = stable_screen(env, artifacts, &format!("cancel-{dismissal:?}-closed"));
                let previous = configured_popups(artifacts).len();
                click_at(env, window, 30, ready.char_height / 2);
                let position = wait_for("menu to open before cancellation", artifacts, || {
                    configured_popups(artifacts).get(previous).copied()
                });
                let region = ScreenRect {
                    x: position.x.try_into().unwrap(),
                    y: position.y.try_into().unwrap(),
                    width: position.width.try_into().unwrap(),
                    height: position.height.try_into().unwrap(),
                };
                input(env, &["mousemove", "--window", window, "500", "400"]);
                let opened = stable_screen(env, artifacts, &format!("cancel-{dismissal:?}-open"));
                let changed = (region.y..region.bottom())
                    .flat_map(|y| (region.x..region.right()).map(move |x| (x, y)))
                    .filter(|&(x, y)| {
                        closed
                            .get_pixel(x, y)
                            .0
                            .iter()
                            .zip(opened.get_pixel(x, y).0)
                            .any(|(a, b)| a.abs_diff(b) > 8)
                    })
                    .count();
                assert!(
                    changed > (region.width * region.height / 2) as usize,
                    "the popup panel must be visibly painted before cancellation"
                );
                match dismissal {
                    Dismissal::Escape => input(env, &["key", "Escape"]),
                    Dismissal::OutsideClick => click_at(env, window, 800, 400),
                }
                input(env, &["mousemove", "--window", window, "500", "400"]);
                let dismissed =
                    stable_screen(env, artifacts, &format!("cancel-{dismissal:?}-dismissed"));
                assert!(
                    changed_rectangle(&closed, &dismissed, region).is_none(),
                    "dismissal must restore the pixels behind the popup; {}",
                    artifacts.display()
                );
                input(env, &["type", "--clearmodifiers", "x"]);
                let expected = match dismissal {
                    Dismissal::Escape => "x",
                    Dismissal::OutsideClick => "xx",
                };
                wait_for("typing after dismissal", artifacts, || {
                    (fs::read_to_string(artifacts.join("text")).ok().as_deref() == Some(expected))
                        .then_some(())
                });
                assert_eq!(
                    events(artifacts),
                    ["ready"],
                    "dismissal must not select an item"
                );
            }
        },
    );
}

#[test]
fn keyboard_opens_submenu_and_skips_disabled_item() {
    with_native_menu(
        "menu-interaction.el",
        |env, window, ready, artifacts, _, _| {
            click_at(env, window, 30, ready.char_height / 2);
            let _ = stable_screen(env, artifacts, "keyboard-root");
            input(env, &["key", "Down", "Down", "Right", "Return"]);
            wait_for("keyboard nested action", artifacts, || {
                (events(artifacts) == ["ready", "nested"]).then_some(())
            });
            let mut unchecked_indicator = None;
            for (name, expected) in [
                ("checked", vec!["ready", "nested", "checked"]),
                ("unchecked", vec!["ready", "nested", "checked", "unchecked"]),
            ] {
                click_at(env, window, 30, ready.char_height / 2);
                let pixels = stable_screen(env, artifacts, name);
                // Crop the checkbox gutter only, excluding the label and other
                // rows. This checks the rendered state as well as Lisp state.
                let indicator = image::imageops::crop_imm(
                    &pixels,
                    9,
                    ready.char_height + 4 + 3 * (ready.char_height + 3),
                    ready.char_height,
                    ready.char_height,
                )
                .to_image();
                if let Some(unchecked) = &unchecked_indicator {
                    assert_ne!(
                        &indicator, unchecked,
                        "checked indicator must be visibly different"
                    );
                } else {
                    unchecked_indicator = Some(indicator);
                }
                input(env, &["key", "Down", "Down", "Down", "Return"]);
                wait_for("skip disabled and toggle", artifacts, || {
                    (events(artifacts) == expected).then_some(())
                });
            }
        },
    );
}

#[test]
fn pointer_switches_headings_and_disabled_click_does_not_execute() {
    with_native_menu(
        "menu-interaction.el",
        |env, window, ready, artifacts, _, _| {
            click_at(env, window, 30, ready.char_height / 2);
            let _ = stable_screen(env, artifacts, "disabled-open");
            click_at(
                env,
                window,
                40,
                ready.char_height + 4 + 2 * (ready.char_height + 3) + 8,
            );
            let _ = stable_screen(env, artifacts, "disabled-clicked");
            assert_eq!(events(artifacts), ["ready"]);
            input(env, &["key", "Escape"]);
            click_at(env, window, 30, ready.char_height / 2);
            let _ = stable_screen(env, artifacts, "switch-start");
            // Four double-width glyphs plus heading padding, then the Other heading.
            input(
                env,
                &[
                    "mousemove",
                    "--window",
                    window,
                    &(40 + ready.char_width * 8).to_string(),
                    &(ready.char_height / 2).to_string(),
                ],
            );
            let _ = stable_screen(env, artifacts, "switch-other");
            input(env, &["key", "Down", "Return"]);
            wait_for("other heading action", artifacts, || {
                (events(artifacts) == ["ready", "other"]).then_some(())
            });
        },
    );
}

#[derive(Clone, Copy, Debug)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[test]
fn popup_and_submenu_stay_on_screen_at_each_corner() {
    with_native_menu("menu-corners.el", |env, window, _, artifacts, _, _| {
        for corner in [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomLeft,
            Corner::BottomRight,
        ] {
            let (x, y) = match corner {
                Corner::TopLeft => (16, 60),
                Corner::TopRight => (980, 60),
                Corner::BottomLeft => (16, 640),
                Corner::BottomRight => (980, 640),
            };
            let before = configured_popups(artifacts).len();
            input(
                env,
                &[
                    "mousemove",
                    "--window",
                    window,
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "3",
                ],
            );
            wait_for("edge popup", artifacts, || {
                (configured_popups(artifacts).len() > before).then_some(())
            });
            // Clear the pointer highlight so only the selected row changes.
            input(env, &["mousemove", "--window", window, "500", "400"]);
            let root_pixels = stable_screen(env, artifacts, &format!("edge-{corner:?}-root"));
            input(env, &["key", "Home", "Down", "Right"]);
            wait_for("edge submenu", artifacts, || {
                (configured_popups(artifacts).len() > before + 1).then_some(())
            });
            let child_pixels = stable_screen(env, artifacts, &format!("edge-{corner:?}-child"));
            let positions = configured_popups(artifacts);
            let root = &positions[before];
            let child = &positions[before + 1];
            if matches!(corner, Corner::BottomLeft | Corner::BottomRight) {
                assert_eq!(
                    root.y + root.height,
                    700,
                    "bottom-edge menu should slide upward: {root:?}"
                );
            }
            let child_x = root.x + child.x;
            let child_y = root.y + child.y;
            for (x, y, width, height) in [
                (root.x, root.y, root.width, root.height),
                (child_x, child_y, child.width, child.height),
            ] {
                assert!(
                    x >= 0 && y >= 0 && x + width <= 1000 && y + height <= 700,
                    "{corner:?}: popup offscreen: {positions:?}; {}",
                    artifacts.display()
                );
            }
            assert!(
                child.x == root.width || child.x + child.width == 0,
                "{corner:?}: nested popup must touch either side: {positions:?}"
            );
            let highlighted = highlighted_row(
                &root_pixels,
                &child_pixels,
                ScreenRect {
                    x: root.x as u32 + 1,
                    y: root.y as u32 + 1,
                    width: root.width as u32 - 2,
                    height: root.height as u32 - 2,
                },
            );
            assert_eq!(
                child_y, highlighted as i32,
                "submenu must align with the rendered row after edge constraint"
            );
            if matches!(corner, Corner::TopRight | Corner::BottomRight) {
                assert!(child.x < 0, "right-edge submenu should flip left");
            }
            input(env, &["key", "Escape"]);
        }
        assert_eq!(events(artifacts), ["ready"]);
    });
}

#[test]
fn right_edge_heading_dropdown_stays_visible_and_submenu_flips_left() {
    with_native_menu(
        "menu-heading-edge.el",
        |env, window, ready, artifacts, _, _| {
            let _ = stable_screen(env, artifacts, "edge-heading-closed");
            click_at(env, window, 920, ready.char_height / 2);
            wait_for("edge heading popup", artifacts, || {
                (!configured_popups(artifacts).is_empty()).then_some(())
            });
            let root_pixels = stable_screen(env, artifacts, "edge-heading");
            input(env, &["key", "Home", "Down", "Right"]);
            wait_for("left-facing submenu", artifacts, || {
                (configured_popups(artifacts).len() >= 2).then_some(())
            });
            let child_pixels = stable_screen(env, artifacts, "edge-heading-child");
            let positions = configured_popups(artifacts);
            let root = &positions[0];
            let child = &positions[1];
            assert_eq!(root.y, ready.char_height as i32);
            assert!(
                root.x >= 0
                    && root.x < 920
                    && root.x + root.width >= 920
                    && root.x + root.width <= 1000,
                "constrained dropdown must stay onscreen and below the clicked heading: {root:?}"
            );
            assert_eq!(
                child.x + child.width,
                0,
                "child should touch parent's left edge: {child:?}"
            );
            let highlighted = highlighted_row(
                &root_pixels,
                &child_pixels,
                ScreenRect {
                    x: root.x as u32 + 1,
                    y: root.y as u32 + 1,
                    width: root.width as u32 - 2,
                    height: root.height as u32 - 2,
                },
            );
            assert_eq!(
                root.y + child.y,
                highlighted as i32,
                "left-facing child must align with rendered parent row: {child:?}"
            );
            click_at(
                env,
                window,
                (root.x + child.x + 30) as u32,
                (root.y + child.y + 10) as u32,
            );
            wait_for("flipped submenu command", artifacts, || {
                (events(artifacts) == ["ready", "nested"]).then_some(())
            });
        },
    );
}

#[test]
fn tall_bottom_submenu_slides_up_and_last_item_remains_clickable() {
    with_native_menu(
        "menu-bottom-submenu.el",
        |env, window, _, artifacts, _, _| {
            for (cycle, x) in [16, 980].into_iter().enumerate() {
                let before = configured_popups(artifacts).len();
                input(
                    env,
                    &[
                        "mousemove",
                        "--window",
                        window,
                        &x.to_string(),
                        "640",
                        "click",
                        "3",
                    ],
                );
                wait_for("bottom root menu", artifacts, || {
                    (configured_popups(artifacts).len() > before).then_some(())
                });
                input(env, &["mousemove", "--window", window, "500", "400"]);
                let root_pixels = stable_screen(env, artifacts, &format!("bottom-{cycle}-root"));
                input(env, &["key", "End"]);
                // Keep the press alive until the native popup grab is accepted;
                // this placement test does not exercise rapid key-release races.
                input(env, &["keydown", "Right"]);
                wait_for("tall bottom child", artifacts, || {
                    (configured_popups(artifacts).len() > before + 1).then_some(())
                });
                input(env, &["keyup", "Right"]);
                let child_pixels = stable_screen(env, artifacts, &format!("bottom-{cycle}-child"));
                let positions = configured_popups(artifacts);
                let root = positions[before];
                let child = positions[before + 1];
                let row_y = highlighted_row(
                    &root_pixels,
                    &child_pixels,
                    ScreenRect {
                        x: root.x as u32 + 1,
                        y: root.y as u32 + 1,
                        width: root.width as u32 - 2,
                        height: root.height as u32 - 2,
                    },
                ) as i32;
                let child_x = root.x + child.x;
                let child_y = root.y + child.y;
                assert!(child.height > 100, "fixture must exercise a tall child");
                assert!(
                    row_y + child.height > 700,
                    "row-aligned child must overflow before adjustment"
                );
                assert!(child_y < row_y, "child must slide above its parent row");
                assert_eq!(
                    child_y + child.height,
                    700,
                    "child must fit against output bottom"
                );
                assert!(child_x >= 0 && child_x + child.width <= 1000 && child_y >= 0);
                if cycle == 0 {
                    assert_eq!(
                        child.x, root.width,
                        "left-edge child should attach to right"
                    );
                } else {
                    assert_eq!(
                        child.x + child.width,
                        0,
                        "right-edge child should flip left"
                    );
                }
                // Independently locate the rendered child, outside the parent.
                let region = if cycle == 0 {
                    ScreenRect {
                        x: (root.x + root.width) as u32,
                        y: 0,
                        width: (1000 - root.x - root.width) as u32,
                        height: 700,
                    }
                } else {
                    ScreenRect {
                        x: 0,
                        y: 0,
                        width: root.x as u32,
                        height: 700,
                    }
                };
                let visible = changed_rectangle(&root_pixels, &child_pixels, region)
                    .expect("visible tall submenu");
                assert_eq!(
                    (
                        visible.x as i32,
                        visible.y as i32,
                        visible.width as i32,
                        visible.height as i32
                    ),
                    (child_x, child_y, child.width, child.height),
                    "rendered child must match constrained bounds"
                );
                click_at(env, window, visible.x + 40, visible.bottom() - 10);
                let mut expected = vec!["ready"];
                expected.extend(std::iter::repeat_n("bottom-last", cycle + 1));
                wait_for("last child command after sliding", artifacts, || {
                    (events(artifacts) == expected).then_some(())
                });
            }
        },
    );
}
