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
       (_ (error "Unknown image API case: %s" case))))
    (run-at-time 1 nil (lambda () (kill-emacs 0)))))
