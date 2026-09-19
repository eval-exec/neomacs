# Windows GUI launcher (#389)

The reporter's v0.0.18 screenshot shows a console behind the editor. Neomacs
has a console-subsystem executable and the installer directly targeted it.
The report was inspected on Linux; native Windows reproduction is pending.

GNU Emacs separates `emacs.exe` from `runemacs.exe`. `nt/Makefile.in` links the
launcher with `-mwindows`; `nt/runemacs.c` passes `STARTF_USESHOWWINDOW` and
`SW_HIDE` to `CreateProcess`. This preserves the editor's console entry point.

Neomacs follows that separation with `src/bin/runneomacs/{main,windows}.rs`.
The launcher locates the adjacent editor, passes an explicit application path,
forwards the original UTF-16 argument tail, inherits cwd/environment, and
closes the returned process/thread handles with `OwnedHandle`. Startup errors
produce a dialog and a failing launcher exit status. Successful launch returns
immediately; callers needing the editor's exit status use `neomacs.exe`.
Unlike GNU's default creation flags, we explicitly request a new hidden console
so a launcher invoked from a terminal cannot affect that terminal's console.

`windows-tools` gates both `cmdproxy` and `runneomacs`. The typed production
capability metadata enables it only on Windows; Linux/macOS fresh builds omit
both targets. Cargo's explicit `--all-features` can still compile unsupported
platform stubs, but ordinary non-Windows builds and packages exclude them.
Windows portable archives include the launcher; installed GUI shortcuts use it.
The main editor's subsystem and CLI behavior are unchanged.

Validation:

- Installer shortcut regression observed failing before its target changed.
- Production-platform helper policy observed failing before validation changed.
- Windows native integration tests inspect both PE subsystems and exercise a
  console child through the actual launcher, including Unicode/spaces/quotes,
  backslashes, an empty argument, working directory, and console visibility.
- Additional native tests close the missing-editor error dialog and check its
  failure status, and verify real-editor batch output and exit status.
- Local verification: 109 xtask tests passed; isolated checks of launcher and
  native-test sources passed for Windows x86-64 and ARM64; Linux
  `cargo xtask fresh-build --release` completed and compiled neither helper.
- Native tests run after fresh-build/bootstrap in Windows release and installer
  CI. They have not been executed on this Linux development host.

API references:
- https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw
- https://learn.microsoft.com/en-us/cpp/c-language/parsing-c-command-line-arguments
