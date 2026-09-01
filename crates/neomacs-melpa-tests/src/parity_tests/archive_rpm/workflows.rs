use expect_test::expect;

use super::ParityBatchCase;

fn opens_a_cpio_bundle_browses_its_listing_and_extracts_a_configuration_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_a_cpio_bundle_browses_its_listing_and_extracts_a_configuration_file",
        r##"(cl-labels
    ((pad4
      (size)
      (make-string (% (- 4 (% size 4)) 4) 0))
     (entry
      (ino mode uid gid name contents)
      (let* ((data (string-as-unibyte contents))
             (name-field (string-as-unibyte (concat name "\0")))
             (header
              (format
               "070701%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x"
               ino mode uid gid 1 0 (length data)
               0 0 0 0 (length name-field) 0)))
        (concat
         header name-field
         (pad4 (+ 110 (length name-field)))
         data
         (pad4 (length data))))))
  (let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (path (expand-file-name "widget-backup.cpio" root))
         (archive-visit-single-files nil)
         archive-buffer extracted-buffer)
    (unwind-protect
        (progn
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert
             (entry 1 #o040755 0 0 "./etc/widget/" "")
             (entry 2 #o100640 1000 100
                    "./etc/widget/server.conf"
                    "listen=127.0.0.1:8080\nmode=production\n")
             (entry 3 #o100755 0 0
                    "./usr/bin/widget-health"
                    "#!/bin/sh\nprintf 'healthy\\n'\n")
             (entry 4 #o120777 0 0
                    "./usr/bin/widget-current"
                    "widget-health")
             (entry 0 0 0 0 "TRAILER!!!" ""))
            (let ((coding-system-for-write 'no-conversion))
              (write-region (point-min) (point-max) path nil 'silent)))
          (setq archive-buffer (find-file-noselect path))
          (let (listing-lines extract-command extracted-state)
            (with-current-buffer archive-buffer
              (archive-mode t)
              (setq listing-lines
                    (cl-remove-if-not
                     (lambda (line)
                       (string-match-p
                        "\\(?:widget/\\|widget-health\\|widget-current\\)"
                        line))
                     (split-string
                      (buffer-substring-no-properties
                       (point-min) (point-max))
                      "\n" t)))
              (goto-char (point-min))
              (search-forward "./etc/widget/server.conf")
              (setq extract-command (key-binding (kbd "e")))
              (call-interactively extract-command)
              (setq extracted-buffer (current-buffer)))
            (with-current-buffer extracted-buffer
              (setq extracted-state
                    (list
                     (buffer-string)
                     (eq archive-superior-buffer archive-buffer)
                     (not (buffer-modified-p))
                     (string-suffix-p
                      "widget-backup.cpio:./etc/widget/server.conf"
                      buffer-file-name))))
            (with-current-buffer archive-buffer
              (list
               major-mode
               archive-subtype
               (eq extract-command 'archive-extract)
               listing-lines
               extracted-state))))
      (when (buffer-live-p extracted-buffer)
        (with-current-buffer extracted-buffer
          (set-buffer-modified-p nil))
        (kill-buffer extracted-buffer))
      (when (buffer-live-p archive-buffer)
        (with-current-buffer archive-buffer
          (set-buffer-modified-p nil))
        (kill-buffer archive-buffer))
      (when (file-exists-p path)
        (delete-file path)))))"##,
        expect![[
            r#"OK (archive-mode cpio t ("  drwxr-xr-x        0          0/0          ./etc/widget/" "  -rw-r-----       38       1000/100        ./etc/widget/server.conf" "  -rwxr-xr-x       29          0/0          ./usr/bin/widget-health" "  lrwxrwxrwx       13          0/0          ./usr/bin/widget-current -> widget-health") ("listen=127.0.0.1:8080\nmode=production\n" t t t))"#
        ]],
    )
}

fn extracts_a_firmware_image_from_an_rpm_without_changing_any_binary_byte() -> ParityBatchCase {
    ParityBatchCase::value(
        "extracts_a_firmware_image_from_an_rpm_without_changing_any_binary_byte",
        r##"(cl-labels
    ((hex-bytes
      (hex)
      (let ((bytes
             (string-as-unibyte
              (make-string (/ (length hex) 2) 0))))
        (dotimes (offset (/ (length hex) 2))
          (aset
           bytes offset
           (string-to-number
            (substring hex (* offset 2) (+ (* offset 2) 2))
            16)))
        bytes)))
  (let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (bin-dir (expand-file-name "bin" root))
         (xz (expand-file-name "xz" bin-dir))
         (log (expand-file-name "xz-invocations.log" root))
         (path (expand-file-name "widget-firmware-9.4-1.noarch.rpm" root))
         (real-xz (executable-find "xz"))
         (rpm-hex
          "edabeedb03000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008eade8010000000000000000000000008eade801000000000000000700000040000003e8000000060000000000000001000003e9000000060000001000000001000003ea000000060000001400000001000003ec000000060000001600000001000003fe0000000600000031000000010000046400000006000000380000000100000465000000060000003d000000017769646765742d6669726d7761726500392e3400310057696467657420636f6e74726f6c6c6572206669726d77617265006e6f61726368006370696f00787a00fd377a585a000004e6d6b44604c067980221011600000000000000001067b980e00117005f5d00180ddd04639c721b76a1b8f861d3c55b5057a7de82172d7413edd6ea853a9e4ee2989174c24d2b041d95ef7f343aad0472ce916bac4df2b71403dde7495da5e9a50d6dd83a7f6f6f8e14c3663f6b07ad74b987995c52d37d29d113279d3f000000b4287ccf89391dd000018301980200003e86611db1c467fb020000000004595a")
         (archive-visit-single-files nil)
         (process-environment (copy-sequence process-environment))
         (exec-path (copy-sequence exec-path))
         archive-buffer extracted-buffer)
    (unwind-protect
        (progn
          (unless real-xz
            (error "The archive-rpm xz workflow requires xz"))
          (make-directory bin-dir t)
          (with-temp-buffer
            (insert
             "#!/bin/sh\n"
             "set -eu\n"
             "printf '%s\\n' \"$@\" >> "
             (shell-quote-argument log)
             "\n"
             "[ \"$#\" -eq 3 ]\n"
             "[ \"$1\" = -q ]\n"
             "[ \"$2\" = -c ]\n"
             "[ \"$3\" = -d ]\n"
             "exec "
             (shell-quote-argument real-xz)
             " \"$@\"\n")
            (write-region (point-min) (point-max) xz nil 'silent))
          (set-file-modes xz #o755)
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert (hex-bytes rpm-hex))
            (let ((coding-system-for-write 'no-conversion))
              (write-region (point-min) (point-max) path nil 'silent)))
          (push bin-dir exec-path)
          (setenv "PATH"
                  (concat bin-dir path-separator (getenv "PATH")))
          (setq archive-buffer (find-file-noselect path))
          (let (listing-line extract-command extracted-state)
            (with-current-buffer archive-buffer
              (setq listing-line
                    (car
                     (cl-remove-if-not
                      (lambda (line)
                        (string-match-p
                         "./usr/lib/firmware/widget.bin\\'" line))
                      (split-string
                       (buffer-substring-no-properties
                        (point-min) (point-max))
                       "\n" t))))
              (goto-char (point-min))
              (search-forward "./usr/lib/firmware/widget.bin")
              (setq extract-command (key-binding (kbd "e")))
              (let ((coding-system-for-read 'no-conversion))
                (call-interactively extract-command))
              (setq extracted-buffer (current-buffer)))
            (with-current-buffer extracted-buffer
              (setq extracted-state
                    (list
                     (string-to-list
                      (encode-coding-string
                       (buffer-string) buffer-file-coding-system))
                     (secure-hash 'sha256 (current-buffer))
                     buffer-file-coding-system
                     enable-multibyte-characters
                     (eq archive-superior-buffer archive-buffer)
                     (not (buffer-modified-p)))))
            (list
             (with-current-buffer archive-buffer major-mode)
             (with-current-buffer archive-buffer archive-subtype)
             (eq extract-command 'archive-extract)
             listing-line
             extracted-state
             (with-temp-buffer
               (insert-file-contents log)
               (buffer-string)))))
      (when (buffer-live-p extracted-buffer)
        (with-current-buffer extracted-buffer
          (set-buffer-modified-p nil))
        (kill-buffer extracted-buffer))
      (when (buffer-live-p archive-buffer)
        (with-current-buffer archive-buffer
          (set-buffer-modified-p nil))
        (kill-buffer archive-buffer))
      (dolist (file (list path log xz))
        (when (file-exists-p file)
          (delete-file file)))
      (when (file-directory-p bin-dir)
        (delete-directory bin-dir)))))"##,
        expect![[
            r#"OK (archive-mode rpm t "  -rw-------       14          0/0          ./usr/lib/firmware/widget.bin" ((0 1 2 10 13 31 32 127 128 129 191 200 254 255) "78b23e4f97b8c75fecf06fddfb5c83b14e802f6918a71c27c244c8531782cf05" no-conversion t t t) "-q\n-c\n-d\n-q\n-c\n-d\n")"#
        ]],
    )
}

fn opens_a_common_gzip_rpm_and_extracts_its_packaged_readme() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_a_common_gzip_rpm_and_extracts_its_packaged_readme",
        r##"(cl-labels
    ((hex-bytes
     (hex)
      (let ((bytes
             (string-as-unibyte
              (make-string (/ (length hex) 2) 0))))
        (dotimes (offset (/ (length hex) 2))
          (aset
           bytes offset
           (string-to-number
            (substring hex (* offset 2) (+ (* offset 2) 2))
            16)))
        bytes)))
  (let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (path (expand-file-name "widget-3.2-7.noarch.rpm" root))
         (rpm-hex
          "edabeedb03000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008eade8010000000000000000000000008eade801000000000000000700000036000003e8000000060000000000000001000003e9000000060000000700000001000003ea000000060000000b00000001000003ec000000060000000d00000001000003fe00000006000000250000000100000464000000060000002c000000010000046500000006000000310000000177696467657400332e32003700576964676574206465706c6f796d656e742066696c6573006e6f61726368006370696f00677a6970001f8b08000000000002ff333037303730348000306d61986862801d18a271930c8802464630969e7e6971917e71466251aa7e4a7eb27e79664a7a6a897e90aba38bafab5e49450943385844a12031393b313d952b2cb5a838333f4fc158cf888bc100d9ad8400b1ead001dc4f21418e9e3eae418a8a8a0c4000004d60406528010000")
         (archive-visit-single-files nil)
         archive-buffer extracted-buffer)
    (unwind-protect
        (progn
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (insert (hex-bytes rpm-hex))
            (let ((coding-system-for-write 'no-conversion))
              (write-region (point-min) (point-max) path nil 'silent)))
          (setq archive-buffer (find-file-noselect path))
          (if (not
               (with-current-buffer archive-buffer
                 (eq major-mode 'archive-mode)))
              (with-current-buffer archive-buffer
                (list
                 major-mode
                 archive-subtype
                 (buffer-size)
                 (string-to-list
                  (buffer-substring (point-min) (+ (point-min) 6)))))
            (let (metadata listing-line extract-command)
              (with-current-buffer archive-buffer
                (setq metadata
                      (buffer-substring-no-properties
                       (point-min)
                       (save-excursion
                         (goto-char (point-min))
                         (search-forward "M   Filemode")
                         (line-beginning-position))))
                (setq listing-line
                      (car
                       (cl-remove-if-not
                        (lambda (line)
                          (string-match-p
                           "./usr/share/doc/widget/README.txt\\'" line))
                        (split-string
                         (buffer-substring-no-properties
                          (point-min) (point-max))
                         "\n" t))))
                (goto-char (point-min))
                (search-forward "./usr/share/doc/widget/README.txt")
                (setq extract-command (key-binding (kbd "e")))
                (call-interactively extract-command)
                (setq extracted-buffer (current-buffer)))
              (list
               (with-current-buffer archive-buffer major-mode)
               (with-current-buffer archive-buffer archive-subtype)
               (eq extract-command 'archive-extract)
               metadata
               listing-line
               (with-current-buffer extracted-buffer
                 (list
                  (buffer-string)
                  (eq archive-superior-buffer archive-buffer)
                  (not (buffer-modified-p))))))))
      (when (buffer-live-p extracted-buffer)
        (with-current-buffer extracted-buffer
          (set-buffer-modified-p nil))
        (kill-buffer extracted-buffer))
      (when (buffer-live-p archive-buffer)
        (with-current-buffer archive-buffer
          (set-buffer-modified-p nil))
        (kill-buffer archive-buffer))
      (when (file-exists-p path)
        (delete-file path)))))"##,
        expect![[
            r#"OK (archive-mode rpm t "Name:         widget\nVersion:      3.2\nRelease:      7\nSummary:      Widget deployment files\nArchitecture: noarch\nFormat:       cpio\nCompression:  gzip\n\n" "  -rw-r--r--       27          0/0          ./usr/share/doc/widget/README.txt" ("Widget package\nVersion 3.2\n" t t))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opens_a_cpio_bundle_browses_its_listing_and_extracts_a_configuration_file(),
        extracts_a_firmware_image_from_an_rpm_without_changing_any_binary_byte(),
        opens_a_common_gzip_rpm_and_extracts_its_packaged_readme(),
    ]
}
