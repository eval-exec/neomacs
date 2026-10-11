use super::*;
use std::path::Path;

#[test]
fn inherited_shell_wins_on_every_platform() {
    for platform in [ShellPlatform::Posix, ShellPlatform::Windows] {
        let resolved = resolve_for(
            platform,
            Some(OsString::from(r"D:\tools\custom-shell.exe")),
            Some(Path::new(r"D:\neomacs\libexec")),
        );
        assert!(matches!(resolved, ResolvedShellFileName::Environment(_)));
        let expected = match platform {
            ShellPlatform::Posix => r"D:\tools\custom-shell.exe",
            ShellPlatform::Windows => "D:/tools/custom-shell.exe",
        };
        assert_eq!(resolved.lisp_name(), expected);
    }
}

#[test]
fn posix_without_shell_uses_bin_sh() {
    let resolved = resolve_for(ShellPlatform::Posix, None, Some(Path::new("/opt/libexec")));

    assert_eq!(resolved, ResolvedShellFileName::PosixSh);
    assert_eq!(resolved.lisp_name(), "/bin/sh");
}

#[test]
fn windows_without_shell_uses_private_cmdproxy() {
    let resolved = resolve_for(
        ShellPlatform::Windows,
        None,
        Some(Path::new(r"C:\Program Files\Neomacs\libexec")),
    );

    assert!(matches!(
        resolved,
        ResolvedShellFileName::WindowsCmdProxy(_)
    ));
    assert_eq!(
        resolved.lisp_name(),
        "C:/Program Files/Neomacs/libexec/cmdproxy.exe"
    );
}

#[test]
fn windows_without_an_executable_location_keeps_a_path_search_fallback() {
    let resolved = resolve_for(ShellPlatform::Windows, None, None);

    assert!(matches!(
        resolved,
        ResolvedShellFileName::WindowsCmdProxy(_)
    ));
    assert_eq!(resolved.lisp_name(), "cmdproxy.exe");
}

#[test]
fn windows_treats_an_empty_shell_as_missing_but_posix_preserves_it() {
    let windows = resolve_for(
        ShellPlatform::Windows,
        Some(OsString::new()),
        Some(Path::new(r"C:\neomacs\libexec")),
    );
    let posix = resolve_for(ShellPlatform::Posix, Some(OsString::new()), None);

    assert_eq!(windows.lisp_name(), "C:/neomacs/libexec/cmdproxy.exe");
    assert!(matches!(posix, ResolvedShellFileName::Environment(_)));
    assert_eq!(posix.lisp_name(), "");
}
