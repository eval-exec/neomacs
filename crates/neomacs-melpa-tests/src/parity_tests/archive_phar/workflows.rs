use expect_test::expect;

use super::ParityBatchCase;

fn archive_phar_opens_a_release_and_extracts_the_selected_source_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "archive_phar_opens_a_release_and_extracts_the_selected_source_file",
        r##"(let* ((release-dir
               (expand-file-name "release/" temporary-file-directory))
              (archive-file
               (expand-file-name "application.phar" release-dir))
              (fake-php
               (expand-file-name "bin/php" temporary-file-directory))
              (archive-phar-php-executable fake-php)
              (php-runtime-php-executable fake-php)
              (archive-hidden-columns nil)
              (archive-visit-single-files nil)
              archive-buffer
              member-buffer)
         (make-directory (file-name-directory fake-php) t)
         (make-directory release-dir t)
         (with-temp-file fake-php
           (insert
            (format
             "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in\n  '%s\tsrc/Main.php') printf '%%s' '<?php\nfinal class Main { public static function run(): string { return \"ready\"; } }\n' ;;\n  '%s') printf '%%s' '[{\"pathname\":\"bin/console\",\"mtime\":1700000000,\"size\":52},{\"pathname\":\"src/Main.php\",\"mtime\":1700000060,\"size\":86}]' ;;\n  *) printf '%%s' 'unexpected archive-phar input' >&2; exit 64 ;;\nesac\n"
             archive-file archive-file)))
         (set-file-modes fake-php #o700)
         (with-temp-file archive-file
           (insert
            "<?php echo 'application'; __HALT_COMPILER(); ?>PHAR-V1"))
         (unwind-protect
             (progn
               (setq archive-buffer (find-file-noselect archive-file))
               (switch-to-buffer archive-buffer)
               (goto-char (point-min))
               (search-forward "src/Main.php")
               (beginning-of-line)
               (archive-extract)
               (setq member-buffer (current-buffer))
               (list
                (with-current-buffer archive-buffer
                  (list
                   major-mode
                   mode-name
                   buffer-read-only
                   (buffer-substring-no-properties
                    (point-min) (point-max))))
                (with-current-buffer member-buffer
                  (list
                   (buffer-name)
                   (file-relative-name buffer-file-name release-dir)
                   (buffer-substring-no-properties
                    (point-min) (point-max))
                   buffer-read-only
                   (buffer-modified-p)))))
           (when (buffer-live-p member-buffer)
             (kill-buffer member-buffer))
           (when (buffer-live-p archive-buffer)
             (kill-buffer archive-buffer))))"##,
        expect![[
            r##"OK ((archive-mode "Phar-Archive" t "M Si       Date&time         Filename\n- --  --------------------  ----------------\n  52  14-Nov-2023 22:13:20  bin/console\n  86  14-Nov-2023 22:14:20  src/Main.php\n- --  --------------------  ----------------\n 138                         2 files\n") ("Main.php (application.phar)" "application.phar:src/Main.php" "<?php\nfinal class Main { public static function run(): string { return \"ready\"; } }\n" nil nil))"##
        ]],
    )
}

fn archive_phar_reverts_an_updated_release_and_opens_its_new_documentation_member()
-> ParityBatchCase {
    ParityBatchCase::value(
        "archive_phar_reverts_an_updated_release_and_opens_its_new_documentation_member",
        r##"(let* ((release-dir
               (expand-file-name "release/" temporary-file-directory))
              (archive-file
               (expand-file-name "application.phar" release-dir))
              (state-file
               (expand-file-name "published-v2" release-dir))
              (fake-php
               (expand-file-name "bin/php" temporary-file-directory))
              (archive-phar-php-executable fake-php)
              (php-runtime-php-executable fake-php)
              (archive-hidden-columns nil)
              (archive-visit-single-files nil)
              archive-buffer
              member-buffer)
         (make-directory (file-name-directory fake-php) t)
         (make-directory release-dir t)
         (with-temp-file fake-php
           (insert
            (format
             "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in\n  '%s\tdocs/release notes.txt') printf '%%s' 'Version 2\n- safer migrations\n- faster startup\n' ;;\n  '%s') if [ -f '%s' ]; then printf '%%s' '[{\"pathname\":\"bin/console\",\"mtime\":1700000000,\"size\":52},{\"pathname\":\"src/Main.php\",\"mtime\":1700000060,\"size\":86},{\"pathname\":\"docs/release notes.txt\",\"mtime\":1700000120,\"size\":46}]'; else printf '%%s' '[{\"pathname\":\"bin/console\",\"mtime\":1700000000,\"size\":52},{\"pathname\":\"src/Main.php\",\"mtime\":1700000060,\"size\":86}]'; fi ;;\n  *) printf '%%s' 'unexpected archive-phar input' >&2; exit 64 ;;\nesac\n"
             archive-file archive-file state-file)))
         (set-file-modes fake-php #o700)
         (with-temp-file archive-file
           (insert
            "<?php echo 'application'; __HALT_COMPILER(); ?>PHAR-V1"))
         (unwind-protect
             (progn
               (setq archive-buffer (find-file-noselect archive-file))
               (switch-to-buffer archive-buffer)
               (with-temp-file state-file
                 (insert "published"))
               (with-temp-file archive-file
                 (insert
                  "<?php echo 'application'; __HALT_COMPILER(); ?>PHAR-V2"))
               (revert-buffer nil t)
               (goto-char (point-min))
               (search-forward "docs/release notes.txt")
               (beginning-of-line)
               (archive-extract)
               (setq member-buffer (current-buffer))
               (list
                (with-current-buffer archive-buffer
                  (buffer-substring-no-properties
                   (point-min) (point-max)))
                (with-current-buffer member-buffer
                  (list
                   (buffer-name)
                   (file-relative-name buffer-file-name release-dir)
                   (buffer-substring-no-properties
                    (point-min) (point-max))
                   (buffer-modified-p)))))
           (when (buffer-live-p member-buffer)
             (kill-buffer member-buffer))
           (when (buffer-live-p archive-buffer)
             (kill-buffer archive-buffer))))"##,
        expect![[
            r##"OK ("M Si       Date&time         Filename\n- --  --------------------  ----------------\n  52  14-Nov-2023 22:13:20  bin/console\n  86  14-Nov-2023 22:14:20  src/Main.php\n  46  14-Nov-2023 22:15:20  docs/release notes.txt\n- --  --------------------  ----------------\n 184                         3 files\n" ("release notes.txt (application.phar)" "application.phar:docs/release notes.txt" "Version 2\n- safer migrations\n- faster startup\n" nil))"##
        ]],
    )
}

fn archive_phar_keeps_the_listing_open_when_a_member_cannot_be_read() -> ParityBatchCase {
    ParityBatchCase::value(
        "archive_phar_keeps_the_listing_open_when_a_member_cannot_be_read",
        r##"(let* ((release-dir
               (expand-file-name "release/" temporary-file-directory))
              (archive-file
               (expand-file-name "damaged.phar" release-dir))
              (call-log
               (expand-file-name "php-extraction-input" release-dir))
              (fake-php
               (expand-file-name "bin/php" temporary-file-directory))
              (archive-phar-php-executable fake-php)
              (php-runtime-php-executable fake-php)
              (archive-hidden-columns nil)
              (archive-visit-single-files nil)
              archive-buffer)
         (make-directory (file-name-directory fake-php) t)
         (make-directory release-dir t)
         (with-temp-file fake-php
           (insert
            (format
             "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in\n  *missing.php) printf '%%s' \"$input\" > '%s'; printf '%%s' 'member missing from archive' >&2; exit 7 ;;\n  *) printf '%%s' '[{\"pathname\":\"src/Present.php\",\"mtime\":1700000000,\"size\":25},{\"pathname\":\"src/missing.php\",\"mtime\":1700000060,\"size\":31}]' ;;\nesac\n"
             call-log)))
         (set-file-modes fake-php #o700)
         (with-temp-file archive-file
           (insert "<?php __HALT_COMPILER(); ?>TRUNCATED"))
         (unwind-protect
             (progn
               (setq archive-buffer (find-file-noselect archive-file))
               (switch-to-buffer archive-buffer)
               (goto-char (point-min))
               (search-forward "src/missing.php")
               (beginning-of-line)
               (archive-extract)
               (list
                (eq (window-buffer) archive-buffer)
                (buffer-live-p
                 (get-buffer "missing.php (damaged.phar)"))
                (with-temp-buffer
                  (insert-file-contents call-log)
                  (string-replace
                   archive-file "damaged.phar" (buffer-string)))
                (with-current-buffer archive-buffer
                  (list
                   major-mode
                   buffer-read-only
                   (buffer-modified-p)
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))
           (when (buffer-live-p archive-buffer)
             (kill-buffer archive-buffer))))"##,
        expect![[
            r##"OK (t nil "damaged.phar\11src/missing.php" (archive-mode t nil "M Si       Date&time         Filename\n- --  --------------------  ----------------\n  25  14-Nov-2023 22:13:20  src/Present.php\n  31  14-Nov-2023 22:14:20  src/missing.php\n- --  --------------------  ----------------\n  56                         2 files\n"))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        archive_phar_opens_a_release_and_extracts_the_selected_source_file(),
        archive_phar_reverts_an_updated_release_and_opens_its_new_documentation_member(),
        archive_phar_keeps_the_listing_open_when_a_member_cannot_be_read(),
    ]
}
