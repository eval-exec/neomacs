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

/// The POSIX TZ rules of the C library this binary is linked against.
pub(crate) const POSIX_TZ_SEMANTICS: PosixTzSemantics = if cfg!(target_os = "macos") {
    PosixTzSemantics::Darwin
} else {
    PosixTzSemantics::Glibc
};
