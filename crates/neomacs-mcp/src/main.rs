//! `neomacs-mcp --socket PATH`: let a stdio MCP client talk to the editor
//! endpoint started by `neomacs-mcp-start` (lisp/neomacs-mcp.el).
#![forbid(unsafe_code)]

#[cfg(unix)]
mod relay;

fn main() {
    #[cfg(unix)]
    relay::main();
    #[cfg(not(unix))]
    {
        eprintln!("neomacs-mcp: Unix sockets are unsupported on this platform");
        std::process::exit(1);
    }
}
