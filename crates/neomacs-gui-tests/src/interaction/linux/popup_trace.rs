//! Observe native popup lifetimes from the Wayland wire trace.

use crate::interaction::strip_ansi;

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
        // libwayland colorizes the protocol dump unless NO_COLOR is set, and
        // whether it does depends on the library version (wayland 1.22, the
        // runner's, prints plain text; 1.26 colorizes by default). The object
        // and interface names are what this parser keys on, so drop the CSI
        // sequences instead of depending on the environment.
        let line = &strip_ansi(line);
        if let Some((_, args)) = line.split_once(".get_popup(new id xdg_popup") {
            let args = object_tail(args)?;
            let (id, rest) = args.split_once(',')?;
            let surface = numeric_id_after(line, "xdg_surface")?;
            let parent = numeric_id_after(rest, "xdg_surface")?;
            return Some(Self::Created {
                id: PopupId(id.parse().ok()?),
                surface,
                parent,
            });
        }
        let (_, object) = line.split_once("xdg_popup")?;
        let (id, method) = object_tail(object)?.split_once('.')?;
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

/// Strip the `#` (server-side) or `@` (client-side) object-id separator.
///
/// libwayland's debug dump uses `INTERFACE#ID` for objects allocated by the
/// other side of the connection and `INTERFACE@ID` for the dumping side's
/// proxies.  Both name the same protocol object; which one appears depends on
/// which side produced the dump, not on the events.  The CI runner's client
/// dump prints `@` (its whole popup trace parsed as empty before this), while
/// older/other dumps print `#`.
fn object_tail(text: &str) -> Option<&str> {
    text.strip_prefix('#').or_else(|| text.strip_prefix('@'))
}

/// The numeric object id following `INTERFACE` without its separator.
fn numeric_id_after(text: &str, interface: &str) -> Option<u32> {
    let (_, tail) = text.split_once(interface)?;
    let digits: &str = object_tail(tail)?
        .split(|character: char| !character.is_ascii_digit())
        .next()?;
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
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

#[test]
fn client_side_object_ids_are_recognized_like_server_side_ids() {
    // Verbatim lines from the CI runner's libwayland 1.26 client dump
    // (native-menu-31881 wayland.log).  libwayland prints client proxies with
    // `@` and server-side objects with `#`; both are the same protocol, and
    // the runner's dump uses `@`.  Parsing only `#` hides every popup on CI.
    let log = r#"[2787522.930]  -> xdg_surface@89.get_popup(new id xdg_popup@90, xdg_surface@21, xdg_positioner@87)
[2787522.950]  -> xdg_popup@90.grab(wl_seat@8, 11)
[2787523.091] xdg_popup@90.configure(8, 17, 150, 28)
[2787598.288]  -> xdg_surface@122.get_popup(new id xdg_popup@120, xdg_surface@89, xdg_positioner@81)
[2787598.309]  -> xdg_popup@120.grab(wl_seat@8, 11)
[2787598.419] xdg_popup@120.configure(150, 4, 215, 28)
[2787780.787]  -> xdg_popup@120.destroy()
[2787780.802]  -> xdg_popup@90.destroy()
"#;
    assert_eq!(
        configured(log),
        vec![
            ParentRect {
                x: 8,
                y: 17,
                width: 150,
                height: 28
            },
            ParentRect {
                x: 150,
                y: 4,
                width: 215,
                height: 28
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

#[cfg(test)]
mod tests {
    use super::super::DesktopPoint;
    use super::observed;

    #[test]
    fn colored_protocol_dumps_keep_popup_lifetimes_visible() {
        let plain = "[1.0]  -> xdg_surface#31.get_popup(new id xdg_popup#28, xdg_surface#21, xdg_positioner#33)\n\
                     [1.1] xdg_popup#28.configure(8, 17, 170, 88)\n";
        // libwayland (1.26) colors the dump unless NO_COLOR is set: the object
        // name and the numeric id are wrapped in separate SGR sequences.
        let colored = "[1.0] \u{1b}[32m\u{1b}[31m -> \u{1b}[34mxdg_surface\u{1b}[35m#31\u{1b}[36m.get_popup\u{1b}[0m(new id \u{1b}[34mxdg_popup\u{1b}[35m#28\u{1b}[0m, \u{1b}[34mxdg_surface\u{1b}[35m#21\u{1b}[0m, \u{1b}[34mxdg_positioner\u{1b}[35m#33\u{1b}[0m)\n\
                       [1.1] \u{1b}[34mxdg_popup\u{1b}[35m#28\u{1b}[36m.configure\u{1b}[0m(8, 17, 170, 88)\n";
        let origin = DesktopPoint { x: 0.0, y: 0.0 };
        let summarize = |log: &str| {
            observed(log, origin)
                .into_iter()
                .map(|window| {
                    (
                        window.id,
                        window.bounds.x,
                        window.bounds.y,
                        window.bounds.width,
                        window.bounds.height,
                        window.title,
                    )
                })
                .collect::<Vec<_>>()
        };
        let expected = summarize(plain);
        assert_eq!(expected.len(), 1, "the plain dump must expose one popup");
        assert_eq!(expected[0].1, 8.0);
        assert_eq!(expected[0].2, 17.0);
        assert_eq!(expected[0].3, 170.0);
        assert_eq!(expected[0].4, 88.0);
        assert_eq!(
            summarize(colored),
            expected,
            "the colorized dump must parse to the same popup"
        );
    }
}
