//! Exact immutable font assets for hermetic layout and GUI tests.
//! Release files are pinned by SHA-256. Font families are never aliased to
//! an unrelated fixture; consumers load these exact faces into an owned catalog.
use super::*;

#[derive(Debug)]
struct ArchiveByteLimit(u64);

#[derive(Debug)]
struct PinnedZip {
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
    prefix: &'static str,
    download_limit: ArchiveByteLimit,
}

fn prepare_zip_font(archive: &PinnedZip, font: &ArchivedFont) -> Result<PathBuf, FixtureError> {
    let cache = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&cache)?;
    let lock_path = cache.join(format!(".{}.lock", archive.name));
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;
    let result = (|| {
        let destination = cache.join(format!("{}-{}", archive.name, font.name));
        if destination.is_file() && verify_sha256(&destination, font.sha256).is_ok() {
            return Ok(destination);
        }
        let archive_path = neomacs_infra::pinned::pinned_file_with_limit(
            archive.name,
            archive.url,
            archive.sha256,
            archive.download_limit.0,
        )
        .map_err(|source| FixtureError::Fixture {
            name: archive.name.to_owned(),
            source,
        })?;
        let file = File::open(&archive_path).map_err(|source| FixtureError::Io {
            path: archive_path.clone(),
            source,
        })?;
        let mut zip = zip::ZipArchive::new(file).map_err(|source| FixtureError::Zip {
            path: archive_path.clone(),
            source,
        })?;
        // ZIP keys use '/' on every host, including Windows. These are
        // immutable archive entries, not native filesystem paths.
        let entry_name = if archive.prefix.is_empty() {
            font.name.to_owned()
        } else {
            format!("{}/{}", archive.prefix, font.name)
        };
        let entry = zip
            .by_name(&entry_name)
            .map_err(|source| FixtureError::Zip {
                path: archive_path.clone(),
                source,
            })?;
        if entry.size() > MAX_PINNED_FIXTURE_BYTES {
            return Err(FixtureError::InvalidArchiveEntry(PathBuf::from(
                &entry_name,
            )));
        }
        let partial = destination.with_extension("partial");
        let mut output = File::create(&partial).map_err(|source| FixtureError::Io {
            path: partial.clone(),
            source,
        })?;
        io::copy(&mut entry.take(MAX_PINNED_FIXTURE_BYTES + 1), &mut output).map_err(|source| {
            FixtureError::Io {
                path: partial.clone(),
                source,
            }
        })?;
        verify_sha256(&partial, font.sha256)?;
        if destination.exists() {
            fs::remove_file(&destination).map_err(|source| FixtureError::Io {
                path: destination.clone(),
                source,
            })?;
        }
        fs::rename(&partial, &destination).map_err(|source| FixtureError::Io {
            path: destination.clone(),
            source,
        })?;
        Ok(destination)
    })();
    FileExt::unlock(&lock).map_err(|source| FixtureError::Io {
        path: lock_path,
        source,
    })?;
    result
}

/// Verified upstream `noto-sans-google.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn noto_sans_variable() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "NotoSans-variable.ttf",
            "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosans/NotoSans%5Bwdth,wght%5D.ttf",
            "bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d",
        ).unwrap_or_else(|error| panic!("pinned NotoSans-variable.ttf: {error}"))
    }).as_path()
}

/// Verified upstream `noto-sans-italic-google.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn noto_sans_italic_variable() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "NotoSans-Italic-variable.ttf",
            "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosans/NotoSans-Italic%5Bwdth,wght%5D.ttf",
            "58e6e0ebd1931b29a365aa2d3e2ee9a9e831a3af7cf3ad1462d4e72154f0b291",
        ).unwrap_or_else(|error| panic!("pinned NotoSans-Italic-variable.ttf: {error}"))
    }).as_path()
}

/// Verified upstream `hack-italic.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn hack_italic_3_003() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "Hack-Italic-3.003.ttf",
            "https://raw.githubusercontent.com/source-foundry/Hack/b47d21f808fbf4784565feccde2213ef2af924b3/build/ttf/Hack-Italic.ttf",
            "096fb67a2b85f3c866e9cb3e965b27c2c10b977315f4d3d7f095674be35091c1",
        ).unwrap_or_else(|error| panic!("pinned Hack-Italic-3.003.ttf: {error}"))
    }).as_path()
}

/// Verified upstream `noto-cjk-vf.ttc`; exact file identity is part of the fixture.
#[must_use]
pub fn noto_sans_cjk_variable() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "NotoSansCJK-VF-2.004.otf.ttc",
            "https://raw.githubusercontent.com/notofonts/noto-cjk/523d033d6cb47f4a80c58a35753646f5c3608a78/Sans/Variable/OTC/NotoSansCJK-VF.otf.ttc",
            "d3d8256cdec8dbcb3552284bc6b20c734dd60c2ee9df83b5758e34807c4bac32",
        ).unwrap_or_else(|error| panic!("pinned NotoSansCJK-VF-2.004.otf.ttc: {error}"))
    }).as_path()
}

/// Verified upstream `JetBrainsMono-Regular.otf`; exact file identity is part of the fixture.
#[must_use]
pub fn jetbrains_mono_regular_2_304() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "JetBrainsMono-Regular-2.304.otf",
            "https://raw.githubusercontent.com/JetBrains/JetBrainsMono/cd5227bd1f61dff3bbd6c814ceaf7ffd95e947d9/fonts/otf/JetBrainsMono-Regular.otf",
            "621dcc09521ebed968763593bef20721aa3a886244b23f3db52f626b96c49427",
        ).unwrap_or_else(|error| panic!("pinned JetBrainsMono-Regular-2.304.otf: {error}"))
    }).as_path()
}

/// Verified upstream `JetBrainsMono-Bold.otf`; exact file identity is part of the fixture.
#[must_use]
pub fn jetbrains_mono_bold_2_304() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "JetBrainsMono-Bold-2.304.otf",
            "https://raw.githubusercontent.com/JetBrains/JetBrainsMono/cd5227bd1f61dff3bbd6c814ceaf7ffd95e947d9/fonts/otf/JetBrainsMono-Bold.otf",
            "aeb06620f935cd8c75769154228943ca1051962727eb07213a8bb7f31dbb9c36",
        ).unwrap_or_else(|error| panic!("pinned JetBrainsMono-Bold-2.304.otf: {error}"))
    }).as_path()
}

/// Verified upstream `JetBrainsMono-Italic.otf`; exact file identity is part of the fixture.
#[must_use]
pub fn jetbrains_mono_italic_2_304() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "JetBrainsMono-Italic-2.304.otf",
            "https://raw.githubusercontent.com/JetBrains/JetBrainsMono/cd5227bd1f61dff3bbd6c814ceaf7ffd95e947d9/fonts/otf/JetBrainsMono-Italic.otf",
            "b6b6b6c91e419c0957f6addd7c7e3c731d2e5ee7cffa337168effe15d92aef83",
        ).unwrap_or_else(|error| panic!("pinned JetBrainsMono-Italic-2.304.otf: {error}"))
    }).as_path()
}

/// Verified upstream `JetBrainsMonoNerdFont-Regular.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn jetbrains_mono_nerd_regular_3_4_0() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "JetBrainsMonoNerdFont-Regular-3.4.0.ttf",
            "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/fa7b859994228a9c8759f99c55a8d31ee92a1b5e/patched-fonts/JetBrainsMono/Ligatures/Regular/JetBrainsMonoNerdFont-Regular.ttf",
            "0ec29a68b539ece7078fc714cebff0c0accb2f4948f8f7963d9f5e86633b12d9",
        ).unwrap_or_else(|error| panic!("pinned JetBrainsMonoNerdFont-Regular-3.4.0.ttf: {error}"))
    }).as_path()
}

/// Verified upstream `JetBrainsMonoNerdFont-Bold.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn jetbrains_mono_nerd_bold_3_4_0() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "JetBrainsMonoNerdFont-Bold-3.4.0.ttf",
            "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/fa7b859994228a9c8759f99c55a8d31ee92a1b5e/patched-fonts/JetBrainsMono/Ligatures/Bold/JetBrainsMonoNerdFont-Bold.ttf",
            "e82e27a7f37c9a0a13cc4e417503a149c6a0280586930772d2ebed803159c864",
        ).unwrap_or_else(|error| panic!("pinned JetBrainsMonoNerdFont-Bold-3.4.0.ttf: {error}"))
    }).as_path()
}

/// Verified upstream `JetBrainsMonoNerdFont-Italic.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn jetbrains_mono_nerd_italic_3_4_0() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "JetBrainsMonoNerdFont-Italic-3.4.0.ttf",
            "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/fa7b859994228a9c8759f99c55a8d31ee92a1b5e/patched-fonts/JetBrainsMono/Ligatures/Italic/JetBrainsMonoNerdFont-Italic.ttf",
            "981133a258ef4c62769a7a7214a4b455ac1c63923784a70f50c48977b67f7025",
        ).unwrap_or_else(|error| panic!("pinned JetBrainsMonoNerdFont-Italic-3.4.0.ttf: {error}"))
    }).as_path()
}

/// Verified upstream `SymbolsNerdFontMono-Regular.ttf`; exact file identity is part of the fixture.
#[must_use]
pub fn symbols_nerd_mono_3_4_0() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "SymbolsNerdFontMono-Regular-3.4.0.ttf",
            "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/fa7b859994228a9c8759f99c55a8d31ee92a1b5e/patched-fonts/NerdFontsSymbolsOnly/SymbolsNerdFontMono-Regular.ttf",
            "f0f624d9b474bea1662cf7e862d44aebe1ae1f6c7f9cb7a0ca5d0e5ac9561c60",
        ).unwrap_or_else(|error| panic!("pinned SymbolsNerdFontMono-Regular-3.4.0.ttf: {error}"))
    }).as_path()
}

/// Explicit outline faces from the immutable dejavu_2_37 release.
#[derive(Debug)]
pub struct DejaVuFixtures {
    mono_regular: PathBuf,
    mono_bold: PathBuf,
    mono_oblique: PathBuf,
    mono_bold_oblique: PathBuf,
    sans_regular: PathBuf,
    sans_bold: PathBuf,
    sans_oblique: PathBuf,
    sans_bold_oblique: PathBuf,
    serif_regular: PathBuf,
    serif_bold: PathBuf,
    serif_italic: PathBuf,
    serif_bold_italic: PathBuf,
}

impl DejaVuFixtures {
    #[must_use]
    pub fn mono_regular(&self) -> &Path {
        self.mono_regular.as_path()
    }
    #[must_use]
    pub fn mono_bold(&self) -> &Path {
        self.mono_bold.as_path()
    }
    #[must_use]
    pub fn mono_oblique(&self) -> &Path {
        self.mono_oblique.as_path()
    }
    #[must_use]
    pub fn mono_bold_oblique(&self) -> &Path {
        self.mono_bold_oblique.as_path()
    }
    #[must_use]
    pub fn sans_regular(&self) -> &Path {
        self.sans_regular.as_path()
    }
    #[must_use]
    pub fn sans_bold(&self) -> &Path {
        self.sans_bold.as_path()
    }
    #[must_use]
    pub fn sans_oblique(&self) -> &Path {
        self.sans_oblique.as_path()
    }
    #[must_use]
    pub fn sans_bold_oblique(&self) -> &Path {
        self.sans_bold_oblique.as_path()
    }
    #[must_use]
    pub fn serif_regular(&self) -> &Path {
        self.serif_regular.as_path()
    }
    #[must_use]
    pub fn serif_bold(&self) -> &Path {
        self.serif_bold.as_path()
    }
    #[must_use]
    pub fn serif_italic(&self) -> &Path {
        self.serif_italic.as_path()
    }
    #[must_use]
    pub fn serif_bold_italic(&self) -> &Path {
        self.serif_bold_italic.as_path()
    }
}

#[must_use]
pub fn dejavu_2_37() -> &'static DejaVuFixtures {
    static FILES: OnceLock<DejaVuFixtures> = OnceLock::new();
    const ARCHIVE: PinnedZip = PinnedZip {
        name: "dejavu-2.37.zip",
        url: "https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.zip",
        sha256: "7576310b219e04159d35ff61dd4a4ec4cdba4f35c00e002a136f00e96a908b0a",
        prefix: "dejavu-fonts-ttf-2.37/ttf",
        download_limit: ArchiveByteLimit(16 * 1024 * 1024),
    };
    FILES.get_or_init(|| DejaVuFixtures {
        mono_regular: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSansMono.ttf",
                sha256: "b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSansMono.ttf: {error}")),
        mono_bold: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSansMono-Bold.ttf",
                sha256: "bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSansMono-Bold.ttf: {error}")),
        mono_oblique: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSansMono-Oblique.ttf",
                sha256: "742097840c541870e8d6dc5c9b37bb1ceeea6c0dedd1d475faf903ef9df734b0",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSansMono-Oblique.ttf: {error}")),
        mono_bold_oblique: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSansMono-BoldOblique.ttf",
                sha256: "91713a71d550bba22c2a6b2bb2a9ad8f9a159e12e4e9f0a5b2677998ba21213e",
            },
        )
        .unwrap_or_else(|error| {
            panic!("pinned dejavu_2_37/DejaVuSansMono-BoldOblique.ttf: {error}")
        }),
        sans_regular: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSans.ttf",
                sha256: "7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSans.ttf: {error}")),
        sans_bold: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSans-Bold.ttf",
                sha256: "e6476c1b80502924294eed40894c5b18e06c181444ca953e5334262df9c27724",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSans-Bold.ttf: {error}")),
        sans_oblique: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSans-Oblique.ttf",
                sha256: "4af75fa16ee6d3ad43e1ecec41862c24954af26a55c6bb1ebb27bd486a50f5f4",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSans-Oblique.ttf: {error}")),
        sans_bold_oblique: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSans-BoldOblique.ttf",
                sha256: "eb436dca0c2594b73d8b603b892e374fdfd8d885d25ffb4f18df4c4c0b49e50f",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSans-BoldOblique.ttf: {error}")),
        serif_regular: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSerif.ttf",
                sha256: "42d1edeb7952f31b1f96d767ed7030b08a39e0c372b0071641518864e2bffb51",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSerif.ttf: {error}")),
        serif_bold: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSerif-Bold.ttf",
                sha256: "c47b5527bcdc8dcf9ea8c77054454c5a884beaca2f44851a2a823ee639cbf07f",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSerif-Bold.ttf: {error}")),
        serif_italic: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSerif-Italic.ttf",
                sha256: "2e39b1d50f90b933b00c7bb54a96afd3f86419b3d717c7cf202e36f2d4973e47",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSerif-Italic.ttf: {error}")),
        serif_bold_italic: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "DejaVuSerif-BoldItalic.ttf",
                sha256: "8d3dd3d31350309042ed226af82b34539bd773518e6107cb352712853ba80308",
            },
        )
        .unwrap_or_else(|error| panic!("pinned dejavu_2_37/DejaVuSerif-BoldItalic.ttf: {error}")),
    })
}

/// Explicit outline faces from the immutable ubuntu_0_83 release.
#[derive(Debug)]
pub struct UbuntuFixtures {
    mono_regular: PathBuf,
    mono_bold: PathBuf,
    mono_italic: PathBuf,
    mono_bold_italic: PathBuf,
    regular: PathBuf,
    bold: PathBuf,
    italic: PathBuf,
    bold_italic: PathBuf,
}

impl UbuntuFixtures {
    #[must_use]
    pub fn mono_regular(&self) -> &Path {
        self.mono_regular.as_path()
    }
    #[must_use]
    pub fn mono_bold(&self) -> &Path {
        self.mono_bold.as_path()
    }
    #[must_use]
    pub fn mono_italic(&self) -> &Path {
        self.mono_italic.as_path()
    }
    #[must_use]
    pub fn mono_bold_italic(&self) -> &Path {
        self.mono_bold_italic.as_path()
    }
    #[must_use]
    pub fn regular(&self) -> &Path {
        self.regular.as_path()
    }
    #[must_use]
    pub fn bold(&self) -> &Path {
        self.bold.as_path()
    }
    #[must_use]
    pub fn italic(&self) -> &Path {
        self.italic.as_path()
    }
    #[must_use]
    pub fn bold_italic(&self) -> &Path {
        self.bold_italic.as_path()
    }
}

#[must_use]
pub fn ubuntu_0_83() -> &'static UbuntuFixtures {
    static FILES: OnceLock<UbuntuFixtures> = OnceLock::new();
    const ARCHIVE: PinnedZip = PinnedZip {
        name: "ubuntu-0.83.zip",
        url: "https://assets.ubuntu.com/v1/0cef8205-ubuntu-font-family-0.83.zip",
        sha256: "61a2b342526fd552f19fef438bb9211a8212de19ad96e32a1209c039f1d68ecf",
        prefix: "ubuntu-font-family-0.83",
        download_limit: ArchiveByteLimit(16 * 1024 * 1024),
    };
    FILES.get_or_init(|| UbuntuFixtures {
        mono_regular: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "UbuntuMono-R.ttf",
                sha256: "b35dd9d2131d5d83a9b87fe9ad22c6288fa3d17688d43302c14da29812417d63",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/UbuntuMono-R.ttf: {error}")),
        mono_bold: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "UbuntuMono-B.ttf",
                sha256: "11f15c3a6bbd998a8695fdefb3475931c3789aa035d7546f2efe78e83b352f6b",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/UbuntuMono-B.ttf: {error}")),
        mono_italic: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "UbuntuMono-RI.ttf",
                sha256: "960b2bc286c2ff7d49073303858c65e1fc9013c17a971b61123b02c39454ef75",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/UbuntuMono-RI.ttf: {error}")),
        mono_bold_italic: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "UbuntuMono-BI.ttf",
                sha256: "bd255784bb87b5c41513a12a86f0f9cf061bce4e8256d3bfe7234611002e8f48",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/UbuntuMono-BI.ttf: {error}")),
        regular: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "Ubuntu-R.ttf",
                sha256: "52c1afa489ae7bfd893af6cdd9f1af258005703600449e70d338caabcff507e5",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/Ubuntu-R.ttf: {error}")),
        bold: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "Ubuntu-B.ttf",
                sha256: "c90e629e932f768909bdce1481ccd259d8aff1490a4bc1cf94d5413492317336",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/Ubuntu-B.ttf: {error}")),
        italic: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "Ubuntu-RI.ttf",
                sha256: "0626647f11d9c911f934715d1060b590744887224601abaeb22ac8f582b57118",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/Ubuntu-RI.ttf: {error}")),
        bold_italic: prepare_zip_font(
            &ARCHIVE,
            &ArchivedFont {
                name: "Ubuntu-BI.ttf",
                sha256: "2c33de34d51065a5e56bae5e1c7c13cd128268b26569838a38db038e6ac9723d",
            },
        )
        .unwrap_or_else(|error| panic!("pinned ubuntu_0_83/Ubuntu-BI.ttf: {error}")),
    })
}

/// Exact script face for hermetic complex-run and source-position tests.
#[must_use]
pub fn noto_sans_devanagari_variable() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "NotoSansDevanagari-variable.ttf",
            "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosansdevanagari/NotoSansDevanagari%5Bwdth,wght%5D.ttf",
            "14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e",
        ).unwrap_or_else(|error| panic!("pinned NotoSansDevanagari-variable.ttf: {error}"))
    }).as_path()
}

/// Exact script face for hermetic complex-run and source-position tests.
#[must_use]
pub fn noto_sans_thai_variable() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "NotoSansThai-variable.ttf",
            "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosansthai/NotoSansThai%5Bwdth,wght%5D.ttf",
            "5a1c559bb539583c8a1fd99d1c5b9491e5e14478c9cd2bd0970d5c3096cc9ef8",
        ).unwrap_or_else(|error| panic!("pinned NotoSansThai-variable.ttf: {error}"))
    }).as_path()
}

/// The two immutable faces used by the native fixed-pitch weight conformance test.
/// This large release is downloaded only by callers that explicitly request it.
#[derive(Debug)]
pub struct IosevkaFixtures {
    regular: PathBuf,
    bold: PathBuf,
}
impl IosevkaFixtures {
    #[must_use]
    pub fn regular(&self) -> &Path {
        &self.regular
    }
    #[must_use]
    pub fn bold(&self) -> &Path {
        &self.bold
    }
}
#[must_use]
pub fn iosevka_33_2_9() -> &'static IosevkaFixtures {
    static FONTS: OnceLock<IosevkaFixtures> = OnceLock::new();
    FONTS.get_or_init(|| {
        const ARCHIVE: PinnedZip = PinnedZip {
            name: "PkgTTF-Iosevka-33.2.9.zip",
            url: "https://github.com/be5invis/Iosevka/releases/download/v33.2.9/PkgTTF-Iosevka-33.2.9.zip",
            sha256: "02e6888471051b8b2702a764bca9ca9357142aae71485c10eb7a041186f45d96",
            prefix: "", download_limit: ArchiveByteLimit(160 * 1024 * 1024),
        };
        IosevkaFixtures {
            regular: prepare_zip_font(&ARCHIVE, &ArchivedFont { name: "Iosevka-Regular.ttf", sha256: "44bcdf4b5fbfbb2a5e378808daa6449df90962327349d8b1d4deccbd3d52252c" }).unwrap_or_else(|error| panic!("pinned Iosevka regular: {error}")),
            bold: prepare_zip_font(&ARCHIVE, &ArchivedFont { name: "Iosevka-Bold.ttf", sha256: "dc22a8f87c39503897044d98e9b1c27e51006b443f9af17077bb656f8f10d8fb" }).unwrap_or_else(|error| panic!("pinned Iosevka bold: {error}")),
        }
    })
}

/// Verified immutable Hack3.003 face.
#[must_use]
pub fn hack_regular_3_003() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "Hack-Regular-3.003.ttf",
            "https://raw.githubusercontent.com/source-foundry/Hack/b47d21f808fbf4784565feccde2213ef2af924b3/build/ttf/Hack-Regular.ttf",
            "15f55cc0c85a2988d2b4b3a8cdb5d77fdfbaf319e1bb5309d725db9818fb7125",
        ).unwrap_or_else(|error| panic!("pinned Hack fixture: {error}"))
    }).as_path()
}

/// Verified immutable Hack3.003 face.
#[must_use]
pub fn hack_bold_3_003() -> &'static Path {
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        prepare_pinned_file(
            "Hack-Bold-3.003.ttf",
            "https://raw.githubusercontent.com/source-foundry/Hack/b47d21f808fbf4784565feccde2213ef2af924b3/build/ttf/Hack-Bold.ttf",
            "5bbf531eff7f8a0c2559c9a0656718e2828a012a9b1f60b5f54006d59a4de8d4",
        ).unwrap_or_else(|error| panic!("pinned Hack fixture: {error}"))
    }).as_path()
}
