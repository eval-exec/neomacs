use neomacs_display_protocol::WebViewId;

use super::PageLoadState;
use crate::backend::NavigationMilestone;
use crate::{LoadPhase, WebViewEvent, WebViewGeneration};

fn ids() -> (WebViewId, WebViewGeneration) {
    (WebViewId::new(3), WebViewGeneration::new(5))
}

fn progress(progress: f64) -> WebViewEvent {
    let (id, generation) = ids();
    WebViewEvent::LoadProgressChanged {
        id,
        generation,
        progress,
    }
}

fn phase(phase: LoadPhase) -> WebViewEvent {
    let (id, generation) = ids();
    WebViewEvent::LoadChanged {
        id,
        generation,
        phase,
    }
}

fn finished() -> WebViewEvent {
    let (id, generation) = ids();
    WebViewEvent::LoadFinished {
        id,
        generation,
        navigation: None,
    }
}

/// KVO fires once per changed key, and the sampling fallback fires on every
/// service turn; both feed one diff so a value is reported exactly when it
/// changes, whichever path saw it first.
#[test]
fn observed_state_reports_each_value_once_per_change() {
    let (id, generation) = ids();
    let mut state = PageLoadState::new();
    let _ = state.milestone(id, generation, NavigationMilestone::Started);

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
            progress(0.1),
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
        vec![progress(0.7)]
    );
}

/// GNU reads `title` and `URL` live, so a page that loses its title (nil
/// from WKWebView) must not leave the previous page's string with the
/// frontend: the change to nothing is reported as an empty string.  Before a
/// first value exists there is nothing to report.
#[test]
fn a_title_or_uri_that_goes_away_is_reported_as_empty_not_kept() {
    let (id, generation) = ids();
    let mut state = PageLoadState::new();

    assert_eq!(
        state.observe(id, generation, None, None, 0.0),
        Vec::<WebViewEvent>::new(),
        "nil before the first load is not a change"
    );
    let _ = state.observe(id, generation, Some("T".into()), Some("u".into()), 0.5);
    assert_eq!(
        state.observe(id, generation, None, None, 0.5),
        vec![
            WebViewEvent::TitleChanged {
                id,
                generation,
                title: String::new()
            },
            WebViewEvent::UriChanged {
                id,
                generation,
                uri: String::new()
            },
        ]
    );
    assert!(state.observe(id, generation, None, None, 0.5).is_empty());
    assert!(
        state
            .observe(
                id,
                generation,
                Some(String::new()),
                Some(String::new()),
                0.5
            )
            .is_empty(),
        "nil and an empty string are the same reported value"
    );
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
    let mut state = PageLoadState::new();
    let _ = state.milestone(id, generation, NavigationMilestone::Started);
    assert_eq!(
        state.observe(id, generation, None, None, 1.5),
        vec![progress(1.0)]
    );
    assert_eq!(
        state.observe(id, generation, None, None, -0.2),
        vec![progress(0.0)]
    );
}

/// One load, both paths reporting: the milestone publishes the ends, the
/// observer the middle, and no value is published twice.  A finish after the
/// observer already saw `1.0` adds the phase and completion only.
#[test]
fn milestones_and_readings_publish_one_progress_sequence() {
    let (id, generation) = ids();
    let mut state = PageLoadState::new();

    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Started),
        vec![phase(LoadPhase::Started)],
        "a fresh view is already at 0.0: only the phase is news"
    );
    assert_eq!(
        state.observe(id, generation, None, None, 0.3),
        vec![progress(0.3)]
    );
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Committed),
        vec![phase(LoadPhase::Committed)]
    );
    assert_eq!(
        state.observe(id, generation, None, None, 1.0),
        vec![progress(1.0)]
    );
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Finished),
        vec![phase(LoadPhase::Finished), finished()],
        "the observer already published 1.0"
    );

    // A second load starts from 1.0, so its start is a progress change.
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Started),
        vec![progress(0.0), phase(LoadPhase::Started)]
    );
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Finished),
        vec![progress(1.0), phase(LoadPhase::Finished), finished()],
        "a finish the observer did not see publishes the terminal 1.0"
    );
}

/// Progress is terminal once a load finished (or failed): a later, lower
/// sample from the observer does not walk it back, and an idle view's
/// readings before any load are not progress.
#[test]
fn progress_readings_count_only_while_a_load_is_in_flight() {
    let (id, generation) = ids();
    let mut state = PageLoadState::new();

    assert!(
        state.observe(id, generation, None, None, 0.4).is_empty(),
        "no load has started"
    );
    let _ = state.milestone(id, generation, NavigationMilestone::Started);
    let _ = state.milestone(id, generation, NavigationMilestone::Finished);
    assert!(
        state.observe(id, generation, None, None, 0.6).is_empty(),
        "a failed or finished load stays at 1.0"
    );
    assert_eq!(
        state.observe(id, generation, Some("late".into()), None, 0.6),
        vec![WebViewEvent::TitleChanged {
            id,
            generation,
            title: "late".into()
        }],
        "title and URI are still live after the load"
    );
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Started),
        vec![progress(0.0), phase(LoadPhase::Started)]
    );
    assert_eq!(
        state.observe(id, generation, Some("late".into()), None, 0.6),
        vec![progress(0.6)]
    );
}

/// A finish with no start (a load that began before the view was observed,
/// or a failure reported first) still pins the terminal progress.
#[test]
fn a_finish_from_idle_publishes_the_terminal_progress() {
    let (id, generation) = ids();
    let mut state = PageLoadState::new();
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Finished),
        vec![progress(1.0), phase(LoadPhase::Finished), finished()]
    );
    assert!(state.observe(id, generation, None, None, 0.3).is_empty());
}

/// A redirect or commit without a start (the delegate can miss the start
/// of a load that began before the view was observed) still opens a load.
#[test]
fn a_redirect_or_commit_opens_a_load_for_progress_readings() {
    let (id, generation) = ids();
    let mut state = PageLoadState::new();
    assert_eq!(
        state.milestone(id, generation, NavigationMilestone::Redirected),
        vec![phase(LoadPhase::Redirected)]
    );
    assert_eq!(
        state.observe(id, generation, None, None, 0.2),
        vec![progress(0.2)]
    );
}
