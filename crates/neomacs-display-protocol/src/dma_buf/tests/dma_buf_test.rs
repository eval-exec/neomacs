use super::*;

#[test]
fn import_support_requires_the_exact_format_and_modifier() {
    let linear = DmaBufFormat {
        fourcc: 0x34325258,
        modifier: 0,
    };
    let tiled = DmaBufFormat {
        modifier: 0x0200000028a6bf04,
        ..linear
    };
    let formats = DmaBufImportFormats::new([linear, linear]);
    assert_eq!(formats, DmaBufImportFormats::new([linear]));
    assert!(formats.contains(linear));
    assert!(!formats.contains(tiled));
    assert!(!formats.contains(DmaBufFormat {
        fourcc: 0x34325241,
        ..linear
    }));
    assert!(!DmaBufImportFormats::default().contains(linear));
}
