//! Typed reads of GNU's C-forwarded variables.
//!
//! GNU's `DEFVAR_*` variables live in C globals; `set_buffer_internal_2`
//! (src/buffer.c:2342ff) swaps a buffer's forwarded local bindings into those
//! globals, and a dynamic `let` writes the same storage. What a C reader sees
//! is therefore `find_symbol_value`, never the variable's default.
//!
//! [`ForwardedVariable`] wraps the symbol id and exposes only that visible
//! read, so the buffer-blind `VariableView::value` default read cannot be
//! reached for these variables by accident.

use neovm_value::intern::SymId;
use neovm_value::value::Value;

use crate::emacs_core::text_host::TextHost;

/// A GNU C-forwarded variable. The symbol id is private: the only read routes
/// are [`Self::visible_value`] and [`Self::is_truthy`].
#[derive(Clone, Copy)]
pub(crate) struct ForwardedVariable(SymId);

impl ForwardedVariable {
    /// GNU `find_symbol_value` for the wrapped variable: the current buffer's
    /// forwarded/local binding and the live dynamic binding.
    #[inline(always)]
    pub(crate) fn visible_value<H: TextHost>(self, host: &H) -> Option<Value> {
        host.forwarded_variable_value(self.0)
    }

    /// The truthiness of [`Self::visible_value`]; `None` and nil are false.
    #[inline(always)]
    pub(crate) fn is_truthy<H: TextHost>(self, host: &H) -> bool {
        self.visible_value(host)
            .is_some_and(|value| value.is_truthy())
    }
}

/// The audited GNU variables the text scanner/search code reads. Each name is
/// one GNU `DEFVAR_*` whose C storage `set_buffer_internal_2` updates.
#[derive(Clone, Copy)]
pub(crate) enum GnuForwardedVariable {
    /// `DEFVAR_BOOL` src/syntax.c:3764.
    ParseSexpIgnoreComments,
    /// `DEFVAR_BOOL` src/syntax.c:3767.
    ParseSexpLookupProperties,
    /// `DEFVAR_BOOL` + `Fmake_variable_buffer_local` src/syntax.c:3811/3815.
    CommentEndCanBeEscaped,
    /// `DEFVAR_INT` + `Fmake_variable_buffer_local` src/syntax.c:3773/3777.
    SyntaxPropertizeDone,
    /// `DEFVAR_LISP` src/syntax.c:3793.
    FindWordBoundaryFunctionTable,
    /// `DEFVAR_LISP` src/character.c:1138.
    CharScriptTable,
    /// `DEFVAR_LISP` src/category.c:442.
    WordCombiningCategories,
    /// `DEFVAR_LISP` src/category.c:487.
    WordSeparatingCategories,
}

impl GnuForwardedVariable {
    fn name(self) -> &'static str {
        match self {
            Self::ParseSexpIgnoreComments => "parse-sexp-ignore-comments",
            Self::ParseSexpLookupProperties => "parse-sexp-lookup-properties",
            Self::CommentEndCanBeEscaped => "comment-end-can-be-escaped",
            Self::SyntaxPropertizeDone => "syntax-propertize--done",
            Self::FindWordBoundaryFunctionTable => "find-word-boundary-function-table",
            Self::CharScriptTable => "char-script-table",
            Self::WordCombiningCategories => "word-combining-categories",
            Self::WordSeparatingCategories => "word-separating-categories",
        }
    }

    /// The typed handle for this GNU variable. One intern per variant.
    #[inline(always)]
    pub(crate) fn forwarded(self) -> ForwardedVariable {
        use std::sync::OnceLock;

        static PARSE_SEXP_IGNORE_COMMENTS: OnceLock<SymId> = OnceLock::new();
        static PARSE_SEXP_LOOKUP_PROPERTIES: OnceLock<SymId> = OnceLock::new();
        static COMMENT_END_CAN_BE_ESCAPED: OnceLock<SymId> = OnceLock::new();
        static SYNTAX_PROPERITIZE_DONE: OnceLock<SymId> = OnceLock::new();
        static FIND_WORD_BOUNDARY_FUNCTION_TABLE: OnceLock<SymId> = OnceLock::new();
        static CHAR_SCRIPT_TABLE: OnceLock<SymId> = OnceLock::new();
        static WORD_COMBINING_CATEGORIES: OnceLock<SymId> = OnceLock::new();
        static WORD_SEPARATING_CATEGORIES: OnceLock<SymId> = OnceLock::new();

        let name: &'static str = self.name();
        let id = match self {
            Self::ParseSexpIgnoreComments => {
                *PARSE_SEXP_IGNORE_COMMENTS.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::ParseSexpLookupProperties => {
                *PARSE_SEXP_LOOKUP_PROPERTIES.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::CommentEndCanBeEscaped => {
                *COMMENT_END_CAN_BE_ESCAPED.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::SyntaxPropertizeDone => {
                *SYNTAX_PROPERITIZE_DONE.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::FindWordBoundaryFunctionTable => {
                *FIND_WORD_BOUNDARY_FUNCTION_TABLE.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::CharScriptTable => {
                *CHAR_SCRIPT_TABLE.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::WordCombiningCategories => {
                *WORD_COMBINING_CATEGORIES.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::WordSeparatingCategories => {
                *WORD_SEPARATING_CATEGORIES.get_or_init(|| neovm_value::intern::intern(name))
            }
        };
        ForwardedVariable(id)
    }
}
