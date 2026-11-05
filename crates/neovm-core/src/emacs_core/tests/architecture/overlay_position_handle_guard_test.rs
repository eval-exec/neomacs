//! The lower read handle has no public mint. Its crate-private construction is
//! reserved for Buffer's existing attachment operation.
use std::path::Path;
use syn::visit::{self, Visit};

use super::super::architecture_production_source_roots::production_source_files;
use super::{code_only, test_gated_ranges};

const OWNER: &str = "src/buffer/overlay_index.rs";
const MINT: &str = "set_overlay_position_handle";

#[derive(Default)]
struct MintCalls {
    function: String,
    calls: Vec<(String, usize)>,
}
impl<'a> Visit<'a> for MintCalls {
    fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
        let previous = std::mem::replace(&mut self.function, node.sig.ident.to_string());
        visit::visit_item_fn(self, node);
        self.function = previous;
    }
    fn visit_impl_item_fn(&mut self, node: &'a syn::ImplItemFn) {
        let previous = std::mem::replace(&mut self.function, node.sig.ident.to_string());
        visit::visit_impl_item_fn(self, node);
        self.function = previous;
    }
    fn visit_expr_call(&mut self, node: &'a syn::ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref() {
            let mut segments = path.path.segments.iter().rev();
            if segments.next().is_some_and(|s| s.ident == "new")
                && segments
                    .next()
                    .is_some_and(|s| s.ident == "OverlayPositionHandle")
            {
                self.calls.push((
                    self.function.clone(),
                    path.path.segments.last().unwrap().ident.span().start().line,
                ));
            }
        }
        visit::visit_expr_call(self, node);
    }
}
fn production_calls(source: &str) -> Vec<(String, usize)> {
    let tree = syn::parse_file(source).expect("parse production source");
    let mut scanner = MintCalls::default();
    scanner.visit_file(&tree);
    let code = code_only(source);
    let ranges = test_gated_ranges(&code);
    let mut offsets = vec![0];
    offsets.extend(source.match_indices('\n').map(|(offset, _)| offset + 1));
    scanner.calls.retain(|(_, line)| {
        let offset = offsets[line - 1];
        !ranges
            .iter()
            .any(|(start, end)| *start <= offset && offset <= *end)
    });
    scanner.calls
}
fn permitted(path: &Path, function: &str) -> bool {
    path == Path::new(OWNER) && function == MINT
}
#[test]
fn overlay_position_handle_mint_is_only_the_buffer_attachment() {
    let mut seen = 0;
    for file in production_source_files() {
        if file.relative.split('/').any(|part| part == "tests") {
            continue;
        }
        let relative = file.legacy();
        let source = std::fs::read_to_string(&file.path).unwrap();
        for (function, line) in production_calls(&source) {
            assert!(
                permitted(Path::new(&relative), &function),
                "unapproved overlay handle mint: {relative}:{line} {function}"
            );
            seen += 1;
        }
    }
    assert_eq!(seen, 1, "guard must bind the actual sole attachment mint");
}
#[test]
fn overlay_position_handle_guard_refuses_other_files_and_functions() {
    assert!(permitted(Path::new(OWNER), MINT));
    assert!(!permitted(Path::new("src/buffer/overlay.rs"), MINT));
    assert!(!permitted(Path::new(OWNER), "another_attachment"));
    assert_eq!(
        production_calls("fn wrong() { crate::value::OverlayPositionHandle::new(o, s); }").len(),
        1
    );
    assert_eq!(
        production_calls("#[cfg(test)] fn fixture() { OverlayPositionHandle::new(o, s); }").len(),
        0
    );
    assert_eq!(
        production_calls("// OverlayPositionHandle::new(o, s);\nfn wrong() {} ").len(),
        0
    );
}
