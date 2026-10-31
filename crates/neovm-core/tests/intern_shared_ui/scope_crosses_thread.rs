#[path = "support/source.rs"]
mod source;
use source::domain::InternReadDomain;
fn main() {
    let domain = InternReadDomain::new();
    std::thread::scope(|threads| {
        let scope = domain.try_scope().unwrap();
        threads.spawn(move || drop(scope));
    });
}
