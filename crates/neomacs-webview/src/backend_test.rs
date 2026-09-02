use neomacs_display_protocol::WebViewId;

use crate::backend::NavigationMilestone;
use crate::{LoadPhase, WebViewEvent, WebViewGeneration};

/// Every milestone a native delegate can report maps to the GNU-visible
/// `load-changed` phase (src/xwidget.c:2427-2447 spells the four strings),
/// and the two ends of a load also pin the progress the frontend would
/// otherwise have to guess.
#[test]
fn navigation_milestones_have_total_normalized_event_semantics() {
    let id = WebViewId::new(7);
    let generation = WebViewGeneration::new(11);

    assert_eq!(
        NavigationMilestone::Started.normalized_events(id, generation),
        vec![
            WebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress: 0.0,
            },
            WebViewEvent::LoadChanged {
                id,
                generation,
                phase: LoadPhase::Started,
            },
        ]
    );
    assert_eq!(
        NavigationMilestone::Redirected.normalized_events(id, generation),
        vec![WebViewEvent::LoadChanged {
            id,
            generation,
            phase: LoadPhase::Redirected,
        }]
    );
    assert_eq!(
        NavigationMilestone::Committed.normalized_events(id, generation),
        vec![WebViewEvent::LoadChanged {
            id,
            generation,
            phase: LoadPhase::Committed,
        }]
    );
    assert_eq!(
        NavigationMilestone::Finished.normalized_events(id, generation),
        vec![
            WebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress: 1.0,
            },
            WebViewEvent::LoadChanged {
                id,
                generation,
                phase: LoadPhase::Finished,
            },
            WebViewEvent::LoadFinished {
                id,
                generation,
                navigation: None,
            },
        ]
    );
}

/// The strings are GNU's, verbatim: `xwidget-webkit-callback' compares
/// `(nth 3 last-input-event)' against "load-finished".
#[test]
fn load_phases_spell_gnus_event_strings() {
    assert_eq!(LoadPhase::Started.gnu_name(), "load-started");
    assert_eq!(LoadPhase::Redirected.gnu_name(), "load-redirected");
    assert_eq!(LoadPhase::Committed.gnu_name(), "load-committed");
    assert_eq!(LoadPhase::Finished.gnu_name(), "load-finished");
}
