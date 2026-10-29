//! Immutable, explicit native catalogues for conformance tests.
//! The configuration is never made process-current. All queries and their
//! caches are borrowed from the same owner; native access is serialized.
use super::*;
use std::collections::{BTreeSet, HashMap};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("cannot create an isolated fontconfig configuration")]
    Create,
    #[error("NUL in pinned font path: {0}")]
    FontPath(PathBuf),
    #[error("cannot add pinned font to isolated fontconfig: {0}")]
    AddFont(PathBuf),
    #[error("cannot parse isolated fontconfig rules")]
    Rules,
    #[error("cannot build isolated fontconfig font sets")]
    Build,
    #[error("cannot disable isolated fontconfig rescans")]
    Rescan,
}

#[derive(Debug)]
pub(super) enum ConfigAccessError {
    SystemUnavailable,
}

#[derive(Debug)]
pub(super) struct OwnedFcConfig(NonNull<fontconfig_sys::FcConfig>);

// SAFETY: the initialized configuration owns native data, not thread-affine
// GUI objects or Rust callbacks. It is transferred exclusively into a Mutex;
// every subsequent native access (including reference probes) holds that Mutex.
unsafe impl Send for OwnedFcConfig {}
static_assertions::assert_impl_all!(OwnedFcConfig: Send);
static_assertions::assert_not_impl_any!(OwnedFcConfig: Sync);

impl Drop for OwnedFcConfig {
    fn drop(&mut self) {
        // SAFETY: this owner was created by FcConfigCreate and has not been
        // made process-current or exported. Last Arc drop has no live guard.
        unsafe { fontconfig_sys::FcConfigDestroy(self.0.as_ptr()) };
    }
}

struct ConfigData {
    native: Arc<Mutex<OwnedFcConfig>>,
    caches: Mutex<FontconfigCaches>,
    // Foundry queries are bounded by this immutable file set, not arbitrary
    // per-character strings. The answers cannot cross a configuration owner.
    files: BTreeSet<PathBuf>,
    foundries: Mutex<HashMap<PathBuf, Option<String>>>,
}

/// One validated, immutable native font catalogue and its bounded query cache.
#[derive(Clone)]
pub struct TestFontconfig(Arc<ConfigData>);

impl std::fmt::Debug for TestFontconfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestFontconfig")
            .field("files", &self.0.files.len())
            .finish_non_exhaustive()
    }
}

static_assertions::assert_impl_all!(TestFontconfig: Send, Sync);
static_assertions::assert_eq_size!(TestFontconfig, usize);

// The sys crate does not yet declare the memory parser. This public Fontconfig
// API avoids a temporary config path and all process/global configuration edits.
#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigParseAndLoadFromMemory(
        config: *mut fontconfig_sys::FcConfig,
        buffer: *const fontconfig_sys::FcChar8,
        complain: fontconfig_sys::FcBool,
    ) -> fontconfig_sys::FcBool;
}

fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl TestFontconfig {
    pub(crate) fn new(
        files: &[PathBuf],
        monospace: &FontFamilyName,
        sans_serif: &FontFamilyName,
        serif: &FontFamilyName,
    ) -> Result<Self, ConfigError> {
        // SAFETY: creating a new independent configuration has no borrowed input.
        let pointer =
            NonNull::new(unsafe { fontconfig_sys::FcConfigCreate() }).ok_or(ConfigError::Create)?;
        let config = OwnedFcConfig(pointer);
        // Explicit aliases name real loaded faces; there are no includes,
        // system directories, cache directories or host resource files.
        let rules = format!(
            "<fontconfig><alias binding=\"same\"><family>monospace</family><prefer><family>{}</family></prefer></alias><alias binding=\"same\"><family>sans-serif</family><prefer><family>{}</family></prefer></alias><alias binding=\"same\"><family>serif</family><prefer><family>{}</family></prefer></alias></fontconfig>",
            xml_text(monospace.as_str()),
            xml_text(sans_serif.as_str()),
            xml_text(serif.as_str()),
        );
        let rules = CString::new(rules).map_err(|_| ConfigError::Rules)?;
        // SAFETY: live exclusive config and NUL-terminated XML buffer. Parsing
        // retains its own native rules, not a pointer into the CString.
        if unsafe { FcConfigParseAndLoadFromMemory(pointer.as_ptr(), rules.as_ptr().cast(), 1) }
            == 0
        {
            return Err(ConfigError::Rules);
        }
        let files: BTreeSet<_> = files.iter().cloned().collect();
        for file in &files {
            let name = CString::new(file.as_os_str().as_bytes())
                .map_err(|_| ConfigError::FontPath(file.clone()))?;
            // SAFETY: configuration is exclusive and name is live throughout
            // native scanning. The immutable fixture file remains owned on disk.
            if unsafe {
                fontconfig_sys::FcConfigAppFontAddFile(pointer.as_ptr(), name.as_ptr().cast())
            } == 0
            {
                return Err(ConfigError::AddFont(file.clone()));
            }
        }
        // SAFETY: initialized exclusive configuration; zero disables rescans
        // of this immutable application-font catalogue, not global fontconfig.
        if unsafe { fontconfig_sys::FcConfigSetRescanInterval(pointer.as_ptr(), 0) } == 0 {
            return Err(ConfigError::Rescan);
        }
        // SAFETY: every file and rule has been added to this exclusive config.
        if unsafe { fontconfig_sys::FcConfigBuildFonts(pointer.as_ptr()) } == 0 {
            return Err(ConfigError::Build);
        }
        Ok(Self(Arc::new(ConfigData {
            native: Arc::new(Mutex::new(config)),
            caches: Mutex::new(FontconfigCaches::default()),
            files,
            foundries: Mutex::new(HashMap::new()),
        })))
    }

    /// A distinct cache owner over the same immutable, mutex-guarded native
    /// catalogue. Deterministic cache tests cannot be warmed by another case.
    pub(crate) fn fresh_cache_view(&self) -> Self {
        Self(Arc::new(ConfigData {
            native: Arc::clone(&self.0.native),
            caches: Mutex::new(FontconfigCaches::default()),
            files: self.0.files.clone(),
            foundries: Mutex::new(HashMap::new()),
        }))
    }

    pub(super) fn with_reference_config<R>(&self, read: impl FnOnce(ConfigBorrow<'_>) -> R) -> R {
        let access = QueryOwner::Fixture(self).acquire();
        read(access.borrow())
    }

    pub(super) fn query_uncached(
        &self,
        family: Option<&str>,
        ranges: &[(u32, u32)],
        required: Option<u32>,
        languages: &[String],
        kind: FcQueryKind,
    ) -> Vec<ListedFont> {
        self.with_reference_config(|config| {
            fc_query_candidates_uncached(config, family, ranges, required, languages, kind)
        })
    }

    pub(crate) fn list_candidates(
        &self,
        family: Option<&str>,
        ranges: &[(u32, u32)],
        required: Option<u32>,
        languages: &[String],
    ) -> Vec<ListedFont> {
        fc_list_candidates(
            QueryOwner::Fixture(self),
            family,
            ranges,
            required,
            languages,
        )
    }

    pub(crate) fn foundry_for_file(&self, file: &str) -> Option<String> {
        let path = Path::new(file);
        if !self.0.files.contains(path) {
            return None;
        }
        if let Some(foundry) = self
            .0
            .foundries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(path)
        {
            return foundry.clone();
        }
        let foundry = self.with_reference_config(|config| foundry_for_file_uncached(config, file));
        self.0
            .foundries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(path.to_owned(), foundry.clone());
        foundry
    }

    pub(crate) fn find_font_for_spec(
        &self,
        family: Option<&str>,
        registry: Option<&str>,
        language: Option<&str>,
        weight: Option<FontWeight>,
        slant: Option<FontSlant>,
        width: Option<FontWidth>,
    ) -> Option<SpecFontMatch> {
        find_font_for_spec(
            QueryOwner::Fixture(self),
            family,
            registry,
            language,
            weight,
            slant,
            width,
        )
    }

    pub(crate) fn clear_caches(&self) {
        invalidate_catalog_caches(QueryOwner::Fixture(self));
        self.0
            .foundries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ConfigBorrow<'a> {
    System,
    Fixture(&'a OwnedFcConfig),
}

impl ConfigBorrow<'_> {
    pub(super) fn native_ptr(self) -> Result<*mut fontconfig_sys::FcConfig, ConfigAccessError> {
        match self {
            Self::System => fontconfig_handle()
                .map(|_| ptr::null_mut())
                .ok_or(ConfigAccessError::SystemUnavailable),
            Self::Fixture(config) => Ok(config.0.as_ptr()),
        }
    }

    pub(super) fn apply_defaults(self, pattern: &FcPatternGuard) -> Result<(), ConfigError> {
        match self {
            Self::System => Ok(()),
            Self::Fixture(_) => {
                // FcPatternGet is untyped: an explicit LangSet, string, integer
                // or double remains authoritative regardless of its storage type.
                for default in [PatternDefault::Language, PatternDefault::Dpi] {
                    let property = default.property();
                    let mut value = std::mem::MaybeUninit::<fontconfig_sys::FcValue>::uninit();
                    // SAFETY: live pattern and writable value slot; only the
                    // return code is read, never uninitialized output data.
                    let result = unsafe {
                        fontconfig_sys::FcPatternGet(
                            pattern.0,
                            property.as_ptr(),
                            0,
                            value.as_mut_ptr(),
                        )
                    };
                    if result == fontconfig_sys::FcResultMatch {
                        continue;
                    }
                    let added = match default {
                        PatternDefault::Language => {
                            // SAFETY: live exclusive pattern; Fontconfig copies
                            // the static NUL-terminated language string.
                            unsafe {
                                fontconfig_sys::FcPatternAddString(
                                    pattern.0,
                                    property.as_ptr(),
                                    c"en".as_ptr().cast(),
                                )
                            }
                        }
                        PatternDefault::Dpi => {
                            // SAFETY: live exclusive pattern; the fixture's DPI
                            // default is used only when no value was supplied.
                            unsafe {
                                fontconfig_sys::FcPatternAddDouble(
                                    pattern.0,
                                    property.as_ptr(),
                                    96.0,
                                )
                            }
                        }
                    };
                    if added == 0 {
                        return Err(ConfigError::Rules);
                    }
                }
                Ok(())
            }
        }
    }

    fn match_one(self, family: &str) -> Option<ListedFont> {
        fc_query_candidates_uncached(self, Some(family), &[], None, &[], FcQueryKind::Match)
            .into_iter()
            .next()
    }

    pub(super) fn match_family(self, family: &str) -> Option<String> {
        match self {
            Self::System => fc_match_family(family),
            Self::Fixture(_) => self
                .match_one(family)
                .map(|candidate| candidate.matched.family),
        }
    }

    pub(super) fn match_spacing(self, family: &str) -> Option<i32> {
        match self {
            Self::System => fontconfig_handle().and_then(|fc| {
                with_matched_family_pattern(fc, family, |p| p.get_int(fontconfig::FC_SPACING).ok())
            }),
            Self::Fixture(_) => self
                .match_one(family)
                .and_then(|candidate| candidate.spacing),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PatternDefault {
    Language,
    Dpi,
}

impl PatternDefault {
    fn property(self) -> &'static CStr {
        match self {
            Self::Language => fontconfig::FC_LANG,
            Self::Dpi => fontconfig_sys::constants::FC_DPI,
        }
    }
}

/// Both resources of a query come from this one closed context.
#[derive(Clone, Copy, Debug)]
pub enum QueryOwner<'a> {
    System,
    Fixture(&'a TestFontconfig),
}

#[must_use = "the native configuration guard must remain live throughout its query"]
#[derive(Debug)]
pub(super) enum ConfigAccess<'a> {
    System,
    Fixture(std::sync::MutexGuard<'a, OwnedFcConfig>),
}

impl<'a> QueryOwner<'a> {
    pub(super) fn caches(self) -> &'a Mutex<FontconfigCaches> {
        match self {
            Self::System => fontconfig_caches(),
            Self::Fixture(config) => &config.0.caches,
        }
    }

    pub(super) fn acquire(self) -> ConfigAccess<'a> {
        match self {
            Self::System => ConfigAccess::System,
            Self::Fixture(config) => {
                ConfigAccess::Fixture(config.0.native.lock().unwrap_or_else(|p| p.into_inner()))
            }
        }
    }
}

impl ConfigAccess<'_> {
    pub(super) fn borrow(&self) -> ConfigBorrow<'_> {
        match self {
            Self::System => ConfigBorrow::System,
            Self::Fixture(guard) => ConfigBorrow::Fixture(guard),
        }
    }
}
