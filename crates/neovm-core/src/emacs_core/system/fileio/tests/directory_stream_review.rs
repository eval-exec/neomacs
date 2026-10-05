use super::*;
use crate::emacs_core::error::{FlowKind, signal};
#[test]
fn retry_checks_quit_without_requiring_host_signals_or_a_race() {
    let mut attempts = 0;
    let mut polls = 0;
    let result = next_with_retry(
        || {
            attempts += 1;
            Err(io::Error::from_raw_os_error(if attempts % 2 == 0 {
                libc::EINTR
            } else {
                libc::EAGAIN
            }))
        },
        || {
            polls += 1;
            if polls == 3 {
                Err(signal("quit", vec![]))
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(attempts, 3);
    assert_eq!(polls, 3);
    match result {
        Err(DirectoryNextError::Quit(flow)) => match flow.into_kind() {
            FlowKind::Signal(s) => assert_eq!(s.symbol_name(), "quit"),
            other => panic!("unexpected {other:?}"),
        },
        _ => panic!("retry should stop at quit"),
    }
}
#[test]
fn retry_distinguishes_end_of_stream_from_permanent_errors() {
    let mut polls = 0;
    assert!(
        next_with_retry(
            || Ok(None),
            || {
                polls += 1;
                Ok(())
            }
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(polls, 0);
    match next_with_retry(
        || Err(io::Error::from_raw_os_error(libc::EIO)),
        || {
            polls += 1;
            Ok(())
        },
    ) {
        Err(DirectoryNextError::Io(err)) => assert_eq!(err.raw_os_error(), Some(libc::EIO)),
        _ => panic!("expected EIO"),
    }
    assert_eq!(polls, 0);
}
