use std::process::Command;

#[test]
fn neomacsclient_version_matches_emacs_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--version")
        .output()
        .expect("neomacsclient --version should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "neomacsclient 31.0.90\n"
    );
}

#[cfg(unix)]
#[test]
fn neomacsclient_sends_gnu_server_request_over_local_socket() {
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::thread;

    let repo_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let dir = tempfile::Builder::new()
        .prefix("neomacsclient-cli-")
        .tempdir_in(repo_tmp)
        .expect("repo-local tempdir");
    let socket = dir.path().join("server");
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
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::thread;

    let repo_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let dir = tempfile::Builder::new()
        .prefix("neomacsclient-parent-frame-")
        .tempdir_in(repo_tmp)
        .expect("repo-local tempdir");
    let socket = dir.path().join("server");
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
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::thread;

    let repo_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let dir = tempfile::Builder::new()
        .prefix("neomacsclient-create-frame-")
        .tempdir_in(repo_tmp)
        .expect("repo-local tempdir");
    let socket = dir.path().join("server");
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

    let output = Command::new(env!("CARGO_BIN_EXE_neomacsclient"))
        .arg("--socket-name")
        .arg(&socket)
        .arg("--create-frame")
        .arg("--no-wait")
        .arg("file.txt")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("neomacsclient should run");

    let request = server.join().expect("server thread should finish");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "FRAME_OK");
    // GNU emacsclient sends the creating client's environment before `-dir',
    // so a daemon-created frame observes DISPLAY and the rest of the client's
    // process environment (lib-src/emacsclient.c, "Send over our environment").
    assert!(request.starts_with("-env "));
    assert!(request.contains(" -dir "));
    assert!(request.contains(" -nowait "));
    assert!(!request.contains(" -display "));
    assert!(request.contains(" -window-system "));
    assert!(!request.contains(" -current-frame "));
    assert!(request.contains(" -file file.txt "));
    assert!(request.ends_with(" \n"));
}

#[cfg(unix)]
#[test]
fn neomacsclient_tty_identifies_its_terminal_to_the_server() {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::thread;

    let repo_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let dir = tempfile::Builder::new()
        .prefix("neomacsclient-tty-")
        .tempdir_in(repo_tmp)
        .expect("repo-local tempdir");
    let socket = dir.path().join("server");
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
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    let repo_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let dir = tempfile::Builder::new()
        .prefix("neomacsclient-resize-")
        .tempdir_in(repo_tmp)
        .expect("repo-local tempdir");
    let socket = dir.path().join("server");
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
    use std::path::PathBuf;
    use std::thread;

    let repo_tmp = PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("tmp");
    fs::create_dir_all(&repo_tmp).expect("repo-local tmp dir");
    let dir = tempfile::Builder::new()
        .prefix("neomacsclient-tcp-")
        .tempdir_in(repo_tmp)
        .expect("repo-local tempdir");
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
