//! GNU's image-failure diagnostics, as values rather than as lost `Option`s.
//!
//! GNU never lets a failed image load disappear. Every refusal inside a loader
//! goes through `image_error` (`src/image.c:1414`), which is `vadd_to_log`:
//! the text lands in `*Messages*` and redisplay carries on with a placeholder.
//! The three callers that word those refusals — `image_not_found_error`
//! (`src/image.c:1427`), `image_invalid_data_error` (`src/image.c:1420`) and
//! `image_size_error` (`src/image.c:1432`) — are the vocabulary this type
//! mirrors.
//!
//! Carrying the *cause* rather than a `String` is what makes "a decode failed
//! and nobody was told" unrepresentable: a failure that reaches the display
//! side is a value here, and every value here has GNU text. There is no
//! variant meaning "something went wrong" with nothing to say.
//!
//! The text is GNU's C-level text, with the grave accents its source literals
//! carry (``Cannot find image file `%s'``). GNU's `vadd_to_log` runs the format
//! through `Fformat_message`, which applies `text-quoting-style`; the evaluator
//! does the same when it logs these (see
//! `Context::log_pending_image_diagnostics`), so a diagnostic must not be
//! pre-quoted here.

use std::fmt::{self, Display, Formatter};

/// GNU's name for an image type, as its loaders spell it in diagnostics.
///
/// A fixed table rather than an uppercasing of the Lisp symbol: GNU writes
/// these names as string literals inside each loader's error call
/// (`"Not a PNG file: `%s'"`, `"PNG error: %s"`), and the two do not always
/// agree. The declared type is what matters, not the bytes — an image declared
/// `png` whose bytes are a JPEG is a PNG the loader refused, and GNU says so.
#[derive(Clone, Debug, PartialEq, Eq, Hash, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum ImageFormatName {
    Png,
    Jpeg,
    Gif,
    Tiff,
    Xpm,
    Xbm,
    Pbm,
    Webp,
    Svg,
    Imagemagick,
    Postscript,
    NativeImage,
    /// A type this build has no table entry for, under the Lisp symbol's name.
    #[strum(default)]
    Other(String),
}

impl ImageFormatName {
    /// Map a Lisp image type symbol (`png`, `native-image`) to GNU's spelling.
    #[must_use]
    pub fn from_lisp_type(name: &str) -> Self {
        Self::from(name)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Tiff => "TIFF",
            Self::Xpm => "XPM",
            Self::Xbm => "XBM",
            Self::Pbm => "PBM",
            Self::Webp => "WEBP",
            Self::Svg => "SVG",
            Self::Imagemagick => "IMAGEMAGICK",
            Self::Postscript => "POSTSCRIPT",
            Self::NativeImage => "NATIVE-IMAGE",
            Self::Other(name) => name,
        }
    }

    /// Whether this is the type GNU's own `image_error` calls name in a
    /// `Not a <TYPE> file:` / `Not a <TYPE> image:` diagnostic.
    ///
    /// Only PNG and PBM are worded that way (`src/image.c:8302`, `:8323`,
    /// `:7643`, `:7677`); every other loader reports a signature mismatch
    /// through `image_invalid_data_error` instead, which names the spec or the
    /// data and not the type. Formatting the wrong one would invent a message
    /// GNU never prints.
    #[must_use]
    pub fn words_signature_mismatch(&self) -> bool {
        matches!(self, Self::Png | Self::Pbm)
    }
}

impl Display for ImageFormatName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a `:data` image is called when GNU has to name it.
///
/// GNU's `:file` arms pass the file name to `image_not_found_error` and the
/// `:data` arms pass the *whole spec* as a Lisp object (`image_error ("Not a
/// PNG image: `%s'", img->spec)`, `src/image.c:8323`), because a data image has
/// no name to report. The two print differently and the difference is the
/// user's only clue which image the message is about, so the choice is a
/// value rather than a formatting decision at each call site.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageDiagnosticSubject {
    /// The image's `:file` value (`image_spec_value (spec, QCfile, NULL)`).
    File(String),
    /// The printed image specification, for a source with no file.
    Spec(String),
}

impl ImageDiagnosticSubject {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::File(path) | Self::Spec(path) => path,
        }
    }
}

impl Display for ImageDiagnosticSubject {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What GNU calls the source a load command is about.
///
/// GNU words its image diagnostics with the *declared* type and with the
/// subject the source actually has. The type is a fact about the request, not
/// about the bytes: an image declared `png` whose bytes are a JPEG is a PNG its
/// loader refused, and GNU says so (`Not a PNG file: `%s'`, `src/image.c:8302`).
/// The subject differs the same way — a `:data` image has no file to name, so
/// GNU prints the whole specification instead (`src/image.c:8323`). Both have
/// to reach the loader that words the failure.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageLoadIdentity {
    format: ImageFormatName,
    subject: ImageDiagnosticSubject,
}

impl ImageLoadIdentity {
    #[must_use]
    pub const fn new(format: ImageFormatName, subject: ImageDiagnosticSubject) -> Self {
        Self { format, subject }
    }

    /// The image's declared GNU type, as its loaders spell it.
    #[must_use]
    pub const fn format(&self) -> &ImageFormatName {
        &self.format
    }

    /// What GNU names when it has to say which image failed.
    #[must_use]
    pub const fn subject(&self) -> &ImageDiagnosticSubject {
        &self.subject
    }

    /// An identity for a source that arrived with no specification at all.
    ///
    /// Only the specification-free load entry points use this. GNU has no such
    /// path — every image it loads comes from a Lisp specification that names
    /// its type — so a failure here has no GNU wording to borrow, and
    /// [`Self::is_unspecified`] is what stops one from being dressed in some.
    #[must_use]
    pub fn unspecified() -> Self {
        Self {
            format: ImageFormatName::Other(String::new()),
            subject: ImageDiagnosticSubject::Spec(String::new()),
        }
    }

    /// Whether this source came with no specification to word a failure with.
    #[must_use]
    pub fn is_unspecified(&self) -> bool {
        matches!((&self.format, &self.subject), (ImageFormatName::Other(name), ImageDiagnosticSubject::Spec(printed))
            if name.is_empty() && printed.is_empty())
    }
}

/// One GNU image-failure diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageDiagnostic {
    /// GNU `image_not_found_error` (`src/image.c:8285`, `:1427`):
    /// ``Cannot find image file `PATH'``.
    ///
    /// GNU reaches this when `image_find_image_file` finds nothing *or* when
    /// the found file will not open, so it is the "the bytes are not there"
    /// verdict, not the "there are no bytes like that" one.
    FileNotFound { file: String },
    /// The bytes are not the declared format.
    ///
    /// GNU words this two ways and the subject decides which: the `:file` arms
    /// pass the file to `image_error ("Not a PNG file: `%s'", file)`
    /// (`src/image.c:8302`) and the `:data` arms pass the whole spec
    /// (`src/image.c:8323`), because a data image has no file to name. Only
    /// PNG and PBM have this arm at all; every other loader reports a
    /// signature mismatch as invalid data.
    NotAFormat {
        format: ImageFormatName,
        subject: ImageDiagnosticSubject,
    },
    /// The declared format's loader recognised the source and then failed
    /// inside it: GNU's `PNG error: %s` / `Error reading JPEG image ...`.
    FormatError {
        format: ImageFormatName,
        detail: String,
    },
    /// GNU `image_size_error` (`src/image.c:1432`), raised by
    /// `check_image_size` (`src/image.c:1811`) for a source over
    /// `max-image-size`.
    InvalidSize,
    /// The bytes decoded, and Neomacs still could not turn them into something
    /// drawable.
    ///
    /// This variant exists to keep a gap visible rather than to fill it: GNU
    /// has no single sentence for this case, because its loaders report their
    /// own allocation failures in their own words (`Unable to create X
    /// pixmap`, `Unable to allocate X image`). Dressing a Neomacs limitation
    /// in GNU's vocabulary would make a divergence look like compatibility.
    NotDrawable,
}

impl ImageDiagnostic {
    /// GNU's `image_size_error` text, verbatim from `src/image.c:1434`.
    pub const INVALID_SIZE_MESSAGE: &'static str = "Invalid image size (see `max-image-size')";

    /// The diagnostic as GNU's `image_error` would word it, before
    /// `text-quoting-style` rewrites the grave accents.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::FileNotFound { file } => format!("Cannot find image file `{file}'"),
            Self::NotAFormat { format, subject } if format.words_signature_mismatch() => {
                let noun = match subject {
                    ImageDiagnosticSubject::File(_) => "file",
                    ImageDiagnosticSubject::Spec(_) => "image",
                };
                format!("Not a {format} {noun}: `{subject}'")
            }
            // `image_invalid_data_error` (`src/image.c:1420`) is what every
            // other loader reports a signature mismatch through.
            Self::NotAFormat { subject, .. } => format!("Invalid image data `{subject}'"),
            Self::FormatError { format, detail } => match format {
                // GNU's PNG loader hands libpng's message straight to
                // `image_error ("PNG error: %s", ...)` (`src/image.c:8184`).
                ImageFormatName::Png => format!("PNG error: {detail}"),
                // The JPEG loader names the spec as well as libjpeg's text.
                ImageFormatName::Jpeg => {
                    format!("Error reading JPEG image: {detail}")
                }
                other => format!("{other} error: {detail}"),
            },
            Self::InvalidSize => Self::INVALID_SIZE_MESSAGE.to_owned(),
            Self::NotDrawable => "Unable to create image".to_owned(),
        }
    }
}

impl Display for ImageDiagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

#[cfg(test)]
#[path = "image_diagnostic/tests/diagnostic_test.rs"]
mod diagnostic_tests;

impl ImageLoadIdentity {
    /// The diagnostic for bytes that are not this image's declared format.
    ///
    /// The subject decides GNU's noun: a `:file` gives `Not a PNG file:`, a
    /// `:data` gives `Not a PNG image:`, and a loader with neither arm for its
    /// type reports invalid data instead.
    #[must_use]
    pub fn wrong_format(&self) -> ImageDiagnostic {
        ImageDiagnostic::NotAFormat {
            format: self.format.clone(),
            subject: self.subject.clone(),
        }
    }

    /// The diagnostic for a source whose bytes could not be read at all.
    #[must_use]
    pub fn not_found(&self) -> ImageDiagnostic {
        ImageDiagnostic::FileNotFound {
            file: self.subject.as_str().to_owned(),
        }
    }
}
