use super::Retirements;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Tooltip,
    Submenu,
    Root,
}

struct Resource(Role, Rc<RefCell<Vec<Role>>>);

impl Drop for Resource {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}

#[test]
fn detached_children_survive_until_commit_and_retire_before_parents() {
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let mut retired = Retirements::default();
    retired.push(Resource(Role::Tooltip, dropped.clone()));
    retired.push(Resource(Role::Submenu, dropped.clone()));
    retired.push(Resource(Role::Root, dropped.clone()));
    assert!(dropped.borrow().is_empty());
    retired.commit();
    assert_eq!(
        *dropped.borrow(),
        [Role::Tooltip, Role::Submenu, Role::Root]
    );
    retired.commit();
    assert_eq!(dropped.borrow().len(), 3);
}

#[test]
fn shutdown_drains_uncommitted_retirements_in_order() {
    let dropped = Rc::new(RefCell::new(Vec::new()));
    {
        let mut retired = Retirements::default();
        retired.push(Resource(Role::Submenu, dropped.clone()));
        retired.push(Resource(Role::Root, dropped.clone()));
    }
    assert_eq!(*dropped.borrow(), [Role::Submenu, Role::Root]);
}
