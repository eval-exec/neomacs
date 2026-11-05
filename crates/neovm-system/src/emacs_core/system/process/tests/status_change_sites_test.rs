//! The release-lineage status-change sites (private child_status data).

#[test]
fn the_status_change_sites_are_the_release_lineages_tick_bumps() {
    use neovm_eval::emacs_core::eval::data::process_state::StatusChangeSite;
    use neovm_eval::emacs_core::eval::data::process_state::{
        StatusChangeNotifier, StatusChangeRecorder,
    };

    assert_eq!(StatusChangeSite::COUNT, 9);
    assert_eq!(StatusChangeSite::ALL.len(), 9);

    let cited: Vec<&'static str> = StatusChangeSite::ALL.iter().map(|s| s.gnu()).collect();
    assert_eq!(
        cited,
        vec![
            "src/process.c:1128",
            "src/process.c:1148",
            "src/process.c:6058",
            "src/process.c:6075",
            "src/process.c:6084",
            "src/process.c:6141",
            "src/process.c:6927",
            "src/process.c:7178",
            "src/process.c:7746",
        ]
    );

    for site in StatusChangeSite::ALL {
        assert!(
            site.gnu().starts_with("src/process.c:"),
            "{site:?} needs a citation"
        );
        assert!(!site.what().is_empty(), "{site:?} needs a status");
    }

    // GNU consumes four of the nine on the spot -- `status_notify` is called
    // within a few lines of the bump -- and leaves the rest for the wait's own
    // `status_notify` at :5554 / :5854.
    let synchronous: Vec<StatusChangeSite> = StatusChangeSite::ALL
        .into_iter()
        .filter(|site| {
            matches!(
                site.notifier(),
                StatusChangeNotifier::SynchronouslyAtTheSite { .. }
            )
        })
        .collect();
    assert_eq!(
        synchronous,
        vec![
            StatusChangeSite::DeleteProcessConnection,
            StatusChangeSite::DeleteProcessChild,
            StatusChangeSite::NonBlockingConnectFailed,
            StatusChangeSite::ProcessSendSignalSigcont,
        ]
    );

    // And exactly one of the nine has no analogue in this port, with a reason.
    let without: Vec<StatusChangeSite> = StatusChangeSite::ALL
        .into_iter()
        .filter(|site| matches!(site.recorder(), StatusChangeRecorder::NoAnalogue { .. }))
        .collect();
    assert_eq!(without, vec![StatusChangeSite::PtyEioBeforeFork]);
    for site in StatusChangeSite::ALL {
        match site.recorder() {
            StatusChangeRecorder::Here(where_) => {
                assert!(!where_.is_empty(), "{site:?} needs a recording site")
            }
            StatusChangeRecorder::NoAnalogue { why } => {
                assert!(!why.is_empty(), "{site:?} needs a reason")
            }
        }
    }
}
