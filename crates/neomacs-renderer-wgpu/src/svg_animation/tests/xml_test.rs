use super::{Duration, compile, eval, patch};

fn animated_attribute(source: &str) -> String {
    let animation = compile(source);
    let overrides = eval::evaluate(&animation, Duration::ZERO);
    let patched = patch::apply(&animation, source.as_bytes(), &overrides).expect("patch applies");
    let text = String::from_utf8(patched).expect("UTF-8 SVG");
    let document = resvg::usvg::roxmltree::Document::parse(&text)
        .unwrap_or_else(|error| panic!("patched SVG must be valid XML: {error}\n{text}"));
    document
        .descendants()
        .find(|node| node.has_tag_name("rect"))
        .expect("target rect")
        .attribute("data-label")
        .expect("animated attribute")
        .to_owned()
}

#[test]
fn replacement_preserves_apostrophe_in_single_quoted_attribute() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect data-label='base'><set attributeName="data-label" to="it's animated" dur="1s"/></rect></svg>"#;
    assert_eq!(animated_attribute(source), "it's animated");
}

#[test]
fn insertion_preserves_decoded_quotes_ampersands_and_angle_brackets() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect><set attributeName="data-label" to="say &quot;hello&quot; &amp; &lt;world&gt; &amp;amp;" dur="1s"/></rect></svg>"#;
    assert_eq!(animated_attribute(source), "say \"hello\" & <world> &amp;");
}

#[test]
fn replacement_preserves_character_reference_whitespace() {
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect data-label="base"><set attributeName="data-label" to="a&#x9;b&#xA;c&#xD;d" dur="1s"/></rect></svg>"#;
    assert_eq!(animated_attribute(source), "a\tb\nc\rd");
}
