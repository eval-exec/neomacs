//! XML and HTML parsing support, matching GNU Emacs's xml.c.
//!
//! Provides real implementations for:
//! - `libxml-parse-html-region` — HTML parsing via `tl` crate
//! - `libxml-parse-xml-region` — XML parsing via `quick-xml` crate
//! - `libxml-available-p` — feature availability probe

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::buffer::EmacsByteRange;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::value::ValueKind;

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
    ctx: &mut super::eval::Context,
    args: &[Value],
) -> Result<Option<(crate::buffer::BufferId, EmacsByteRange)>, Flow> {
    let Some(buf) = ctx.buffers.current_buffer() else {
        return Ok(None);
    };

    let region = super::position::LispRegionArgs::from_optional_values(
        &ctx.buffers,
        args.first().copied(),
        args.get(1).copied(),
        buf.point_min_lisp_char_pos(),
        buf.point_max_lisp_char_pos(),
    )?;
    let byte_range = region.accessible_byte_range(buf)?;

    Ok(Some((buf.id, byte_range)))
}

fn read_region_bytes(
    ctx: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<Vec<u8>, Flow> {
    let bytes =
        super::fns::read_buffer_region_bytes_in_manager(&ctx.buffers, buffer_id, byte_range)?;
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
        stack: &mut Vec<(String, Vec<Value>, Vec<Value>)>,
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
    let dom = tl::parse(&input, tl::ParserOptions::default()).ok()?;
    let parser = dom.parser();
    let handles = dom.children();

    let mut nodes: Vec<TopNode> = Vec::new();
    for handle in handles {
        let node = handle.get(parser)?;
        if let Some(top) = convert_tl_top_node(node, parser, discard_comments) {
            nodes.push(top);
        }
    }

    if nodes.is_empty() {
        return Some(Value::NIL);
    }

    // libxml2's HTML parser performs full HTML5 tree construction, implicitly
    // creating the <html>/<head>/<body> document structure. The `tl` crate
    // returns the fragment verbatim, so reconstruct that wrapper here.
    wrap_html_document(nodes)
}

/// Reconstruct libxml2's implicit `<html>`/`<head>`/`<body>` document structure
/// around a parsed HTML fragment.
///
/// Faithfully covered (verified against GNU): bare fragments, plain text,
/// `head`-routed elements (`title`/`meta`/`link`/`base`/`style`/`script`),
/// an explicit `<head>`, and the head→body transition once body content
/// appears. A fragment that already has a single `<html>` root, or that carries
/// top-level comments, is left as libxml2's lighter handling would *not*
/// rewrite it the same way, so those keep the pre-existing pass-through /
/// `(top ...)` behavior rather than risk a wrong wrap.
fn wrap_html_document(nodes: Vec<TopNode>) -> Option<Value> {
    let has_html_root = nodes.len() == 1 && nodes[0].tag.as_deref() == Some("html");
    let has_top_comment = nodes
        .iter()
        .any(|n| n.tag.is_none() && is_comment_node(&n.value));

    if has_html_root || has_top_comment {
        // Risky to rewrite faithfully — preserve previous behavior.
        let values: Vec<Value> = nodes.into_iter().map(|n| n.value).collect();
        if values.len() == 1 {
            return Some(values.into_iter().next().unwrap());
        }
        return Some(Value::list(
            std::iter::once(Value::symbol("top"))
                .chain(std::iter::once(Value::NIL))
                .chain(values)
                .collect(),
        ));
    }

    // Partition top-level nodes into the head and body sections, honoring the
    // head→body transition: once body content appears, later head-type
    // elements stay in the body (matching libxml2).
    let mut head: Vec<Value> = Vec::new();
    let mut body: Vec<Value> = Vec::new();
    let mut seen_body_content = false;

    for node in nodes {
        match node.tag.as_deref() {
            Some("head") => {
                // Unwrap an explicit <head>, merging its children into head.
                head.extend(element_children(&node.value));
            }
            Some("body") => {
                seen_body_content = true;
                body.extend(element_children(&node.value));
            }
            Some(tag) if !seen_body_content && is_head_element(tag) => {
                head.push(node.value);
            }
            _ => {
                seen_body_content = true;
                body.push(node.value);
            }
        }
    }

    // (html nil [head nil HEAD...] (body nil BODY...))
    let has_head = !head.is_empty();
    let has_body = !body.is_empty();
    let mut html_children: Vec<Value> = vec![Value::symbol("html"), Value::NIL];
    if has_head {
        let mut head_node = vec![Value::symbol("head"), Value::NIL];
        head_node.extend(head);
        html_children.push(Value::list(head_node));
    }
    // libxml2 omits <body> only when there is exclusively head content.
    if has_body || !has_head {
        let mut body_node = vec![Value::symbol("body"), Value::NIL];
        body_node.extend(body);
        html_children.push(Value::list(body_node));
    }
    Some(Value::list(html_children))
}

/// Convert a top-level `tl` node, capturing its element tag (lowercased) so the
/// HTML document wrapper can classify it. Returns `None` for nodes that convert
/// to nothing (e.g. blank text or a discarded comment).
fn convert_tl_top_node(
    node: &tl::Node,
    parser: &tl::Parser,
    discard_comments: bool,
) -> Option<TopNode> {
    let value = convert_tl_node(node, parser, discard_comments)?;
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

/// Is `value` a `(comment ...)` element node?
fn is_comment_node(value: &Value) -> bool {
    super::value::list_to_vec(value)
        .and_then(|items| items.first().copied())
        .and_then(|head| head.as_symbol_name())
        .is_some_and(|name| name == "comment")
}

/// Return the children of an element node `(tag attrs child...)` (i.e. drop the
/// leading tag symbol and attribute list).
fn element_children(value: &Value) -> Vec<Value> {
    match super::value::list_to_vec(value) {
        Some(items) if items.len() > 2 => items[2..].to_vec(),
        _ => Vec::new(),
    }
}

fn convert_tl_node(node: &tl::Node, parser: &tl::Parser, discard_comments: bool) -> Option<Value> {
    match node {
        tl::Node::Raw(raw) => {
            let text = raw.as_utf8_str();
            if text.trim().is_empty() {
                None
            } else {
                // GNU's libxml2 parser resolves HTML character references
                // before `make_dom' copies an XML_TEXT_NODE into Lisp.  `tl'
                // deliberately exposes source bytes, so normalize them at
                // this adapter boundary using the HTML text context.
                let text = htmlize::unescape(text.as_ref());
                Some(Value::string(text.as_ref()))
            }
        }
        tl::Node::Comment(comment) => {
            if discard_comments {
                None
            } else {
                let text = comment.as_utf8_str();
                Some(Value::list(vec![
                    Value::symbol("comment"),
                    Value::NIL,
                    Value::string(text.as_ref()),
                ]))
            }
        }
        tl::Node::Tag(element) => {
            // The `tl` crate leaves the trailing `/` of an XHTML-style
            // self-closing tag (`<hr/>`, `<rect/>`) in the tag name.
            // libxml2's HTML parser never does that, so strip it to match GNU.
            let raw_tag = element.name().as_utf8_str();
            let tag = raw_tag.strip_suffix('/').unwrap_or(raw_tag.as_ref());
            let attrs = convert_tl_attributes(element.attributes());
            let children = element.children();
            let children_handles = children.top();
            let mut child_values = Vec::new();
            for handle in children_handles.iter() {
                let child_node = handle.get(parser)?;
                if let Some(val) = convert_tl_node(child_node, parser, discard_comments) {
                    child_values.push(val);
                }
            }
            Some(make_element_node(tag, attrs, child_values))
        }
    }
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
pub(crate) fn builtin_libxml_available_p(args: Vec<Value>) -> EvalResult {
    super::builtins::expect_args("libxml-available-p", &args, 0)?;
    Ok(Value::T)
}

/// (libxml-parse-html-region &optional START END BASE-URL DISCARD-COMMENTS)
pub(crate) fn builtin_libxml_parse_html_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::builtins::expect_max_args("libxml-parse-html-region", &args, 4)?;
    let Some((buffer_id, byte_range)) = validate_region_byte_bounds(ctx, &args)? else {
        return Ok(Value::NIL);
    };
    // GNU `parse_region` calls `validate_region` before checking BASE-URL.
    validate_base_url(&args)?;
    let discard_comments = args.get(3).is_some_and(|v| v.is_truthy());

    let bytes = read_region_bytes(ctx, buffer_id, byte_range)?;
    if bytes.is_empty() {
        return Ok(Value::NIL);
    }

    match parse_html_region(&bytes, discard_comments) {
        Some(val) => Ok(val),
        None => Ok(Value::NIL),
    }
}

/// (libxml-parse-xml-region &optional START END BASE-URL DISCARD-COMMENTS)
pub(crate) fn builtin_libxml_parse_xml_region(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::builtins::expect_max_args("libxml-parse-xml-region", &args, 4)?;
    let Some((buffer_id, byte_range)) = validate_region_byte_bounds(ctx, &args)? else {
        return Ok(Value::NIL);
    };
    // GNU `parse_region` calls `validate_region` before checking BASE-URL.
    validate_base_url(&args)?;
    let discard_comments = args.get(3).is_some_and(|v| v.is_truthy());

    let bytes = read_region_bytes(ctx, buffer_id, byte_range)?;
    if bytes.is_empty() {
        return Ok(Value::NIL);
    }

    match parse_xml_region(&bytes, discard_comments) {
        Some(val) => Ok(val),
        None => Ok(Value::NIL),
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
