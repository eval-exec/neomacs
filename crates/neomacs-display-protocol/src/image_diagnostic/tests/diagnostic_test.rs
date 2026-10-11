//! The text these diagnostics produce is a compatibility surface: it is what a
//! user reads in `*Messages*` when an image does not appear, and GNU's users
//! have read exactly these lines for decades. Each case below is the string
//! observed from GNU 31.1 `emacs -Q` under Xvfb for the matching failure
//! (recorded in `tmp/imgmsg/`), with the `text-quoting-style` rewrite the
//! evaluator applies afterwards undone.

use super::{ImageDiagnostic, ImageDiagnosticSubject, ImageFormatName};

/// GNU 31.1: `(insert-image (create-image "…/does-not-exist.png" 'png))`
/// then a redisplay, `*Messages*` line 1.
#[test]
fn missing_file_matches_gnu_31_1() {
    let diagnostic = ImageDiagnostic::FileNotFound {
        file: "/tmp/imgmsg/fixtures/does-not-exist.png".to_owned(),
    };
    assert_eq!(
        diagnostic.message(),
        "Cannot find image file `/tmp/imgmsg/fixtures/does-not-exist.png'"
    );
}

/// GNU 31.1: the same probe against a file whose bytes are not a PNG.
#[test]
fn undecodable_file_names_the_declared_format_and_the_file() {
    let diagnostic = ImageDiagnostic::NotAFormat {
        format: ImageFormatName::Png,
        subject: ImageDiagnosticSubject::File("/tmp/imgmsg/fixtures/notimage.png".to_owned()),
    };
    assert_eq!(
        diagnostic.message(),
        "Not a PNG file: `/tmp/imgmsg/fixtures/notimage.png'"
    );
}

/// GNU 31.1, `:data` arm: GNU names the *spec*, because a data image has no
/// file to report.
#[test]
fn undecodable_data_names_the_spec_not_a_file() {
    let diagnostic = ImageDiagnostic::NotAFormat {
        format: ImageFormatName::Png,
        subject: ImageDiagnosticSubject::Spec(
            "(image :type png :data definitely not an image :scale default)".to_owned(),
        ),
    };
    assert_eq!(
        diagnostic.message(),
        "Not a PNG image: `(image :type png :data definitely not an image :scale default)'"
    );
}

/// GNU 31.1: a truncated PNG fails *inside* libpng, which reports through
/// `image_error ("PNG error: %s", ...)`. GNU's libpng text for a stream that
/// ends early is `Read error`.
#[test]
fn truncated_png_reports_inside_the_loader() {
    let diagnostic = ImageDiagnostic::FormatError {
        format: ImageFormatName::Png,
        detail: "Read error".to_owned(),
    };
    assert_eq!(diagnostic.message(), "PNG error: Read error");
}

/// GNU 31.1: `(setq max-image-size 100)` then an image wider than that.
#[test]
fn oversize_refusal_is_word_for_word_gnu() {
    assert_eq!(
        ImageDiagnostic::InvalidSize.message(),
        "Invalid image size (see `max-image-size')"
    );
}

/// The Lisp type symbol is not the diagnostic name: GNU writes the loader's
/// name as a literal, and `native-image` is spelled with a dash.
#[test]
fn declared_types_map_to_gnus_spelling() {
    for (symbol, format, diagnostic_name) in [
        ("png", ImageFormatName::Png, "PNG"),
        ("jpeg", ImageFormatName::Jpeg, "JPEG"),
        ("gif", ImageFormatName::Gif, "GIF"),
        ("tiff", ImageFormatName::Tiff, "TIFF"),
        ("xpm", ImageFormatName::Xpm, "XPM"),
        ("xbm", ImageFormatName::Xbm, "XBM"),
        ("pbm", ImageFormatName::Pbm, "PBM"),
        ("webp", ImageFormatName::Webp, "WEBP"),
        ("svg", ImageFormatName::Svg, "SVG"),
        ("imagemagick", ImageFormatName::Imagemagick, "IMAGEMAGICK"),
        ("postscript", ImageFormatName::Postscript, "POSTSCRIPT"),
        ("native-image", ImageFormatName::NativeImage, "NATIVE-IMAGE"),
    ] {
        assert_eq!(ImageFormatName::from_lisp_type(symbol), format);
        assert_eq!(ImageFormatName::from(symbol), format);
        assert_eq!(format.as_str(), diagnostic_name);
        assert_eq!(format.to_string(), diagnostic_name);
    }
}

#[test]
fn unknown_image_type_names_are_preserved_without_normalization() {
    for symbol in [
        "bmp",
        "PNG",
        "Png",
        "NativeImage",
        "native_image",
        " png",
        "png ",
        "other",
        "",
        "未知",
    ] {
        let format = ImageFormatName::from_lisp_type(symbol);
        assert_eq!(format, ImageFormatName::Other(symbol.to_owned()));
        assert_eq!(ImageFormatName::from(symbol), format);
        assert_eq!(format.as_str(), symbol);
        assert_eq!(format.to_string(), symbol);
    }
}

/// Only PNG and PBM have a `Not a <TYPE> file:` arm in GNU; a loader that has
/// none must not be given one.
#[test]
fn only_the_formats_gnu_words_this_way_do_so() {
    assert!(ImageFormatName::Png.words_signature_mismatch());
    assert!(ImageFormatName::Pbm.words_signature_mismatch());
    assert!(!ImageFormatName::Jpeg.words_signature_mismatch());
    assert!(!ImageFormatName::Svg.words_signature_mismatch());
}

/// Every diagnostic has text: there is no "failed, nothing to say" value.
#[test]
fn every_variant_carries_gnu_text() {
    let all = [
        ImageDiagnostic::FileNotFound {
            file: "f".to_owned(),
        },
        ImageDiagnostic::NotAFormat {
            format: ImageFormatName::Pbm,
            subject: ImageDiagnosticSubject::File("f".to_owned()),
        },
        ImageDiagnostic::NotAFormat {
            format: ImageFormatName::Png,
            subject: ImageDiagnosticSubject::Spec("s".to_owned()),
        },
        ImageDiagnostic::FormatError {
            format: ImageFormatName::Gif,
            detail: "d".to_owned(),
        },
        ImageDiagnostic::InvalidSize,
    ];
    for diagnostic in all {
        assert!(!diagnostic.message().is_empty());
    }
}

/// A loader with no `Not a <TYPE> file:` arm reports a signature mismatch the
/// way GNU does: as invalid data, naming the subject and not the type.  The
/// declared type must not leak into a sentence GNU never writes.
#[test]
fn a_loader_gnu_does_not_word_this_way_reports_invalid_data() {
    let diagnostic = ImageDiagnostic::NotAFormat {
        format: ImageFormatName::Jpeg,
        subject: ImageDiagnosticSubject::File("/tmp/x.jpg".to_owned()),
    };
    assert_eq!(diagnostic.message(), "Invalid image data `/tmp/x.jpg'");
}
