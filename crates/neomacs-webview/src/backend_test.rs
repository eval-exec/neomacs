use neomacs_display_protocol::WebViewId;

use crate::backend::NavigationMilestone;
use crate::{WebViewEvent, WebViewGeneration};

#[test]
fn navigation_milestones_have_total_normalized_event_semantics() {
    let id = WebViewId::new(7);
    let generation = WebViewGeneration::new(11);

    assert_eq!(
        NavigationMilestone::Started.normalized_events(id, generation),
        vec![WebViewEvent::LoadProgressChanged {
            id,
            generation,
            progress: 0.0,
        }]
    );
    assert!(
        NavigationMilestone::StateChanged
            .normalized_events(id, generation)
            .is_empty()
    );
    assert_eq!(
        NavigationMilestone::Finished.normalized_events(id, generation),
        vec![
            WebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress: 1.0,
            },
            WebViewEvent::LoadFinished {
                id,
                generation,
                navigation: None,
            },
        ]
    );
}
