use super::*;

#[test]
fn timer_vectors_normalize_lower_components_before_ordering() {
    let _context = Context::new();
    let mut slots = vec![Value::NIL; 10];
    slots[1] = Value::fixnum(0);
    slots[2] = Value::fixnum(0);
    slots[3] = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    slots[8] = Value::fixnum(0);
    // GNU timefns.c:decode_time_components carries these microseconds into
    // seconds. The audit's GNU timer probe remains pending instead of firing.
    let timer = pending_gnu_timer(Value::vector(slots)).unwrap();
    let now = GnuTimerTimestamp::now();
    assert!(timer.when > now);
    assert!(timer.when.duration_until(now).as_secs() > 1_000_000_000);
}

#[test]
fn timer_vectors_reject_seconds_outside_time_t() {
    let _context = Context::new();
    let mut slots = vec![Value::NIL; 10];
    slots[1] = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    slots[2] = Value::fixnum(0);
    slots[3] = Value::fixnum(0);
    slots[8] = Value::fixnum(0);
    assert!(pending_gnu_timer(Value::vector(slots)).is_none());
}

#[test]
fn timer_duration_saturates_like_gnu_timespec_sub() {
    let mut slots = vec![Value::NIL; 10];
    slots[1] = Value::fixnum(-(1_i64 << 47));
    slots[2] = Value::fixnum(0);
    slots[3] = Value::fixnum(0);
    slots[8] = Value::fixnum(0);
    let earliest = pending_gnu_timer(Value::vector(slots.clone()))
        .unwrap()
        .when;
    slots[1] = Value::fixnum((1_i64 << 47) - 1);
    slots[2] = Value::fixnum(65_535);
    slots[3] = Value::fixnum(999_999);
    slots[8] = Value::fixnum(999_000);
    let latest = pending_gnu_timer(Value::vector(slots)).unwrap().when;
    let saturated = Duration::new(i64::MAX as u64, 999_999_999);
    assert_eq!(latest.duration_until(earliest), saturated);
    assert_eq!(earliest.overdue_duration(latest), saturated);
    assert_eq!(earliest.duration_until(latest), Duration::ZERO);
    assert_eq!(latest.overdue_duration(earliest), Duration::ZERO);
}
