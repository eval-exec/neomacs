/// Stand-in for `neovm-value`'s `intern::NameId`.
///
/// `publication.rs` names this type from `retained_name_at_for_test`, which
/// the grace model compiles and does not call.
#[derive(Clone, Copy)]
pub struct NameId;
