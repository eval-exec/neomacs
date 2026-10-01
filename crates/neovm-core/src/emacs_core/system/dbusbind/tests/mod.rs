mod surface;

std::cfg_select! {
    neomacs_have_dbus => {
        mod call;
        mod connection;
        mod signal;
        mod types;
    }
    _ => {}
}
