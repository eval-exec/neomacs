use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::{
    DisplayWindowId, FrameRect, PresentationId, PresentedHitError, PresentedHitIndex,
    PresentedHitQuery, PresentedHitRegion, PresentedRegionKind, PresentedTextPosition,
    PresentedTextPositionHit, PresentedTextPositionSource,
};

#[derive(Clone, Copy, Debug)]
enum DirectMode {
    Validated,
    InvalidReturn,
    WrongWindow,
    Error,
}

#[derive(Debug)]
struct DirectSource {
    positions: Vec<PresentedTextPosition>,
    flattened: AtomicUsize,
    queried: AtomicUsize,
    mode: DirectMode,
}

impl DirectSource {
    fn new(positions: Vec<PresentedTextPosition>, mode: DirectMode) -> Arc<Self> {
        Arc::new(Self {
            positions,
            flattened: AtomicUsize::new(0),
            queried: AtomicUsize::new(0),
            mode,
        })
    }
}

impl PresentedTextPositionSource for DirectSource {
    fn text_positions(&self) -> Result<Vec<PresentedTextPosition>, PresentedHitError> {
        self.flattened.fetch_add(1, Ordering::Relaxed);
        Ok(self.positions.clone())
    }

    fn hit_text_position(
        &self,
        window: DisplayWindowId,
        x: f32,
        y: f32,
    ) -> PresentedTextPositionHit {
        self.queried.fetch_add(1, Ordering::Relaxed);
        let result = match self.mode {
            DirectMode::Validated => {
                if self
                    .positions
                    .iter()
                    .any(|position| !super::rect_has_valid_geometry(position.bounds()))
                {
                    Err(PresentedHitError::InvalidTextPositionGeometry)
                } else {
                    Ok(self.positions.iter().copied().find(|position| {
                        position.window() == window && super::contains(position.bounds(), x, y)
                    }))
                }
            }
            DirectMode::InvalidReturn => Ok(self.positions.first().copied()),
            DirectMode::WrongWindow => Ok(Some(position(DisplayWindowId::new(99), 50, 0.0))),
            DirectMode::Error => Err(PresentedHitError::InvalidTextPositionGeometry),
        };
        PresentedTextPositionHit::Resolved(result)
    }

    fn is_empty(&self) -> Option<bool> {
        Some(self.positions.is_empty())
    }
}

#[derive(Debug)]
struct LegacySource {
    positions: Vec<PresentedTextPosition>,
    flattened: AtomicUsize,
}

impl PresentedTextPositionSource for LegacySource {
    fn text_positions(&self) -> Result<Vec<PresentedTextPosition>, PresentedHitError> {
        self.flattened.fetch_add(1, Ordering::Relaxed);
        Ok(self.positions.clone())
    }
}

fn position(window: DisplayWindowId, buffer_position: i64, x: f32) -> PresentedTextPosition {
    PresentedTextPosition::new(
        window,
        FrameRect::new(x, 0.0, 10.0, 10.0).unwrap(),
        buffer_position,
        0,
        buffer_position,
    )
}

fn invalid_transport_position(buffer_position: i64) -> PresentedTextPosition {
    // FrameRect's public constructor rejects this, but its transparent serde
    // transport permits the raw rectangle. Hit-index validation must reject
    // malformed transport data through both the direct and legacy sources.
    let bounds: FrameRect =
        serde_json::from_str(r#"{"x":-1.0,"y":0.0,"width":10.0,"height":10.0}"#).unwrap();
    assert!(FrameRect::new(bounds.x(), bounds.y(), bounds.width(), bounds.height()).is_err());
    PresentedTextPosition::new(DisplayWindowId::new(1), bounds, buffer_position, 0, 0)
}

fn index(source: Arc<dyn PresentedTextPositionSource>) -> PresentedHitIndex {
    PresentedHitIndex::from_parts(
        PresentationId::new(4),
        vec![PresentedHitRegion::new(
            Some(DisplayWindowId::new(1)),
            PresentedRegionKind::TextBody,
            FrameRect::new(0.0, 0.0, 100.0, 100.0).unwrap(),
            0,
        )],
        Vec::new(),
    )
    .unwrap()
    .with_deferred_text(source)
}

fn hit(index: &PresentedHitIndex, x: f32, y: f32) -> Option<PresentedTextPosition> {
    let point = crate::InteractionProjection::settled(index.presentation())
        .map(
            crate::GeometryPoint::<crate::RootSurfaceSpace, crate::LogicalPixels>::from_px(x, y)
                .unwrap(),
        )
        .unwrap();
    index
        .resolve(PresentedHitQuery::new(point))
        .unwrap()
        .and_then(|resolved| resolved.text_position())
}

#[test]
fn direct_hit_preserves_original_overlap_order_and_window_without_flattening() {
    let window = DisplayWindowId::new(1);
    let source = DirectSource::new(
        vec![
            position(DisplayWindowId::new(2), 1, 0.0),
            position(window, 30, 4.0),
            position(window, 10, 0.0),
        ],
        DirectMode::Validated,
    );
    let index = index(source.clone());
    assert_eq!(hit(&index, 5.0, 1.0).unwrap().buffer_position(), 30);
    assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
    assert!(index.text_deferred());
}

#[test]
fn direct_miss_and_half_open_bounds_do_not_flatten() {
    let source = DirectSource::new(
        vec![position(DisplayWindowId::new(1), 30, 0.0)],
        DirectMode::Validated,
    );
    let index = index(source.clone());
    assert!(hit(&index, 10.0, 1.0).is_none());
    assert!(hit(&index, 1.0, 10.0).is_none());
    assert!(hit(&index, 50.0, 50.0).is_none());
    assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
    assert!(index.text_deferred());
}

#[test]
fn direct_source_serialization_equality_and_explicit_positions_match_eager() {
    let positions = vec![
        position(DisplayWindowId::new(1), 30, 4.0),
        position(DisplayWindowId::new(1), 10, 0.0),
    ];
    let source = DirectSource::new(positions.clone(), DirectMode::Validated);
    let index = index(source.clone());
    let eager = PresentedHitIndex::from_parts(
        index.presentation(),
        index.regions().to_vec(),
        positions.clone(),
    )
    .unwrap();
    assert_eq!(hit(&index, 5.0, 1.0), hit(&eager, 5.0, 1.0));
    assert!(index.text_deferred());
    let encoded = serde_json::to_string(&index).unwrap();
    assert_eq!(encoded, serde_json::to_string(&eager).unwrap());
    let decoded: PresentedHitIndex = serde_json::from_str(&encoded).unwrap();
    assert_eq!(index, eager);
    assert_eq!(decoded, eager);
    assert_eq!(index.text_positions(), positions);
    assert_eq!(source.flattened.load(Ordering::Relaxed), 1);
    assert!(!index.text_deferred());
    let direct_calls = source.queried.load(Ordering::Relaxed);
    assert_eq!(hit(&index, 5.0, 1.0), hit(&eager, 5.0, 1.0));
    assert_eq!(source.queried.load(Ordering::Relaxed), direct_calls);
}

#[test]
fn existing_sources_default_to_one_materialized_lookup() {
    let source = Arc::new(LegacySource {
        positions: vec![position(DisplayWindowId::new(1), 10, 0.0)],
        flattened: AtomicUsize::new(0),
    });
    let index = index(source.clone());
    assert_eq!(hit(&index, 1.0, 1.0).unwrap().buffer_position(), 10);
    assert_eq!(hit(&index, 1.0, 1.0).unwrap().buffer_position(), 10);
    assert_eq!(source.flattened.load(Ordering::Relaxed), 1);
    assert!(!index.text_deferred());
}

#[test]
fn direct_source_emptiness_does_not_flatten() {
    for positions in [Vec::new(), vec![position(DisplayWindowId::new(1), 10, 0.0)]] {
        let empty = positions.is_empty();
        let source = DirectSource::new(positions, DirectMode::Validated);
        let index = PresentedHitIndex::default().with_deferred_text(source.clone());
        assert_eq!(index.is_empty(), empty);
        assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
        assert!(index.text_deferred());
    }
}

#[derive(Debug)]
struct ErrorEvents(Arc<AtomicUsize>);

impl tracing::Subscriber for ErrorEvents {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() == tracing::Level::ERROR {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

#[test]
fn invalid_direct_geometry_logs_and_caches_empty_like_legacy_geometry() {
    let invalid = invalid_transport_position(10);
    let source = DirectSource::new(vec![invalid], DirectMode::InvalidReturn);
    let direct = index(source.clone());
    let legacy = index(Arc::new(LegacySource {
        positions: vec![invalid],
        flattened: AtomicUsize::new(0),
    }));
    let errors = Arc::new(AtomicUsize::new(0));
    tracing::subscriber::with_default(ErrorEvents(errors.clone()), || {
        assert!(hit(&direct, 1.0, 1.0).is_none());
        assert!(hit(&direct, 1.0, 1.0).is_none());
        assert!(hit(&legacy, 1.0, 1.0).is_none());
        assert!(hit(&legacy, 1.0, 1.0).is_none());
    });
    assert_eq!(errors.load(Ordering::Relaxed), 2);
    assert_eq!(direct.text_positions(), legacy.text_positions());
    assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
    assert_eq!(source.queried.load(Ordering::Relaxed), 1);
    assert!(!direct.text_deferred());
    // Built failure takes precedence over the source's original nonempty state.
    assert!(direct.text.is_empty());
}

#[test]
fn invalid_geometry_in_an_unqueried_row_rejects_the_whole_source() {
    let source = DirectSource::new(
        vec![
            position(DisplayWindowId::new(1), 10, 0.0),
            invalid_transport_position(20),
        ],
        DirectMode::Validated,
    );
    let index = index(source.clone());
    assert!(hit(&index, 1.0, 1.0).is_none());
    assert!(index.text_positions().is_empty());
    assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
}

#[test]
fn direct_error_and_wrong_window_cache_an_empty_index() {
    for mode in [DirectMode::Error, DirectMode::WrongWindow] {
        let source = DirectSource::new(vec![position(DisplayWindowId::new(1), 10, 0.0)], mode);
        let index = index(source.clone());
        assert!(hit(&index, 1.0, 1.0).is_none());
        assert!(hit(&index, 1.0, 1.0).is_none());
        assert!(index.text_positions().is_empty());
        assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
        assert_eq!(source.queried.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn immutable_direct_source_supports_concurrent_hits_without_flattening() {
    let source = DirectSource::new(
        vec![position(DisplayWindowId::new(1), 10, 0.0)],
        DirectMode::Validated,
    );
    let index = Arc::new(index(source.clone()));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let index = index.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    assert_eq!(hit(&index, 1.0, 1.0).unwrap().buffer_position(), 10);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(source.queried.load(Ordering::Relaxed), 80);
    assert_eq!(source.flattened.load(Ordering::Relaxed), 0);
    assert!(index.text_deferred());
}
