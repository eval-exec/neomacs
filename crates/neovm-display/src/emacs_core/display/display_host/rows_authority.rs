//! Audited type authority for the native services of a numeric Rows query.
use super::DisplayHost;
use std::any::TypeId;

/// Unnameable outside the private rows_authority module's parent subtree.
///
/// `pub` is required because DisplayHost's provided method is public; the
/// private containing module, lack of a re-export and private field keep this
/// type sealed. In particular, do not expose an associated-type alias or
/// other public naming path: external host implementations must inherit the
/// concrete-Self default rather than return a forwarded identity.
#[derive(Clone, Copy, Debug)]
pub struct ActualHostType(TypeId);

impl ActualHostType {
    /// Only the trusted DisplayHost default needs to construct this value.
    pub(super) fn of<T: ?Sized + 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// Type-level contract for an audited host, not a geometry or owner proof.
///
/// The authority can be copied into the existing frontend query closure and
/// passed explicitly; it requires no Context, engine or TLS policy field.
/// It matches the sealed provided method's actual concrete TypeId, not an
/// overrideable host declaration or trait-object vtable address. Unknown hosts
/// cannot safely mint a token.
/// This immutable type contract may cross mutators; it owns no host or Lisp
/// state. Each consumer still validates its own installed Context and host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InertRowsHostAuthority {
    host_type: TypeId,
}

static_assertions::assert_impl_all!(InertRowsHostAuthority: Send, Sync, Copy, std::fmt::Debug);

impl InertRowsHostAuthority {
    /// Approve the concrete native host type T for a closed numeric Rows scope.
    ///
    /// # Safety
    /// Audit T's geometry/image/font services for EVERY instance and state,
    /// including future changes to its implementation. image_catalog and
    /// image_catalog_shared must always return None. Font lookup/open/coverage
    /// and metric services must never provide external geometry to this scope.
    /// These accessors must be inert native operations: no Lisp, callbacks,
    /// reentry, collection/safepoints, owner destruction, mutable external
    /// resources or other observable side effects. This is not an assertion
    /// about popup/clipboard methods, which the closed Rows producer excludes.
    /// The consumer must still exclude those other entry points, actual cold
    /// face realization, dynamic forms and every unproved display dependency.
    /// It must validate the ACTUAL installed host's sealed TypeId before
    /// trusting this contract and recheck all owner/GC/source/freshness guards.
    /// A token alone permits no saved pointer access or query-cache admission.
    pub unsafe fn for_type<T: DisplayHost + 'static>() -> Self {
        Self {
            host_type: TypeId::of::<T>(),
        }
    }

    pub(crate) fn accepts(self, host: &(dyn DisplayHost + 'static)) -> bool {
        // The private return type seals the provided method against external
        // overrides. Its default computes TypeId for the actual implementor;
        // this is not a forwardable public identity declaration. The 'static
        // bound is on the installed object type, never on this short borrow.
        actual_rows_host_type(host) == self.host_type
    }

    pub(crate) fn host_type(self) -> TypeId {
        self.host_type
    }
}

/// Crate-only extraction for original-epoch host revalidation. No public
/// caller can construct an identity or declare a forwarded type through it.
pub(crate) fn actual_rows_host_type(host: &(dyn DisplayHost + 'static)) -> TypeId {
    host.__rows_host_identity().0
}
