//! Guards the rule that a transported position names its presentation.
//!
//! Coordinates leaving the render thread are only meaningful next to the
//! presentation they were resolved against. A pointer position on the root
//! surface and a position in the presentation being drawn are the same pixel
//! while the panes are settled and different pixels for the whole length of a
//! `split-window` morph, so an `(x, y)` with nothing beside it cannot be
//! interpreted correctly by whoever receives it.
//!
//! `InputEvent::PresentedPointer` gets this right: it carries `presentation`
//! alongside the pair. `InputEvent::FileDrop` did not — it carried the
//! window's raw `mouse_pos`, which nothing read, which is the only reason it
//! was not already wrong. This test is what stops the next variant from
//! quietly acquiring a bare pair.

use std::path::PathBuf;

/// Every `InputEvent` variant with a raw coordinate pair and no presentation,
/// as `variant` -> why the pair is interpretable without one.
///
/// Empty on purpose. Filling it in is a claim that the receiver can tell which
/// composition the numbers describe by some other means; if that is hard to
/// argue, the variant should carry the presentation or drop the pair.
fn allowlist() -> &'static [(&'static str, &'static str)] {
    &[]
}

fn input_event_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/thread_comm.rs");
    std::fs::read_to_string(path).expect("thread_comm.rs is part of this crate")
}

/// `(variant name, field block)` for every variant of `pub enum InputEvent`.
///
/// Deliberately a text scan rather than a match over the enum: a match would
/// name today's variants and go on compiling when a new one is added, which is
/// the case this guard exists for.
fn input_event_variants(source: &str) -> Vec<(String, String)> {
    let start = source
        .find("pub enum InputEvent {")
        .expect("the transport enum is declared here");
    let body_start = start + "pub enum InputEvent {".len();
    let bytes = source.as_bytes();
    let mut depth = 1usize;
    let mut end = body_start;
    while end < bytes.len() && depth > 0 {
        match bytes[end] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        end += 1;
    }
    let body = &source[body_start..end - 1];

    // `index` only ever advances by a whole character, by an ASCII
    // identifier's length, or to just past an ASCII `}`, so it always lands on
    // a character boundary. Stepping a byte at a time would not: these doc
    // comments contain em dashes, and slicing into one panics.
    let mut variants = Vec::new();
    let mut index = 0usize;
    let mut pending_name: Option<String> = None;
    let body_bytes = body.as_bytes();
    while let Some(character) = body[index..].chars().next() {
        match character {
            '{' if pending_name.is_some() => {
                let field_start = index + 1;
                let mut depth = 1usize;
                let mut cursor = field_start;
                while cursor < body_bytes.len() && depth > 0 {
                    match body_bytes[cursor] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    cursor += 1;
                }
                variants.push((
                    pending_name.take().expect("checked above"),
                    body[field_start..cursor - 1].to_string(),
                ));
                index = cursor;
            }
            ',' => {
                pending_name = None;
                index += 1;
            }
            _ => {
                let name: String = body[index..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    index += name.len();
                    pending_name = Some(name);
                } else {
                    index += character.len_utf8();
                }
            }
        }
    }
    variants
}

#[test]
fn every_transported_position_names_the_presentation_it_was_resolved_against() {
    // If this fails, a variant is handing the evaluator two numbers with no
    // way to say which composition they describe. The evaluator cannot detect
    // that: the numbers look right on every settled frame, which is every
    // frame a developer is likely to test by hand.
    let source = input_event_source();
    let mut unexplained: Vec<String> = input_event_variants(&source)
        .into_iter()
        .filter(|(_, fields)| fields.contains("x: f32") && fields.contains("y: f32"))
        .filter(|(_, fields)| !fields.contains("presentation"))
        .map(|(name, _)| name)
        .collect();
    unexplained.retain(|name| !allowlist().iter().any(|(allowed, _)| allowed == name));

    assert!(
        unexplained.is_empty(),
        "these InputEvent variants carry a raw coordinate pair with no presentation: \
         {unexplained:?}. Resolve the position against the displayed presentation the way \
         PresentedPointer does, drop the pair, or add an entry to this test's allowlist.",
    );
}

#[test]
fn the_guard_still_finds_the_positions_that_do_name_a_presentation() {
    // Without this the scan above is indistinguishable from one that fails to
    // parse the enum at all and therefore inspects nothing.
    let source = input_event_source();
    let variants = input_event_variants(&source);

    let positioned: Vec<&String> = variants
        .iter()
        .filter(|(_, fields)| fields.contains("x: f32") && fields.contains("y: f32"))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        positioned,
        vec!["PresentedPointer"],
        "PresentedPointer is the transport's one positioned event; if that changed, \
         the new one has to be checked too"
    );
}
