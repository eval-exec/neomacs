;;; image-api-parity.el --- Image API contracts from GNU image.c -*- lexical-binding: t -*-
(require 'image)

(defconst image-api-png
  "iVBORw0KGgoAAAANSUhEUgAAAAUAAAADCAIAAADUVFKvAAAAEElEQVR4nGP4z8CAjBgI8AF1yA7yBqYM2wAAAABJRU5ErkJggg==")
(defconst image-api-alpha-png
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAEElEQVR4nGP4zwAE/xn+AwAL/QL+6O7ISQAAAABJRU5ErkJggg==")

(defun image-api-normalize (value)
  ;; Compare decoder bytes, independently of each printer's escape spelling.
  (cond ((stringp value) (list (multibyte-string-p value) (string-to-list value)))
        ((consp value) (cons (image-api-normalize (car value))
                             (image-api-normalize (cdr value))))
        (t value)))

(defun image-api-size ()
  (let ((image-scaling-factor 1.3))
    (mapcar
     (lambda (source)
       (mapcar
        (lambda (props)
          (image-size (apply #'create-image (cdr source) (car source) t props) t))
        '((:scale 1) (:scale 1.3) (:scale default)
          (:scale 2 :max-width 7) (:scale 0.5 :width 11)
          (:scale 1 :height 12) (:scale 1 :margin (2 . 3) :relief -1))))
     (list (cons 'png (base64-decode-string image-api-png))
           (cons 'svg "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"52.91\" height=\"17.75\"><rect width=\"100%\" height=\"100%\" fill=\"red\"/></svg>")))))

(defun image-api-mask ()
  (mapcar
   (lambda (bytes)
     (mapcar (lambda (props)
               (image-mask-p (apply #'create-image (base64-decode-string bytes)
                                    'png t props)))
             '(nil (:mask nil) (:mask heuristic) (:heuristic-mask t))))
   (list image-api-png image-api-alpha-png
         "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAMAAADD/I+4AAAABlBMVEX/AAAA/wDSh+9xAAAAAnRSTlMA/1uRIrUAAAALSURBVHicY2BgBAAABAACv3o/SgAAAABJRU5ErkJggg=="
         "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAMAAADD/I+4AAAABlBMVEX/AAAA/wDSh+9xAAAAAnRSTlN///M2R4wAAAALSURBVHicY2BgBAAABAACv3o/SgAAAABJRU5ErkJggg=="
         "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAABnRSTlMA/wAAAACkwsAdAAAAD0lEQVR4nGP4z8DA8J8BAAf/Af8Bf4mnAAAAAElFTkSuQmCC"
         "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAMAAADD/I+4AAAABlBMVEX/AAAA/wDSh+9xAAAAAHRSTlM2uXDMAAAAC0lEQVR4nGNgYAQAAAQAAr96P0oAAAAASUVORK5CYII="
         )))

(defun image-api-metadata ()
  (mapcar
   (lambda (encoded)
     (mapcar
      (lambda (index)
        (let ((image (create-image (base64-decode-string encoded) 'gif t
                                   :scale 1 :index index)))
          (unless (equal (image-size image t) '(1 . 1))
            (error "GIF fixture did not decode"))
          (image-api-normalize (image-metadata image))))
      ;; The last fixture has two frames with distinct GCEs.
      (if (equal encoded (car (last image-api-gifs))) '(0 1) '(0))))
   image-api-gifs))

(defconst image-api-gifs
  ;; No extension; zero/nonzero GCE; comment continuations/high bytes; two frames.
  '(
    "R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAkQBADs="
    "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAICRAEAOw=="
    "R0lGODlhAQABAIAAAAAAAP///yH5BAEKAAAALAAAAAABAAEAAAICRAEAOw=="
    "R0lGODlhAQABAIAAAAAAAP///yH+Av+AA2J5ZQAh+QQBCgAAACwAAAAAAQABAAACAkQBADs="
    "R0lGODlhAQABAIAAAAAAAP///yH5BAECAAAALAAAAAABAAEAAAICRAEAIfkEAQQAAAAsAAAAAAEAAQAAAgJEAQA7"
    ))

(defun image-api-webp ()
  (mapcar
   (lambda (index)
     (image-metadata
      (create-image
       (base64-decode-string
        "UklGRoQAAABXRUJQVlA4WAoAAAACAAAAAQAAAAAAQU5JTQYAAAAAAAAAAABBTk1GKAAAAAAAAAAAAAEAAAAAABQAAAJWUDhMDwAAAC8BAAAABxD9j/4HIqL/AQBBTk1GKAAAAAAAAAAAAAEAAAAAACgAAAJWUDhMDwAAAC8BAAAAB9D/iP4HIqL/AQA=")
       'webp t :scale 1 :index index)))
   '(0 1)))

(defconst image-api-bigtiffs
  ;; Attested GNU 31.1: little-/big-endian BigTIFF, each with 2x1 and 3x2 pages.
  '(
    "SUkrAAgAAAAQAAAAAAAAAAoAAAAAAAAAAAEEAAEAAAAAAAAAAgAAAAAAAAABAQQAAQAAAAAAAAABAAAAAAAAAAIBAwABAAAAAAAAAAgAAAAAAAAAAwEDAAEAAAAAAAAAAQAAAAAAAAAGAQMAAQAAAAAAAAACAAAAAAAAABEBBAABAAAAAAAAAOgAAAAAAAAAFQEDAAEAAAAAAAAAAwAAAAAAAAAWAQQAAQAAAAAAAAABAAAAAAAAABcBBAABAAAAAAAAAAYAAAAAAAAAHAEDAAEAAAAAAAAAAQAAAAAAAADwAAAAAAAAAP8AAP8AAAAACgAAAAAAAAAAAQQAAQAAAAAAAAADAAAAAAAAAAEBBAABAAAAAAAAAAIAAAAAAAAAAgEDAAEAAAAAAAAACAAAAAAAAAADAQMAAQAAAAAAAAABAAAAAAAAAAYBAwABAAAAAAAAAAIAAAAAAAAAEQEEAAEAAAAAAAAAyAEAAAAAAAAVAQMAAQAAAAAAAAADAAAAAAAAABYBBAABAAAAAAAAAAIAAAAAAAAAFwEEAAEAAAAAAAAAEgAAAAAAAAAcAQMAAQAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAP8AAP8AAP8AAP8AAP8AAP8A"
    "TU0AKwAIAAAAAAAAAAAAEAAAAAAAAAAKAQAABAAAAAAAAAABAAAAAgAAAAABAQAEAAAAAAAAAAEAAAABAAAAAAECAAMAAAAAAAAAAQAIAAAAAAAAAQMAAwAAAAAAAAABAAEAAAAAAAABBgADAAAAAAAAAAEAAgAAAAAAAAERAAQAAAAAAAAAAQAAAOgAAAAAARUAAwAAAAAAAAABAAMAAAAAAAABFgAEAAAAAAAAAAEAAAABAAAAAAEXAAQAAAAAAAAAAQAAAAYAAAAAARwAAwAAAAAAAAABAAEAAAAAAAAAAAAAAAAA8P8AAP8AAAAAAAAAAAAAAAoBAAAEAAAAAAAAAAEAAAADAAAAAAEBAAQAAAAAAAAAAQAAAAIAAAAAAQIAAwAAAAAAAAABAAgAAAAAAAABAwADAAAAAAAAAAEAAQAAAAAAAAEGAAMAAAAAAAAAAQACAAAAAAAAAREABAAAAAAAAAABAAAByAAAAAABFQADAAAAAAAAAAEAAwAAAAAAAAEWAAQAAAAAAAAAAQAAAAIAAAAAARcABAAAAAAAAAABAAAAEgAAAAABHAADAAAAAAAAAAEAAQAAAAAAAAAAAAAAAAAAAP8AAP8AAP8AAP8AAP8AAP8A"
    ))

(defun image-api-bigtiff-case (encoded)
  (let* ((max-image-size nil)
         (bytes (base64-decode-string encoded))
         (file (make-temp-file "neomacs-image-api-bigtiff-" nil ".unknown")))
    (unwind-protect
        (progn
          (let ((coding-system-for-write 'no-conversion))
            (with-temp-file file
              (set-buffer-multibyte nil)
              (insert bytes)))
          ;; Type comes from the TIFF bytes, not an extension hint. Both
          ;; sources must select the same page and return count-only metadata.
          (mapcar
           (lambda (source)
             (mapcar
              (lambda (index)
                (let ((image (create-image (car source) 'tiff (cdr source)
                                           :scale 1 :index index)))
                  (condition-case e
                      (list (image-size image t) (image-metadata image))
                    (error (list 'error (error-message-string e))))))
              '(0 1)))
           (list (cons bytes t) (cons file nil))))
      (delete-file file))))

(defun image-api-tiff ()
  (append
   (mapcar
    (lambda (index)
      (let ((image (create-image
                    (base64-decode-string
                     "SUkqAAgAAAAKAAABBAABAAAAAgAAAAEBBAABAAAAAQAAAAIBAwADAAAAhgAAAAMBAwABAAAAAQAAAAYBAwABAAAAAgAAABEBBAABAAAAjAAAABUBAwABAAAAAwAAABYBBAABAAAAAQAAABcBBAABAAAABgAAABwBAwABAAAAAQAAAJIAAAAIAAgACAD/AAD/AAAKAAABBAABAAAAAwAAAAEBBAABAAAAAgAAAAIBAwADAAAAEAEAAAMBAwABAAAAAQAAAAYBAwABAAAAAgAAABEBBAABAAAAFgEAABUBAwABAAAAAwAAABYBBAABAAAAAgAAABcBBAABAAAAEgAAABwBAwABAAAAAQAAAAAAAAAIAAgACAAA/wAA/wAA/wAA/wAA/wAA/wA=")
                    'tiff t :scale 1 :index index)))
        (condition-case e
            (list (image-size image t) (image-metadata image))
          (error (list 'error (error-message-string e))))))
    '(0 1))
   (mapcar #'image-api-bigtiff-case image-api-bigtiffs)))

(defun image-api-tiff-damaged-links ()
  (let* ((file (base64-decode-string
               "SUkqAAgAAAAKAAABBAABAAAAAgAAAAEBBAABAAAAAQAAAAIBAwADAAAAhgAAAAMBAwABAAAAAQAAAAYBAwABAAAAAgAAABEBBAABAAAAjAAAABUBAwABAAAAAwAAABYBBAABAAAAAQAAABcBBAABAAAABgAAABwBAwABAAAAAQAAAJIAAAAIAAgACAD/AAD/AAAKAAABBAABAAAAAwAAAAEBBAABAAAAAgAAAAIBAwADAAAAEAEAAAMBAwABAAAAAQAAAAYBAwABAAAAAgAAABEBBAABAAAAFgEAABUBAwABAAAAAwAAABYBBAABAAAAAgAAABcBBAABAAAAEgAAABwBAwABAAAAAQAAAAAAAAAIAAgACAAA/wAA/wAA/wAA/wAA/wAA/wA=")))
    (mapcar
     (lambda (change)
       (let ((bytes (copy-sequence file)))
         (dotimes (i 4)
           (aset bytes (+ (car change) i)
                 (logand 255 (ash (cdr change) (* -8 i)))))
         (mapcar
          (lambda (index)
            (let ((image (create-image bytes 'tiff t :scale 1 :index index)))
              (condition-case e
                  (list (image-size image t) (image-metadata image))
                (error (list 'error (error-message-string e))))))
          (if (= (car change) 130) '(0) '(0 1)))))
     '((130 . 4294967040) (130 . 8)
       (268 . 4294967040) (268 . 8)))))


(defun image-api-wait-cache-size (pred &optional stable-seconds timeout-seconds)
  "Wait until (funcall PRED (image-cache-size)) holds (and stays unchanged
for STABLE-SECONDS when non-nil), bounded by TIMEOUT-SECONDS."
  (let ((deadline (+ (float-time) (or timeout-seconds 10.0)))
        (stable-for (or stable-seconds 0.0))
        (stable-since nil)
        (last-size nil))
    (while (< (float-time) deadline)
      (let ((size (image-cache-size)))
        (if (funcall pred size)
            (if (<= stable-for 0.0)
                (setq deadline 0.0)
              (if (eql size last-size)
                  (when (>= (- (float-time) stable-since) stable-for)
                    (setq deadline 0.0))
                (setq last-size size
                      stable-since (float-time))))
          (setq last-size nil
                stable-since nil)))
      (when (> deadline 0.0)
        (sit-for 0.05)))
    (image-cache-size)))

(defun image-api-cache ()
  ;; GNU frees the frame image cache on the caller's thread; the shared-cache
  ;; frontend must make the same post-invalidation total visible immediately.
  (let ((buf (get-buffer-create "*image-api-cache*")))
    (switch-to-buffer buf)
    (erase-buffer)
    (redisplay t)
    ;; State-gate on startup chrome icon residency rather than a blind 0.3s
    ;; sleep: under parallel load, async GPU startup plus toolbar icon upload
    ;; can exceed 0.3s, which would otherwise let toolbar bytes land inside
    ;; the first image's delta window.
    (let* ((base (image-api-wait-cache-size (lambda (n) (> n 0)) 0.3 10.0))
           (img (create-image (base64-decode-string image-api-png) 'png t :scale 1))
           (aimg (create-image (base64-decode-string image-api-alpha-png) 'png t :scale 1)))
      (goto-char (point-max))
      (insert-image img)
      (redisplay t)
      (let ((after-plain (image-api-wait-cache-size (lambda (n) (> n base)) 0.0 10.0)))
        (goto-char (point-max))
        (insert-image aimg)
        (redisplay t)
        (let* ((after-alpha (image-api-wait-cache-size (lambda (n) (> n after-plain)) 0.0 10.0))
               ;; 5x3 opaque PNG = one RGBA pixmap; 2x1 RGBA = one pixmap.
               (plain (- after-plain base))
               (masked (- after-alpha after-plain))
               (flush-immediate (progn (image-flush img t) (image-cache-size)))
               (clear-immediate (progn (clear-image-cache t) (image-cache-size))))
          (list plain masked (- flush-immediate after-alpha) clear-immediate))))))

(defun image-api-cancel ()
  ;; A decode may still be in flight when the cache is freed.  GNU frees and
  ;; reloads synchronously; the observable contract is a clean zero, the same
  ;; geometry, and a cache that can hold the image again afterwards.
  (let* ((big (create-image
               (concat "<svg xmlns=\"http://www.w3.org/2000/svg\""
                       " width=\"1200\" height=\"900\">"
                       "<rect width=\"100%\" height=\"100%\" fill=\"blue\"/></svg>")
               'svg t :scale 1))
         (buf (get-buffer-create "*image-api-cancel*")))
    (switch-to-buffer buf)
    (erase-buffer)
    (insert-image big)
    (redisplay t)
    (let* ((cleared (progn (clear-image-cache t) (image-cache-size)))
           (size (image-size big t))
           (settled (progn (redisplay t) (sit-for 0.4) (image-cache-size))))
      ;; At most the displayed image plus ordinary chrome may be resident:
      ;; a cancelled decode that repopulated the cache would exceed this.
      (list cleared size
            (>= settled (* 1200 900 4))
            (<= settled (+ (* 1200 900 4) 1000000))))))

(defun image-api-failed ()
  ;; GNU src/image.c:3568-3583 (lookup_image) and :1655-1733 (Fimage_size,
  ;; Fimage_mask_p, Fimage_metadata): failed loads record DEFAULT_IMAGE_WIDTH/
  ;; HEIGHT (30x30) or explicit fixnum :width/:height without margins/relief,
  ;; log the diagnostic to *Messages*, and never signal.
  (let* ((text-quoting-style 'curve)
         (call (lambda (fn)
                 (condition-case e
                     (list 'ok (funcall fn))
                   (error (list 'err (error-message-string e))))))
         (missing (create-image "/nonexistent/neomacs-image-api-failed-837.png" 'png))
         (bad-png (create-image "not a png" 'png t))
         (tiny-png (base64-decode-string image-api-png))
         (cw (float (frame-char-width)))
         (ch (float (frame-char-height))))
    (with-current-buffer "*Messages*"
      (let ((inhibit-read-only t))
        (erase-buffer)))
    (let* ((missing-px (funcall call (lambda () (image-size missing t))))
           (msg-after-1 (with-current-buffer "*Messages*" (buffer-string)))
           (missing-ch (funcall call
                                (lambda ()
                                  (let ((sz (image-size missing nil)))
                                    (list (floatp (car sz))
                                          (floatp (cdr sz))
                                          (= (car sz) (/ 30.0 cw))
                                          (= (cdr sz) (/ 30.0 ch)))))))
           (msg-after-2 (with-current-buffer "*Messages*" (buffer-string)))
           (missing-mask (funcall call (lambda () (image-mask-p missing))))
           (missing-meta (funcall call (lambda () (image-metadata missing))))
           (msg-after-4 (with-current-buffer "*Messages*" (buffer-string))))
      (list
       (cons 'missing-px missing-px)
       (cons 'msg-1 msg-after-1)
       (cons 'missing-ch missing-ch)
       (cons 'msg-2 msg-after-2)
       (cons 'missing-mask missing-mask)
       (cons 'missing-meta missing-meta)
       (cons 'msg-4 msg-after-4)
       (cons 'bad-png-px (funcall call (lambda () (image-size bad-png t))))
       (cons 'bad-png-mask (funcall call (lambda () (image-mask-p bad-png))))
       (cons 'bad-png-meta (funcall call (lambda () (image-metadata bad-png))))
       (cons 'w50 (funcall call (lambda () (image-size (create-image "bad" 'png t :width 50) t))))
       (cons 'h70 (funcall call (lambda () (image-size (create-image "bad" 'png t :height 70) t))))
       (cons 'wh (funcall call (lambda () (image-size (create-image "bad" 'png t :width 50 :height 70) t))))
       (cons 'w0 (funcall call (lambda () (image-size (create-image "bad" 'png t :width 0 :height 0) t))))
       (cons 'wneg (funcall call (lambda () (image-size (create-image "bad" 'png t :width -5 :height -7) t))))
       (cons 'wflt (funcall call (lambda () (image-size (create-image "bad" 'png t :width 40.5 :height 20.5) t))))
       (cons 'wstr (funcall call (lambda () (image-size (create-image "bad" 'png t :width "50") t))))
       (cons 'margin-relief (funcall call (lambda () (image-size (create-image "bad" 'png t :margin 10 :relief 4) t))))
       (cons 'scaled-max (funcall call (lambda () (image-size (create-image "bad" 'png t :scale 2.0 :max-width 10 :max-height 10) t))))
       (cons 'oversized (let ((max-image-size 0.0001))
                          (funcall call (lambda () (image-size (create-image tiny-png 'png t) t)))))
       (cons 'invalid-size (funcall call (lambda () (image-size '(image :type nonexistent-type-837 :data "x") t))))
       (cons 'invalid-mask (funcall call (lambda () (image-mask-p '(image :type nonexistent-type-837 :data "x")))))
       (cons 'invalid-meta (funcall call (lambda () (image-metadata '(image :type nonexistent-type-837 :data "x")))))
       (cons 'nosrc-size (funcall call (lambda () (image-size '(image :type png) t))))
       (cons 'nosrc-mask (funcall call (lambda () (image-mask-p '(image :type png)))))
       (cons 'nosrc-meta (funcall call (lambda () (image-metadata '(image :type png)))))
       ;; Multi-page TIFF whose selected page fails TIFFReadRGBAImage sets
       ;; `(count N)` on img->lisp_data before returning 0 (GNU image.c:9570-9580),
       ;; while a 1-page failing TIFF leaves metadata nil and page 1 still decodes.
       (let* ((tiff2 (copy-sequence
                      (base64-decode-string
                       "SUkqAAgAAAAKAAABBAABAAAAAgAAAAEBBAABAAAAAQAAAAIBAwADAAAAhgAAAAMBAwABAAAAAQAAAAYBAwABAAAAAgAAABEBBAABAAAAjAAAABUBAwABAAAAAwAAABYBBAABAAAAAQAAABcBBAABAAAABgAAABwBAwABAAAAAQAAAJIAAAAIAAgACAD/AAD/AAAKAAABBAABAAAAAwAAAAEBBAABAAAAAgAAAAIBAwADAAAAEAEAAAMBAwABAAAAAQAAAAYBAwABAAAAAgAAABEBBAABAAAAFgEAABUBAwABAAAAAwAAABYBBAABAAAAAgAAABcBBAABAAAAEgAAABwBAwABAAAAAQAAAAAAAAAIAAgACAAA/wAA/wAA/wAA/wAA/wAA/wA="))))
         ;; Corrupt IFD0 compression (entry 3, offset 54) -> 5 (LZW) and
         ;; StripByteCounts (entry 8, offset 114) -> 99999 so page 0 fails decode.
         (aset tiff2 54 5)
         (aset tiff2 55 0)
         (aset tiff2 114 #x9f)
         (aset tiff2 115 #x86)
         (aset tiff2 116 #x01)
         (aset tiff2 117 0)
         (let* ((tiff1 (copy-sequence tiff2))
                (_ (progn (aset tiff1 130 0) (aset tiff1 131 0)
                          (aset tiff1 132 0) (aset tiff1 133 0)))
                (img2-p0 (create-image tiff2 'tiff t :index 0))
                (img1-p0 (create-image tiff1 'tiff t :index 0))
                (img2-p1 (create-image tiff2 'tiff t :index 1)))
           (cons 'failed-tiff
                 (list
                  (list (image-size img2-p0 t) (image-metadata img2-p0))
                  (list (image-size img1-p0 t) (image-metadata img1-p0))
                  (list (image-size img2-p1 t) (image-metadata img2-p1))))))))))

(defun image-api-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_IMAGE_RESULT")
    (let ((print-length nil) (print-level nil))
      (prin1 value (current-buffer))
      (insert "\n"))))

(let ((case (or (getenv "NEOMACS_IMAGE_API_CASE") "size")))
  (if (equal case "reflow")
      (progn
        ;; No measurement API or explicit display size: initial redisplay
        ;; reserves a provisional slot until asynchronous geometry arrives.
        (switch-to-buffer (get-buffer-create "*Image decode reflow*"))
        (erase-buffer)
        (insert-image
         (create-image
          "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"130\" height=\"80\"><rect width=\"130\" height=\"80\" fill=\"red\"/></svg>"
          'svg t :scale 1))
        (insert "\nafter\n")
        (goto-char (point-min))
        (run-at-time
         2 nil
         (lambda ()
           ;; Read the displayed matrix without a buffer edit or redisplay.
           (image-api-write
            (list (>= (car (window-line-height 0)) 80)
                  (>= (cdr (posn-x-y (posn-at-point 3))) 80)))
           (kill-emacs 0))))
    (image-api-write
     (pcase case
       ("size" (image-api-size))
       ("mask" (image-api-mask))
       ("metadata" (image-api-metadata))
       ("webp" (image-api-webp))
       ("tiff" (image-api-tiff))
       ("tiff-links" (image-api-tiff-damaged-links))
       ("cache" (image-api-cache))
       ("cancel" (image-api-cancel))
       ("failed" (image-api-failed))
       (_ (error "Unknown image API case: %s" case))))
    (run-at-time 1 nil (lambda () (kill-emacs 0)))))
