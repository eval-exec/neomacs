//! Pinned native grammars used by GNU Emacs 31.1's built-in modes.
//! Each fixture requires a real shared library, not a mocked parser.
//! Generated ABI 13/14 keeps these fixtures loadable by both Ubuntu CI
//! (tree-sitter 0.20) and newer tree-sitter runtimes used by developers.
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
                revision: "5df623f1c50bb0cce9b9ae85c48ce90459349730",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "echo hello\n",
                root_type: "program",
            },
            c => $crate::TreeSitterGrammarCase {
                language: "c",
                repository: "https://github.com/tree-sitter/tree-sitter-c",
                revision: "3aa2995549d5d8b26928e8d3fa2770fd4327414e",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "int main(void) { return 0; }\n",
                root_type: "translation_unit",
            },
            c_sharp => $crate::TreeSitterGrammarCase {
                language: "c-sharp",
                repository: "https://github.com/tree-sitter/tree-sitter-c-sharp",
                revision: "285c993f01d9955932b45a6192055003aa70a570",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {}\n",
                root_type: "compilation_unit",
            },
            cmake => $crate::TreeSitterGrammarCase {
                language: "cmake",
                repository: "https://github.com/uyha/tree-sitter-cmake",
                revision: "f2569dd1fce0f252097a25bcbcb9ed8898840310",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "set(NAME hello)\n",
                root_type: "source_file",
            },
            cpp => $crate::TreeSitterGrammarCase {
                language: "cpp",
                repository: "https://github.com/tree-sitter/tree-sitter-cpp",
                revision: "464e8d9093f9308922cb1eea8fc250c5f5983e9a",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {};\n",
                root_type: "translation_unit",
            },
            css => $crate::TreeSitterGrammarCase {
                language: "css",
                repository: "https://github.com/tree-sitter/tree-sitter-css",
                revision: "185475a45c413e9294e5fd5c4165df9ca1eb05dc",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "body { color: red; }\n",
                root_type: "stylesheet",
            },
            dockerfile => $crate::TreeSitterGrammarCase {
                language: "dockerfile",
                repository: "https://github.com/camdencheek/tree-sitter-dockerfile",
                revision: "439c3e7b8a9bfdbf1f7d7c2beaae4173dc484cbf",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "FROM alpine\n",
                root_type: "source_file",
            },
            elixir => $crate::TreeSitterGrammarCase {
                language: "elixir",
                repository: "https://github.com/elixir-lang/tree-sitter-elixir",
                revision: "388e4d085388f792f452a059ad9c712479571808",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "source",
            },
            go => $crate::TreeSitterGrammarCase {
                language: "go",
                repository: "https://github.com/tree-sitter/tree-sitter-go",
                revision: "12fe553fdaaa7449f764bc876fd777704d4fb752",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "package main\nfunc main() {}\n",
                root_type: "source_file",
            },
            gomod => $crate::TreeSitterGrammarCase {
                language: "gomod",
                repository: "https://github.com/camdencheek/tree-sitter-go-mod",
                revision: "a9aea5e358cde4d0f8ff20b7bc4fa311e359c7ca",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "module example.com/hello\n",
                root_type: "source_file",
            },
            gowork => $crate::TreeSitterGrammarCase {
                language: "gowork",
                repository: "https://github.com/omertuc/tree-sitter-go-work",
                revision: "67f23e57b990440aae78cce03244a94bacc377b5",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "go 1.20\nuse ./hello\n",
                root_type: "source_file",
            },
            heex => $crate::TreeSitterGrammarCase {
                language: "heex",
                repository: "https://github.com/phoenixframework/tree-sitter-heex",
                revision: "f3d55855935f055d2583485d064be259d9ff053d",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "<div>hello</div>\n",
                root_type: "fragment",
            },
            html => $crate::TreeSitterGrammarCase {
                language: "html",
                repository: "https://github.com/tree-sitter/tree-sitter-html",
                revision: "b30e4aea714d39a051353b5dac074c1806512047",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "<div>hello</div>\n",
                root_type: "document",
            },
            java => $crate::TreeSitterGrammarCase {
                language: "java",
                repository: "https://github.com/tree-sitter/tree-sitter-java",
                revision: "de816732671830873ee52a91998a0febb603e012",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {}\n",
                root_type: "program",
            },
            javascript => $crate::TreeSitterGrammarCase {
                language: "javascript",
                repository: "https://github.com/tree-sitter/tree-sitter-javascript",
                revision: "a2ef0796e8b9db9107c9ad210b3121b8ad918d5e",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "const name = \"hello\";\n",
                root_type: "program",
            },
            jsdoc => $crate::TreeSitterGrammarCase {
                language: "jsdoc",
                repository: "https://github.com/tree-sitter/tree-sitter-jsdoc",
                revision: "0c5d665d04bc4ab5efda348bae3e7e34f20f264a",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "/** This is a description of the foo function. */",
                root_type: "document",
            },
            json => $crate::TreeSitterGrammarCase {
                language: "json",
                repository: "https://github.com/tree-sitter/tree-sitter-json",
                revision: "e2a0ad1b38af1d2affec29e66dbe63f214f0e531",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "{\"name\":\"hello\"}\n",
                root_type: "document",
            },
            lua => $crate::TreeSitterGrammarCase {
                language: "lua",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-lua",
                revision: "99fc677e6971c425e8d407f59c77ab897e585c92",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "local name = \"hello\"\n",
                root_type: "chunk",
            },
            markdown => $crate::TreeSitterGrammarCase {
                language: "markdown",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
                revision: "5cdc549ab8f461aff876c5be9741027189299cec",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tree-sitter-markdown/src"),
                sample: "# hello\n",
                root_type: "document",
            },
            markdown_inline => $crate::TreeSitterGrammarCase {
                language: "markdown-inline",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
                revision: "e70017dcf8caad949e41425e7191c3e82127b825",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tree-sitter-markdown-inline/src"),
                sample: "hello **world**",
                root_type: "inline",
            },
            php => $crate::TreeSitterGrammarCase {
                language: "php",
                repository: "https://github.com/tree-sitter/tree-sitter-php",
                revision: "11d2b8b272690ef48018afe9e651c977bfb061f2",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("php/src"),
                sample: "<?php $name = \"hello\";\n",
                root_type: "program",
            },
            python => $crate::TreeSitterGrammarCase {
                language: "python",
                repository: "https://github.com/tree-sitter/tree-sitter-python",
                revision: "e6226391f67a007b11012aa394ffd3dca399062c",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "module",
            },
            ruby => $crate::TreeSitterGrammarCase {
                language: "ruby",
                repository: "https://github.com/tree-sitter/tree-sitter-ruby",
                revision: "557ec01403bd1fcfea50f1805641ca773cbaaa42",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "program",
            },
            rust => $crate::TreeSitterGrammarCase {
                language: "rust",
                repository: "https://github.com/tree-sitter/tree-sitter-rust",
                revision: "c447dcce961ac438aaeaf117347749fe7d1e8365",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "fn main() {}\n",
                root_type: "source_file",
            },
            toml => $crate::TreeSitterGrammarCase {
                language: "toml",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-toml",
                revision: "d7d3ba873f0d22aef59aa36bb0532936fc670b7d",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "document",
            },
            tsx => $crate::TreeSitterGrammarCase {
                language: "tsx",
                repository: "https://github.com/tree-sitter/tree-sitter-typescript",
                revision: "14af1670c16cab8422d2a504dc4e5961293934e3",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tsx/src"),
                sample: "const name = <div>hello</div>;\n",
                root_type: "program",
            },
            typescript => $crate::TreeSitterGrammarCase {
                language: "typescript",
                repository: "https://github.com/tree-sitter/tree-sitter-typescript",
                revision: "14af1670c16cab8422d2a504dc4e5961293934e3",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("typescript/src"),
                sample: "const name: string = \"hello\";\n",
                root_type: "program",
            },
            yaml => $crate::TreeSitterGrammarCase {
                language: "yaml",
                repository: "https://github.com/tree-sitter-grammars/tree-sitter-yaml",
                revision: "1608168bb28493bf108460a0e5a6023a86dca738",
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
