use neomacs_display_protocol::WebViewId;

use super::observed::ObservedWebState;
use crate::{WebViewEvent, WebViewGeneration};

fn ids() -> (WebViewId, WebViewGeneration) {
    (WebViewId::new(3), WebViewGeneration::new(5))
}

/// KVO fires once per changed key, and the sampling fallback fires on every
/// service turn; both feed one diff so a value is reported exactly when it
/// changes, whichever path saw it first.
#[test]
fn observed_state_reports_each_value_once_per_change() {
    let (id, generation) = ids();
    let mut state = ObservedWebState::new();

    let first = state.observe(id, generation, Some("A".into()), Some("u1".into()), 0.1);
    assert_eq!(
        first,
        vec![
            WebViewEvent::TitleChanged {
                id,
                generation,
                title: "A".into()
            },
            WebViewEvent::UriChanged {
                id,
                generation,
                uri: "u1".into()
            },
            WebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress: 0.1
            },
        ]
    );
    assert!(
        state
            .observe(id, generation, Some("A".into()), Some("u1".into()), 0.1)
            .is_empty(),
        "an unchanged sample is not an event"
    );
    assert_eq!(
        state.observe(id, generation, Some("A".into()), Some("u1".into()), 0.7),
        vec![WebViewEvent::LoadProgressChanged {
            id,
            generation,
            progress: 0.7
        }]
    );
}

/// WKWebView answers nil for `title` and `URL` before a load and during some
/// transitions.  A nil is remembered so the next real value is reported, but
/// it is never sent to Lisp as an empty title or URI.
#[test]
fn observed_state_never_reports_a_missing_title_or_uri() {
    let (id, generation) = ids();
    let mut state = ObservedWebState::new();

    assert_eq!(
        state.observe(id, generation, None, None, 0.0),
        Vec::<WebViewEvent>::new()
    );
    let _ = state.observe(id, generation, Some("T".into()), Some("u".into()), 0.5);
    assert!(state.observe(id, generation, None, None, 0.5).is_empty());
    assert_eq!(
        state.observe(id, generation, Some("T".into()), Some("u".into()), 0.5),
        vec![
            WebViewEvent::TitleChanged {
                id,
                generation,
                title: "T".into()
            },
            WebViewEvent::UriChanged {
                id,
                generation,
                uri: "u".into()
            },
        ],
        "a value that came back after a nil gap is a change again"
    );
}

/// `estimatedProgress` is documented as 0.0..=1.0; anything else from the
/// framework is clamped before Lisp sees it.
#[test]
fn observed_progress_is_clamped_to_the_unit_interval() {
    let (id, generation) = ids();
    let mut state = ObservedWebState::new();
    assert_eq!(
        state.observe(id, generation, None, None, 1.5),
        vec![WebViewEvent::LoadProgressChanged {
            id,
            generation,
            progress: 1.0
        }]
    );
    assert_eq!(
        state.observe(id, generation, None, None, -0.2),
        vec![WebViewEvent::LoadProgressChanged {
            id,
            generation,
            progress: 0.0
        }]
    );
}
