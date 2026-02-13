//! Network streams and enhanced subprocess I/O.
//!
//! Implements:
//! - TCP client connections (open-network-stream)
//! - Process filters (async output handlers)
//! - Process sentinels (state change handlers)
//! - URL fetching (basic HTTP)
//! - Pipe-based IPC
//! - Process output buffer management

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::error::{signal, Flow};
use super::value::Value;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Network connection types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionType {
    Plain,
    // Tls, // future: TLS support
}

/// Status of a network connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkStatus {
    Open,
    Closed,
    Failed(String),
    Connecting,
}

/// A TCP network stream.
pub struct NetworkStream {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub stream: Option<TcpStream>,
    pub buffer_name: Option<String>,
    pub filter: Option<String>,
    pub sentinel: Option<String>,
    pub status: NetworkStatus,
    pub output_buffer: Vec<u8>,
    pub coding_system: String,
    pub conn_type: ConnectionType,
}

/// Handles async output from a subprocess or network stream.
pub struct ProcessFilter {
    pub name: String,
    pub output_buffer: String,
}

/// Handles state changes for a process or network stream.
pub struct ProcessSentinel {
    pub name: String,
}

// ---------------------------------------------------------------------------
// NetworkManager
// ---------------------------------------------------------------------------

/// Central registry for network connections, process filters, and sentinels.
pub struct NetworkManager {
    connections: HashMap<u64, NetworkStream>,
    next_id: u64,
    process_filters: HashMap<u64, ProcessFilter>,
    process_sentinels: HashMap<u64, ProcessSentinel>,
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            next_id: 1,
            process_filters: HashMap::new(),
            process_sentinels: HashMap::new(),
        }
    }

    // -- Network streams ----------------------------------------------------

    /// Open a TCP connection to `host:port`.  Returns the connection id on
    /// success, or an error string on failure.
    pub fn open_connection(
        &mut self,
        name: &str,
        host: &str,
        port: u16,
        buffer: Option<&str>,
    ) -> Result<u64, String> {
        let addr_str = format!("{}:{}", host, port);
        let addrs: Vec<_> = addr_str
            .to_socket_addrs()
            .map_err(|e| format!("DNS resolution failed for {}: {}", addr_str, e))?
            .collect();

        if addrs.is_empty() {
            return Err(format!("No addresses found for {}", addr_str));
        }

        let stream = TcpStream::connect_timeout(&addrs[0], Duration::from_secs(30))
            .map_err(|e| format!("Connection to {} failed: {}", addr_str, e))?;

        // Set a default read timeout so receive_data doesn't block forever.
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

        let id = self.next_id;
        self.next_id += 1;

        let conn = NetworkStream {
            id,
            name: name.to_string(),
            host: host.to_string(),
            port,
            stream: Some(stream),
            buffer_name: buffer.map(|s| s.to_string()),
            filter: None,
            sentinel: None,
            status: NetworkStatus::Open,
            output_buffer: Vec::new(),
            coding_system: "utf-8".to_string(),
            conn_type: ConnectionType::Plain,
        };
        self.connections.insert(id, conn);
        Ok(id)
    }

    /// Close a network connection.  Returns true if the connection existed.
    pub fn close_connection(&mut self, id: u64) -> bool {
        if let Some(conn) = self.connections.get_mut(&id) {
            // Drop the TcpStream, which closes the socket.
            conn.stream = None;
            conn.status = NetworkStatus::Closed;
            true
        } else {
            false
        }
    }

    /// Send raw bytes over a connection.  Returns the number of bytes
    /// written.
    pub fn send_data(&mut self, id: u64, data: &[u8]) -> Result<usize, String> {
        let conn = self
            .connections
            .get_mut(&id)
            .ok_or_else(|| format!("No connection with id {}", id))?;

        match conn.status {
            NetworkStatus::Open => {}
            ref status => {
                return Err(format!("Connection {} is not open (status: {:?})", id, status));
            }
        }

        let stream = conn
            .stream
            .as_mut()
            .ok_or_else(|| format!("Connection {} has no underlying stream", id))?;

        let n = stream
            .write(data)
            .map_err(|e| format!("Write error on connection {}: {}", id, e))?;

        stream
            .flush()
            .map_err(|e| format!("Flush error on connection {}: {}", id, e))?;

        Ok(n)
    }

    /// Receive data from a connection.  An optional timeout overrides the
    /// socket's default read timeout.  Returns the bytes read (may be empty
    /// if the timeout expires with no data).
    pub fn receive_data(
        &mut self,
        id: u64,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>, String> {
        let conn = self
            .connections
            .get_mut(&id)
            .ok_or_else(|| format!("No connection with id {}", id))?;

        match conn.status {
            NetworkStatus::Open => {}
            ref status => {
                return Err(format!("Connection {} is not open (status: {:?})", id, status));
            }
        }

        let stream = conn
            .stream
            .as_mut()
            .ok_or_else(|| format!("Connection {} has no underlying stream", id))?;

        if let Some(dur) = timeout {
            let _ = stream.set_read_timeout(Some(dur));
        }

        let mut buf = vec![0u8; 4096];
        match stream.read(&mut buf) {
            Ok(0) => {
                // EOF — remote closed connection.
                conn.status = NetworkStatus::Closed;
                Ok(Vec::new())
            }
            Ok(n) => {
                buf.truncate(n);
                // Also append to the connection's output_buffer for later
                // consumption by accept-process-output.
                conn.output_buffer.extend_from_slice(&buf);
                Ok(buf)
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout expired, no data available.
                Ok(Vec::new())
            }
            Err(e) => {
                conn.status = NetworkStatus::Failed(e.to_string());
                Err(format!("Read error on connection {}: {}", id, e))
            }
        }
    }

    /// Query the status of a connection.
    pub fn connection_status(&self, id: u64) -> Option<&NetworkStatus> {
        self.connections.get(&id).map(|c| &c.status)
    }

    /// Get a reference to a connection by id.
    pub fn get_connection(&self, id: u64) -> Option<&NetworkStream> {
        self.connections.get(&id)
    }

    /// List all connections: (id, name, host, port).
    pub fn list_connections(&self) -> Vec<(u64, &str, &str, u16)> {
        self.connections
            .values()
            .map(|c| (c.id, c.name.as_str(), c.host.as_str(), c.port))
            .collect()
    }

    /// Delete a connection entirely (close + remove from registry).
    pub fn delete_connection(&mut self, id: u64) -> bool {
        if let Some(mut conn) = self.connections.remove(&id) {
            conn.stream = None;
            // Also remove associated filter/sentinel.
            self.process_filters.remove(&id);
            self.process_sentinels.remove(&id);
            true
        } else {
            false
        }
    }

    // -- Process filters and sentinels --------------------------------------

    /// Set the filter function for a process/connection id.
    pub fn set_process_filter(&mut self, process_id: u64, filter_name: &str) {
        self.process_filters.insert(
            process_id,
            ProcessFilter {
                name: filter_name.to_string(),
                output_buffer: String::new(),
            },
        );
    }

    /// Set the sentinel function for a process/connection id.
    pub fn set_process_sentinel(&mut self, process_id: u64, sentinel_name: &str) {
        self.process_sentinels.insert(
            process_id,
            ProcessSentinel {
                name: sentinel_name.to_string(),
            },
        );
    }

    /// Get the filter function name for a process/connection.
    pub fn get_process_filter(&self, process_id: u64) -> Option<&str> {
        self.process_filters
            .get(&process_id)
            .map(|f| f.name.as_str())
    }

    /// Get the sentinel function name for a process/connection.
    pub fn get_process_sentinel(&self, process_id: u64) -> Option<&str> {
        self.process_sentinels
            .get(&process_id)
            .map(|s| s.name.as_str())
    }

    /// Remove the filter for a process/connection.
    pub fn remove_process_filter(&mut self, process_id: u64) {
        self.process_filters.remove(&process_id);
    }

    /// Remove the sentinel for a process/connection.
    pub fn remove_process_sentinel(&mut self, process_id: u64) {
        self.process_sentinels.remove(&process_id);
    }

    // -- Output handling ----------------------------------------------------

    /// Drain and return accumulated output for a connection, decoding as
    /// UTF-8 (lossy).  An optional timeout triggers a receive attempt first.
    pub fn accept_process_output(
        &mut self,
        id: u64,
        timeout: Option<Duration>,
    ) -> Result<String, String> {
        // If a timeout is given, try to read fresh data first.
        if timeout.is_some() {
            let _ = self.receive_data(id, timeout);
        }

        let conn = self
            .connections
            .get_mut(&id)
            .ok_or_else(|| format!("No connection with id {}", id))?;

        let data = std::mem::take(&mut conn.output_buffer);
        Ok(String::from_utf8_lossy(&data).into_owned())
    }

    /// Whether there is un-consumed output buffered for a connection.
    pub fn process_output_pending(&self, id: u64) -> bool {
        self.connections
            .get(&id)
            .map(|c| !c.output_buffer.is_empty())
            .unwrap_or(false)
    }

    // -- URL fetching -------------------------------------------------------

    /// Perform a basic synchronous HTTP GET.  Connects to the host on port
    /// 80 (or 443 not yet supported), sends a minimal HTTP/1.0 request, and
    /// returns the entire response body.
    pub fn url_retrieve_synchronously(&mut self, url: &str) -> Result<String, String> {
        // Parse scheme, host, port, path from the URL.
        let (host, port, path) = parse_http_url(url)?;

        let addr_str = format!("{}:{}", host, port);
        let addrs: Vec<_> = addr_str
            .to_socket_addrs()
            .map_err(|e| format!("DNS resolution failed for {}: {}", host, e))?
            .collect();

        if addrs.is_empty() {
            return Err(format!("No addresses found for {}", host));
        }

        let mut stream = TcpStream::connect_timeout(&addrs[0], Duration::from_secs(30))
            .map_err(|e| format!("Connection to {}:{} failed: {}", host, port, e))?;

        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));

        let request = format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host,
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|e| format!("Read error: {}", e))?;

        let response_str = String::from_utf8_lossy(&response).into_owned();

        // Split headers from body at the first \r\n\r\n.
        if let Some(pos) = response_str.find("\r\n\r\n") {
            Ok(response_str[pos + 4..].to_string())
        } else {
            // No header/body separator found; return everything.
            Ok(response_str)
        }
    }
}

// ---------------------------------------------------------------------------
// URL parsing helper
// ---------------------------------------------------------------------------

/// Very basic HTTP URL parser.  Returns (host, port, path).
fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = url.strip_prefix("https://") {
        // We note the scheme but don't actually do TLS.
        stripped
    } else {
        return Err(format!("Unsupported URL scheme in: {}", url));
    };

    let default_port: u16 = if url.starts_with("https://") {
        443
    } else {
        80
    };

    // Split host(+port) from path.
    let (hostport, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };

    // Split host from port.
    let (host, port) = if let Some(colon_idx) = hostport.rfind(':') {
        let port_str = &hostport[colon_idx + 1..];
        let port: u16 = port_str
            .parse()
            .map_err(|_| format!("Invalid port in URL: {}", port_str))?;
        (&hostport[..colon_idx], port)
    } else {
        (hostport, default_port)
    };

    if host.is_empty() {
        return Err("Empty host in URL".to_string());
    }

    Ok((host.to_string(), port, path.to_string()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins (eval-dependent — need access to NetworkManager on the Evaluator)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- NetworkManager unit tests ------------------------------------------

    #[test]
    fn network_manager_new() {
        let nm = NetworkManager::new();
        assert_eq!(nm.connections.len(), 0);
        assert_eq!(nm.next_id, 1);
        assert!(nm.list_connections().is_empty());
    }

    #[test]
    fn network_manager_default_trait() {
        let nm = NetworkManager::default();
        assert_eq!(nm.connections.len(), 0);
    }

    // -- Process filter set/get/remove --------------------------------------

    #[test]
    fn process_filter_set_and_get() {
        let mut nm = NetworkManager::new();
        assert!(nm.get_process_filter(1).is_none());

        nm.set_process_filter(1, "my-filter-fn");
        assert_eq!(nm.get_process_filter(1), Some("my-filter-fn"));
    }

    #[test]
    fn process_filter_overwrite() {
        let mut nm = NetworkManager::new();
        nm.set_process_filter(1, "first");
        nm.set_process_filter(1, "second");
        assert_eq!(nm.get_process_filter(1), Some("second"));
    }

    #[test]
    fn process_filter_remove() {
        let mut nm = NetworkManager::new();
        nm.set_process_filter(1, "my-filter");
        nm.remove_process_filter(1);
        assert!(nm.get_process_filter(1).is_none());
    }

    #[test]
    fn process_filter_remove_nonexistent() {
        let mut nm = NetworkManager::new();
        // Should not panic.
        nm.remove_process_filter(999);
        assert!(nm.get_process_filter(999).is_none());
    }

    #[test]
    fn process_filter_multiple_ids() {
        let mut nm = NetworkManager::new();
        nm.set_process_filter(1, "filter-a");
        nm.set_process_filter(2, "filter-b");
        nm.set_process_filter(3, "filter-c");
        assert_eq!(nm.get_process_filter(1), Some("filter-a"));
        assert_eq!(nm.get_process_filter(2), Some("filter-b"));
        assert_eq!(nm.get_process_filter(3), Some("filter-c"));
    }

    // -- Process sentinel set/get/remove ------------------------------------

    #[test]
    fn process_sentinel_set_and_get() {
        let mut nm = NetworkManager::new();
        assert!(nm.get_process_sentinel(1).is_none());

        nm.set_process_sentinel(1, "my-sentinel-fn");
        assert_eq!(nm.get_process_sentinel(1), Some("my-sentinel-fn"));
    }

    #[test]
    fn process_sentinel_overwrite() {
        let mut nm = NetworkManager::new();
        nm.set_process_sentinel(1, "first");
        nm.set_process_sentinel(1, "second");
        assert_eq!(nm.get_process_sentinel(1), Some("second"));
    }

    #[test]
    fn process_sentinel_remove() {
        let mut nm = NetworkManager::new();
        nm.set_process_sentinel(1, "my-sentinel");
        nm.remove_process_sentinel(1);
        assert!(nm.get_process_sentinel(1).is_none());
    }

    #[test]
    fn process_sentinel_remove_nonexistent() {
        let mut nm = NetworkManager::new();
        nm.remove_process_sentinel(999);
        assert!(nm.get_process_sentinel(999).is_none());
    }

    // -- Connection lifecycle (no real TCP) ----------------------------------

    #[test]
    fn close_nonexistent_connection() {
        let mut nm = NetworkManager::new();
        assert!(!nm.close_connection(999));
    }

    #[test]
    fn delete_nonexistent_connection() {
        let mut nm = NetworkManager::new();
        assert!(!nm.delete_connection(999));
    }

    #[test]
    fn connection_status_nonexistent() {
        let nm = NetworkManager::new();
        assert!(nm.connection_status(999).is_none());
    }

    #[test]
    fn get_connection_nonexistent() {
        let nm = NetworkManager::new();
        assert!(nm.get_connection(999).is_none());
    }

    // -- Output buffer management -------------------------------------------

    #[test]
    fn process_output_pending_nonexistent() {
        let nm = NetworkManager::new();
        assert!(!nm.process_output_pending(999));
    }

    #[test]
    fn accept_output_nonexistent() {
        let mut nm = NetworkManager::new();
        let result = nm.accept_process_output(999, None);
        assert!(result.is_err());
    }

    // -- send_data on nonexistent connection --------------------------------

    #[test]
    fn send_data_nonexistent() {
        let mut nm = NetworkManager::new();
        let result = nm.send_data(999, b"hello");
        assert!(result.is_err());
    }

    // -- receive_data on nonexistent connection -----------------------------

    #[test]
    fn receive_data_nonexistent() {
        let mut nm = NetworkManager::new();
        let result = nm.receive_data(999, None);
        assert!(result.is_err());
    }

    // -- open_connection with refused port ------------------------------------

    #[test]
    fn open_connection_refused() {
        let mut nm = NetworkManager::new();
        // Port 1 on localhost should refuse connections on virtually all systems.
        let result = nm.open_connection("test", "127.0.0.1", 1, None);
        assert!(result.is_err());
    }

    // -- URL parser ---------------------------------------------------------

    #[test]
    fn parse_http_url_basic() {
        let (host, port, path) = parse_http_url("http://example.com/foo").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/foo");
    }

    #[test]
    fn parse_http_url_with_port() {
        let (host, port, path) = parse_http_url("http://example.com:8080/bar").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8080);
        assert_eq!(path, "/bar");
    }

    #[test]
    fn parse_http_url_no_path() {
        let (host, port, path) = parse_http_url("http://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/");
    }

    #[test]
    fn parse_https_url() {
        let (host, port, path) = parse_http_url("https://secure.example.com/api").unwrap();
        assert_eq!(host, "secure.example.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/api");
    }

    #[test]
    fn parse_url_unsupported_scheme() {
        let result = parse_http_url("ftp://example.com/file");
        assert!(result.is_err());
    }

    #[test]
    fn parse_url_empty_host() {
        let result = parse_http_url("http:///path");
        assert!(result.is_err());
    }

    #[test]
    fn parse_url_deep_path() {
        let (host, port, path) = parse_http_url("http://host.com/a/b/c?q=1").unwrap();
        assert_eq!(host, "host.com");
        assert_eq!(port, 80);
        assert_eq!(path, "/a/b/c?q=1");
    }

    // -- Helper function tests ----------------------------------------------

    #[test]
    fn expect_args_correct_count() {
        let args = vec![Value::Int(1), Value::Int(2)];
        assert!(expect_args("test", &args, 2).is_ok());
    }

    #[test]
    fn expect_args_wrong_count() {
        let args = vec![Value::Int(1)];
        assert!(expect_args("test", &args, 2).is_err());
    }

    #[test]
    fn expect_min_args_sufficient() {
        let args = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        assert!(expect_min_args("test", &args, 2).is_ok());
    }

    #[test]
    fn expect_min_args_insufficient() {
        let args = vec![Value::Int(1)];
        assert!(expect_min_args("test", &args, 3).is_err());
    }

    #[test]
    fn expect_string_from_str() {
        let v = Value::string("hello");
        assert_eq!(expect_string(&v).unwrap(), "hello");
    }

    #[test]
    fn expect_string_from_symbol() {
        let v = Value::symbol("foo");
        assert_eq!(expect_string(&v).unwrap(), "foo");
    }

    #[test]
    fn expect_string_from_nil() {
        assert_eq!(expect_string(&Value::Nil).unwrap(), "nil");
    }

    #[test]
    fn expect_string_from_true() {
        assert_eq!(expect_string(&Value::True).unwrap(), "t");
    }

    #[test]
    fn expect_string_wrong_type() {
        let v = Value::Int(42);
        assert!(expect_string(&v).is_err());
    }

    #[test]
    fn expect_int_from_int() {
        let v = Value::Int(42);
        assert_eq!(expect_int(&v).unwrap(), 42);
    }

    #[test]
    fn expect_int_from_char() {
        let v = Value::Char('A');
        assert_eq!(expect_int(&v).unwrap(), 65);
    }

    #[test]
    fn expect_int_wrong_type() {
        let v = Value::string("not a number");
        assert!(expect_int(&v).is_err());
    }

    // -- NetworkStatus / ConnectionType equality ----------------------------

    #[test]
    fn network_status_eq() {
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
        assert_eq!(ConnectionType::Plain, ConnectionType::Plain);
    }

    // -- Filter and sentinel coexistence ------------------------------------

    #[test]
    fn filter_and_sentinel_independent() {
        let mut nm = NetworkManager::new();
        nm.set_process_filter(1, "my-filter");
        nm.set_process_sentinel(1, "my-sentinel");
        assert_eq!(nm.get_process_filter(1), Some("my-filter"));
        assert_eq!(nm.get_process_sentinel(1), Some("my-sentinel"));

        nm.remove_process_filter(1);
        assert!(nm.get_process_filter(1).is_none());
        // Sentinel should be unaffected.
        assert_eq!(nm.get_process_sentinel(1), Some("my-sentinel"));
    }

    // -- List connections (empty) -------------------------------------------

    #[test]
    fn list_connections_empty() {
        let nm = NetworkManager::new();
        assert!(nm.list_connections().is_empty());
    }

    // -- url_retrieve_synchronously with bad URL scheme ---------------------

    #[test]
    fn url_retrieve_bad_scheme() {
        let mut nm = NetworkManager::new();
        let result = nm.url_retrieve_synchronously("ftp://example.com");
        assert!(result.is_err());
    }

    // -- url_retrieve_synchronously with refused port ------------------------

    #[test]
    fn url_retrieve_refused_port() {
        let mut nm = NetworkManager::new();
        // Port 1 on localhost should refuse connections.
        let result = nm.url_retrieve_synchronously("http://127.0.0.1:1/path");
        assert!(result.is_err());
    }
}
