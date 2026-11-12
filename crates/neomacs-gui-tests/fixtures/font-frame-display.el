;;; font-frame-display.el --- Font listing with live GUI and TTY frames -*- lexical-binding: t -*-

(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defvar font-frame-observations nil)
(defvar font-frame-passes nil)

(defun font-frame-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font-frame-observe (label expected operation)
  (condition-case problem
      (let* ((value (funcall operation))
             (passed (equal value expected)))
        (push passed font-frame-passes)
        (push `((label . ,label) (passed . ,(if passed t :json-false))
                (value . ,(format "%S" value))
                (expected . ,(format "%S" expected)))
              font-frame-observations))
    (error
     (push nil font-frame-passes)
     (push `((label . ,label) (passed . :json-false)
             (error . ,(format "%S" problem)))
           font-frame-observations))))

(defun font-frame-script-case (label expected spec)
  (font-frame-observe
   label expected
   (lambda () (not (null (list-fonts spec nil 1))))))

(defun font-frame-scalable-cases ()
  ;; The private font directory contains pinned Hack and Spleen fonts.
  ;; An explicitly nil property requests non-scalable fonts; absence does not.
  (font-frame-script-case
   "scalable-explicit-nil" nil (font-spec :family "Hack" :scalable nil))
  (font-frame-script-case
   "scalable-true" t (font-spec :family "Hack" :scalable t))
  (font-frame-script-case
   "scalable-absent" t (font-spec :family "Hack"))
  (font-frame-script-case
   "scalable-name-false" nil (font-spec :name "Hack:scalable=false"))
  (font-frame-script-case
   "scalable-name-true" t (font-spec :name "Hack:scalable=true"))
  (font-frame-script-case
   "scalable-name-mixed-case" t (font-spec :name "Hack:scalable=fAlSe"))
  (font-frame-script-case
   "scalable-nil-average-width-zero" nil
   (font-spec :family "Hack" :avgwidth 0 :scalable nil)))

(defun font-frame-remapping-cases ()
  (set-frame-font "Hack-13" t)
  (let* ((plain (x-list-fonts "Hack-13" 'default nil 1))
         (unconstrained (x-list-fonts "Hack-13" nil nil 1))
         (pixels (font-get (font-spec :name (car plain)) :size)))
    (unless (and (integerp pixels) (> pixels 0))
      (error "Pinned Hack FACE did not provide a pixel size: %S" plain))
    ;; lookup_named_face receives a NULL window, but get_lface_attributes
    ;; still consults the current buffer's remapping whenever FRAME is live.
    (let ((face-remapping-alist '((default (:height 2.0) default))))
      (font-frame-observe
       "face-remapping-default-height" (* 2 pixels)
       (lambda ()
         (font-get (font-spec :name (car (x-list-fonts "Hack-13" 'default nil 1)))
                   :size)))
      (font-frame-observe
       "face-remapping-nil-face" unconstrained
       (lambda () (x-list-fonts "Hack-13" nil nil 1)))
      (font-frame-observe
       "face-remapping-invalid-face" plain
       (lambda () (x-list-fonts "Hack-13" [] nil 1))))
    (font-frame-observe
     "face-remapping-restored" plain
     (lambda () (x-list-fonts "Hack-13" 'default nil 1)))))

(defun font-frame-property-cases ()
  (font-frame-script-case
   "otf-unsupported-hack-script" nil
   (font-spec :family "Hack" :otf '(zzzz) :font-frame-case 'unsupported-otf))
  (font-frame-observe
   "font-extra-existing-slots-shared" t
   (lambda ()
     (let* ((spec (font-spec :family "Hack" :lang 'en :font-frame-custom 11
                             :font-frame-case 'shared-extra))
            (entity (car (list-fonts spec nil 1))))
       (font-put spec :lang 'fr)
       (font-put spec :font-frame-custom 22)
       (and entity (eq (font-get entity :lang) 'fr)
            (equal (font-get entity :font-frame-custom) 22)))))
  (font-frame-observe
   "font-native-key-updates-primary-slot" t
   (lambda ()
     (let* ((fake (cons "font-frame-forged" 999))
            (spec (font-spec :family "Hack" :font-entity fake
                             :font-frame-case 'native-key))
            (before (font-get spec :font-entity))
            (entity (car (list-fonts spec nil 1)))
            (after (font-get spec :font-entity)))
       (and entity (eq before fake) (not (eq after fake))
            (eq after (font-get entity :font-entity))
            (consp after) (stringp (car after))
            (not (multibyte-string-p (car after)))
            (equal (cdr after) 0)))))
  (font-frame-observe
   "font-point-size-zero-dpi" '((16) nil)
   (lambda ()
     (list
      (mapcar (lambda (font) (font-get font :size))
              (list-fonts (font-spec :family "Spleen" :size 50.0 :dpi 0)))
      (mapcar (lambda (font) (font-get font :size))
              (list-fonts (font-spec :family "Spleen" :size 50.0)))))))

(defun font-frame-script-cases ()
  ;; Every named script has a unique specification. GNU caches script
  ;; selection, so this never changes representatives for a previously used
  ;; specification. Explicit nil must precede an unconstrained Hack query.
  (let ((script-representative-chars
         '((nil . [#x4e00])
           (font-frame-all ?A #x4e00)
           (font-frame-any . [?A #x4e00])
           (font-frame-empty . [])
           (font-frame-invalid . [nil t font-frame-not-a-character])
           (font-frame-legacy-all ?A #x4e00)
           (font-frame-legacy-any . [#x4e00]))))
    (font-frame-script-case
     "script-explicit-nil-han" nil (font-spec :family "Hack" :script nil))
    (font-frame-script-case
     "script-missing-property" t (font-spec :family "Hack"))
    (font-frame-script-case
     "script-all-latin-and-han" nil
     (font-spec :family "Hack" :script 'font-frame-all))
    (font-frame-script-case
     "script-any-latin-or-han" t
     (font-spec :family "Hack" :script 'font-frame-any))
    (font-frame-script-case
     "script-any-empty" nil (font-spec :family "Hack" :script 'font-frame-empty))
    (font-frame-script-case
     "script-any-invalid" nil (font-spec :family "Hack" :script 'font-frame-invalid))
    (font-frame-script-case
     "script-unknown" t (font-spec :family "Hack" :script 'font-frame-unknown))
    (font-frame-script-case
     "script-legacy-overrides-all" t
     (font-spec :family "Hack" :registry "iso8859-2" :script 'font-frame-legacy-all))
    (font-frame-script-case
     "script-legacy-overrides-any" t
     (font-spec :family "Hack" :registry "iso8859-2" :script 'font-frame-legacy-any))))

(defun font-frame-listing-review-cases ()
  (font-frame-observe
   "otf-woff-positive-and-missing-script" '(t nil)
   (lambda ()
     (list
      (not (null (list-fonts
                  (font-spec :family "FrameLayout" :otf '(latn nil (ccmp) (kern)))
                  nil 1)))
      (not (null (list-fonts
                  (font-spec :family "FrameLayout" :otf '(zzzz)) nil 1))))))
  (font-frame-observe
   "bitmap-native-average-width" '(80)
   (lambda ()
     (mapcar (lambda (font) (font-get font :avgwidth))
             (list-fonts (font-spec :family "FrameBitmap")))))
  (font-frame-observe
   "bitmap-average-width-admission" '(t nil nil)
   (lambda ()
     (mapcar (lambda (width)
               (not (null (list-fonts
                           (font-spec :family "FrameBitmap" :avgwidth width) nil 1))))
             '(80 120 0))))
  (font-frame-observe
   "prefer-numeric-italic-oblique" '(italic oblique)
   (lambda ()
     (mapcar (lambda (slant)
               (font-get (car (list-fonts
                               (font-spec :family "FrameSlant") nil 1
                               (font-spec :slant slant))) :slant))
             '(italic oblique))))
  (font-frame-observe
   "prefer-missing-weight-unscored" t
   (lambda ()
     (let ((font (car (list-fonts
                      (font-spec :family "FrameWeight") nil 1
                      (font-spec :weight 'bold)))))
       (and font (null (font-get font :weight))))))
  (font-frame-observe
   "prefer-missing-width-unscored" t
   (lambda ()
     (let ((font (car (list-fonts
                      (font-spec :family "FrameWidth") nil 1
                      (font-spec :width 'expanded)))))
       (and font (null (font-get font :width))))))
  (font-frame-observe
   "xlfd-positioned-family" '("-*-Hack-*-*-*-*-*-*-*-*-*-*-*-*" t)
   (lambda ()
     (let* ((symbols-with-pos-enabled t)
            (family (position-symbol 'Hack 1))
            (spec (font-spec :family family)))
       (list (font-xlfd-name spec) (symbol-with-pos-p (font-get spec :family)))))))

(defun font-frame-registry-family-cases ()
  ;; Pinned GNU ftfont_spec_pattern keeps an explicit empty family, whereas
  ;; PREFER contributes only scores. Registry syntax is an unanchored Emacs
  ;; regexp after GNU's literal-dot/wildcard conversion, with first-table match.
  (font-frame-script-case "family-omitted-unrestricted" t (font-spec))
  (font-frame-script-case "family-empty-restricted" nil (font-spec :family ""))
  (font-frame-script-case "family-whitespace-restricted" nil (font-spec :family "   "))
  (font-frame-observe
   "find-font-empty-family" nil
   (lambda () (not (null (find-font (font-spec :family ""))))))
  (font-frame-script-case
   "registry-substring" t (font-spec :family "Hack" :registry "iso8859"))
  (font-frame-script-case
   "registry-regexp-bracket" t (font-spec :family "Hack" :registry "iso8859-[12]"))
  (font-frame-script-case
   "registry-regexp-alternation" t
   (font-spec :family "Hack" :registry "^iso8859-2$\\|^iso8859-1$"))
  (font-frame-script-case
   "registry-regexp-casefold" t (font-spec :family "Hack" :registry "ISO8859"))
  (font-frame-script-case
   "registry-whitespace-not-trimmed" nil
   (font-spec :family "Hack" :registry " iso8859-1 "))
  (font-frame-observe
   "registry-invalid-error-order" '(invalid-regexp nil nil)
   (lambda ()
     (list
      (condition-case problem
          (list-fonts (font-spec :family "Hack" :registry "[") nil 1)
        (invalid-regexp (car problem)))
      (list-fonts (font-spec :registry "[") nil 0)
      (list-fonts (font-spec :registry "[" :type 'ns) nil 1))))
  (font-frame-observe
   "registry-invalid-preference-ignored" t
   (lambda ()
     (not (null (list-fonts (font-spec :family "Hack") nil 1
                            (font-spec :registry "["))))))
  (font-frame-script-case
   "find-font-otf-uses-listing" nil (font-spec :family "Hack" :otf '(zzzz)))
  (font-frame-observe
   "find-font-registry-error" 'invalid-regexp
   (lambda ()
     (condition-case problem
         (find-font (font-spec :family "Hack" :registry "["))
       (invalid-regexp (car problem))))))

(defun font-frame-probe ()
  (condition-case problem
      (let* ((gui (selected-frame))
             (pattern "-SRC-Hack-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1")
             tty names)
        (font-frame-script-cases)
        (font-frame-scalable-cases)
        (font-frame-remapping-cases)
        (font-frame-property-cases)
        (font-frame-listing-review-cases)
        (font-frame-registry-family-cases)
        (setq tty (make-terminal-frame
                   `((tty . ,(getenv "NEOMACS_FONT_FRAME_TTY"))
                     (tty-type . "xterm-256color")
                     (name . "private-font-frame-tty"))))
        (select-frame gui)
        (setq names (x-list-fonts pattern nil gui 1))
        (unless (and (display-graphic-p gui) (not (display-graphic-p tty))
                     (= (length names) 1) (stringp (car names)))
          (error "Mixed-frame setup did not supply the pinned Hack font: %S" names))
        ;; GNU clears FACE for a TTY target before applying WIDTH. Listing
        ;; then uses the selected GUI frame's font driver.
        (font-frame-observe
         "gui-selected-tty-default-face-negative-width" names
         (lambda () (x-list-fonts pattern 'default tty 1 -1)))
        (font-frame-observe
         "gui-selected-tty-invalid-face-negative-width" names
         (lambda () (x-list-fonts pattern [] tty 1 -1)))
        (select-frame tty)
        (font-frame-observe
         "tty-selected-nil-frame" nil
         (lambda () (x-list-fonts pattern nil nil 1)))
        (font-frame-observe
         "tty-selected-tty-frame" nil
         (lambda () (x-list-fonts pattern nil tty 1)))
        ;; A live explicit GUI frame remains usable when the selected frame
        ;; and the dynamically bound startup symbols both describe a TTY.
        (let ((window-system nil) (initial-window-system nil))
          (font-frame-observe
           "tty-selected-gui-frame-globals-nil" names
           (lambda () (x-list-fonts pattern nil gui 1))))
        (let ((passed (not (memq nil font-frame-passes))))
          (font-frame-write
           `((passed . ,(if passed t :json-false)) (version . ,(emacs-version))
             (gui-graphic . t) (tty-graphic . :json-false)
             (script-cases . 9)
             (scalable-cases . 7)
             (face-remapping-cases . 4)
             (font-property-cases . 4)
             (listing-review-cases . 7)
             (registry-family-cases . 13)
             (observations . ,(vconcat (reverse font-frame-observations)))))
          (select-frame gui)
          (kill-emacs (if passed 0 1))))
    (error
     (font-frame-write
      `((passed . :json-false) (error . ,(format "%S" problem))
        (observations . ,(vconcat (reverse font-frame-observations)))))
     (kill-emacs 2))))

(run-at-time 0.7 nil #'font-frame-probe)
