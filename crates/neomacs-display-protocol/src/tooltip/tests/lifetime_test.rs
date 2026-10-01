use super::*;

#[test]
fn invalidating_native_context_cancels_an_already_queued_ticket() {
    let context = Arc::new(TooltipContext::default());
    let mut client = TooltipClient::new(context.clone());
    let queued = client.present(None);
    context.invalidate();
    assert!(!queued.is_current());
    assert!(!queued.mark_visible());
    let current = client.present(None);
    assert!(current.mark_visible());
}

#[test]
fn delayed_help_cannot_resurrect_after_native_cancellation() {
    let context = Arc::new(TooltipContext::default());
    let mut client = TooltipClient::new(context.clone());
    let generation = client.generation();
    context.invalidate();
    let stale = client.present(Some(generation));
    assert!(!stale.is_current());
    assert!(!stale.mark_visible());
}

#[test]
fn replacement_and_dismissal_have_independent_identities() {
    let mut client = TooltipClient::default();
    let old = client.present(None);
    assert!(old.mark_visible());
    let new = client.present(None);
    assert!(!old.is_current());
    assert!(!old.same_request(&new));
    assert!(new.mark_visible());
    assert!(!old.cancel());
    assert!(new.is_current());
    assert!(client.dismiss().unwrap().1);
    assert!(client.dismiss().is_none());
}

#[test]
fn stale_delayed_show_cannot_cancel_a_newer_visible_tooltip() {
    let context = Arc::new(TooltipContext::default());
    let mut client = TooltipClient::new(context.clone());
    let old_generation = client.generation();
    context.invalidate();
    let current = client.present(None);
    assert!(current.mark_visible());
    let stale = client.present(Some(old_generation));
    assert!(!stale.is_current());
    assert!(current.is_current());
    assert!(client.dismiss().unwrap().1);
}
