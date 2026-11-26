use super::tests::{frame, point};
use super::*;
use neomacs_display_protocol::input_progress::InputStream;
use neomacs_display_protocol::scroll_coverage::AuxiliaryScrollCoverage;
use neomacs_display_protocol::{DisplayFrameId, DisplayWindowId};

#[test]
fn auxiliary_replacement_reconciles_origin_and_withdrawal_preserves_submitted_hits() {
    let first = frame(1, 0.0, 1);
    let next = frame(1, 10.0, 1);
    let mut retention = CoverageRetention::default();
    retention.refresh(&first);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let mut scroll = InputScroll::default();
    assert!(scroll.push(&retention, &first, 1.0, 1.0, 12.0, delivery.receipt(), None));
    scroll.paint(&mut first.clone());
    scroll.submit();
    let hit_position = |scroll: &InputScroll| {
        scroll
            .hit(point(&first, 1.0))
            .unwrap()
            .unwrap()
            .unwrap()
            .text_position()
            .unwrap()
            .buffer_position()
    };
    assert_eq!(hit_position(&scroll), 3);
    let change = retention.apply_auxiliary(
        AuxiliaryScrollCoverage {
            frame: DisplayFrameId::new(0),
            base: first.presentation_id,
            surfaces: next.scroll_surfaces.clone(),
        },
        Some(&first),
    );
    assert!(change.changed());
    assert!(scroll.reconcile_auxiliary(&retention, Some(&first)));
    let active = scroll.active.as_ref().unwrap();
    assert_eq!(
        active.surface.coverage().publication,
        next.scroll_surfaces[0].coverage().publication
    );
    assert_eq!(active.offset, 2.0);
    assert_eq!(
        hit_position(&scroll),
        3,
        "replacement cannot change hits before submission"
    );
    assert!(!scroll.reconcile_auxiliary(&retention, Some(&first)));
    scroll.paint(&mut first.clone());
    scroll.submit();
    assert_eq!(
        hit_position(&scroll),
        3,
        "translated origin preserves the submitted target"
    );
    retention.apply_auxiliary(
        AuxiliaryScrollCoverage {
            frame: DisplayFrameId::new(0),
            base: PresentationId::new(99),
            surfaces: Vec::new(),
        },
        Some(&first),
    );
    assert!(scroll.reconcile_auxiliary(&retention, Some(&first)));
    assert!(!scroll.active());
    assert_eq!(
        hit_position(&scroll),
        3,
        "withdrawal retains hits for pixels still submitted"
    );
    scroll.paint(&mut first.clone());
    scroll.submit();
    assert!(
        scroll.hit(point(&first, 1.0)).is_none(),
        "new authoritative pixels retire projected hits"
    );
}

#[test]
fn preview_intent_from_pending_or_rejected_auxiliary_and_backward_origin_change_is_refused() {
    use neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent;
    let base = frame(1, 0.0, 1);
    // Backward bridge grew coverage upward, changing `origin` and minting a new
    // `CoveragePublicationId` under the same lease epoch.
    let backward_bridge = frame(1, 10.0, 1);
    let base_pub = base.scroll_surfaces[0].coverage().publication;
    let bridge_pub = backward_bridge.scroll_surfaces[0].coverage().publication;
    assert_ne!(base_pub, bridge_pub);
    assert_eq!(
        base.scroll_surfaces[0].coverage().epoch,
        backward_bridge.scroll_surfaces[0].coverage().epoch
    );

    let mut retention = CoverageRetention::default();
    retention.refresh(&base);
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let mut scroll = InputScroll::default();

    // H1/R1: while the auxiliary backward-bridge update is still pending (or
    // was rejected by the renderer), a ScrollPreview built from the evaluator's
    // newer mirror carries `bridge_pub` and MUST be refused against `base_pub`.
    let pending_intent = ResolvedScrollIntent {
        frame: 0,
        window: DisplayWindowId::new(1),
        presentation: base.presentation_id,
        epoch: 1,
        publication: bridge_pub,
        offset: 12.0,
        inputs: vec![delivery.receipt()],
    };
    assert!(
        !scroll.resolve(&retention, &base, pending_intent),
        "renderer must refuse preview intent whose publication id is not installed"
    );
    assert!(!scroll.active());

    // Once the auxiliary update installs on the exact base, the stale base
    // publication is refused and the newly installed backward-bridge publication
    // resolves cleanly.
    let change = retention.apply_auxiliary(
        AuxiliaryScrollCoverage {
            frame: DisplayFrameId::new(0),
            base: base.presentation_id,
            surfaces: backward_bridge.scroll_surfaces.clone(),
        },
        Some(&base),
    );
    assert!(change.changed());
    let stale_intent = ResolvedScrollIntent {
        frame: 0,
        window: DisplayWindowId::new(1),
        presentation: base.presentation_id,
        epoch: 1,
        publication: base_pub,
        offset: 12.0,
        inputs: vec![delivery.receipt()],
    };
    assert!(
        !scroll.resolve(&retention, &base, stale_intent),
        "stale publication id must be refused after auxiliary replacement"
    );
    let matching_intent = ResolvedScrollIntent {
        frame: 0,
        window: DisplayWindowId::new(1),
        presentation: base.presentation_id,
        epoch: 1,
        publication: bridge_pub,
        offset: 12.0,
        inputs: vec![delivery.receipt()],
    };
    assert!(scroll.resolve(&retention, &base, matching_intent));
    assert!(scroll.active());

    // H4/R5: an unconditional withdrawal with a mismatched base still revokes
    // the surface so neither preview nor hit can use it afterward.
    let revoked = retention.apply_auxiliary(
        AuxiliaryScrollCoverage {
            frame: DisplayFrameId::new(0),
            base: PresentationId::new(999),
            surfaces: Vec::new(),
        },
        Some(&base),
    );
    assert_eq!(revoked.revoked, 1);
    assert_eq!(revoked.conservative_revocations, 1);
    assert!(scroll.reconcile_auxiliary(&retention, Some(&base)));
    assert!(!scroll.active());
    let after_revoke = ResolvedScrollIntent {
        frame: 0,
        window: DisplayWindowId::new(1),
        presentation: base.presentation_id,
        epoch: 1,
        publication: bridge_pub,
        offset: 12.0,
        inputs: vec![delivery.receipt()],
    };
    assert!(
        !scroll.resolve(&retention, &base, after_revoke),
        "revoked surface must not accept preview intents"
    );
}
