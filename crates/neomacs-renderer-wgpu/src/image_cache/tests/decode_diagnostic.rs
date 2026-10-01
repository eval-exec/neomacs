//! What a failed decode says about itself.
//!
//! The decode chain answers `Option` and each of its fallbacks — banded attempt,
//! whole-image decode, XPM, XBM, SVG — collapses the reason, so by the time
//! nothing is left the only surviving facts are the source and its bytes. These
//! are the sentences GNU's own loaders produce for the same four failures, as
//! observed from GNU 31.1 under Xvfb (`tmp/imgmsg/`).

use super::*;
use neomacs_display_protocol::image_diagnostic::{
    ImageDiagnostic, ImageDiagnosticSubject, ImageFormatName, ImageLoadIdentity,
};

fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tmp/imgmsg/fixtures")
        .join(name)
        .to_str()
        .expect("utf8 fixture path")
        .to_owned()
}

/// A `:file` PNG, named the way GNU's `:file` arm names it.
fn png_file_identity(path: &str) -> ImageLoadIdentity {
    ImageLoadIdentity::new(
        ImageFormatName::Png,
        ImageDiagnosticSubject::File(path.to_owned()),
    )
}

/// A `:data` PNG, which GNU names by its printed specification because it has
/// no file to report.
fn png_data_identity(printed_spec: &str) -> ImageLoadIdentity {
    ImageLoadIdentity::new(
        ImageFormatName::Png,
        ImageDiagnosticSubject::Spec(printed_spec.to_owned()),
    )
}

fn diagnostic_for_file(path: &str) -> ImageDiagnostic {
    let source = DecodeFailureSource::File {
        path: path.to_owned(),
    };
    source.diagnostic(&png_file_identity(path))
}

/// GNU 31.1: `Cannot find image file `PATH'`.
#[test]
fn a_file_that_cannot_be_read_names_the_file() {
    let path = fixture("does-not-exist.png");
    assert_eq!(
        diagnostic_for_file(&path).message(),
        format!("Cannot find image file `{path}'")
    );
}

/// GNU 31.1: `Not a PNG file: `PATH'` — the file was found and opened, and its
/// signature was not PNG's. The declared type is what names the sentence, not
/// what the bytes happen to be.
#[test]
fn bytes_that_are_not_the_declared_format_name_the_format_and_file() {
    let path = fixture("notimage.png");
    assert_eq!(
        diagnostic_for_file(&path).message(),
        format!("Not a PNG file: `{path}'")
    );
}

/// The same bytes through `:data` get GNU's other noun, and the specification
/// instead of a path.
#[test]
fn the_same_bytes_as_data_are_named_by_their_specification() {
    let bytes = std::fs::read(fixture("notimage.png")).expect("fixture");
    let spec = "(image :type png :data definitely not an image :scale default)";
    let source = DecodeFailureSource::Bytes {
        data: neomacs_display_protocol::image::EncodedBytes::new(bytes),
    };
    assert_eq!(
        source.diagnostic(&png_data_identity(spec)).message(),
        format!("Not a PNG image: `{spec}'")
    );
}

/// GNU 31.1, a truncated PNG: the signature matched, so this is the loader's
/// own failure, and libpng's words for a stream that ends early are
/// `Read error`.
#[test]
fn a_truncated_png_reports_the_loaders_own_failure() {
    let path = fixture("truncated.png");
    assert_eq!(
        diagnostic_for_file(&path).message(),
        "PNG error: Read error"
    );
}

/// A source that arrived with no specification has no GNU sentence that is
/// true of it, and must not be dressed in one.
#[test]
fn a_specification_free_source_is_not_given_gnus_words() {
    let source = DecodeFailureSource::Bytes {
        data: neomacs_display_protocol::image::EncodedBytes::new(vec![0xde, 0xad]),
    };
    assert_eq!(
        source.diagnostic(&ImageLoadIdentity::unspecified()),
        ImageDiagnostic::NotDrawable
    );
}
