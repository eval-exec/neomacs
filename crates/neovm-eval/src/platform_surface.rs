//! The GNU build surface of the host this binary targets.
//!
//! GNU's platform Lisp surface is a CONFIGURE-TIME fact: an X11 (GTK3) build
//! compiles and runs `syms_of_xterm`/`syms_of_xfns`, loads `lisp/term/x-win.el`
//! and the xselect/xsettings objects; an NS build runs `syms_of_nsterm`/
//! `syms_of_nsselect` and loads `lisp/term/ns-win.el` (`src/emacs.c:2360-2436`,
//! `lisp/loadup.el:304-362`); a w32 build its own family. Neomacs presents one
//! GUI on every host, so its platform tables must model the REFERENCE's build
//! surface, not the running kernel's capabilities.
//!
//! [`PlatformSurface`] is derived in exactly one place ([`PlatformSurface::host`])
//! and every per-host table (the `DEFVAR_BOOL` rows, the C syms blocks, the
//! Lisp term-file aliases) carries it and reads it with an exhaustive match;
//! there are no ad-hoc `cfg!` probes at those sites.
//!
//! Test and `test-support` builds may install a scoped override through the
//! signed eval `test_support::authority` fence so both surfaces can be
//! asserted on either host. Production never reads a runtime override: the
//! override module does not exist without `cfg(any(test, feature =
//! "test-support"))`.

/// The GNU reference build surface this binary models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlatformSurface {
    /// GTK3/X11: XOBJ (`xterm.o xfns.o xselect.o ...`), `term/x-win.el`.
    X11,
    /// AppKit/NS: NS_OBJC_OBJ (`nsterm.o nsfns.o nsselect.o ...`),
    /// `term/ns-win.el`.
    Ns,
    /// Windows GUI: the w32 object/DEFVAR family.
    W32,
}

impl PlatformSurface {
    /// The host's reference surface. The ONLY cfg dispatch for this fact.
    pub(crate) const fn host() -> Self {
        if cfg!(windows) {
            Self::W32
        } else if cfg!(target_os = "macos") {
            Self::Ns
        } else {
            Self::X11
        }
    }
}

/// Which surfaces declare a table entry (one bit per [`PlatformSurface`]).
///
/// Rows carry this instead of per-surface booleans; the registration loop
/// reads it through [`PlatformSurface`]'s exhaustive match in
/// [`SurfaceSet::contains`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceSet(u8);

impl SurfaceSet {
    /// Declared on every reference surface.
    pub(crate) const ALL: Self = Self(0b111);
    /// X11 only (xterm.c/xfns.c/xselect.c/xsettings.c objects, `term/x-win.el`).
    pub(crate) const X11: Self = Self(0b001);
    /// NS only (nsterm.m/nsfns.m/nsselect.m objects).
    pub(crate) const NS: Self = Self(0b010);

    pub(crate) const fn contains(self, surface: PlatformSurface) -> bool {
        let bit = match surface {
            PlatformSurface::X11 => 0b001,
            PlatformSurface::Ns => 0b010,
            PlatformSurface::W32 => 0b100,
        };
        self.0 & bit != 0
    }
}

/// The surface this build must present.
pub(crate) fn platform_surface() -> PlatformSurface {
    #[cfg(any(test, feature = "test-support"))]
    {
        if let Some(surface) = test_override::current() {
            return surface;
        }
    }
    PlatformSurface::host()
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod test_override {
    //! Scoped, thread-local surface override for tests. Compiled ONLY for
    //! test/test-support builds; production has no override path.

    use super::PlatformSurface;
    use std::cell::Cell;

    thread_local! {
        // The carrier-TLS class table is keyed by (module, name): this
        // test-support-only override is a SourceOnly record at
        // neovm-eval/src/platform_surface.rs, RAII-restored and holding no
        // Lisp Values.
        static PLATFORM_SURFACE_TEST_OVERRIDE: Cell<Option<PlatformSurface>> =
            const { Cell::new(None) };
    }

    pub(crate) fn current() -> Option<PlatformSurface> {
        PLATFORM_SURFACE_TEST_OVERRIDE.with(Cell::get)
    }

    /// Restores the previous override (which may itself be `None`) on drop,
    /// so nested scopes compose.
    pub(crate) struct Guard(Option<PlatformSurface>);

    impl Drop for Guard {
        fn drop(&mut self) {
            PLATFORM_SURFACE_TEST_OVERRIDE.with(|cell| cell.set(self.0));
        }
    }

    pub(crate) fn install(surface: PlatformSurface) -> Guard {
        let previous = PLATFORM_SURFACE_TEST_OVERRIDE.with(|cell| cell.replace(Some(surface)));
        Guard(previous)
    }
}
