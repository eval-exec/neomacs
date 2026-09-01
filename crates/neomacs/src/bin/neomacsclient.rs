use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::Duration;

#[cfg(unix)]
use std::sync::atomic::{AtomicI32, Ordering};
#[cfg(unix)]
use std::sync::{Arc, Mutex};

use neovm_core::GNU_EMACS_VERSION;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum FrameRequest {
    #[default]
    Current,
    NewGraphical,
    NewTty,
    Reuse,
}

impl FrameRequest {
    fn creates_frame(self) -> bool {
        self != Self::Current
    }

    fn uses_current_frame(self) -> bool {
        matches!(self, Self::Current | Self::Reuse)
    }

    fn requests_window_system(self) -> bool {
        matches!(self, Self::NewGraphical | Self::Reuse)
    }
}

#[derive(Debug)]
struct TtyIdentity {
    device: String,
    terminal_type: String,
}

impl TtyIdentity {
    #[cfg(unix)]
    fn from_stdout() -> Result<Self, String> {
        use std::ffi::CStr;
        use std::os::fd::AsRawFd;

        let terminal_type = env::var("TERM")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "please set the TERM variable to your terminal type".to_string())?;
        let mut device = vec![libc::c_char::default(); 1024];
        let error =
            unsafe { libc::ttyname_r(io::stdout().as_raw_fd(), device.as_mut_ptr(), device.len()) };
        if error != 0 {
            return Err(format!(
                "could not get terminal name: {}",
                io::Error::from_raw_os_error(error)
            ));
        }
        let device = unsafe { CStr::from_ptr(device.as_ptr()) }
            .to_str()
            .map_err(|_| "terminal name is not valid UTF-8".to_string())?
            .to_owned();
        Ok(Self {
            device,
            terminal_type,
        })
    }

    #[cfg(not(unix))]
    fn from_stdout() -> Result<Self, String> {
        Err("creating a TTY frame is not supported on this platform".to_string())
    }
}

#[derive(Debug, Default)]
struct Options {
    nowait: bool,
    quiet: bool,
    suppress_output: bool,
    eval: bool,
    frame: FrameRequest,
    socket_name: Option<String>,
    server_file: Option<String>,
    alternate_editor: Option<String>,
    timeout: Option<Duration>,
    tramp_prefix: Option<String>,
    display: Option<String>,
    parent_id: Option<String>,
    frame_parameters: Option<String>,
    args: Vec<String>,
}

fn main() {
    let code = match run(env::args_os().collect()) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("*ERROR*: {err}");
            1
        }
    };
    process::exit(code);
}

fn run(argv: Vec<OsString>) -> Result<(), String> {
    let prog = argv
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or("neomacsclient")
        .to_string();
    let options = parse_options(&prog, argv.into_iter().skip(1))?;

    if !(options.eval || options.frame.creates_frame() || !options.args.is_empty()) {
        return Err(format!(
            "{prog}: file name or argument required\nTry '{prog} --help' for more information"
        ));
    }

    run_client(&prog, options)
}

fn parse_options(prog: &str, args: impl IntoIterator<Item = OsString>) -> Result<Options, String> {
    let mut options = Options::default();
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            options.args.extend(args[i + 1..].iter().cloned());
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            options.args.push(arg.clone());
            i += 1;
            continue;
        }

        match arg.as_str() {
            "-n" | "--no-wait" => options.nowait = true,
            "-q" | "--quiet" => options.quiet = true,
            "-u" | "--suppress-output" => options.suppress_output = true,
            "-e" | "--eval" => options.eval = true,
            "-V" | "--version" => {
                println!("neomacsclient {GNU_EMACS_VERSION}");
                process::exit(0);
            }
            "-H" | "--help" => {
                print_help(prog);
                process::exit(0);
            }
            "-t" | "-nw" | "--tty" | "--nw" | "--no-window-system" => {
                options.frame = FrameRequest::NewTty;
            }
            "-c" | "--create-frame" => {
                if options.frame == FrameRequest::Current {
                    options.frame = FrameRequest::NewGraphical;
                }
            }
            "-r" | "--reuse-frame" => {
                if options.frame != FrameRequest::NewTty {
                    options.frame = FrameRequest::Reuse;
                }
            }
            _ => {
                if let Some(value) = option_value(arg, "--socket-name", "-s", &args, &mut i)? {
                    options.socket_name = Some(value);
                } else if let Some(value) = option_value(arg, "--server-file", "-f", &args, &mut i)?
                {
                    options.server_file = Some(value);
                } else if let Some(value) =
                    option_value(arg, "--alternate-editor", "-a", &args, &mut i)?
                {
                    options.alternate_editor = Some(value);
                } else if let Some(value) = option_value(arg, "--timeout", "-w", &args, &mut i)? {
                    let seconds = value
                        .parse::<u64>()
                        .map_err(|_| format!("Invalid timeout: \"{value}\""))?;
                    options.timeout = Some(Duration::from_secs(seconds));
                } else if let Some(value) = option_value(arg, "--tramp", "-T", &args, &mut i)? {
                    options.tramp_prefix = Some(value);
                } else if let Some(value) = option_value(arg, "--display", "-d", &args, &mut i)? {
                    options.display = Some(value);
                } else if let Some(value) = option_value(arg, "--parent-id", "", &args, &mut i)? {
                    options.parent_id = Some(value);
                    if options.frame == FrameRequest::Current {
                        options.frame = FrameRequest::NewGraphical;
                    }
                } else if let Some(value) =
                    option_value(arg, "--frame-parameters", "-F", &args, &mut i)?
                {
                    options.frame_parameters = Some(value);
                } else {
                    return Err(format!(
                        "{prog}: unrecognized option '{arg}'\nTry '{prog} --help' for more information"
                    ));
                }
            }
        }
        i += 1;
    }

    Ok(options)
}

fn option_value(
    arg: &str,
    long: &str,
    short: &str,
    args: &[String],
    index: &mut usize,
) -> Result<Option<String>, String> {
    if !long.is_empty() {
        if arg == long {
            *index += 1;
            return args
                .get(*index)
                .cloned()
                .map(Some)
                .ok_or_else(|| format!("{long} requires an argument"));
        }
        if let Some(value) = arg.strip_prefix(&format!("{long}=")) {
            return Ok(Some(value.to_string()));
        }
    }

    if !short.is_empty() && arg == short {
        *index += 1;
        return args
            .get(*index)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{short} requires an argument"));
    }

    Ok(None)
}

fn print_help(prog: &str) {
    println!(
        "\
Usage: {prog} [OPTIONS] FILE...
Tell a Neomacs server to visit files or evaluate forms.

Options:
  -V, --version              Print version info and return
  -H, --help                 Print this help
  -n, --no-wait              Do not wait for the server to return
  -e, --eval                 Treat FILE arguments as Elisp expressions
  -q, --quiet                Do not display success messages
  -u, --suppress-output      Do not display return values
  -s, --socket-name SOCKET   Use a local Unix server socket
-f, --server-file FILE     Use a TCP authentication file
  -a, --alternate-editor CMD Run CMD if the server is not available
  -w, --timeout SECONDS      Wait this many seconds for server replies
  -T, --tramp PREFIX         Prefix absolute file names for Tramp
"
    );
}

fn run_client(prog: &str, options: Options) -> Result<(), String> {
    if let Some(server_file) = options
        .server_file
        .clone()
        .or_else(|| env::var("EMACS_SERVER_FILE").ok())
    {
        return run_tcp_client(prog, options, &server_file);
    }

    #[cfg(unix)]
    {
        run_unix_client(prog, options)
    }

    #[cfg(not(unix))]
    {
        Err(format!(
            "{prog}: local socket mode is unsupported on this platform; use --server-file"
        ))
    }
}

#[cfg(unix)]
fn run_unix_client(prog: &str, options: Options) -> Result<(), String> {
    let socket = resolve_socket_path(&options)?;
    let mut stream = match std::os::unix::net::UnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(err) => {
            return fail_or_alternate(
                prog,
                &options,
                &format!("can't connect to {}: {err}", socket.display()),
            );
        }
    };
    if let Some(timeout) = options.timeout {
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|err| format!("failed to set socket timeout: {err}"))?;
    }

    let request = build_request(&options)?;
    let lifecycle = (options.frame == FrameRequest::NewTty)
        .then(|| {
            stream
                .try_clone()
                .map_err(|err| format!("failed to clone server connection: {err}"))
                .and_then(TtyLifecycle::start)
        })
        .transpose()?;
    if let Some(lifecycle) = &lifecycle {
        lifecycle.write_request(request.as_bytes())?;
    } else {
        stream
            .write_all(request.as_bytes())
            .map_err(|err| format!("failed to send request to server: {err}"))?;
    }
    read_responses(&mut stream, &options, lifecycle.as_ref())
}

fn run_tcp_client(prog: &str, options: Options, server_file: &str) -> Result<(), String> {
    let config = match read_tcp_server_config(server_file) {
        Ok(config) => config,
        Err(err) => return fail_or_alternate(prog, &options, &err),
    };
    let mut stream = match TcpStream::connect((&*config.host, config.port)) {
        Ok(stream) => stream,
        Err(err) => {
            return fail_or_alternate(
                prog,
                &options,
                &format!("can't connect to {}:{}: {err}", config.host, config.port),
            );
        }
    };
    if let Some(timeout) = options.timeout {
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|err| format!("failed to set socket timeout: {err}"))?;
    }

    let mut request = String::new();
    push_arg_command(&mut request, "-auth", &config.auth_key);
    request.push_str(&build_request(&options)?);
    #[cfg(unix)]
    let lifecycle = (options.frame == FrameRequest::NewTty)
        .then(|| {
            stream
                .try_clone()
                .map_err(|err| format!("failed to clone server connection: {err}"))
                .and_then(TtyLifecycle::start)
        })
        .transpose()?;
    #[cfg(not(unix))]
    let lifecycle: Option<TtyLifecycle> = None;
    #[cfg(unix)]
    if let Some(lifecycle) = &lifecycle {
        lifecycle.write_request(request.as_bytes())?;
    } else {
        stream
            .write_all(request.as_bytes())
            .map_err(|err| format!("failed to send request to server: {err}"))?;
    }
    #[cfg(not(unix))]
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("failed to send request to server: {err}"))?;
    read_responses(&mut stream, &options, lifecycle.as_ref())
}

struct TcpServerConfig {
    host: String,
    port: u16,
    auth_key: String,
}

fn read_tcp_server_config(server_file: &str) -> Result<TcpServerConfig, String> {
    let path = resolve_tcp_server_file(server_file)
        .ok_or_else(|| format!("can't find server file: {server_file}"))?;
    let contents = fs::read_to_string(&path)
        .map_err(|err| format!("cannot read server file {}: {err}", path.display()))?;
    let mut lines = contents.lines();
    let endpoint = lines
        .next()
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| format!("invalid server file: {}", path.display()))?;
    let (host, port) = endpoint
        .rsplit_once(':')
        .ok_or_else(|| format!("invalid server address in {}", path.display()))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("invalid server port in {}", path.display()))?;
    let auth_key = lines
        .next()
        .ok_or_else(|| format!("cannot read authentication info from {}", path.display()))?
        .trim_end_matches(['\r', '\n'])
        .to_string();
    if auth_key.is_empty() {
        return Err(format!(
            "empty authentication info in server file {}",
            path.display()
        ));
    }

    Ok(TcpServerConfig {
        host: host.to_string(),
        port,
        auth_key,
    })
}

fn resolve_tcp_server_file(server_file: &str) -> Option<PathBuf> {
    let path = Path::new(server_file);
    if path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }

    if let Some(home) = env::var_os("HOME") {
        let emacs_d = PathBuf::from(&home)
            .join(".emacs.d")
            .join("server")
            .join(server_file);
        if emacs_d.exists() {
            return Some(emacs_d);
        }
    }

    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        let xdg_path = PathBuf::from(xdg)
            .join("emacs")
            .join("server")
            .join(server_file);
        if xdg_path.exists() {
            return Some(xdg_path);
        }
    } else if let Some(home) = env::var_os("HOME") {
        let config_path = PathBuf::from(home)
            .join(".config")
            .join("emacs")
            .join("server")
            .join(server_file);
        if config_path.exists() {
            return Some(config_path);
        }
    }

    None
}

#[cfg(unix)]
fn resolve_socket_path(options: &Options) -> Result<PathBuf, String> {
    if let Some(socket) = options
        .socket_name
        .clone()
        .or_else(|| env::var("EMACS_SOCKET_NAME").ok())
    {
        return Ok(socket_path_from_name(&socket));
    }

    Ok(socket_path_from_name("server"))
}

#[cfg(unix)]
fn socket_path_from_name(name: &str) -> PathBuf {
    let path = Path::new(name);
    if path.components().count() > 1 || path.is_absolute() {
        return path.to_path_buf();
    }

    if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
        return Path::new(&runtime_dir).join("emacs").join(name);
    }

    let tmp = env::var_os("TMPDIR").unwrap_or_else(|| OsString::from("/tmp"));
    PathBuf::from(tmp)
        .join(format!("emacs{}", effective_uid()))
        .join(name)
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn build_request(options: &Options) -> Result<String, String> {
    let mut request = String::new();
    let cwd = env::current_dir().map_err(|err| format!("cannot get current directory: {err}"))?;
    let mut cwd = cwd.to_string_lossy().into_owned();
    if !cwd.ends_with('/') {
        cwd.push('/');
    }
    let display = effective_display(options);
    let tty = (options.frame == FrameRequest::NewTty)
        .then(TtyIdentity::from_stdout)
        .transpose()?;

    if options.frame.creates_frame() {
        for (name, value) in env::vars_os() {
            let mut entry = name.to_string_lossy().into_owned();
            entry.push('=');
            entry.push_str(&value.to_string_lossy());
            push_arg_command(&mut request, "-env", &entry);
        }
    }

    push_command(&mut request, "-dir");
    if let Some(prefix) = &options.tramp_prefix {
        request.push_str(&quote_argument(prefix));
    }
    request.push_str(&quote_argument(&cwd));
    request.push(' ');

    if options.nowait {
        push_flag(&mut request, "-nowait");
    }
    if options.frame.uses_current_frame() {
        push_flag(&mut request, "-current-frame");
    }
    if let Some(display) = &display {
        push_arg_command(&mut request, "-display", display);
    }
    if let Some(parent_id) = &options.parent_id {
        push_arg_command(&mut request, "-parent-id", parent_id);
    }
    if options.frame.creates_frame()
        && let Some(frame_parameters) = &options.frame_parameters
    {
        push_arg_command(&mut request, "-frame-parameters", frame_parameters);
    }
    if let Some(tty) = &tty {
        push_command(&mut request, "-tty");
        request.push_str(&quote_argument(&tty.device));
        request.push(' ');
        request.push_str(&quote_argument(&tty.terminal_type));
        request.push(' ');
    }
    // This flag asks the server to use its window-system display.  GNU sends
    // it even when the client has no explicit DISPLAY/WAYLAND_DISPLAY; a
    // daemon may already own a usable graphical display.
    if options.frame.requests_window_system() {
        push_flag(&mut request, "-window-system");
    }

    if options.eval {
        if options.args.is_empty() {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|err| format!("failed to read stdin: {err}"))?;
            for line in input.lines() {
                push_arg_command(&mut request, "-eval", line);
            }
        } else {
            for arg in &options.args {
                push_arg_command(&mut request, "-eval", arg);
            }
        }
    } else {
        for arg in &options.args {
            if is_position_arg(arg) {
                push_arg_command(&mut request, "-position", arg);
            } else {
                push_command(&mut request, "-file");
                if let Some(prefix) = &options.tramp_prefix
                    && Path::new(arg).is_absolute()
                {
                    request.push_str(&quote_argument(prefix));
                }
                request.push_str(&quote_argument(arg));
                request.push(' ');
            }
        }
    }

    request.push('\n');
    Ok(request)
}

fn effective_display(options: &Options) -> Option<String> {
    if let Some(display) = options
        .display
        .as_ref()
        .filter(|display| !display.is_empty())
    {
        return Some(display.clone());
    }
    if options.frame.requests_window_system() {
        return env::var("WAYLAND_DISPLAY")
            .ok()
            .filter(|display| !display.is_empty())
            .or_else(|| {
                env::var("DISPLAY")
                    .ok()
                    .filter(|display| !display.is_empty())
            });
    }
    None
}

fn push_flag(request: &mut String, flag: &str) {
    request.push_str(flag);
    request.push(' ');
}

fn push_command(request: &mut String, command: &str) {
    request.push_str(command);
    request.push(' ');
}

fn push_arg_command(request: &mut String, command: &str, arg: &str) {
    push_command(request, command);
    request.push_str(&quote_argument(arg));
    request.push(' ');
}

fn is_position_arg(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('+') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == ':')
}

fn quote_argument(arg: &str) -> String {
    let mut quoted = String::with_capacity(arg.len() * 2);
    if arg.starts_with('-') {
        quoted.push('&');
    }
    for ch in arg.chars() {
        match ch {
            ' ' => quoted.push_str("&_"),
            '\n' => quoted.push_str("&n"),
            '&' => quoted.push_str("&&"),
            _ => quoted.push(ch),
        }
    }
    quoted
}

fn unquote_argument(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut chars = arg.chars();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            match chars.next() {
                Some('_') => out.push(' '),
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('&'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn read_responses(
    stream: &mut impl Read,
    options: &Options,
    lifecycle: Option<&TtyLifecycle>,
) -> Result<(), String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut ok = true;

    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read server response: {err}"))?;
        if read == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if let Some(value) = line.strip_prefix("-print ") {
            if !options.suppress_output {
                print!("{}", unquote_argument(value));
            }
        } else if let Some(value) = line.strip_prefix("-print-nonl ") {
            if !options.suppress_output {
                print!("{}", unquote_argument(value));
            }
        } else if let Some(value) = line.strip_prefix("-error ") {
            eprintln!("*ERROR*: {}", unquote_argument(value));
            ok = false;
        } else if let Some(value) = line.strip_prefix("-emacs-pid ") {
            if let (Some(lifecycle), Ok(pid)) = (lifecycle, value.trim().parse::<i32>()) {
                lifecycle.record_emacs_pid(pid);
            }
        } else if line.starts_with("-suspend ")
            && let Some(lifecycle) = lifecycle
        {
            lifecycle.stop_from_server();
        }
    }

    if ok {
        Ok(())
    } else {
        Err("server reported an error".to_string())
    }
}

struct TtyLifecycle {
    #[cfg(unix)]
    emacs_pid: Arc<AtomicI32>,
    #[cfg(unix)]
    server: Arc<Mutex<Box<dyn Write + Send>>>,
    #[cfg(unix)]
    signals: signal_hook::iterator::Handle,
    #[cfg(unix)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TtyLifecycle {
    #[cfg(unix)]
    fn start(server: impl Write + Send + 'static) -> Result<Self, String> {
        use signal_hook::consts::signal::{SIGCONT, SIGTSTP, SIGTTOU, SIGWINCH};

        let mut signals =
            signal_hook::iterator::Signals::new([SIGCONT, SIGTSTP, SIGTTOU, SIGWINCH])
                .map_err(|error| format!("failed to install TTY signal handlers: {error}"))?;
        let handle = signals.handle();
        let emacs_pid = Arc::new(AtomicI32::new(0));
        let signal_emacs_pid = Arc::clone(&emacs_pid);
        let server: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(server)));
        let signal_server = Arc::clone(&server);
        let thread = std::thread::Builder::new()
            .name("neomacsclient-signals".to_string())
            .spawn(move || {
                for signal in signals.forever() {
                    match signal {
                        SIGWINCH => {
                            let pid = signal_emacs_pid.load(Ordering::Acquire);
                            if pid > 0 {
                                unsafe { libc::kill(pid, SIGWINCH) };
                            }
                        }
                        SIGCONT => {
                            if let Ok(mut server) = signal_server.lock() {
                                let _ = server.write_all(b"-resume \n");
                                let _ = server.flush();
                            }
                        }
                        SIGTSTP | SIGTTOU => {
                            if let Ok(mut server) = signal_server.lock() {
                                let _ = server.write_all(b"-suspend \n");
                                let _ = server.flush();
                            }
                            unsafe { libc::raise(libc::SIGSTOP) };
                        }
                        _ => {}
                    }
                }
            })
            .map_err(|error| format!("failed to start TTY signal handler: {error}"))?;
        Ok(Self {
            emacs_pid,
            server,
            signals: handle,
            thread: Some(thread),
        })
    }

    #[cfg(unix)]
    fn write_request(&self, request: &[u8]) -> Result<(), String> {
        let mut server = self
            .server
            .lock()
            .map_err(|_| "server connection writer poisoned".to_string())?;
        server
            .write_all(request)
            .and_then(|()| server.flush())
            .map_err(|error| format!("failed to send request to server: {error}"))
    }

    fn record_emacs_pid(&self, pid: i32) {
        #[cfg(unix)]
        if pid > 0 {
            self.emacs_pid.store(pid, Ordering::Release);
        }
        #[cfg(not(unix))]
        let _ = pid;
    }

    fn stop_from_server(&self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(0, libc::SIGSTOP);
        }
    }
}

impl Drop for TtyLifecycle {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            self.signals.close();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

fn fail_or_alternate(prog: &str, options: &Options, message: &str) -> Result<(), String> {
    let Some(alternate) = &options.alternate_editor else {
        return Err(format!("{prog}: {message}"));
    };
    if alternate.is_empty() {
        return Err(format!(
            "{prog}: automatic daemon startup is not implemented in neomacsclient yet"
        ));
    }

    let status = Command::new("sh")
        .arg("-c")
        .arg(alternate)
        .args(&options.args)
        .status()
        .map_err(|err| format!("{prog}: failed to run alternate editor: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{prog}: alternate editor exited with {status}"))
    }
}
