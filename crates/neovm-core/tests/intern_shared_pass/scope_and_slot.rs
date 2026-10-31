#[path = "../intern_shared_ui/support/source.rs"]
mod source;
use source::domain::InternReadDomain;
use source::slot::{InstalledScope, ScopeSlot};
fn main() {
    let domain = InternReadDomain::new();
    let snapshot = vec![3, 5];
    {
        let scope = domain.try_scope().unwrap();
        let installed = InstalledScope::new(&scope);
        let mut slot = ScopeSlot::new();
        {
            let mut outer = slot.install(&installed);
            { let _nested = outer.install(&installed); }
            assert!(outer.slot().ensure_empty().is_err());
            assert_eq!(installed.borrow(&snapshot).get()[1], 5);
        }
        slot.ensure_empty().unwrap();
    }
    domain.try_grace().unwrap().verify_domain(&domain).unwrap();
}
