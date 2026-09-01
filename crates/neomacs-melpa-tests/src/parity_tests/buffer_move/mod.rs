use std::fmt::Write as _;
use std::time::Duration;

use crate::{BUFFER_MOVE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const BUFFER_MOVE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn symbol(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Up => "buf-move-up",
            Self::Down => "buf-move-down",
            Self::Left => "buf-move-left",
            Self::Right => "buf-move-right",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Up => "<up>",
            Self::Down => "<down>",
            Self::Left => "<left>",
            Self::Right => "<right>",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowSlot {
    TopLeft,
    TopRight,
    Bottom,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Main,
    Minibuffer,
}

impl WindowSlot {
    fn symbol(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::Bottom => "bottom",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
            Self::Left => "left",
            Self::Right => "right",
            Self::Main => "main",
            Self::Minibuffer => "minibuffer",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowLayout {
    ReadmeThreePane,
    HorizontalPair,
    FourPaneGrid,
    SingleWindow,
    MainAndMinibuffer,
}

impl WindowLayout {
    fn symbol(self) -> &'static str {
        match self {
            Self::ReadmeThreePane => "readme-three-pane",
            Self::HorizontalPair => "horizontal-pair",
            Self::FourPaneGrid => "four-pane-grid",
            Self::SingleWindow => "single-window",
            Self::MainAndMinibuffer => "main-and-minibuffer",
        }
    }

    fn contains(self, slot: WindowSlot) -> bool {
        match self {
            Self::ReadmeThreePane => matches!(
                slot,
                WindowSlot::TopLeft | WindowSlot::TopRight | WindowSlot::Bottom
            ),
            Self::HorizontalPair => matches!(slot, WindowSlot::Left | WindowSlot::Right),
            Self::FourPaneGrid => matches!(
                slot,
                WindowSlot::TopLeft
                    | WindowSlot::TopRight
                    | WindowSlot::BottomLeft
                    | WindowSlot::BottomRight
            ),
            Self::SingleWindow => slot == WindowSlot::Main,
            Self::MainAndMinibuffer => {
                matches!(slot, WindowSlot::Main | WindowSlot::Minibuffer)
            }
        }
    }

    fn adjacent(self, from: WindowSlot, direction: Direction) -> Option<WindowSlot> {
        use Direction::{Down, Left, Right, Up};
        use WindowSlot::{
            Bottom, BottomLeft, BottomRight, Left as LeftSlot, Main, Minibuffer,
            Right as RightSlot, TopLeft, TopRight,
        };

        match (self, from, direction) {
            (Self::ReadmeThreePane, TopLeft, Right) => Some(TopRight),
            (Self::ReadmeThreePane, TopLeft | TopRight, Down) => Some(Bottom),
            (Self::ReadmeThreePane, TopRight, Left) => Some(TopLeft),
            (Self::HorizontalPair, LeftSlot, Right) => Some(RightSlot),
            (Self::HorizontalPair, RightSlot, Left) => Some(LeftSlot),
            (Self::FourPaneGrid, TopLeft, Right) => Some(TopRight),
            (Self::FourPaneGrid, TopLeft, Down) => Some(BottomLeft),
            (Self::FourPaneGrid, TopRight, Left) => Some(TopLeft),
            (Self::FourPaneGrid, TopRight, Down) => Some(BottomRight),
            (Self::FourPaneGrid, BottomLeft, Up) => Some(TopLeft),
            (Self::FourPaneGrid, BottomLeft, Right) => Some(BottomRight),
            (Self::FourPaneGrid, BottomRight, Up) => Some(TopRight),
            (Self::FourPaneGrid, BottomRight, Left) => Some(BottomLeft),
            (Self::MainAndMinibuffer, Main, Down) => Some(Minibuffer),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MoveBehavior {
    Swap,
    Move,
}

impl MoveBehavior {
    fn symbol(self) -> &'static str {
        match self {
            Self::Swap => "swap",
            Self::Move => "move",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionRequest {
    FollowDestination,
    RequestDocumentedStay,
}

impl SelectionRequest {
    fn symbol(self) -> &'static str {
        match self {
            Self::FollowDestination => "follow-destination",
            Self::RequestDocumentedStay => "request-documented-stay",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlockReason {
    NoNeighbor,
    TargetDedicated,
    SourceDedicated,
    Minibuffer,
}

impl BlockReason {
    fn symbol(self) -> &'static str {
        match self {
            Self::NoNeighbor => "no-neighbor",
            Self::TargetDedicated => "target-dedicated",
            Self::SourceDedicated => "source-dedicated",
            Self::Minibuffer => "minibuffer",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExistingRoute {
    layout: WindowLayout,
    from: WindowSlot,
    direction: Direction,
    to: WindowSlot,
    behavior: MoveBehavior,
    selection: SelectionRequest,
}

impl ExistingRoute {
    pub(crate) fn new(
        layout: WindowLayout,
        from: WindowSlot,
        direction: Direction,
        to: WindowSlot,
        behavior: MoveBehavior,
        selection: SelectionRequest,
    ) -> Result<Self, String> {
        if !layout.contains(from) || !layout.contains(to) {
            return Err(format!(
                "route endpoint is absent from {layout:?}: {from:?} -> {to:?}"
            ));
        }
        if layout.adjacent(from, direction) != Some(to) {
            return Err(format!(
                "route is not adjacent in {layout:?}: {from:?} {direction:?} {to:?}"
            ));
        }
        if selection == SelectionRequest::RequestDocumentedStay && behavior != MoveBehavior::Swap {
            return Err("documented stay is only defined for swap behavior".into());
        }
        Ok(Self {
            layout,
            from,
            direction,
            to,
            behavior,
            selection,
        })
    }

    pub(crate) fn elisp(self) -> String {
        format!(
            "'(:layout {} :from {} :direction {} :to {} :command {} :behavior {} :selection {})",
            self.layout.symbol(),
            self.from.symbol(),
            self.direction.symbol(),
            self.to.symbol(),
            self.direction.command(),
            self.behavior.symbol(),
            self.selection.symbol(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BlockedRoute {
    layout: WindowLayout,
    from: WindowSlot,
    direction: Direction,
    target: Option<WindowSlot>,
    reason: BlockReason,
    behavior: MoveBehavior,
}

impl BlockedRoute {
    pub(crate) fn new(
        layout: WindowLayout,
        from: WindowSlot,
        direction: Direction,
        target: Option<WindowSlot>,
        reason: BlockReason,
    ) -> Result<Self, String> {
        if !layout.contains(from) {
            return Err(format!(
                "blocked source is absent from {layout:?}: {from:?}"
            ));
        }
        if target.is_some_and(|slot| !layout.contains(slot)) {
            return Err(format!(
                "blocked target is absent from {layout:?}: {target:?}"
            ));
        }
        let actual = layout.adjacent(from, direction);
        match reason {
            BlockReason::NoNeighbor if target.is_none() && actual.is_none() => {}
            BlockReason::Minibuffer
                if target == Some(WindowSlot::Minibuffer) && actual == target => {}
            BlockReason::TargetDedicated | BlockReason::SourceDedicated
                if target.is_some()
                    && actual == target
                    && target != Some(WindowSlot::Minibuffer) => {}
            _ => {
                return Err(format!(
                    "block reason does not match route: {layout:?} {from:?} {direction:?} target={target:?} reason={reason:?}"
                ));
            }
        }
        Ok(Self {
            layout,
            from,
            direction,
            target,
            reason,
            behavior: MoveBehavior::Swap,
        })
    }

    pub(crate) fn elisp(self) -> String {
        let target = self.target.map_or("nil", WindowSlot::symbol);
        format!(
            "'(:layout {} :from {} :direction {} :target {} :command {} :reason {} :behavior {})",
            self.layout.symbol(),
            self.from.symbol(),
            self.direction.symbol(),
            target,
            self.direction.command(),
            self.reason.symbol(),
            self.behavior.symbol(),
        )
    }
}

const TRANSIENT_ARROW_CYCLE: [Direction; 4] = [
    Direction::Right,
    Direction::Down,
    Direction::Left,
    Direction::Up,
];

pub(crate) fn transient_arrow_cycle() -> [Direction; 4] {
    TRANSIENT_ARROW_CYCLE
}

pub(crate) fn contiguous_transient_keys() -> String {
    let mut keys = String::from("M-x buf-move RET");
    for direction in TRANSIENT_ARROW_CYCLE {
        write!(&mut keys, " {}", direction.key()).expect("append static buffer-move key");
    }
    keys.push_str(" a");
    keys
}

const BUFFER_MOVE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'windmove)
(require 'buffer-move)

;; Prime GNU's reserved menu-row realization once so per-case window edges and
;; configuration restoration have the same shared/fresh baseline.
(set-window-configuration (current-window-configuration))

;; Unicode fixture output can reuse this GNU infrastructure buffer.  Own its
;; exact mutable state in every case rather than letting first use skew order.
(get-buffer-create " *code-conversion-work*")

(defconst buffer366-test-source-sha256
  "f53f8ede64251f2984cfc43e25a5f26927ce53a46be3982602093835ca2477f1")
(defconst buffer366-test-state-symbols
  '(buffer-move-behavior buffer-move-stay-after-swap
    windmove-wrap-around windmove-allow-all-windows
    switch-to-prev-buffer-skip switch-to-visible-buffer
    overriding-terminal-local-map pre-command-hook
    set-transient-map-timer set-transient-map-timeout
    unread-command-events executing-kbd-macro
    this-command real-this-command this-original-command
    last-command real-last-command last-repeatable-command
    last-command-event last-input-event last-nonmenu-event last-event-frame
    current-prefix-arg prefix-arg deactivate-mark
    extended-command-history command-history minibuffer-history
    suggest-key-bindings execute-extended-command--binding-timer
    undo-auto-current-boundary-timer undo-auto--undoably-changed-buffers))
(defconst buffer366-test-terminal-state-symbols
  '(undo-auto-current-boundary-timer undo-auto--undoably-changed-buffers))
(defconst buffer366-test-forbidden-external-functions
  '(call-process call-process-region process-file make-process start-process
    make-network-process open-network-stream
    url-retrieve url-retrieve-synchronously))

(defvar buffer366-test-world nil)
(defvar buffer366-test-owned-buffers nil)
(defvar buffer366-test-slots nil)
(defvar buffer366-test-buffer-roles nil)
(defvar buffer366-test-layout nil)
(defvar buffer366-test-external-events nil)
(defvar buffer366-test-external-advices nil)
(defvar buffer366-test-command-events nil)
(defvar buffer366-test-command-observer-installed nil)
(defvar buffer366-test-transient-advice-installed nil)
(defvar buffer366-test-capture-transient nil)
(defvar buffer366-test-transient-map nil)
(defvar buffer366-test-transient-exit nil)
(defvar buffer366-test-transient-hook nil)
(defvar buffer366-test-transient-calls nil)
(defvar buffer366-test-baseline-configuration nil)
(defvar buffer366-test-baseline-windows nil)
(defvar buffer366-test-baseline-window nil)
(defvar buffer366-test-baseline-buffer nil)

(defun buffer366-test-variable-state (symbol)
  (if (boundp symbol)
      (list :bound t :value (symbol-value symbol))
    '(:bound nil)))

(defun buffer366-test-restore-variable (symbol state)
  (if (plist-get state :bound)
      (set symbol (plist-get state :value))
    (makunbound symbol)))

(defun buffer366-test-variable-restored-p (symbol state)
  (if (plist-get state :bound)
      (and (boundp symbol) (eq (symbol-value symbol) (plist-get state :value)))
    (not (boundp symbol))))

(defun buffer366-test-copy-value (value)
  (cond ((stringp value) (copy-sequence value))
        ((consp value)
         (cons (buffer366-test-copy-value (car value))
               (buffer366-test-copy-value (cdr value))))
        ((vectorp value) (apply #'vector (mapcar #'buffer366-test-copy-value value)))
        (t value)))

(defun buffer366-test-condition-state (condition)
  ;; Copy strings independently so `print-circle' cannot turn the real GNU
  ;; condition's equal data/message strings into a harness-only backreference.
  (list :symbol (car condition)
        :data (buffer366-test-copy-value (cdr condition))
        :message (copy-sequence (error-message-string condition))))

(defun buffer366-test-attempt (phase thunk errors)
  (condition-case condition
      (progn (funcall thunk) errors)
    (t (cons (list phase (buffer366-test-condition-state condition)) errors))))

(defun buffer366-test-file-sha256 (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun buffer366-test-provenance ()
  (let* ((library (locate-library "buffer-move"))
         (root (and library (file-name-directory library)))
         (source (and root (expand-file-name "buffer-move.el" root)))
         (helper 'buf-move-to)
         (commands '(buf-move-up buf-move-down
                     buf-move-left buf-move-right buf-move)))
    (unless (and root source (file-exists-p source)
                 (equal (buffer366-test-file-sha256 source)
                        buffer366-test-source-sha256))
      (error "Buffer Move installed source drifted: %S" source))
    (let ((file (symbol-file helper 'defun)))
      (unless (and file
                   (file-in-directory-p (file-truename file)
                                        (file-truename root))
                   (not (commandp helper)))
        (error "Buffer Move private helper provenance drifted: %S %S" helper file)))
    (dolist (symbol commands)
      (let ((file (symbol-file symbol 'defun)))
        (unless (and file
                     (file-in-directory-p (file-truename file)
                                          (file-truename root))
                     (commandp symbol))
          (error "Buffer Move public command provenance drifted: %S %S"
                 symbol file))))
    (unless (and (equal buffer-move-version "0.6.3")
                 (eq (default-value 'buffer-move-behavior) 'swap)
                 (null (default-value 'buffer-move-stay-after-swap)))
      (error "Buffer Move source defaults drifted: %S"
             (list buffer-move-version
                   (default-value 'buffer-move-behavior)
                   (default-value 'buffer-move-stay-after-swap))))
    (list :melpa-version "20220512.755"
          :source-version buffer-move-version
          :commit "e7800b3ab1bd76ee475ef35507ec51ecd5a3f065"
          :source-sha256 buffer366-test-source-sha256
          :commands (copy-sequence commands)
          :defaults (list (default-value 'buffer-move-behavior)
                          (default-value 'buffer-move-stay-after-swap))
          :dependency-closure nil)))

(defun buffer366-test-window-history-snapshot (window)
  (mapcar
   (lambda (entry)
     (list :buffer (car entry)
           :start (marker-position (nth 1 entry))
           :start-insertion (marker-insertion-type (nth 1 entry))
           :point (marker-position (nth 2 entry))
           :point-insertion (marker-insertion-type (nth 2 entry))))
   (window-prev-buffers window)))

(defun buffer366-test-window-history-restore (state)
  (mapcar
   (lambda (entry)
     (let ((buffer (plist-get entry :buffer)))
       (unless (buffer-live-p buffer)
         (error "Buffer Move baseline history buffer died: %S" buffer))
       (list buffer
             (with-current-buffer buffer
               (copy-marker (plist-get entry :start)
                            (plist-get entry :start-insertion)))
             (with-current-buffer buffer
               (copy-marker (plist-get entry :point)
                            (plist-get entry :point-insertion))))))
   state))

(defun buffer366-test-window-structure ()
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window)
           :pixel-edges (window-pixel-edges window)
           :point (window-point window) :start (window-start window)
           :hscroll (window-hscroll window)
           :vscroll (window-vscroll window t)
           :prev (buffer366-test-window-history-snapshot window)
           :next (copy-tree (window-next-buffers window))
           :dedicated (window-dedicated-p window)
           :parameters
           (copy-tree (seq-filter #'cdr (window-parameters window)))
           :margins (window-margins window)
           :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)))
   (window-list nil t)))

(defun buffer366-test-readable-window-structure (state)
  (mapcar
   (lambda (entry)
     (list :buffer (buffer-name (plist-get entry :buffer))
           :edges (plist-get entry :edges)
           :pixel-edges (plist-get entry :pixel-edges)
           :point (plist-get entry :point) :start (plist-get entry :start)
           :hscroll (plist-get entry :hscroll)
           :vscroll (plist-get entry :vscroll)
           :prev
           (mapcar
            (lambda (history)
              (list :buffer (buffer-name (plist-get history :buffer))
                    :start (plist-get history :start)
                    :start-insertion (plist-get history :start-insertion)
                    :point (plist-get history :point)
                    :point-insertion (plist-get history :point-insertion)))
            (plist-get entry :prev))
           :next (mapcar #'buffer-name (plist-get entry :next))
           :dedicated (plist-get entry :dedicated)
           :parameters
           (copy-tree (seq-filter #'cdr (plist-get entry :parameters)))
           :margins (plist-get entry :margins)
           :fringes (plist-get entry :fringes)
           :scroll-bars (plist-get entry :scroll-bars)))
   state))

(defun buffer366-test-owned-timer-p (timer)
  ;; `M-x' may schedule these nonrepeating GNU command-loop maintenance
  ;; timers.  They are not Buffer Move processes, but they are new identities
  ;; owned by this exact real command loop and must be canceled, never pumped.
  (and (null (timer--repeat-delay timer))
       (memq (timer--function timer)
             '(undo-auto--boundary-timer
               completions--background-update eldoc--update))))

(defun buffer366-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Buffer Move baseline window died: %S" window))
      (unless (eq (window-buffer window) (plist-get entry :buffer))
        (set-window-dedicated-p window nil)
        (set-window-buffer window (plist-get entry :buffer)))
      (set-window-dedicated-p window (plist-get entry :dedicated))
      (dolist (parameter (window-parameters window))
        (set-window-parameter window (car parameter) nil))
      (dolist (parameter (plist-get entry :parameters))
        (set-window-parameter window (car parameter) (cdr parameter)))
      (set-window-prev-buffers
       window (buffer366-test-window-history-restore (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (apply #'set-window-margins window (plist-get entry :margins))
      (let ((fringes (plist-get entry :fringes)))
        (set-window-fringes window (nth 0 fringes) (nth 1 fringes)
                            (nth 2 fringes) (nth 3 fringes)))
      (let ((bars (plist-get entry :scroll-bars)))
        (set-window-scroll-bars window (nth 0 bars) (nth 2 bars)
                                (nth 3 bars) (nth 5 bars) (nth 6 bars)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun buffer366-test-buffer-content-state (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (let ((minimum (point-min)) (maximum (point-max)))
        (list :buffer buffer
              :text (save-restriction (widen) (buffer-string))
              :point (point) :mark (mark t) :active mark-active
              :modified (buffer-modified-p)
              :undo buffer-undo-list :read-only buffer-read-only
              :min minimum :max maximum)))))

(defun buffer366-test-restore-buffer-content (state)
  (when state
    (let ((buffer (plist-get state :buffer)))
      (unless (buffer-live-p buffer)
        (error "Buffer Move baseline buffer died: %S" buffer))
      (with-current-buffer buffer
        (let ((inhibit-read-only t) (undo (plist-get state :undo)))
          (widen) (erase-buffer) (insert (plist-get state :text))
          (setq buffer-undo-list undo))
        (goto-char (min (plist-get state :point) (point-max)))
        (if (plist-get state :mark)
            (set-mark (min (plist-get state :mark) (point-max)))
          (set-marker (mark-marker) nil))
        (setq mark-active (plist-get state :active)
              buffer-read-only (plist-get state :read-only))
        (set-buffer-modified-p (plist-get state :modified))
        (narrow-to-region (plist-get state :min) (plist-get state :max))))))

(defun buffer366-test-buffer-content-restored-p (state)
  (or (null state)
      (let ((buffer (plist-get state :buffer)))
        (and (buffer-live-p buffer)
             (with-current-buffer buffer
               (and (equal (save-restriction (widen) (buffer-string))
                           (plist-get state :text))
                    (= (point) (plist-get state :point))
                    (equal (mark t) (plist-get state :mark))
                    (eq mark-active (plist-get state :active))
                    (eq (buffer-modified-p) (plist-get state :modified))
                    (eq buffer-undo-list (plist-get state :undo))
                    (eq buffer-read-only (plist-get state :read-only))
                    (= (point-min) (plist-get state :min))
                    (= (point-max) (plist-get state :max))))))))

(defun buffer366-test-forbidden-external (original &rest arguments)
  (let ((event (list :operation original :arguments arguments)))
    (push event buffer366-test-external-events)
    (error "Buffer Move attempted forbidden external boundary: %S" event)))

(defun buffer366-test-install-external-guards ()
  (dolist (function buffer366-test-forbidden-external-functions)
    (advice-add function :around #'buffer366-test-forbidden-external)
    (push function buffer366-test-external-advices)))

(defun buffer366-test-tree-contains-eq (needle tree)
  (cond ((eq needle tree) t)
        ((consp tree)
         (or (buffer366-test-tree-contains-eq needle (car tree))
             (buffer366-test-tree-contains-eq needle (cdr tree))))
        (t nil)))

(defun buffer366-test-around-set-transient-map (original map &rest arguments)
  (if (not buffer366-test-capture-transient)
      (apply original map arguments)
    (unless (equal arguments '(t))
      (error "Buffer Move transient arguments drifted: %S" arguments))
    (when buffer366-test-transient-map
      (error "Buffer Move installed transient map more than once"))
    (let ((before (copy-sequence pre-command-hook))
          (exit (apply original map arguments)))
      (setq buffer366-test-transient-map map
            buffer366-test-transient-exit exit
            buffer366-test-transient-calls (1+ buffer366-test-transient-calls))
      (let ((added (seq-difference pre-command-hook before #'eq)))
        (unless (= (length added) 1)
          (error "Buffer Move transient hook identity drifted: %S" added))
        (setq buffer366-test-transient-hook (car added)))
      exit)))

(defun buffer366-test-enable-transient-observation ()
  (when buffer366-test-transient-advice-installed
    (error "Buffer Move transient observer already installed"))
  (advice-add 'set-transient-map :around #'buffer366-test-around-set-transient-map)
  (setq buffer366-test-transient-advice-installed t
        buffer366-test-capture-transient t
        buffer366-test-transient-calls 0))

(defun buffer366-test-disable-transient-observation ()
  (setq buffer366-test-capture-transient nil)
  (when buffer366-test-transient-advice-installed
    (advice-remove 'set-transient-map #'buffer366-test-around-set-transient-map)
    (setq buffer366-test-transient-advice-installed nil)))

(defun buffer366-test-slot-window (slot)
  (or (cdr (assq slot buffer366-test-slots))
      (error "Buffer Move slot is not present: %S in %S" slot buffer366-test-layout)))

(defun buffer366-test-window-slot (window)
  (or (car (rassq window buffer366-test-slots))
      (error "Buffer Move window is outside owned layout: %S" window)))

(defun buffer366-test-buffer-role (buffer)
  (or (cdr (assq buffer buffer366-test-buffer-roles))
      (and (minibufferp buffer) 'minibuffer-buffer)
      (error "Buffer Move buffer is outside owned world: %S" buffer)))

(defun buffer366-test-line-state (buffer position)
  (with-current-buffer buffer
    (save-restriction
      (widen)
      (save-excursion
        (goto-char position)
        (let ((line (line-number-at-pos)) (column (current-column)))
          (list :position position :line line :column column
                :text (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))))))))

(defun buffer366-test-history-state (window)
  (if (window-minibuffer-p window)
      (list :prev :not-applicable :next :not-applicable)
    (list
     :prev
     (mapcar
      (lambda (entry)
        (list :buffer (buffer366-test-buffer-role (car entry))
              :start (marker-position (nth 1 entry))
              :start-insertion (marker-insertion-type (nth 1 entry))
              :point (marker-position (nth 2 entry))
              :point-insertion (marker-insertion-type (nth 2 entry))))
      (window-prev-buffers window))
     :next (mapcar #'buffer366-test-buffer-role (window-next-buffers window)))))

(defun buffer366-test-view-state (slot &optional history)
  (let* ((window (buffer366-test-slot-window slot))
         (buffer (window-buffer window)))
    (append
     (list :slot slot :edges (window-edges window)
           :body (list (window-body-width window) (window-body-height window))
           :buffer (buffer366-test-buffer-role buffer)
           :selected (eq window (selected-window))
           :shows-current-buffer (eq buffer (current-buffer))
           :start (buffer366-test-line-state buffer (window-start window))
           :point (buffer366-test-line-state buffer (window-point window))
           :hscroll (window-hscroll window)
           :dedicated (window-dedicated-p window)
           :minibuffer (window-minibuffer-p window))
     (when history (buffer366-test-history-state window)))))

(defun buffer366-test-layout-state (&optional history)
  (list :layout buffer366-test-layout
        :selected (buffer366-test-window-slot (selected-window))
        :current (buffer366-test-buffer-role (current-buffer))
        :windows
        (mapcar (lambda (entry)
                  (buffer366-test-view-state (car entry) history))
                buffer366-test-slots)))

(defun buffer366-test-owned-buffer-bytes ()
  (mapcar
   (lambda (entry)
     (let ((buffer (car entry)))
       (list :buffer (cdr entry)
             :text (with-current-buffer buffer
                     (buffer-substring-no-properties (point-min) (point-max))))))
   (reverse buffer366-test-buffer-roles)))

(defun buffer366-test-new-buffer (name role tag)
  (when (get-buffer name)
    (error "Buffer Move refuses ambient buffer collision: %S" name))
  (let ((buffer (get-buffer-create name)))
    (push buffer buffer366-test-owned-buffers)
    (push (cons buffer role) buffer366-test-buffer-roles)
    (with-current-buffer buffer
      (dotimes (line 24)
        (insert (format "%s-%02d | abcdefghijklmnopqrstuvwxyz 0123456789 界\n"
                        tag (1+ line))))
      (goto-char (point-min))
      (set-buffer-modified-p nil)
      (setq buffer-undo-list nil))
    buffer))

(defun buffer366-test-position (buffer line column)
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (forward-line (1- line))
      (move-to-column column)
      (point))))

(defun buffer366-test-set-view (window buffer start-line point-line point-column hscroll)
  (set-window-dedicated-p window nil)
  (set-window-buffer window buffer)
  (set-window-start window (buffer366-test-position buffer start-line 0) 'noforce)
  (set-window-hscroll window hscroll)
  (set-window-point window
                    (buffer366-test-position buffer point-line point-column)))

(defun buffer366-test-register-layout (layout entries)
  (setq buffer366-test-layout layout
        buffer366-test-slots entries)
  entries)

(defun buffer366-test-reset-subworld ()
  (dolist (window (window-list nil 'no-minibuf))
    (set-window-dedicated-p window nil))
  (buffer366-test-restore-windows buffer366-test-baseline-configuration
                                  buffer366-test-baseline-windows)
  (select-window buffer366-test-baseline-window)
  (set-buffer buffer366-test-baseline-buffer)
  (let (errors)
    (dolist (buffer buffer366-test-owned-buffers)
      (setq errors
            (buffer366-test-attempt
             (list 'reset-buffer (buffer-name buffer))
             (lambda ()
               (when (buffer-live-p buffer)
                 (with-current-buffer buffer
                   (setq kill-buffer-query-functions nil)
                   (set-buffer-modified-p nil))
                 (kill-buffer buffer)))
             errors)))
    (when errors (error "Buffer Move subworld reset failed: %S" (nreverse errors))))
  (setq buffer366-test-slots nil
        buffer366-test-buffer-roles nil
        buffer366-test-layout nil
        buffer-move-behavior 'swap
        buffer-move-stay-after-swap nil
        windmove-wrap-around nil
        windmove-allow-all-windows nil))

(defun buffer366-test-readme-layout ()
  (buffer366-test-reset-subworld)
  (delete-other-windows)
  (let* ((top-left (selected-window))
         (bottom (split-window top-left nil 'below))
         (top-right (split-window top-left nil 'right))
         (a (buffer366-test-new-buffer "bm366-readme-A" 'A "A"))
         (b (buffer366-test-new-buffer "bm366-readme-B" 'B "B"))
         (c (buffer366-test-new-buffer "bm366-readme-C" 'C "C")))
    (buffer366-test-register-layout
     'readme-three-pane
     (list (cons 'top-left top-left) (cons 'top-right top-right)
           (cons 'bottom bottom)))
    (buffer366-test-set-view top-left a 2 4 9 2)
    (buffer366-test-set-view top-right b 5 7 12 5)
    (buffer366-test-set-view bottom c 8 10 15 8)
    (select-window top-right)
    buffer366-test-slots))

(defun buffer366-test-horizontal-layout (prefix &optional same-buffer)
  (buffer366-test-reset-subworld)
  (delete-other-windows)
  (let* ((left (selected-window))
         (right (split-window left nil 'right))
         (a (buffer366-test-new-buffer (format "bm366-%s-A" prefix) 'A "A"))
         (b (if same-buffer a
              (buffer366-test-new-buffer (format "bm366-%s-B" prefix) 'B "B"))))
    (buffer366-test-register-layout
     'horizontal-pair (list (cons 'left left) (cons 'right right)))
    (buffer366-test-set-view left a 2 4 9 2)
    (buffer366-test-set-view right b (if same-buffer 11 8)
                              (if same-buffer 13 10)
                              (if same-buffer 16 13)
                              (if same-buffer 9 6))
    (select-window left)
    buffer366-test-slots))

(defun buffer366-test-grid-layout ()
  (buffer366-test-reset-subworld)
  (delete-other-windows)
  (let* ((top-left (selected-window))
         (top-right (split-window top-left nil 'right))
         (bottom-left (split-window top-left nil 'below))
         (bottom-right (split-window top-right nil 'below))
         (a (buffer366-test-new-buffer "bm366-grid-A" 'A "A"))
         (b (buffer366-test-new-buffer "bm366-grid-B" 'B "B"))
         (c (buffer366-test-new-buffer "bm366-grid-C" 'C "C"))
         (d (buffer366-test-new-buffer "bm366-grid-D" 'D "D")))
    (buffer366-test-register-layout
     'four-pane-grid
     (list (cons 'top-left top-left) (cons 'top-right top-right)
           (cons 'bottom-left bottom-left) (cons 'bottom-right bottom-right)))
    (buffer366-test-set-view top-left a 2 4 9 2)
    (buffer366-test-set-view top-right b 5 7 12 5)
    (buffer366-test-set-view bottom-left c 8 10 15 8)
    (buffer366-test-set-view bottom-right d 11 13 18 11)
    (select-window top-left)
    buffer366-test-slots))

(defun buffer366-test-single-layout (prefix &optional include-minibuffer)
  (buffer366-test-reset-subworld)
  (delete-other-windows)
  (let* ((main (selected-window))
         (a (buffer366-test-new-buffer (format "bm366-%s-A" prefix) 'A "A"))
         (entries (list (cons 'main main))))
    (when include-minibuffer
      (setq entries (append entries (list (cons 'minibuffer (minibuffer-window))))))
    (buffer366-test-register-layout
     (if include-minibuffer 'main-and-minibuffer 'single-window) entries)
    (buffer366-test-set-view main a 2 4 9 2)
    (select-window main)
    entries))

(defun buffer366-test-route-state (route)
  (list :layout (plist-get route :layout)
        :from (plist-get route :from)
        :direction (plist-get route :direction)
        :to (or (plist-get route :to) (plist-get route :target))
        :command (plist-get route :command)
        :behavior (plist-get route :behavior)
        :selection-request (plist-get route :selection)
        :block-reason (plist-get route :reason)))

(defun buffer366-test-invoke-existing (route)
  (unless (and (eq buffer366-test-layout (plist-get route :layout))
               (eq (selected-window)
                   (buffer366-test-slot-window (plist-get route :from))))
    (error "Buffer Move route/world mismatch: %S %S"
           route (buffer366-test-layout-state)))
  (setq buffer-move-behavior (plist-get route :behavior)
        buffer-move-stay-after-swap
        (eq (plist-get route :selection) 'request-documented-stay))
  (call-interactively (plist-get route :command))
  (buffer366-test-layout-state))

(defun buffer366-test-invoke-blocked (route)
  (unless (and (eq buffer366-test-layout (plist-get route :layout))
               (eq (selected-window)
                   (buffer366-test-slot-window (plist-get route :from))))
    (error "Buffer Move blocked route/world mismatch: %S %S"
           route (buffer366-test-layout-state)))
  (setq buffer-move-behavior (plist-get route :behavior)
        buffer-move-stay-after-swap nil)
  (condition-case condition
      (list :returned (call-interactively (plist-get route :command)))
    (t (buffer366-test-condition-state condition))))

(defun buffer366-test-command-observer ()
  (when (and (memq this-command
                   '(buf-move buf-move-up buf-move-down buf-move-left
                     buf-move-right self-insert-command))
             (or (not (eq this-command 'self-insert-command))
                 (assq (current-buffer) buffer366-test-buffer-roles)))
    (push
     (append
      (list :command this-command
            :selected (and buffer366-test-slots
                           (buffer366-test-window-slot (selected-window)))
            :buffer (and buffer366-test-buffer-roles
                         (buffer366-test-buffer-role (current-buffer)))
            :map-active
            (and buffer366-test-transient-map
                 (buffer366-test-tree-contains-eq
                  buffer366-test-transient-map overriding-terminal-local-map))
            :right-binding (key-binding (kbd "<right>")))
      (when (memq this-command
                  '(buf-move-up buf-move-down buf-move-left buf-move-right))
        (list :layout (buffer366-test-layout-state))))
     buffer366-test-command-events)))

(defun buffer366-test-install-command-observer ()
  (add-hook 'post-command-hook #'buffer366-test-command-observer)
  (setq buffer366-test-command-observer-installed t))

(defun buffer366-test-remove-command-observer ()
  (when buffer366-test-command-observer-installed
    (remove-hook 'post-command-hook #'buffer366-test-command-observer)
    (setq buffer366-test-command-observer-installed nil)))

(defun buffer366-test-configure-world ()
  (setq buffer-move-behavior 'swap
        buffer-move-stay-after-swap nil
        windmove-wrap-around nil
        windmove-allow-all-windows nil
        switch-to-prev-buffer-skip nil
        switch-to-visible-buffer t
        overriding-terminal-local-map nil
        pre-command-hook (copy-sequence pre-command-hook)
        set-transient-map-timer nil
        set-transient-map-timeout nil
        unread-command-events nil
        executing-kbd-macro nil
        extended-command-history nil
        command-history nil
        minibuffer-history nil
        suggest-key-bindings nil
        execute-extended-command--binding-timer nil
        undo-auto-current-boundary-timer nil
        undo-auto--undoably-changed-buffers nil))

(defun buffer366-test-new-timers (timers-before idle-before)
  (delete-dups
   (append (seq-difference timer-list timers-before #'eq)
           (seq-difference timer-idle-list idle-before #'eq))))

(defun buffer366-test-cleanup-nonwindow-clean-p (state)
  (and (null (plist-get state :new-buffers))
       (null (plist-get state :new-processes))
       (null (plist-get state :new-timers))
       (plist-get state :variables)
       (plist-get state :messages)
       (plist-get state :warnings)
       (plist-get state :minibuffer)
       (plist-get state :code-conversion)
       (plist-get state :frame-predicate)
       (plist-get state :buffer)
       (plist-get state :window)
       (null (plist-get state :external-events))
       (null (plist-get state :external-advices))
       (null (plist-get state :command-observer))
       (null (plist-get state :transient-observer))
       (not (memq t (plist-get state :owned-live)))
       (null (plist-get state :body-error))
       (null (plist-get state :cleanup-errors))))

(defun buffer366-test-cleanup-clean-p (state)
  (and (buffer366-test-cleanup-nonwindow-clean-p state)
       (plist-get state :windows)
       (null (plist-get state :window-difference))
       (plist-get state :configuration)))

(defun buffer366-test-run (case-name thunk)
  (buffer366-test-provenance)
  (unless (string-match-p "\\`[a-z0-9_-]+\\'" case-name)
    (error "Buffer Move invalid case name: %S" case-name))
  (let* ((buffer366-test-world (list :case case-name))
         (buffer366-test-owned-buffers nil)
         (buffer366-test-slots nil)
         (buffer366-test-buffer-roles nil)
         (buffer366-test-layout nil)
         (buffer366-test-external-events nil)
         (buffer366-test-external-advices nil)
         (buffer366-test-command-events nil)
         (buffer366-test-command-observer-installed nil)
         (buffer366-test-transient-advice-installed nil)
         (buffer366-test-capture-transient nil)
         (buffer366-test-transient-map nil)
         (buffer366-test-transient-exit nil)
         (buffer366-test-transient-hook nil)
         (buffer366-test-transient-calls 0)
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-before (copy-sequence timer-idle-list))
         (buffer-before (current-buffer))
         (window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (buffer366-test-window-structure))
         (frame-predicate-before (frame-parameter nil 'buffer-predicate))
         (messages-before
          (buffer366-test-buffer-content-state (get-buffer "*Messages*")))
         (warnings-before
          (buffer366-test-buffer-content-state (get-buffer "*Warnings*")))
         (minibuffer-before
          (buffer366-test-buffer-content-state
           (window-buffer (minibuffer-window))))
         (conversion-before
          (buffer366-test-buffer-content-state
           (get-buffer " *code-conversion-work*")))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (buffer366-test-variable-state symbol)))
                  buffer366-test-state-symbols))
         (buffer366-test-baseline-configuration configuration-before)
         (buffer366-test-baseline-windows windows-before)
         (buffer366-test-baseline-window window-before)
         (buffer366-test-baseline-buffer buffer-before)
         body-value body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (buffer366-test-configure-world)
              (buffer366-test-install-external-guards)
              (setq body-value (funcall thunk)))
          (t (setq body-error (buffer366-test-condition-state condition))))
      (setq cleanup-errors
            (buffer366-test-attempt
             'remove-command-observer #'buffer366-test-remove-command-observer
             cleanup-errors))
      (setq cleanup-errors
            (buffer366-test-attempt
             'deactivate-transient
             (lambda ()
               (when (and buffer366-test-transient-map
                          buffer366-test-transient-exit
                          (buffer366-test-tree-contains-eq
                           buffer366-test-transient-map
                           overriding-terminal-local-map))
                 (funcall buffer366-test-transient-exit)))
             cleanup-errors))
      (setq cleanup-errors
            (buffer366-test-attempt
             'remove-transient-observer
             #'buffer366-test-disable-transient-observation cleanup-errors))
      (dotimes (pass 2)
        (let ((index 0))
          (dolist (timer (buffer366-test-new-timers timers-before idle-before))
            (setq cleanup-errors
                  (buffer366-test-attempt
                   (list 'cancel-timer pass index)
                   (lambda ()
                     (let ((function (timer--function timer)))
                       (cancel-timer timer)
                       (unless (buffer366-test-owned-timer-p timer)
                         (error "Unexpected Buffer Move timer: %S" function))))
                   cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (process (seq-difference (process-list) processes-before #'eq))
            (setq cleanup-errors
                  (buffer366-test-attempt
                   (list 'reap-process pass index)
                   (lambda ()
                     (let ((command (process-command process)))
                       (set-process-query-on-exit-flag process nil)
                       (when (process-live-p process) (delete-process process))
                       (error "Unexpected Buffer Move process: %S" command)))
                   cleanup-errors))
            (setq index (1+ index)))))
      (let ((index 0))
        (dolist (window (window-list nil 'no-minibuf))
          (setq cleanup-errors
                (buffer366-test-attempt
                 (list 'clear-dedication index)
                 (lambda () (set-window-dedicated-p window nil)) cleanup-errors))
          (setq index (1+ index))))
      (setq cleanup-errors
            (buffer366-test-attempt
             'restore-windows-first
             (lambda ()
               (unless (eq (frame-parameter nil 'buffer-predicate)
                           frame-predicate-before)
                 (set-frame-parameter
                  nil 'buffer-predicate frame-predicate-before))
               (buffer366-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      (let ((index 0))
        (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
          (setq cleanup-errors
                (buffer366-test-attempt
                 (list 'kill-buffer-first index (buffer-name buffer))
                 (lambda ()
                   (when (buffer-live-p buffer)
                     (let ((allowed
                            (or (memq buffer buffer366-test-owned-buffers)
                                (string-prefix-p " *Minibuf-" (buffer-name buffer))
                                (member (buffer-name buffer)
                                        '("*Completions*" "*Messages*" "*Warnings*")))))
                       (with-current-buffer buffer
                         (setq kill-buffer-query-functions nil)
                         (set-buffer-modified-p nil))
                       (kill-buffer buffer)
                       (unless allowed
                         (error "Unexpected Buffer Move buffer: %S"
                                (buffer-name buffer))))))
                 cleanup-errors))
          (setq index (1+ index))))
      (dolist (entry states-before)
        (unless (memq (car entry) buffer366-test-terminal-state-symbols)
          (setq cleanup-errors
                (buffer366-test-attempt
                 (list 'restore-variable (car entry))
                 (lambda ()
                   (buffer366-test-restore-variable (car entry) (cdr entry)))
                 cleanup-errors))))
      (dolist (entry `((messages . ,messages-before)
                       (warnings . ,warnings-before)
                       (minibuffer . ,minibuffer-before)
                       (code-conversion . ,conversion-before)))
        (setq cleanup-errors
              (buffer366-test-attempt
               (list 'restore-buffer-state (car entry))
               (lambda () (buffer366-test-restore-buffer-content (cdr entry)))
               cleanup-errors)))
      (setq cleanup-errors
            (buffer366-test-attempt
             'restore-windows-second
             (lambda ()
               (unless (eq (frame-parameter nil 'buffer-predicate)
                           frame-predicate-before)
                 (set-frame-parameter
                  nil 'buffer-predicate frame-predicate-before))
               (buffer366-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      (let ((index 0))
        (dolist (timer (buffer366-test-new-timers timers-before idle-before))
          (setq cleanup-errors
                (buffer366-test-attempt
                 (list 'restore-reaction-timer index)
                 (lambda ()
                   (let ((function (timer--function timer)))
                     (cancel-timer timer)
                     (unless (buffer366-test-owned-timer-p timer)
                       (error "Unexpected Buffer Move restore timer: %S" function))))
                 cleanup-errors))
          (setq index (1+ index))))
      (dolist (entry states-before)
        (when (memq (car entry) buffer366-test-terminal-state-symbols)
          (setq cleanup-errors
                (buffer366-test-attempt
                 (list 'restore-terminal-variable (car entry))
                 (lambda ()
                   (buffer366-test-restore-variable (car entry) (cdr entry)))
                 cleanup-errors))))
      ;; Restoration can schedule resources.  Attempt every sibling again
      ;; before removing the fail-closed external guards.
      (dotimes (pass 2)
        (let ((index 0))
          (dolist (timer (buffer366-test-new-timers timers-before idle-before))
            (setq cleanup-errors
                  (buffer366-test-attempt
                   (list 'final-timer pass index)
                   (lambda () (cancel-timer timer)) cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (process (seq-difference (process-list) processes-before #'eq))
            (setq cleanup-errors
                  (buffer366-test-attempt
                   (list 'final-process pass index)
                   (lambda ()
                     (set-process-query-on-exit-flag process nil)
                     (when (process-live-p process) (delete-process process))
                     (when (process-live-p process)
                       (error "Buffer Move process survived: %S" process)))
                   cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
            (setq cleanup-errors
                  (buffer366-test-attempt
                   (list 'final-buffer pass index (buffer-name buffer))
                   (lambda ()
                     (when (buffer-live-p buffer)
                       (with-current-buffer buffer
                         (setq kill-buffer-query-functions nil)
                         (set-buffer-modified-p nil))
                       (kill-buffer buffer)))
                   cleanup-errors))
            (setq index (1+ index)))))
      (setq cleanup-errors
            (buffer366-test-attempt
             'restore-windows-final
             (lambda ()
               (unless (eq (frame-parameter nil 'buffer-predicate)
                           frame-predicate-before)
                 (set-frame-parameter
                  nil 'buffer-predicate frame-predicate-before))
               (buffer366-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      (let ((index 0))
        (dolist (function buffer366-test-external-advices)
          (setq cleanup-errors
                (buffer366-test-attempt
                 (list 'remove-external-advice index function)
                 (lambda ()
                   (advice-remove function #'buffer366-test-forbidden-external))
                 cleanup-errors))
          (setq index (1+ index))))
      (setq buffer366-test-external-advices nil))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let* ((variable-mismatches
            (delq nil
                  (mapcar
                   (lambda (entry)
                     (unless (buffer366-test-variable-restored-p
                              (car entry) (cdr entry))
                       (car entry)))
                   states-before)))
           (cleanup-state
            (list
             :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
             :new-processes (seq-difference (process-list) processes-before #'eq)
             :new-timers (buffer366-test-new-timers timers-before idle-before)
             :variables (null variable-mismatches)
             :variable-mismatches variable-mismatches
             :messages (buffer366-test-buffer-content-restored-p messages-before)
             :warnings (buffer366-test-buffer-content-restored-p warnings-before)
             :minibuffer (buffer366-test-buffer-content-restored-p minibuffer-before)
             :code-conversion
             (buffer366-test-buffer-content-restored-p conversion-before)
             :windows (equal (buffer366-test-window-structure) windows-before)
             :window-difference
             (unless (equal (buffer366-test-window-structure) windows-before)
               (list :before
                     (buffer366-test-readable-window-structure windows-before)
                     :after
                     (buffer366-test-readable-window-structure
                      (buffer366-test-window-structure))))
             :configuration
             (compare-window-configurations
              (current-window-configuration) configuration-before)
             :frame-predicate
             (eq (frame-parameter nil 'buffer-predicate) frame-predicate-before)
             :buffer (eq (current-buffer) buffer-before)
             :window (eq (selected-window) window-before)
             :external-events (nreverse buffer366-test-external-events)
             :external-advices buffer366-test-external-advices
             :command-observer buffer366-test-command-observer-installed
             :transient-observer buffer366-test-transient-advice-installed
             :owned-live (mapcar #'buffer-live-p buffer366-test-owned-buffers)
             :body-error body-error :cleanup-errors cleanup-errors)))
      (if (buffer366-test-cleanup-clean-p cleanup-state)
          (list :result body-value :cleanup 'clean)
        (error "Buffer Move workflow/cleanup failure: %S" cleanup-state)))))
"####;

fn buffer_move_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BUFFER_MOVE_MELPA_PIN, "buffer-move.el")
        .expect("prepare exact shallow Buffer Move source below ./tmp")
        .with_prelude(BUFFER_MOVE_TEST_PRELUDE)
        .with_timeout(BUFFER_MOVE_TEST_TIMEOUT)
}

fn assert_buffer_move_batch(cases: Vec<ParityBatchCase>) {
    let test_name = std::thread::current()
        .name()
        .unwrap_or("unnamed buffer-move parity test")
        .to_owned();
    assert_oracle_batch_cases(
        buffer_move_oracle(),
        &test_name,
        "buffer_move_parity",
        &cases,
    );
}

fn assert_typed_fixture_contracts() {
    assert_eq!(
        transient_arrow_cycle(),
        [
            Direction::Right,
            Direction::Down,
            Direction::Left,
            Direction::Up,
        ]
    );
    assert_eq!(
        contiguous_transient_keys(),
        "M-x buf-move RET <right> <down> <left> <up> a"
    );
    assert!(
        ExistingRoute::new(
            WindowLayout::ReadmeThreePane,
            WindowSlot::Bottom,
            Direction::Up,
            WindowSlot::TopLeft,
            MoveBehavior::Swap,
            SelectionRequest::FollowDestination,
        )
        .is_err(),
        "full-width README bottom/up is geometry-dependent, not a typed route"
    );
    assert!(
        ExistingRoute::new(
            WindowLayout::HorizontalPair,
            WindowSlot::Left,
            Direction::Right,
            WindowSlot::Right,
            MoveBehavior::Move,
            SelectionRequest::RequestDocumentedStay,
        )
        .is_err(),
        "the documented stay request cannot be constructed for move behavior"
    );
    assert!(
        BlockedRoute::new(
            WindowLayout::SingleWindow,
            WindowSlot::Main,
            Direction::Left,
            Some(WindowSlot::Main),
            BlockReason::NoNeighbor,
        )
        .is_err(),
        "a no-neighbor route cannot declare a target"
    );
    let source_dedicated = BlockedRoute::new(
        WindowLayout::HorizontalPair,
        WindowSlot::Left,
        Direction::Right,
        Some(WindowSlot::Right),
        BlockReason::SourceDedicated,
    )
    .expect("source-dedicated horizontal route is canonical");
    assert_eq!(source_dedicated.behavior, MoveBehavior::Swap);
}

#[test]
fn buffer_move_package_batch() {
    assert_typed_fixture_contracts();
    assert_buffer_move_batch(workflows::workflow_batch_cases());
}
