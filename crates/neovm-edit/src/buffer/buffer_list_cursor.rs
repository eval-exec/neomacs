//! Native cons-tail mirror, active only while an owned buffer-list cursor lives.
//!
//! GNU FOR_EACH_ALIST_VALUE reads the current cons's cdr after the body.
//! Reordering overwrites that cdr; removing a cons retains its old cdr.
//! No RefCell borrow spans the caller's loop body or a Lisp callback.

use std::cell::RefCell;
use std::rc::Rc;

use super::{BufferId, FxHashMap};

#[derive(Clone, Copy)]
pub(super) enum BufferOrderChange {
    Append(BufferId),
    Remove(BufferId),
    MoveFirst(BufferId),
    MoveLast(BufferId),
    MoveAfter(BufferId, BufferId),
}

pub(super) struct BufferListLinks {
    head: Option<BufferId>,
    tail: Option<BufferId>,
    // Unlike prev, next retains removed nodes for every active cursor.
    next: FxHashMap<BufferId, Option<BufferId>>,
    prev: FxHashMap<BufferId, Option<BufferId>>,
}

impl BufferListLinks {
    pub(super) fn new(order: &[BufferId]) -> Self {
        let mut links = Self {
            head: None,
            tail: None,
            next: FxHashMap::default(),
            prev: FxHashMap::default(),
        };
        for &id in order {
            links.append(id);
        }
        links
    }

    fn append(&mut self, id: BufferId) {
        self.prev.insert(id, self.tail);
        self.next.insert(id, None);
        if let Some(tail) = self.tail {
            self.next.insert(tail, Some(id));
        } else {
            self.head = Some(id);
        }
        self.tail = Some(id);
    }

    fn unlink(&mut self, id: BufferId) {
        let previous = self
            .prev
            .remove(&id)
            .expect("unlink requires a live list node");
        let next = self.next[&id];
        if let Some(previous) = previous {
            self.next.insert(previous, next);
        } else {
            self.head = next;
        }
        if let Some(next) = next {
            self.prev.insert(next, previous);
        } else {
            self.tail = previous;
        }
        // Do not clear next[id]: an outer cursor can still hold this cons.
    }

    #[inline(never)]
    pub(super) fn apply(&mut self, change: BufferOrderChange) {
        match change {
            BufferOrderChange::Append(id) => self.append(id),
            BufferOrderChange::Remove(id) => self.unlink(id),
            BufferOrderChange::MoveFirst(id) => {
                self.unlink(id);
                self.next.insert(id, self.head);
                self.prev.insert(id, None);
                if let Some(head) = self.head {
                    self.prev.insert(head, Some(id));
                } else {
                    self.tail = Some(id);
                }
                self.head = Some(id);
            }
            BufferOrderChange::MoveLast(id) => {
                self.unlink(id);
                self.append(id);
            }
            BufferOrderChange::MoveAfter(id, after) => {
                self.unlink(id);
                let next = self.next[&after];
                self.next.insert(after, Some(id));
                self.next.insert(id, next);
                self.prev.insert(id, Some(after));
                if let Some(next) = next {
                    self.prev.insert(next, Some(id));
                } else {
                    self.tail = Some(id);
                }
            }
        }
    }
}

enum CursorPosition {
    BeforeFirst(Option<BufferId>),
    At(BufferId),
    Exhausted,
}

pub(super) struct BufferListCursor {
    links: Rc<RefCell<BufferListLinks>>,
    position: CursorPosition,
}

impl BufferListCursor {
    pub(super) fn new(links: Rc<RefCell<BufferListLinks>>) -> Self {
        let head = links.borrow().head;
        Self {
            links,
            position: CursorPosition::BeforeFirst(head),
        }
    }
}

impl Iterator for BufferListCursor {
    type Item = BufferId;

    fn next(&mut self) -> Option<Self::Item> {
        let next = match self.position {
            CursorPosition::BeforeFirst(head) => head,
            CursorPosition::At(id) => self.links.borrow().next[&id],
            CursorPosition::Exhausted => None,
        };
        self.position = match next {
            Some(id) => CursorPosition::At(id),
            None => CursorPosition::Exhausted,
        };
        next
    }
}

static_assertions::assert_not_impl_any!(BufferListCursor: Send, Sync);

#[cfg(test)]
mod tests {
    use super::super::{BufferManager, Value};
    use super::*;

    fn setup() -> (BufferManager, BufferId, BufferId, BufferId) {
        crate::test_utils::init_test_tracing();
        let mut manager = BufferManager::new();
        let a = manager.create_buffer("cursor-a");
        let b = manager.create_buffer("cursor-b");
        let c = manager.create_buffer("cursor-c");
        (manager, a, b, c)
    }

    fn at_a(manager: &mut BufferManager, a: BufferId) -> impl Iterator<Item = BufferId> + use<> {
        let mut cursor = manager.buffer_list_cursor();
        assert!(cursor.any(|id| id == a));
        cursor
    }

    #[test]
    fn cursor_observes_append_and_current_bury() {
        let (mut manager, a, b, c) = setup();
        let mut cursor = at_a(&mut manager, a);
        let d = manager.create_buffer("cursor-appended");
        assert_eq!(cursor.by_ref().collect::<Vec<_>>(), [b, c, d]);
        assert_eq!(cursor.next(), None);
        drop(cursor);
        let mut cursor = at_a(&mut manager, a);
        assert!(manager.note_buffer_order_tail(a));
        assert_eq!(cursor.next(), None);
    }

    #[test]
    fn cursor_preserves_move_then_kill_operation_order() {
        let (mut manager, a, b, c) = setup();
        let cursor = at_a(&mut manager, a);
        manager.note_buffer_order_head(b);
        assert!(manager.kill_buffer(a));
        assert_eq!(cursor.collect::<Vec<_>>(), [c]);

        let (mut manager, a, b, c) = setup();
        let cursor = at_a(&mut manager, a);
        assert!(manager.kill_buffer(a));
        manager.note_buffer_order_head(b);
        // B's rewritten cdr also revisits the original scratch head.
        let suffix = cursor.collect::<Vec<_>>();
        assert_eq!(suffix.first(), Some(&b));
        assert_eq!(suffix.last(), Some(&c));
    }

    #[test]
    fn cursor_retains_removed_future_nodes_in_removal_order() {
        let (mut manager, a, b, c) = setup();
        let cursor = at_a(&mut manager, a);
        assert!(manager.kill_buffer(a));
        assert!(manager.kill_buffer(b));
        assert_eq!(cursor.collect::<Vec<_>>(), [b, c]);
        let (mut manager, a, b, c) = setup();
        let cursor = at_a(&mut manager, a);
        assert!(manager.kill_buffer(b));
        assert!(manager.kill_buffer(a));
        assert_eq!(cursor.collect::<Vec<_>>(), [c]);
    }

    #[test]
    fn nested_cursor_and_clone_have_correct_owner_lifetimes() {
        let (mut manager, a, b, c) = setup();
        let outer = at_a(&mut manager, a);
        let mut inner = at_a(&mut manager, a);
        let mut detached = manager.clone();
        assert!(detached.buffer_order_observer.is_none());
        assert!(detached.note_buffer_order_tail(a));
        assert!(manager.kill_buffer(a));
        assert_eq!(inner.next(), Some(b));
        drop(inner);
        let d = manager.create_buffer("cursor-after-inner-drop");
        assert_eq!(outer.collect::<Vec<_>>(), [b, c, d]);
        assert!(
            manager
                .buffer_order_observer
                .as_ref()
                .unwrap()
                .upgrade()
                .is_none()
        );
        manager.create_buffer("cursor-after-last-drop");
        assert!(manager.buffer_order_observer.is_none());
    }

    #[test]
    fn cursor_covers_indirect_append_bulk_removal_and_move_after() {
        let (mut manager, a, b, c) = setup();
        let cursor = at_a(&mut manager, a);
        let indirect = manager
            .create_indirect_buffer(b, "cursor-indirect", false)
            .unwrap();
        assert!(manager.note_buffer_order_after(indirect, a));
        assert!(manager.kill_buffer(b));
        // Removing B first rewrites indirect.next from B to C, then removing
        // indirect rewrites A.next to C. No callback runs inside this bulk edit.
        assert_eq!(cursor.collect::<Vec<_>>(), [c]);
        assert!(manager.get_any(indirect).is_some());
    }

    #[test]
    fn killed_direct_buffer_keeps_unlock_predicate_state() {
        let (mut manager, a, _, _) = setup();
        let buffer = manager.get_mut(a).unwrap();
        buffer.set_buffer_local("buffer-file-truename", Value::string("/cursor-dead/a"));
        buffer.text.set_modification_state(17, 15, 4);
        assert!(manager.kill_buffer(a));
        let dead = manager.get_any(a).unwrap();
        assert_eq!(
            (
                dead.modified_tick(),
                dead.chars_modified_tick(),
                dead.save_modified_tick()
            ),
            (17, 15, 4)
        );
        assert!(
            dead.buffer_local_value("buffer-file-truename")
                .unwrap()
                .is_string()
        );
        assert!(manager.get(a).is_none());
    }
}
