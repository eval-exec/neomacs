;;; ob-wgsl.el --- Babel functions for live GPU shader blocks -*- lexical-binding: t -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;; Author: Neomacs Contributors
;; Keywords: literate programming, multimedia, graphics

;; This file is part of GNU Emacs.

;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;; NeoMacs extension (experimental): org-babel support for `wgsl' and
;; `glsl' source blocks whose execution renders a live, animated GPU
;; surface below the block — literate shader documents.  Built on shader
;; surfaces (doc/display-engine/SHADER_SURFACES.md); see
;; etc/ob-wgsl-demo.org for a walk-through.
;;
;;   #+begin_src wgsl :width 480 :height 200 :uniforms '((speed . 1.5))
;;   fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { ... }
;;   #+end_src
;;
;;   #+begin_src glsl
;;   void mainImage(out vec4 fragColor, in vec2 fragCoord) { ... }
;;   #+end_src
;;
;; `C-c C-c' compiles the block body with naga and composites the running
;; shader into the buffer at the block's #+RESULTS.  Recognized header
;; arguments:
;;
;;   :width N, :height N   surface size in pixels (default 400 x 225)
;;   :uniforms ALIST       quoted alist of initial uniform values, e.g.
;;                         '((speed . 2.0) (tint . [1.0 0.5 0.2])); each
;;                         entry generates a shader accessor (`u_speed()')
;;   :channel0 SOURCE      what `iChannel0' samples: a variable naming a
;;                         surface handle, a plain surface id, or a quoted
;;                         '(image ...) / '(video ...) spec
;;
;; How the result is displayed (and why an overlay):
;;
;; `org-babel-insert-result' strips text properties from string results
;; (`substring-no-properties' in ob-core.el), so a `display' property
;; smuggled through the returned string can never survive.  Org's own
;; precedent for live content anchored to org text is an OVERLAY whose
;; `display' property carries the media spec: `org--make-preview-overlay'
;; renders LaTeX preview images that way, and inline-image display of
;; ob-latex/ob-ditaa file results works the same (an overlay over the
;; inserted link).  ob-wgsl follows that pattern: execution returns a
;; plain descriptive string (the plain-text/TTY/export fallback), org
;; inserts it under #+RESULTS as usual, and then — from
;; `org-babel-after-execute-hook', the earliest point at which the result
;; region exists — an overlay is placed over the result content whose
;; `display' is (surface :id HANDLE :width W :height H).  Re-execution
;; with the default `:results replace' deletes the old result text, the
;; old overlay evaporates (`evaporate' t), and the hook explicitly
;; destroys the old GPU surface before placing the new overlay.
;;
;; Why errors are returned as results instead of signaled:
;;
;; `org-babel-execute-src-block' calls the language executor with no
;; `condition-case' around it (ob-core.el), so a signaled error aborts
;; the whole command: `org-babel-insert-result' never runs, the
;; `#+RESULTS' region keeps the STALE previous output (potentially a
;; still-running surface from the last successful compile), and naga's
;; multi-line span-annotated diagnostic is flattened into a transient
;; echo-area message.  Returning the diagnostic as the block's result is
;; strictly better UX: the full error text lands under #+RESULTS exactly
;; where the author is looking, replaces the stale output, persists until
;; the next run, and the surface overlay is simply not placed.  A short
;; first line is also echoed.  (The shader playground pops a dedicated
;; error buffer instead; a result block is org's equivalent.)
;;
;; Lifetime and garbage collection:
;;
;; Surface handles are GC-managed: dropping the last reference frees the
;; GPU texture at the next garbage collection.  Overlay properties DO
;; root their values in NeoMacs — `BufferManager::trace_roots'
;; (crates/neovm-core/src/buffer/buffer.rs) walks `buffer.overlays.trace_roots',
;; `OverlayList::trace_roots' (crates/neovm-core/src/buffer/overlay.rs) pushes
;; every overlay object, and the mark phase traces each overlay's
;; property list (the `VecLikeType::Overlay' arm in
;; crates/neovm-core/src/tagged/gc.rs, "overlay-plist") — so the handle stored
;; in the overlay's `display' spec and `ob-wgsl-surface' property cannot
;; be reaped while the overlay is displayed.  A buffer-local registry
;; (`ob-wgsl--overlays') additionally keeps handles of overlays that
;; already evaporated (deleted overlays leave the buffer's overlay list
;; and would otherwise drop their handle before we can free it) reachable
;; until the next prune destroys them explicitly, and lets
;; `kill-buffer-hook' free everything the buffer created.

;;; Code:

(require 'ob)
(require 'org-element)
(require 'neomacs-surface)

(defvar org-babel-tangle-lang-exts)
(add-to-list 'org-babel-tangle-lang-exts '("wgsl" . "wgsl"))
(add-to-list 'org-babel-tangle-lang-exts '("glsl" . "glsl"))

;; `C-c '' on a block edits in the shader playground's major mode when
;; available (it highlights both WGSL and Shadertoy-dialect GLSL).
(when (require 'neomacs-shader-playground nil t)
  (defvar org-src-lang-modes)
  (add-to-list 'org-src-lang-modes '("wgsl" . neomacs-shader-playground))
  (add-to-list 'org-src-lang-modes '("glsl" . neomacs-shader-playground)))

(defvar org-babel-default-header-args:wgsl
  '((:results . "replace") (:exports . "code"))
  "Default header arguments for wgsl blocks.
The rendered surface only exists inside a NeoMacs GUI session, so
`:exports code' keeps exported documents meaningful.")

(defvar org-babel-default-header-args:glsl
  '((:results . "replace") (:exports . "code"))
  "Default header arguments for glsl blocks.")

(defvar ob-wgsl-default-width 400
  "Surface width in pixels when a block has no :width header argument.")

(defvar ob-wgsl-default-height 225
  "Surface height in pixels when a block has no :height header argument.")

(defvar ob-wgsl--pending nil
  "Execution handoff to `ob-wgsl--after-execute'.
A list (BUFFER HANDLE WIDTH HEIGHT), staged by `ob-wgsl--execute' and
consumed (reset to nil) by the hook after `org-babel-insert-result' has
created the result region.  HANDLE is nil when compilation failed — the
hook then only prunes the previous run's overlay.  If org aborts between
staging and the hook, the orphaned handle is unreferenced as soon as the
variable is overwritten and the GC frees its GPU objects.")

(defvar-local ob-wgsl--overlays nil
  "All live ob-wgsl result overlays in this buffer.
Each overlay carries the surface handle in its `ob-wgsl-surface'
property.  The list serves two purposes: `ob-wgsl--prune-overlays'
explicitly destroys surfaces whose overlay was replaced or evaporated,
and `ob-wgsl--cleanup-buffer' (on `kill-buffer-hook') frees everything
this buffer created.  It also keeps an evaporated overlay's handle
GC-reachable until the prune, since deleting an overlay removes it from
the buffer's GC-rooted overlay list.")

;;; Header-argument normalization

(defun ob-wgsl--dimension (value default)
  "Return header-arg VALUE as a positive number, or DEFAULT."
  (cond ((numberp value) value)
        ((and (stringp value)
              (string-match-p "\\`[0-9]+\\(?:\\.[0-9]*\\)?\\'" value))
         (string-to-number value))
        (t default)))

(defun ob-wgsl--uniforms (value)
  "Normalize the :uniforms header argument VALUE to an alist.
Org evaluates quoted header values, so `:uniforms \\='((speed . 2.0))'
arrives as a ready alist; a string containing an alist is read."
  (cond ((null value) nil)
        ((consp value) value)
        ((stringp value)
         (condition-case nil
             (let ((read (car (read-from-string value))))
               (and (consp read) read))
           (error nil)))
        (t nil)))

(defun ob-wgsl--channel0 (value)
  "Resolve the :channel0 header argument VALUE to a surface source.
Accepts a surface handle or plain integer id, a quoted (image ...) or
\(video ...) spec, or the name of a variable holding any of those
\(e.g. `:channel0 my-source' after (setq my-source
\(neomacs-surface-create ...)))."
  (cond
   ((null value) nil)
   ((integerp value) value)
   ((eq (type-of value) 'neomacs-surface) value)
   ((consp value) value)
   ((symbolp value) (if (boundp value) (symbol-value value) value))
   ((stringp value)
    (let ((symbol (intern-soft value)))
      (cond ((and symbol (boundp symbol)) (symbol-value symbol))
            ((string-prefix-p "(" value)
             (condition-case nil (car (read-from-string value)) (error value)))
            (t value))))
   (t value)))

;;; Result display

(defun ob-wgsl--result-region ()
  "Result-content region (BEG . END) for the src block at point, or nil.
Point must be on the block (`org-babel-current-src-block-location').
Returns nil for inline blocks and when no #+RESULTS section exists
\(e.g. `:results silent' or `:results none')."
  (when (org-element-type-p (org-element-context) 'src-block)
    (when-let* ((result (org-babel-where-is-src-block-result)))
      (save-excursion
        (goto-char result)
        (forward-line 1)                ; skip the #+RESULTS: keyword line
        (let ((beg (point))
              (end (org-babel-result-end)))
          (and (> end beg) (cons beg end)))))))

(defun ob-wgsl--prune-overlays (beg end)
  "Destroy surfaces of registered overlays that are dead or in BEG..END.
Dead overlays (evaporated when `:results replace' deleted the previous
result text, or deleted by `ob-wgsl--on-modification') always go; live
ones only when they intersect BEG..END — the region about to receive a
new overlay.  Surfaces are destroyed explicitly rather than left to the
GC so the GPU objects are freed immediately."
  (let (live)
    (dolist (overlay ob-wgsl--overlays)
      (if (and (overlay-buffer overlay)
               (not (and beg end
                         (< (overlay-start overlay) end)
                         (> (overlay-end overlay) beg))))
          (push overlay live)
        (when-let* ((handle (overlay-get overlay 'ob-wgsl-surface)))
          (ignore-errors (neomacs-surface-destroy handle)))
        (delete-overlay overlay)))
    (setq ob-wgsl--overlays (nreverse live))))

(defun ob-wgsl--on-modification (overlay &rest _)
  "Delete OVERLAY when the result text under it is edited.
Same policy as org's LaTeX preview overlays: a stale rendering must not
shadow edited text.  The surface handle stays reachable through
`ob-wgsl--overlays' until the next prune or buffer cleanup destroys it."
  (delete-overlay overlay))

(defun ob-wgsl--place-overlay (beg end handle width height)
  "Overlay BEG..END with surface HANDLE displayed at WIDTH x HEIGHT.
The trailing newline stays outside the overlay so the display
replacement keeps the result's line structure.  The handle rides in the
overlay's `display' spec and `ob-wgsl-surface' property; overlay
properties are GC roots in NeoMacs (see Commentary), so the surface
cannot be collected while displayed."
  (let* ((end (if (eq (char-before end) ?\n) (1- end) end))
         (overlay (and (> end beg) (make-overlay beg end))))
    (when overlay
      (overlay-put overlay 'ob-wgsl t)
      (overlay-put overlay 'ob-wgsl-surface handle)
      (overlay-put overlay 'evaporate t)
      (overlay-put overlay 'modification-hooks
                   (list #'ob-wgsl--on-modification))
      (overlay-put overlay 'display
                   (list 'surface :id handle :width width :height height))
      (push overlay ob-wgsl--overlays))
    overlay))

(defun ob-wgsl--after-execute ()
  "Show the staged surface over the executed block's result region.
Runs on `org-babel-after-execute-hook', after `org-babel-insert-result'
has (re)created the result text; `org-babel-current-src-block-location'
is still bound to the executed block.  Consumes `ob-wgsl--pending'.  A
handle that cannot be displayed (no result region: silent/none results,
inline blocks) is destroyed rather than leaked until the next GC."
  (let ((pending ob-wgsl--pending))
    (setq ob-wgsl--pending nil)
    (pcase pending
      (`(,buffer ,handle ,width ,height)
       (let ((placed nil))
         (when (and (buffer-live-p buffer)
                    (eq buffer (current-buffer))
                    org-babel-current-src-block-location)
           (save-excursion
             (save-restriction
               (widen)
               (goto-char org-babel-current-src-block-location)
               (let ((region (ob-wgsl--result-region)))
                 (ob-wgsl--prune-overlays (car-safe region) (cdr-safe region))
                 (when (and handle region)
                   (setq placed (ob-wgsl--place-overlay
                                 (car region) (cdr region)
                                 handle width height)))))))
         (when (and handle (not placed))
           (ignore-errors (neomacs-surface-destroy handle))))))))

(defun ob-wgsl--cleanup-buffer ()
  "Destroy every surface this buffer's wgsl/glsl blocks created."
  (dolist (overlay ob-wgsl--overlays)
    (when-let* ((handle (overlay-get overlay 'ob-wgsl-surface)))
      (ignore-errors (neomacs-surface-destroy handle)))
    (delete-overlay overlay))
  (setq ob-wgsl--overlays nil))

;;; Execution

(defun ob-wgsl--execute (body params language)
  "Compile BODY as LANGUAGE (:shader for WGSL, :glsl) with PARAMS.
Returns the string result org will insert; stages the created surface
for `ob-wgsl--after-execute'.  Compile errors are returned as the
result, not signaled (see Commentary)."
  (let ((width (ob-wgsl--dimension (cdr (assq :width params))
                                   ob-wgsl-default-width))
        (height (ob-wgsl--dimension (cdr (assq :height params))
                                    ob-wgsl-default-height))
        (uniforms (ob-wgsl--uniforms (cdr (assq :uniforms params))))
        (channel0 (ob-wgsl--channel0 (cdr (assq :channel0 params)))))
    (add-hook 'org-babel-after-execute-hook #'ob-wgsl--after-execute)
    (add-hook 'kill-buffer-hook #'ob-wgsl--cleanup-buffer nil t)
    (if (not (and (fboundp 'neomacs-surface-available-p)
                  (neomacs-surface-available-p)))
        (progn
          (setq ob-wgsl--pending (list (current-buffer) nil width height))
          "shader surfaces need the NeoMacs GUI; block not compiled")
      (condition-case err
          (let ((handle (apply #'neomacs-surface-create
                               language body
                               :width width :height height :animate t
                               (append
                                (and uniforms (list :uniforms uniforms))
                                (and channel0 (list :channel0 channel0))))))
            (setq ob-wgsl--pending
                  (list (current-buffer) handle width height))
            (format "live shader surface, %s x %s px (NeoMacs compositor)"
                    width height))
        (error
         ;; Failed compile: stage a handle-less pending so the hook still
         ;; prunes the previous run's overlay, and return naga's full
         ;; diagnostics as the result (see Commentary for why returning
         ;; beats signaling here).
         (setq ob-wgsl--pending (list (current-buffer) nil width height))
         (message "ob-wgsl: %s"
                  (car (split-string (error-message-string err) "\n")))
         (error-message-string err))))))

(defun org-babel-execute:wgsl (body params)
  "Execute BODY as a WGSL fragment shader with PARAMS.
BODY defines `fn mainImage(fragCoord: vec2<f32>) -> vec4<f32>\\=' against
the shader-surface contract (doc/display-engine/SHADER_SURFACES.md); a
successful compile renders a live animated surface at the block's
#+RESULTS.  Called by `org-babel-execute-src-block'."
  (ob-wgsl--execute body params :shader))

(defun org-babel-execute:glsl (body params)
  "Execute BODY as Shadertoy-dialect GLSL with PARAMS.
BODY defines `void mainImage(out vec4 fragColor, in vec2 fragCoord)\\='
reading `iTime', `iResolution', `iMouse', `texture(iChannel0, uv)' —
most Shadertoy/Ghostty shaders paste unmodified.  Thin wrapper around
the wgsl executor selecting the GLSL front end."
  (ob-wgsl--execute body params :glsl))

(provide 'ob-wgsl)
;;; ob-wgsl.el ends here
