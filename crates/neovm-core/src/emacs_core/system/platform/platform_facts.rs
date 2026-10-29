//! Platform properties the port must follow, each one a typed answer.
//!
//! GNU reads these from the host -- some at configure time, some by asking the
//! C library -- and one `#ifdef` or libc call then decides the behaviour on
//! every path.  Rust has no portable way to ask the same question, so the
//! answer lives here exactly once and tests ask it instead of spelling a
//! `cfg (target_os)` assumption of their own.
#![cfg_attr(not(test), allow(dead_code))] // test-facing inventory today

/// Whether this platform's file system can represent a file name containing
/// bytes that are not valid UTF-8.
///
/// APFS and HFS+ store names as Unicode, so the kernel rejects such a name
/// with `EILSEQ` before any file is created or opened.  GNU does not
/// synthesize or sanitize the name: its primitives pass the encoded bytes to
/// the syscall and report the failure through `report_file_error`, which
/// fills the error data with `strerror (errno)` and the name
/// (src/fileio.c:298-301; callers such as `Fwrite_region` at :5656 and
/// `Finsert_file_contents` at :5312).  On a GNU/Linux ext4 file system the
/// same bytes are ordinary name bytes and the operation succeeds.
///
/// Tests that exercise raw-byte names must therefore assert the round trip on
/// [`Representable`](RawByteFilenames::Representable) and GNU's `file-error`
/// (with the platform's `strerror (EILSEQ)` text) on
/// [`Rejected`](RawByteFilenames::Rejected).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RawByteFilenames {
    /// File names are arbitrary byte strings outside NUL and `/`.
    Representable,
    /// File names must be valid Unicode; non-UTF-8 bytes fail with EILSEQ.
    Rejected,
}

/// The answer for the file systems this build's tests run on.
///
/// Windows file names are UTF-16 and likewise cannot carry an arbitrary byte
/// string, so it shares APFS's answer.  This is a property of the platform's
/// native file system, which is what `std::env::temp_dir` lands on in CI.
pub(crate) const RAW_BYTE_FILENAMES: RawByteFilenames =
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        RawByteFilenames::Rejected
    } else {
        RawByteFilenames::Representable
    };

/// Which frozen `format` transcript set this host's C library produces.
///
/// GNU decides whether an integer argument is formatted as `long double` from
/// the host's `LDBL_MANT_DIG` vs `DBL_MANT_DIG` (src/editfns.c:3905-3946:
/// `if (LDBL_MANT_DIG <= DBL_MANT_DIG)` stays on `double`), and it hands every
/// float conversion to the host C library's `sprintf` (:3948-3955).  The two
/// properties move together on the hosts this port tests -- GNU/Linux has an
/// 80-bit long double and glibc's NaN text, Apple arm64 has `long double ==
/// double` and Darwin's NaN/padding text -- so the frozen transcripts were
/// recorded from GNU on each shape with the canonical recording locale.
///
/// A host shape nobody recorded (a BSD, say) must record its own set rather
/// than borrow one: the fixtures encode the C library's bytes, not an
/// inference from the platform name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloatTranscriptSet {
    /// Recorded from GNU 31.1 built on GNU/Linux (glibc, 80-bit long double).
    GnuLinux,
    /// Recorded from GNU 31.1 built on Apple arm64 (Darwin libc, long
    /// double == double).
    Apple,
}

impl FloatTranscriptSet {
    /// Pick the frozen transcript recorded on this host shape.
    ///
    /// `const` so a fixture table can be assembled at compile time from both
    /// recordings.
    pub(crate) const fn pick(self, gnu_linux: &'static str, apple: &'static str) -> &'static str {
        match self {
            Self::GnuLinux => gnu_linux,
            Self::Apple => apple,
        }
    }
}

/// The transcript set for the host this binary was built for.
pub(crate) const FLOAT_TRANSCRIPTS: FloatTranscriptSet = if cfg!(target_os = "macos") {
    FloatTranscriptSet::Apple
} else {
    FloatTranscriptSet::GnuLinux
};

/// Which C library's POSIX TZ-string rules GNU's `tzlookup` inherits.
///
/// GNU hands an explicit numeric or `(OFFSET NAME)` ZONE to libc's `tzalloc`
/// as a POSIX TZ string (src/timefns.c:240 `tzlookup`; the `tzalloc
/// (zone_string)` call at :306-314, whose comment records that `tzalloc`
/// mishandles some POSIX strings and GNU falls back to tzdb).  The parsing
/// rules are therefore the C library's, and the two libraries this port's
/// tests run against disagree: glibc rejects an abbreviation shorter than
/// three characters and clamps offsets to +/-24h, while Darwin's libc keeps
/// both and turns an out-of-range numeric zone into UTC instead of clamping.
/// Both behaviours were read out of GNU 31.1 itself, on each host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PosixTzSemantics {
    /// GNU/Linux glibc: strict abbreviations, +/-24h clamp.
    Glibc,
    /// Apple libc: abbreviations and offsets pass through, numeric overflow
    /// becomes UTC.
    Darwin,
}

/// Which `string-collate-lessp`/`string-collate-equalp` path GNU compiled.
///
/// GNU wraps the locale-aware path in
/// `#if defined __STDC_ISO_10646__ || defined WINDOWSNT`
/// (src/fns.c:628-645 for lessp, :679-694 for equalp).  glibc defines
/// `__STDC_ISO_10646__`, so GNU/Linux parses LOCALE with `newlocale` and
/// compares with `strcoll_l`; Darwin's libc does not define it, so macOS GNU
/// compiles the `else` arm and returns `Fstring_lessp`/`Fstring_equal`
/// directly -- LOCALE is not read, not even type-checked, and IGNORE-CASE is
/// ignored.  The platform decides this at configure time; the answer is
/// recorded once here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StringCollation {
    /// The C library advertises ISO 10646 collation: LOCALE-aware path.
    LocaleAware,
    /// GNU compiles the byte-comparison fallback: LOCALE is ignored.
    ByteComparison,
}

/// The collation path this binary's GNU reference was built with.
pub(crate) const STRING_COLLATION: StringCollation = if cfg!(target_vendor = "apple") {
    StringCollation::ByteComparison
} else {
    StringCollation::LocaleAware
};

/// The order the C compiler that built the reference GNU evaluates function
/// arguments in.
///
/// C leaves argument evaluation order unspecified, and GNU's Lisp surface can
/// observe it: `Ffile_attributes_lessp` is `Fstring_lessp (Fcar (f1),
/// Fcar (f2))` (src/dired.c:1137-1144), so with two bad arguments the
/// `wrong-type-argument` payload names whichever `Fcar` ran first.  GCC on
/// x86-64 evaluates right-to-left; the Apple Clang build of GNU evaluates
/// left-to-right.  Mirrored where a multi-argument C call can signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CArgumentEvaluation {
    /// GCC x86-64: the last argument's nested calls run first.
    RightToLeft,
    /// Apple Clang: the first argument's nested calls run first.
    LeftToRight,
}

/// The evaluation order of this host's GNU reference build.
pub(crate) const C_ARGUMENT_EVALUATION: CArgumentEvaluation = if cfg!(target_vendor = "apple") {
    CArgumentEvaluation::LeftToRight
} else {
    CArgumentEvaluation::RightToLeft
};

/// GNU's `system-configuration`: the `config.guess` triple of the host GNU
/// was configured on.
///
/// GNU records the value at configure time (configure.ac, the
/// `system-configuration` substitution).  On GNU/Linux `config.guess` answers
/// the canonical system triple (`x86_64-pc-linux-gnu`); on Darwin the
/// `arm64:Darwin:*` case builds `aarch64-apple-darwin$UNAME_RELEASE`
/// (build-aux/config.guess:1457-1458, `UNAME_RELEASE` from `uname -r`), which
/// is why the macOS reference reports `aarch64-apple-darwin27.0.0`.  A port
/// built on the same host has to reproduce that answer at run time; the
/// reference build manifest records the oracle's own value so
/// `check-gnu-oracle.sh` can prove the two agree.
pub(crate) fn system_configuration() -> String {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-pc-linux-gnu".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        match unix_kernel_release() {
            Some(release) => {
                let machine = option_env!("TARGET")
                    .and_then(|triple| triple.split_once("-apple-darwin"))
                    .map(|(machine, _)| machine)
                    .unwrap_or("aarch64");
                format!("{machine}-apple-darwin{release}")
            }
            // `uname` failing is the only way to get here; the build's own
            // triple is the closest stand-in for a value `config.guess`
            // would have produced.
            None => build_target_triple(),
        }
    }
    #[cfg(not(any(all(target_os = "linux", target_arch = "x86_64"), target_os = "macos")))]
    {
        build_target_triple()
    }
}

/// The build's own triple, spelled the way `config.guess` spells this host.
fn build_target_triple() -> String {
    option_env!("TARGET").map(str::to_owned).unwrap_or_else(|| {
        let os = match std::env::consts::OS {
            "linux" => "unknown-linux-gnu",
            "macos" => "apple-darwin",
            "windows" => "pc-windows-msvc",
            other => other,
        };
        format!("{}-{os}", std::env::consts::ARCH)
    })
}

#[cfg(target_os = "macos")]
fn unix_kernel_release() -> Option<String> {
    use std::ffi::CStr;
    let mut uts = unsafe { std::mem::zeroed::<libc::utsname>() };
    // SAFETY: `uts` is a valid out-parameter for uname.
    if unsafe { libc::uname(&mut uts) } != 0 {
        return None;
    }
    // SAFETY: uname fills release with a NUL-terminated string.
    Some(
        unsafe { CStr::from_ptr(uts.release.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// GNU's `PATH_BITMAPS` (`src/epaths.in:67`), the configure `bitmapdir`
/// substitution that initializes `x-bitmap-file-path`
/// (`src/image.c:13269-13270`, `decode_env_path (0, PATH_BITMAPS, 0)`).
///
/// configure keeps the built-in `/usr/include/X11/bitmaps` only when it did
/// not need `--x-includes` (configure.ac:2754-2755).  With a non-standard X11
/// prefix it accumulates just the include directories that contain
/// `X11/bitmaps` or `bitmaps` (:2757-2769); the accumulation can be empty,
/// and `decode_env_path` renders an empty element as `.` (:2758-2760,
/// src/emacs.c:3262-3305), which is why the macOS reference answers `(".")`.
///
/// GNU/Linux reference builds use the system X11, so they keep the default.
/// macOS has no system X11: the reference is built against Homebrew's X
/// libraries (scripts/ci/build-gnu-oracle.sh, `macos-x11`), whose include tree
/// carries the headers but no bitmap directory, so configure's accumulation
/// is empty.  A macOS reference built against XQuartz would answer that
/// tree's bitmap directory instead; the build manifest records the oracle's
/// own value and the cus-start oracle test re-reads it from the resolved
/// GNU, so a recipe change cannot pass unnoticed.
pub(crate) const X_BITMAP_FILE_PATH: &str = if cfg!(target_os = "macos") {
    ""
} else {
    "/usr/include/X11/bitmaps"
};

/// Whether the kernel raises `SIGCHLD` when a stopped child is continued.
///
/// GNU learns every child transition from one place -- `handle_child_signal`,
/// the only caller of `child_status_changed (pid, &status, WUNTRACED |
/// WCONTINUED)` (src/process.c:7691-7749).  POSIX requires `SIGCHLD` for a
/// continued child when the stop was reported, and Linux delivers it; Darwin
/// does not.  Measured on macOS 27: a child that stops itself delivers
/// `SIGCHLD`, and a later `SIGCONT` delivers none, while `waitpid` still
/// reports the `WCONTINUED` continuation (and reports one even when the child
/// was never stopped at all, e.g. a bare `SIGCONT` to an orphaned process
/// group, where Darwin ignores the `SIGTSTP`).
///
/// A port that polls `waitpid` instead of installing GNU's signal handler has
/// to answer the question the signal would have answered: a `WCONTINUED` the
/// kernel never signals must not become a status change, or every
/// `continue-process` on macOS runs the sentinel twice.
pub(crate) const SIGCHLD_ON_CHILD_CONTINUE: bool = !cfg!(target_vendor = "apple");

/// Whether the C library can start an asynchronous `getaddrinfo`
/// (`HAVE_GETADDRINFO_A`).
///
/// GNU's `make-network-process` takes the asynchronous DNS path only where
/// configure found `getaddrinfo_a` (src/process.c:4146-4185, glibc).  Without
/// it -- Darwin's libc -- the `:nowait` arm falls through to the synchronous
/// lookup and signals a name failure before the process exists
/// (`error ("%s", msg)`, src/process.c:4203-4205), so `:nowait` does not make
/// a bad hostname asynchronous.  The port must take the same branch: the
/// observable difference is `(make-network-process ... :nowait t)` returning
/// a `connect` process on glibc and signalling
/// "HOST/SERVICE nodename nor servname provided" on macOS.
pub(crate) const ASYNC_GETADDRINFO: bool = cfg!(all(target_os = "linux", target_env = "gnu"));

/// The POSIX TZ rules of the C library this binary is linked against.
pub(crate) const POSIX_TZ_SEMANTICS: PosixTzSemantics = if cfg!(target_os = "macos") {
    PosixTzSemantics::Darwin
} else {
    PosixTzSemantics::Glibc
};
