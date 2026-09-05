//! Network streams and enhanced subprocess I/O.
//!
//! Implements:
//! - TCP client connections (open-network-stream)
//! - Process filters (async output handlers)
//! - Process sentinels (state change handlers)
//! - URL fetching (basic HTTP)
//! - Pipe-based IPC
//! - Process output buffer management

pub(crate) mod http;

#[cfg(test)]
use crate::emacs_core::error::LispCondition;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[cfg(test)]
use super::error::{Flow, signal};
use super::value::Value;
#[cfg(test)]
use super::value::ValueKind;
use crate::heap_types::LispString;

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
    pub name: Value,
    pub host: String,
    pub port: u16,
    pub stream: Option<TcpStream>,
    pub buffer_name: Value,
    pub filter: Value,
    pub sentinel: Value,
    pub status: NetworkStatus,
    pub output_buffer: Vec<u8>,
    pub coding_system: Value,
    pub conn_type: ConnectionType,
}

/// Handles async output from a subprocess or network stream.
pub struct ProcessFilter {
    pub function: Value,
    pub output_buffer: LispString,
}

/// Handles state changes for a process or network stream.
pub struct ProcessSentinel {
    pub function: Value,
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
            name: Value::string(name),
            host: host.to_string(),
            port,
            stream: Some(stream),
            buffer_name: buffer.map(Value::string).unwrap_or(Value::NIL),
            filter: Value::NIL,
            sentinel: Value::NIL,
            status: NetworkStatus::Open,
            output_buffer: Vec::new(),
            coding_system: Value::symbol("utf-8"),
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
                return Err(format!(
                    "Connection {} is not open (status: {:?})",
                    id, status
                ));
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
    pub fn receive_data(&mut self, id: u64, timeout: Option<Duration>) -> Result<Vec<u8>, String> {
        let conn = self
            .connections
            .get_mut(&id)
            .ok_or_else(|| format!("No connection with id {}", id))?;

        match conn.status {
            NetworkStatus::Open => {}
            ref status => {
                return Err(format!(
                    "Connection {} is not open (status: {:?})",
                    id, status
                ));
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
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
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
    pub fn list_connections(&self) -> Vec<(u64, Value, &str, u16)> {
        self.connections
            .values()
            .map(|c| (c.id, c.name, c.host.as_str(), c.port))
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
    pub fn set_process_filter(&mut self, process_id: u64, filter: Value) {
        self.process_filters.insert(
            process_id,
            ProcessFilter {
                function: filter,
                output_buffer: LispString::from_utf8(""),
            },
        );
    }

    /// Set the sentinel function for a process/connection id.
    pub fn set_process_sentinel(&mut self, process_id: u64, sentinel: Value) {
        self.process_sentinels
            .insert(process_id, ProcessSentinel { function: sentinel });
    }

    /// Get the filter function name for a process/connection.
    pub fn get_process_filter(&self, process_id: u64) -> Option<Value> {
        self.process_filters.get(&process_id).map(|f| f.function)
    }

    /// Get the sentinel function name for a process/connection.
    pub fn get_process_sentinel(&self, process_id: u64) -> Option<Value> {
        self.process_sentinels.get(&process_id).map(|s| s.function)
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

    let default_port: u16 = if url.starts_with("https://") { 443 } else { 80 };

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

#[cfg(test)]
fn expect_string(value: &Value) -> Result<String, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .expect("ValueKind::String must carry LispString payload")),
        ValueKind::Symbol(id) => Ok(crate::emacs_core::intern::resolve_sym(id).to_owned()),
        ValueKind::Nil => Ok("nil".to_string()),
        ValueKind::T => Ok("t".to_string()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

#[cfg(test)]
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins (eval-dependent — need access to NetworkManager on the Context)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
