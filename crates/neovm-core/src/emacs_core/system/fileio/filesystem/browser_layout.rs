//! Stable virtual path identities shared by the browser host and Lisp runtime.

/// Browser filesystem namespace fixed at compile time for `neomacs-wasm`.
///
/// Keeping these paths together prevents startup environment variables,
/// filesystem mounts, and invocation metadata from describing different
/// virtual roots.
pub enum BrowserFileSystemLayout {}

impl BrowserFileSystemLayout {
    /// Read-only product resources mounted from the authenticated bundle.
    pub const RUNTIME_ROOT: &'static str = "/neomacs";
    /// Origin-private persistent storage exposed as the editor's home.
    pub const HOME: &'static str = "/neomacs-fake";
    /// Session-local, non-persistent scratch storage.
    pub const TEMPORARY: &'static str = "/tmp";
    /// Browser-local XDG configuration root.
    pub const XDG_CONFIG_HOME: &'static str = "/neomacs-fake/.config";
    /// Browser-local XDG cache root.
    pub const XDG_CACHE_HOME: &'static str = "/neomacs-fake/.cache";
    /// Browser-local XDG application-data root.
    pub const XDG_DATA_HOME: &'static str = "/neomacs-fake/.local/share";
    /// Browser-local XDG application-state root.
    pub const XDG_STATE_HOME: &'static str = "/neomacs-fake/.local/state";
}
