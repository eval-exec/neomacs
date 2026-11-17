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
    /// SHA-256 of the generated `parser.c` this revision must contain: the
    /// content half of the pin, verified at prefetch and on every cached use.
    pub source_sha256: &'static str,
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
            source_sha256: self.source_sha256,
            source_directory: match self.source_directory {
                GrammarSourceDirectory::Root => None,
                GrammarSourceDirectory::Subdirectory(directory) => Some(directory),
            },
        }
    }

    /// Read this grammar from the shared cache, verifying the pin.
    ///
    /// Cache-only: no network, no git, no editor subprocess. A cold or stale
    /// cache is an error naming `cargo xtask treesit-grammars`, which
    /// prefetches through [`TreeSitterGrammarCase::prefetch`].
    pub fn prepare(self) -> Result<PathBuf, String> {
        let source = self.source();
        super::cached_tree_sitter_grammar(
            source.language,
            source.revision,
            source.source_directory,
            source.source_sha256,
        )
    }

    /// Fetch, hash-verify, and build this grammar into the shared cache.
    ///
    /// The prefetch half of the fixture: run it through
    /// `cargo xtask treesit-grammars`, never from a test.
    pub fn prefetch(self, gnu: &EmacsRuntime) -> Result<PathBuf, String> {
        let source = self.source();
        super::prefetch_cached_tree_sitter_grammar_with_source_directory(
            gnu,
            source.language,
            source.repository,
            source.revision,
            source.source_directory,
            source.source_sha256,
        )
    }

    /// Read the grammar through [`TreeSitterGrammarCase::prepare`] and expose
    /// its generated sources for compiler-backed fixtures. Cache layout
    /// remains owned by this preparation module.
    pub fn prepare_generated_source(self) -> Result<PathBuf, String> {
        self.prepare()?;
        let source = self.source();
        Ok(super::GrammarCachePaths::for_source(source)
            .checkout
            .join(source.source_directory.unwrap_or("src")))
    }
}

/// A grammar pinned outside the GNU 31.1 catalog by a suite that needs it.
///
/// Same cache, same hash rule, listed here so one prefetch command primes
/// every suite that prepares a grammar and no call site keeps a second,
/// private copy of a pin.
#[derive(Clone, Copy, Debug)]
pub struct ExtraTreeSitterGrammarPin {
    pub language: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub source_sha256: &'static str,
    pub source_directory: Option<&'static str>,
}

impl ExtraTreeSitterGrammarPin {
    /// Read this grammar from the shared cache, verifying the pin.
    pub fn prepare(&self) -> Result<PathBuf, String> {
        super::cached_tree_sitter_grammar(
            self.language,
            self.revision,
            self.source_directory,
            self.source_sha256,
        )
    }

    /// Fetch, hash-verify, and build this grammar into the shared cache.
    pub fn prefetch(&self, gnu: &EmacsRuntime) -> Result<PathBuf, String> {
        super::prefetch_cached_tree_sitter_grammar_with_source_directory(
            gnu,
            self.language,
            self.repository,
            self.revision,
            self.source_directory,
            self.source_sha256,
        )
    }
}

/// Grammars pinned by individual suites rather than the GNU 31.1 catalog:
/// the melpa parity modes and the perf harness's editor scenarios. Every
/// entry is prefetched by `cargo xtask treesit-grammars`.
pub const EXTRA_TREE_SITTER_GRAMMAR_PINS: &[ExtraTreeSitterGrammarPin] = &[
    ExtraTreeSitterGrammarPin {
        language: "astro",
        repository: "https://github.com/virchau13/tree-sitter-astro",
        revision: "213f6e6973d9b456c6e50e86f19f66877e7ef0ee",
        source_sha256: "64bba63bd3f101007ce4dffae94ce3e38b2793f40ddf39db101b7ecd5e612c29",
        source_directory: None,
    },
    ExtraTreeSitterGrammarPin {
        language: "css",
        repository: "https://github.com/tree-sitter/tree-sitter-css",
        revision: "dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f",
        source_sha256: "2e5150071220012ee635ac9e2119f4f3a93e0b51a7a3b69ab0cd87c18cf5e51e",
        source_directory: None,
    },
    ExtraTreeSitterGrammarPin {
        language: "tsx",
        repository: "https://github.com/tree-sitter/tree-sitter-typescript",
        revision: "75b3874edb2dc714fb1fd77a32013d0f8699989f",
        source_sha256: "1902cb53fa7ff5179df89b2eea863165e84c8cc866226419dc26921d8c055885",
        source_directory: Some("tsx/src"),
    },
    ExtraTreeSitterGrammarPin {
        language: "ada",
        repository: "https://github.com/briot/tree-sitter-ada",
        revision: "6b58259a08b1a22ba0247a7ce30be384db618da6",
        source_sha256: "525de7106c68b96cc0f80997856250f2604620bbba4d4812210d0be8d05a7cef",
        source_directory: None,
    },
    ExtraTreeSitterGrammarPin {
        language: "asciidoc",
        repository: "https://github.com/cathaysia/tree-sitter-asciidoc",
        revision: "68b1a5b9bdb01dcd049c1252ea0c33a2063df72e",
        source_sha256: "bf641449a33fa34162de4bad158f2dd0dc1c39faf9139a9a31e2ce0986051ae9",
        source_directory: Some("tree-sitter-asciidoc/src"),
    },
    ExtraTreeSitterGrammarPin {
        language: "asciidoc-inline",
        repository: "https://github.com/cathaysia/tree-sitter-asciidoc",
        revision: "68b1a5b9bdb01dcd049c1252ea0c33a2063df72e",
        source_sha256: "25ab70ffa5cc8aa5c4a3989fdd635a694718d828bf9a0a5f7eff46ea5c44ae43",
        source_directory: Some("tree-sitter-asciidoc_inline/src"),
    },
    ExtraTreeSitterGrammarPin {
        language: "rust",
        repository: "https://github.com/tree-sitter/tree-sitter-rust",
        revision: "18b0515fca567f5a10aee9978c6d2640e878671a",
        source_sha256: "a0f424079fd2ba8fe6ae29538e4a773e43c7a1271e6962eb6e8185f67a5c0db9",
        source_directory: None,
    },
];

/// The site-pinned grammar entry for `language`.
///
/// A suite that prepares an out-of-catalog grammar resolves its pin through
/// this one list instead of keeping a private copy: the entry names the
/// revision and the `parser.c` SHA-256 that the cache will be verified
/// against. Note the catalog and the extras may pin different revisions of
/// one repository -- `css` here is the revision astro-ts-mode was attested
/// with, not the GNU 31.1 catalog's.
pub fn extra_tree_sitter_grammar_pin(language: &str) -> Result<ExtraTreeSitterGrammarPin, String> {
    EXTRA_TREE_SITTER_GRAMMAR_PINS
        .iter()
        .copied()
        .find(|pin| pin.language == language)
        .ok_or_else(|| {
            format!(
                "no pinned Tree-sitter grammar for '{language}' in EXTRA_TREE_SITTER_GRAMMAR_PINS"
            )
        })
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

                source_sha256: "b9cff39a81cf3e6e06f2dc8d9610f49bb947bea08c54aaf880955e40e0e9c3d5",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "echo hello\n",
                root_type: "program",
            },
            c => $crate::TreeSitterGrammarCase {
                language: "c",

                repository: "https://github.com/tree-sitter/tree-sitter-c",

                revision: "3aa2995549d5d8b26928e8d3fa2770fd4327414e",

                source_sha256: "5a44cd6746b323aaa79f57364ad4c24e5510782c55f1371599aad6090ec7c5c8",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "int main(void) { return 0; }\n",
                root_type: "translation_unit",
            },
            c_sharp => $crate::TreeSitterGrammarCase {
                language: "c-sharp",

                repository: "https://github.com/tree-sitter/tree-sitter-c-sharp",

                revision: "285c993f01d9955932b45a6192055003aa70a570",

                source_sha256: "6637fd5a829f0bfd1368e4032fa2ba489306f7828cf36e0b56a154f3bd5d0213",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {}\n",
                root_type: "compilation_unit",
            },
            cmake => $crate::TreeSitterGrammarCase {
                language: "cmake",

                repository: "https://github.com/uyha/tree-sitter-cmake",

                revision: "f2569dd1fce0f252097a25bcbcb9ed8898840310",

                source_sha256: "dc6452c88d060deac258a98fb38755ca86f3d58e73e11b1ec0d2d2933c0c2f68",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "set(NAME hello)\n",
                root_type: "source_file",
            },
            cpp => $crate::TreeSitterGrammarCase {
                language: "cpp",

                repository: "https://github.com/tree-sitter/tree-sitter-cpp",

                revision: "464e8d9093f9308922cb1eea8fc250c5f5983e9a",

                source_sha256: "2a35a43b4af6c9f7b69624ac00c2c50808912591450dc79c05dea03ac1bae814",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {};\n",
                root_type: "translation_unit",
            },
            css => $crate::TreeSitterGrammarCase {
                language: "css",

                repository: "https://github.com/tree-sitter/tree-sitter-css",

                revision: "185475a45c413e9294e5fd5c4165df9ca1eb05dc",

                source_sha256: "f2f2b94f9a522ef42764bc5539ec1dfcf73e2a12e1cde77d818159c881d7e0e8",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "body { color: red; }\n",
                root_type: "stylesheet",
            },
            dockerfile => $crate::TreeSitterGrammarCase {
                language: "dockerfile",

                repository: "https://github.com/camdencheek/tree-sitter-dockerfile",

                revision: "439c3e7b8a9bfdbf1f7d7c2beaae4173dc484cbf",

                source_sha256: "5ed4ed9ede55fbf4c3764419fdad6cbe6422547f398c1a01377dfbfacd3fadd2",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "FROM alpine\n",
                root_type: "source_file",
            },
            elixir => $crate::TreeSitterGrammarCase {
                language: "elixir",

                repository: "https://github.com/elixir-lang/tree-sitter-elixir",

                revision: "388e4d085388f792f452a059ad9c712479571808",

                source_sha256: "d1c2000b477e873e44e485f55809e3af7e2bfce83fa4d58cfa3a9f83660e7d79",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "source",
            },
            go => $crate::TreeSitterGrammarCase {
                language: "go",

                repository: "https://github.com/tree-sitter/tree-sitter-go",

                revision: "12fe553fdaaa7449f764bc876fd777704d4fb752",

                source_sha256: "7aa4657c76dd66538228ddb319447225f9dddf54573e521743b4203f1d909051",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "package main\nfunc main() {}\n",
                root_type: "source_file",
            },
            gomod => $crate::TreeSitterGrammarCase {
                language: "gomod",

                repository: "https://github.com/camdencheek/tree-sitter-go-mod",

                revision: "a9aea5e358cde4d0f8ff20b7bc4fa311e359c7ca",

                source_sha256: "871780ffd98d62adaae06d787dbaa44ca1effafad939d544d70df130105ace1e",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "module example.com/hello\n",
                root_type: "source_file",
            },
            gowork => $crate::TreeSitterGrammarCase {
                language: "gowork",

                repository: "https://github.com/omertuc/tree-sitter-go-work",

                revision: "67f23e57b990440aae78cce03244a94bacc377b5",

                source_sha256: "91bb9a35cc386dd376b237508b51795081a4e2c81edd54aef5932407fb30a9a0",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "go 1.20\nuse ./hello\n",
                root_type: "source_file",
            },
            heex => $crate::TreeSitterGrammarCase {
                language: "heex",

                repository: "https://github.com/phoenixframework/tree-sitter-heex",

                revision: "f3d55855935f055d2583485d064be259d9ff053d",

                source_sha256: "abadb99768e86047cf8c07873b72247bd1976f95bcf8fddbb65b737267b402f8",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "<div>hello</div>\n",
                root_type: "fragment",
            },
            html => $crate::TreeSitterGrammarCase {
                language: "html",

                repository: "https://github.com/tree-sitter/tree-sitter-html",

                revision: "b30e4aea714d39a051353b5dac074c1806512047",

                source_sha256: "65768172733b3bbe461cbdc14ea928f00fbfcc51d8ac68f0a5c72071c6a0bbf1",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "<div>hello</div>\n",
                root_type: "document",
            },
            java => $crate::TreeSitterGrammarCase {
                language: "java",

                repository: "https://github.com/tree-sitter/tree-sitter-java",

                revision: "de816732671830873ee52a91998a0febb603e012",

                source_sha256: "4add5150cf4531eb5dd97f3343dcf65cd11704c84711348b328582b83424a0e4",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "class Hello {}\n",
                root_type: "program",
            },
            javascript => $crate::TreeSitterGrammarCase {
                language: "javascript",

                repository: "https://github.com/tree-sitter/tree-sitter-javascript",

                revision: "a2ef0796e8b9db9107c9ad210b3121b8ad918d5e",

                source_sha256: "1150721590eca3c7b6623c7dc3498184f3e22c5d895a08b4806ebf5804f817c3",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "const name = \"hello\";\n",
                root_type: "program",
            },
            jsdoc => $crate::TreeSitterGrammarCase {
                language: "jsdoc",

                repository: "https://github.com/tree-sitter/tree-sitter-jsdoc",

                revision: "0c5d665d04bc4ab5efda348bae3e7e34f20f264a",

                source_sha256: "b851240b8d9cf049dddc4b84f04b5de552b9ed966bf09230d93e9fb0a419dd98",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "/** This is a description of the foo function. */",
                root_type: "document",
            },
            json => $crate::TreeSitterGrammarCase {
                language: "json",

                repository: "https://github.com/tree-sitter/tree-sitter-json",

                revision: "e2a0ad1b38af1d2affec29e66dbe63f214f0e531",

                source_sha256: "e8e1ff5df0d73e3b82574129724e68ef4fa0faf1b8c43dd3f5c1a84839f830ab",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "{\"name\":\"hello\"}\n",
                root_type: "document",
            },
            lua => $crate::TreeSitterGrammarCase {
                language: "lua",

                repository: "https://github.com/tree-sitter-grammars/tree-sitter-lua",

                revision: "99fc677e6971c425e8d407f59c77ab897e585c92",

                source_sha256: "fa3a2efe5803017c536d4727fe408b9eaf3e33b86662a5b1a1d12227d0022231",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "local name = \"hello\"\n",
                root_type: "chunk",
            },
            markdown => $crate::TreeSitterGrammarCase {
                language: "markdown",

                repository: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",

                revision: "5cdc549ab8f461aff876c5be9741027189299cec",

                source_sha256: "53428cb28f9f119850377748288d8020ea84a9a9ba21f08baed0fb1a632739ba",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tree-sitter-markdown/src"),
                sample: "# hello\n",
                root_type: "document",
            },
            markdown_inline => $crate::TreeSitterGrammarCase {
                language: "markdown-inline",

                repository: "https://github.com/tree-sitter-grammars/tree-sitter-markdown",

                revision: "e70017dcf8caad949e41425e7191c3e82127b825",

                source_sha256: "d7239a32717e87c1e597ba25402d198c2c4398613bc64f0d88c84dda15b2dff8",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tree-sitter-markdown-inline/src"),
                sample: "hello **world**",
                root_type: "inline",
            },
            php => $crate::TreeSitterGrammarCase {
                language: "php",

                repository: "https://github.com/tree-sitter/tree-sitter-php",

                revision: "11d2b8b272690ef48018afe9e651c977bfb061f2",

                source_sha256: "62587f0d77b67589c86c9c2d64869ae60320bcb4ac3f6a572bcd2a7f47455022",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("php/src"),
                sample: "<?php $name = \"hello\";\n",
                root_type: "program",
            },
            python => $crate::TreeSitterGrammarCase {
                language: "python",

                repository: "https://github.com/tree-sitter/tree-sitter-python",

                revision: "e6226391f67a007b11012aa394ffd3dca399062c",

                source_sha256: "5df63395433197cb65168e95f5ce705cb1dd4c12a11a85020755a5c4b8b55c21",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "module",
            },
            ruby => $crate::TreeSitterGrammarCase {
                language: "ruby",

                repository: "https://github.com/tree-sitter/tree-sitter-ruby",

                revision: "557ec01403bd1fcfea50f1805641ca773cbaaa42",

                source_sha256: "4ce468358b6f4e25a35c8cf6bc0eaf60665bc22d602f8c939323c2347255cd15",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "program",
            },
            rust => $crate::TreeSitterGrammarCase {
                language: "rust",

                repository: "https://github.com/tree-sitter/tree-sitter-rust",

                revision: "c447dcce961ac438aaeaf117347749fe7d1e8365",

                source_sha256: "bd34b47f9cce5b4f85153e1180bdb4aa3d2b12fd0c33bb22bcbd9ffc1c3f80d5",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "fn main() {}\n",
                root_type: "source_file",
            },
            toml => $crate::TreeSitterGrammarCase {
                language: "toml",

                repository: "https://github.com/tree-sitter-grammars/tree-sitter-toml",

                revision: "d7d3ba873f0d22aef59aa36bb0532936fc670b7d",

                source_sha256: "1991a2608e6f0214e563fe5f762e8df72954dc69a3dd9a1e22ea8a870e4052c3",
                source_directory: $crate::GrammarSourceDirectory::Root,
                sample: "name = \"hello\"\n",
                root_type: "document",
            },
            tsx => $crate::TreeSitterGrammarCase {
                language: "tsx",

                repository: "https://github.com/tree-sitter/tree-sitter-typescript",

                revision: "14af1670c16cab8422d2a504dc4e5961293934e3",

                source_sha256: "1902cb53fa7ff5179df89b2eea863165e84c8cc866226419dc26921d8c055885",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("tsx/src"),
                sample: "const name = <div>hello</div>;\n",
                root_type: "program",
            },
            typescript => $crate::TreeSitterGrammarCase {
                language: "typescript",

                repository: "https://github.com/tree-sitter/tree-sitter-typescript",

                revision: "14af1670c16cab8422d2a504dc4e5961293934e3",

                source_sha256: "74fe453edd70f4eae9af0a1050cbd7943d8971d59165b6aaebbaa0a0b716d1aa",
                source_directory: $crate::GrammarSourceDirectory::Subdirectory("typescript/src"),
                sample: "const name: string = \"hello\";\n",
                root_type: "program",
            },
            yaml => $crate::TreeSitterGrammarCase {
                language: "yaml",

                repository: "https://github.com/tree-sitter-grammars/tree-sitter-yaml",

                revision: "1608168bb28493bf108460a0e5a6023a86dca738",

                source_sha256: "2a5e194b70cce476e5d696007aee076b8bef720dded0d56972844f420a2f80f2",
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
