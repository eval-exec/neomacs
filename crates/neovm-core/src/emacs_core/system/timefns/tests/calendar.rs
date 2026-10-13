use super::*;

#[test]
fn calendar_fields_validate_before_normalization() {
    assert!(CalendarTime::try_from([0, 0, 0, 4_294_967_297, 1, 2000]).is_err());
    assert!(CalendarTime::try_from([0, 0, 0, 1, 1, 1_099_511_627_776]).is_err());
    let epoch = CalendarTime::try_from([0, 0, 0, 1, 1, 1970]).unwrap();
    assert_eq!(epoch.epoch_seconds().unwrap(), 0);
    #[cfg(unix)]
    {
        let tm = epoch.into_tm();
        assert_eq!((tm.tm_mon, tm.tm_year), (0, 70));
    }
}
