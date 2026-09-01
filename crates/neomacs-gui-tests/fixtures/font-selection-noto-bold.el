;;; font-selection-noto-bold.el --- Font selection oracle fixture -*- lexical-binding: t -*-

(defconst neomacs-font-selection-family "Noto Sans")
(defconst neomacs-font-selection-text "neomacs")
(defconst neomacs-font-selection-default-weight 'bold)
(defconst neomacs-font-selection-default-slant 'normal)
(defconst neomacs-font-selection-default-height 150)
(defconst neomacs-font-selection-default-size 12)

(defconst neomacs-font-selection-weight-candidates
  '((:weight thin)
    (:weight ultra-light)
    (:weight light)
    (:weight semi-light)
    (:weight regular)
    (:weight medium)
    (:weight semi-bold)
    (:weight bold)
    (:weight extra-bold)
    (:weight black)
    (:weight ultra-heavy))
  "GNU font.c weight table rows, using one representative per row.")

(defconst neomacs-font-selection-slant-candidates
  '((:slant reverse-oblique)
    (:slant reverse-italic)
    (:slant normal)
    (:slant italic)
    (:slant oblique))
  "GNU font.c slant table rows, using one representative per row.")

(defconst neomacs-font-selection-size-candidates
  '((:height 100 :size 10)
    (:height 150 :size 12)
    (:height 220 :size 18))
  "Representative face :height and font-spec :size pairs.")

(defun neomacs-font-selection-make-case (axis id &rest overrides)
  (let ((case (list :id id
                    :axis axis
                    :family neomacs-font-selection-family
                    :weight neomacs-font-selection-default-weight
                    :slant neomacs-font-selection-default-slant
                    :height neomacs-font-selection-default-height
                    :size neomacs-font-selection-default-size
                    :text neomacs-font-selection-text)))
    (while overrides
      (let ((key (pop overrides))
            (value (pop overrides)))
        (setq case (plist-put case key value))))
    case))

(defun neomacs-font-selection-weight-case (candidate)
  (let ((weight (plist-get candidate :weight)))
    (neomacs-font-selection-make-case
     'weight
     (intern (format "noto-sans-weight-%s-h150-s12" weight))
     :weight weight)))

(defun neomacs-font-selection-slant-case (candidate)
  (let ((slant (plist-get candidate :slant)))
    (neomacs-font-selection-make-case
     'slant
     (intern (format "noto-sans-slant-%s-h150-s12" slant))
     :slant slant)))

(defun neomacs-font-selection-size-case (candidate)
  (let ((height (plist-get candidate :height))
        (size (plist-get candidate :size)))
    (neomacs-font-selection-make-case
     'size
     (intern (format "noto-sans-size-bold-normal-h%s-s%s" height size))
     :height height
     :size size)))

(defconst neomacs-font-selection-cases
  (append
   (mapcar #'neomacs-font-selection-weight-case
           neomacs-font-selection-weight-candidates)
   (mapcar #'neomacs-font-selection-slant-case
           neomacs-font-selection-slant-candidates)
   (mapcar #'neomacs-font-selection-size-case
           neomacs-font-selection-size-candidates))
  "Font-selection requests compared between GNU Emacs and NEO Emacs.")

(defconst neomacs-font-selection-selected-cases
  (let ((requested-id (getenv "NEOMACS_GUI_FONT_SELECTION_CASE")))
    (if (not requested-id)
        neomacs-font-selection-cases
      (let ((requested-symbol (intern requested-id)))
        (delq nil
              (mapcar (lambda (case)
                        (and (eq (plist-get case :id) requested-symbol) case))
                      neomacs-font-selection-cases)))))
  "Oracle cases selected by the optional test-harness case filter.")

(defun neomacs-font-selection-label (case)
  (format "axis=%s family=%s weight=%s slant=%s height=%s size=%s"
          (plist-get case :axis)
          (plist-get case :family)
          (plist-get case :weight)
          (plist-get case :slant)
          (plist-get case :height)
          (plist-get case :size)))

(defun neomacs-font-selection-case-request (case)
  (let ((request (copy-sequence case)))
    (plist-put request :label (neomacs-font-selection-label case))
    request))

(defun neomacs-font-selection-face-plist (case)
  (list :family (plist-get case :family)
        :weight (plist-get case :weight)
        :slant (plist-get case :slant)
        :height (plist-get case :height)))

(switch-to-buffer (get-buffer-create "*neomacs-font-selection-noto-bold*"))
(erase-buffer)

(insert "Font selection oracle matrix\n\n")
(dolist (case neomacs-font-selection-selected-cases)
  (let* ((label (neomacs-font-selection-label case))
         (text (plist-get case :text))
         (start nil))
    (insert label "\n")
    (setq start (point))
    (insert text "\n")
    (put-text-property start (point) 'face
                       (neomacs-font-selection-face-plist case))
    (insert "\n")))
(goto-char (point-min))

(defun neomacs-font-selection-info-list (font)
  (let ((info (and font (font-info font))))
    (and info (append info nil))))

(defun neomacs-font-selection-font-fields (font)
  (let ((info (neomacs-font-selection-info-list font)))
    (list :type (and font (type-of font))
          :family (and font (font-get font :family))
          :weight (and font (font-get font :weight))
          :slant (and font (font-get font :slant))
          :xlfd (and font (font-xlfd-name font nil t))
          :font-info info
          :font-info-file (and info (nth 12 info)))))

(defun neomacs-font-selection-font-spec (case)
  (font-spec :family (plist-get case :family)
             :weight (plist-get case :weight)
             :slant (plist-get case :slant)
             :size (plist-get case :size)))

(defun neomacs-font-selection-case-result (case)
  (let* ((target (propertize
                  (plist-get case :text)
                  'face
                  (neomacs-font-selection-face-plist case)))
         (entity (find-font (neomacs-font-selection-font-spec case)))
         (object (font-at 0 nil target)))
    (list :case (plist-get case :id)
          :label (neomacs-font-selection-label case)
          :request (neomacs-font-selection-case-request case)
          :find-font (neomacs-font-selection-font-fields entity)
          :font-at (neomacs-font-selection-font-fields object))))

(defun neomacs-font-selection-result ()
  (list :cases
        (mapcar #'neomacs-font-selection-case-result
                neomacs-font-selection-selected-cases)))

(defun neomacs-font-selection-write-oracle-result ()
  (let ((path (getenv "NEOMACS_GUI_FONT_SELECTION_RESULT")))
    (when path
      (make-directory (file-name-directory path) t)
      (with-temp-file path
        (prin1 (neomacs-font-selection-result) (current-buffer))
        (insert "\n")))))

(defun neomacs-font-selection-json-escape (value)
  (let ((start 0)
        (out ""))
    (while (string-match "[\\\"\n\r\t]" value start)
      (setq out (concat out (substring value start (match-beginning 0))
                        (pcase (match-string 0 value)
                          ("\"" "\\\"")
                          ("\\" "\\\\")
                          ("\n" "\\n")
                          ("\r" "\\r")
                          ("\t" "\\t"))))
      (setq start (match-end 0)))
    (concat out (substring value start))))

(defun neomacs-font-selection-write-state ()
  (let ((path (getenv "NEOMACS_GUI_STATE_JSON")))
    (when path
      (let* ((visible-text (buffer-substring-no-properties
                            (window-start)
                            (window-end nil t)))
             (payload
              (format
               "{\"buffer_name\":\"%s\",\"point\":%d,\"window_start\":%d,\"window_end\":%d,\"visible_text\":\"%s\"}\n"
               (neomacs-font-selection-json-escape (buffer-name))
               (point)
               (window-start)
               (window-end nil t)
               (neomacs-font-selection-json-escape visible-text))))
        (make-directory (file-name-directory path) t)
        (with-temp-file path
          (insert payload))))))

(neomacs-font-selection-write-oracle-result)
(neomacs-font-selection-write-state)

(let ((snap-json (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON"))
      (snap-txt (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT")))
  (when (and snap-json (fboundp 'neomacs--write-frame-snapshot))
    (make-directory (file-name-directory snap-json) t)
    (neomacs--write-frame-snapshot snap-json t 'json))
  (when (and snap-txt (fboundp 'neomacs--write-frame-snapshot))
    (make-directory (file-name-directory snap-txt) t)
    (neomacs--write-frame-snapshot snap-txt t 'text-faces)))

(run-at-time 2 nil (lambda () (kill-emacs 0)))

;;; font-selection-noto-bold.el ends here
