use crate::flamegraph::folded_to_svg;

#[test]
fn renders_folded_to_svg() {
    let folded = "main;foo;bar 10\nmain;foo;baz 5\nmain;qux 3";
    let svg = folded_to_svg(folded, "lisp-cpu").unwrap();
    assert!(
        svg.contains("<svg"),
        "output is not svg: {}",
        &svg[..svg.len().min(120)]
    );
    // Frame names from the folded input should appear in the rendered flamegraph.
    assert!(svg.contains("foo"), "frame name 'foo' missing from svg");
    assert!(svg.contains("bar"), "frame name 'bar' missing from svg");
}

#[test]
fn empty_folded_yields_placeholder_svg() {
    let svg = folded_to_svg("   \n  ", "lisp-cpu").unwrap();
    assert!(svg.contains("<svg"));
    assert!(
        svg.contains("no samples"),
        "placeholder text missing: {svg}"
    );
}
