use super::*;

fn make_ctx() -> super::super::eval::Context {
    super::super::eval::Context::new()
}

#[test]
fn libxml_parse_xml_region_arity_and_nil_returns() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();

    // No args, no buffer → nil
    assert_eq!(
        builtin_libxml_parse_xml_region(&mut ctx, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_libxml_parse_xml_region(&mut ctx, vec![Value::NIL]).unwrap(),
        Value::NIL
    );

    // Too many args → error
    let wrong_arity = builtin_libxml_parse_xml_region(
        &mut ctx,
        vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .unwrap_err();
    match wrong_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("libxml-parse-xml-region"), Value::fixnum(5)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    // Wrong type for START → error
    let wrong_type =
        builtin_libxml_parse_xml_region(&mut ctx, vec![Value::string("x"), Value::fixnum(1)])
            .unwrap_err();
    match wrong_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::string("x")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    // Wrong type for BASE-URL → error (validated before buffer access)
    let wrong_base =
        builtin_libxml_parse_xml_region(&mut ctx, vec![Value::NIL, Value::NIL, Value::fixnum(42)])
            .unwrap_err();
    match wrong_base {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(42)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn libxml_parse_html_region_arity_and_nil_returns() {
    crate::test_utils::init_test_tracing();
    let mut ctx = make_ctx();

    // No args, no buffer → nil
    assert_eq!(
        builtin_libxml_parse_html_region(&mut ctx, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_libxml_parse_html_region(&mut ctx, vec![Value::NIL]).unwrap(),
        Value::NIL
    );

    // Too many args → error
    let wrong_arity = builtin_libxml_parse_html_region(
        &mut ctx,
        vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .unwrap_err();
    match wrong_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("libxml-parse-html-region"), Value::fixnum(5)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    // Wrong type for START → error
    let wrong_type =
        builtin_libxml_parse_html_region(&mut ctx, vec![Value::string("x"), Value::fixnum(1)])
            .unwrap_err();
    match wrong_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::string("x")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    // Wrong type for BASE-URL → error (validated before buffer access)
    let wrong_base =
        builtin_libxml_parse_html_region(&mut ctx, vec![Value::NIL, Value::NIL, Value::fixnum(42)])
            .unwrap_err();
    match wrong_base {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(42)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn libxml_available_p_returns_true_and_validates_arity() {
    crate::test_utils::init_test_tracing();
    assert_eq!(builtin_libxml_available_p(vec![]).unwrap(), Value::T);

    let libxml_arity = builtin_libxml_available_p(vec![Value::fixnum(1)]).unwrap_err();
    match libxml_arity {
        Flow::Signal(sig) => {
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
            head_title.clone(),
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
fn zlib_available_p_returns_true() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::emacs_core::zlib::builtin_zlib_available_p(vec![]).unwrap(),
        Value::T
    );
    let zlib_arity =
        crate::emacs_core::zlib::builtin_zlib_available_p(vec![Value::fixnum(1)]).unwrap_err();
    match zlib_arity {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("zlib-available-p"), Value::fixnum(1)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

/// `zlib-decompress-region` inserts RAW BYTES, never decoded characters.
///
/// GNU inserts the inflate output with `insert_from_gap (decompressed,
/// decompressed, 0, false)` (src/decompress.c:311) -- nchars EQUALS nbytes,
/// because the function is defined only for unibyte buffers, where "character
/// positions and bytes are the same" (its own comment). Treating the output as
/// multibyte text collapses each multibyte sequence to a single character and
/// then narrows it to one byte, so `Omega' (U+03A9, CE A9) arrives as the lone
/// byte A9 and a 4-byte emoji arrives as NUL. That is silent corruption of any
/// compressed non-ASCII payload, which is how it stayed hidden: `jka-compr'
/// shells out to gzip and never touches this path.
///
/// The fixture is "A<U+03A9>B<U+4F60>C<U+1F600>D\n" gzipped, whose plain bytes
/// are 65 206 169 66 228 189 160 67 240 159 152 128 68 10 -- deliberately one
/// 2-byte, one 3-byte and one 4-byte sequence. The 4-byte one carries its
/// weight: U+1F600 truncated to a byte is 0x00, and a NUL in the middle of the
/// output is the corruption most likely to be misread later as an empty or
/// terminated result rather than as damaged data.
#[test]
fn zlib_decompress_region_inserts_raw_bytes_not_decoded_characters() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(with-temp-buffer
             (set-buffer-multibyte nil)
             (dolist (byte '(31 139 8 0 0 0 0 0 2 255 115 60 183 210 233 201
                             222 5 206 31 230 207 104 112 225 2 0 78 227 62
                             114 14 0 0 0))
               (insert byte))
             (list (zlib-decompress-region (point-min) (point-max))
                   (string-to-list (buffer-string))))"#,
    );
    assert_eq!(
        result, "OK (t (65 206 169 66 228 189 160 67 240 159 152 128 68 10))",
        "every decompressed byte must survive; a multibyte-decoded reading \
         collapsed 206 169 to 169, 228 189 160 to 96, and 240 159 152 128 to NUL"
    );
}

/// libxml2 substitutes predefined entities and character references straight
/// into the character data it is building, so an element whose text contains
/// `&amp;` still reaches GNU's `make_dom` (`src/xml.c:123-160`) as ONE
/// `XML_TEXT_NODE` and comes back as one string child.
///
/// `quick_xml` reports the reference as a separate `Event::GeneralRef`, so
/// without re-joining the run Neomacs both dropped the resolved character and
/// handed the caller three children where GNU hands one.
/// `citeproc-term--from-xml-frag` branches on exactly that count
/// (`(if (= (length frag) 2) ...)`) while parsing the CSL locale's
/// `<term name="editortranslator" form="verb">edited &amp; translated by</term>`,
/// and on the split it called `cl-caddr` on the string `"edited "` -- the
/// `(wrong-type-argument listp "edited ")` org-ref's CSL export raised.
///
/// A CDATA section stays its own node in libxml2, so it does NOT join the run.
///
/// Every expected value here was produced by running the input through GNU
/// Emacs 31.0.90's `libxml-parse-xml-region`.
#[test]
fn xml_entity_references_join_the_surrounding_text_node_like_libxml2() {
    crate::test_utils::init_test_tracing();
    let cases: &[(&str, &str)] = &[
        (
            r#"<r><term form="verb">edited &amp; translated by</term></r>"#,
            r#"(r nil (term ((form . "verb")) "edited & translated by"))"#,
        ),
        (r#"<r><t>a&lt;b&gt;c</t></r>"#, r#"(r nil (t nil "a<b>c"))"#),
        // Character references resolve the same way.
        (r#"<r><t>&#65;&#x42;C</t></r>"#, r#"(r nil (t nil "ABC"))"#),
        (r#"<r><t>&amp;</t></r>"#, r#"(r nil (t nil "&"))"#),
        // A run that resolved a reference is never ignorable whitespace, even
        // though `XML_PARSE_NOBLANKS` would drop the spaces on their own.
        (r#"<r><t> &amp; </t></r>"#, r#"(r nil (t nil " & "))"#),
        // An element boundary still ends the run.
        (
            r#"<r><t>x<b/>y</t></r>"#,
            r#"(r nil (t nil "x" (b nil) "y"))"#,
        ),
        // CDATA is its own node in libxml2 and stays a separate string.
        (
            r#"<r><t><![CDATA[raw]]>tail</t></r>"#,
            r#"(r nil (t nil "raw" "tail"))"#,
        ),
        // Whitespace-only text between elements is still dropped.
        ("<r>\n  <t>v</t>\n</r>", r#"(r nil (t nil "v"))"#),
    ];
    for (input, expected) in cases {
        let parsed = parse_xml_region(input.as_bytes(), false).expect("parse");
        assert_eq!(
            crate::emacs_core::print::print_value(&parsed),
            *expected,
            "input: {input}"
        );
    }
}
