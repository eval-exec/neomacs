//! XML and HTML parsing support, matching GNU Emacs's xml.c.
//!
//! Provides real implementations for:
//! - `libxml-parse-html-region` — HTML parsing via `tl` crate
//! - `libxml-parse-xml-region` — XML parsing via `quick-xml` crate
//! - `libxml-available-p` — feature availability probe

use neovm_edit::buffer::{BufferId, BufferManager, EmacsByteRange};
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_edit::emacs_core::position::LispRegionArgs;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, expect_args, expect_max_args, signal};
use neovm_value::value::ValueKind;
use neovm_value::value::*;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_optional_string(v: Value) -> Result<Option<String>, Flow> {
    if v.is_nil() {
        return Ok(None);
    }
    match v.kind() {
        ValueKind::String => Ok(Some(
            v.as_lisp_string()
                .unwrap()
                .as_utf8_str()
                .unwrap_or_default()
                .to_string(),
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), v],
        )),
    }
}

fn validate_base_url(args: &[Value]) -> Result<(), Flow> {
    if args.len() > 2 && !args[2].is_nil() {
        expect_optional_string(args[2])?;
    }
    Ok(())
}

/// Validate region arguments, handling nil start/end as point-min/point-max.
fn validate_region_byte_bounds(
    buffers: &mut BufferManager,
    args: &[Value],
) -> Result<Option<(BufferId, EmacsByteRange)>, Flow> {
    let Some(buf) = buffers.current_buffer() else {
        return Ok(None);
    };

    let region = LispRegionArgs::from_optional_values(
        buffers,
        args.first().copied(),
        args.get(1).copied(),
        buf.point_min_lisp_char_pos(),
        buf.point_max_lisp_char_pos(),
    )?;
    let byte_range = region.accessible_byte_range(buf)?;

    Ok(Some((buf.id(), byte_range)))
}

fn read_region_bytes(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    byte_range: EmacsByteRange,
) -> Result<Vec<u8>, Flow> {
    let bytes = neovm_edit::emacs_core::editfns::read_buffer_region_bytes_in_manager(
        buffers, buffer_id, byte_range,
    )?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// GNU Emacs parse tree format
// ---------------------------------------------------------------------------
//
// Elements: (tag-name ((attr1 . "val1") (attr2 . "val2")) child1 child2 ...)
// Text:     "string"
// Comment:  (comment nil "text")
// Top-level with comments: (top nil children...)
// ---------------------------------------------------------------------------

/// Tracks XML namespace prefixes declared along the open-element chain so that
/// element/attribute qnames can be resolved the way libxml2 does: a declared
/// prefix (or the built-in `xml`) is stripped, leaving only the local name,
/// while an *undeclared* prefix is kept verbatim. The set of prefixes is the
/// union over every open ancestor element, so a prefix declared on an ancestor
/// stays in scope for descendants and goes out of scope when that ancestor's
/// end tag is reached.
#[derive(Default)]
struct NamespaceScopes {
    /// One frame per currently-open element, each holding the prefixes that
    /// element declared (`xmlns:PREFIX`). Empty string means a default
    /// (`xmlns`) declaration, which never affects qname resolution.
    frames: Vec<Vec<String>>,
}

impl NamespaceScopes {
    /// `xml` is reserved/predeclared, so a prefix is "declared" if it is `xml`
    /// or appears in any open element's declaration set.
    fn prefix_declared(&self, prefix: &str) -> bool {
        prefix == "xml"
            || self
                .frames
                .iter()
                .any(|frame| frame.iter().any(|p| p == prefix))
    }

    /// Resolve a qname: strip the prefix iff it is a declared namespace prefix,
    /// otherwise return the qname unchanged.
    fn resolve(&self, qname: &str) -> String {
        match qname.split_once(':') {
            Some((prefix, local)) if !prefix.is_empty() && self.prefix_declared(prefix) => {
                local.to_string()
            }
            _ => qname.to_string(),
        }
    }
}

/// Character data accumulated since the last non-character-data event.
///
/// libxml2 substitutes the predefined entities (`&amp;`, `&lt;`, `&gt;`,
/// `&quot;`, `&apos;`) and character references directly into the character
/// data it is building, so `<term>edited &amp; translated by</term>` reaches
/// `make_dom` (GNU `src/xml.c:123-160`) as ONE `XML_TEXT_NODE` and GNU returns
/// one string child.  `quick_xml` instead reports the reference as its own
/// `Event::GeneralRef` between two `Event::Text`s, so the run has to be
/// re-joined here or a caller counting an element's children sees three where
/// GNU sees one.
///
/// `citeproc-term--from-xml-frag` counts exactly that
/// (`(if (= (length frag) 2) ...)`), and on the split it takes the two-form
/// branch and calls `cl-caddr` on the string `"edited "` — the
/// `(wrong-type-argument listp "edited ")` org-ref's CSL export raised.
#[derive(Default)]
struct PendingText {
    text: String,
    /// A run that resolved a reference is never "ignorable whitespace", even
    /// if the resolved character is a space: libxml2's `XML_PARSE_NOBLANKS`
    /// only drops text that was blank in the source.
    saw_reference: bool,
}

impl PendingText {
    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Emit the accumulated run as one string child, dropping it when it is
    /// ignorable whitespace (libxml2 `XML_PARSE_NOBLANKS`, which GNU passes
    /// at `src/xml.c:226-227`).
    fn flush(
        &mut self,
        stack: &mut [(String, Vec<Value>, Vec<Value>)],
        top_level: &mut Vec<Value>,
        has_top_level_comments: &mut bool,
    ) {
        if self.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        let saw_reference = std::mem::replace(&mut self.saw_reference, false);
        if !saw_reference && is_xml_blank_text(&text) {
            return;
        }
        let node = Value::string(text.as_str());
        if let Some((_, _, children)) = stack.last_mut() {
            children.push(node);
        } else {
            *has_top_level_comments = true;
            top_level.push(node);
        }
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// Parse region using quick-xml and return Elisp parse tree.
fn parse_xml_region(data: &[u8], discard_comments: bool) -> Option<Value> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(false);

    // Stack of (tag-name, attributes, children)
    let mut stack: Vec<(String, Vec<Value>, Vec<Value>)> = Vec::new();
    let mut top_level: Vec<Value> = Vec::new();
    let mut has_top_level_comments = false;
    let mut scopes = NamespaceScopes::default();
    let mut pending = PendingText::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                pending.flush(&mut stack, &mut top_level, &mut has_top_level_comments);
                // Push this element's namespace declarations before resolving
                // its own qname, so a prefix declared and used on the same
                // element (`<p:root xmlns:p=...>`) resolves correctly.
                scopes.frames.push(collect_ns_declarations(e.attributes()));
                let name = e.name();
                let raw_tag = String::from_utf8_lossy(name.as_ref());
                let tag = scopes.resolve(raw_tag.as_ref());
                let attrs = parse_xml_attributes(e.attributes(), &scopes);
                stack.push((tag, attrs, Vec::new()));
            }
            Ok(Event::Empty(ref e)) => {
                pending.flush(&mut stack, &mut top_level, &mut has_top_level_comments);
                // Empty elements open and close immediately; their declarations
                // only scope their own qname/attributes, so push/resolve/pop.
                scopes.frames.push(collect_ns_declarations(e.attributes()));
                let name = e.name();
                let raw_tag = String::from_utf8_lossy(name.as_ref());
                let tag = scopes.resolve(raw_tag.as_ref());
                let attrs = parse_xml_attributes(e.attributes(), &scopes);
                scopes.frames.pop();
                let node = make_element_node(&tag, attrs, Vec::new());
                if let Some((_, _, children)) = stack.last_mut() {
                    children.push(node);
                } else {
                    top_level.push(node);
                }
            }
            Ok(Event::End(_)) => {
                pending.flush(&mut stack, &mut top_level, &mut has_top_level_comments);
                scopes.frames.pop();
                if let Some((tag, attrs, children)) = stack.pop() {
                    let node = make_element_node(&tag, attrs, children);
                    if let Some((_, _, parent_children)) = stack.last_mut() {
                        parent_children.push(node);
                    } else {
                        top_level.push(node);
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let decoded = e.decode().ok()?;
                let text = quick_xml::escape::unescape(decoded.as_ref()).ok()?;
                pending.text.push_str(text.as_ref());
            }
            Ok(Event::GeneralRef(ref e)) => {
                // libxml2 substitutes the reference into the character data it
                // is accumulating, so this belongs to the SAME text node as the
                // characters around it.
                let name = e.decode().ok()?;
                let reference = format!("&{name};");
                let resolved = quick_xml::escape::unescape(&reference).ok()?;
                pending.text.push_str(resolved.as_ref());
                pending.saw_reference = true;
            }
            Ok(Event::CData(ref e)) => {
                // libxml2 keeps a CDATA section as its own
                // XML_CDATA_SECTION_NODE, which GNU turns into its own string
                // child (`src/xml.c:158-160`), so it does NOT join the run.
                pending.flush(&mut stack, &mut top_level, &mut has_top_level_comments);
                let text = String::from_utf8_lossy(e.as_ref());
                if !text.is_empty() {
                    let node = Value::string(text.as_ref());
                    if let Some((_, _, children)) = stack.last_mut() {
                        children.push(node);
                    } else {
                        has_top_level_comments = true;
                        top_level.push(node);
                    }
                }
            }
            Ok(Event::Comment(ref e)) => {
                pending.flush(&mut stack, &mut top_level, &mut has_top_level_comments);
                if discard_comments && stack.is_empty() {
                    continue;
                }
                let text = String::from_utf8_lossy(e);
                let node = Value::list(vec![
                    Value::symbol("comment"),
                    Value::NIL,
                    Value::string(text.as_ref()),
                ]);
                if let Some((_, _, children)) = stack.last_mut() {
                    children.push(node);
                } else {
                    has_top_level_comments = true;
                    top_level.push(node);
                }
            }
            Ok(Event::Eof) => {
                pending.flush(&mut stack, &mut top_level, &mut has_top_level_comments);
                break;
            }
            Err(_) => return None,
            _ => continue,
        }
    }

    if !stack.is_empty() {
        return None;
    }

    if top_level.is_empty() {
        return Some(Value::NIL);
    }

    if has_top_level_comments && top_level.len() > 1 {
        Some(Value::list(
            std::iter::once(Value::symbol("top"))
                .chain(std::iter::once(Value::NIL))
                .chain(top_level)
                .collect(),
        ))
    } else {
        Some(top_level.remove(0))
    }
}

/// Collect the namespace-prefix declarations on a single element: every
/// `xmlns:PREFIX="..."` contributes `PREFIX`, and a bare `xmlns="..."`
/// contributes the empty string (a default namespace, which is recorded but
/// never strips a prefix). Matches libxml2, which consumes these as namespace
/// nodes rather than ordinary attributes.
fn collect_ns_declarations(attrs: quick_xml::events::attributes::Attributes<'_>) -> Vec<String> {
    let mut decls = Vec::new();
    for attr in attrs.flatten() {
        let key = attr.key.as_ref();
        if key == b"xmlns" {
            decls.push(String::new());
        } else if let Some(prefix) = key.strip_prefix(b"xmlns:") {
            decls.push(String::from_utf8_lossy(prefix).into_owned());
        }
    }
    decls
}

/// Parse XML attributes into a list of dotted pairs. Namespace-declaration
/// attributes (`xmlns`, `xmlns:*`) are dropped, and any remaining attribute
/// name carrying a declared namespace prefix is resolved to its local name —
/// reproducing the property list libxml2 hands to GNU's `make_dom`.
fn parse_xml_attributes(
    attrs: quick_xml::events::attributes::Attributes<'_>,
    scopes: &NamespaceScopes,
) -> Vec<Value> {
    let mut result = Vec::new();
    for attr in attrs.flatten() {
        let key = attr.key.as_ref();
        // Drop namespace declarations; they are not ordinary attributes.
        if key == b"xmlns" || key.starts_with(b"xmlns:") {
            continue;
        }
        let raw_key = String::from_utf8_lossy(key);
        let key = scopes.resolve(raw_key.as_ref());
        let val = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(&attr.value).into_owned());
        result.push(Value::cons(Value::symbol(&key), Value::string(&val)));
    }
    result
}

fn is_xml_blank_text(text: &str) -> bool {
    text.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
}

/// Build an Elisp element node: (tag-name ((attr . val) ...) children...)
fn make_element_node(tag: &str, attrs: Vec<Value>, children: Vec<Value>) -> Value {
    let attr_list = if attrs.is_empty() {
        Value::NIL
    } else {
        Value::list(attrs)
    };
    let mut elements = vec![Value::symbol(tag), attr_list];
    elements.extend(children);
    Value::list(elements)
}

// ---------------------------------------------------------------------------
// HTML parsing via tl crate
// ---------------------------------------------------------------------------

/// A converted top-level HTML node, tagged with its element name (if it is an
/// element) so the HTML5 document wrapper can classify it as head vs body.
struct TopNode {
    /// The lowercase element name, or `None` for text/comment nodes.
    tag: Option<String>,
    value: Value,
}

/// HTML elements that belong in `<head>` when they appear before any body
/// content, matching how libxml2's HTML parser routes them. Notably `noscript`
/// is *not* here: libxml2 places it in `<body>` in this fragment context.
fn is_head_element(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "title" | "base" | "link" | "meta" | "style" | "script"
    )
}

/// Parse region as HTML using tl and return Elisp parse tree.
fn parse_html_region(data: &[u8], discard_comments: bool) -> Option<Value> {
    let input = String::from_utf8_lossy(data);
    let mut remaining = input.as_ref();
    // libxml2 recognizes a UTF-8 BOM only when it precedes document content;
    // a lone BOM is parsed as text. Strip one leading BOM under that rule so an
    // encoded HTML file still reaches the prolog scanner below.
    let bom = '\u{FEFF}';
    if remaining.starts_with(bom) && remaining.len() > bom.len_utf8() {
        remaining = &remaining[bom.len_utf8()..];
    }

    // shield_html handles comments, processing instructions, DOCTYPEs, and
    // text-element bodies uniformly, including at the document prolog.
    let (protected, shielded) = shield_html(remaining);
    let dom = tl::parse(protected.as_ref(), tl::ParserOptions::default()).ok()?;
    let parser = dom.parser();
    let mut nodes: Vec<TopNode> = Vec::new();
    for handle in dom.children() {
        let node = handle.get(parser)?;
        if let Some(top) = convert_tl_top_node(node, parser, &shielded) {
            nodes.push(top);
        }
    }

    if nodes.is_empty() {
        return Some(Value::NIL);
    }

    // libxml2's HTML parser performs full HTML5 tree construction, implicitly
    // creating the <html>/<head>/<body> document structure. The `tl` crate
    // returns the fragment verbatim, so reconstruct that wrapper here.
    wrap_html_document(nodes, discard_comments)
}

/// Reconstruct libxml2's implicit `<html>`/`<head>`/`<body>` document structure
/// around a parsed HTML fragment.
///
/// Document-level comments (and processing instructions, which libxml2's HTML
/// parser exposes as comments) are returned in a `(top ...)` list around the
/// root element when `DISCARD-COMMENTS` is nil, and dropped when it is non-nil.
/// This matches GNU `parse_region` / `make_dom` in `src/xml.c`: the flag only
/// removes document-level comments, never nested ones.
fn wrap_html_document(nodes: Vec<TopNode>, discard_comments: bool) -> Option<Value> {
    // libxml2 drops document-level leading/trailing ASCII whitespace but keeps
    // whitespace between elements and inside elements.
    let mut doc_comments: Vec<Value> = Vec::new();
    let mut fragment: Vec<TopNode> = Vec::new();
    let mut started = false;
    for node in nodes {
        if !started {
            if is_top_comment(&node) {
                if !discard_comments {
                    doc_comments.push(node.value);
                }
                continue;
            }
            if is_blank_text_node(&node) {
                continue;
            }
            started = true;
        }
        fragment.push(node);
    }
    // Around an explicit root, document-level blanks include blanks between
    // the root and trailing comments. For a bare fragment all content,
    // including blanks before a comment, stays in the implicit body; only
    // trailing blanks at the very end are dropped.
    let has_html_root = fragment
        .iter()
        .any(|node| !is_top_comment(node) && node.tag.as_deref() == Some("html"));
    if has_html_root {
        let mut keep = vec![true; fragment.len()];
        let mut saw_content = false;
        for index in (0..fragment.len()).rev() {
            if is_blank_text_node(&fragment[index]) {
                if !saw_content {
                    keep[index] = false;
                }
            } else if !is_top_comment(&fragment[index]) {
                saw_content = true;
            }
        }
        fragment = fragment
            .into_iter()
            .zip(keep)
            .filter_map(|(node, keep)| keep.then_some(node))
            .collect();
    } else {
        while fragment.last().is_some_and(is_blank_text_node) {
            fragment.pop();
        }
    }
    if !fragment.iter().any(|n| !is_top_comment(n)) {
        return Some(Value::NIL);
    }

    // A single explicit <html> root is already a complete document. Keep
    // document-level comments on both sides of it in source order.
    let non_comment_count = fragment.iter().filter(|n| !is_top_comment(n)).count();
    if has_html_root && non_comment_count == 1 {
        let mut values = doc_comments;
        for node in fragment {
            if is_top_comment(&node) && discard_comments {
                continue;
            }
            values.push(node.value);
        }
        if values.len() == 1 {
            return values.into_iter().next();
        }
        return Some(top_document(values));
    }

    let document = wrap_html_fragment(fragment)?;
    if doc_comments.is_empty() {
        return Some(document);
    }
    let mut values = vec![Value::symbol("top"), Value::NIL];
    values.extend(doc_comments);
    values.push(document);
    Some(Value::list(values))
}

fn top_document(values: Vec<Value>) -> Value {
    Value::list(
        std::iter::once(Value::symbol("top"))
            .chain(std::iter::once(Value::NIL))
            .chain(values)
            .collect(),
    )
}

fn is_top_comment(node: &TopNode) -> bool {
    node.tag.is_none() && is_comment_node(&node.value)
}

fn is_blank_text_node(node: &TopNode) -> bool {
    node.tag.is_none() && is_blank_string(&node.value)
}

fn is_blank_string(value: &Value) -> bool {
    value
        .as_lisp_string()
        .and_then(|text| text.as_utf8_str())
        .is_some_and(|text| text.chars().all(|c| c.is_ascii_whitespace()))
}

/// Where a top-level node lands while reconstructing libxml2's document.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FragmentMode {
    /// Before any body content: head elements and comments go to `<head>`.
    Head,
    /// After an explicit `</head>`: head elements and comments are direct
    /// `<html>` children until body content starts.
    AfterHead,
    /// Body content is open; later nodes stay in `<body>`.
    Body,
    /// After an explicit `<body>` start: later nodes are direct `<html>`
    /// children.
    AfterBody,
}

/// Wrap the fragment part of a document in libxml2's implicit html/head/body
/// structure.
fn wrap_html_fragment(nodes: Vec<TopNode>) -> Option<Value> {
    let mut before_head = ChildVec::default();
    let mut head = ChildVec::default();
    let mut between = ChildVec::default();
    let mut body = ChildVec::default();
    let mut after_body = ChildVec::default();
    // Independent facts needed for the result: explicit head/body elements are
    // emitted even when empty, and their attributes are preserved.
    let mut explicit_head_seen = false;
    let mut head_seen = false;
    let mut head_attrs = Value::NIL;
    let mut body_attrs = Value::NIL;
    let mut mode = FragmentMode::Head;

    for node in nodes {
        if is_top_comment(&node) {
            match mode {
                FragmentMode::Head => {
                    if head_seen {
                        head.push(node.value);
                    } else {
                        // libxml2 keeps comments before head content as direct
                        // children of <html>, before any <head> node.
                        before_head.push(node.value);
                    }
                }
                FragmentMode::AfterHead => between.push(node.value),
                FragmentMode::Body => body.push(node.value),
                FragmentMode::AfterBody => after_body.push(node.value),
            }
            continue;
        }
        if is_blank_text_node(&node) && matches!(mode, FragmentMode::Head | FragmentMode::AfterHead)
        {
            // Whitespace does not end the head phase or become head content.
            continue;
        }
        match node.tag.as_deref() {
            Some("head") => match mode {
                FragmentMode::AfterBody => after_body.push(node.value),
                FragmentMode::Body => {
                    // A head tag after body content has started is ignored;
                    // its children are body content.
                    body.extend_children(&node.value);
                }
                FragmentMode::Head | FragmentMode::AfterHead => {
                    // Unwrap an explicit <head>, merging its children into head.
                    head_attrs = element_attributes(&node.value);
                    head.extend_children(&node.value);
                    explicit_head_seen = true;
                    head_seen = true;
                    if mode == FragmentMode::Head {
                        mode = FragmentMode::AfterHead;
                    }
                }
            },
            Some("body") => match mode {
                FragmentMode::AfterBody => after_body.push(node.value),
                FragmentMode::Head | FragmentMode::AfterHead => {
                    // An explicit body starts the body only when no body
                    // content has appeared yet.
                    mode = FragmentMode::AfterBody;
                    body_attrs = element_attributes(&node.value);
                    body.extend_children(&node.value);
                }
                FragmentMode::Body => {
                    // A body tag after content has started merges into the
                    // implicit body and does not switch modes.
                    body.extend_children(&node.value);
                }
            },
            Some(tag) if is_head_element(tag) => match mode {
                FragmentMode::Head => {
                    head.push(node.value);
                    head_seen = true;
                }
                FragmentMode::AfterHead => between.push(node.value),
                FragmentMode::Body => body.push(node.value),
                FragmentMode::AfterBody => after_body.push(node.value),
            },
            _ => match mode {
                FragmentMode::Head | FragmentMode::AfterHead => {
                    body.push(node.value);
                    mode = FragmentMode::Body;
                }
                FragmentMode::Body => body.push(node.value),
                FragmentMode::AfterBody => after_body.push(node.value),
            },
        }
    }

    let before_head = before_head.finish();
    let head = head.finish();
    let between = between.finish();
    let body = body.finish();
    let after_body = after_body.finish();
    // (html nil BEFORE [head nil HEAD...] [BETWEEN...]
    //        (body nil BODY...) [AFTER...])
    let has_before_head = !before_head.is_empty();
    let has_head = !head.is_empty() || explicit_head_seen;
    let has_body = !body.is_empty() || mode == FragmentMode::AfterBody;
    let mut html_children: Vec<Value> = vec![Value::symbol("html"), Value::NIL];
    html_children.extend(before_head);
    if has_head {
        let mut head_node = vec![Value::symbol("head"), head_attrs];
        head_node.extend(head);
        html_children.push(Value::list(head_node));
    }
    html_children.extend(between);
    // libxml2 omits <body> when there is only head content or only comments
    // before the head.
    if has_body || (!has_head && !has_before_head) {
        let mut body_node = vec![Value::symbol("body"), body_attrs];
        body_node.extend(body);
        html_children.push(Value::list(body_node));
    }
    html_children.extend(after_body);
    Some(Value::list(html_children))
}

/// Convert a top-level `tl` node, capturing its element tag (lowercased) so the
/// HTML document wrapper can classify it. Returns `None` for nodes that convert
/// to nothing (e.g. blank text or a discarded comment).
fn convert_tl_top_node(
    node: &tl::Node,
    parser: &tl::Parser,
    shielded: &ShieldedHtml,
) -> Option<TopNode> {
    let value = convert_tl_node(node, parser, shielded)?;
    let tag = match node {
        tl::Node::Tag(element) => {
            let raw_tag = element.name().as_utf8_str();
            let name = raw_tag.strip_suffix('/').unwrap_or(raw_tag.as_ref());
            Some(name.to_ascii_lowercase())
        }
        _ => None,
    };
    Some(TopNode { tag, value })
}

/// Build `(comment nil TEXT)`.
fn comment_value(text: &str) -> Value {
    Value::list(vec![
        Value::symbol("comment"),
        Value::NIL,
        Value::string(text),
    ])
}

/// Is `value` a `(comment ...)` element node?
fn is_comment_node(value: &Value) -> bool {
    neovm_value::value::list_to_vec(value)
        .and_then(|items| items.first().copied())
        .and_then(|head| head.as_bare_symbol_name())
        .is_some_and(|name| name == "comment")
}

/// Return the children of an element node `(tag attrs child...)` (i.e. drop the
/// leading tag symbol and attribute list).
fn element_children(value: &Value) -> Vec<Value> {
    match neovm_value::value::list_to_vec(value) {
        Some(items) if items.len() > 2 => items[2..].to_vec(),
        _ => Vec::new(),
    }
}

/// Return the attribute list of an element node `(tag attrs child...)`.
fn element_attributes(value: &Value) -> Value {
    match neovm_value::value::list_to_vec(value) {
        Some(items) if items.len() > 1 => items[1],
        _ => Value::NIL,
    }
}

/// Accumulates element children while coalescing adjacent text runs.
///
/// Appending to a String and creating the Lisp string once avoids the
/// quadratic copying of merging a growing string for every piece.
#[derive(Default)]
struct ChildVec {
    values: Vec<Value>,
    text: String,
}

impl ChildVec {
    fn push(&mut self, value: Value) {
        if let Some(text) = value.as_lisp_string().and_then(|text| text.as_utf8_str()) {
            self.text.push_str(text);
        } else {
            self.flush_text();
            self.values.push(value);
        }
    }

    fn extend_children(&mut self, value: &Value) {
        for child in element_children(value) {
            self.push(child);
        }
    }

    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            let text = std::mem::take(&mut self.text);
            self.values.push(Value::string(text));
        }
    }

    fn finish(mut self) -> Vec<Value> {
        self.flush_text();
        self.values
    }
}

fn convert_tl_node(node: &tl::Node, parser: &tl::Parser, shielded: &ShieldedHtml) -> Option<Value> {
    match node {
        tl::Node::Raw(raw) => {
            // GNU's libxml2 parser resolves HTML character references before
            // make_dom copies a text node into Lisp. tl deliberately exposes
            // source bytes, so normalize them at this adapter boundary. Keep
            // whitespace here; the document builder trims only leading and
            // trailing document-level blanks.
            let raw_text = raw.as_utf8_str();
            let text = htmlize::unescape(raw_text.as_ref());
            if text.is_empty() {
                None
            } else {
                Some(Value::string(text.as_ref()))
            }
        }
        tl::Node::Comment(comment) => {
            let raw_text = comment.as_utf8_str();
            let mut text = raw_text.as_ref();
            if text.starts_with("<!--") {
                text = &text[4..];
            }
            if text.ends_with("-->") {
                text = &text[..text.len() - 3];
            }
            if shielded.ignored_comments.contains(text) {
                return None;
            }
            let text = shielded
                .pi_comments
                .get(text)
                .map(String::as_str)
                .unwrap_or(text);
            Some(comment_value(text))
        }
        tl::Node::Tag(element) => {
            // The `tl` crate leaves the trailing `/` of an XHTML-style
            // self-closing tag (`<hr/>`, `<rect/>`) in the tag name.
            // libxml2's HTML parser never does that, so strip it to match GNU.
            let raw_tag = element.name().as_utf8_str();
            let tag = raw_tag.strip_suffix('/').unwrap_or(raw_tag.as_ref());
            let attrs = convert_tl_attributes(element.attributes());

            // libxml2 routes content directly under <html> into an implicit
            // <body>. Rebuild the html element's children through the fragment
            // wrapper so inline content and its whitespace match GNU.
            if tag.eq_ignore_ascii_case("html") {
                let children = element.children();
                let mut inner: Vec<TopNode> = Vec::new();
                for handle in children.top().iter() {
                    let child = handle.get(parser)?;
                    if let Some(top) = convert_tl_top_node(child, parser, shielded) {
                        inner.push(top);
                    }
                }
                if !inner.iter().any(|node| !is_blank_text_node(node)) {
                    return Some(make_element_node(tag, attrs, Vec::new()));
                }
                let built = wrap_html_fragment(inner)?;
                let mut child_values = element_children(&built);
                while child_values.last().is_some_and(is_blank_string) {
                    child_values.pop();
                }
                return Some(make_element_node(tag, attrs, child_values));
            }

            let is_rcdata_tag =
                tag.eq_ignore_ascii_case("title") || tag.eq_ignore_ascii_case("textarea");
            let is_text_element = is_rcdata_tag
                || tag.eq_ignore_ascii_case("style")
                || tag.eq_ignore_ascii_case("script");
            let child_values = if is_text_element {
                let inner = element.inner_text(parser);
                let text = match shielded.text_elements.get(inner.as_ref()) {
                    Some(original) if is_rcdata_tag => htmlize::unescape(original),
                    Some(original) => Cow::Borrowed(original.as_str()),
                    None => {
                        let fallback = raw_text_content(element, tag);
                        if is_rcdata_tag {
                            Cow::Owned(htmlize::unescape(&fallback).into_owned())
                        } else {
                            Cow::Owned(fallback)
                        }
                    }
                };
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![Value::string(text.as_ref())]
                }
            } else {
                let children = element.children();
                let children_handles = children.top();
                let mut vals = ChildVec::default();
                for handle in children_handles.iter() {
                    let child_node = handle.get(parser)?;
                    if let Some(val) = convert_tl_node(child_node, parser, shielded) {
                        vals.push(val);
                    }
                }
                vals.finish()
            };
            // libxml2 drops whitespace-only direct children of head.
            let child_values = if tag.eq_ignore_ascii_case("head") {
                child_values
                    .into_iter()
                    .filter(|value| !is_blank_string(value))
                    .collect()
            } else {
                child_values
            };
            Some(make_element_node(tag, attrs, child_values))
        }
    }
}

/// Choose a prefix once per document that is absent from its text.
///
/// Retries are bounded so a crafted run of matching bytes cannot make this
/// quadratic; after the bounded candidates, a randomly seeded hash gives a
/// collision probability that is negligible.
fn unique_prefix(input: &str, base: &str) -> String {
    for counter in 0..32u32 {
        let prefix = format!("{base}{counter:x}-");
        if !input.contains(&prefix) {
            return prefix;
        }
    }
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write(input.as_bytes());
    format!("{base}{:x}-", hasher.finish())
}

/// Whether the `<` at `index` starts an HTML tag-like token rather than text.
fn starts_tag(input: &str, index: usize) -> bool {
    input.as_bytes().get(index + 1).is_some_and(|byte| {
        byte.is_ascii_alphabetic() || *byte == b'/' || *byte == b'!' || *byte == b'?'
    })
}

/// Text elements and processing instructions hidden from `tl`.
#[derive(Default)]
struct ShieldedHtml {
    /// Placeholder -> original raw-text or RCDATA body.
    text_elements: HashMap<String, String>,
    /// Placeholder -> original processing-instruction text.
    pi_comments: HashMap<String, String>,
    /// Boundary comments standing in for dropped DOCTYPEs.
    ignored_comments: HashSet<String>,
}

/// Replace text-element bodies and processing instructions with internal
/// placeholders before `tl` sees them.
///
/// `<script>`/`<style>` are HTML raw-text elements and `title`/`textarea` are
/// RCDATA: their contents are not parsed as markup. libxml2's HTML parser also
/// exposes a processing instruction as a comment running to the first `>` (or
/// to the end of input). `tl` does not implement these rules and can corrupt
/// the tree, so shield them and restore the exact bytes during conversion. A
/// stray `<!DOCTYPE ...>` after content is dropped, as libxml2 does.
fn shield_html(input: &str) -> (Cow<'_, str>, ShieldedHtml) {
    if !input.match_indices('<').any(|(index, _)| {
        let rest = &input[index..];
        !starts_tag(input, index)
            || rest.starts_with("<?")
            || protected_tag_name(rest).is_some()
            || rest
                .get(..9)
                .is_some_and(|p| p.eq_ignore_ascii_case("<!DOCTYPE"))
    }) {
        return (Cow::Borrowed(input), ShieldedHtml::default());
    }

    let prefix = unique_prefix(input, "neomacs-internal-");
    let mut protected = String::with_capacity(input.len());
    let mut shielded = ShieldedHtml::default();
    let bytes = input.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let Some(offset) = input[index..].find('<') else {
            protected.push_str(&input[index..]);
            break;
        };
        let lt = index + offset;
        protected.push_str(&input[index..lt]);
        index = lt;
        let rest = &input[index..];

        if rest.starts_with("<!--") {
            if let Some(end) = rest.find("-->") {
                let end = index + end + 3;
                protected.push_str(&input[index..end]);
                index = end;
            } else {
                protected.push_str(rest);
                break;
            }
            continue;
        }

        if rest
            .get(..9)
            .is_some_and(|p| p.eq_ignore_ascii_case("<!DOCTYPE"))
        {
            // libxml2 drops DOCTYPEs anywhere, including after body content.
            // Keep a comment boundary so removing the declaration cannot join
            // the surrounding bytes into markup or an entity reference. The
            // comment is dropped during conversion.
            let token = format!("{prefix}doctype-{}", shielded.ignored_comments.len());
            protected.push_str("<!--");
            protected.push_str(&token);
            protected.push_str("-->");
            shielded.ignored_comments.insert(token);
            index += rest.find('>').map_or(rest.len(), |end| end + 1);
            continue;
        }

        if rest.starts_with("<?") {
            let (content, consumed) = match rest.find('>') {
                Some(end) => (&rest[1..end], end + 1),
                None => (&rest[1..], rest.len()),
            };
            let token = format!("{prefix}pi-{}", shielded.pi_comments.len());
            protected.push_str("<!--");
            protected.push_str(&token);
            protected.push_str("-->");
            shielded.pi_comments.insert(token, content.to_string());
            index += consumed;
            continue;
        }

        // A literal `<` that does not begin a tag (e.g. `a < b`) is text.
        if !starts_tag(input, index) {
            // Encode a literal `<` so tl keeps it as text instead of trying to
            // parse the following bytes as a tag. Conversion unescapes it.
            protected.push_str("&lt;");
            index += 1;
            continue;
        }

        if let Some(tag) = protected_tag_name(rest) {
            let Some(open_end) = opening_tag_end(rest) else {
                protected.push_str(rest);
                break;
            };
            let open_end = index + open_end;
            let after_open = &input[open_end + 1..];
            let (inner_len, close_len) = match closing_tag_start(after_open, tag) {
                Some(close_start) => {
                    let close_len = after_open[close_start..]
                        .find('>')
                        .map_or(after_open.len() - close_start, |gt| gt + 1);
                    (close_start, close_len)
                }
                None => (after_open.len(), 0),
            };
            protected.push_str(&input[index..=open_end]);
            let inner = &after_open[..inner_len];
            if !inner.is_empty() {
                let token = format!("{prefix}text-{}", shielded.text_elements.len());
                protected.push_str(&token);
                shielded.text_elements.insert(token, inner.to_string());
            }
            if close_len > 0 {
                protected.push_str(&after_open[inner_len..inner_len + close_len]);
            }
            index = open_end + 1 + inner_len + close_len;
            continue;
        }

        // Copy an ordinary tag through its terminator so `<script>` text in an
        // attribute value is not mistaken for a raw-text element.
        if let Some(end) = opening_tag_end(rest) {
            let end = index + end + 1;
            protected.push_str(&input[index..end]);
            index = end;
        } else {
            protected.push_str(rest);
            break;
        }
    }

    (Cow::Owned(protected), shielded)
}

/// Name of a text element whose body `tl` must not parse as markup.
///
/// `script`/`style` are raw-text elements and `title`/`textarea` are RCDATA.
/// Both are opaque to tag parsing; libxml2 resolves character references only
/// in RCDATA.
fn protected_tag_name(rest: &str) -> Option<&'static str> {
    let bytes = rest.as_bytes();
    for (name, tag) in [
        ("script", &b"script"[..]),
        ("style", &b"style"[..]),
        ("title", &b"title"[..]),
        ("textarea", &b"textarea"[..]),
    ] {
        let Some(candidate) = bytes.get(1..1 + tag.len()) else {
            continue;
        };
        if bytes[0] == b'<' && candidate.eq_ignore_ascii_case(tag) {
            let boundary = bytes.get(1 + tag.len()).copied();
            if boundary.is_some_and(|b| b == b'>' || b == b'/' || b.is_ascii_whitespace()) {
                return Some(name);
            }
        }
    }
    None
}

/// Raw source text between a raw-text element's start and end tags.
///
/// `<script>` and `<style>` are HTML raw-text elements: libxml2 exposes their
/// source bytes verbatim, including entity-like text and markup-looking code.
/// `tl`'s `inner_html` re-serializes its parsed children, which corrupts text
/// such as `if (a < b)` inside a script, so slice the original tag source
/// instead. The closing tag is matched case-insensitively at a tag-name
/// boundary, as the HTML parser does.
fn raw_text_content(element: &tl::HTMLTag<'_>, tag: &str) -> String {
    let raw = element.raw().as_utf8_str();
    let Some(open_end) = opening_tag_end(raw.as_ref()) else {
        return String::new();
    };
    let rest = &raw[open_end + 1..];
    let end = closing_tag_start(rest, tag).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// Index of the `>` closing a start tag, skipping quoted attribute values.
///
/// A quote starts a value only when it follows `=`, as in the HTML
/// attribute-value state. An apostrophe inside an unquoted value, such as
/// `data-x=don't`, is not a delimiter.
fn opening_tag_end(raw: &str) -> Option<usize> {
    let mut quote = None;
    let mut value_expected = false;
    for (index, byte) in raw.bytes().enumerate() {
        match quote {
            Some(q) if byte == q => quote = None,
            Some(_) => {}
            None => {
                if (byte == b'\'' || byte == b'"') && value_expected {
                    quote = Some(byte);
                } else if byte == b'>' {
                    return Some(index);
                }
                if !byte.is_ascii_whitespace() {
                    value_expected = byte == b'=';
                }
            }
        }
    }
    None
}

/// Index of a case-insensitive `</TAG` at a tag-name boundary in `text`.
fn closing_tag_start(text: &str, tag: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let tag = tag.as_bytes();
    if tag.is_empty() || bytes.len() < tag.len() + 2 {
        return None;
    }
    for start in 0..=bytes.len() - tag.len() - 2 {
        if bytes[start] != b'<' || bytes[start + 1] != b'/' {
            continue;
        }
        let name = &bytes[start + 2..start + 2 + tag.len()];
        if !name.eq_ignore_ascii_case(tag) {
            continue;
        }
        let boundary = bytes.get(start + 2 + tag.len()).copied();
        if boundary.is_some_and(|b| b == b'>' || b == b'/' || b.is_ascii_whitespace()) {
            return Some(start);
        }
    }
    None
}

fn convert_tl_attributes(attrs: &tl::Attributes) -> Vec<Value> {
    let mut result = Vec::new();
    for (key, val) in attrs.iter() {
        let val_str = val.unwrap_or_default();
        // Attribute reference recovery differs from text recovery when a
        // semicolon is omitted and the next byte is alphanumeric or `='.
        // libxml2 applies that context before exposing `property->content'.
        let val_str = htmlize::unescape_attribute(val_str.as_ref());
        result.push(Value::cons(
            Value::symbol(key.as_ref()),
            Value::string(val_str.as_ref()),
        ));
    }
    result
}

// ---------------------------------------------------------------------------
// Builtin functions
// ---------------------------------------------------------------------------

/// (libxml-available-p) → t
#[doc(hidden)]
pub fn builtin_libxml_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("libxml-available-p", &args, 0)?;
    Ok(Value::T)
}

/// (libxml-parse-html-region &optional START END BASE-URL DISCARD-COMMENTS)
#[doc(hidden)]
pub fn builtin_libxml_parse_html_region<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_max_args("libxml-parse-html-region", &args, 4)?;
    let (edit, _vars) = host.edit_and_vars();
    let Some((buffer_id, byte_range)) = validate_region_byte_bounds(&mut edit.buffers, &args)?
    else {
        return Ok(Value::NIL);
    };
    // GNU `parse_region` calls `validate_region` before checking BASE-URL.
    validate_base_url(&args)?;
    let discard_comments = args.get(3).is_some_and(|v| v.is_truthy());

    let bytes = read_region_bytes(&mut edit.buffers, buffer_id, byte_range)?;
    if bytes.is_empty() {
        return Ok(Value::NIL);
    }

    match parse_html_region(&bytes, discard_comments) {
        Some(val) => Ok(val),
        None => Ok(Value::NIL),
    }
}

/// (libxml-parse-xml-region &optional START END BASE-URL DISCARD-COMMENTS)
#[doc(hidden)]
pub fn builtin_libxml_parse_xml_region<H: EditHost>(host: &mut H, args: Vec<Value>) -> EvalResult {
    expect_max_args("libxml-parse-xml-region", &args, 4)?;
    let (edit, _vars) = host.edit_and_vars();
    let Some((buffer_id, byte_range)) = validate_region_byte_bounds(&mut edit.buffers, &args)?
    else {
        return Ok(Value::NIL);
    };
    // GNU `parse_region` calls `validate_region` before checking BASE-URL.
    validate_base_url(&args)?;
    let discard_comments = args.get(3).is_some_and(|v| v.is_truthy());

    let bytes = read_region_bytes(&mut edit.buffers, buffer_id, byte_range)?;
    if bytes.is_empty() {
        return Ok(Value::NIL);
    }

    match parse_xml_region(&bytes, discard_comments) {
        Some(val) => Ok(val),
        None => Ok(Value::NIL),
    }
}

#[cfg(test)]
#[path = "tests/xml_test.rs"]
mod tests;
