use super::*;
use crate::emacs_core::error::{expect_args, expect_min_args};

// -- NetworkManager unit tests ------------------------------------------

#[test]
fn network_manager_new() {
    crate::test_utils::init_test_tracing();
    let nm = NetworkManager::new();
    assert_eq!(nm.connections.len(), 0);
    assert_eq!(nm.next_id, 1);
    assert!(nm.list_connections().is_empty());
}

#[test]
fn network_manager_default_trait() {
    crate::test_utils::init_test_tracing();
    let nm = NetworkManager::default();
    assert_eq!(nm.connections.len(), 0);
}

// -- Process filter set/get/remove --------------------------------------

#[test]
fn process_filter_set_and_get() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    assert!(nm.get_process_filter(1).is_none());

    nm.set_process_filter(1, Value::symbol("my-filter-fn"));
    assert_eq!(
        nm.get_process_filter(1),
        Some(Value::symbol("my-filter-fn"))
    );
}

#[test]
fn process_filter_overwrite() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.set_process_filter(1, Value::symbol("first"));
    nm.set_process_filter(1, Value::symbol("second"));
    assert_eq!(nm.get_process_filter(1), Some(Value::symbol("second")));
}

#[test]
fn process_filter_remove() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.set_process_filter(1, Value::symbol("my-filter"));
    nm.remove_process_filter(1);
    assert!(nm.get_process_filter(1).is_none());
}

#[test]
fn process_filter_remove_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    // Should not panic.
    nm.remove_process_filter(999);
    assert!(nm.get_process_filter(999).is_none());
}

#[test]
fn process_filter_multiple_ids() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.set_process_filter(1, Value::symbol("filter-a"));
    nm.set_process_filter(2, Value::symbol("filter-b"));
    nm.set_process_filter(3, Value::symbol("filter-c"));
    assert_eq!(nm.get_process_filter(1), Some(Value::symbol("filter-a")));
    assert_eq!(nm.get_process_filter(2), Some(Value::symbol("filter-b")));
    assert_eq!(nm.get_process_filter(3), Some(Value::symbol("filter-c")));
}

// -- Process sentinel set/get/remove ------------------------------------

#[test]
fn process_sentinel_set_and_get() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    assert!(nm.get_process_sentinel(1).is_none());

    nm.set_process_sentinel(1, Value::symbol("my-sentinel-fn"));
    assert_eq!(
        nm.get_process_sentinel(1),
        Some(Value::symbol("my-sentinel-fn"))
    );
}

#[test]
fn process_sentinel_overwrite() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.set_process_sentinel(1, Value::symbol("first"));
    nm.set_process_sentinel(1, Value::symbol("second"));
    assert_eq!(nm.get_process_sentinel(1), Some(Value::symbol("second")));
}

#[test]
fn process_sentinel_remove() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.set_process_sentinel(1, Value::symbol("my-sentinel"));
    nm.remove_process_sentinel(1);
    assert!(nm.get_process_sentinel(1).is_none());
}

#[test]
fn process_sentinel_remove_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.remove_process_sentinel(999);
    assert!(nm.get_process_sentinel(999).is_none());
}

// -- Connection lifecycle (no real TCP) ----------------------------------

#[test]
fn close_nonexistent_connection() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    assert!(!nm.close_connection(999));
}

#[test]
fn delete_nonexistent_connection() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    assert!(!nm.delete_connection(999));
}

#[test]
fn connection_status_nonexistent() {
    crate::test_utils::init_test_tracing();
    let nm = NetworkManager::new();
    assert!(nm.connection_status(999).is_none());
}

#[test]
fn get_connection_nonexistent() {
    crate::test_utils::init_test_tracing();
    let nm = NetworkManager::new();
    assert!(nm.get_connection(999).is_none());
}

// -- Output buffer management -------------------------------------------

#[test]
fn process_output_pending_nonexistent() {
    crate::test_utils::init_test_tracing();
    let nm = NetworkManager::new();
    assert!(!nm.process_output_pending(999));
}

#[test]
fn accept_output_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    let result = nm.accept_process_output(999, None);
    assert!(result.is_err());
}

// -- send_data on nonexistent connection --------------------------------

#[test]
fn send_data_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    let result = nm.send_data(999, b"hello");
    assert!(result.is_err());
}

// -- receive_data on nonexistent connection -----------------------------

#[test]
fn receive_data_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    let result = nm.receive_data(999, None);
    assert!(result.is_err());
}

// -- open_connection with refused port ------------------------------------

#[test]
fn open_connection_refused() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    // Port 1 on localhost should refuse connections on virtually all systems.
    let result = nm.open_connection("test", "127.0.0.1", 1, None);
    assert!(result.is_err());
}

// -- Helper function tests ----------------------------------------------

#[test]
fn expect_args_correct_count() {
    crate::test_utils::init_test_tracing();
    let args = vec![Value::fixnum(1), Value::fixnum(2)];
    assert!(expect_args("test", &args, 2).is_ok());
}

#[test]
fn expect_args_wrong_count() {
    crate::test_utils::init_test_tracing();
    let args = vec![Value::fixnum(1)];
    assert!(expect_args("test", &args, 2).is_err());
}

#[test]
fn expect_min_args_sufficient() {
    crate::test_utils::init_test_tracing();
    let args = vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)];
    assert!(expect_min_args("test", &args, 2).is_ok());
}

#[test]
fn expect_min_args_insufficient() {
    crate::test_utils::init_test_tracing();
    let args = vec![Value::fixnum(1)];
    assert!(expect_min_args("test", &args, 3).is_err());
}

#[test]
fn expect_string_from_str() {
    crate::test_utils::init_test_tracing();
    let v = Value::string("hello");
    assert_eq!(expect_string(&v).unwrap(), "hello");
}

#[test]
fn expect_string_from_symbol() {
    crate::test_utils::init_test_tracing();
    let v = Value::symbol("foo");
    assert_eq!(expect_string(&v).unwrap(), "foo");
}

#[test]
fn expect_string_from_nil() {
    crate::test_utils::init_test_tracing();
    assert_eq!(expect_string(&Value::NIL).unwrap(), "nil");
}

#[test]
fn expect_string_from_true() {
    crate::test_utils::init_test_tracing();
    assert_eq!(expect_string(&Value::T).unwrap(), "t");
}

#[test]
fn expect_string_wrong_type() {
    crate::test_utils::init_test_tracing();
    let v = Value::fixnum(42);
    assert!(expect_string(&v).is_err());
}

#[test]
fn expect_int_from_int() {
    crate::test_utils::init_test_tracing();
    let v = Value::fixnum(42);
    assert_eq!(expect_int(&v).unwrap(), 42);
}

#[test]
fn expect_int_from_char() {
    crate::test_utils::init_test_tracing();
    let v = Value::char('A');
    assert_eq!(expect_int(&v).unwrap(), 65);
}

#[test]
fn expect_int_wrong_type() {
    crate::test_utils::init_test_tracing();
    let v = Value::string("not a number");
    assert!(expect_int(&v).is_err());
}

// -- NetworkStatus / ConnectionType equality ----------------------------

#[test]
fn network_status_eq() {
    crate::test_utils::init_test_tracing();
    assert_eq!(NetworkStatus::Open, NetworkStatus::Open);
    assert_eq!(NetworkStatus::Closed, NetworkStatus::Closed);
    assert_eq!(NetworkStatus::Connecting, NetworkStatus::Connecting);
    assert_eq!(
        NetworkStatus::Failed("err".into()),
        NetworkStatus::Failed("err".into())
    );
    assert_ne!(NetworkStatus::Open, NetworkStatus::Closed);
}

#[test]
fn connection_type_eq() {
    crate::test_utils::init_test_tracing();
    assert_eq!(ConnectionType::Plain, ConnectionType::Plain);
}

// -- Filter and sentinel coexistence ------------------------------------

#[test]
fn filter_and_sentinel_independent() {
    crate::test_utils::init_test_tracing();
    let mut nm = NetworkManager::new();
    nm.set_process_filter(1, Value::symbol("my-filter"));
    nm.set_process_sentinel(1, Value::symbol("my-sentinel"));
    assert_eq!(nm.get_process_filter(1), Some(Value::symbol("my-filter")));
    assert_eq!(
        nm.get_process_sentinel(1),
        Some(Value::symbol("my-sentinel"))
    );

    nm.remove_process_filter(1);
    assert!(nm.get_process_filter(1).is_none());
    // Sentinel should be unaffected.
    assert_eq!(
        nm.get_process_sentinel(1),
        Some(Value::symbol("my-sentinel"))
    );
}

// -- List connections (empty) -------------------------------------------

#[test]
fn list_connections_empty() {
    crate::test_utils::init_test_tracing();
    let nm = NetworkManager::new();
    assert!(nm.list_connections().is_empty());
}
