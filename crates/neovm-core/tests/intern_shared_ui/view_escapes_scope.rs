#[path = "support/source.rs"]
mod source;
use source::domain::InternReadDomain;
fn main() {
    let domain = InternReadDomain::new();
    let snapshot = vec![1, 2, 3];
    let view;
    {
        let scope = domain.try_scope().unwrap();
        view = scope.borrow(&snapshot);
    }
    assert_eq!(view.get()[0], 1);
}
