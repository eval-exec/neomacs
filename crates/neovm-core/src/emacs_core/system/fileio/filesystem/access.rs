//! Validated permissions for access checks on existing filesystem entries.

/// Access predicate requested by a Lisp filesystem primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessMode {
    /// Check all requested permissions on an existing entry, never its parent.
    Existing(AccessPermissions),
    Exists,
    Read,
    /// The entry is writable, or a missing entry can be created in its parent.
    WriteOrCreate,
    Execute,
    ReadAndSearch,
}

/// A POSIX R_OK/W_OK/X_OK combination. Zero checks existence only.
///
/// Unlike WriteOrCreate, no combination permits an absent entry. The private
/// representation prevents invalid Lisp access masks reaching an adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessPermissions(u8);

impl AccessPermissions {
    pub fn from_posix_mask(mask: i64) -> Option<Self> {
        (0..=7).contains(&mask).then_some(Self(mask as u8))
    }

    pub fn requires_read(self) -> bool {
        self.0 & 4 != 0
    }
    pub fn requires_write(self) -> bool {
        self.0 & 2 != 0
    }
    pub fn requires_execute(self) -> bool {
        self.0 & 1 != 0
    }

    /// Evaluate all requested permissions after the adapter establishes existence.
    pub fn is_satisfied_by(self, readable: bool, writable: bool, executable: bool) -> bool {
        (!self.requires_read() || readable)
            && (!self.requires_write() || writable)
            && (!self.requires_execute() || executable)
    }
}
