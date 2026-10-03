use super::*;

#[test]
fn ordinary_startup_is_unlimited_and_reply_budget_is_independent() {
    for args in [vec![], vec!["-w", "1"], vec!["--timeout=0"]] {
        let options = parse_options("client", args.into_iter().map(OsString::from)).unwrap();
        assert_eq!(options.startup_timeout, None);
    }
    let options = parse_options(
        "client",
        ["-w", "1", "--startup-timeout=3"].map(OsString::from),
    )
    .unwrap();
    assert_eq!(options.timeout, Some(Duration::from_secs(1)));
    assert_eq!(options.startup_timeout, Some(Duration::from_secs(3)));
    let options = parse_options(
        "client",
        ["-w", "1", "-w", "0", "--startup-timeout", "0"].map(OsString::from),
    )
    .unwrap();
    assert_eq!(options.timeout, None);
    assert_eq!(options.startup_timeout, None);
    for args in [
        vec!["--startup-timeout"],
        vec!["--startup-timeout=-1"],
        vec!["--startup-timeout=bad"],
    ] {
        assert!(parse_options("client", args.into_iter().map(OsString::from)).is_err());
    }
}
