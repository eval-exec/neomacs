//! Observe native popup lifetimes from the Wayland wire trace.

// xdg_popup.configure is the compositor's accepted position, relative to the
// parent surface. Unlike set_anchor_rect, it includes constraint adjustments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ParentRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PopupId(u32);

enum PopupEvent {
    Created(PopupId),
    Configured(PopupId, ParentRect),
    Destroyed(PopupId),
}

impl PopupEvent {
    fn parse(line: &str) -> Option<Self> {
        if let Some((_, args)) = line.split_once(".get_popup(new id xdg_popup#") {
            let (id, _) = args.split_once(',')?;
            return Some(Self::Created(PopupId(id.parse().ok()?)));
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
pub(super) fn configured(log: &str) -> Vec<ParentRect> {
    let mut live = std::collections::HashMap::<PopupId, usize>::new();
    let mut lifetimes = Vec::new();
    for event in log.lines().filter_map(PopupEvent::parse) {
        match event {
            PopupEvent::Created(id) => {
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
