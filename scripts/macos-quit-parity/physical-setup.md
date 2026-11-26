Physical Command-Q uses one persistent app, bundle `org.neomacs.GuiDriver`. The fixed-path app has been built once with an ad-hoc signature and verified as `org.neomacs.GuiDriver`. It has not been launched, and no permissions have been requested. Permission setup awaits the user at the Mac mini's logged-in desktop; the test remains BLOCKED until then.

At that desktop, select the ABC keyboard input source for the physical Q key. Use these fixed paths:

```sh
cd /Users/eval-exec/Projects/github.com/eval-exec/neomacs/tmp/wt-lead-remote.noindex/lanes/dispgh668-optionb
export NEOMACS_GUI_DRIVER_APP=/Users/eval-exec/Projects/github.com/eval-exec/neomacs/tmp/wt-lead-remote.noindex/gui-driver/NeomacsGuiDriver.app
export NEOMACS_GUI_DRIVER_SOCKET=/Users/eval-exec/Projects/github.com/eval-exec/neomacs/tmp/wt-lead-remote.noindex/gui.sock
```

Optional, recommended: in Keychain Access, choose Certificate Assistant → Create a Certificate. Create a self-signed Code Signing certificate in the login keychain, named `Neomacs GUI Driver Testing`. The user performs this step; lanes never create keychain items. If you choose this certificate before the first permission grant, replace the prepared ad-hoc build once with that signing identity:

```sh
NEOMACS_GUI_DRIVER_SIGN_IDENTITY='Neomacs GUI Driver Testing' bash scripts/build-macos-gui-driver.sh
```

If you skip the certificate, use the already prepared app directly. Do not run the build script again.

Keep this app at the fixed path. Do not rebuild it unless `Driver.swift` changes. An ad-hoc rebuild changes its signing identity and can invalidate the grant; the optional certificate keeps the signing identity stable across later builds.

Request setup permissions explicitly:

```sh
open -n "$NEOMACS_GUI_DRIVER_APP" --args "$NEOMACS_GUI_DRIVER_SOCKET" --request-permissions
```

Approve Accessibility for **Neomacs GUI Driver** (`org.neomacs.GuiDriver`), rather than sshd or a per-run executable. Screen Recording may also be requested by the helper; it is used for screenshot tests and is not required by this input-only Command-Q session. The user decides and clicks the permission dialogs. Lanes never edit TCC or click those dialogs.

After approving, cleanly stop the app through its socket, then restart without permission requests:

```sh
python3 scripts/macos-quit-parity/driver_socket.py --socket "$NEOMACS_GUI_DRIVER_SOCKET" --shutdown
open -n "$NEOMACS_GUI_DRIVER_APP" --args "$NEOMACS_GUI_DRIVER_SOCKET"
python3 scripts/macos-quit-parity/driver_socket.py --socket "$NEOMACS_GUI_DRIVER_SOCKET"
```

The shutdown command releases held input, closes the listener, and removes its socket. It is part of this user-run restart procedure; normal tests disconnect their own session and leave the app running. The final preflight command either reports `ready` or reports `BLOCKED` with the exact missing socket or permission reason. Ordinary tests never request permissions.

Leave the app running and report that preflight result. The lane can then run the paired harness with `--physical` through the guarded Mac workflow. Both GNU and Neo use the same driver socket, attach their owned PID with its exact canonical executable, and send Command-down, Q-down, Q-up, Command-up. The per-run Swift helper remains only for the quit AppleEvent; it no longer injects physical keys in this row.
