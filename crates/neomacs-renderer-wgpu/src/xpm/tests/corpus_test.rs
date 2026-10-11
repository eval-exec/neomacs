//! Cross-check the XPM decoder against an independent implementation over the
//! XPM assets the tree ships.
//!
//! GNU is the specification for this decoder, so agreement with a third-party
//! decoder is not a parity gate: `image-extras` implements libXpm-style XPM3,
//! and where it differs from GNU the difference is adjudicated against
//! `xpm_load_image` and recorded here rather than silently tolerated. What the
//! cross-check buys is a second reading of every shipped asset -- decode
//! failures and gross pixel differences that the GNU-shaped unit tests cannot
//! see, because those only exercise the cases their fixtures name.

use super::*;

/// A fallback no shipped asset uses: an unresolved key would then surface as a
/// disagreement with the independent decoder instead of hiding inside a
/// legitimate white pixel.
const FALLBACK: [u8; 3] = [0xff, 0x00, 0xff];

/// Assets the independent decoder refuses and GNU does not. Recording them
/// explicitly keeps the list from growing unnoticed.
///
/// `letter.xpm` names its foreground `opaque`. That name is in no X11 database,
/// and both of GNU's loaders still paint it with the frame foreground: the
/// libXpm path maps `opaque` to `FRAME_FOREGROUND_PIXEL` by name
/// (src/image.c:5634-5641), and the hand-written path -- the one cairo, NS and
/// pgtk builds compile (src/image.c:5811 gates the libXpm one) -- leaves the
/// key out of the color table and paints those pixels with the same pixel
/// (src/image.c:6512-6538). This decoder takes the hand-written route, so it
/// decodes the asset; `image-extras` rejects the whole image instead.
///
/// `separator.xpm` carries one pixel row three characters wide where its header
/// says two (`".+ "`). GNU's only length test is `len < width * chars_per_pixel`
/// (src/image.c:6530-6531), so it reads the two characters it asked for and
/// ignores the rest -- which is what this decoder does; `image-extras` rejects
/// the row instead.
const ASSETS_THE_INDEPENDENT_DECODER_REFUSES: [&str; 2] = ["letter.xpm", "separator.xpm"];

/// Every `etc/images/*.xpm` asset -- the corpus GNU ships for its own toolbars,
/// which the other tests in this module already pin by name.
fn shipped_xpm_assets() -> Vec<std::path::PathBuf> {
    let dir = neomacs_infra::workspace_root().join("etc/images");
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "xpm"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no XPM assets in {}", dir.display());
    paths
}

/// Compare one asset pixel for pixel with the independent decode.
///
/// Two differences are expected and bounded:
///
/// - a masked pixel's RGB is unspecified in both decoders, so only the alpha
///   is a claim there (GNU's rule is that `None` is transparency);
/// - the two reduce GNU's 16-bit channels to 8 bits differently and both are
///   deliberate: GNU truncates (`lookup_rgb_color`, src/image.c:6884-6892),
///   `image-extras` rounds. `cancel.xpm`'s `0x01c6` channel is the measured
///   example -- this decoder answers 1, the independent one 2 -- so channels
///   are allowed to differ by one step, which still catches a mis-resolved
///   color (that is off by far more than 1).
fn compare_with_independent_decode(
    path: &std::path::Path,
    ours: (u32, u32, &[u8]),
    theirs: &image::RgbaImage,
) {
    let (width, height, ours) = ours;
    assert_eq!(
        (width, height),
        (theirs.width(), theirs.height()),
        "{}: dimensions",
        path.display()
    );
    for (index, (mine, theirs)) in ours.chunks_exact(4).zip(theirs.pixels()).enumerate() {
        let mine = [mine[0], mine[1], mine[2], mine[3]];
        assert_eq!(
            mine[3],
            theirs.0[3],
            "{}: pixel {index}: alpha",
            path.display()
        );
        if mine[3] == 0 {
            continue;
        }
        for channel in 0..3 {
            assert!(
                mine[channel].abs_diff(theirs.0[channel]) <= 1,
                "{}: pixel {index}: {mine:?} vs {:?}",
                path.display(),
                theirs.0
            );
        }
    }
}

#[test]
fn shipped_xpm_corpus_agrees_with_an_independent_decoder() {
    image_extras::register();
    for path in shipped_xpm_assets() {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let data = std::fs::read(&path).expect("the asset is readable");
        let (width, height, rgba) = decode_xpm_data(&data, FALLBACK)
            .unwrap_or_else(|| panic!("{}: our decoder refused a shipped asset", path.display()));
        match image::open(&path) {
            Ok(independent) => {
                assert!(
                    !ASSETS_THE_INDEPENDENT_DECODER_REFUSES.contains(&file_name),
                    "{file_name}: listed as refused by the independent decoder, but it decoded"
                );
                compare_with_independent_decode(
                    &path,
                    (width, height, &rgba),
                    &independent.to_rgba8(),
                );
            }
            Err(error) => assert!(
                ASSETS_THE_INDEPENDENT_DECODER_REFUSES.contains(&file_name),
                "{}: the independent decoder refused it ({error}); adjudicate against \
                 xpm_load_image and record the outcome",
                path.display()
            ),
        }
    }
}
