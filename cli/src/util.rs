use std::net::TcpListener;

/// Binds to "127.0.0.1:0" to let the OS assign an available port,
/// then returns the listener.
pub fn listen_to_free_port(host: &str) -> TcpListener {
    TcpListener::bind(format!("{host}:0")).expect("Could not bind to a free port")
}
