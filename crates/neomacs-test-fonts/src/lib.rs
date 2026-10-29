//! Downloaded, integrity-checked font assets used only by tests.
//!
//! The source archive is pinned by both release URL and SHA-256.  Tests cache
//! the verified files below the workspace's `./tmp` directory so no binary
//! fixtures need to be committed.

/// Runtime workspace root: nextest's NEXTEST_WORKSPACE_ROOT when present,
/// the compile-time constant otherwise (see neomacs-infra).
pub fn workspace_root() -> std::path::PathBuf {
    neomacs_infra::workspace_root()
}

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use fs4::FileExt;
use sha2::{Digest, Sha256};
use thiserror::Error;

const RELEASE_NAME: &str = "spleen-2.2.0";
const RELEASE_URL: &str =
    "https://github.com/fcambus/spleen/releases/download/2.2.0/spleen-2.2.0.tar.gz";
const RELEASE_SHA256: &str = "ec42925c6b56d2138c862b2f97147c872e472f674bf03423417d827a08d69a89";
const WOFF2_COLLECTION_NAME: &str = "w3c-roundtrip-collection-order-001.woff2";
const WOFF2_COLLECTION_URL: &str = "https://raw.githubusercontent.com/w3c/woff2-compiled-tests/1fd8cd583645618f4df36c65a297479840ad5510/Decoder/Tests/xhtml1/roundtrip-collection-order-001.woff2";
const WOFF2_COLLECTION_SHA256: &str =
    "7a246412785b588a43acf9cddaab0a36674f8581bd328a6107e9761e38713058";
const NOTO_COLOR_EMOJI_NAME: &str = "noto-color-emoji-2.051.ttf";
const NOTO_COLOR_EMOJI_URL: &str = "https://raw.githubusercontent.com/googlefonts/noto-emoji/8998f5dd683424a73e2314a8c1f1e359c19e8742/fonts/NotoColorEmoji.ttf";
const NOTO_COLOR_EMOJI_SHA256: &str =
    "72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b";
// The layered-color build of the same family, as Google Fonts ships it. It
// carries COLR version 1 (`BaseGlyphList` paint graphs), CPAL, an `SVG `
// fallback table and a `glyf` table whose emoji outlines are empty — the
// combination that makes a COLRv0-only rasterizer draw nothing.
const NOTO_COLOR_EMOJI_COLRV1_NAME: &str = "noto-color-emoji-colrv1.ttf";
const NOTO_COLOR_EMOJI_COLRV1_URL: &str = "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notocoloremoji/NotoColorEmoji-Regular.ttf";
const NOTO_COLOR_EMOJI_COLRV1_SHA256: &str =
    "4d82a18d8d95f60ba883ce242bbadbf84a576e987745dd7ba38d71c67cff2d73";
const NABLA_NAME: &str = "nabla.ttf";
const NABLA_URL: &str = "https://raw.githubusercontent.com/google/fonts/34a2c3daeeb2715576af0786ae4e9dc6c7d8ca2a/ofl/nabla/Nabla%5BEDPT,EHLT%5D.ttf";
const NABLA_SHA256: &str = "e45cec60eb2099b4b4ffee8ebe005d1d3060771071ede1caeeeb12e37a2c00ae";
const MPLUS_1_CODE_THIN_NAME: &str = "MPLUS1Code-Thin.ttf";
const MPLUS_1_CODE_THIN_URL: &str = "https://raw.githubusercontent.com/coz-m/MPLUS_FONTS/9e2b62ba986047127eb0a73b3ca70d75a50114bd/fonts/MPLUS1Code/ttf/MPLUS1Code-Thin.ttf";
const MPLUS_1_CODE_THIN_SHA256: &str =
    "730c24ad92d46ef913d92dd2ffcd41a535609604b8213d8326292d2664ffd107";
// The variable build Google Fonts ships for the same family the host may
// install as static faces: one face whose `wght` axis spans 100..900, so an
// in-range request must stay a synthesized instance weight instead of
// snapping to a nearest static sibling.
const NOTO_SANS_MONO_VARIABLE_NAME: &str = "NotoSansMono-variable.ttf";
const NOTO_SANS_MONO_VARIABLE_URL: &str = "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosansmono/NotoSansMono%5Bwdth,wght%5D.ttf";
const NOTO_SANS_MONO_VARIABLE_SHA256: &str =
    "2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081";
const NOTO_SANS_ARABIC_VARIABLE_NAME: &str = "NotoSansArabic-variable.ttf";
const NOTO_SANS_ARABIC_VARIABLE_URL: &str = "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosansarabic/NotoSansArabic%5Bwdth,wght%5D.ttf";
const NOTO_SANS_ARABIC_VARIABLE_SHA256: &str =
    "63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e";
const NOTO_SERIF_VARIABLE_NAME: &str = "NotoSerif-variable.ttf";
const NOTO_SERIF_VARIABLE_URL: &str = "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notoserif/NotoSerif%5Bwdth,wght%5D.ttf";
const NOTO_SERIF_VARIABLE_SHA256: &str =
    "4d8e6761424656867019081a1a01336f3cb086982682698714054fc33f782713";
const NOTO_SANS_HEBREW_VARIABLE_NAME: &str = "NotoSansHebrew-variable.ttf";
const NOTO_SANS_HEBREW_VARIABLE_URL: &str = "https://raw.githubusercontent.com/google/fonts/142c8963e7606b510c93a644c82a4c4cdeae6ef9/ofl/notosanshebrew/NotoSansHebrew%5Bwdth,wght%5D.ttf";
const NOTO_SANS_HEBREW_VARIABLE_SHA256: &str =
    "7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88";

// The Linux GUI CI job installs fonts-ubuntu 0.869+git20240321 and
// fonts-dejavu-core 2.37 from noble; the desktop-font fixtures capture pixel
// grids for those exact faces ("Ubuntu Mono 13" is a 10x20 cell, "DejaVu Sans
// Mono 12" 10x20 / "16" 13x25).  Pin both packages by hash so the suites
// resolve the same faces on any host instead of whatever it has installed.
// The upstream orig tarball's members are byte-identical to the installed
// `/usr/share/fonts/truetype/ubuntu` files (verified against the noble .deb).
const UBUNTU_FONTS_RELEASE: &str = "fonts-ubuntu-0.869+git20240321";
const UBUNTU_FONTS_ARCHIVE_URL: &str = "https://archive.ubuntu.com/ubuntu/pool/main/f/fonts-ubuntu/fonts-ubuntu_0.869+git20240321.orig.tar.gz";
const UBUNTU_FONTS_ARCHIVE_SHA256: &str =
    "a3583a3d112b3a19957189474507ec13431187a582f484d600a46a34150233e8";
const UBUNTU_FONTS: [(&str, &str); 4] = [
    (
        "UbuntuMono[wght].ttf",
        "fbf1e748836994f730e602f7dcf2525564d6d78aa336080cbb73af909d0e08ee",
    ),
    (
        "UbuntuMono-Italic[wght].ttf",
        "4148a3a8e6a702da461992047bcc07179f1eed12a3020384be754d015240678a",
    ),
    (
        "Ubuntu[wdth,wght].ttf",
        "28c4c189a44803b1986fd16074187034dc6d94ad35f5e87de13dd0e786b70b73",
    ),
    (
        "Ubuntu-Italic[wdth,wght].ttf",
        "f5689f6ae7f16f7ede2012702abd605ac1afa03c950713523b9953d600937975",
    ),
];
const DEJAVU_FONTS_RELEASE: &str = "dejavu-fonts-ttf-2.37";
const DEJAVU_FONTS_ARCHIVE_URL: &str = "https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.zip";
const DEJAVU_FONTS_ARCHIVE_SHA256: &str =
    "7576310b219e04159d35ff61dd4a4ec4cdba4f35c00e002a136f00e96a908b0a";
const DEJAVU_FONTS: [(&str, &str); 4] = [
    (
        "DejaVuSansMono.ttf",
        "b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582",
    ),
    (
        "DejaVuSansMono-Bold.ttf",
        "bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769",
    ),
    (
        "DejaVuSans.ttf",
        "7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954",
    ),
    (
        "DejaVuSans-Bold.ttf",
        "e6476c1b80502924294eed40894c5b18e06c181444ca953e5334262df9c27724",
    ),
];
const LXGW_NERD_RELEASE: &str = "lxgw-wenkai-nerd-1.522";
const LXGW_NERD_ARCHIVE_URL: &str = "https://github.com/Yikai-Liao/LxgwWenKaiNerdFont/releases/download/v1.522/lxgw-wenkai-nerd-1.522.tar.gz";
const LXGW_NERD_ARCHIVE_SHA256: &str =
    "a09a5f9b31ba87f7da06fef1b8174934c9db57c3c50e0c830f0572f19ef9972f";
const LXGW_NERD_REGULAR: &str = "LXGWWenKaiNerdFont-Regular.ttf";
const LXGW_NERD_REGULAR_SHA256: &str =
    "5d07bc2b306b8fb8da5eb47dc9394e93ab63cd945bba305b9605e3c442408b1a";
const MAX_LXGW_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PINNED_FIXTURE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
struct ArchivedFont {
    name: &'static str,
    sha256: &'static str,
}

const ARCHIVED_FONTS: [ArchivedFont; 5] = [
    ArchivedFont {
        name: "spleen-8x16.bdf",
        sha256: "4a3d97ee61a8c86a7525d8c723cb8a14081f395cd2feb4227ba5e3baf0629bae",
    },
    ArchivedFont {
        name: "spleen-8x16.pcf",
        sha256: "b469833f073927a92ac4ba0f75863a6b04d234d5af7d81a637857697620d5314",
    },
    ArchivedFont {
        name: "spleen-8x16.otb",
        sha256: "73784c46ffb7ff31adcee06923f839e02170fb0132a30323f1be1036b8f1da67",
    },
    ArchivedFont {
        name: "spleen-8x16.woff",
        sha256: "5ca783bd09fec9856fae0e142a1d0fac5542c2508e0c35859b39c42113a503b2",
    },
    ArchivedFont {
        name: "spleen-8x16.woff2",
        sha256: "7062d0818b46b713b08ee78e457a3e78659a2b8d3107f609676cef4e5d38aa2c",
    },
];

static SPLEEN_FIXTURES: OnceLock<SpleenFixtures> = OnceLock::new();
static WOFF2_COLLECTION_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static LXGW_NERD_REGULAR_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static MPLUS_1_CODE_THIN_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_SANS_MONO_VARIABLE_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_SANS_ARABIC_VARIABLE_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_SERIF_VARIABLE_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_SANS_HEBREW_VARIABLE_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_COLOR_EMOJI_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NOTO_COLOR_EMOJI_COLRV1_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static NABLA_FIXTURE: OnceLock<PathBuf> = OnceLock::new();
static DESKTOP_FONTS_FIXTURE: OnceLock<DesktopFontFixtures> = OnceLock::new();

/// The reported dual-width fixed-pitch font from issue #516, pinned to v3.1.0.
#[must_use]
pub fn plemol_jp_console_nf_regular() -> &'static Path {
    static FIXTURE: OnceLock<PathBuf> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            prepare_plemol_jp().unwrap_or_else(|error| panic!("PlemolJP fixture: {error}"))
        })
        .as_path()
}

fn prepare_plemol_jp() -> Result<PathBuf, FixtureError> {
    const FONT_SHA: &str = "0dadabb6766f13e31787d26d73467dd613a777e509a8214bc898333299dd4be4";
    const ZIP_SHA: &str = "015142b7ce4fb497ea6eb14567c435b69450eb5028fb7d29c032d1ffb3854abb";
    const ENTRY: &str = "PlemolJP_NF_v3.1.0/PlemolJPConsole_NF/PlemolJPConsoleNF-Regular.ttf";
    let root = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&root)?;
    let lock_path = root.join(".plemol-jp-v3.1.0.lock");
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;
    let result = (|| {
        let destination = root.join("PlemolJPConsoleNF-Regular-v3.1.0.ttf");
        if destination.exists() && verify_sha256(&destination, FONT_SHA).is_ok() {
            return Ok(destination);
        }
        let archive = neomacs_infra::pinned::pinned_file_with_limit(
            "PlemolJP_NF_v3.1.0.zip",
            "https://github.com/yuru7/PlemolJP/releases/download/v3.1.0/PlemolJP_NF_v3.1.0.zip",
            ZIP_SHA,
            192 * 1024 * 1024,
        )
        .map_err(|source| FixtureError::Fixture {
            name: "PlemolJP_NF_v3.1.0.zip".to_owned(),
            source,
        })?;
        let file = File::open(&archive).map_err(|source| FixtureError::Io {
            path: archive.clone(),
            source,
        })?;
        let mut zip = zip::ZipArchive::new(file).map_err(|source| FixtureError::Zip {
            path: archive.clone(),
            source,
        })?;
        let entry = zip.by_name(ENTRY).map_err(|source| FixtureError::Zip {
            path: archive.clone(),
            source,
        })?;
        let mut bytes = Vec::new();
        entry
            .take(MAX_PINNED_FIXTURE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| FixtureError::Io {
                path: archive.clone(),
                source,
            })?;
        let actual = sha256_bytes(&bytes);
        if actual != FONT_SHA {
            return Err(FixtureError::Checksum {
                path: destination,
                expected: FONT_SHA,
                actual,
            });
        }
        let partial = destination.with_extension("partial");
        write_all(&partial, &bytes)?;
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

/// Paths to the pinned Spleen faces used by the font boundary tests.
#[derive(Clone, Debug)]
pub struct SpleenFixtures {
    root: PathBuf,
}

impl SpleenFixtures {
    #[must_use]
    pub fn bdf(&self) -> PathBuf {
        self.root.join("spleen-8x16.bdf")
    }

    #[must_use]
    pub fn pcf(&self) -> PathBuf {
        self.root.join("spleen-8x16.pcf")
    }

    #[must_use]
    pub fn pcf_gz(&self) -> PathBuf {
        self.root.join("spleen-8x16.pcf.gz")
    }

    #[must_use]
    pub fn otb(&self) -> PathBuf {
        self.root.join("spleen-8x16.otb")
    }

    #[must_use]
    pub fn woff(&self) -> PathBuf {
        self.root.join("spleen-8x16.woff")
    }

    #[must_use]
    pub fn woff2(&self) -> PathBuf {
        self.root.join("spleen-8x16.woff2")
    }
}

/// Download and verify the pinned fixture archive on first use in this test
/// process, then return paths within the workspace-local cache.
///
/// # Panics
///
/// Panics with a detailed message when the network, checksum, archive, or cache
/// operation fails. Tests must not silently skip compatibility coverage.
#[must_use]
pub fn spleen_2_2_0() -> &'static SpleenFixtures {
    SPLEEN_FIXTURES.get_or_init(|| {
        prepare_spleen_fixtures()
            .unwrap_or_else(|error| panic!("failed to prepare pinned Spleen test fonts: {error}"))
    })
}

/// Download and verify the W3C decoder-suite WOFF2 collection used to check
/// that nonzero collection selectors survive decoding to standalone SFNT.
#[must_use]
pub fn woff2_collection() -> &'static Path {
    WOFF2_COLLECTION_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                WOFF2_COLLECTION_NAME,
                WOFF2_COLLECTION_URL,
                WOFF2_COLLECTION_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned WOFF2 collection fixture: {error}")
            })
        })
        .as_path()
}

/// Download and verify the bitmap-only color SFNT used to check that exact
/// platform selection retains Swash's scalable color-bitmap replay path.
#[must_use]
pub fn noto_color_emoji_2_051() -> &'static Path {
    NOTO_COLOR_EMOJI_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_COLOR_EMOJI_NAME,
                NOTO_COLOR_EMOJI_URL,
                NOTO_COLOR_EMOJI_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned Noto Color Emoji test font: {error}")
            })
        })
        .as_path()
}

/// Download and verify the COLRv1 build of Noto Color Emoji.
///
/// Its emoji glyphs are painted by COLR version 1 paint graphs and their
/// `glyf` outlines are empty, so it draws nothing unless the rasterizer
/// traverses `BaseGlyphList`.  Unlike the bitmap build, this face also gives
/// the monochrome-outline fallback nothing to show.
#[must_use]
pub fn noto_color_emoji_colrv1() -> &'static Path {
    NOTO_COLOR_EMOJI_COLRV1_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_COLOR_EMOJI_COLRV1_NAME,
                NOTO_COLOR_EMOJI_COLRV1_URL,
                NOTO_COLOR_EMOJI_COLRV1_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned COLRv1 Noto Color Emoji test font: {error}")
            })
        })
        .as_path()
}

/// Nabla: a variable COLRv1 display face (axes `EDPT` and `EHLT`).
///
/// Its paint graph varies — the `EDPT` axis drives which of the P0..P3
/// isometric layers render — so it is the fixture for color glyphs that must
/// follow a resolved variable-font instance.  It also carries an `SVG `
/// table, giving a second real document to render.
#[must_use]
pub fn nabla_color_colrv1() -> &'static Path {
    NABLA_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(NABLA_NAME, NABLA_URL, NABLA_SHA256)
                .unwrap_or_else(|error| panic!("failed to prepare pinned Nabla test font: {error}"))
        })
        .as_path()
}

/// The author's static Thin face. A process-local catalog containing only this
/// file checks nearest-weight matching without any installed Regular sibling.
#[must_use]
pub fn mplus_1_code_thin() -> &'static Path {
    MPLUS_1_CODE_THIN_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                MPLUS_1_CODE_THIN_NAME,
                MPLUS_1_CODE_THIN_URL,
                MPLUS_1_CODE_THIN_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned M PLUS Thin test font: {error}")
            })
        })
        .as_path()
}

/// Noto Sans Mono's variable build: one face whose `wght` axis spans
/// 100..900.  Unlike an installed static family (each face a discrete
/// weight), an in-range request here must resolve to a synthesized instance
/// weight, so tests can exercise variable-family weight preservation on any
/// host regardless of which Noto package its fontconfig sees.
#[must_use]
pub fn noto_sans_mono_variable() -> &'static Path {
    NOTO_SANS_MONO_VARIABLE_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_SANS_MONO_VARIABLE_NAME,
                NOTO_SANS_MONO_VARIABLE_URL,
                NOTO_SANS_MONO_VARIABLE_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned variable Noto Sans Mono test font: {error}")
            })
        })
        .as_path()
}

/// Pinned Arabic shaping face for contextual-joining and bidi row tests.
/// The catalog fixture must select this file explicitly rather than an
/// installed Noto sibling whose advances may differ between hosts.
#[must_use]
pub fn noto_sans_arabic_variable() -> &'static Path {
    NOTO_SANS_ARABIC_VARIABLE_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_SANS_ARABIC_VARIABLE_NAME,
                NOTO_SANS_ARABIC_VARIABLE_URL,
                NOTO_SANS_ARABIC_VARIABLE_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned variable Noto Sans Arabic test font: {error}")
            })
        })
        .as_path()
}

/// Pinned Hebrew shaping face used alongside the Arabic, CJK, Latin and
/// emoji fixtures, so a Unicode row never selects an installed fallback.
#[must_use]
pub fn noto_sans_hebrew_variable() -> &'static Path {
    NOTO_SANS_HEBREW_VARIABLE_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_SANS_HEBREW_VARIABLE_NAME,
                NOTO_SANS_HEBREW_VARIABLE_URL,
                NOTO_SANS_HEBREW_VARIABLE_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned variable Noto Sans Hebrew test font: {error}")
            })
        })
        .as_path()
}

/// Pinned proportional serif face for rich-face row and boundary-string tests.
#[must_use]
pub fn noto_serif_variable() -> &'static Path {
    NOTO_SERIF_VARIABLE_FIXTURE
        .get_or_init(|| {
            prepare_pinned_file(
                NOTO_SERIF_VARIABLE_NAME,
                NOTO_SERIF_VARIABLE_URL,
                NOTO_SERIF_VARIABLE_SHA256,
            )
            .unwrap_or_else(|error| {
                panic!("failed to prepare pinned Noto Serif test font: {error}")
            })
        })
        .as_path()
}

/// The Ubuntu and DejaVu faces the Linux GUI job installs.
#[derive(Clone, Debug)]
pub struct DesktopFontFixtures {
    directory: PathBuf,
}

impl DesktopFontFixtures {
    /// Directory containing exactly the pinned faces. Point a fontconfig
    /// `<dir>` at this directory and at nothing else.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// Download and verify the desktop font set the Linux GUI CI job installs.
///
/// The desktop-font suites capture pixel grids ("Ubuntu Mono 13" is a 10x20
/// cell, "DejaVu Sans Mono 12" 10x20), so resolving whatever Ubuntu or DejaVu
/// build the host happens to carry — the pre-0.869 Ubuntu Mono was 9x18, and
/// fontconfig 2.18 ranks Noto above DejaVu — makes those suites host tests.
/// The returned directory holds the exact 0.869 and 2.37 faces, verified by
/// per-file SHA-256 after the hash-verified archives are unpacked.
#[must_use]
pub fn desktop_fonts() -> &'static DesktopFontFixtures {
    DESKTOP_FONTS_FIXTURE.get_or_init(|| {
        prepare_desktop_fonts().unwrap_or_else(|error| panic!("desktop font fixtures: {error}"))
    })
}

fn prepare_desktop_fonts() -> Result<DesktopFontFixtures, FixtureError> {
    let cache_root = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&cache_root)?;
    let lock_path = cache_root.join(".desktop-fonts.lock");
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;
    let result = prepare_desktop_fonts_locked(&cache_root);
    FileExt::unlock(&lock).map_err(|source| FixtureError::Io {
        path: lock_path,
        source,
    })?;
    result
}

fn prepare_desktop_fonts_locked(cache_root: &Path) -> Result<DesktopFontFixtures, FixtureError> {
    let directory = cache_root.join("desktop-fonts");
    if desktop_font_files_match(&directory)? {
        return Ok(DesktopFontFixtures { directory });
    }
    let ubuntu_archive = neomacs_infra::pinned::pinned_file_with_limit(
        &format!("{UBUNTU_FONTS_RELEASE}.orig.tar.gz"),
        UBUNTU_FONTS_ARCHIVE_URL,
        UBUNTU_FONTS_ARCHIVE_SHA256,
        64 * 1024 * 1024,
    )
    .map_err(|source| FixtureError::Fixture {
        name: format!("{UBUNTU_FONTS_RELEASE}.orig.tar.gz"),
        source,
    })?;
    let dejavu_archive = neomacs_infra::pinned::pinned_file_with_limit(
        &format!("{DEJAVU_FONTS_RELEASE}.zip"),
        DEJAVU_FONTS_ARCHIVE_URL,
        DEJAVU_FONTS_ARCHIVE_SHA256,
        64 * 1024 * 1024,
    )
    .map_err(|source| FixtureError::Fixture {
        name: format!("{DEJAVU_FONTS_RELEASE}.zip"),
        source,
    })?;
    if directory.exists() {
        fs::remove_dir_all(&directory).map_err(|source| FixtureError::Io {
            path: directory.clone(),
            source,
        })?;
    }
    create_dir_all(&directory)?;
    for (name, sha256) in UBUNTU_FONTS {
        let member = Path::new(UBUNTU_FONTS_RELEASE).join(name);
        extract_verified_tar_member(&ubuntu_archive, &member, &directory.join(name), sha256)?;
    }
    for (name, sha256) in DEJAVU_FONTS {
        let member = format!("{DEJAVU_FONTS_RELEASE}/ttf/{name}");
        extract_verified_zip_member(&dejavu_archive, &member, &directory.join(name), sha256)?;
    }
    Ok(DesktopFontFixtures { directory })
}

fn desktop_font_files_match(directory: &Path) -> Result<bool, FixtureError> {
    for (name, sha256) in UBUNTU_FONTS.into_iter().chain(DEJAVU_FONTS) {
        let path = directory.join(name);
        if !path.is_file() || verify_sha256(&path, sha256).is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn extract_verified_tar_member(
    archive_path: &Path,
    member: &Path,
    destination: &Path,
    sha256: &'static str,
) -> Result<(), FixtureError> {
    let file = File::open(archive_path).map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    for entry in archive.entries().map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })? {
        let mut entry = entry.map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
        let path = entry.path().map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
        if path.as_ref() != member {
            continue;
        }
        if entry.size() > MAX_PINNED_FIXTURE_BYTES {
            return Err(FixtureError::InvalidArchiveEntry(path.into_owned()));
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|source| FixtureError::Io {
                path: archive_path.to_path_buf(),
                source,
            })?;
        return write_verified(destination, &bytes, sha256);
    }
    Err(FixtureError::MissingArchiveEntry {
        archive: archive_path.to_path_buf(),
        entry: member.display().to_string(),
    })
}

fn extract_verified_zip_member(
    archive_path: &Path,
    member: &str,
    destination: &Path,
    sha256: &'static str,
) -> Result<(), FixtureError> {
    let file = File::open(archive_path).map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|source| FixtureError::Zip {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let mut entry = match zip.by_name(member) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => {
            return Err(FixtureError::MissingArchiveEntry {
                archive: archive_path.to_path_buf(),
                entry: member.to_owned(),
            });
        }
        Err(source) => {
            return Err(FixtureError::Zip {
                path: archive_path.to_path_buf(),
                source,
            });
        }
    };
    if entry.size() > MAX_PINNED_FIXTURE_BYTES {
        return Err(FixtureError::InvalidArchiveEntry(PathBuf::from(member)));
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_PINNED_FIXTURE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
    write_verified(destination, &bytes, sha256)
}

fn write_verified(
    destination: &Path,
    bytes: &[u8],
    expected: &'static str,
) -> Result<(), FixtureError> {
    let actual = sha256_bytes(bytes);
    if actual != expected {
        return Err(FixtureError::Checksum {
            path: destination.to_path_buf(),
            expected,
            actual,
        });
    }
    let partial = destination.with_extension("partial");
    write_all(&partial, bytes)?;
    fs::rename(&partial, destination).map_err(|source| FixtureError::Io {
        path: destination.to_path_buf(),
        source,
    })
}

/// Exact reporter family, published by the Nerd Font patch maintainer. The
/// release archive and its extracted Regular face are independently verified.
#[must_use]
pub fn lxgw_wenkai_nerd_regular_1_522() -> &'static Path {
    LXGW_NERD_REGULAR_FIXTURE
        .get_or_init(|| {
            prepare_lxgw_nerd_regular().unwrap_or_else(|error| {
                panic!("failed to prepare pinned LXGW WenKai Nerd test font: {error}")
            })
        })
        .as_path()
}

fn prepare_lxgw_nerd_regular() -> Result<PathBuf, FixtureError> {
    let cache = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&cache)?;
    let lock_path = cache.join(format!(".{LXGW_NERD_RELEASE}.lock"));
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;
    let result = (|| {
        let archive_path = neomacs_infra::pinned::pinned_file_with_limit(
            &format!("{LXGW_NERD_RELEASE}.tar.gz"),
            LXGW_NERD_ARCHIVE_URL,
            LXGW_NERD_ARCHIVE_SHA256,
            MAX_LXGW_ARCHIVE_BYTES,
        )
        .map_err(|source| FixtureError::Fixture {
            name: format!("{LXGW_NERD_RELEASE}.tar.gz"),
            source,
        })?;
        let directory = cache.join(LXGW_NERD_RELEASE);
        create_dir_all(&directory)?;
        let destination = directory.join(LXGW_NERD_REGULAR);
        if destination.is_file() && verify_sha256(&destination, LXGW_NERD_REGULAR_SHA256).is_ok() {
            return Ok(destination);
        }
        let file = File::open(&archive_path).map_err(|source| FixtureError::Io {
            path: archive_path.clone(),
            source,
        })?;
        let mut archive = tar::Archive::new(GzDecoder::new(file));
        let entries = archive.entries().map_err(|source| FixtureError::Io {
            path: archive_path.clone(),
            source,
        })?;
        let expected = Path::new(LXGW_NERD_RELEASE).join(LXGW_NERD_REGULAR);
        for entry in entries {
            let mut entry = entry.map_err(|source| FixtureError::Io {
                path: archive_path.clone(),
                source,
            })?;
            let path = entry.path().map_err(|source| FixtureError::Io {
                path: archive_path.clone(),
                source,
            })?;
            if path.as_ref() != expected {
                continue;
            }
            if entry.size() > MAX_PINNED_FIXTURE_BYTES {
                return Err(FixtureError::InvalidArchiveEntry(path.into_owned()));
            }
            let partial = directory.join(format!(".{LXGW_NERD_REGULAR}.partial"));
            let mut output = File::create(&partial).map_err(|source| FixtureError::Io {
                path: partial.clone(),
                source,
            })?;
            io::copy(&mut entry, &mut output).map_err(|source| FixtureError::Io {
                path: partial.clone(),
                source,
            })?;
            verify_sha256(&partial, LXGW_NERD_REGULAR_SHA256)?;
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
            return Ok(destination);
        }
        Err(FixtureError::MissingArchiveEntry {
            archive: archive_path,
            entry: expected.display().to_string(),
        })
    })();
    FileExt::unlock(&lock).map_err(|source| FixtureError::Io {
        path: lock_path,
        source,
    })?;
    result
}

#[derive(Debug, Error)]
enum FixtureError {
    #[error("ZIP failure at {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("I/O failure at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("pinned fixture {name}: {source}")]
    Fixture {
        name: String,
        #[source]
        source: neomacs_infra::pinned::PinnedError,
    },
    #[error("SHA-256 mismatch for {path}: expected {expected}, got {actual}")]
    Checksum {
        path: PathBuf,
        expected: &'static str,
        actual: String,
    },
    #[error("archive {archive} did not contain {entry}")]
    MissingArchiveEntry { archive: PathBuf, entry: String },
    #[error("invalid archive entry path: {0}")]
    InvalidArchiveEntry(PathBuf),
}

fn prepare_spleen_fixtures() -> Result<SpleenFixtures, FixtureError> {
    let cache_root = workspace_root().join("tmp/font-fixtures");
    create_dir_all(&cache_root)?;

    let lock_path = cache_root.join(format!(".{RELEASE_NAME}.lock"));
    let lock = open_lock(&lock_path)?;
    lock.lock().map_err(|source| FixtureError::Io {
        path: lock_path.clone(),
        source,
    })?;

    let result = prepare_spleen_fixtures_locked(&cache_root);
    FileExt::unlock(&lock).map_err(|source| FixtureError::Io {
        path: lock_path,
        source,
    })?;
    result
}

fn prepare_pinned_file(
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
) -> Result<PathBuf, FixtureError> {
    neomacs_infra::pinned::pinned_file(name, url, sha256).map_err(|source| FixtureError::Fixture {
        name: name.to_owned(),
        source,
    })
}

fn prepare_spleen_fixtures_locked(cache_root: &Path) -> Result<SpleenFixtures, FixtureError> {
    let archive = neomacs_infra::pinned::pinned_file(
        &format!("{RELEASE_NAME}.tar.gz"),
        RELEASE_URL,
        RELEASE_SHA256,
    )
    .map_err(|source| FixtureError::Fixture {
        name: format!("{RELEASE_NAME}.tar.gz"),
        source,
    })?;

    let fixture_root = cache_root.join(RELEASE_NAME);
    if !all_extracted_files_match(&fixture_root)? {
        extract_verified_fonts(&archive, &fixture_root)?;
    }
    ensure_gzipped_pcf(&fixture_root)?;

    Ok(SpleenFixtures { root: fixture_root })
}

fn open_lock(path: &Path) -> Result<File, FixtureError> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| FixtureError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn create_dir_all(path: &Path) -> Result<(), FixtureError> {
    fs::create_dir_all(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn all_extracted_files_match(root: &Path) -> Result<bool, FixtureError> {
    for font in ARCHIVED_FONTS {
        let path = root.join(font.name);
        if !path.exists() || verify_sha256(&path, font.sha256).is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn extract_verified_fonts(archive_path: &Path, root: &Path) -> Result<(), FixtureError> {
    if root.exists() {
        fs::remove_dir_all(root).map_err(|source| FixtureError::Io {
            path: root.to_path_buf(),
            source,
        })?;
    }
    create_dir_all(root)?;

    let archive_file = File::open(archive_path).map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let mut wanted: BTreeMap<&str, &str> = ARCHIVED_FONTS
        .iter()
        .map(|font| (font.name, font.sha256))
        .collect();
    let entries = archive.entries().map_err(|source| FixtureError::Io {
        path: archive_path.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let mut entry = entry.map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
        let entry_path = entry.path().map_err(|source| FixtureError::Io {
            path: archive_path.to_path_buf(),
            source,
        })?;
        let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if wanted.remove(name).is_none() {
            continue;
        }
        let expected = Path::new(RELEASE_NAME).join(name);
        if entry_path.as_ref() != expected {
            return Err(FixtureError::InvalidArchiveEntry(entry_path.into_owned()));
        }
        let destination = root.join(name);
        let mut output = File::create(&destination).map_err(|source| FixtureError::Io {
            path: destination.clone(),
            source,
        })?;
        io::copy(&mut entry, &mut output).map_err(|source| FixtureError::Io {
            path: destination,
            source,
        })?;
    }

    if let Some((name, _)) = wanted.first_key_value() {
        return Err(FixtureError::MissingArchiveEntry {
            archive: archive_path.to_path_buf(),
            entry: format!("{RELEASE_NAME}/{name}"),
        });
    }
    for font in ARCHIVED_FONTS {
        verify_sha256(&root.join(font.name), font.sha256)?;
    }
    Ok(())
}

fn ensure_gzipped_pcf(root: &Path) -> Result<(), FixtureError> {
    let source_path = root.join("spleen-8x16.pcf");
    let gzip_path = root.join("spleen-8x16.pcf.gz");
    if gzip_path.exists() && gzip_expands_to(&gzip_path, &source_path)? {
        return Ok(());
    }

    let source = File::open(&source_path).map_err(|source| FixtureError::Io {
        path: source_path.clone(),
        source,
    })?;
    let partial = gzip_path.with_extension("gz.partial");
    let output = File::create(&partial).map_err(|source| FixtureError::Io {
        path: partial.clone(),
        source,
    })?;
    let mut encoder = GzEncoder::new(output, Compression::best());
    io::copy(&mut io::BufReader::new(source), &mut encoder).map_err(|source| FixtureError::Io {
        path: partial.clone(),
        source,
    })?;
    encoder.finish().map_err(|source| FixtureError::Io {
        path: partial.clone(),
        source,
    })?;
    if gzip_path.exists() {
        fs::remove_file(&gzip_path).map_err(|source| FixtureError::Io {
            path: gzip_path.clone(),
            source,
        })?;
    }
    fs::rename(&partial, &gzip_path).map_err(|source| FixtureError::Io {
        path: gzip_path.clone(),
        source,
    })?;
    if !gzip_expands_to(&gzip_path, &source_path)? {
        return Err(FixtureError::Checksum {
            path: gzip_path,
            expected: "the verified PCF payload",
            actual: "a different decompressed payload".to_owned(),
        });
    }
    Ok(())
}

fn gzip_expands_to(gzip_path: &Path, source_path: &Path) -> Result<bool, FixtureError> {
    let expected = read_all(source_path)?;
    let compressed = File::open(gzip_path).map_err(|source| FixtureError::Io {
        path: gzip_path.to_path_buf(),
        source,
    })?;
    let mut actual = Vec::new();
    GzDecoder::new(compressed)
        .read_to_end(&mut actual)
        .map_err(|source| FixtureError::Io {
            path: gzip_path.to_path_buf(),
            source,
        })?;
    Ok(actual == expected)
}

fn verify_sha256(path: &Path, expected: &'static str) -> Result<(), FixtureError> {
    let actual = sha256_bytes(&read_all(path)?);
    if actual == expected {
        Ok(())
    } else {
        Err(FixtureError::Checksum {
            path: path.to_path_buf(),
            expected,
            actual,
        })
    }
}

fn read_all(path: &Path) -> Result<Vec<u8>, FixtureError> {
    fs::read(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_all(path: &Path, bytes: &[u8]) -> Result<(), FixtureError> {
    let mut file = File::create(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(bytes).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests;

mod layout_fonts;
pub use layout_fonts::*;
