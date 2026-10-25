//! Nontext cache tests with independent legacy equality and linear query oracles.
use super::*;
use std::sync::{Arc, Barrier, Mutex};

fn rect(x: f32, y: f32, width: f32, height: f32) -> FrameRect {
    FrameRect::new(x, y, width, height).unwrap()
}

fn query(presentation: PresentationId, x: f32, y: f32) -> PresentedHitQuery {
    let point = crate::InteractionProjection::settled(presentation)
        .map(
            crate::GeometryPoint::<crate::RootSurfaceSpace, crate::LogicalPixels>::from_px(x, y)
                .unwrap(),
        )
        .unwrap();
    PresentedHitQuery::new(point)
}

fn cold(index: &PresentedHitIndex) {
    assert!(index.nontext_buckets.get().is_none());
}

fn cache_address(index: &PresentedHitIndex) -> Option<usize> {
    index
        .nontext_buckets
        .get()
        .map(|value| value.as_ref() as *const NontextBuckets as usize)
}

fn fixture(presentation: PresentationId) -> PresentedHitIndex {
    let w = DisplayWindowId::new(1);
    let other = DisplayWindowId::new(2);
    let mut index = PresentedHitIndex::from_parts_with_strings(
        presentation,
        vec![
            PresentedHitRegion::new(
                Some(w),
                PresentedRegionKind::TextBody,
                rect(0.0, -0.0, 40.0, 20.0),
                0,
            ),
            PresentedHitRegion::new(
                Some(other),
                PresentedRegionKind::TextBody,
                rect(0.0, 0.0, 20.0, 10.0),
                2,
            ),
            PresentedHitRegion::new(
                Some(other),
                PresentedRegionKind::TextBody,
                rect(4.0, 0.0, 20.0, 10.0),
                2,
            ),
            PresentedHitRegion::new(
                Some(w),
                PresentedRegionKind::TabLine,
                rect(0.0, 20.0, 40.0, 10.0),
                3,
            ),
            PresentedHitRegion::new(
                None,
                PresentedRegionKind::ToolBar,
                rect(0.0, 30.0, 0.0, 1.0),
                9,
            ),
            PresentedHitRegion::new(
                Some(DisplayWindowId::new(4)),
                PresentedRegionKind::TextBody,
                rect(2.0, 32.0, 25.0, 3.0),
                1,
            ),
        ],
        vec![
            PresentedTextPosition::new(w, rect(5.0, 0.0, 10.0, 10.0), 11, 0, 0),
            PresentedTextPosition::new(w, rect(8.0, 0.0, 10.0, 10.0), 12, 0, 1),
            PresentedTextPosition::new(other, rect(0.0, 0.0, 20.0, 10.0), 21, 0, 0),
            PresentedTextPosition::new(
                DisplayWindowId::new(4),
                rect(3.0, 32.5, 10.0, 2.0),
                41,
                1,
                1,
            ),
        ],
        vec![
            PresentedStringPosition::new(
                w,
                PresentedWindowChromeArea::TabLine,
                rect(0.0, 20.0, 15.0, 10.0),
                crate::GlyphStringId::new(1),
                3,
            ),
            PresentedStringPosition::new(
                w,
                PresentedWindowChromeArea::TabLine,
                rect(5.0, 20.0, 15.0, 10.0),
                crate::GlyphStringId::new(2),
                4,
            ),
        ],
    )
    .unwrap()
    .with_resize_handles(vec![
        PresentedResizeHandle::new(
            w,
            PresentedResizeAxis::Horizontal,
            PresentedResizeEdge::Trailing,
            rect(30.0, 0.0, 10.0, 20.0),
        ),
        PresentedResizeHandle::new(
            other,
            PresentedResizeAxis::Vertical,
            PresentedResizeEdge::Leading,
            rect(30.0, 0.0, 10.0, 20.0),
        ),
    ])
    .unwrap();
    index
        .bind_pointer_regions(&[
            PresentedPointerRegion::new_owned(
                PresentedRegionId::new(Some(w), PresentedRegionKind::TextBody),
                rect(5.0, 0.0, 10.0, 10.0),
                Some(InteractionId::new(1)),
                Some(PointerAppearanceId::try_from(1_usize).unwrap()),
            ),
            PresentedPointerRegion::new_owned(
                PresentedRegionId::new(Some(w), PresentedRegionKind::TextBody),
                rect(8.0, 0.0, 10.0, 10.0),
                Some(InteractionId::new(2)),
                None,
            ),
        ])
        .unwrap();
    index
}

fn inside(r: FrameRect, x: f32, y: f32) -> bool {
    x >= r.x() && x < r.x() + r.width() && y >= r.y() && y < r.y() + r.height()
}

// Linear references never call the bucket builder, candidate iterator or new cache.
fn linear_region_hit(
    index: &PresentedHitIndex,
    region: PresentedHitRegion,
    x: f32,
    y: f32,
) -> PresentedHit {
    let text_position = if region.kind() == PresentedRegionKind::TextBody {
        index
            .text
            .parts()
            .positions
            .iter()
            .copied()
            .find(|p| Some(p.window()) == region.window() && inside(p.bounds(), x, y))
    } else {
        None
    };
    let string_position = index.string_positions.iter().copied().find(|p| {
        Some(p.window()) == region.window()
            && p.region() == region.kind()
            && inside(p.bounds(), x, y)
    });
    PresentedHit {
        region,
        text_position,
        string_position,
    }
}

fn linear_hit(index: &PresentedHitIndex, x: f32, y: f32) -> Option<PresentedHit> {
    if let Some(handle) = index
        .resize_handles
        .iter()
        .find(|h| inside(h.bounds(), x, y))
    {
        return Some(PresentedHit {
            region: PresentedHitRegion::new(
                Some(handle.window()),
                handle.axis().region_kind(),
                handle.bounds(),
                i32::MAX,
            ),
            text_position: None,
            string_position: None,
        });
    }
    let mut best: Option<&PresentedHitRegion> = None;
    for region in &index.regions {
        if inside(region.bounds(), x, y) && best.is_none_or(|old| region.z_order() > old.z_order())
        {
            best = Some(region);
        }
    }
    best.map(|region| linear_region_hit(index, *region, x, y))
}

fn linear_unified(
    index: &PresentedHitIndex,
    x: f32,
    y: f32,
) -> Option<(
    Option<PresentedHit>,
    Option<InteractionId>,
    Option<PointerAppearanceId>,
)> {
    let mut hit = linear_hit(index, x, y);
    let pointer = if hit.is_some_and(|h| h.region().kind().resize_axis().is_some()) {
        None
    } else {
        index
            .pointer_regions
            .iter()
            .find(|p| inside(p.bounds(), x, y))
    };
    if let Some(pointer) = pointer {
        let region = index
            .regions
            .iter()
            .find(|r| Some(r.id()) == pointer.owner())
            .unwrap();
        hit = Some(linear_region_hit(index, *region, x, y));
    }
    if hit.is_none() && pointer.is_none() {
        None
    } else {
        Some((
            hit,
            pointer.and_then(PresentedPointerRegion::interaction),
            pointer.and_then(PresentedPointerRegion::appearance),
        ))
    }
}

#[test]
fn nontext_query_parity_stale_and_half_open_bounds() {
    let p = PresentationId::new(100);
    let index = fixture(p);
    cold(&index);
    let stale = query(PresentationId::new(101), 6.0, 1.0);
    assert!(matches!(
        index.resolve(stale),
        Err(PresentedHitError::StalePresentation { .. })
    ));
    cold(&index);
    assert!(matches!(
        index.resolve_unified(stale),
        Err(PresentedHitError::StalePresentation { .. })
    ));
    cold(&index);
    let mut address = None;
    for y in [
        -1.0, 0.0, 0.5, 9.999, 10.0, 19.999, 20.0, 29.999, 30.0, 31.0, 32.0, 32.5, 34.499, 34.5,
        35.0,
    ] {
        for x in [
            -1.0, 0.0, 2.0, 3.0, 4.0, 5.0, 8.0, 12.999, 13.0, 14.999, 15.0, 20.0, 24.0, 27.0,
            29.999, 30.0, 39.999, 40.0,
        ] {
            let expected = linear_hit(&index, x, y);
            let expected_unified = linear_unified(&index, x, y);
            assert_eq!(
                index.resolve(query(p, x, y)).unwrap(),
                expected,
                "x={x} y={y}"
            );
            assert_eq!(
                index.resolve_unified(query(p, x, y)).unwrap().map(|h| (
                    h.semantic(),
                    h.interaction(),
                    h.appearance()
                )),
                expected_unified,
                "x={x} y={y}"
            );
            let actual = cache_address(&index).unwrap();
            assert_eq!(*address.get_or_insert(actual), actual);
        }
    }
}

// Independent snapshot of the OLD eager bucket structure/field order. It does
// not call same_nontext_buckets, build_presented_hit_buckets, or inspect caches.
#[derive(Debug, PartialEq)]
struct OldBucket {
    top: f32,
    bottom: f32,
    prefix_max_bottom: f32,
    candidates: Vec<usize>,
    prefix_max_right: Vec<f32>,
}

fn old_buckets(bounds: impl Iterator<Item = FrameRect>) -> Vec<OldBucket> {
    let mut entries: Vec<_> = bounds
        .enumerate()
        .map(|(i, r)| (r.y(), r.y() + r.height(), i, r))
        .collect();
    entries.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let mut answer = Vec::new();
    let mut start = 0;
    let mut prefix_max_bottom = 0.0_f32;
    while start < entries.len() {
        let mut end = start + 1;
        while end < entries.len()
            && entries[start].0.total_cmp(&entries[end].0).is_eq()
            && entries[start].1.total_cmp(&entries[end].1).is_eq()
        {
            end += 1;
        }
        entries[start..end].sort_by(|a, b| a.3.x().total_cmp(&b.3.x()).then(a.2.cmp(&b.2)));
        prefix_max_bottom = prefix_max_bottom.max(entries[start].1);
        let mut right = 0.0_f32;
        let prefix_max_right = entries[start..end]
            .iter()
            .map(|e| {
                right = right.max(e.3.x() + e.3.width());
                right
            })
            .collect();
        answer.push(OldBucket {
            top: entries[start].0,
            bottom: entries[start].1,
            prefix_max_bottom,
            candidates: entries[start..end].iter().map(|e| e.2).collect(),
            prefix_max_right,
        });
        start = end;
    }
    answer
}

#[derive(Debug, PartialEq)]
struct OldIndex<'a> {
    presentation: PresentationId,
    regions: &'a [PresentedHitRegion],
    resize_handles: &'a [PresentedResizeHandle],
    text: &'a PresentedTextIndex,
    strings: &'a [PresentedStringPosition],
    regions_buckets: Vec<OldBucket>,
    resize_buckets: Vec<OldBucket>,
    strings_buckets: Vec<OldBucket>,
    pointers: &'a [PresentedPointerRegion],
    pointers_buckets: Vec<OldBucket>,
}

fn old_index(index: &PresentedHitIndex) -> OldIndex<'_> {
    OldIndex {
        presentation: index.presentation,
        regions: &index.regions,
        resize_handles: &index.resize_handles,
        text: &index.text,
        strings: &index.string_positions,
        regions_buckets: old_buckets(index.regions.iter().map(|r| r.bounds())),
        resize_buckets: old_buckets(index.resize_handles.iter().map(|r| r.bounds())),
        strings_buckets: old_buckets(index.string_positions.iter().map(|r| r.bounds())),
        pointers: &index.pointer_regions,
        pointers_buckets: old_buckets(index.pointer_regions.iter().map(|r| r.bounds())),
    }
}

fn zeros(ys: &[f32]) -> PresentedHitIndex {
    PresentedHitIndex::from_parts(
        PresentationId::new(200),
        ys.iter()
            .enumerate()
            .map(|(i, y)| {
                PresentedHitRegion::new(
                    Some(DisplayWindowId::new(i as i64 + 1)),
                    PresentedRegionKind::TextBody,
                    rect(0.0, *y, 1.0, 1.0),
                    0,
                )
            })
            .collect(),
        vec![],
    )
    .unwrap()
}

#[test]
fn nontext_eq_matches_old_signed_zero_results_without_publication() {
    for (a, b, expected) in [
        (vec![-0.0, 0.0], vec![0.0, 0.0], false),
        (vec![-0.0], vec![0.0], true),
    ] {
        let left = zeros(&a);
        let right = zeros(&b);
        assert_eq!(old_index(&left) == old_index(&right), expected);
        assert_eq!(left == right, expected);
        cold(&left);
        cold(&right);
    }
    let make = |signed: bool| {
        let mut value = PresentedHitIndex::from_parts(
            PresentationId::new(201),
            vec![PresentedHitRegion::new(
                Some(DisplayWindowId::new(1)),
                PresentedRegionKind::TextBody,
                rect(0.0, 0.0, 10.0, 10.0),
                0,
            )],
            vec![],
        )
        .unwrap();
        value
            .bind_pointer_regions(
                &[-0.0, 0.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, y)| {
                        PresentedPointerRegion::new_owned(
                            PresentedRegionId::new(
                                Some(DisplayWindowId::new(1)),
                                PresentedRegionKind::TextBody,
                            ),
                            rect(0.0, if signed { y } else { 0.0 }, 1.0, 1.0),
                            Some(InteractionId::new(i as u32 + 1)),
                            None,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        value
    };
    let left = make(true);
    let right = make(false);
    assert_eq!(left.pointer_regions, right.pointer_regions);
    assert!(!(old_index(&left) == old_index(&right)));
    assert!(left != right);
    cold(&left);
    cold(&right);
}

#[derive(Debug)]
struct Probe {
    positions: Vec<PresentedTextPosition>,
    fail: bool,
    tag: &'static str,
    log: Arc<Mutex<Vec<&'static str>>>,
}
impl PresentedTextPositionSource for Probe {
    fn text_positions(&self) -> Result<Vec<PresentedTextPosition>, PresentedHitError> {
        self.log.lock().unwrap().push(self.tag);
        if self.fail {
            Err(PresentedHitError::InvalidTextPositionGeometry)
        } else {
            Ok(self.positions.clone())
        }
    }
    fn is_empty(&self) -> Option<bool> {
        Some(self.positions.is_empty())
    }
}

fn probe_index(
    tag: &'static str,
    case: usize,
    right: bool,
    log: &Arc<Mutex<Vec<&'static str>>>,
) -> PresentedHitIndex {
    let p = PresentationId::new(if case == 1 && right { 302 } else { 301 });
    let w = DisplayWindowId::new(1);
    let width = if case == 2 && right { 39.0 } else { 40.0 };
    PresentedHitIndex::from_parts_with_strings(
        p,
        vec![
            PresentedHitRegion::new(
                Some(w),
                PresentedRegionKind::TextBody,
                rect(0.0, 0.0, width, 20.0),
                0,
            ),
            PresentedHitRegion::new(
                Some(w),
                PresentedRegionKind::TabLine,
                rect(0.0, 20.0, 40.0, 10.0),
                0,
            ),
        ],
        vec![],
        vec![PresentedStringPosition::new(
            w,
            PresentedWindowChromeArea::TabLine,
            rect(0.0, 20.0, 10.0, 10.0),
            crate::GlyphStringId::new(if case == 4 && right { 9 } else { 8 }),
            0,
        )],
    )
    .unwrap()
    .with_resize_handles(vec![PresentedResizeHandle::new(
        w,
        PresentedResizeAxis::Horizontal,
        PresentedResizeEdge::Trailing,
        rect(30.0, 0.0, if case == 3 && right { 9.0 } else { 10.0 }, 20.0),
    )])
    .unwrap()
    .with_deferred_text(Arc::new(Probe {
        positions: vec![PresentedTextPosition::new(
            w,
            rect(0.0, 0.0, 10.0, 10.0),
            1,
            0,
            0,
        )],
        fail: case == 5,
        tag,
        log: Arc::clone(log),
    }))
}

#[test]
fn nontext_eq_preserves_old_text_materialization_and_short_circuit_order() {
    for case in 0..6 {
        let expected = case == 0 || case == 5;
        let reference_log = Arc::new(Mutex::new(Vec::new()));
        let a = probe_index("left", case, false, &reference_log);
        let b = probe_index("right", case, true, &reference_log);
        assert_eq!(old_index(&a) == old_index(&b), expected);
        let actual_log = Arc::new(Mutex::new(Vec::new()));
        let a = probe_index("left", case, false, &actual_log);
        let b = probe_index("right", case, true, &actual_log);
        assert_eq!(a == b, expected);
        assert_eq!(*actual_log.lock().unwrap(), *reference_log.lock().unwrap());
        assert_eq!(
            *actual_log.lock().unwrap(),
            if (1..=3).contains(&case) {
                vec![]
            } else {
                vec!["left", "right"]
            }
        );
        cold(&a);
        cold(&b);
    }
    let log = Arc::new(Mutex::new(Vec::new()));
    let same = probe_index("self", 0, false, &log);
    assert!(same == same); // A blanket identity shortcut would suppress OLD text work.
    assert_eq!(*log.lock().unwrap(), vec!["self"]);
    cold(&same);
}

#[test]
fn nontext_clone_debug_and_structural_reads_stay_cold() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let value = probe_index("source", 0, false, &log);
    assert!(!value.regions().is_empty());
    assert!(!value.resize_handles().is_empty());
    assert!(!value.string_positions().is_empty());
    assert!(!value.is_empty());
    let _ = format!("{value:?}");
    let copy = value.clone();
    cold(&value);
    cold(&copy);
    assert!(log.lock().unwrap().is_empty());
    value
        .resolve(query(value.presentation(), 1.0, 1.0))
        .unwrap();
    assert!(cache_address(&value).is_some());
    let warm_copy = value.clone();
    cold(&warm_copy);
    assert_eq!(old_index(&value), old_index(&warm_copy));
    assert!(value == warm_copy);
    cold(&warm_copy);
    assert_eq!(*log.lock().unwrap(), vec!["source"]);
}

#[derive(serde::Serialize)]
struct OldWire<'a> {
    presentation: PresentationId,
    regions: &'a [PresentedHitRegion],
    resize_handles: &'a [PresentedResizeHandle],
    text_positions: &'a [PresentedTextPosition],
    string_positions: &'a [PresentedStringPosition],
}

#[test]
fn nontext_wire_matches_old_primary_encoding_without_cache_warmup() {
    let value = fixture(PresentationId::new(400));
    let positions = value.text.parts().positions.clone();
    let expected = serde_json::to_string(&OldWire {
        presentation: value.presentation,
        regions: &value.regions,
        resize_handles: &value.resize_handles,
        text_positions: &positions,
        string_positions: &value.string_positions,
    })
    .unwrap();
    let encoded = serde_json::to_string(&value).unwrap();
    assert_eq!(encoded, expected);
    assert!(!encoded.contains("bucket"));
    cold(&value);
    let decoded: PresentedHitIndex = serde_json::from_str(&encoded).unwrap();
    cold(&decoded);
    assert_eq!(decoded.regions, value.regions);
    assert_eq!(decoded.resize_handles, value.resize_handles);
    assert_eq!(decoded.string_positions, value.string_positions);
    assert!(decoded.pointer_regions.is_empty());
    assert_eq!(
        decoded
            .resolve(query(decoded.presentation(), 1.0, 1.0))
            .unwrap(),
        linear_hit(&decoded, 1.0, 1.0)
    );
    let log = Arc::new(Mutex::new(Vec::new()));
    let lazy = probe_index("wire", 0, false, &log);
    serde_json::to_string(&lazy).unwrap();
    serde_json::to_string(&lazy).unwrap();
    assert_eq!(*log.lock().unwrap(), vec!["wire"]);
    cold(&lazy);
}

#[test]
fn nontext_valid_queries_initialize_one_shared_payload_concurrently() {
    let value = Arc::new(fixture(PresentationId::new(500)));
    let expected = linear_unified(&value, 9.0, 1.0);
    cold(&value);
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let value = Arc::clone(&value);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                let result = value
                    .resolve_unified(query(value.presentation(), 9.0, 1.0))
                    .unwrap()
                    .map(|h| (h.semantic(), h.interaction(), h.appearance()));
                (result, cache_address(&value).unwrap())
            })
        })
        .collect();
    let mut address = None;
    for thread in threads {
        let (actual, pointer) = thread.join().unwrap();
        assert_eq!(actual, expected);
        assert_eq!(*address.get_or_insert(pointer), pointer);
    }
    assert_eq!(cache_address(&value), address);
}

#[test]
fn nontext_successful_writers_reset_and_failed_pointer_writers_preserve_cache() {
    let p = PresentationId::new(600);
    let mut value = fixture(p);
    let hit = value.resolve_unified(query(p, 9.0, 1.0)).unwrap();
    let address = cache_address(&value);
    let old_pointers = value.pointer_regions.clone();
    let owner =
        PresentedRegionId::new(Some(DisplayWindowId::new(1)), PresentedRegionKind::TextBody);
    let unknown = PresentedRegionId::new(
        Some(DisplayWindowId::new(99)),
        PresentedRegionKind::TextBody,
    );
    let errors = [
        (
            PresentedPointerRegion::new(
                rect(1.0, 1.0, 2.0, 2.0),
                Some(InteractionId::new(9)),
                None,
            ),
            PresentedHitError::MissingPointerSemanticOwner,
        ),
        (
            PresentedPointerRegion::new_owned(
                unknown,
                rect(1.0, 1.0, 2.0, 2.0),
                Some(InteractionId::new(9)),
                None,
            ),
            PresentedHitError::UnknownPointerSemanticOwner(unknown),
        ),
        (
            PresentedPointerRegion::new_owned(
                owner,
                rect(39.0, 1.0, 2.0, 2.0),
                Some(InteractionId::new(9)),
                None,
            ),
            PresentedHitError::PointerOutsideSemanticRegion,
        ),
    ];
    for (pointer, expected) in errors {
        assert_eq!(value.bind_pointer_regions(&[pointer]), Err(expected));
        assert_eq!(value.pointer_regions, old_pointers);
        assert_eq!(cache_address(&value), address);
        assert_eq!(value.resolve_unified(query(p, 9.0, 1.0)).unwrap(), hit);
    }
    value
        .bind_pointer_regions(&[PresentedPointerRegion::new_owned(
            owner,
            rect(0.0, 0.0, 10.0, 10.0),
            Some(InteractionId::new(9)),
            None,
        )])
        .unwrap();
    cold(&value);
    assert_eq!(
        value
            .resolve_unified(query(p, 9.0, 1.0))
            .unwrap()
            .unwrap()
            .interaction(),
        Some(InteractionId::new(9))
    );
    let value = value
        .with_resize_handles(vec![PresentedResizeHandle::new(
            DisplayWindowId::new(1),
            PresentedResizeAxis::Vertical,
            PresentedResizeEdge::Trailing,
            rect(0.0, 0.0, 3.0, 20.0),
        )])
        .unwrap();
    cold(&value);
    assert_eq!(
        value
            .resolve(query(p, 1.0, 1.0))
            .unwrap()
            .unwrap()
            .region()
            .kind(),
        PresentedRegionKind::BottomDivider
    );
    let address = cache_address(&value);
    let log = Arc::new(Mutex::new(Vec::new()));
    let value = value.with_deferred_text(Arc::new(Probe {
        positions: vec![],
        fail: false,
        tag: "replace-text",
        log: Arc::clone(&log),
    }));
    assert_eq!(
        cache_address(&value),
        address,
        "text-only replacement must not invalidate nontext payload"
    );
    assert!(log.lock().unwrap().is_empty());
}

#[test]
fn nontext_frame_owner_replacement_rejects_stale_before_initialization_and_clear_stays_cold() {
    let a = fixture(PresentationId::new(701));
    let retained = Arc::new(a);
    let mut frame = crate::FrameGlyphBuffer::with_size(100.0, 60.0);
    frame.presentation_id = retained.presentation();
    frame
        .install_presented_hit_index((*retained).clone())
        .unwrap();
    frame
        .resolve_presented_hit(query(frame.presentation_id, 9.0, 1.0))
        .unwrap();
    assert!(cache_address(frame.presented_hit_index()).is_some());
    let b = PresentedHitIndex::from_parts_with_strings(
        PresentationId::new(702),
        vec![
            PresentedHitRegion::new(
                Some(DisplayWindowId::new(3)),
                PresentedRegionKind::TextBody,
                rect(50.0, 0.0, 20.0, 20.0),
                0,
            ),
            PresentedHitRegion::new(
                Some(DisplayWindowId::new(3)),
                PresentedRegionKind::TabLine,
                rect(50.0, 20.0, 20.0, 10.0),
                0,
            ),
        ],
        vec![PresentedTextPosition::new(
            DisplayWindowId::new(3),
            rect(50.0, 0.0, 10.0, 10.0),
            99,
            0,
            0,
        )],
        vec![PresentedStringPosition::new(
            DisplayWindowId::new(3),
            PresentedWindowChromeArea::TabLine,
            rect(50.0, 20.0, 10.0, 10.0),
            crate::GlyphStringId::new(99),
            7,
        )],
    )
    .unwrap();
    frame.presentation_id = b.presentation();
    frame.install_presented_hit_index(b).unwrap();
    cold(frame.presented_hit_index());
    assert!(matches!(
        frame.resolve_presented_hit(query(retained.presentation(), 9.0, 1.0)),
        Err(PresentedHitError::StalePresentation { .. })
    ));
    cold(frame.presented_hit_index());
    assert!(matches!(
        frame.install_presented_hit_index((*retained).clone()),
        Err(PresentedHitError::StalePresentation { .. })
    ));
    cold(frame.presented_hit_index());
    assert_eq!(
        frame
            .resolve_presented_hit(query(frame.presentation_id, 51.0, 1.0))
            .unwrap()
            .unwrap()
            .semantic()
            .unwrap()
            .text_position()
            .unwrap()
            .buffer_position(),
        99
    );
    assert!(
        frame
            .resolve_presented_hit(query(frame.presentation_id, 9.0, 1.0))
            .unwrap()
            .is_none()
    );
    let string_hit = frame
        .resolve_presented_hit(query(frame.presentation_id, 51.0, 21.0))
        .unwrap()
        .unwrap()
        .semantic()
        .unwrap();
    assert_eq!(string_hit.region().window(), Some(DisplayWindowId::new(3)));
    assert_eq!(
        string_hit.string_position().unwrap().string(),
        crate::GlyphStringId::new(99)
    );
    assert_eq!(string_hit.string_position().unwrap().char_index(), 7);
    assert!(
        frame
            .resolve_presented_hit(query(frame.presentation_id, 1.0, 21.0))
            .unwrap()
            .is_none(),
        "old A string geometry must not survive B replacement"
    );

    assert!(
        retained
            .resolve(query(retained.presentation(), 9.0, 1.0))
            .unwrap()
            .is_some()
    );
    frame.clear_all();
    assert_eq!(
        frame.presented_hit_index().presentation(),
        frame.presentation_id
    );
    cold(frame.presented_hit_index());
    assert!(frame.presented_hit_index().is_empty());
}

#[test]
fn nontext_equality_is_independent_of_both_cache_states_and_reset() {
    let p = PresentationId::new(800);
    for warm_left in [false, true] {
        for warm_right in [false, true] {
            let left = fixture(p);
            let mut right = fixture(p);
            if warm_left {
                left.resolve(query(p, 9.0, 1.0)).unwrap();
            }
            if warm_right {
                right.resolve(query(p, 9.0, 1.0)).unwrap();
            }
            let left_address = cache_address(&left);
            let right_address = cache_address(&right);
            assert_eq!(old_index(&left), old_index(&right));
            assert!(left == right);
            assert_eq!(cache_address(&left), left_address);
            assert_eq!(cache_address(&right), right_address);
            let pointers = right.pointer_regions.clone();
            right.bind_pointer_regions(&pointers).unwrap();
            cold(&right);
            assert_eq!(old_index(&left), old_index(&right));
            assert!(left == right);
            assert_eq!(cache_address(&left), left_address);
            cold(&right);
        }
    }
}

#[test]
fn nontext_validation_keeps_existing_constructor_and_pointer_error_order() {
    let p = PresentationId::new(900);
    let w = DisplayWindowId::new(1);
    // Deserialization of the geometry value bypasses its checked constructor,
    // exactly as malformed transport does; index admission must still reject it.
    let bad: FrameRect =
        serde_json::from_str(r#"{"x":0.0,"y":0.0,"width":-1.0,"height":1.0}"#).unwrap();
    assert!(FrameRect::new(-1.0, 0.0, 1.0, 1.0).is_err());
    let negative_origin: FrameRect =
        serde_json::from_str(r#"{"x":-1.0,"y":0.0,"width":1.0,"height":1.0}"#).unwrap();
    assert_eq!(
        PresentedHitIndex::from_parts(
            p,
            vec![PresentedHitRegion::new(
                Some(w),
                PresentedRegionKind::TextBody,
                negative_origin,
                0
            )],
            vec![]
        )
        .unwrap_err(),
        PresentedHitError::InvalidRegionGeometry
    );
    let bad_region = PresentedHitRegion::new(Some(w), PresentedRegionKind::TabLine, bad, 0);
    let good_region = PresentedHitRegion::new(
        Some(w),
        PresentedRegionKind::TabLine,
        rect(0.0, 0.0, 10.0, 10.0),
        0,
    );
    let bad_text = PresentedTextPosition::new(w, bad, 1, 0, 0);
    let bad_string = PresentedStringPosition::new(
        w,
        PresentedWindowChromeArea::TabLine,
        bad,
        crate::GlyphStringId::new(1),
        0,
    );
    let outside_string = PresentedStringPosition::new(
        w,
        PresentedWindowChromeArea::TabLine,
        rect(9.0, 0.0, 2.0, 1.0),
        crate::GlyphStringId::new(1),
        0,
    );
    assert_eq!(
        PresentedHitIndex::from_parts_with_strings(
            p,
            vec![bad_region],
            vec![bad_text],
            vec![bad_string]
        )
        .unwrap_err(),
        PresentedHitError::InvalidRegionGeometry
    );
    assert_eq!(
        PresentedHitIndex::from_parts_with_strings(
            p,
            vec![good_region],
            vec![bad_text],
            vec![bad_string]
        )
        .unwrap_err(),
        PresentedHitError::InvalidTextPositionGeometry
    );
    assert_eq!(
        PresentedHitIndex::from_parts_with_strings(
            p,
            vec![good_region],
            vec![],
            vec![bad_string, outside_string]
        )
        .unwrap_err(),
        PresentedHitError::InvalidStringPositionGeometry
    );
    assert_eq!(
        PresentedHitIndex::from_parts_with_strings(
            p,
            vec![good_region],
            vec![],
            vec![outside_string]
        )
        .unwrap_err(),
        PresentedHitError::StringPositionOutsideSemanticRegion
    );
    // The first matching owner remains canonical even when a later duplicate
    // could contain the string. A different owner-selection rule is observable.
    let later_owner = PresentedHitRegion::new(
        Some(w),
        PresentedRegionKind::TabLine,
        rect(0.0, 0.0, 20.0, 10.0),
        0,
    );
    assert_eq!(
        PresentedHitIndex::from_parts_with_strings(
            p,
            vec![good_region, later_owner],
            vec![],
            vec![outside_string]
        )
        .unwrap_err(),
        PresentedHitError::StringPositionOutsideSemanticRegion
    );
    let mut index = fixture(p);
    index.resolve(query(p, 9.0, 1.0)).unwrap();
    let address = cache_address(&index);
    let old_pointers = index.pointer_regions.clone();
    let missing = PresentedPointerRegion::new(rect(0.0, 0.0, 1.0, 1.0), None, None);
    let unknown_owner = PresentedRegionId::new(
        Some(DisplayWindowId::new(99)),
        PresentedRegionKind::TextBody,
    );
    let unknown =
        PresentedPointerRegion::new_owned(unknown_owner, rect(0.0, 0.0, 1.0, 1.0), None, None);
    assert_eq!(
        index.bind_pointer_regions(&[unknown.clone(), missing.clone()]),
        Err(PresentedHitError::UnknownPointerSemanticOwner(
            unknown_owner
        ))
    );
    assert_eq!(
        index.bind_pointer_regions(&[missing, unknown]),
        Err(PresentedHitError::MissingPointerSemanticOwner)
    );
    assert_eq!(index.pointer_regions, old_pointers);
    assert_eq!(cache_address(&index), address);
    assert_eq!(
        index
            .clone()
            .with_resize_handles(vec![PresentedResizeHandle::new(
                w,
                PresentedResizeAxis::Vertical,
                PresentedResizeEdge::Trailing,
                bad
            )])
            .unwrap_err(),
        PresentedHitError::InvalidResizeHandleGeometry
    );
    assert_eq!(cache_address(&index), address);
    // Wire admission keeps the same semantic error as direct construction.
    let wire = serde_json::json!({"presentation": 900, "regions": [good_region], "text_positions": [], "string_positions": [outside_string]});
    let error = serde_json::from_value::<PresentedHitIndex>(wire).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&PresentedHitError::StringPositionOutsideSemanticRegion.to_string())
    );
    let good_wire = serde_json::to_value(fixture(p)).unwrap();
    for (family, expected) in [
        ("regions", PresentedHitError::InvalidRegionGeometry),
        (
            "text_positions",
            PresentedHitError::InvalidTextPositionGeometry,
        ),
        (
            "string_positions",
            PresentedHitError::InvalidStringPositionGeometry,
        ),
        (
            "resize_handles",
            PresentedHitError::InvalidResizeHandleGeometry,
        ),
    ] {
        let mut malformed = good_wire.clone();
        malformed[family][0]["bounds"]["width"] = serde_json::json!(-1.0);
        let error = serde_json::from_value::<PresentedHitIndex>(malformed).unwrap_err();
        assert!(
            error.to_string().contains(&expected.to_string()),
            "family={family}: {error}"
        );
    }
    let mut negative_wire = good_wire;
    negative_wire["regions"][0]["bounds"]["x"] = serde_json::json!(-1.0);
    assert!(
        serde_json::from_value::<PresentedHitIndex>(negative_wire)
            .unwrap_err()
            .to_string()
            .contains(&PresentedHitError::InvalidRegionGeometry.to_string())
    );
}
