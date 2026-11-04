;;; neomacs-showcase.el --- GPU Feature Showcase Demo -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'neomacs-webkit nil t)
(require 'neomacs-video nil t)
(require 'neomacs-image nil t)
(require 'neomacs-effects nil t)

(defvar showcase--log-file "/tmp/neomacs-showcase.log"
  "Path to the log file.")

(defvar showcase--start-time nil
  "Float time when showcase started (for relative timestamps).")

(defun showcase--log (fmt &rest args)
  "Log FMT with ARGS to log file and stderr.
Each line is prefixed with a relative timestamp from showcase start."
  (let* ((elapsed (if showcase--start-time
                      (- (float-time) showcase--start-time)
                    0.0))
         (msg (apply #'format fmt args))
         (line (format "[%6.2fs] %s\n" elapsed msg)))
    (princ line #'external-debugging-output)
    (write-region line nil showcase--log-file t 'silent)))

(defvar showcase--timers nil
  "Section-internal timers (cancelled between sections).")

(defvar showcase--seq-timers nil
  "Sequencer timers (cancelled only on stop/restart).")

(defvar showcase--child-frames nil
  "Child frames created during demo.")

(defvar showcase--step 0
  "Current section index.")

(defvar showcase--buffers nil
  "Buffers created during demo.")

(defvar showcase--section-start-time nil
  "Float time when current section started.")

(defvar showcase--section-duration 0
  "Duration of current section in seconds.")

(defvar showcase--countdown-timer nil
  "Repeating timer for mode-line countdown.")

(defvar showcase--original-mode-line-format nil
  "Saved mode-line-format for restore.")

(defvar showcase--logo-path
  (let ((dir (file-name-directory (or load-file-name default-directory))))
    (expand-file-name "../../assets/logo-128.png" dir))
  "Path to the Neomacs logo.")

(defvar showcase--image-path
  (seq-find #'file-exists-p
            (list (expand-file-name "~/Pictures/559-4K.jpg")
                  (expand-file-name "~/Pictures/4k_image_1.jpg")
                  (expand-file-name "test/data/image/black.jpg")))
  "Path to a test image, or nil.")

(defvar showcase--video-path
  (let ((p (expand-file-name "~/Videos/4k_f1.mp4")))
    (when (file-exists-p p) p))
  "Path to first test video (4k_f1.mp4), or nil.")

(defvar showcase--video-path-2
  (let ((p (expand-file-name "~/Videos/4k2.webm")))
    (when (file-exists-p p) p))
  "Path to second test video (4k2.webm), or nil.")

(defvar showcase--sections
  '(("Opening Title"             4  showcase--section-title)
    ("Smooth Cursor Animation"   7  showcase--section-cursor-anim)
    ("Scroll Effects"            8  showcase--section-scroll)
    ("Cursor Effects Parade"    11  showcase--section-cursor-effects)
    ("Atmospheric Effects"       9  showcase--section-atmosphere)
    ("Code Intelligence"         9  showcase--section-code-intel)
    ("Rounded Box Faces"         7  showcase--section-box-faces)
    ("Fancy Box Styles"         12  showcase--section-fancy-box)
    ("Window Transitions"        9  showcase--section-window-trans)
    ("Inline Media"             13  showcase--section-media)
    ("Child Frames"             16  showcase--section-child-frames)
    ("Retro / Visual Madness"    8  showcase--section-retro)
    ("Grand Finale"             10  showcase--section-finale))
  "Alist of (NAME DURATION-SECS FUNCTION).")

(defvar showcase--rust-code
  "use wgpu::*;

pub struct WgpuRenderer {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,
    glyph_cache: HashMap<GlyphKey, CachedGlyph>,
}

impl WgpuRenderer {
    pub fn render_frame(&mut self, glyphs: &FrameGlyphBuffer) {
        let mut encoder = self.device.create_command_encoder(
            &CommandEncoderDescriptor { label: Some(\"frame\") },
        );
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(self.background),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            for glyph in &glyphs.glyphs {
                self.draw_glyph(&mut pass, glyph);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
"
  "Rust code snippet for demo buffers.")

(defvar showcase--elisp-code
  "(defun neomacs--layout-window (window params frame-glyphs)
  \"Layout WINDOW contents into FRAME-GLYPHS using PARAMS.\"
  (let* ((buffer (window-buffer window))
         (start (window-start window))
         (end (window-end window t))
         (faces (make-hash-table :test 'eq)))
    (with-current-buffer buffer
      (save-excursion
        (goto-char start)
        (while (< (point) end)
          (let ((face (get-text-property (point) 'face))
                (invisible (get-text-property (point) 'invisible))
                (display (get-text-property (point) 'display)))
            (cond
             (invisible
              (let ((next (next-single-char-property-change
                           (point) 'invisible nil end)))
                (when (and (get-text-property (point) 'ellipsis)
                           (not (eq invisible t)))
                  (push (make-glyph :char ?\\x2026 :face 'shadow
                                    :x x :y y)
                        (gethash window frame-glyphs)))
                (goto-char next)))
             (display
              (pcase display
                (`(space . ,props)
                 (let ((width (or (plist-get props :width)
                                  (plist-get props :align-to))))
                   (setq x (+ x (* width char-w)))))
                (`(image . ,props)
                 (let* ((id (plist-get props :id))
                        (w (plist-get props :width))
                        (h (plist-get props :height)))
                   (push (make-glyph :image id :x x :y y
                                     :width w :height h)
                         (gethash window frame-glyphs))
                   (setq x (+ x w))))
                (_ (forward-char 1))))
             (t
              (let ((ch (char-after))
                    (resolved (resolve-face face faces)))
                (push (make-glyph :char ch :face resolved
                                  :x x :y y
                                  :font-size (face-font-size resolved)
                                  :weight (face-weight resolved))
                      (gethash window frame-glyphs))
                (setq x (+ x (char-advance ch resolved)))
                (when (= ch ?\\n)
                  (setq x left-margin y (+ y row-height)))
                (forward-char 1)))))))\n      frame-glyphs)))
"
  "Deeply nested Elisp code for demo buffers.")

(defvar showcase--prose
  "The cursor dances across the screen with spring physics,
trailing soft blue light that fades into the darkness.
Above, an aurora of green and violet ripples silently.

This is not your grandfather's text editor.

Every glyph is rendered on the GPU. Every animation is
computed at 60 fps in the render thread. The layout engine
reads Emacs buffers directly through FFI, resolving faces,
invisible text, and display properties in Rust \x2014 then sends
a complete frame of glyphs over a crossbeam channel.

Window transitions cross-fade. Scroll effects warp and curl.
Child frames float with rounded corners and 3D shadows.
Inline video plays at native resolution. WebKit views render
the entire web, composited seamlessly into your editing surface.

All of this happens while Emacs Lisp runs undisturbed on the
main thread, blissfully unaware that the pixels it commands
now travel through a GPU pipeline that would make a game
engine developer nod approvingly.

The future of text editing is here.  It's called Neomacs.
"
  "Prose for zen mode demo.")

;;; --- Utility functions ---

(defun showcase--cancel-timers ()
  "Cancel section-internal timers (preserves sequencer timers)."
  (showcase--log "  cancel-timers: killing %d section timers (seq-timers: %d preserved)"
                 (length showcase--timers) (length showcase--seq-timers))
  (dolist (timer showcase--timers)
    (when (timerp timer) (cancel-timer timer)))
  (setq showcase--timers nil))

(defun showcase--cancel-seq-timers ()
  "Cancel sequencer timers (the run-all schedule)."
  (showcase--log "  cancel-seq-timers: killing %d sequencer timers"
                 (length showcase--seq-timers))
  (dolist (timer showcase--seq-timers)
    (when (timerp timer) (cancel-timer timer)))
  (setq showcase--seq-timers nil))

(defun showcase--stop-countdown ()
  "Stop the mode-line countdown timer."
  (when (timerp showcase--countdown-timer)
    (cancel-timer showcase--countdown-timer)
    (setq showcase--countdown-timer nil)))

(defun showcase--start-countdown (section-name duration)
  "Start a mode-line countdown for SECTION-NAME lasting DURATION seconds."
  (showcase--stop-countdown)
  (setq showcase--section-start-time (float-time))
  (setq showcase--section-duration duration)
  (showcase--update-mode-line section-name)
  (setq showcase--countdown-timer
        (run-at-time 0.5 nil
                     (lambda () (showcase--update-mode-line section-name)))))

(defun showcase--update-mode-line (section-name)
  "Update mode-line with SECTION-NAME and elapsed/remaining time."
  (let* ((elapsed (- (float-time) showcase--section-start-time))
         (remaining (max 0 (- showcase--section-duration elapsed)))
         (elapsed-i (floor elapsed))
         (remaining-i (ceiling remaining)))
    (set-default 'mode-line-format
                 (format "[%d/%d] %s  %ds / %ds  (%ds left)"
                         showcase--step
                         (length showcase--sections)
                         section-name elapsed-i
                         showcase--section-duration
                         remaining-i))
    (force-mode-line-update t)))

(defun showcase--schedule (delay fn)
  "Run FN after DELAY seconds, tracking the timer."
  (let ((timer (run-at-time delay nil fn)))
    (push timer showcase--timers)))

(defun showcase--get-buffer (name)
  "Get or create buffer NAME, track it."
  (let ((buf (get-buffer-create name)))
    (unless (memql buf showcase--buffers)
      (push buf showcase--buffers))
    buf))

(defun showcase--cleanup-child-frames ()
  "Delete all child frames."
  (dolist (f showcase--child-frames)
    (when (frame-live-p f)
      (delete-frame f)))
  (setq showcase--child-frames nil))

(defun showcase--make-child (name x y w h &optional params)
  "Create child frame NAME at (X,Y) size (W,H) chars with PARAMS."
  (let* ((buf (showcase--get-buffer (format "*showcase-%s*" name)))
         (frame (make-frame
                 (append
                  `((parent-frame . ,(selected-frame))
                    (left . ,x)
                    (top . ,y)
                    (width . ,w)
                    (height . ,h)
                    (minibuffer)
                    (no-accept-focus)
                    (child-frame-border-width . 3)
                    (internal-border-width . 6)
                    (undecorated . t)
                    (visibility . t))
                  params))))
    (unwind-protect
        (progn
          (select-frame frame 'norecord)
          (switch-to-buffer buf)
          (erase-buffer))
      (when (frame-live-p frame)
        (select-frame frame 'norecord))
      (when (buffer-live-p buf)
        (set-buffer buf)))
    (set-face-attribute 'child-frame-border frame :background "#4D99FF")
    (set-face-attribute 'internal-border frame :background "#334466")
    (push frame showcase--child-frames)
    frame))

(defun showcase--effect-option (effect)
  "Return the option `neomacs-effects' generates for gallery EFFECT."
  (intern (format "neomacs-effect-%s" effect)))

(defun showcase--effect-enable (effect)
  "Turn on gallery EFFECT, for the callers that only know it at run time.
`setopt' needs a literal variable name; the rest of this file uses it."
  (customize-set-variable (showcase--effect-option effect) '(:enabled t)))

(defun showcase--effect-reset (effect)
  "Restore gallery EFFECT's standard value, which also reaches the renderer."
  (custom-reevaluate-setting (showcase--effect-option effect)))

(defun showcase--reset-all-effects ()
  "Disable every visual effect for a clean slate."
  (showcase--log "RESET-ALL-EFFECTS called (step=%d)" showcase--step)
  (showcase--cancel-timers)
  (showcase--cleanup-child-frames)
  (delete-other-windows)
  ;; Stop all video pipelines and close webkit views (release fds)
  (when (fboundp 'neomacs-video-stop-all)
    (let ((inhibit-message t))
      (ignore-errors (neomacs-video-stop-all))))
  (when (fboundp 'neomacs-webkit-close-all)
    (let ((inhibit-message t))
      (ignore-errors (neomacs-webkit-close-all))))
  ;; Restore every renderer-defined default, then turn off the one shader
  ;; whose Rust default is intentionally enabled.
  (neomacs--effects-apply '((cursor-color-cycle :enabled nil))))

(defun showcase--title-card (title &optional subtitle)
  "Show a title card with TITLE and optional SUBTITLE."
  (showcase--log "  title-card: \"%s\" %s" title (or subtitle ""))
  (let ((buf (showcase--get-buffer "*Showcase*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "\n\n\n\n\n\n")
      (let ((start (point)))
        (insert "    " title "\n")
        (put-text-property start (point) 'face
                           '(:height 2.0 :foreground "cyan" :weight bold)))
      (when subtitle
        (insert "\n")
        (let ((start (point)))
          (insert "    " subtitle "\n")
          (put-text-property start (point) 'face
                             '(:height 1.3 :foreground "gray70"))))
      (goto-char (point-min)))))

(defun showcase--animate-cursor (positions interval)
  "Move cursor through POSITIONS ((line . col) ...) every INTERVAL secs."
  (let ((delay 0))
    (dolist (pos positions)
      (let ((line (car pos))
            (col (cdr pos)))
        (showcase--schedule delay
          (lambda ()
            (let ((buf-name (buffer-name))
                  (total-lines (count-lines (point-min) (point-max))))
              (goto-char (point-min))
              (forward-line line)
              (move-to-column col)
              (showcase--log "  animate-cursor: buf=%s lines=%d target=(%d,%d) fwd-result=%d point=%d"
                             buf-name total-lines line col line (point))
              (redisplay t)))))
      (setq delay (+ delay interval)))))

(defun showcase--fontify (buf mode)
  "Enable MODE in BUF and force full fontification."
  (with-current-buffer buf
    (funcall mode)
    (font-lock-mode 1)
    (jit-lock-mode nil)
    (font-lock-ensure)))

(defun showcase--insert-rust-buffer ()
  "Prepare a buffer with font-locked Rust code."
  (let ((buf (showcase--get-buffer "*Showcase-Rust*")))
    (with-current-buffer buf
      (erase-buffer)
      (insert showcase--rust-code))
    (showcase--fontify buf (if (fboundp 'rust-mode) 'rust-mode 'c-mode))
    buf))

(defun showcase--insert-elisp-buffer ()
  "Prepare a buffer with font-locked Elisp code."
  (let ((buf (showcase--get-buffer "*Showcase-Elisp*")))
    (with-current-buffer buf
      (erase-buffer)
      (insert showcase--elisp-code))
    (showcase--fontify buf 'emacs-lisp-mode)
    buf))

(defun showcase--insert-long-elisp-buffer ()
  "Prepare a tall buffer with 200+ lines of wide Elisp (60-90% frame width)."
  (let ((buf (showcase--get-buffer "*Showcase-Long*")))
    (with-current-buffer buf
      (erase-buffer)
      (insert ";;; neomacs-renderer.el --- GPU-accelerated display engine for Emacs  -*- lexical-binding: t -*-\n")
      (insert ";;; Commentary: This module implements the core rendering pipeline.\n\n")
      (dotimes (i 60)
        (let ((n (1+ i)))
          (insert (format "(defun neomacs-render-pipeline-stage-%03d (frame-buffer glyph-cache &optional render-config)\n" n))
          (insert (format "  \"Execute rendering stage %d: process glyph batch and submit to GPU command encoder.\"\n" n))
          (insert (format "  (let* ((surface-descriptor (neomacs--create-surface-descriptor frame-buffer %d))\n" n))
          (insert "         (command-encoder (neomacs--begin-render-pass surface-descriptor render-config))\n")
          (insert (format "         (glyph-batch (neomacs--collect-visible-glyphs glyph-cache %d frame-buffer))\n" n))
          (insert "         (pipeline-state (neomacs--resolve-pipeline-state command-encoder glyph-batch)))\n")
          (insert (format "    (when (and pipeline-state (neomacs--validate-render-state pipeline-state %d))\n" n))
          (insert "      (neomacs--submit-draw-calls command-encoder glyph-batch pipeline-state render-config)\n")
          (insert (format "      (neomacs--present-surface surface-descriptor command-encoder) ;; stage %d complete\n" n))
          (insert (format "      (message \"Render stage %d: %%d glyphs submitted\" (length glyph-batch)))))\n\n" n)))))
    (showcase--fontify buf 'emacs-lisp-mode)
    (with-current-buffer buf (goto-char (point-min)))
    buf))

(defun showcase--insert-prose-buffer ()
  "Prepare a prose buffer."
  (let ((buf (showcase--get-buffer "*Showcase-Prose*")))
    (with-current-buffer buf
      (erase-buffer)
      (insert showcase--prose))
    buf))

;;; --- Section functions ---

(defun showcase--section-title ()
  "Opening title card."
  (showcase--log ">>> SECTION 0: Opening Title START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "\n\n")
      ;; Logo + NEOMACS on same line
      (insert "  ")
      (when (and showcase--logo-path (file-exists-p showcase--logo-path))
        (condition-case nil
            (let ((img (create-image showcase--logo-path nil nil
                                     :max-width 128 :max-height 128)))
              (when img (insert-image img "[logo]")))
          (error nil))
        (insert " "))
      ;; Save position for NEOMACS typing (after logo)
      (let ((neomacs-pos (point)))
        (insert "\n\n")
        ;; Subtitle content
        (let ((start (point)))
          (insert "    Rust-powered GPU rendering. Aiming for multi-threaded Elisp.\n")
          (put-text-property start (point) 'face
                             '(:height 1.3 :foreground "gray70")))
        (let ((start (point)))
          (insert "    10x performance. 100% Emacs compatible.\n")
          (put-text-property start (point) 'face
                             '(:height 1.3 :foreground "gray70")))
        (insert "\n")
        (let ((start (point)))
          (insert "    Make Emacs Great Again!\n")
          (put-text-property start (point) 'face
                             '(:height 1.5 :foreground "#FF6633" :weight bold)))
        (insert "\n")
        ;; GitHub avatar
        (insert "    ")
        (let ((avatar-path "/tmp/eval-exec-avatar.jpg"))
          (if (file-exists-p avatar-path)
              (condition-case err
                  (let ((img (create-image avatar-path nil nil
                                           :max-width 200 :max-height 200)))
                    (if img
                        (progn
                          (insert-image img "[avatar]")
                          (insert "\n"))
                      (showcase--log "  title avatar: create-image returned nil")))
                (error (showcase--log "  title avatar error: %S" err)))
            (showcase--log "  title avatar: file not found at %s" avatar-path)))
        ;; GitHub link below avatar
        (let ((start (point)))
          (insert "    https://github.com/eval-exec/neomacs\n")
          (put-text-property start (point) 'face
                             '(:height 1.2 :foreground "#4D99FF")))
        (goto-char (point-min))
        ;; Create marker NOW, after all content is inserted
        (let ((marker (copy-marker neomacs-pos)))
          (set-marker-insertion-type marker t)
          ;; Type NEOMACS letter by letter with round box face
          (let* ((text "NEOMACS")
                 (delay 0.0)
                 (face '(:height 9.0 :foreground "#9966FF" :weight bold
                         :box (:line-width (2 . 2) :color "#9966FF" :corner-radius 20)))
                 (len (length text)))
            (dotimes (i len)
              (let ((ch (aref text i))
                    (m marker)
                    (d delay))
                (showcase--schedule d
                  (lambda ()
                    (switch-to-buffer buf)
                    (let ((inhibit-read-only t))
                      (goto-char m)
                      (insert (propertize (char-to-string ch) 'face face))
                      (redisplay t)))))
              (setq delay (+ delay 0.2)))
            ;; After typing, move cursor to "E"
            (showcase--schedule delay
              (lambda ()
                (switch-to-buffer buf)
                (goto-char (point-min))
                (when (search-forward "NEOMACS" nil t)
                  (goto-char (+ (match-beginning 0) 1)))
                (redisplay t))))))))
  ;; Breathing border (purple, slow)
  (setopt neomacs-effect-breathing-border
          '(:enabled t :color "#9966FF"
            :min-opacity 0.05 :max-opacity 0.4 :cycle-ms 4000)))

(defun showcase--section-cursor-anim ()
  "Demonstrate smooth cursor with spring physics and glow."
  (showcase--log ">>> SECTION 1: Smooth Cursor Animation START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--insert-rust-buffer)))
    (switch-to-buffer buf)
    (text-scale-set 2)
    (goto-char (point-min))
    (forward-line 5)
    (recenter)
    (redisplay t)
    ;; Enable smooth cursor + glow + line highlight
    (setopt neomacs-effect-cursor-glow
            '(:enabled t :color "#4D99FF" :radius 40))
    (setopt neomacs-effect-line-highlight
            '(:enabled t))
    ;; Animate cursor through positions
    (let ((delay 0.3))
      (showcase--schedule delay
        (lambda ()
          (switch-to-buffer buf)
          (goto-char (point-min))
          (forward-line 15)
          (move-to-column 20)
          (showcase--log "  cursor-anim: line 16 col 20 \n buf=%s point=%d line=%d"
                         (buffer-name) (point) (line-number-at-pos))
          (redisplay t)))
      (setq delay (+ delay 0.8999999999999999))
      (showcase--schedule delay
        (lambda ()
          (switch-to-buffer buf)
          (goto-char (point-min))
          (forward-line 3)
          (move-to-column 30)
          (showcase--log "  cursor-anim: line 4 col 30 \n point=%d line=%d"
                         (point) (line-number-at-pos))
          (redisplay t)))
      (setq delay (+ delay 0.6))
      (showcase--schedule delay
        (lambda ()
          (switch-to-buffer buf)
          (goto-char (point-min))
          (forward-line 25)
          (move-to-column 10)
          (showcase--log "  cursor-anim: line 26 col 10 \n point=%d line=%d"
                         (point) (line-number-at-pos))
          (redisplay t)))
      (setq delay (+ delay 0.6))
      (showcase--schedule delay
        (lambda ()
          (switch-to-buffer buf)
          (goto-char (point-min))
          (forward-line 8)
          (end-of-line)
          (showcase--log "  cursor-anim: line 9 eol \n point=%d line=%d"
                         (point) (line-number-at-pos))
          (redisplay t)))
      (setq delay (+ delay 0.6))
      (showcase--schedule delay
        (lambda ()
          (switch-to-buffer buf)
          (goto-char (point-min))
          (forward-line 20)
          (move-to-column 40)
          (showcase--log "  cursor-anim: line 21 col 40 \n point=%d line=%d"
                         (point) (line-number-at-pos))
          (redisplay t)))
      (setq delay (+ delay 0.6))
      (showcase--schedule delay
        (lambda ()
          (switch-to-buffer buf)
          (goto-char (point-min))
          (forward-line 12)
          (move-to-column 15)
          (showcase--log "  cursor-anim: line 13 col 15 \n point=%d line=%d"
                         (point) (line-number-at-pos))
          (redisplay t))))))

(defun showcase--section-scroll ()
  "Cycle through scroll effects with programmatic scrolling."
  (showcase--log ">>> SECTION 2: Scroll Effects START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--insert-long-elisp-buffer)))
    (switch-to-buffer buf)
    (goto-char (point-min))
    (let ((delay 0))
      (dolist (effect '(page-curl wobbly wave chromatic-aberration liquid))
        (showcase--schedule delay
          (lambda ()
            (showcase--log "  scroll effect: %s" effect)
            (setopt neomacs-scroll-transition-enabled t
                    neomacs-scroll-transition-effect effect)
            (message "Scroll Effect: %s" effect)))
        (showcase--schedule (+ delay 0.2)
          (lambda ()
            (switch-to-buffer buf)
            (ignore-errors (scroll-up 15))))
        (showcase--schedule (+ delay 0.6)
          (lambda ()
            (switch-to-buffer buf)
            (ignore-errors (scroll-down 15))))
        (setq delay (+ delay 0.9))))))

(defun showcase--section-cursor-effects ()
  "Parade through cursor effects using stable registry names."
  (showcase--log ">>> SECTION 3: Cursor Effects Parade START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--insert-rust-buffer)))
    (switch-to-buffer buf)
    (text-scale-set 12)
    (goto-char (point-min))
    (forward-line 8)
    (move-to-column 20)
    (recenter)
    (redisplay t)
    (let ((effects '(("Firework" . cursor-firework)
                     ("Tornado" . cursor-tornado)
                     ("Portal" . cursor-portal)
                     ("Lightning" . cursor-lightning)
                     ("Particles" . cursor-particles)
                     ("Comet" . cursor-comet)
                     ("DNA Helix" . cursor-dna-helix)
                     ("Snowflake" . cursor-snowflake)
                     ("Sparkle Burst" . cursor-sparkle-burst)
                     ("Color Cycle + Glow" . cursor-color-cycle)))
          (delay 0)
          previous)
      (dolist (entry effects)
        (let ((name (car entry))
              (effect (cdr entry))
              (old previous))
          (showcase--schedule delay
            (lambda ()
              (when old
                (showcase--effect-reset old))
              (showcase--effect-enable effect)
              (when (eq effect 'cursor-color-cycle)
                (setopt neomacs-effect-cursor-glow
                        '(:enabled t :radius 35)))
              (showcase--log "  cursor effect: %s" name)
              (message "Cursor Effect: %s" name))))
        (setq previous (cdr entry))
        (setq delay (+ delay 0.65)))
      (let ((time 0.5))
        (dotimes (i 14)
          (let ((direction (nth (mod i 7)
                                '(left left left right right right right))))
            (showcase--schedule time
              (lambda ()
                (switch-to-buffer buf)
                (ignore-errors
                  (if (eq direction 'right) (right-char 1) (left-char 1)))
                (redisplay t))))
          (setq time (+ time 0.5)))))))

(defun showcase--section-atmosphere ()
  "Cycle through ambient atmospheric effects."
  (showcase--log ">>> SECTION 4: Atmospheric Effects START")
  (showcase--reset-all-effects)
  (switch-to-buffer (showcase--insert-rust-buffer))
  (goto-char (point-min))
  ;; Each entry is a complete atomic profile.
  (let ((effects '(("Aurora Borealis"
                    ((aurora :enabled t :color1 "#33CC66"
                             :color2 "#4D66E6" :height 80 :opacity 0.18)))
                   ("Matrix Rain"
                    ((matrix-rain :enabled t :color "#00FF33"
                                  :speed 180 :opacity 0.15)))
                   ("Constellation" ((constellation :enabled t)))
                   ("Circuit Trace" ((circuit-trace :enabled t)))
                   ("Rain Effect"
                    ((rain-effect :enabled t :color "#8099CC"
                                  :drop-count 40 :speed 150 :opacity 0.18)))
                   ("Neon Border" ((neon-border :enabled t)))))
        (delay 0))
    (dolist (entry effects)
      (let ((name (car entry))
            (profile (cadr entry)))
        (showcase--schedule delay
          (lambda ()
            (neomacs--effects-apply profile)
            (showcase--log "  atmosphere: %s" name)
            (message "Atmosphere: %s" name))))
      (setq delay (+ delay 1.0)))))

(defun showcase--section-code-intel ()
  "Progressively enable code intelligence overlays."
  (showcase--log ">>> SECTION 5: Code Intelligence START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--insert-elisp-buffer)))
    (switch-to-buffer buf)
    (goto-char (point-min))
    (display-line-numbers-mode 1)
    ;; Progressive overlay enable
    (let ((features '((0.2 "Line Numbers"
                       (display-line-numbers-mode 1))
                      (0.8 "Rainbow Indent Guides"
                       (setopt neomacs-effect-indent-guides
                               '(:enabled t :rainbow-enabled t)))
                      (1.4 "Line Highlight"
                       (setopt neomacs-effect-line-highlight
                               '(:enabled t)))
                      (2.0 "Visible Whitespace"
                       (setopt neomacs-effect-show-whitespace
                               '(:enabled t :color "#33FF66")))
                      (2.6 "Minimap"
                       (setopt neomacs-effect-minimap
                               '(:enabled t)))
                      (3.2 "Focus Mode"
                       (setopt neomacs-effect-focus-mode
                               '(:enabled t :opacity 0.3)))
                      (3.8 "Scroll Progress"
                       (setopt neomacs-effect-scroll-progress
                               '(:enabled t)))
                      (4.5 "Split + Inactive Dimming"
                       (progn
                         (split-window-right)
                         (other-window 1)
                         (switch-to-buffer (showcase--insert-prose-buffer))
                         (other-window 1)
                         (setopt neomacs-effect-inactive-dim
                                 '(:enabled t :opacity 0.2))))
                      (5.5 "Zen Mode"
                       (progn
                         (delete-other-windows)
                         (showcase--effect-reset 'focus-mode)
                         (showcase--effect-reset 'minimap)
                         (switch-to-buffer (showcase--insert-prose-buffer))
                         (setopt neomacs-effect-zen-mode
                                 '(:enabled t :content-width-pct 60
                                   :margin-opacity 0.3)))))))
      (dolist (feat features)
        (let ((d (nth 0 feat))
              (name (nth 1 feat))
              (form (nth 2 feat))
              (b buf))
          (showcase--schedule d
            (lambda ()
              (switch-to-buffer b)
              (ignore-errors (eval form t))
              (showcase--log "  code-intel: %s" name)
              (message "Code Intel: %s" name))))))))

(defun showcase--section-box-faces ()
  "Demonstrate :box face attribute with rounded corners."
  (showcase--log ">>> SECTION 6: Rounded Box Faces START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase-Box*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "\n")
      (let ((start (point)))
        (insert "  Rounded Box Faces\n")
        (put-text-property start (point) 'face
                           '(:height 2.0 :foreground "#00FFCC" :weight bold)))
      (insert "\n")

      ;; Row 1: Corner Radius
      (insert "  Corner Radius:  ")
      (dolist (r '(4 8 12 16 24))
        (let ((start (point)))
          (insert (format " radius %d " r))
          (put-text-property start (point) 'face
                             `(:box (:line-width (1 . 1) :color "#4D99FF" :corner-radius ,r)
                               :foreground "#FFFFFF" :background "#2244AA" :height 1.3)))
        (insert "  "))
      (insert "\n\n")

      ;; Row 2: Colors
      (insert "  Colors:  ")
      (dolist (spec '(("#FF4D4D" "#661A1A" "red")
                      ("#4DFF4D" "#1A661A" "green")
                      ("#4D99FF" "#1A3366" "blue")
                      ("#FF4DFF" "#661A66" "purple")
                      ("#FFCC4D" "#664D1A" "gold")))
        (let ((start (point)))
          (insert (format " %s " (nth 2 spec)))
          (put-text-property start (point) 'face
                             `(:box (:line-width (1 . 1) :color ,(car spec) :corner-radius 12)
                               :foreground "#FFFFFF" :background ,(nth 1 spec) :height 1.3)))
        (insert " "))
      (insert "\n\n")

      ;; Row 3: Line Width
      (insert "  Line Width:  ")
      (dolist (w '(1 2 3 4 5))
        (let ((start (point)))
          (insert (format " width %d " w))
          (put-text-property start (point) 'face
                             `(:box (:line-width ,(cons w w) :color "#FF4DFF" :corner-radius 10)
                               :foreground "#FFFFFF" :background "#442244" :height 1.3)))
        (insert "  "))
      (insert "\n\n")

      ;; Row 4: Tags
      (insert "  Tags:  ")
      (dolist (tag '(("Rust" "#FF6633" "#331A0D")
                     ("GPU" "#33CCFF" "#0D3344")
                     ("Emacs" "#9966FF" "#2D1A66")
                     ("wgpu" "#66FF66" "#1A441A")
                     ("Lisp" "#FF66CC" "#441A33")
                     ("60fps" "#FFCC33" "#44330D")))
        (let ((start (point)))
          (insert (format " %s " (car tag)))
          (put-text-property start (point) 'face
                             `(:box (:line-width (1 . 1) :color ,(nth 1 tag) :corner-radius 8)
                               :foreground ,(nth 1 tag) :background ,(nth 2 tag)
                               :height 1.2 :weight bold)))
        (insert " "))
      (insert "\n\n")

      ;; Row 5: Large rounded boxes
      (insert "  Large:  ")
      (let ((start (point)))
        (insert "  NEOMACS  ")
        (put-text-property start (point) 'face
                           '(:box (:line-width (2 . 2) :color "#00FFCC" :corner-radius 20)
                             :foreground "#00FFCC" :background "#0D3333"
                             :height 2.0 :weight bold)))
      (insert "  ")
      (let ((start (point)))
        (insert "  GPU-Accelerated  ")
        (put-text-property start (point) 'face
                           '(:box (:line-width (2 . 2) :color "#9966FF" :corner-radius 16)
                             :foreground "#9966FF" :background "#1A0D33"
                             :height 1.8 :weight bold)))
      (insert "\n\n")

      ;; Row 6: Live typing area
      (insert "  Live Typing:  ")
      (let ((start (point)))
        (insert " ")
        (put-text-property start (point) 'face
                           '(:box (:line-width (1 . 1) :color "#33CCFF" :corner-radius 12)
                             :foreground "#FFFFFF" :background "#0D3344"
                             :height 1.5)))
      (backward-char 1)
      (let ((marker (point-marker)))
        (set-marker-insertion-type marker t)
        (goto-char (point-min))

        ;; Schedule typing "This is a round box" letter by letter
        (let ((text "This is a round box")
              (delay 2.0))
          (dotimes (i (length text))
            (let ((ch (aref text i))
                  (m marker))
              (showcase--schedule delay
                (lambda ()
                  (with-current-buffer buf
                    (let ((inhibit-read-only t))
                      (goto-char m)
                      (insert (propertize (char-to-string ch) 'face
                                          '(:box (:line-width (1 . 1) :color "#33CCFF"
                                                  :corner-radius 12)
                                            :foreground "#FFFFFF" :background "#0D3344"
                                            :height 1.5)))
                      (redisplay t))))))
            (setq delay (+ delay 0.12)))
          ;; After typing, move cursor to beginning
          (showcase--schedule delay
            (lambda ()
              (with-current-buffer buf
                (beginning-of-line)
                (redisplay t)))))))))

(defun showcase--section-fancy-box ()
  "Demonstrate all 11 fancy animated :box border styles, revealed one by one."
  (showcase--log ">>> SECTION: Fancy Box Styles START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase-FancyBox*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "\n")
      (let ((start (point)))
        (insert "  Fancy Animated Box Borders\n")
        (put-text-property start (point) 'face
                           '(:height 2.0 :foreground "#FF66CC" :weight bold)))
      (insert "\n\n"))
    (goto-char (point-max))
    (setq buffer-read-only t)

    ;; Style specs: (label face-plist)
    (let ((styles
           '((" Solid "
              (:box (:line-width 3 :color "dodger blue" :corner-radius 10
                     :border-style solid)
                :foreground "#FFFFFF" :background "#1A2244" :height 1.5))
             (" Rainbow "
              (:box (:line-width 3 :color "white" :corner-radius 12
                     :border-style rainbow)
                :foreground "#FFFFFF" :background "#222222" :height 1.5))
             (" Animated Rainbow "
              (:box (:line-width 3 :color "white" :corner-radius 12
                     :border-style animated-rainbow :border-speed 100)
                :foreground "#FFFFFF" :background "#222222" :height 1.5))
             (" Gradient "
              (:box (:line-width 3 :color "deep pink" :corner-radius 10
                     :border-style gradient :color2 "cyan")
                :foreground "#FFFFFF" :background "#331A33" :height 1.5))
             (" Pulsing Glow "
              (:box (:line-width 2 :color "lime green" :corner-radius 10
                     :border-style glow)
                :foreground "#FFFFFF" :background "#0D330D" :height 1.5))
             (" Neon Double-Stroke "
              (:box (:line-width 3 :color "magenta" :corner-radius 10
                     :border-style neon :color2 "cyan")
                :foreground "#FFFFFF" :background "#220D33" :height 1.5))
             (" Dashed "
              (:box (:line-width 3 :color "gold" :corner-radius 10
                     :border-style dashed)
                :foreground "#FFFFFF" :background "#332D0D" :height 1.5))
             (" Comet Trail "
              (:box (:line-width 3 :color "orange red" :corner-radius 12
                     :border-style comet)
                :foreground "#FFFFFF" :background "#331A0D" :height 1.5))
             (" Iridescent "
              (:box (:line-width 3 :color "steel blue" :corner-radius 10
                     :border-style iridescent)
                :foreground "#FFFFFF" :background "#1A2233" :height 1.5))
             (" Fire / Plasma "
              (:box (:line-width 3 :color "dark red" :corner-radius 10
                     :border-style fire)
                :foreground "#FFFFFF" :background "#330D0D" :height 1.5))
             (" Heartbeat "
              (:box (:line-width 2 :color "red" :corner-radius 10
                     :border-style heartbeat :color2 "green")
                :foreground "#FFFFFF" :background "#330D0D" :height 1.5))))
          (delay 0.0))
      (dolist (spec styles)
        (let ((label (car spec))
              (face-plist (cadr spec))
              (b buf)
              (d delay))
          (showcase--schedule d
            (lambda ()
              (with-current-buffer b
                (let ((inhibit-read-only t))
                  (goto-char (point-max))
                  (insert "  ")
                  (let ((start (point)))
                    (insert (propertize label 'face face-plist))
                    (insert "\n\n"))
                  (goto-char (point-max))
                  (redisplay t))))))
        (setq delay (+ delay 0.8))))))

(defun showcase--section-window-trans ()
  "Demonstrate buffer crossfade, window split animations, text-scale."
  (showcase--log ">>> SECTION 7: Window Transitions START")
  (showcase--reset-all-effects)
  (let ((buf-rust (showcase--insert-rust-buffer))
        (buf-elisp (showcase--insert-elisp-buffer))
        (buf-prose (showcase--insert-prose-buffer)))
    (switch-to-buffer buf-rust)
    ;; Crossfade between buffers
    (showcase--schedule 0.3
      (lambda ()
        (showcase--log "  window-trans: crossfade 1")
        (message "Transition: Buffer Crossfade")
        (switch-to-buffer buf-elisp)))
    (showcase--schedule 1.1
      (lambda ()
        (showcase--log "  window-trans: crossfade 2")
        (switch-to-buffer buf-prose)))
    (showcase--schedule 1.9
      (lambda ()
        (showcase--log "  window-trans: crossfade 3")
        (switch-to-buffer buf-rust)))
    ;; Split windows
    (showcase--schedule 2.7
      (lambda ()
        (showcase--log "  window-trans: split")
        (message "Transition: Window Split")
        (split-window-right)
        (other-window 1)
        (switch-to-buffer buf-elisp)))
    (showcase--schedule 3.3
      (lambda ()
        (split-window-below)
        (other-window 1)
        (switch-to-buffer buf-prose)))
    ;; Focus gradient border
    (showcase--schedule 3.8
      (lambda ()
        (showcase--log "  window-trans: focus border")
        (message "Transition: Focus Gradient Border")
        (setopt neomacs-effect-focus-gradient-border
                '(:enabled t :top-color "#4D99FF"
                  :bot-color "#FF4DFF" :width 3 :opacity 0.8))))
    (showcase--schedule 4.3
      (lambda ()
        (other-window 1)))
    ;; Text scale change
    (showcase--schedule 4.8
      (lambda ()
        (showcase--log "  window-trans: text-scale")
        (message "Transition: Font Size Crossfade")
        (delete-other-windows)
        (switch-to-buffer buf-rust)
        (text-scale-increase 2)))
    (showcase--schedule 5.5
      (lambda ()
        (text-scale-decrease 3)))))

(defun showcase--section-media ()
  "Demonstrate inline image, video, and WebKit \x2014 all in the main buffer."
  (showcase--log ">>> SECTION 8: Inline Media START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase-Media*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert (propertize "Inline Media\n" 'face '(:height 2.0 :foreground "cyan" :weight bold)))
      (insert "\n")

      ;; Inline Image
      (insert (propertize "Image: " 'face '(:foreground "gold" :weight bold)))
      (if showcase--image-path
          (condition-case err
              (let ((img (create-image showcase--image-path nil nil
                                       :max-width 400 :max-height 200)))
                (if img (insert-image img "[IMAGE]") (insert "[no image]")))
            (error (insert (format "[error: %S]" err))))
        (insert "[no image file]"))
      (insert "\n\n")

      ;; Videos
      (insert (propertize "Videos: " 'face '(:foreground "gold" :weight bold)))
      (if showcase--video-path
          (condition-case nil
              (let* ((uri (concat "file://" (expand-file-name showcase--video-path)))
                     (video-id (neomacs-video-load uri)))
                (if video-id
                    (progn
                      (let ((start (point)))
                        (insert " ")
                        (put-text-property start (point) 'display
                          `(video :id ,video-id
                                  :width 350 :height 200
                                  :loop-count -1 :autoplay t))
                        (put-text-property start (point) 'neomacs-video-id video-id))
                      (neomacs-video-play video-id))
                  (insert "[video 1 nil]")))
            (error (insert "[video 1 error]")))
        (insert "[no video 1]"))
      (insert "  ")
      (if showcase--video-path-2
          (condition-case nil
              (let* ((uri (concat "file://" (expand-file-name showcase--video-path-2)))
                     (video-id (neomacs-video-load uri)))
                (if video-id
                    (progn
                      (let ((start (point)))
                        (insert " ")
                        (put-text-property start (point) 'display
                          `(video :id ,video-id
                                  :width 350 :height 200
                                  :loop-count -1 :autoplay t))
                        (put-text-property start (point) 'neomacs-video-id video-id))
                      (neomacs-video-play video-id))
                  (insert "[video 2 nil]")))
            (error (insert "[video 2 error]")))
        (insert "[no video 2]"))
      (insert "\n")

      (goto-char (point-min))))

  ;; After 3s: inline WebKit browsers
  (showcase--schedule 3.0
    (lambda ()
      (showcase--log "  media: showing inline WebKit browsers")
      (message "Inline Media: WebKit Browsers")
      (let ((buf (get-buffer "*Showcase-Media*")))
        (when buf
          (switch-to-buffer buf)
          (let ((inhibit-read-only t))
            (goto-char (point-max))
            (insert "\n")
            (insert (propertize "WebKit: " 'face '(:foreground "gold" :weight bold)))
            (let ((wk-w (max 400 (/ (* (frame-pixel-width) 43) 100)))
                  (wk-h (max 300 (/ (* (frame-pixel-height) 50) 100))))
              ;; WebKit 1: Neomacs GitHub
              (condition-case nil
                  (progn
                    (neomacs-webkit-init)
                    (neomacs-webkit-insert
                     "https://github.com/eval-exec/neomacs"
                     wk-w wk-h t))
                (error (insert "[webkit 1 error]")))
              (insert "  ")
              ;; WebKit 2: Google
              (condition-case nil
                  (neomacs-webkit-insert
                   "https://www.google.com"
                   wk-w wk-h t)
                (error (insert "[webkit 2 error]"))))
            (insert "\n")
            ;; Scroll to make WebKit visible
            (goto-char (point-max))
            (recenter -2)
            (redisplay t)))))))

(defun showcase--section-child-frames ()
  "Demonstrate child frames with 3D shadows, borders, and transparency."
  (showcase--log ">>> SECTION 9: Child Frames START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase-CF*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert (propertize "Child Frames\n" 'face '(:height 2.0 :foreground "cyan" :weight bold)))
      (insert "\n")
      (insert (propertize "  Rounded corners, 3D shadows, transparency\n"
                          'face '(:height 1.3 :foreground "gray70")))
      (insert "\n\n\n\n\n\n\n\n\n\n")
      (goto-char (point-min))))

  ;; Configure child frame style
  (when (fboundp 'neomacs-set-child-frame-style)
    (neomacs-set-child-frame-style
     :corner-radius 16 :shadow t
     :shadow-layers 8 :shadow-offset 3 :shadow-opacity 40))

  ;; Child frame 1: Image
  (when showcase--image-path
    (let ((f (showcase--make-child "cf-img" 40 80 38 16
                                   '((alpha-background . 90)))))
      (save-current-buffer
        (unwind-protect
            (progn
              (select-frame f 'norecord)
              (insert (propertize "Floating Image\n" 'face '(:foreground "gold")))
              (condition-case nil
                  (let ((img (create-image showcase--image-path nil nil
                                           :max-width 280 :max-height 180)))
                    (when img (insert-image img "[IMG]")))
                (error nil))
              (insert "\n"))
          (when (frame-live-p f)
            (select-frame f 'norecord))
          (when (buffer-live-p (get-buffer "*showcase-cf-img*"))
            (set-buffer "*showcase-cf-img*"))))))

  ;; Child frame 2: Video
  (when showcase--video-path
    (let ((f (showcase--make-child "cf-vid" 520 80 38 16
                                   '((alpha-background . 80)))))
      (save-current-buffer
        (unwind-protect
            (progn
              (select-frame f 'norecord)
              (insert (propertize "Floating Video\n" 'face '(:foreground "gold")))
              (condition-case nil
                  (let* ((uri (concat "file://" (expand-file-name showcase--video-path)))
                         (video-id (neomacs-video-load uri)))
                    (when video-id
                      (let ((start (point)))
                        (insert " ")
                        (put-text-property start (point) 'display
                          `(video :id ,video-id
                                  :width 280 :height 180
                                  :loop-count -1 :autoplay t))
                        (put-text-property start (point) 'neomacs-video-id video-id))
                      (neomacs-video-play video-id)))
                (error nil))
              (insert "\n"))
          (when (frame-live-p f)
            (select-frame f 'norecord))
          (when (buffer-live-p (get-buffer "*showcase-cf-vid*"))
            (set-buffer "*showcase-cf-vid*"))))))

  ;; Child frame 3: Documentation popup (after 3s)
  (showcase--schedule 3.0
    (lambda ()
      (showcase--log "  child-frames: showing doc popup")
      (let ((f (showcase--make-child "cf-doc" 200 320 50 10
                                     '((alpha-background . 95)))))
        (save-current-buffer
          (unwind-protect
              (progn
                (select-frame f 'norecord)
                (insert (propertize "Documentation\n" 'face '(:foreground "cyan" :height 1.3)))
                (insert "\n")
                (insert (propertize "neomacs-set-child-frame-style" 'face '(:foreground "#FF6633")))
                (insert "\n\n")
                (insert "  :corner-radius  16\n")
                (insert "  :shadow         t\n")
                (insert "  :shadow-layers  8\n")
                (insert "  :shadow-offset  3\n")
                (insert "  :shadow-opacity 40\n"))
            (when (frame-live-p f)
              (select-frame f 'norecord))
            (when (buffer-live-p (get-buffer "*showcase-cf-doc*"))
              (set-buffer "*showcase-cf-doc*"))))
        (raise-frame f))))

  ;; Child frame 4: WebKit browser (after 5s)
  (showcase--schedule 5.0
    (lambda ()
      (showcase--log "  child-frames: showing WebKit child frame")
      (let ((f (showcase--make-child "cf-web" 150 100 82 30
                                     '((alpha-background . 100)))))
        (save-current-buffer
          (unwind-protect
              (progn
                (select-frame f 'norecord)
                (condition-case err
                    (progn
                      (showcase--log "  child-frames: calling neomacs-webkit-init")
                      (neomacs-webkit-init)
                      (showcase--log "  child-frames: calling neomacs-webkit-insert")
                      (neomacs-webkit-insert
                       "https://github.com/eval-exec/neomacs"
                       750 420 t)
                      (showcase--log "  child-frames: webkit inserted OK"))
                  (error
                   (showcase--log "  child-frames: webkit ERROR: %S" err)
                   (insert "[webkit error]"))))
            (when (frame-live-p f)
              (select-frame f 'norecord))
            (when (buffer-live-p (get-buffer "*showcase-cf-web*"))
              (set-buffer "*showcase-cf-web*"))))
        (raise-frame f)))))

(defun showcase--section-retro ()
  "Stack retro effects: CRT + dot matrix + background pattern + cursor."
  (showcase--log ">>> SECTION 10: Retro / Visual Madness START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase-Retro*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert (propertize "RETRO MODE ACTIVATED\n" 'face '(:height 2.0 :foreground "#4DFF4D" :weight bold)))
      (insert "\n")))
  ;; Stack retro effects in one renderer update.
  (neomacs--effects-apply
   '((scanlines :enabled t :spacing 3 :opacity 0.12 :color "#000000")
     (dot-matrix :enabled t)
     (bg-pattern :style 1 :spacing 16 :color "#003300" :opacity 0.08)
     (cursor-firework :enabled t)
     (typing-ripple :enabled t :max-radius 50 :duration-ms 400)))
  ;; Simulate typing
  (showcase--log "  retro: starting typing simulation")
  (let ((retro-buf (get-buffer "*Showcase-Retro*"))
        (text "> GPU render pipeline...\n> Glyph cache loaded.\n> WebKit online.\n> 60 fps locked.\n> Systems go.\n")
        (delay 0.2))
    (dotimes (i (length text))
      (let ((ch (aref text i)))
        (showcase--schedule delay
          (lambda ()
            (with-current-buffer retro-buf
              (let ((inhibit-read-only t))
                (goto-char (point-max))
                (insert (char-to-string ch)))))))
      (setq delay (+ delay 0.015)))))

(defun showcase--section-finale ()
  "Return to title with layered eye-candy."
  (showcase--log ">>> SECTION 11: Grand Finale START")
  (showcase--reset-all-effects)
  (let ((buf (showcase--get-buffer "*Showcase*")))
    (switch-to-buffer buf)
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert "\n\n")
      ;; Logo + NEOMACS on same line
      (insert "  ")
      (when (and showcase--logo-path (file-exists-p showcase--logo-path))
        (condition-case err
            (let ((img (create-image showcase--logo-path nil nil
                                     :max-width 128 :max-height 128)))
              (when img (insert-image img "[logo]")))
          (error (showcase--log "  finale logo error: %S" err)))
        (insert " "))
      ;; Save position for NEOMACS typing (after logo)
      (let ((neomacs-pos (point)))
        (insert "\n\n")
        ;; Subtitle content
        (let ((start (point)))
          (insert "    Rust-powered GPU rendering. Aiming for multi-threaded Elisp.\n")
          (put-text-property start (point) 'face
                             '(:height 1.3 :foreground "gray70")))
        (let ((start (point)))
          (insert "    10x performance. 100% Emacs compatible.\n")
          (put-text-property start (point) 'face
                             '(:height 1.3 :foreground "gray70")))
        (insert "\n")
        (let ((start (point)))
          (insert "    Make Emacs Great Again!\n")
          (put-text-property start (point) 'face
                             '(:height 1.5 :foreground "#FF6633" :weight bold)))
        (insert "\n")
        ;; GitHub avatar + link on same line
        (insert "    ")
        (let ((avatar-path "/tmp/eval-exec-avatar.jpg"))
          (if (file-exists-p avatar-path)
              (condition-case err
                  (let ((img (create-image avatar-path nil nil
                                           :max-width 200 :max-height 200)))
                    (if img
                        (progn
                          (insert-image img "[avatar]")
                          (insert " "))
                      (showcase--log "  finale avatar: create-image returned nil")))
                (error (showcase--log "  finale avatar error: %S" err)))
            (showcase--log "  finale avatar: file not found at %s" avatar-path)))
        (let ((start (point)))
          (insert "https://github.com/eval-exec/neomacs\n")
          (put-text-property start (point) 'face
                             '(:height 1.2 :foreground "#4D99FF")))
        (goto-char (point-min))
        ;; Create marker NOW, after all content is inserted
        (let ((marker (copy-marker neomacs-pos)))
          (set-marker-insertion-type marker t)
          ;; Type NEOMACS letter by letter with round box face
          (let* ((text "NEOMACS")
                 (delay 0.0)
                 (face '(:height 9.0 :foreground "#9966FF" :weight bold
                         :box (:line-width (2 . 2) :color "#9966FF" :corner-radius 20)))
                 (len (length text)))
            (dotimes (i len)
              (let ((ch (aref text i))
                    (m marker)
                    (d delay))
                (showcase--schedule d
                  (lambda ()
                    (switch-to-buffer buf)
                    (let ((inhibit-read-only t))
                      (goto-char m)
                      (insert (propertize (char-to-string ch) 'face face))
                      (redisplay t)))))
              (setq delay (+ delay 0.2)))
            ;; After typing, move cursor to "E"
            (showcase--schedule delay
              (lambda ()
                (switch-to-buffer buf)
                (goto-char (point-min))
                (when (search-forward "NEOMACS" nil t)
                  (goto-char (+ (match-beginning 0) 1)))
                (redisplay t))))))))

  ;; Stack the prettiest effects.
  (neomacs--effects-apply
   '((aurora :enabled t :color1 "#33CC66" :color2 "#9966FF"
             :height 100 :opacity 0.2)
     (neon-border :enabled t)
     (cursor-portal :enabled t)
     (cursor-color-cycle :enabled t :speed 0.6
                         :saturation 0.9 :lightness 0.6)
     (breathing-border :enabled t :color "#00FFCC"
                       :min-opacity 0.1 :max-opacity 0.5 :cycle-ms 3000))))

;;; --- Sequencer ---

(defun showcase-run-all ()
  "Run all showcase sections automatically."
  (interactive)
  ;; Clear log file on fresh run
  (write-region "" nil showcase--log-file nil 'silent)
  (setq showcase--start-time (float-time))
  (showcase--log "=== SHOWCASE RUN-ALL START ===")
  (showcase--cancel-timers)
  (showcase--cancel-seq-timers)
  (setq showcase--step 0)
  (let ((offset 0))
    (dotimes (i (length showcase--sections))
      (let* ((section (nth i showcase--sections))
             (name (nth 0 section))
             (duration (nth 1 section))
             (fn (nth 2 section))
             (title-time offset)
             (content-time (+ offset 2)))
        (showcase--log "  scheduling section %d [%s]: title-card at +%ds, content at +%ds (duration %ds)"
                       i name title-time content-time duration)
        ;; Title card
        (let ((idx i) (n name))
          (push (run-at-time title-time nil
                  (lambda ()
                    (showcase--log "--- SEQ title-card fire: section %d [%s]" idx n)
                    (setq showcase--step idx)
                    (showcase--title-card (format "Section %d: %s" idx n)
                                         (format "(%ds)" (nth 1 (nth idx showcase--sections))))))
                showcase--seq-timers))
        ;; Content
        (let ((idx i) (n name) (dur duration) (f fn))
          (push (run-at-time content-time nil
                  (lambda ()
                    (showcase--log "--- SEQ content fire: section %d [%s] (duration %ds)" idx n dur)
                    (showcase--start-countdown n dur)
                    (condition-case err
                        (funcall f)
                      (error (showcase--log "!!! SECTION %d ERROR: %S" idx err)))))
                showcase--seq-timers))
        (setq offset (+ offset duration))))
    (showcase--log "  total scheduled duration: %ds, seq-timers count: %d"
                   offset (length showcase--seq-timers))))

(defun showcase-run-section (n)
  "Run section N (0-indexed)."
  (interactive "nSection: ")
  (showcase--log "=== RUN-SECTION %d (manual jump) ===" n)
  (when (and (>= n 0) (< n (length showcase--sections)))
    (showcase--cancel-timers)
    (showcase--cancel-seq-timers)
    (setq showcase--step n)
    (let* ((section (nth n showcase--sections))
           (name (nth 0 section))
           (duration (nth 1 section))
           (fn (nth 2 section)))
      (showcase--start-countdown name duration)
      (funcall fn))))

(defun showcase-next ()
  "Run the next section."
  (interactive)
  (showcase-run-section (1+ showcase--step)))

(defun showcase-prev ()
  "Run the previous section."
  (interactive)
  (showcase-run-section (max 0 (1- showcase--step))))

(defun showcase-quit ()
  "Stop showcase and clean up."
  (interactive)
  (showcase--log "=== SHOWCASE QUIT ===")
  (showcase--stop-countdown)
  (showcase--cancel-seq-timers)
  (showcase--reset-all-effects)
  (when showcase--original-mode-line-format
    (set-default 'mode-line-format showcase--original-mode-line-format)
    (force-mode-line-update t))
  (dolist (buf showcase--buffers)
    (when (buffer-live-p buf)
      (kill-buffer buf)))
  (setq showcase--buffers nil)
  (message "Showcase stopped."))

;;; --- Keymap and Init ---

(defvar showcase-map (make-sparse-keymap)
  "Showcase keymap.")

(define-key showcase-map "a" #'showcase-run-all)
(define-key showcase-map "n" #'showcase-next)
(define-key showcase-map "p" #'showcase-prev)
(define-key showcase-map "q" #'showcase-quit)
(define-key showcase-map "0" (lambda () (interactive) (showcase-run-section 0)))
(define-key showcase-map "1" (lambda () (interactive) (showcase-run-section 1)))
(define-key showcase-map "2" (lambda () (interactive) (showcase-run-section 2)))
(define-key showcase-map "3" (lambda () (interactive) (showcase-run-section 3)))
(define-key showcase-map "4" (lambda () (interactive) (showcase-run-section 4)))
(define-key showcase-map "5" (lambda () (interactive) (showcase-run-section 5)))
(define-key showcase-map "6" (lambda () (interactive) (showcase-run-section 6)))
(define-key showcase-map "7" (lambda () (interactive) (showcase-run-section 7)))
(define-key showcase-map "8" (lambda () (interactive) (showcase-run-section 8)))
(define-key showcase-map "9" (lambda () (interactive) (showcase-run-section 9)))

(defun showcase--init ()
  "Initialize the showcase."
  (setq showcase--start-time (float-time))
  (write-region "" nil showcase--log-file nil 'silent)
  (showcase--log "=== SHOWCASE INIT ===")
  (setq showcase--original-mode-line-format (default-value 'mode-line-format))
  ;; Double the default face height and maximize the frame
  (set-face-attribute 'default nil :height (truncate (* 1.6 (face-attribute 'default :height))))
  (set-frame-parameter nil 'fullscreen 'maximized)
  (set-frame-parameter nil 'alpha-background 100)
  ;; Download GitHub avatar if not cached
  (unless (file-exists-p "/tmp/eval-exec-avatar.jpg")
    (ignore-errors
      (require 'url)
      (url-copy-file "https://github.com/eval-exec.png?size=200"
                     "/tmp/eval-exec-avatar.jpg" t)))
  (setq overriding-local-map showcase-map)
  (if (member "--no-auto" command-line-args)
      (progn
        (delete "--no-auto" command-line-args)
        (showcase-run-section 0)
        (message "Neomacs Showcase: a=run-all  n=next  p=prev  0-9=jump  q=quit"))
    (showcase-run-all)))

;; Auto-initialize when loaded
(showcase--init)

(provide 'neomacs-showcase)
;;; neomacs-showcase.el ends here
