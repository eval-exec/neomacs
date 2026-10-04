;;; telega-fixture-gui.el --- account-free Telega GUI fixture scenario -*- lexical-binding: t; -*-
;;
;; This scenario runs inside `neomacs -Q -l' on a harness-owned private
;; display.  It mounts the pinned Telega frontend provisioned by
;; neomacs-infra through an explicit `load-path' built by the harness (no
;; package discovery), points Telega's REAL `telega-server-command' at the
;; offline mock process (`NEOMACS_TELEGA_MOCK'), and drives the real root
;; buffer: chat list, avatars, paging.
;;
;; Isolation contract: every writable path is below
;; `NEOMACS_TELEGA_FIXTURE_ROOT' (which the harness creates fresh), HOME/XDG
;; point into that tree, and no personal configuration, Telegram account,
;; database, cache, photo, or network connection is used.  This is
;; *configuration* isolation: the fixture never reads a user path through
;; Telega, but it does not create a filesystem/network sandbox, so the
;; assertions below check the paths the fixture set rather than claiming an
;; OS-level boundary.  The harness additionally verifies the process
;; environment from /proc.
;;
;; The scenario writes `checkpoint.json' periodically (window-start/window-end,
;; chat count, auth state, the delivered avatar's file state) so the Rust test
;; can wait on readiness instead of sleeping, and answers `snapshot-request'
;; files with a full frame snapshot.  On any error it writes
;; `scenario-error.json' and exits nonzero.

(defconst tf/root (getenv "NEOMACS_TELEGA_FIXTURE_ROOT")
  "Fixture-owned root directory.")
(defconst tf/mock (getenv "NEOMACS_TELEGA_MOCK")
  "Offline telega-server stand-in the harness built.")
(defconst tf/source (getenv "NEOMACS_TELEGA_SOURCE")
  "Pinned Telega source file prepared by neomacs-infra.")
(defconst tf/load-path (getenv "NEOMACS_TELEGA_LOAD_PATH")
  "Colon-separated provisioned package directories (Telega + dependencies).")
(defconst tf/empty-elpa (getenv "NEOMACS_TELEGA_EMPTY_ELPA")
  "Fixture-owned empty package directory; package.el never sees a user tree.")
(defconst tf/checkpoint-path (expand-file-name "checkpoint.json" tf/root))
(defconst tf/request-path (expand-file-name "snapshot-request" tf/root))
(defconst tf/ready-path (expand-file-name "snapshot-ready" tf/root))
(defconst tf/quit-path (expand-file-name "quit-request" tf/root))
(defconst tf/error-path (expand-file-name "scenario-error.json" tf/root))
(defconst tf/isolation-path (expand-file-name "isolation.json" tf/root))
(defconst tf/snapshot-path (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON"))

(defvar tf/chats-loaded nil)
(defvar tf/generation 0)
(defvar tf/started-at (float-time))

(defun tf/write-atomic (path contents)
  "Write CONTENTS to PATH via rename so readers never see a partial file."
  (let ((tmp (format "%s.tmp.%d" path (emacs-pid))))
    (write-region contents nil tmp nil 'quiet)
    (rename-file tmp path t)))

(defun tf/fail (error)
  "Record ERROR and exit nonzero; the harness treats this as a scenario failure."
  (let ((payload (list :kind "scenario-error"
                       :error (format "%S" error)
                       :backtrace (condition-case nil
                                      (with-output-to-string (backtrace))
                                    (error "")))))
    (ignore-errors (tf/write-atomic tf/error-path (json-encode payload)))
    (kill-emacs 3)))

(defmacro tf/guard (&rest body)
  "Run BODY, turning any error into a recorded scenario failure."
  `(condition-case err (progn ,@body) (error (tf/fail err))))

(defun tf/assert-isolation ()
  "Refuse to run outside the fixture-owned environment."
  (unless (and tf/root (file-directory-p tf/root))
    (error "NEOMACS_TELEGA_FIXTURE_ROOT is missing or not a directory"))
  (unless (and tf/mock (file-executable-p tf/mock))
    (error "NEOMACS_TELEGA_MOCK is missing or not executable"))
  (unless (and tf/source (file-readable-p tf/source))
    (error "NEOMACS_TELEGA_SOURCE is missing or unreadable"))
  (unless (and tf/load-path (not (string-empty-p tf/load-path)))
    (error "NEOMACS_TELEGA_LOAD_PATH is empty"))
  (unless (and tf/empty-elpa (file-directory-p tf/empty-elpa))
    (error "NEOMACS_TELEGA_EMPTY_ELPA is missing or not a directory"))
  (when (getenv "EMACSLOADPATH")
    (error "EMACSLOADPATH must be stripped; the fixture builds load-path explicitly"))
  (let ((home (expand-file-name "~/"))
        (display (getenv "DISPLAY"))
        (wayland (getenv "WAYLAND_DISPLAY"))
        (runtime (getenv "XDG_RUNTIME_DIR")))
    (unless (string-prefix-p (file-name-as-directory tf/root) home)
      (error "HOME %s escapes the fixture root %s" home tf/root))
    (unless (and display (not (string-empty-p display)))
      (error "no private DISPLAY was provided"))
    (when (and wayland (not (string-empty-p wayland)))
      (error "WAYLAND_DISPLAY must not route the X11 fixture at a user session"))
    (when (getenv "WAYLAND_SOCKET")
      (error "WAYLAND_SOCKET must be absent, not merely empty"))
    (when (getenv "DBUS_SESSION_BUS_ADDRESS")
      (error "DBUS_SESSION_BUS_ADDRESS must not point at the user's bus"))
    ;; The harness publishes the owned runtime directory through
    ;; /proc/<pid>/fd/<fd>; the Rust side canonicalizes it against the owned
    ;; directory, so here it is enough that it exists and is a directory.
    (unless (and runtime (file-directory-p runtime))
      (error "XDG_RUNTIME_DIR %s is not a readable directory" runtime)))
  (dolist (variable '("HOME" "XDG_CONFIG_HOME" "XDG_CACHE_HOME" "XDG_DATA_HOME"
                      "XDG_STATE_HOME"))
    (let ((value (getenv variable)))
      (when (and value
                 (not (string-prefix-p (file-name-as-directory tf/root)
                                       (file-name-as-directory value))))
        (error "%s=%s escapes the fixture root" variable value)))))

(defun tf/record-isolation ()
  "Persist the scenario's view of its isolation for the harness to verify."
  (tf/write-atomic
   tf/isolation-path
   (concat
    (json-encode
     (list :root tf/root
           :home (expand-file-name "~/")
           :display (getenv "DISPLAY")
           :runtime-dir (getenv "XDG_RUNTIME_DIR")
           :runtime-truename (condition-case nil
                                 (file-truename (or (getenv "XDG_RUNTIME_DIR") "/"))
                               (error "unresolved"))
           :wayland-display (or (getenv "WAYLAND_DISPLAY") "unset")
           :wayland-socket (or (getenv "WAYLAND_SOCKET") "unset")
           :dbus (or (getenv "DBUS_SESSION_BUS_ADDRESS") "unset")
           :emacsloadpath (or (getenv "EMACSLOADPATH") "unset")
           :source tf/source
           :load-path (split-string tf/load-path ":" t)
           :telega-directory telega-directory
           :telega-database-dir telega-database-dir
           :telega-cache-dir telega-cache-dir
           :server-command telega-server-command
           :server-logfile (format "%S" telega-server-logfile)
           :graphic (and (display-graphic-p) t)))
    "\n")))

(require 'json)

(defun tf/configure-telega-paths ()
  "Point every Telega path and command at the fixture, never at a user tree.
Run BEFORE loading Telega so no defcustom default is ever computed from the
ambient environment."
  (setq telega-directory (expand-file-name "telega-dir" tf/root)
        telega-database-dir (expand-file-name "telega-db" tf/root)
        telega-cache-dir (expand-file-name "telega-cache" tf/root)
        telega-temp-dir (expand-file-name "telega-temp" tf/root)
        telega-server-command tf/mock
        telega-server-logfile nil
        telega-use-docker nil
        telega-use-test-dc nil
        telega-use-images t
        telega-root-show-avatars t
        telega-chat-show-avatars t
        telega-use-file-database nil
        telega-use-chat-info-database nil
        telega-use-message-database nil
        telega-debug nil))

(defun tf/mount-pinned-telega ()
  "Mount exactly the provisioned packages and load the pinned Telega source.
`package.el' is denied any discovery surface: an empty user directory and no
directory list; `load-path' is the harness-provided package list only."
  (require 'package)
  (setq package-user-dir tf/empty-elpa
        package-directory-list nil
        package-check-signature nil
        package-enable-at-startup nil)
  (dolist (directory (split-string tf/load-path ":" t))
    (unless (file-directory-p directory)
      (error "provisioned load-path entry %s is not a directory" directory))
    (add-to-list 'load-path directory))
  (unless (file-readable-p tf/source)
    (error "pinned Telega source %s is unreadable" tf/source))
  (load tf/source nil t t))

(defconst tf/probe-file-id 5000
  "File id of the first synthetic chat's avatar (see the Rust scenario).")

(defun tf/avatar-state ()
  "Frontend state of the first synthetic chat's avatar, for checkpoints.
`:file-table-*' reflects Telega's own file table, which is where a real
`updateFile' lands; `:avatar-file-*' reflects the chat's renewed photo object
and `:avatar-spec-*' the cached avatar image Telega built for it."
  (let* ((chat (and (boundp 'telega--chats)
                    (hash-table-p telega--chats)
                    (gethash 1000 telega--chats)))
         (photo (and chat (plist-get chat :photo)))
         (chat-file (and photo (plist-get photo :small)))
         (chat-local (and chat-file (plist-get chat-file :local)))
         (table-file (and (boundp 'telega--files)
                          (hash-table-p telega--files)
                          (gethash tf/probe-file-id telega--files)))
         (table-local (and table-file (plist-get table-file :local)))
         (image (and chat (plist-get chat :telega-avatar-1)))
         (data (and image (plist-get (cdr image) :data))))
    (list :file-table-path (and table-local (plist-get table-local :path))
          :file-table-downloaded (and table-local
                                      (plist-get table-local :is_downloading_completed)
                                      t)
          :avatar-file-path (and chat-local (plist-get chat-local :path))
          :avatar-file-downloaded (and chat-local
                                       (plist-get chat-local :is_downloading_completed)
                                       t)
          :avatar-spec-references-photo (and data
                                             (string-match-p "avatar-" data)
                                             t)
          :avatar-spec-initials (and data (string-match-p "cgrad" data) t))))

(defun tf/checkpoint-data ()
  "Snapshot editor-visible state the test waits on and asserts against."
  (let* ((root-buffer (get-buffer telega-root-buffer-name))
         (window (and root-buffer (get-buffer-window root-buffer))))
    (append
     (list :generation tf/generation
           :chats-loaded (and tf/chats-loaded t)
           :chats (and (boundp 'telega--chats)
                       (hash-table-p telega--chats)
                       (hash-table-count telega--chats))
           :auth (and (boundp 'telega--auth-state)
                      (format "%S" telega--auth-state))
           :window-start (and window (window-start window))
           :window-end (and window (window-end window))
           :buffer-size (and root-buffer (with-current-buffer root-buffer (buffer-size)))
           :selected-buffer (buffer-name (window-buffer (selected-window)))
           :graphic (and (display-graphic-p) t))
     (tf/avatar-state))))

(defun tf/write-checkpoint ()
  (setq tf/generation (1+ tf/generation))
  (tf/write-atomic tf/checkpoint-path (concat (json-encode (tf/checkpoint-data)) "\n")))

(defun tf/answer-snapshot-request ()
  "Write a full frame snapshot for the token in the request file."
  (when (file-exists-p tf/request-path)
    (let* ((token (with-temp-buffer
                    (insert-file-contents tf/request-path)
                    (string-trim (buffer-string))))
           (snapshot (and (fboundp 'neomacs--write-frame-snapshot)
                          (neomacs--write-frame-snapshot tf/snapshot-path t 'json))))
      (unless snapshot
        (error "neomacs--write-frame-snapshot is unavailable"))
      (tf/write-atomic tf/ready-path token)
      (delete-file tf/request-path))))

(defun tf/tick ()
  "Periodic readiness checkpoint + snapshot service; never throws."
  (condition-case err
      (progn
        (tf/write-checkpoint)
        (tf/answer-snapshot-request)
        (when (file-exists-p tf/quit-path)
          (kill-emacs 0)))
    (error (tf/fail err))))

(tf/guard
 (tf/assert-isolation)
 (tf/configure-telega-paths)
 (tf/mount-pinned-telega)
 (tf/record-isolation)

 (add-hook 'telega-chats-fetched-hook
           (lambda ()
             (setq tf/chats-loaded t)
             ;; Show the real root buffer in the selected window; the
             ;; `-Q -l' startup path can otherwise leave *scratch* selected.
             (run-at-time 0 nil (lambda ()
                                  (switch-to-buffer telega-root-buffer-name)
                                  (tf/write-checkpoint)))))

 (telega)

 (run-with-timer 0 0.15 #'tf/tick)

 ;; Watchdog: never outlive the harness by much.
 (run-with-timer 180 nil
                 (lambda ()
                   (tf/fail (list 'watchdog :elapsed (- (float-time) tf/started-at))))))
