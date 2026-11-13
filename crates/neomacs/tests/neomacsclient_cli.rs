use std::process::Command;

/// Create the local-socket fixture in a directory whose full path fits `sun_path`.
///
/// The fixture historically lived under `<workspace>/tmp`; a deeply nested
/// worktree pushes `<dir>/server` past the 108-byte `sun_path` limit, and
/// `UnixListener::bind` fails with `InvalidInput` before the client ever runs.
/// Prefer the repo-local directory when it fits, otherwise the system temp dir.
#[cfg(unix)]
fn socket_tempdir(prefix: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::path::Path;

    const MAX_SOCKET_PATH: usize = 100;

    fn fits(dir: &Path) -> bool {
        dir.join("server").as_os_str().len() + 1 <= MAX_SOCKET_PATH
    }

    let repo_tmp = neomacs_infra::workspace_root().join("tmp");
    std::fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let repo_dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(&repo_tmp)
        .expect("repo-local tempdir");
    if fits(repo_dir.path()) {
        let socket = repo_dir.path().join("server");
        return (repo_dir, socket);
    }
    drop(repo_dir);

    let system_dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(std::env::temp_dir())
        .expect("system tempdir");
    assert!(
        fits(system_dir.path()),
        "no socket directory under the {MAX_SOCKET_PATH} byte sun_path limit for prefix {prefix:?}"
    );
    let socket = system_dir.path().join("server");
    (system_dir, socket)
}

/// Windows has no `sun_path` limit. The TCP server-file test only needs the
/// directory; the returned socket path is unused on this target.
#[cfg(windows)]
fn socket_tempdir(prefix: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("tempdir");
    let socket = dir.path().join("server");
    (dir, socket)
}

#[test]
fn neomacsclient_version_matches_emacs_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--version")
        .output()
        .expect("neomacsclient --version should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "neomacsclient 31.1\n"
    );
}

#[cfg(unix)]
#[test]
fn neomacsclient_sends_gnu_server_request_over_local_socket() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    let (_dir, socket) = socket_tempdir("neomacsclient-cli-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            request.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        stream
            .write_all(b"-print OK&&done\n")
            .expect("write response");
        String::from_utf8(request).expect("utf8 request")
    });

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--no-wait")
        .arg("--eval")
        .arg("(message \"a b\")")
        .arg("--frame-parameters")
        .arg("((name . ignored-on-current-frame))")
        .output()
        .expect("neomacsclient should run");

    assert!(output.status.success(), "{output:?}");
    let request = server.join().expect("server thread should finish");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "OK&done");
    assert!(request.starts_with("-dir "));
    assert!(request.contains(" -nowait "));
    assert!(request.contains(" -current-frame "));
    assert!(!request.contains(" -frame-parameters "));
    assert!(request.contains(" -eval (message&_\"a&_b\") "));
    assert!(request.ends_with(" \n"));
}

#[cfg(unix)]
#[test]
fn neomacsclient_parent_id_implies_a_new_graphical_frame() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;

    let (_dir, socket) = socket_tempdir("neomacsclient-parent-frame-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            request.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        stream.write_all(b"-print PARENT_OK\n").expect("reply");
        String::from_utf8(request).expect("utf8 request")
    });

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--parent-id")
        .arg("42")
        .env("DISPLAY", ":9")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("neomacsclient should run");

    assert!(output.status.success(), "{output:?}");
    let request = server.join().expect("server thread should finish");
    assert!(request.contains(" -parent-id 42 "));
    assert!(request.contains(" -window-system "));
    assert!(!request.contains(" -current-frame "));
}

#[cfg(unix)]
#[test]
fn neomacsclient_create_frame_requests_window_system_without_display_arg() {
    // The name predates GNU's tty-frame transport: this test once pinned
    // `-window-system` for `-c` with no display.  GNU 31.1 resolves a
    // frame-creating request with no available display as a TTY request
    // (lib-src/emacsclient.c:659-661), sends `-tty NAME TERM` (:2110-2121),
    // and sends `-window-system` only when `create_frame && !tty` (:2126-2127).
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;
    use std::thread;

    let (_dir, socket) = socket_tempdir("neomacsclient-create-frame-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            request.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        stream
            .write_all(b"-print FRAME_OK\n")
            .expect("write response");
        String::from_utf8(request).expect("utf8 request")
    });

    let mut master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open PTY slave");
    let master_flags = rustix::fs::fcntl_getfl(&master).expect("read PTY master flags");
    rustix::fs::fcntl_setfl(&master, master_flags | rustix::fs::OFlags::NONBLOCK)
        .expect("set the PTY master non-blocking");

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("TERM", "xterm")
        .stdout(Stdio::from(slave))
        .output()
        .expect("neomacsclient should run");

    let request = server.join().expect("server thread should finish");
    // GNU writes the server's `-print' reply to the client's stdout; with
    // stdout bound to the PTY slave, read the reply back from the master.
    // The master is non-blocking and the child has exited, so this drains
    // the buffered output and stops at EOF, EAGAIN, or the closed-slave EIO.
    let mut reply = Vec::new();
    let mut buf = [0u8; 256];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match master.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => reply.extend_from_slice(&buf[..n]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => panic!("read the PTY master: {error}"),
        }
    }
    drop(master);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&reply),
        "FRAME_OK",
        "the client's stdout (the PTY) must carry the server's -print reply"
    );
    // GNU emacsclient sends the creating client's environment before `-dir',
    // so a daemon-created frame observes DISPLAY and the rest of the client's
    // process environment (lib-src/emacsclient.c, "Send over our environment").
    assert!(request.starts_with("-env "));
    assert!(request.contains(" -dir "));
    assert!(request.contains(" -nowait "));
    assert!(!request.contains(" -display "));
    // No display means a TTY request: GNU sends the client's tty identity and
    // does not ask the server for a window-system frame.
    assert!(request.contains(&format!(" -tty {slave_name} xterm ")));
    assert!(!request.contains(" -window-system "));
    assert!(!request.contains(" -current-frame "));
    assert!(request.contains(" -file file.txt "));
    assert!(request.ends_with(" \n"));
}

#[cfg(unix)]
#[test]
fn neomacsclient_fatal_tty_error_matches_gnu_when_stdout_is_not_a_terminal() {
    use std::os::unix::net::UnixListener;

    let (_dir, socket) = socket_tempdir("neomacsclient-fatal-tty-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("TERM", "xterm")
        .output()
        .expect("neomacsclient should run");

    // GNU find_tty (lib-src/emacsclient.c:1136-1146) fails with
    // "<progname>: could not get terminal name\n" and EXIT_FAILURE when a
    // tty frame was requested and stdout is not a terminal; progname is
    // argv[0] verbatim (lib-src/emacsclient.c:1995).
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: could not get terminal name\n",
            env!("CARGO_BIN_EXE_neomacsclient")
        )
    );
    assert!(output.stdout.is_empty(), "{output:?}");
    drop(listener);
}

#[cfg(unix)]
#[test]
fn neomacsclient_fatal_tty_checks_the_terminal_name_before_term() {
    use std::os::unix::net::UnixListener;

    let (_dir, socket) = socket_tempdir("neomacsclient-tty-order-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("TERM")
        .output()
        .expect("neomacsclient should run");

    // GNU find_tty checks ttyname (lib-src/emacsclient.c:1139-1146) BEFORE
    // TERM (:1149-1155): with stdout not a terminal and TERM unset, the
    // terminal name is the reported failure.
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: could not get terminal name\n",
            env!("CARGO_BIN_EXE_neomacsclient")
        )
    );
    drop(listener);
}

#[cfg(unix)]
#[test]
fn neomacsclient_tty_requires_term_when_stdout_is_a_terminal() {
    use std::fs::File;
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;

    let (_dir, socket) = socket_tempdir("neomacsclient-tty-term-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open PTY slave");

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("TERM")
        .stdout(Stdio::from(slave))
        .output()
        .expect("neomacsclient should run");

    // A terminal with no TERM reaches GNU find_tty's second failure
    // (lib-src/emacsclient.c:1149-1155).
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: please set the TERM variable to your terminal type\n",
            env!("CARGO_BIN_EXE_neomacsclient")
        )
    );
    drop(master);
}

#[cfg(unix)]
#[test]
fn neomacsclient_tty_accepts_an_empty_term() {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;
    use std::thread;

    let (_dir, socket) = socket_tempdir("neomacsclient-tty-empty-term-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            request.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        stream
            .write_all(b"-print FRAME_OK\n")
            .expect("write response");
        String::from_utf8(request).expect("utf8 request")
    });

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open PTY slave");

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("TERM", "")
        .stdout(Stdio::from(slave))
        .output()
        .expect("neomacsclient should run");

    let request = server.join().expect("server thread should finish");
    drop(master);
    // GNU egetenv is getenv (lib-src/emacsclient.c:65): TERM="" is SET and is
    // sent as an empty tty type, so the request proceeds.
    assert!(output.status.success(), "{output:?}");
    assert!(
        request.contains(&format!(" -tty {slave_name}  ")),
        "{request}"
    );
    assert!(request.contains(" -file file.txt "));
    assert!(request.ends_with(" \n"));
    assert!(!request.contains(" -window-system "));
}

#[cfg(unix)]
#[test]
fn neomacsclient_term_buffer_tty_is_rejected_like_gnu() {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};

    let (_dir, socket) = socket_tempdir("neomacsclient-term-buffer-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");
    listener
        .set_nonblocking(true)
        .expect("set listener non-blocking");

    // The client connects before find_tty, so a tolerant server drains the
    // connection and replies only if a full request was actually sent.  The
    // GNU :1158-1168 rejection path sends nothing and closes.
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = Vec::new();
                    let mut byte = [0u8; 1];
                    loop {
                        match stream.read(&mut byte) {
                            Ok(0) => break,
                            Ok(_) => {
                                request.push(byte[0]);
                                if byte[0] == b'\n' {
                                    let _ = stream.write_all(b"-print FRAME_OK\n");
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    return request;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Vec::new();
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept client: {error}"),
            }
        }
    });

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open PTY slave");

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env("TERM", "eterm-color")
        .env("INSIDE_EMACS", "foo,term:1")
        .stdout(Stdio::from(slave))
        .output()
        .expect("neomacsclient should run");

    let request = server.join().expect("server thread should finish");
    drop(master);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: opening a frame in an Emacs term buffer is not supported\n",
            env!("CARGO_BIN_EXE_neomacsclient")
        )
    );
    assert!(
        request.is_empty(),
        "the rejected request sent bytes: {request:?}"
    );
}

#[cfg(unix)]
#[test]
fn neomacsclient_tty_identifies_its_terminal_to_the_server() {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;
    use std::thread;

    let (_dir, socket) = socket_tempdir("neomacsclient-tty-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open PTY slave");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            request.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        stream
            .write_all(b"-print FRAME_OK\n")
            .expect("write response");
        String::from_utf8(request).expect("utf8 request")
    });

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("-t")
        .env("TERM", "xterm-256color")
        .stdout(Stdio::from(slave))
        .output()
        .expect("neomacsclient -t should run");

    let request = server.join().expect("server thread should finish");
    drop(master);
    assert!(output.status.success(), "{output:?}");
    assert!(request.contains(&format!(" -tty {slave_name} xterm-256color ")));
    assert!(request.contains(" -env TERM=xterm-256color "));
    assert!(!request.contains(" -window-system "));
    assert!(!request.contains(" -current-frame "));
}

#[cfg(unix)]
#[test]
fn neomacsclient_tty_forwards_resize_to_the_server_process() {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    let (_dir, socket) = socket_tempdir("neomacsclient-resize-");
    let listener = UnixListener::bind(&socket).expect("bind local socket");

    let resize_seen = Arc::new(AtomicBool::new(false));
    let signal_id = signal_hook::flag::register(libc::SIGWINCH, Arc::clone(&resize_seen))
        .expect("install resize observer");
    let test_pid = std::process::id();
    let (release_server, await_release) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            if byte[0] == b'\n' {
                break;
            }
        }
        writeln!(stream, "-emacs-pid {test_pid}").expect("advertise server PID");
        stream.flush().expect("flush server PID");
        await_release.recv().expect("test releases server");
        stream.write_all(b"-print DONE\n").expect("finish response");
    });

    let master = File::from(
        rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR | rustix::pty::OpenptFlags::NOCTTY)
            .expect("posix_openpt failed"),
    );
    rustix::pty::grantpt(&master).expect("grantpt failed");
    rustix::pty::unlockpt(&master).expect("unlockpt failed");
    let slave_name = rustix::pty::ptsname(&master, Vec::new())
        .expect("ptsname_r failed")
        .to_string_lossy()
        .into_owned();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&slave_name)
        .expect("open PTY slave");
    let mut client = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("-t")
        .env("TERM", "xterm-256color")
        .stdout(Stdio::from(slave))
        .spawn()
        .expect("spawn neomacsclient -t");

    let deadline = Instant::now() + Duration::from_secs(2);
    while !resize_seen.load(Ordering::Acquire) && Instant::now() < deadline {
        unsafe { libc::kill(client.id() as libc::pid_t, libc::SIGWINCH) };
        thread::sleep(Duration::from_millis(10));
    }
    let forwarded = resize_seen.load(Ordering::Acquire);
    release_server.send(()).expect("release fake server");
    let status = client.wait().expect("wait for neomacsclient");
    server.join().expect("server thread should finish");
    signal_hook::low_level::unregister(signal_id);
    drop(master);

    assert!(status.success(), "neomacsclient failed: {status}");
    assert!(
        forwarded,
        "SIGWINCH was not forwarded to the PID advertised by the server"
    );
}

#[test]
fn neomacsclient_sends_gnu_auth_for_tcp_server_file() {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    let (dir, _socket) = socket_tempdir("neomacsclient-tcp-");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind tcp listener");
    let port = listener.local_addr().expect("local addr").port();
    let auth_key = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!?";
    let auth_file = dir.path().join("server-auth");
    fs::write(&auth_file, format!("127.0.0.1:{port} 12345\n{auth_key}")).expect("write auth file");

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).expect("read request byte");
            request.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        stream
            .write_all(b"-print TCP_OK\n")
            .expect("write response");
        String::from_utf8(request).expect("utf8 request")
    });

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--server-file")
        .arg(&auth_file)
        .arg("--eval")
        .arg("(+ 1 2)")
        .output()
        .expect("neomacsclient should run");

    let request = server.join().expect("server thread should finish");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "TCP_OK");
    assert!(request.starts_with(&format!("-auth {auth_key} -dir ")));
    assert!(request.contains(" -current-frame "));
    assert!(request.contains(" -eval (+&_1&_2) "));
    assert!(request.ends_with(" \n"));
}
