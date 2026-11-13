use super::*;
use neovm_value::error::FlowKind;

#[test]
fn libxml_available_p_returns_true_and_validates_arity() {
    crate::test_utils::init_test_tracing();
    assert_eq!(builtin_libxml_available_p(vec![]).unwrap(), Value::T);

    let libxml_arity = builtin_libxml_available_p(vec![Value::fixnum(1)]).unwrap_err();
    match libxml_arity.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("libxml-available-p"), Value::fixnum(1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn libxml_parse_xml_region_matches_gnu_tree_shape() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        parse_xml_region(br#"<root><empty/></root>"#, false).unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::NIL,
            Value::list(vec![Value::symbol("empty"), Value::NIL]),
        ])
    );

    assert_eq!(
        parse_xml_region(
            br#"<root>
  <a attr="x&amp;y">text</a>
</root>"#,
            false
        )
        .unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("a"),
                Value::list(vec![Value::cons(
                    Value::symbol("attr"),
                    Value::string("x&y")
                )]),
                Value::string("text"),
            ]),
        ])
    );
}

#[test]
fn libxml_parse_xml_region_discards_only_toplevel_comments() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        parse_xml_region(br#"<!--top--><root><!--inner--><a/></root>"#, true).unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("comment"),
                Value::NIL,
                Value::string("inner")
            ]),
            Value::list(vec![Value::symbol("a"), Value::NIL]),
        ])
    );
}

#[test]
fn libxml_parse_xml_region_resolves_declared_namespace_prefixes() {
    crate::test_utils::init_test_tracing();

    // GNU/libxml2 strips a *declared* prefix from element and attribute names,
    // drops the `xmlns:*` declaration itself, but keeps an *undeclared* prefix
    // (`b:`) verbatim. Oracle:
    //   (root nil (item ((b:x . "1")) "ns"))
    assert_eq!(
        parse_xml_region(
            br#"<a:root xmlns:a="urn:a"><a:item b:x="1">ns</a:item></a:root>"#,
            false
        )
        .unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("item"),
                Value::list(vec![Value::cons(Value::symbol("b:x"), Value::string("1"))]),
                Value::string("ns"),
            ]),
        ])
    );

    // When both `a` and `b` are declared, both prefixes are stripped. Oracle:
    //   (root nil (item ((x . "1")) "ns"))
    assert_eq!(
        parse_xml_region(
            br#"<a:root xmlns:a="urn:a" xmlns:b="urn:b"><a:item b:x="1">ns</a:item></a:root>"#,
            false
        )
        .unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("item"),
                Value::list(vec![Value::cons(Value::symbol("x"), Value::string("1"))]),
                Value::string("ns"),
            ]),
        ])
    );
}

#[test]
fn libxml_parse_xml_region_strips_builtin_xml_prefix_and_default_ns() {
    crate::test_utils::init_test_tracing();

    // The reserved `xml` prefix is always declared, so `xml:lang` -> `lang`.
    // Oracle: (root ((lang . "en")) (item nil "x"))
    assert_eq!(
        parse_xml_region(br#"<root xml:lang="en"><item>x</item></root>"#, false).unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::list(vec![Value::cons(
                Value::symbol("lang"),
                Value::string("en")
            )]),
            Value::list(vec![Value::symbol("item"), Value::NIL, Value::string("x")]),
        ])
    );

    // A default-namespace declaration (`xmlns`) is dropped from the attribute
    // list but does not strip any prefix; unprefixed attributes are unchanged.
    // Oracle: (root ((attr . "v")) (item ((k . "w")) "x"))
    assert_eq!(
        parse_xml_region(
            br#"<root xmlns="urn:d" attr="v"><item k="w">x</item></root>"#,
            false
        )
        .unwrap(),
        Value::list(vec![
            Value::symbol("root"),
            Value::list(vec![Value::cons(Value::symbol("attr"), Value::string("v"))]),
            Value::list(vec![
                Value::symbol("item"),
                Value::list(vec![Value::cons(Value::symbol("k"), Value::string("w"))]),
                Value::string("x"),
            ]),
        ])
    );
}

#[test]
fn libxml_parse_html_region_strips_self_closing_void_tag_slash() {
    crate::test_utils::init_test_tracing();

    // The `tl` crate leaves the trailing `/` in `<hr/>`'s tag name; libxml2
    // never does. With the implicit html/body wrapper this matches GNU exactly.
    // Oracle: (html nil (body nil (div nil (hr nil))))
    let parsed = parse_html_region(br#"<div><hr/></div>"#, false).unwrap();
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("body"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("div"),
                Value::NIL,
                Value::list(vec![Value::symbol("hr"), Value::NIL]),
            ]),
        ]),
    ]);
    assert_eq!(parsed, expected);
}

#[test]
fn libxml_parse_html_region_decodes_character_references_like_gnu_libxml2() {
    crate::test_utils::init_test_tracing();

    let parsed = parse_html_region(
        br#"<p title="A &amp; B&nbsp;&#x41;&#65; &bogus; &amp x &copy x &amp=x">T &amp; &nbsp; &#x41; &#65; &copy; &bogus; &amp x &copy x &amp=x</p>"#,
        false,
    )
    .unwrap();
    let paragraph = Value::list(vec![
        Value::symbol("p"),
        Value::list(vec![Value::cons(
            Value::symbol("title"),
            Value::string("A & B\u{a0}AA &bogus; & x © x &amp=x"),
        )]),
        Value::string("T & \u{a0} A A © &bogus; & x © x &=x"),
    ]);

    assert_eq!(parsed, html(vec![body(vec![paragraph])]));
}

/// Build `(html nil CHILDREN...)`.
fn html(children: Vec<Value>) -> Value {
    let mut v = vec![Value::symbol("html"), Value::NIL];
    v.extend(children);
    Value::list(v)
}

/// Build `(body nil CHILDREN...)`.
fn body(children: Vec<Value>) -> Value {
    let mut v = vec![Value::symbol("body"), Value::NIL];
    v.extend(children);
    Value::list(v)
}

/// Build `(comment nil TEXT)`.
fn comment(text: &str) -> Value {
    Value::list(vec![
        Value::symbol("comment"),
        Value::NIL,
        Value::string(text),
    ])
}

#[test]
fn libxml_parse_html_region_wraps_fragment_in_html_body() {
    crate::test_utils::init_test_tracing();

    // Bare fragment gets the implicit html/body wrapper.
    // Oracle: (html nil (body nil (p nil "hi")))
    assert_eq!(
        parse_html_region(br#"<p>hi</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("hi"),
        ])])])
    );

    // Bare text. Oracle: (html nil (body nil "hello"))
    assert_eq!(
        parse_html_region(br#"hello"#, false).unwrap(),
        html(vec![body(vec![Value::string("hello")])])
    );

    // Multiple body nodes. Oracle: (html nil (body nil (p nil "a") (p nil "b")))
    assert_eq!(
        parse_html_region(br#"<p>a</p><p>b</p>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("a")]),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("b")]),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_routes_head_elements_and_transitions_to_body() {
    crate::test_utils::init_test_tracing();

    // A leading <title> is routed into <head>, the <p> into <body>.
    // Oracle: (html nil (head nil (title nil "T")) (body nil (p nil "b")))
    let head_title = Value::list(vec![
        Value::symbol("head"),
        Value::NIL,
        Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
    ]);
    assert_eq!(
        parse_html_region(br#"<title>T</title><p>b</p>"#, false).unwrap(),
        Value::list(vec![
            Value::symbol("html"),
            Value::NIL,
            head_title,
            body(vec![Value::list(vec![
                Value::symbol("p"),
                Value::NIL,
                Value::string("b"),
            ])]),
        ])
    );

    // An explicit <head> is unwrapped, not double-nested.
    // Oracle (same shape): (html nil (head nil (title nil "T")) (body nil (p nil "b")))
    assert_eq!(
        parse_html_region(br#"<head><title>T</title></head><p>b</p>"#, false).unwrap(),
        Value::list(vec![
            Value::symbol("html"),
            Value::NIL,
            head_title,
            body(vec![Value::list(vec![
                Value::symbol("p"),
                Value::NIL,
                Value::string("b"),
            ])]),
        ])
    );

    // Once body content (text) appears, a later <title> stays in the body.
    // Oracle: (html nil (body nil "hi" (title nil "T")))
    assert_eq!(
        parse_html_region(br#"hi<title>T</title>"#, false).unwrap(),
        html(vec![body(vec![
            Value::string("hi"),
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T"),]),
        ])])
    );

    // Only head content: libxml2 emits no <body>.
    // Oracle: (html nil (head nil (title nil "T")))
    assert_eq!(
        parse_html_region(br#"<title>T</title>"#, false).unwrap(),
        Value::list(vec![
            Value::symbol("html"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("head"),
                Value::NIL,
                Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T"),]),
            ]),
        ])
    );
}

#[test]
fn libxml_parse_html_region_handles_xml_prolog_and_doctype() {
    crate::test_utils::init_test_tracing();

    // GNU libxml2 skips XML declaration and <!DOCTYPE ...> in html mode,
    // or records <?xml ...?> as a comment when comments are preserved.
    // In `tl`, an unhandled <?xml ...?> produces a corrupted tag named `##`.
    let html_with_prolog = br#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html><body><p>Hello</p></body></html>"#;

    // Discard comments: prolog and DOCTYPE are stripped, clean html tree returned.
    let parsed_discard = parse_html_region(html_with_prolog, true).unwrap();
    assert_eq!(
        parsed_discard,
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("Hello"),
        ])])])
    );

    // Keep comments: prolog is preserved as a comment node.
    let parsed_keep = parse_html_region(html_with_prolog, false).unwrap();
    let expected_keep = Value::list(vec![
        Value::symbol("top"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("comment"),
            Value::NIL,
            Value::string(r#"?xml version="1.0" encoding="utf-8"?"#),
        ]),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("Hello"),
        ])])]),
    ]);
    assert_eq!(parsed_keep, expected_keep);

    // Case insensitivity: lowercase doctype is also stripped
    let html_lowercase = br#"<!doctype html><html><body><p>World</p></body></html>"#;
    let parsed_lower = parse_html_region(html_lowercase, true).unwrap();
    assert_eq!(
        parsed_lower,
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("World"),
        ])])])
    );
}

#[test]
fn libxml_parse_html_region_preserves_raw_content_in_style_and_script() {
    crate::test_utils::init_test_tracing();

    let doc = br#"<html><head><style>body > div { color: red; } a < b a&amp;b</style><script data-kind="a>b">var x = 1; if (a < b) { window.alert("&amp;"); }</script></head><body><p>Text</p></body></html>"#;
    let parsed = parse_html_region(doc, false).unwrap();

    let expected = html(vec![
        Value::list(vec![
            Value::symbol("head"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("style"),
                Value::NIL,
                Value::string("body > div { color: red; } a < b a&amp;b"),
            ]),
            Value::list(vec![
                Value::symbol("script"),
                Value::list(vec![Value::cons(
                    Value::symbol("data-kind"),
                    Value::string("a>b"),
                )]),
                Value::string("var x = 1; if (a < b) { window.alert(\"&amp;\"); }"),
            ]),
        ]),
        body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("Text"),
        ])]),
    ]);
    assert_eq!(parsed, expected);
}

#[test]
fn libxml_parse_html_region_protects_raw_text_from_markup_like_content() {
    crate::test_utils::init_test_tracing();

    // libxml2 treats a script body as raw text: `</body>` in a string must not
    // close the document's body. GNU 31.1 oracle:
    // (html nil (body nil (script nil "document.write(\"</body></html>\");")
    //                   (p nil "x")))
    let doc =
        br#"<html><body><script>document.write("</body></html>");</script><p>x</p></body></html>"#;
    let expected = html(vec![body(vec![
        Value::list(vec![
            Value::symbol("script"),
            Value::NIL,
            Value::string("document.write(\"</body></html>\");"),
        ]),
        Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
    ])]);
    assert_eq!(parse_html_region(doc, false).unwrap(), expected);
    assert_eq!(parse_html_region(doc, true).unwrap(), expected);
}

#[test]
fn libxml_parse_html_region_handles_apostrophes_in_unquoted_attributes() {
    crate::test_utils::init_test_tracing();

    // An apostrophe in an unquoted value is not a quote opener.
    // GNU 31.1 oracle: (script ((data-x . "don't")) "hello")
    let doc = br#"<html><body><script data-x=don't>hello</script></body></html>"#;
    let expected = html(vec![body(vec![Value::list(vec![
        Value::symbol("script"),
        Value::list(vec![Value::cons(
            Value::symbol("data-x"),
            Value::string("don't"),
        )]),
        Value::string("hello"),
    ])])]);
    assert_eq!(parse_html_region(doc, false).unwrap(), expected);
}

#[test]
fn libxml_parse_html_region_strips_comment_delimiters() {
    crate::test_utils::init_test_tracing();

    let doc = br#"<div><!-- simple comment --></div>"#;

    // GNU's `discard-comments' only drops document-level comments; this one is
    // nested inside <div>, so it remains in both modes. GNU 31.1 oracle:
    // (html nil (body nil (div nil (comment nil " simple comment "))))
    let expected = html(vec![body(vec![Value::list(vec![
        Value::symbol("div"),
        Value::NIL,
        comment(" simple comment "),
    ])])]);
    assert_eq!(parse_html_region(doc, true).unwrap(), expected);
    assert_eq!(parse_html_region(doc, false).unwrap(), expected);
}

#[test]
fn libxml_parse_html_region_handles_processing_instructions_like_libxml2() {
    crate::test_utils::init_test_tracing();

    let xsl =
        br#"<?xml-stylesheet type="text/css" href="x.css"?><html><body><p>Hello</p></body></html>"#;
    let document = html(vec![body(vec![Value::list(vec![
        Value::symbol("p"),
        Value::NIL,
        Value::string("Hello"),
    ])])]);
    assert_eq!(
        parse_html_region(xsl, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(r#"?xml-stylesheet type="text/css" href="x.css"?"#),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(xsl, true).unwrap(), document);

    // Every leading PI is a comment, and a PI need not end in `?>`.
    let multi = br#"<?xml version="1.0"?><?foo bar?><html><body>x</body></html>"#;
    let document = html(vec![body(vec![Value::string("x")])]);
    assert_eq!(
        parse_html_region(multi, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(r#"?xml version="1.0"?"#),
            comment("?foo bar?"),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(multi, true).unwrap(), document);

    let no_qmark = br#"<?foo><body>x</body>"#;
    let document = html(vec![body(vec![Value::string("x")])]);
    assert_eq!(
        parse_html_region(no_qmark, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment("?foo"),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(no_qmark, true).unwrap(), document);

    // libxml2 stops a PI at the first `>`, even inside a quoted attribute.
    let embedded = br#"<?foo href="a>b"?><p>x</p>"#;
    let document = html(vec![body(vec![
        Value::string("b\"?>"),
        Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
    ])]);
    assert_eq!(
        parse_html_region(embedded, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(r#"?foo href="a"#),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(embedded, true).unwrap(), document);

    // Unterminated prolog tokens leave no root element.
    assert_eq!(parse_html_region(br#"<?foo"#, false).unwrap(), Value::NIL);
}

#[test]
fn libxml_parse_html_region_places_non_leading_pis_as_comments() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (body nil "x" (comment nil "?foo?") "y"))
    let expected = html(vec![body(vec![
        Value::string("x"),
        comment("?foo?"),
        Value::string("y"),
    ])]);
    assert_eq!(parse_html_region(br#"x<?foo?>y"#, false).unwrap(), expected);
    assert_eq!(parse_html_region(br#"x<?foo?>y"#, true).unwrap(), expected);

    // A PI after content ends at the first `>`, or at end of input.
    // GNU 31.1: (html nil (body nil "x" (comment nil "?foo")))
    let expected = html(vec![body(vec![Value::string("x"), comment("?foo")])]);
    assert_eq!(parse_html_region(br#"x<?foo"#, false).unwrap(), expected);

    // GNU 31.1: (html nil (body nil (p nil "x") (comment nil "?foo?") (p nil "y")))
    assert_eq!(
        parse_html_region(br#"<p>x</p><?foo?><p>y</p>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            comment("?foo?"),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("y")]),
        ])])
    );

    // A PI after an explicit body is a sibling of body.
    // GNU 31.1: (html nil (body nil "x") (comment nil "?foo?"))
    assert_eq!(
        parse_html_region(br#"<body>x</body><?foo?>"#, false).unwrap(),
        html(vec![body(vec![Value::string("x")]), comment("?foo?")])
    );
}

#[test]
fn libxml_parse_html_region_does_not_rewrite_source_placeholder_comments() {
    crate::test_utils::init_test_tracing();

    // A real comment matching the internal PI marker's shape must keep its
    // text; only the generated PI comment is restored.
    // GNU 31.1: (html nil (body nil (p nil "x")
    //                   (comment nil "neomacs-internal-pi-0")
    //                   (comment nil "?foo?")))
    assert_eq!(
        parse_html_region(br#"<p>x</p><!--neomacs-internal-pi-0--><?foo?>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            comment("neomacs-internal-pi-0"),
            comment("?foo?"),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_preserves_non_ascii_whitespace_before_prolog() {
    crate::test_utils::init_test_tracing();

    // U+00A0 is content, not HTML whitespace. The PI after it is a body
    // comment, not a document prolog.
    // GNU 31.1: (html nil (body nil "\u{a0}" (comment nil "?foo?") (p nil "x")))
    let mut doc = "\u{00A0}".as_bytes().to_vec();
    doc.extend_from_slice(br#"<?foo?><p>x</p>"#);
    assert_eq!(
        parse_html_region(&doc, false).unwrap(),
        html(vec![body(vec![
            Value::string("\u{00A0}"),
            comment("?foo?"),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
        ])])
    );

    // A DOCTYPE after content is dropped. GNU 31.1:
    // (html nil (body nil "\u{a0}" (p nil "x")))
    let mut doc = "\u{00A0}".as_bytes().to_vec();
    doc.extend_from_slice(br#"<!DOCTYPE html><p>x</p>"#);
    assert_eq!(
        parse_html_region(&doc, false).unwrap(),
        html(vec![body(vec![
            Value::string("\u{00A0}"),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
        ])])
    );

    // GNU 31.1: (html nil (body nil "x" (p nil "y")))
    assert_eq!(
        parse_html_region(br#"x<!DOCTYPE html><p>y</p>"#, false).unwrap(),
        html(vec![body(vec![
            Value::string("x"),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("y")]),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_preserves_doctype_parse_boundaries() {
    crate::test_utils::init_test_tracing();

    // Removing the DOCTYPE must not join `<` with `b>` into a tag.
    // GNU 31.1: (html nil (body nil (p nil "<b>text")))
    assert_eq!(
        parse_html_region(br#"<p><<!DOCTYPE html>b>text</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("<b>text"),
        ])])])
    );

    // Nor may it join `&am` with `p;` into an entity.
    // GNU 31.1: (html nil (body nil (p nil "&amp;")))
    assert_eq!(
        parse_html_region(br#"<p>&am<!DOCTYPE html>p;</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("&amp;"),
        ])])])
    );

    // Plain text on both sides still coalesces.
    // GNU 31.1: (html nil (body nil (p nil "ab")))
    assert_eq!(
        parse_html_region(br#"<p>a<!DOCTYPE html>b</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("ab"),
        ])])])
    );

    // A tag after the dropped DOCTYPE is still parsed as a tag.
    // GNU 31.1: (html nil (body nil (p nil "a" (b nil "c"))))
    assert_eq!(
        parse_html_region(br#"<p>a<!DOCTYPE html><b>c</b></p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("a"),
            Value::list(vec![Value::symbol("b"), Value::NIL, Value::string("c")]),
        ])])])
    );
}

#[test]
fn libxml_parse_html_region_coalesces_adjacent_text_like_libxml2() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (body nil "hellox"))
    assert_eq!(
        parse_html_region(br#"hello<body>x</body>"#, false).unwrap(),
        html(vec![body(vec![Value::string("hellox")])])
    );
}

#[test]
fn libxml_parse_html_region_preserves_whitespace_like_libxml2() {
    crate::test_utils::init_test_tracing();

    // Inside elements whitespace is content.
    // GNU 31.1: (html nil (body nil (div nil " ")))
    assert_eq!(
        parse_html_region(br#"<div> </div>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("div"),
            Value::NIL,
            Value::string(" "),
        ])])])
    );

    // Whitespace between top-level elements is content.
    // GNU 31.1: (html nil (body nil (div nil "a") " " (div nil "b")))
    assert_eq!(
        parse_html_region(br#"<div>a</div> <div>b</div>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("div"), Value::NIL, Value::string("a"),]),
            Value::string(" "),
            Value::list(vec![Value::symbol("div"), Value::NIL, Value::string("b"),]),
        ])])
    );

    // Whitespace between inline elements is content.
    // GNU 31.1: (html nil (body nil (p nil (b nil "a") " " (i nil "b"))))
    assert_eq!(
        parse_html_region(br#"<p><b>a</b> <i>b</i></p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::list(vec![Value::symbol("b"), Value::NIL, Value::string("a"),]),
            Value::string(" "),
            Value::list(vec![Value::symbol("i"), Value::NIL, Value::string("b"),]),
        ])])])
    );

    // Document-level leading and trailing blanks are dropped.
    // GNU 31.1: (html nil (body nil (p nil "x")))
    let expected = html(vec![body(vec![Value::list(vec![
        Value::symbol("p"),
        Value::NIL,
        Value::string("x"),
    ])])]);
    assert_eq!(
        parse_html_region(br#"   <p>x</p>"#, false).unwrap(),
        expected
    );
    assert_eq!(
        parse_html_region(br#"<p>x</p>   "#, false).unwrap(),
        expected
    );
    assert_eq!(parse_html_region(br#"   "#, false).unwrap(), Value::NIL);

    // Whitespace around dropped DOCTYPEs stays one text run.
    // GNU 31.1: (html nil (body nil (p nil "a b")))
    assert_eq!(
        parse_html_region(br#"<p>a<!DOCTYPE html> <!DOCTYPE html>b</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("a b"),
        ])])])
    );

    // Newlines inside body are content.
    // GNU 31.1: (html nil (body nil "\n" (p nil "x") "\n"))
    assert_eq!(
        parse_html_region(b"<html><body>\n<p>x</p>\n</body></html>", false).unwrap(),
        html(vec![body(vec![
            Value::string("\n"),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            Value::string("\n"),
        ])])
    );

    // Blanks between an explicit root and trailing comments are dropped.
    // GNU 31.1: (top nil (html nil (body nil "x")) (comment nil "c"))
    let root = html(vec![body(vec![Value::string("x")])]);
    assert_eq!(
        parse_html_region(b"<html><body>x</body></html>\n<!--c-->", false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            root.clone(),
            comment("c"),
        ])
    );
    assert_eq!(
        parse_html_region(b"<html><body>x</body></html>\n<!--c-->", true).unwrap(),
        root
    );
    // GNU 31.1: (top nil (comment nil "c") (html nil (body nil "x")))
    assert_eq!(
        parse_html_region(b"<!--c-->\n<html><body>x</body></html>", false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment("c"),
            root.clone(),
        ])
    );

    // Bare fragments keep blanks before comments; only the document tail is
    // trimmed. GNU 31.1:
    // (html nil (body nil (p nil "x") " " (comment nil "c")))
    assert_eq!(
        parse_html_region(b"<p>x</p> <!--c--> ", false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            Value::string(" "),
            comment("c"),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_routes_head_tags_after_body_to_body() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (body nil (p nil "x") (title nil "T")))
    assert_eq!(
        parse_html_region(br#"<p>x</p><head><title>T</title></head>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
        ])])
    );

    // GNU 31.1: (html nil (head nil (title nil "T"))
    //                   (body nil (p nil "x") (meta nil)))
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("head"),
            Value::NIL,
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
        ]),
        body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            Value::list(vec![Value::symbol("meta"), Value::NIL]),
        ]),
    ]);
    assert_eq!(
        parse_html_region(
            br#"<head><title>T</title></head><p>x</p><head><meta></head>"#,
            false
        )
        .unwrap(),
        expected
    );
}

#[test]
fn libxml_parse_html_region_preserves_literal_less_than() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (body nil (p nil "a < b")))
    assert_eq!(
        parse_html_region(br#"<p>a < b</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("a < b"),
        ])])])
    );

    // GNU 31.1: (html nil (body nil "a < b"))
    assert_eq!(
        parse_html_region(br#"a < b"#, false).unwrap(),
        html(vec![body(vec![Value::string("a < b")])])
    );

    // GNU 31.1: (html nil (body nil (p nil "<")))
    assert_eq!(
        parse_html_region(br#"<p><</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("<"),
        ])])])
    );

    // GNU 31.1: (html nil (body nil (p nil "1 <2")))
    assert_eq!(
        parse_html_region(br#"<p>1 <2</p>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("1 <2"),
        ])])])
    );
}

#[test]
fn libxml_parse_html_region_drops_whitespace_in_head_context() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (head nil (title nil "T") (meta nil)))
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("head"),
            Value::NIL,
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
            Value::list(vec![Value::symbol("meta"), Value::NIL]),
        ]),
    ]);
    assert_eq!(
        parse_html_region(b"<title>T</title>\n<meta>", false).unwrap(),
        expected
    );
    assert_eq!(
        parse_html_region(b"<title>T</title> <meta>", false).unwrap(),
        expected
    );

    // GNU 31.1: (html nil (head nil (meta nil)))
    assert_eq!(
        parse_html_region(b"<head>\n<meta></head>", false).unwrap(),
        Value::list(vec![
            Value::symbol("html"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("head"),
                Value::NIL,
                Value::list(vec![Value::symbol("meta"), Value::NIL]),
            ]),
        ])
    );

    // GNU 31.1: (html nil (head nil (title nil "T")) (body nil "x"))
    assert_eq!(
        parse_html_region(
            b"<html>\n<head><title>T</title></head>\n<body>x</body>\n</html>",
            false
        )
        .unwrap(),
        Value::list(vec![
            Value::symbol("html"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("head"),
                Value::NIL,
                Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
            ]),
            body(vec![Value::string("x")]),
        ])
    );

    // GNU 31.1: (html nil (body nil "x"))
    assert_eq!(
        parse_html_region(b"<html> <body>x</body> </html>", false).unwrap(),
        html(vec![body(vec![Value::string("x")])])
    );
}

#[test]
fn libxml_parse_html_region_routes_html_children_into_body() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (body nil (b nil "a") " " (i nil "b")))
    assert_eq!(
        parse_html_region(br#"<html><b>a</b> <i>b</i></html>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("b"), Value::NIL, Value::string("a")]),
            Value::string(" "),
            Value::list(vec![Value::symbol("i"), Value::NIL, Value::string("b")]),
        ])])
    );

    // GNU 31.1: (html nil (body nil (p nil "x")))
    assert_eq!(
        parse_html_region(br#"<html><p>x</p></html>"#, false).unwrap(),
        html(vec![body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("x"),
        ])])])
    );

    // GNU 31.1: (html nil)
    assert_eq!(
        parse_html_region(br#"<html></html>"#, false).unwrap(),
        Value::list(vec![Value::symbol("html"), Value::NIL])
    );

    // GNU 31.1: (html nil (body nil (b nil "a") " "))
    assert_eq!(
        parse_html_region(br#"<html> <b>a</b> </html>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("b"), Value::NIL, Value::string("a")]),
            Value::string(" "),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_places_comments_before_head_under_html() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (comment nil "c") (head nil (title nil "T")))
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        comment("c"),
        Value::list(vec![
            Value::symbol("head"),
            Value::NIL,
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
        ]),
    ]);
    assert_eq!(
        parse_html_region(b"<html><!--c--><head><title>T</title></head></html>", false).unwrap(),
        expected
    );
    assert_eq!(
        parse_html_region(b"<html><!--c--><head><title>T</title></head></html>", true).unwrap(),
        expected
    );

    // GNU 31.1: (html nil (comment nil "c"))
    assert_eq!(
        parse_html_region(b"<html><!--c--></html>", false).unwrap(),
        Value::list(vec![Value::symbol("html"), Value::NIL, comment("c"),])
    );

    // GNU 31.1: (html nil (comment nil "c") (body nil (p nil "x")))
    assert_eq!(
        parse_html_region(b"<html><!--c--><p>x</p></html>", false).unwrap(),
        html(vec![
            comment("c"),
            body(vec![Value::list(vec![
                Value::symbol("p"),
                Value::NIL,
                Value::string("x"),
            ])]),
        ])
    );

    // A comment after implicit head content stays in head.
    // GNU 31.1: (html nil (head nil (title nil "T") (comment nil "c")))
    assert_eq!(
        parse_html_region(b"<title>T</title><!--c-->", false).unwrap(),
        Value::list(vec![
            Value::symbol("html"),
            Value::NIL,
            Value::list(vec![
                Value::symbol("head"),
                Value::NIL,
                Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
                comment("c"),
            ]),
        ])
    );

    // A document-level comment before an explicit head is toplevel when kept
    // and dropped with discard-comments.
    // GNU 31.1 keep: (top nil (comment nil "c") (html nil (head nil (title nil "T"))))
    let head_only = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("head"),
            Value::NIL,
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
        ]),
    ]);
    assert_eq!(
        parse_html_region(b"<!--c--><head><title>T</title></head>", false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment("c"),
            head_only.clone(),
        ])
    );
    assert_eq!(
        parse_html_region(b"<!--c--><head><title>T</title></head>", true).unwrap(),
        head_only
    );
}

#[test]
fn libxml_parse_html_region_strips_a_leading_utf8_bom_like_libxml2() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1 strips one leading BOM when content follows, then handles the
    // prolog normally.
    let mut doc = "\u{FEFF}".as_bytes().to_vec();
    doc.extend_from_slice(br#"<?xml version="1.0"?><html><body>x</body></html>"#);
    let document = html(vec![body(vec![Value::string("x")])]);
    assert_eq!(
        parse_html_region(&doc, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(r#"?xml version="1.0"?"#),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(&doc, true).unwrap(), document);

    // A second BOM is text, not another encoding signature.
    let mut doc = "\u{FEFF}\u{FEFF}".as_bytes().to_vec();
    doc.extend_from_slice(b"<p>x</p>");
    let expected = html(vec![body(vec![
        Value::string("\u{FEFF}"),
        Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
    ])]);
    assert_eq!(parse_html_region(&doc, false).unwrap(), expected);

    // A lone BOM has no following content, so libxml2 parses it as text.
    assert_eq!(
        parse_html_region("\u{FEFF}".as_bytes(), false).unwrap(),
        html(vec![body(vec![Value::string("\u{FEFF}")])])
    );
}

#[test]
fn libxml_parse_html_region_wraps_fragments_around_toplevel_comments() {
    crate::test_utils::init_test_tracing();

    // A prolog comment must not suppress the implicit html/body wrapper.
    let prolog_bare = br#"<?xml version="1.0"?><body>x</body>"#;
    let document = html(vec![body(vec![Value::string("x")])]);
    assert_eq!(
        parse_html_region(prolog_bare, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(r#"?xml version="1.0"?"#),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(prolog_bare, true).unwrap(), document);

    // Leading document-level comment.
    let before = br#"<!-- before --><p>x</p>"#;
    let document = html(vec![body(vec![Value::list(vec![
        Value::symbol("p"),
        Value::NIL,
        Value::string("x"),
    ])])]);
    assert_eq!(
        parse_html_region(before, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(" before "),
            document.clone(),
        ])
    );
    assert_eq!(parse_html_region(before, true).unwrap(), document);

    // A comment after body content stays in <body>, in both modes.
    let after = br#"<p>x</p><!-- after -->"#;
    let expected = html(vec![body(vec![
        Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
        comment(" after "),
    ])]);
    assert_eq!(parse_html_region(after, false).unwrap(), expected);
    assert_eq!(parse_html_region(after, true).unwrap(), expected);

    // PI comment before the fragment, comment after the body content.
    let pi_after = br#"<?xml?><p>x</p><!-- after -->"#;
    let expected_keep = Value::list(vec![
        Value::symbol("top"),
        Value::NIL,
        comment("?xml?"),
        expected.clone(),
    ]);
    assert_eq!(parse_html_region(pi_after, false).unwrap(), expected_keep);
    assert_eq!(parse_html_region(pi_after, true).unwrap(), expected);

    // An explicit html root keeps document-level comments around it.
    let html_root = br#"<!-- a --><html><body>x</body></html><!-- b -->"#;
    let root = html(vec![body(vec![Value::string("x")])]);
    assert_eq!(
        parse_html_region(html_root, false).unwrap(),
        Value::list(vec![
            Value::symbol("top"),
            Value::NIL,
            comment(" a "),
            root.clone(),
            comment(" b "),
        ])
    );
    assert_eq!(parse_html_region(html_root, true).unwrap(), root);

    // Comments without a root element produce nil.
    assert_eq!(
        parse_html_region(br#"<!-- c -->"#, false).unwrap(),
        Value::NIL
    );
    assert_eq!(parse_html_region(br#"<?xml?>"#, false).unwrap(), Value::NIL);
}

#[test]
fn libxml_parse_html_region_places_head_comments_like_libxml2() {
    crate::test_utils::init_test_tracing();

    // Comments after implicit head content stay in <head>.
    let title_comment = br#"<title>T</title><!-- c -->"#;
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("head"),
            Value::NIL,
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
            comment(" c "),
        ]),
    ]);
    assert_eq!(parse_html_region(title_comment, false).unwrap(), expected);
    assert_eq!(parse_html_region(title_comment, true).unwrap(), expected);

    // After an explicit </head>, comments are direct <html> children.
    let closed_head = br#"<head></head><!-- c -->"#;
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![Value::symbol("head"), Value::NIL]),
        comment(" c "),
    ]);
    assert_eq!(parse_html_region(closed_head, false).unwrap(), expected);
    assert_eq!(parse_html_region(closed_head, true).unwrap(), expected);

    let with_body = br#"<head></head><!-- c --><p>x</p>"#;
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![Value::symbol("head"), Value::NIL]),
        comment(" c "),
        body(vec![Value::list(vec![
            Value::symbol("p"),
            Value::NIL,
            Value::string("x"),
        ])]),
    ]);
    assert_eq!(parse_html_region(with_body, false).unwrap(), expected);
    assert_eq!(parse_html_region(with_body, true).unwrap(), expected);
}

#[test]
fn libxml_parse_html_region_keeps_explicit_empty_body_and_after_body_nodes() {
    crate::test_utils::init_test_tracing();

    // GNU 31.1: (html nil (head nil) (body nil))
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![Value::symbol("head"), Value::NIL]),
        body(Vec::new()),
    ]);
    assert_eq!(
        parse_html_region(br#"<head></head><body></body>"#, false).unwrap(),
        expected
    );

    // GNU 31.1: (html nil (body nil))
    assert_eq!(
        parse_html_region(br#"<body></body>"#, false).unwrap(),
        html(vec![body(Vec::new())])
    );

    // Nodes after an explicit body are siblings under html.
    // GNU 31.1: (html nil (body nil "x") (comment nil " after "))
    let expected = html(vec![body(vec![Value::string("x")]), comment(" after ")]);
    assert_eq!(
        parse_html_region(br#"<body>x</body><!-- after -->"#, false).unwrap(),
        expected
    );
    assert_eq!(
        parse_html_region(br#"<body>x</body><!-- after -->"#, true).unwrap(),
        expected
    );

    // GNU 31.1: (html nil (body nil "x") (p nil "y"))
    assert_eq!(
        parse_html_region(br#"<body>x</body><p>y</p>"#, false).unwrap(),
        html(vec![
            body(vec![Value::string("x")]),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("y")]),
        ])
    );

    // A comment inside body stays there; one after </body> moves out.
    // GNU 31.1: (html nil (body nil (comment nil " in ")) (comment nil " out "))
    assert_eq!(
        parse_html_region(br#"<body><!-- in --></body><!-- out -->"#, false).unwrap(),
        html(vec![body(vec![comment(" in ")]), comment(" out ")])
    );

    // GNU 31.1: (html nil (head nil) (body nil) (comment nil " after "))
    let expected = Value::list(vec![
        Value::symbol("html"),
        Value::NIL,
        Value::list(vec![Value::symbol("head"), Value::NIL]),
        body(Vec::new()),
        comment(" after "),
    ]);
    assert_eq!(
        parse_html_region(br#"<head></head><body></body><!-- after -->"#, false).unwrap(),
        expected
    );

    // A second explicit body is a sibling, not merged into the first.
    // GNU 31.1: (html nil (body nil "x") (body nil "y"))
    assert_eq!(
        parse_html_region(br#"<body>x</body><body>y</body>"#, false).unwrap(),
        html(vec![
            body(vec![Value::string("x")]),
            body(vec![Value::string("y")]),
        ])
    );

    // When body content precedes the body tag, the tag merges into the
    // implicit body and after-body mode is not entered.
    // GNU 31.1: (html nil (body nil (p nil "x") "y" (p nil "z")))
    assert_eq!(
        parse_html_region(br#"<p>x</p><body>y</body><p>z</p>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            Value::string("y"),
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("z")]),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_preserves_explicit_head_and_body_attributes() {
    crate::test_utils::init_test_tracing();

    let id =
        |value: &str| Value::list(vec![Value::cons(Value::symbol("id"), Value::string(value))]);

    // GNU 31.1: (html nil (body ((id . "main"))))
    let expected = html(vec![Value::list(vec![Value::symbol("body"), id("main")])]);
    assert_eq!(
        parse_html_region(br#"<body id="main"></body>"#, false).unwrap(),
        expected
    );
    assert_eq!(
        parse_html_region(br#"<body id="main"></body>"#, true).unwrap(),
        expected
    );

    // GNU 31.1: (html nil (head ((id . "h")) (title nil "T")))
    assert_eq!(
        parse_html_region(br#"<head id="h"><title>T</title></head>"#, false).unwrap(),
        html(vec![Value::list(vec![
            Value::symbol("head"),
            id("h"),
            Value::list(vec![Value::symbol("title"), Value::NIL, Value::string("T")]),
        ])])
    );

    // GNU 31.1: (html nil (head ((id . "h"))) (body ((id . "main"))))
    assert_eq!(
        parse_html_region(br#"<head id="h"></head><body id="main"></body>"#, false).unwrap(),
        html(vec![
            Value::list(vec![Value::symbol("head"), id("h")]),
            Value::list(vec![Value::symbol("body"), id("main")]),
        ])
    );

    // GNU 31.1: (html nil (body ((id . "main")) "x") (comment nil " after "))
    assert_eq!(
        parse_html_region(br#"<body id="main">x</body><!-- after -->"#, false).unwrap(),
        html(vec![
            Value::list(vec![Value::symbol("body"), id("main"), Value::string("x"),]),
            comment(" after "),
        ])
    );

    // A body tag after content merges and its attributes are not used.
    // GNU 31.1: (html nil (body nil (p nil "x") "y"))
    assert_eq!(
        parse_html_region(br#"<p>x</p><body id="main">y</body>"#, false).unwrap(),
        html(vec![body(vec![
            Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
            Value::string("y"),
        ])])
    );
}

#[test]
fn libxml_parse_html_region_keeps_nested_comments_when_discarding() {
    crate::test_utils::init_test_tracing();

    // src/xml.c parse_region only removes document-level comments; make_dom
    // still emits comments nested in the root element in both modes.
    let nested = br#"<html><body><!-- inner --><p>x</p></body></html>"#;
    let expected = html(vec![body(vec![
        comment(" inner "),
        Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
    ])]);
    assert_eq!(parse_html_region(nested, false).unwrap(), expected);
    assert_eq!(parse_html_region(nested, true).unwrap(), expected);
}

#[test]
fn libxml_parse_html_region_stops_doctype_at_first_gt_like_libxml2() {
    crate::test_utils::init_test_tracing();

    // libxml2's HTML parser does not honor quotes when terminating a DOCTYPE;
    // it stops at the first `>`, so the rest becomes body text.
    let quoted = br#"<!DOCTYPE html PUBLIC "a>b"><p>x</p>"#;
    let expected = html(vec![body(vec![
        Value::string("b\">"),
        Value::list(vec![Value::symbol("p"), Value::NIL, Value::string("x")]),
    ])]);
    assert_eq!(parse_html_region(quoted, false).unwrap(), expected);
    assert_eq!(parse_html_region(quoted, true).unwrap(), expected);
}

#[test]
fn libxml_parse_html_region_preserves_rcdata_in_title_and_textarea() {
    crate::test_utils::init_test_tracing();

    // RCDATA: `<` is text, not markup, and character references are resolved.
    // GNU 31.1 oracle: (html nil (head nil (title nil "a < b &")))
    let title = br#"<html><head><title>a < b &amp;</title></head></html>"#;
    let expected = html(vec![Value::list(vec![
        Value::symbol("head"),
        Value::NIL,
        Value::list(vec![
            Value::symbol("title"),
            Value::NIL,
            Value::string("a < b &"),
        ]),
    ])]);
    assert_eq!(parse_html_region(title, false).unwrap(), expected);
    assert_eq!(parse_html_region(title, true).unwrap(), expected);

    // GNU 31.1 oracle: (html nil (body nil (textarea nil "a < b &")))
    let textarea = br#"<html><body><textarea>a < b &amp;</textarea></body></html>"#;
    let expected = html(vec![body(vec![Value::list(vec![
        Value::symbol("textarea"),
        Value::NIL,
        Value::string("a < b &"),
    ])])]);
    assert_eq!(parse_html_region(textarea, false).unwrap(), expected);
    assert_eq!(parse_html_region(textarea, true).unwrap(), expected);
}

#[test]
fn zlib_available_p_returns_true() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::emacs_core::zlib::builtin_zlib_available_p(vec![]).unwrap(),
        Value::T
    );
    let zlib_arity =
        crate::emacs_core::zlib::builtin_zlib_available_p(vec![Value::fixnum(1)]).unwrap_err();
    match zlib_arity.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("zlib-available-p"), Value::fixnum(1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}
