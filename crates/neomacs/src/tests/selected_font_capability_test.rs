use super::super::*;

fn native_catalog_font(key: &str) -> neomacs_layout_engine::font_backend::PlatformFontMatch {
    use neomacs_display_protocol::font::{
        FontBackendKind, FontMemoryAsset, FontOutlineAsset, ResolvedFontIdentity,
    };
    use neomacs_layout_engine::font_backend::{
        PlatformFontMatch, PlatformFontMetadata, PlatformFontSize,
    };
    let identity =
        ResolvedFontIdentity::from_memory(FontBackendKind::CoreText, key.to_owned(), 0, None);
    PlatformFontMatch {
        asset: FontOutlineAsset::Memory(
            FontMemoryAsset::new(key, Arc::new(vec![1, 2, 3]), 0).unwrap(),
        ),
        identity,
        metadata: PlatformFontMetadata {
            foundry: None,
            adstyle: None,
            family: "Catalog Fixture".to_owned(),
            weight: Some(400),
            slant: neovm_core::face::FontSlant::Normal,
            width: Some(neovm_core::face::FontWidth::Normal),
            spacing: Some(100),
            design_metrics: None,
            size: PlatformFontSize::Scalable,
        },
    }
}

fn listed_catalog_font(
    font: &neomacs_layout_engine::font_backend::PlatformFontMatch,
) -> neomacs_layout_engine::font::resolver::ListedFontCandidate {
    use neomacs_layout_engine::font_backend::{
        PlatformFontCandidate, PlatformFontCandidateLocator,
    };
    neomacs_layout_engine::font::resolver::ListedFontCandidate {
        candidate: PlatformFontCandidate {
            identity: font.identity.clone(),
            locator: match &font.asset {
                neomacs_display_protocol::font::FontOutlineAsset::File(asset) => {
                    PlatformFontCandidateLocator::File(asset.clone())
                }
                neomacs_display_protocol::font::FontOutlineAsset::Memory(_) => {
                    PlatformFontCandidateLocator::Native
                }
            },
            metadata: font.metadata.clone(),
        },
        registry: Some("iso10646-1".to_owned()),
        driver: Some(neovm_core::emacs_core::intern::intern("ns")),
    }
}

fn core_listed_font_entities(
    catalog: &mut FontEntityCatalog,
    entities: Vec<neomacs_layout_engine::font::resolver::ListedFontCandidate>,
    maximum: Option<usize>,
    filter: Option<&mut neovm_core::emacs_core::eval::FontListFilter<'_>>,
) -> Result<
    Vec<neovm_core::emacs_core::eval::ListedFontEntity>,
    neovm_core::emacs_core::eval::FontListError,
> {
    core_visit_listed_font_entities(catalog, maximum, filter, |visit| {
        for entity in entities {
            if visit(entity).is_break() {
                break;
            }
        }
    })
}

fn list_catalog_font(
    catalog: &mut FontEntityCatalog,
    entity: neomacs_layout_engine::font::resolver::ListedFontCandidate,
) -> ResolvedFontSpecMatch {
    core_listed_font_entities(catalog, vec![entity], None, None)
        .unwrap()
        .remove(0)
        .font
}

#[test]
fn listed_native_font_stays_unmaterialized_until_selected() {
    let mut catalog = FontEntityCatalog::default();
    let font = native_catalog_font("native:catalog-fixture");
    let listed = list_catalog_font(&mut catalog, listed_catalog_font(&font));
    let handle = listed.coverage_handle.expect("listed entity handle");
    assert_eq!(listed.family.as_utf8_str(), Some("Catalog Fixture"));
    assert!(
        catalog.get(handle).is_none(),
        "listing must retain metadata, not a materialized native font"
    );
    assert_eq!(catalog.intern(font.clone()), Some(handle));
    assert_eq!(catalog.get(handle), Some(&font));
    let relisted = list_catalog_font(&mut catalog, listed_catalog_font(&font));
    assert_eq!(relisted.coverage_handle, Some(handle));
    assert_eq!(
        catalog.get(handle),
        Some(&font),
        "listing does not downgrade a used font"
    );
}

#[test]
fn listed_font_materialization_promotes_only_the_exact_entity_handle() {
    let mut catalog = FontEntityCatalog::default();
    let first = native_catalog_font("native:first-instance");
    let second = native_catalog_font("native:second-instance");
    let first_handle = list_catalog_font(&mut catalog, listed_catalog_font(&first))
        .coverage_handle
        .unwrap();
    let second_handle = list_catalog_font(&mut catalog, listed_catalog_font(&second))
        .coverage_handle
        .unwrap();
    assert_ne!(first_handle, second_handle);
    assert!(!catalog.promote(first_handle, second.clone()));
    assert!(catalog.discovered(first_handle).is_some());
    assert!(catalog.get(first_handle).is_none());
    assert!(catalog.promote(first_handle, first.clone()));
    assert_eq!(catalog.get(first_handle), Some(&first));
    assert!(catalog.discovered(first_handle).is_none());
    assert!(catalog.discovered(second_handle).is_some());
    assert!(catalog.get(second_handle).is_none());
}

#[test]
fn selected_font_capability_reuse_preserves_the_complete_opened_font() {
    // Exercise native selection and the exact projection used by face-font,
    // without a window constructor, renderer or event loop.
    let mut metrics = FontMetricsService::new();
    let selected = metrics
        .select_font_for_char('M', "Monospace", 400, false, 16.0)
        .expect("native monospace selection");
    let expected = core_opened_font_from_selection(selected, font_otf_capability_for_file);
    for _ in 0..30 {
        let selected = metrics
            .select_font_for_char('M', "Monospace", 400, false, 16.0)
            .expect("same native selection");
        let cached = core_opened_font_from_selection(selected, |file, face_index| {
            metrics
                .otf_capability_for_file(file, face_index)
                .map(core_font_otf_capability)
        });
        assert_eq!(
            cached, expected,
            "names, metrics, identity and both OTF tables stay unchanged"
        );
    }
    metrics.clear_caches();
    let selected = metrics
        .select_font_for_char('M', "Monospace", 400, false, 16.0)
        .expect("selection after native invalidation");
    let reopened = core_opened_font_from_selection(selected, |file, face_index| {
        metrics
            .otf_capability_for_file(file, face_index)
            .map(core_font_otf_capability)
    });
    assert_eq!(reopened, expected);
}

#[test]
fn listed_font_filter_previews_metadata_and_registers_only_accepted_limit() {
    let mut catalog = FontEntityCatalog::default();
    let mut rejected = native_catalog_font("native:ignored");
    rejected.metadata.family = "Ignored Fixture".to_owned();
    let mut accepted = native_catalog_font("native:accepted");
    accepted.metadata.adstyle = Some("ja".to_owned());
    let unvisited = native_catalog_font("native:unvisited");
    let candidates = [&rejected, &accepted, &unvisited]
        .into_iter()
        .map(listed_catalog_font)
        .collect();
    let mut visited = 0;
    let mut filter = |listed: &neovm_core::emacs_core::eval::ListedFontEntity| {
        visited += 1;
        assert_eq!(
            listed.font.coverage_handle, None,
            "filtering must precede catalog registration"
        );
        Ok(listed.font.family.as_utf8_str() != Some("Ignored Fixture"))
    };
    let listed =
        core_listed_font_entities(&mut catalog, candidates, Some(1), Some(&mut filter)).unwrap();
    assert_eq!(visited, 2);
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].adstyle.as_ref().and_then(LispString::as_utf8_str),
        Some("ja")
    );
    assert_eq!(catalog.entries.len(), 1);
    let handle = listed[0]
        .font
        .coverage_handle
        .expect("accepted entity handle");
    assert_eq!(
        catalog.discovered(handle).unwrap().identity,
        accepted.identity
    );
    assert!(catalog.get(handle).is_none());
    let empty = core_listed_font_entities(
        &mut catalog,
        vec![listed_catalog_font(&unvisited)],
        Some(0),
        None,
    )
    .unwrap();
    assert!(empty.is_empty());
    assert_eq!(
        catalog.entries.len(),
        1,
        "zero limit does not register a candidate"
    );
}

#[test]
fn listed_font_acceptance_limit_stops_candidate_production() {
    let mut catalog = FontEntityCatalog::default();
    let fonts = [
        native_catalog_font("native:reject"),
        native_catalog_font("native:accept"),
        native_catalog_font("native:unvisited"),
    ];
    let mut produced = 0;
    let mut accepted = 0;
    let mut filter = |listed: &neovm_core::emacs_core::eval::ListedFontEntity| {
        assert_eq!(listed.font.coverage_handle, None);
        accepted += 1;
        Ok(accepted == 2)
    };
    let listed =
        core_visit_listed_font_entities(&mut catalog, Some(1), Some(&mut filter), |visit| {
            for font in &fonts {
                produced += 1;
                if visit(listed_catalog_font(font)).is_break() {
                    break;
                }
            }
        })
        .unwrap();
    assert_eq!(
        produced, 2,
        "accepted MAX must stop lower candidate production"
    );
    assert_eq!(accepted, 2);
    assert_eq!(listed.len(), 1);
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(
        catalog
            .discovered(listed[0].font.coverage_handle.unwrap())
            .unwrap()
            .identity,
        fonts[1].identity
    );
    let empty = core_visit_listed_font_entities(&mut catalog, Some(0), None, |_| {
        panic!("zero limit must not invoke the producer")
    })
    .unwrap();
    assert!(empty.is_empty());
}

#[test]
fn listed_font_filter_error_stops_candidate_production_with_original_flow() {
    use neovm_core::emacs_core::error::{FlowRef, signal};
    use neovm_core::emacs_core::eval::FontListError;
    let mut catalog = FontEntityCatalog::default();
    let font = native_catalog_font("native:error");
    let mut produced = 0;
    let mut filter = |_: &neovm_core::emacs_core::eval::ListedFontEntity| {
        Err(signal("invalid-regexp", vec![Value::fixnum(672)]))
    };
    let error = core_visit_listed_font_entities(&mut catalog, None, Some(&mut filter), |visit| {
        for _ in 0..3 {
            produced += 1;
            if visit(listed_catalog_font(&font)).is_break() {
                break;
            }
        }
    })
    .unwrap_err();
    assert_eq!(
        produced, 1,
        "a filter error must stop lower candidate production"
    );
    assert!(catalog.entries.is_empty());
    let FontListError::Filter(flow) = error else {
        panic!("original Lisp flow required")
    };
    let FlowRef::Signal(signal) = flow.kind() else {
        panic!("signal flow required")
    };
    assert_eq!(signal.symbol_name(), "invalid-regexp");
    assert_eq!(signal.data, vec![Value::fixnum(672)]);
}

fn file_catalog_font(
    file: &str,
    selector: u32,
    coordinates: Vec<neomacs_display_protocol::font::FontVariationCoord>,
) -> neomacs_layout_engine::font_backend::PlatformFontMatch {
    use neomacs_display_protocol::font::{FontFileAsset, FontOutlineAsset, ResolvedFontIdentity};
    let mut font = native_catalog_font("unused:file-fixture");
    font.identity =
        ResolvedFontIdentity::from_file_with_variations(file, selector, None, coordinates);
    font.asset = FontOutlineAsset::File(FontFileAsset::from_identity(&font.identity).unwrap());
    font
}

#[cfg(target_os = "linux")]
#[test]
fn listed_base_and_named_file_instances_keep_distinct_discovery_handles() {
    let mut catalog = FontEntityCatalog::default();
    let file = neomacs_test_fonts::noto_sans_mono_variable()
        .to_str()
        .unwrap();
    let base = file_catalog_font(file, 0, Vec::new());
    let named = file_catalog_font(file, 0x0001_0000, Vec::new());
    assert_eq!(
        base.identity, named.identity,
        "unresolved canonical keys lack named-instance coordinates"
    );
    let base_handle = list_catalog_font(&mut catalog, listed_catalog_font(&base))
        .coverage_handle
        .unwrap();
    let named_handle = list_catalog_font(&mut catalog, listed_catalog_font(&named))
        .coverage_handle
        .unwrap();
    assert_ne!(
        base_handle, named_handle,
        "full native selectors must distinguish discovered entities"
    );
    assert_eq!(
        catalog
            .discovered(base_handle)
            .unwrap()
            .identity
            .backend_selector(),
        0
    );
    assert_eq!(
        catalog
            .discovered(named_handle)
            .unwrap()
            .identity
            .backend_selector(),
        0x0001_0000
    );
}

#[cfg(target_os = "linux")]
#[test]
fn listed_named_file_instance_preserves_handle_when_materialization_enriches_axes() {
    let mut catalog = FontEntityCatalog::default();
    let file = neomacs_test_fonts::noto_sans_mono_variable()
        .to_str()
        .unwrap();
    let discovered = file_catalog_font(file, 0x0001_0000, Vec::new());
    let handle = list_catalog_font(&mut catalog, listed_catalog_font(&discovered))
        .coverage_handle
        .unwrap();
    let resolved = FontMetricsService::new()
        .materialize_font_candidate(listed_catalog_font(&discovered).candidate)
        .expect("exact pinned variable-font named instance materializes");
    assert!(
        !resolved.identity.variation_coords.is_empty(),
        "actual finalizer must enrich named-instance axes"
    );
    assert_eq!(resolved.identity.backend_selector(), 0x0001_0000);
    let mut other_selector = resolved.clone();
    other_selector.identity =
        neomacs_display_protocol::font::ResolvedFontIdentity::from_file_with_variations(
            file,
            0x0002_0000,
            None,
            resolved.identity.variation_coords.as_slice().to_vec(),
        );
    assert!(!catalog.promote(handle, other_selector));
    assert!(
        catalog.promote(handle, resolved.clone()),
        "exact named descriptor may gain canonical coordinates"
    );
    assert_eq!(catalog.get(handle), Some(&resolved));
    assert_eq!(catalog.intern(resolved), Some(handle));
    assert_eq!(
        list_catalog_font(&mut catalog, listed_catalog_font(&discovered)).coverage_handle,
        Some(handle)
    );
}
