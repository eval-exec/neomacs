#[path = "surface_test.rs"]
mod surface;

std::cfg_select! {
    neomacs_have_dbus => {
        #[path = "call_test.rs"]
        mod call;
        #[path = "connection_test.rs"]
        mod connection;
        #[path = "signal_test.rs"]
        mod signal;
        #[path = "types_test.rs"]
        mod types;
    }
    _ => {}
}
