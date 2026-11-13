//! GNU xdisp.c's ownership of the bottom/right divider intersection.
//!
//! Geometry cannot recover parent/sibling topology from rectangles. Capture
//! it once for a frame attempt, or along the already checked target route.

use neovm_core::window::{
    Frame, SplitDirection, Window, WindowId, WindowTreePath, WindowTreeRoute,
};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DividerCornerPolicy {
    /// A non-last horizontal child: bottom spans the corner, right stops above it.
    BottomCovers,
    /// GNU's vertical-sibling cases: right spans the corner, bottom stops before it.
    RightCovers,
    /// GNU draws both complete rectangles in all remaining cases.
    #[default]
    Overlap,
}

#[derive(Clone, Copy)]
struct Combination {
    direction: SplitDirection,
    has_next: bool,
}

impl DividerCornerPolicy {
    fn from_topology(
        has_next: bool,
        parent: Option<Combination>,
        grandparent: Option<Combination>,
    ) -> Self {
        // a360 xdisp.c:37780-37835. These branches are mutually exclusive:
        // both edges may cover a corner, but neither may omit it.
        match parent {
            Some(Combination {
                direction: SplitDirection::Horizontal,
                ..
            }) if has_next => Self::BottomCovers,
            Some(Combination {
                direction: SplitDirection::Vertical,
                ..
            }) if has_next => Self::RightCovers,
            Some(Combination {
                direction: SplitDirection::Horizontal,
                ..
            }) if grandparent.is_some_and(|grandparent| {
                grandparent.direction == SplitDirection::Vertical && grandparent.has_next
            }) =>
            {
                Self::RightCovers
            }
            _ => Self::Overlap,
        }
    }
}

fn root_has_next(frame: &Frame) -> bool {
    frame
        .minibuffer_leaf
        .as_ref()
        .is_some_and(|minibuffer| minibuffer.id() != frame.root_window().id())
}

/// One O(windows) traversal, rather than a parent-of search for every leaf.
pub(crate) fn divider_corner_policies(
    frame: &Frame,
) -> Option<FxHashMap<WindowId, DividerCornerPolicy>> {
    fn visit(
        frame: &Frame,
        window: &Window,
        has_next: bool,
        parent: Option<Combination>,
        grandparent: Option<Combination>,
        result: &mut FxHashMap<WindowId, DividerCornerPolicy>,
    ) -> Option<()> {
        match window {
            Window::Leaf { .. } => {
                result.insert(
                    window.id(),
                    DividerCornerPolicy::from_topology(has_next, parent, grandparent),
                );
            }
            Window::Internal {
                direction,
                children,
                ..
            } => {
                let combination = Some(Combination {
                    direction: *direction,
                    has_next,
                });
                for (index, child) in children.iter().enumerate() {
                    visit(
                        frame,
                        frame.tree().find(*child)?,
                        index + 1 < children.len(),
                        combination,
                        parent,
                        result,
                    )?;
                }
            }
        }
        Some(())
    }

    let mut result = FxHashMap::default();
    visit(
        frame,
        frame.root_window(),
        root_has_next(frame),
        None,
        None,
        &mut result,
    )?;
    if let Some(minibuffer) = &frame.minibuffer_leaf {
        result.insert(minibuffer.id(), DividerCornerPolicy::Overlap);
    }
    Some(result)
}

/// O(depth) recollection after Lisp hooks, using the generation-checked route
/// supplied by FrameManager. An invalid route never guesses a corner policy.
pub(crate) fn divider_corner_at_path(
    frame: &Frame,
    path: &WindowTreePath,
) -> Option<DividerCornerPolicy> {
    match &path.route {
        WindowTreeRoute::Minibuffer => frame
            .minibuffer_leaf
            .as_ref()
            .filter(|minibuffer| minibuffer.id() == path.target && minibuffer.is_leaf())
            .map(|_| DividerCornerPolicy::Overlap),
        WindowTreeRoute::Root(indices) => {
            let mut window = frame.root_window();
            let mut has_next = root_has_next(frame);
            let mut parent = None;
            let mut grandparent = None;
            for index in indices {
                let Window::Internal {
                    direction,
                    children,
                    ..
                } = window
                else {
                    return None;
                };
                grandparent = parent;
                parent = Some(Combination {
                    direction: *direction,
                    has_next,
                });
                let child = *children.get(*index)?;
                has_next = *index + 1 < children.len();
                window = frame.tree().find(child)?;
            }
            (window.id() == path.target && window.is_leaf())
                .then(|| DividerCornerPolicy::from_topology(has_next, parent, grandparent))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_corner_topology_uses_exact_gnu_sibling_rules() {
        let h = |has_next| {
            Some(Combination {
                direction: SplitDirection::Horizontal,
                has_next,
            })
        };
        let v = |has_next| {
            Some(Combination {
                direction: SplitDirection::Vertical,
                has_next,
            })
        };
        assert_eq!(
            DividerCornerPolicy::from_topology(true, h(false), None),
            DividerCornerPolicy::BottomCovers
        );
        assert_eq!(
            DividerCornerPolicy::from_topology(true, v(false), None),
            DividerCornerPolicy::RightCovers
        );
        assert_eq!(
            DividerCornerPolicy::from_topology(false, h(false), v(true)),
            DividerCornerPolicy::RightCovers
        );
        // Source tests GRANDPARENT.next, not PARENT.next. A root's owned
        // minibuffer is its next sibling even when its pixel height is zero.
        assert_eq!(
            DividerCornerPolicy::from_topology(false, h(true), v(false)),
            DividerCornerPolicy::Overlap
        );
        assert_eq!(
            DividerCornerPolicy::from_topology(false, h(false), v(false)),
            DividerCornerPolicy::Overlap
        );
        assert_eq!(
            DividerCornerPolicy::from_topology(false, None, None),
            DividerCornerPolicy::Overlap
        );
    }

    #[test]
    fn divider_corner_frame_capture_agrees_with_checked_target_paths() {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        let id = eval
            .frame_manager_mut()
            .create_frame("divider-corners", 800, 600, buffer);
        let first = eval.frame_manager().get(id).unwrap().selected_window;
        let next = eval
            .frame_manager_mut()
            .split_window(
                id,
                first,
                SplitDirection::Vertical,
                buffer,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .unwrap();
        let upper_right = eval
            .frame_manager_mut()
            .split_window(
                id,
                first,
                SplitDirection::Horizontal,
                buffer,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .unwrap();
        let lower_right = eval
            .frame_manager_mut()
            .split_window(
                id,
                next,
                SplitDirection::Horizontal,
                buffer,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .unwrap();
        use neovm_core::window::DisplayFrameManagerExt;
        let paths = eval.frame_manager().leaf_window_paths(id).unwrap();
        let frame = eval.frame_manager().get(id).unwrap();
        let policies = divider_corner_policies(frame).unwrap();
        for (id, mut path) in paths.clone() {
            assert_eq!(
                divider_corner_at_path(frame, &path),
                policies.get(&id).copied()
            );
            path.target = WindowId(u64::MAX);
            assert_eq!(divider_corner_at_path(frame, &path), None);
        }
        assert!(
            policies
                .values()
                .any(|policy| *policy == DividerCornerPolicy::BottomCovers)
        );
        assert_eq!(policies[&upper_right], DividerCornerPolicy::RightCovers);
        assert_eq!(policies[&lower_right], DividerCornerPolicy::RightCovers);
        // With a borrowed/absent minibuffer the root has no NEXT. The
        // grandparent branch must change even for the upper parent's NEXT.
        let frame = eval.frame_manager_mut().get_mut(id).unwrap();
        frame.minibuffer_leaf = None;
        let policies = divider_corner_policies(frame).unwrap();
        assert_eq!(policies[&upper_right], DividerCornerPolicy::Overlap);
        assert_eq!(policies[&lower_right], DividerCornerPolicy::Overlap);
        for (id, path) in paths {
            if policies.contains_key(&id) {
                assert_eq!(
                    divider_corner_at_path(frame, &path),
                    policies.get(&id).copied()
                );
            }
        }
    }
}
