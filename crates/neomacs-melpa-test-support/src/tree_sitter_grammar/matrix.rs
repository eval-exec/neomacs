//! Pinned native grammars used by GNU Emacs 31.1's built-in modes.
//! Each fixture requires a real shared library, not a mocked parser.
use crate::EmacsRuntime;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum GrammarSourceDirectory {
    Root,
    Subdirectory(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct TreeSitterGrammarCase {
    pub language: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub source_directory: GrammarSourceDirectory,
    pub sample: &'static str,
    pub root_type: &'static str,
}

impl TreeSitterGrammarCase {
    fn source(self) -> super::GrammarSourceSpec<'static> {
        super::GrammarSourceSpec {
            language: self.language,
            repository: self.repository,
            revision: self.revision,
            source_directory: match self.source_directory {
                GrammarSourceDirectory::Root => None,
                GrammarSourceDirectory::Subdirectory(directory) => Some(directory),
            },
        }
    }

    pub fn prepare(self, gnu: &EmacsRuntime) -> Result<PathBuf, String> {
        let source = self.source();
        super::prepare_cached_tree_sitter_grammar_with_source_directory(
            gnu,
            source.language,
            source.repository,
            source.revision,
            source.source_directory,
        )
    }

    /// Prepare the grammar and expose its generated sources for compiler-backed
    /// fixtures. Cache layout remains owned by this preparation module.
    pub fn prepare_generated_source(self, gnu: &EmacsRuntime) -> Result<PathBuf, String> {
        self.prepare(gnu)?;
        let source = self.source();
        Ok(super::GrammarCachePaths::for_source(source)
            .checkout
            .join(source.source_directory.unwrap_or("src")))
    }
}

/// A single declaration generates the pinned catalog and its per-language
/// oracle registrations, so adding a grammar cannot silently omit its test.
#[macro_export]
macro_rules! gnu31_tree_sitter_grammar_cases {
    ($consumer:ident) => {
        $consumer! {
            bash => $crate::TreeSitterGrammarCase {
                language: "bash",
                repository: "https://github.com/tree-sitter/tree-sitter-bash",
                revision: "a06c2e4415e9bc0346c6b86d401879ffb44058f7",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "echo hello\n",
                root_type: "program",
            },
            c => $crate::TreeSitterGrammarCase {
                language: "c",
                repository: "https://github.com/tree-sitter/tree-sitter-c",
                revision: "b780e47fc780ddc8da13afa35a3f4ed5c157823d",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "int main(void) { return 0; }\n",
                root_type: "translation_unit",
            },
            c_sharp => $crate::TreeSitterGrammarCase {
                language: "c-sharp",
                repository: "https://github.com/tree-sitter/tree-sitter-c-sharp",
                revision: "8c0abe0b84a3681d3e6852ce2e2a1eecb4f731c5",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {}\n",
                root_type: "compilation_unit",
            },
            cmake => $crate::TreeSitterGrammarCase {
                language: "cmake",
                repository: "https://github.com/uyha/tree-sitter-cmake",
                revision: "58993af75218bc99a1f5a04c832a5937e7c422cb",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "set(NAME hello)\n",
                root_type: "source_file",
            },
            cpp => $crate::TreeSitterGrammarCase {
                language: "cpp",
                repository: "https://github.com/tree-sitter/tree-sitter-cpp",
                revision: "c009222808634c1014f82438d4883753516a2c24",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {};\n",
                root_type: "translation_unit",
            },
            css => $crate::TreeSitterGrammarCase {
                language: "css",
                repository: "https://github.com/tree-sitter/tree-sitter-css",
                revision: "dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "body { color: red; }\n",
                root_type: "stylesheet",
            },
            dockerfile => $crate::TreeSitterGrammarCase {
                language: "dockerfile",
                repository: "https://github.com/camdencheek/tree-sitter-dockerfile",
                revision: "971acdd908568b4531b0ba28a445bf0bb720aba5",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "FROM alpine\n",
                root_type: "source_file",
            },
            elixir => $crate::TreeSitterGrammarCase {
                language: "elixir",
                repository: "https://github.com/elixir-lang/tree-sitter-elixir",
                revision: "4b0c7118760af58a2e7081bbc8396e136f820b37",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "source",
            },
            go => $crate::TreeSitterGrammarCase {
                language: "go",
                repository: "https://github.com/tree-sitter/tree-sitter-go",
                revision: "2346a3ab1bb3857b48b29d779a1ef9799a248cd7",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "package main\nfunc main() {}\n",
                root_type: "source_file",
            },
            gomod => $crate::TreeSitterGrammarCase {
                language: "gomod",
                repository: "https://github.com/camdencheek/tree-sitter-go-mod",
                revision: "2e886870578eeba1927a2dc4bd2e2b3f598c5f9a",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "module example.com/hello\n",
                root_type: "source_file",
            },
            gowork => $crate::TreeSitterGrammarCase {
                language: "gowork",
                repository: "https://github.com/omertuc/tree-sitter-go-work",
                revision: "949a8a470559543857a62102c84700d291fc984c",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "go 1.20\nuse ./hello\n",
                root_type: "source_file",
            },
            heex => $crate::TreeSitterGrammarCase {
                language: "heex",
                repository: "https://github.com/phoenixframework/tree-sitter-heex",
                revision: "5842537f734d7c12685bf27d6005313e3e5a47a0",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "<div>hello</div>\n",
                root_type: "fragment",
            },
            html => $crate::TreeSitterGrammarCase {
                language: "html",
                repository: "https://github.com/tree-sitter/tree-sitter-html",
                revision: "73a3947324f6efddf9e17c0ea58d454843590cc0",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "<div>hello</div>\n",
                root_type: "document",
            },
            java => $crate::TreeSitterGrammarCase {
                language: "java",
                repository: "https://github.com/tree-sitter/tree-sitter-java",
                revision: "e10607b45ff745f5f876bfa3e94fbcc6b44bdc11",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {}\n",
                root_type: "program",
            },
            javascript => $crate::TreeSitterGrammarCase {
                language: "javascript",
                repository: "https://github.com/tree-sitter/tree-sitter-javascript",
                revision: "58404d8cf191d69f2674a8fd507bd5776f46cb11",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "const name = \"hello\";\n",
                root_type: "program",
            },
            jsdoc => $crate::TreeSitterGrammarCase {
                language: "jsdoc",
                repository: "https://github.com/tree-sitter/tree-sitter-jsdoc",
                revision: "658d18dcdddb75c760363faa4963427a7c6b52db",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "/** This is a description of the foo function. */",
                root_type: "document",
            },
            json => $crate::TreeSitterGrammarCase {
                language: "json",
                repository: "https://github.com/tree-sitter/tree-sitter-json",
                revision: "254c42a6476413b776221e03982ac8ae159eeb72",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "{\"name\":\"hello\"}\n",
                root_type: "document",
            },
            lua => $crate::TreeSitterGrammarCase {
                language: "lua",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-lua",
                revision: "10fe0054734eec83049514ea2e718b2a56acd0c9",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "local name = \"hello\"\n",
                root_type: "chunk",
            },
            markdown => $crate::TreeSitterGrammarCase {
                language: "markdown",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
                revision: "a0a00f817d02412bd92c54d316f164d827b57b5c",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tree-sitter-markdown/src"),
                sample: "# hello\n",
                root_type: "document",
            },
            markdown_inline => $crate::TreeSitterGrammarCase {
                language: "markdown-inline",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
                revision: "a0a00f817d02412bd92c54d316f164d827b57b5c",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tree-sitter-markdown-inline/src"),
                sample: "hello **world**",
                root_type: "inline",
            },
            php => $crate::TreeSitterGrammarCase {
                language: "php",
                repository: "https://github.com/tree-sitter/tree-sitter-php",
                revision: "58d5643086ae60e93a3746cca648363ee97c8761",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("php/src"),
                sample: "<?php $name = \"hello\";\n",
                root_type: "program",
            },
            python => $crate::TreeSitterGrammarCase {
                language: "python",
                repository: "https://github.com/tree-sitter/tree-sitter-python",
                revision: "26855eabccb19c6abf499fbc5b8dc7cc9ab8bc64",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "module",
            },
            ruby => $crate::TreeSitterGrammarCase {
                language: "ruby",
                repository: "https://github.com/tree-sitter/tree-sitter-ruby",
                revision: "ad907a69da0c8a4f7a943a7fe012712208da6dee",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "program",
            },
            rust => $crate::TreeSitterGrammarCase {
                language: "rust",
                repository: "https://github.com/tree-sitter/tree-sitter-rust",
                revision: "77a3747266f4d621d0757825e6b11edcbf991ca5",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "fn main() {}\n",
                root_type: "source_file",
            },
            toml => $crate::TreeSitterGrammarCase {
                language: "toml",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-toml",
                revision: "64b56832c2cffe41758f28e05c756a3a98d16f41",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "document",
            },
            tsx => $crate::TreeSitterGrammarCase {
                language: "tsx",
                repository: "https://github.com/tree-sitter/tree-sitter-typescript",
                revision: "75b3874edb2dc714fb1fd77a32013d0f8699989f",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tsx/src"),
                sample: "const name = <div>hello</div>;\n",
                root_type: "program",
            },
            typescript => $crate::TreeSitterGrammarCase {
                language: "typescript",
                repository: "https://github.com/tree-sitter/tree-sitter-typescript",
                revision: "75b3874edb2dc714fb1fd77a32013d0f8699989f",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("typescript/src"),
                sample: "const name: string = \"hello\";\n",
                root_type: "program",
            },
            yaml => $crate::TreeSitterGrammarCase {
                language: "yaml",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-yaml",
                revision: "a1c4812a73ec5e089de8e441fdea3a921e8d5079",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name: hello\n",
                root_type: "stream",
            },
        }
    };
}

macro_rules! define_grammar_catalog {
    ($($test:ident => $case:expr),* $(,)?) => {
        pub const GNU31_TREE_SITTER_GRAMMARS: &[TreeSitterGrammarCase] = &[$($case),*];
    };
}
crate::gnu31_tree_sitter_grammar_cases!(define_grammar_catalog);
