//! Observe native popup lifetimes from the Wayland wire trace.

// xdg_popup.configure is the compositor's accepted position, relative to the
// parent surface. Unlike set_anchor_rect, it includes constraint adjustments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParentRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PopupId(u32);

enum PopupEvent {
    Created {
        id: PopupId,
        surface: u32,
        parent: u32,
    },
    Configured(PopupId, ParentRect),
    Destroyed(PopupId),
}

impl PopupEvent {
    fn parse(line: &str) -> Option<Self> {
        if let Some((_, args)) = line.split_once(".get_popup(new id xdg_popup#") {
            let (id, rest) = args.split_once(',')?;
            let surface = line
                .split_once("xdg_surface#")?
                .1
                .split_once('.')?
                .0
                .parse()
                .ok()?;
            let parent = rest
                .split_once("xdg_surface#")?
                .1
                .split_once(',')?
                .0
                .parse()
                .ok()?;
            return Some(Self::Created {
                id: PopupId(id.parse().ok()?),
                surface,
                parent,
            });
        }
        let (_, object) = line.split_once("xdg_popup#")?;
        let (id, method) = object.split_once('.')?;
        let id = PopupId(id.parse().ok()?);
        if method == "destroy()" {
            return Some(Self::Destroyed(id));
        }
        let args = method.strip_prefix("configure(")?.strip_suffix(')')?;
        let values = args
            .split(", ")
            .map(str::parse::<i32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let [x, y, width, height] = values.as_slice() else {
            return None;
        };
        Some(Self::Configured(
            id,
            ParentRect {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            },
        ))
    }
}

/// One latest configuration per popup lifetime, in creation order. Retired
/// popups remain in the history; reusing a destroyed object ID starts a new
/// lifetime. Unconfigured popups never count as accepted geometry.
pub fn configured(log: &str) -> Vec<ParentRect> {
    let mut live = std::collections::HashMap::<PopupId, usize>::new();
    let mut lifetimes = Vec::new();
    for event in log.lines().filter_map(PopupEvent::parse) {
        match event {
            PopupEvent::Created { id, .. } => {
                assert!(
                    live.insert(id, lifetimes.len()).is_none(),
                    "popup recreated before destroy: {id:?}"
                );
                lifetimes.push(None);
            }
            PopupEvent::Configured(id, rect) => {
                if let Some(&lifetime) = live.get(&id) {
                    lifetimes[lifetime] = Some(rect);
                }
            }
            PopupEvent::Destroyed(id) => {
                live.remove(&id);
            }
        }
    }
    lifetimes.into_iter().flatten().collect()
}

#[test]
fn reconfiguration_and_reused_ids_preserve_popup_lifetimes() {
    let log = r#"[1] -> xdg_surface#53.get_popup(new id xdg_popup#54, xdg_surface#21, xdg_positioner#51)
[2] xdg_popup#54.configure(8, 17, 170, 88)
[3] -> xdg_surface#55.get_popup(new id xdg_popup#56, xdg_surface#53, xdg_positioner#51)
[4] xdg_popup#56.configure(170, 24, 150, 28)
[5] xdg_popup#54.configure(10, 17, 170, 88)
[6] -> xdg_popup#56.destroy()
[7] -> xdg_popup#54.destroy()
[8] -> xdg_surface#55.get_popup(new id xdg_popup#54, xdg_surface#21, xdg_positioner#51)
[9] xdg_popup#54.configure(749, 492, 251, 208)
"#;
    assert_eq!(
        configured(log),
        vec![
            ParentRect {
                x: 10,
                y: 17,
                width: 170,
                height: 88
            },
            ParentRect {
                x: 170,
                y: 24,
                width: 150,
                height: 28
            },
            ParentRect {
                x: 749,
                y: 492,
                width: 251,
                height: 208
            },
        ]
    );
}

/// Current live windows, resolving parent-relative positions through the popup tree.
pub(super) fn observed(log: &str, origin: super::DesktopPoint) -> Vec<super::ObservedWindow> {
    struct Live {
        id: PopupId,
        surface: u32,
        parent: u32,
        generation: u64,
        rect: Option<ParentRect>,
    }
    let mut live: Vec<Live> = vec![];
    let mut generation = 0;
    for event in log.lines().filter_map(PopupEvent::parse) {
        match event {
            PopupEvent::Created {
                id,
                surface,
                parent,
            } => {
                generation += 1;
                live.push(Live {
                    id,
                    surface,
                    parent,
                    generation,
                    rect: None,
                });
            }
            PopupEvent::Configured(id, rect) => {
                if let Some(window) = live.iter_mut().find(|w| w.id == id) {
                    window.rect = Some(rect);
                }
            }
            PopupEvent::Destroyed(id) => live.retain(|w| w.id != id),
        }
    }
    live.iter()
        .filter_map(|window| {
            let rect = window.rect?;
            let (mut x, mut y) = (rect.x as f64 + origin.x, rect.y as f64 + origin.y);
            let mut parent = window.parent;
            for _ in 0..live.len() {
                let Some(ancestor) = live.iter().find(|w| w.surface == parent) else {
                    break;
                };
                let rect = ancestor.rect?;
                x += rect.x as f64;
                y += rect.y as f64;
                parent = ancestor.parent;
            }
            Some(super::ObservedWindow {
                id: super::WindowId {
                    native: window.id.0 as u64,
                    generation: window.generation,
                },
                bounds: super::DesktopRect {
                    x,
                    y,
                    width: rect.width as f64,
                    height: rect.height as f64,
                },
                title: "Neomacs menu".into(),
            })
        })
        .collect()
}
