//! Transfer-mechanics tests for the data-control reader.  The MIME policy it
//! shares with the other backends is tested in `text_policy_test.rs`.

use super::*;

#[test]
fn transfer_reads_until_the_owner_closes_its_end() {
    use std::io::Write;
    let (mut reader, mut writer) = std::io::pipe().unwrap();
    let owner = std::thread::spawn(move || {
        writer.write_all(b"first ").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        writer.write_all(b"second").unwrap();
    });
    let bytes = read_to_end_before(&mut reader, Instant::now() + Duration::from_secs(5)).unwrap();
    owner.join().unwrap();
    assert_eq!(bytes, b"first second");
}

#[test]
fn stalled_selection_owner_times_out() {
    let (mut reader, writer) = std::io::pipe().unwrap();
    let started = Instant::now();
    let result = read_to_end_before(&mut reader, started + Duration::from_millis(50));
    assert_eq!(result, Err("data-control transfer timed out".to_owned()));
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(writer);
}
