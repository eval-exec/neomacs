//! Render Brendan-Gregg folded stacks into an SVG flamegraph via `inferno`.
//!
//! Folded format is one line per collapsed stack: `frameA;frameB;frameC <count>`.
//! This is the human-facing projection of a Lisp CPU capture; the same folded
//! text also feeds speedscope and the ranked-JSON `/report` parser.

/// Render folded stacks to a self-contained SVG string.
///
/// Returns a small placeholder SVG (not an error) when the capture is empty —
/// e.g. profiling an idle editor yields no samples, which is a valid result a
/// browser should still be able to display.
pub fn folded_to_svg(folded: &str, title: &str) -> Result<String, String> {
    if folded.trim().is_empty() {
        return Ok(empty_svg(title));
    }
    let mut opts = inferno::flamegraph::Options::default();
    opts.title = title.to_string();
    let mut out: Vec<u8> = Vec::new();
    inferno::flamegraph::from_lines(&mut opts, folded.lines(), &mut out)
        .map_err(|e| format!("flamegraph render failed: {e}"))?;
    String::from_utf8(out).map_err(|e| format!("flamegraph produced non-utf8 svg: {e}"))
}

fn empty_svg(title: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"600\" height=\"64\">\
<text x=\"12\" y=\"38\" font-family=\"monospace\" font-size=\"14\">\
{title}: no samples captured (idle window)</text></svg>"
    )
}
