use crate::emacs_core::value::Value;

/// Trait for types that hold GC-managed `Value` references.
///
/// Each runtime subsystem implements this to enumerate all `Value`s it holds,
/// so the collector can discover every live object reachable from explicit
/// runtime roots.
pub trait GcTrace {
    /// Push all `Value` references held by `self` into `roots`.
    fn trace_roots(&self, roots: &mut Vec<Value>);

    /// Visit all `Value` references held by `self` without requiring the
    /// caller to materialize an intermediate root vector.
    fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        let mut roots = Vec::new();
        self.trace_roots(&mut roots);
        for root in roots {
            visit(root);
        }
    }
}
