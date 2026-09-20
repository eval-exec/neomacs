//! Same native-input, independent-window, pixel and command contracts on each OS.
use neomacs_gui_tests::interaction::*;
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn wait<T>(label: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out: {label}");
        thread::sleep(Duration::from_millis(40));
    }
}
fn menus(driver: &mut impl DesktopDriver) -> Vec<ObservedWindow> {
    driver
        .observe()
        .unwrap()
        .windows
        .into_iter()
        .filter(|w| w.title == "Neomacs menu")
        .collect()
}
fn events(path: &Path) -> Vec<String> {
    fs::read(path.join("events.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn crop(capture: &Capture, rect: DesktopRect) -> image::RgbImage {
    let (x, y) = capture
        .mapping
        .pixel(DesktopPoint {
            x: rect.x + 1.0,
            y: rect.y + 1.0,
        })
        .unwrap();
    let (right, bottom) = capture
        .mapping
        .pixel(DesktopPoint {
            x: rect.x + rect.width - 1.0,
            y: rect.y + rect.height - 1.0,
        })
        .unwrap();
    image::imageops::crop_imm(&capture.pixels, x, y, right - x, bottom - y).to_image()
}
fn capture(
    driver: &mut impl DesktopDriver,
    path: &Path,
    name: &str,
    region: DesktopRect,
) -> Capture {
    let mut prior = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let image = driver.capture(&path.join(format!("{name}.png"))).unwrap();
        let current = crop(&image, region);
        let stable = prior.as_ref() == Some(&current);
        if stable {
            return image;
        }
        if Instant::now() >= deadline {
            if let Some(previous) = prior {
                previous
                    .save(path.join(format!("{name}-previous-region.png")))
                    .unwrap();
            }
            current
                .save(path.join(format!("{name}-current-region.png")))
                .unwrap();
            panic!("timed out: stable relevant pixels for {name} in {region:?}");
        }
        prior = Some(current);
        thread::sleep(Duration::from_millis(40));
    }
}
fn changed(a: &image::RgbImage, b: &image::RgbImage) -> usize {
    assert_eq!(a.dimensions(), b.dimensions());
    a.pixels()
        .zip(b.pixels())
        .filter(|(a, b)| a.0.iter().zip(b.0).any(|(a, b)| a.abs_diff(b) > 8))
        .count()
}

pub fn exercise(
    driver: &mut impl DesktopDriver,
    path: &Path,
    frame: DesktopRect,
    char_height: f64,
    char_width: f64,
) {
    let heading = DesktopPoint {
        x: frame.x + 16.0 + char_width * 6.0,
        y: frame.y + char_height / 2.0,
    };
    let outside = DesktopPoint {
        x: frame.x + frame.width * 0.75,
        y: frame.y + frame.height * 0.75,
    };
    driver.input(InputAction::Move { point: outside }).unwrap();
    let closed = capture(driver, path, "shared-closed", frame);
    assert!(menus(driver).is_empty());
    driver.click(heading, Button::Left).unwrap();
    let root = wait("visible dropdown window", || {
        let windows = menus(driver);
        (windows.len() == 1).then(|| windows[0].clone())
    });
    assert!(
        (root.bounds.x - (frame.x + 8.0)).abs() <= 1.0,
        "heading alignment: {root:?}"
    );
    assert!(
        (root.bounds.y - (frame.y + char_height)).abs() <= 1.0,
        "below heading: {root:?}"
    );
    driver.input(InputAction::Move { point: outside }).unwrap();
    let opened = capture(driver, path, "shared-open", root.bounds);
    let panel = crop(&opened, root.bounds);
    assert!(
        changed(&crop(&closed, root.bounds), &panel)
            > panel.width() as usize * panel.height() as usize / 2,
        "dropdown must change visible pixels"
    );
    driver
        .click(
            DesktopPoint {
                x: root.bounds.x + 30.0,
                y: root.bounds.y + 10.0,
            },
            Button::Left,
        )
        .unwrap();
    wait("one selected command", || {
        (events(path) == ["ready", "choose"]).then_some(())
    });
    wait("selected menu closes", || {
        menus(driver).is_empty().then_some(())
    });
    driver.press(Key::X).unwrap();
    wait("typing after selection", || {
        (fs::read_to_string(path.join("text")).ok().as_deref() == Some("x")).then_some(())
    });

    for (cycle, escape) in [true, false].into_iter().enumerate() {
        driver.input(InputAction::Move { point: outside }).unwrap();
        let baseline = capture(driver, path, &format!("cancel-{cycle}-closed"), root.bounds);
        driver.click(heading, Button::Left).unwrap();
        let active = wait("reopened menu", || {
            let w = menus(driver);
            (w.len() == 1).then(|| w[0].clone())
        });
        assert_ne!(active.id, root.id, "a reopened popup has a new lifetime");
        driver.input(InputAction::Move { point: outside }).unwrap();
        let opened = capture(driver, path, &format!("cancel-{cycle}-open"), active.bounds);
        assert!(
            changed(
                &crop(&baseline, active.bounds),
                &crop(&opened, active.bounds)
            ) > 100
        );
        if escape {
            driver.press(Key::Escape).unwrap();
        } else {
            driver.click(outside, Button::Left).unwrap();
        }
        wait("dismissed native window", || {
            menus(driver).is_empty().then_some(())
        });
        let dismissed = capture(
            driver,
            path,
            &format!("cancel-{cycle}-dismissed"),
            active.bounds,
        );
        assert_eq!(
            changed(
                &crop(&baseline, active.bounds),
                &crop(&dismissed, active.bounds)
            ),
            0,
            "pixels restored after cancellation"
        );
        assert_eq!(events(path), ["ready", "choose"]);
        driver.press(Key::X).unwrap();
        let expected = "x".repeat(cycle + 2);
        wait("typing after cancellation", || {
            (fs::read_to_string(path.join("text")).ok().as_deref() == Some(expected.as_str()))
                .then_some(())
        });
    }

    driver.click(heading, Button::Left).unwrap();
    let parent_before = wait("root before child", || {
        let windows = menus(driver);
        (windows.len() == 1).then(|| windows[0].clone())
    });
    driver.input(InputAction::Move { point: outside }).unwrap();
    let unselected = capture(driver, path, "parent-unselected", parent_before.bounds);
    driver.press(Key::Home).unwrap();
    driver.press(Key::Down).unwrap();
    driver
        .input(InputAction::Key {
            key: Key::Right,
            down: true,
        })
        .unwrap();
    let windows = wait("nested native window", || {
        let w = menus(driver);
        (w.len() == 2).then_some(w)
    });
    driver
        .input(InputAction::Key {
            key: Key::Right,
            down: false,
        })
        .unwrap();
    let child = windows.iter().find(|w| w.id != parent_before.id).unwrap();
    let parent = windows.iter().find(|w| w.id == parent_before.id).unwrap();
    let selected = capture(driver, path, "parent-selected", parent.bounds);
    let before = crop(&unselected, parent.bounds);
    let after = crop(&selected, parent.bounds);
    let row = (0..after.height())
        .find(|&y| {
            (0..after.width())
                .filter(|&x| {
                    before
                        .get_pixel(x, y)
                        .0
                        .iter()
                        .zip(after.get_pixel(x, y).0)
                        .any(|(a, b)| a.abs_diff(b) > 8)
                })
                .count()
                > after.width() as usize / 2
        })
        .expect("rendered selected row");
    let row_y =
        parent.bounds.y + 1.0 + row as f64 * (parent.bounds.height - 2.0) / after.height() as f64;
    assert!(
        (child.bounds.y - row_y).abs() <= 1.0,
        "submenu must align with its highlighted row"
    );
    assert!(
        (child.bounds.x - (parent.bounds.x + parent.bounds.width)).abs() <= 1.0
            || (child.bounds.x + child.bounds.width - parent.bounds.x).abs() <= 1.0,
        "submenu attaches to parent"
    );
    let observation = driver.observe().unwrap();
    assert!(observation.usable.contains(DesktopPoint {
        x: child.bounds.x,
        y: child.bounds.y
    }));
    assert!(observation.usable.contains(DesktopPoint {
        x: child.bounds.x + child.bounds.width - 1.0,
        y: child.bounds.y + child.bounds.height - 1.0
    }));
    capture(driver, path, "shared-child", child.bounds);
    driver
        .click(
            DesktopPoint {
                x: child.bounds.x + 30.0,
                y: child.bounds.y + 10.0,
            },
            Button::Left,
        )
        .unwrap();
    wait("nested command", || {
        (events(path) == ["ready", "choose", "nested"]).then_some(())
    });
    wait("all menus closed", || {
        menus(driver).is_empty().then_some(())
    });
    driver.click(heading, Button::Left).unwrap();
    let root = wait("disabled-item menu", || {
        let w = menus(driver);
        (w.len() == 1).then(|| w[0].clone())
    });
    driver
        .click(
            DesktopPoint {
                x: root.bounds.x + 30.0,
                y: root.bounds.y + 4.0 + 2.0 * (char_height + 3.0) + 8.0,
            },
            Button::Left,
        )
        .unwrap();
    capture(driver, path, "disabled-click", root.bounds);
    assert_eq!(events(path), ["ready", "choose", "nested"]);
    assert_eq!(menus(driver).len(), 1, "disabled click keeps menu open");
    driver.press(Key::Home).unwrap();
    driver.press(Key::Down).unwrap();
    driver.press(Key::Down).unwrap();
    driver.press(Key::Enter).unwrap();
    wait("keyboard skips disabled and toggles", || {
        (events(path) == ["ready", "choose", "nested", "checked"]).then_some(())
    });
    wait("toggle menu closes", || {
        menus(driver).is_empty().then_some(())
    });
}

pub fn exercise_bottom_edges(driver: &mut impl DesktopDriver, path: &Path) {
    let frame = driver.fit_window().unwrap();
    capture(driver, path, "edge-ready", frame);
    for (cycle, right) in [false, true].into_iter().enumerate() {
        let position = DesktopPoint {
            x: if right {
                frame.x + frame.width - 32.0
            } else {
                frame.x + 16.0
            },
            y: frame.y + frame.height - 75.0,
        };
        driver.click(position, Button::Right).unwrap();
        let root = wait("bottom context menu", || {
            let w = menus(driver);
            (w.len() == 1).then(|| w[0].clone())
        });
        driver.press(Key::End).unwrap();
        driver
            .input(InputAction::Key {
                key: Key::Right,
                down: true,
            })
            .unwrap();
        let windows = wait("tall child", || {
            let w = menus(driver);
            (w.len() == 2).then_some(w)
        });
        driver
            .input(InputAction::Key {
                key: Key::Right,
                down: false,
            })
            .unwrap();
        let child = windows.iter().find(|w| w.id != root.id).unwrap();
        let usable = driver.observe().unwrap().usable;
        assert!(
            child.bounds.height > 100.0,
            "short fixture cannot prove vertical sliding"
        );
        assert!(
            root.bounds.y + root.bounds.height - 25.0 + child.bounds.height
                > usable.y + usable.height,
            "unconstrained child must overflow"
        );
        assert!(
            child.bounds.y < root.bounds.y + root.bounds.height - 25.0,
            "child must slide upward"
        );
        assert!(usable.contains(DesktopPoint {
            x: child.bounds.x,
            y: child.bounds.y
        }));
        assert!(usable.contains(DesktopPoint {
            x: child.bounds.x + child.bounds.width - 1.0,
            y: child.bounds.y + child.bounds.height - 1.0
        }));
        if right {
            assert!(
                (child.bounds.x + child.bounds.width - root.bounds.x).abs() <= 1.0,
                "child flips left"
            );
        } else {
            assert!(
                (child.bounds.x - root.bounds.x - root.bounds.width).abs() <= 1.0,
                "child attaches right"
            );
        }
        let shot = capture(driver, path, &format!("bottom-{cycle}-child"), child.bounds);
        let pixels = crop(&shot, child.bounds);
        assert!(
            pixels.pixels().any(|p| *p != *pixels.get_pixel(0, 0)),
            "child screenshot is blank"
        );
        driver
            .click(
                DesktopPoint {
                    x: child.bounds.x + 30.0,
                    y: child.bounds.y + child.bounds.height - 10.0,
                },
                Button::Left,
            )
            .unwrap();
        let mut expected = vec!["ready".to_owned()];
        expected.extend(std::iter::repeat_n("bottom-last".to_owned(), cycle + 1));
        wait("last child command", || {
            (events(path) == expected).then_some(())
        });
        wait("menus close", || menus(driver).is_empty().then_some(()));
    }
}
