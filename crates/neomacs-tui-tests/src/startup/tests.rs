use super::StartupObservation;
use crate::{TuiLaunch, TuiSession};

#[test]
fn startup_observation_changes_no_launch_arguments_and_is_visible_in_diagnostics() {
    for startup in [
        StartupObservation::Unobserved,
        StartupObservation::NotCertified,
    ] {
        let launch = TuiLaunch::new("sh").args(["-c", "printf ready"]);
        let arguments = launch.args.clone();
        let launch = match startup {
            StartupObservation::Unobserved => launch,
            StartupObservation::NotCertified => launch.with_uncertified_startup(),
        };
        assert_eq!(launch.args, arguments, "no startup protocol is installed");
        let mut session = TuiSession::spawn_launch(launch, "startup diagnostic");
        assert_eq!(session.startup_observation(), startup);
        assert!(session.name.to_string().contains(&format!("{startup:?}")));
        assert_eq!(session.name, "startup diagnostic");
        let recorded_name = session.name.clone();
        assert_eq!(&recorded_name, "startup diagnostic");
        session
            .finish()
            .expect("finish generic startup observation");
    }
}
