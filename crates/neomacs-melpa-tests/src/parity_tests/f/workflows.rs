use expect_test::expect;

use super::ParityBatchCase;

fn walking_a_real_project_tree_and_selecting_the_files_that_matter() -> ParityBatchCase {
    ParityBatchCase::value(
        "walking_a_real_project_tree_and_selecting_the_files_that_matter",
        r##"
(let ((root (f-test-build)))
  (list
   :tree (f-test-tree root)
   :top-level (f-test-relative root (f-entries root))
   ;; Recursive listings reach into the dot directory as readily as any
   ;; other; f does no filtering of its own.
   :every-directory (f-test-relative root (f-directories root nil t))
   :every-file (f-test-relative root (f-files root nil t))
   ;; A predicate selects; note it is applied to the full path, so an
   ;; extension test is the natural way to write it.
   :elisp-only
   (f-test-relative root (f-files root (lambda (path) (f-ext-p path "el")) t))
   :markdown-only
   (f-test-relative root (f-files root (lambda (path) (f-ext-p path "md")) t))
   ;; Globs are relative to `default-directory', not to a path argument.
   :globbed
   (let ((default-directory (f-slash root)))
     (f-test-relative root (f-glob "src/*/*.el")))
   :hidden (f-test-relative root (f-files root #'f-hidden-p t))
   ;; What each listing function counts, side by side.
   :counts (list :entries (length (f-entries root nil t))
                 :files (length (f-files root nil t))
                 :directories (length (f-directories root nil t)))))
"##,
        expect![[
            r#"OK (:tree ((".git" . directory) (".git/config" . file) (".hidden" . file) ("README.md" . file) ("docs" . directory) ("docs/guide.md" . file) ("src" . directory) ("src/core" . directory) ("src/core/engine-test.el" . file) ("src/core/engine.el" . file) ("src/util" . directory) ("src/util/strings.el" . file)) :top-level (".git" ".hidden" "README.md" "docs" "src") :every-directory (".git" "docs" "src" "src/core" "src/util") :every-file (".git/config" ".hidden" "README.md" "docs/guide.md" "src/core/engine-test.el" "src/core/engine.el" "src/util/strings.el") :elisp-only ("src/core/engine-test.el" "src/core/engine.el" "src/util/strings.el") :markdown-only ("README.md" "docs/guide.md") :globbed ("src/core/engine-test.el" "src/core/engine.el" "src/util/strings.el") :hidden nil :counts (:entries 12 :files 7 :directories 5))"#
        ]],
    )
}

fn taking_paths_apart_and_putting_them_back_together() -> ParityBatchCase {
    ParityBatchCase::value(
        "taking_paths_apart_and_putting_them_back_together",
        r##"
(list
 :join (list (f-join "a" "b" "c.el")
             ;; An absolute component discards everything before it.
             (f-join "a" "/b" "c.el")
             (f-join "a/" "/b/" "c.el")
             (f-join "a"))
 :split (list (f-split "/a/b/c.el") (f-split "a/b/c.el")
              (f-split "/") (f-split "a"))
 ;; Only the last extension counts, so a doubled one keeps its first half.
 :extensions (list (f-base "/a/b/c.tar.gz") (f-ext "/a/b/c.tar.gz")
                   (f-no-ext "/a/b/c.tar.gz") (f-swap-ext "/a/b/c.el" "elc")
                   (f-ext "/a/b/noext") (f-base "/a/b/noext")
                   (f-ext "/a/b/.hidden"))
 ;; A trailing slash changes which component is the file name.
 :names (list (f-filename "/a/b") (f-dirname "/a/b")
              (f-filename "/a/b/") (f-dirname "/a/b/")
              (f-filename "/") (f-dirname "/"))
 :relative (list (f-relative "/a/b/c" "/a") (f-relative "/a/b/c" "/a/b/c")
                 (f-relative "/a/b" "/a/x"))
 :depth (list (f-depth "/") (f-depth "/a") (f-depth "/a/b/c"))
 :common-parent (list (f-common-parent '("/a/b/c/d" "/a/b/e/f" "/a/b/g"))
                      (f-common-parent '("/a/b" "/a/b"))
                      (f-common-parent '("/a" "/b")))
 ;; Uniquify keeps only as much of each path as it takes to tell them apart.
 ;; `f-uniquify' documents that it "expects no duplicate paths".  It is
 ;; not merely undefined on them: `f--uniquify' loops until the group
 ;; count reaches the input count, which two identical paths never do, so
 ;; the call does not return.  Nothing below violates the precondition.
 :uniquify (list (f-uniquify '("/a/b/c.el" "/a/d/c.el" "/x/y/z.el"))
                 (f-uniquify '("/deep/a/b/f.el" "/deep/c/b/f.el" "/other/f.el"))
                 (f-uniquify-alist '("/one/two/f.el" "/one/three/f.el")))
 :shape (list (f-absolute-p "/a") (f-relative-p "a")
              (f-root-p "/") (f-root-p "/a")
              (f-slash "/a/b") (f-slash "/a/b/")))
"##,
        expect![[
            r#"OK (:join ("a/b/c.el" "/b/c.el" "/b/c.el" "a") :split (("/" "a" "b" "c.el") ("a" "b" "c.el") ("/") ("a")) :extensions ("c.tar" "gz" "/a/b/c.tar" "/a/b/c.elc" nil "noext" nil) :names ("b" "/a" "b" "/a" "" nil) :relative ("b/c" "." "../b") :depth (0 1 3) :common-parent ("/a/b/" "/a/b/" "/") :uniquify (("b/c.el" "d/c.el" "z.el") ("a/b/f.el" "c/b/f.el" "other/f.el") (("/one/two/f.el" . "two/f.el") ("/one/three/f.el" . "three/f.el"))) :shape (t t t nil "/a/b" "/a/b/"))"#
        ]],
    )
}

fn writing_reading_and_appending_text_and_bytes_round_trip() -> ParityBatchCase {
    ParityBatchCase::value(
        "writing_reading_and_appending_text_and_bytes_round_trip",
        r##"
(let* ((root (f-test-build))
       (notes (f-join root "notes.txt"))
       (blob (f-join root "blob.bin")))
  (f-write-text "first line\n" 'utf-8 notes)
  (let ((after-write (f-read-text notes 'utf-8)))
    (f-append-text "second line\n" 'utf-8 notes)
    (let ((after-append (f-read-text notes 'utf-8)))
      ;; Bytes go in and come out untouched, including a NUL and a byte no
      ;; text encoding would leave alone.
      (f-write-bytes (unibyte-string 0 65 200 10 255) blob)
      (let ((round-tripped (f-read-bytes blob)))
        (list :after-write after-write
              :after-append after-append
              :size-on-disk (f-size notes)
              :bytes (append round-tripped nil)
              :bytes-are-unibyte (not (multibyte-string-p round-tripped))
              :blob-size (f-size blob)
              ;; Appending bytes extends rather than replaces.
              :after-append-bytes
              (progn (f-append-bytes (unibyte-string 1 2) blob)
                     (append (f-read-bytes blob) nil))
              ;; Writing again replaces the whole file.
              :after-rewrite
              (progn (f-write-text "replaced\n" 'utf-8 notes)
                     (list (f-read-text notes 'utf-8) (f-size notes)))
              ;; A plain ASCII literal is already unibyte, so it cannot show
              ;; the predicate distinguishing anything; a string with a
              ;; non-ASCII character in it can.
              :unibyte-check
              (list (f-unibyte-string-p (unibyte-string 200))
                    (f-unibyte-string-p "ascii")
                    (f-unibyte-string-p (string ?\N{LATIN SMALL LETTER E WITH ACUTE}))
                    (condition-case error
                        (f-write-bytes (string ?\N{LATIN SMALL LETTER E WITH ACUTE}) blob)
                      (error (f-test-plain error)))))))))
"##,
        expect![[
            r#"OK (:after-write "first line\n" :after-append "first line\nsecond line\n" :size-on-disk 23 :bytes (0 65 200 10 255) :bytes-are-unibyte t :blob-size 5 :after-append-bytes (0 65 200 10 255 1 2) :after-rewrite ("replaced\n" 9) :unibyte-check (t t nil (wrong-type-argument f-unibyte-string-p "é")))"#
        ]],
    )
}

fn copying_moving_and_deleting_a_subtree() -> ParityBatchCase {
    ParityBatchCase::value(
        "copying_moving_and_deleting_a_subtree",
        r##"
(let* ((root (f-test-build))
       (src (f-join root "src"))
       (fresh (f-join root "fresh-target"))
       (existing (f-join root "existing-target"))
       (contents (f-join root "contents-target")))
  ;; The three destinations differ in exactly one way each, so the two copy
  ;; functions cannot be mistaken for one another:
  ;;   `f-copy' to a name that does not exist  -> that name becomes the copy
  ;;   `f-copy' to a name that already exists  -> refuses, even when empty
  ;;   `f-copy-contents' into a directory      -> only what is in it moves
  (f-copy src fresh)
  (f-mkdir existing)
  (f-mkdir contents)
  (f-copy-contents src contents)
  (let ((to-fresh (f-test-relative root (f-entries fresh nil t)))
        (to-existing (condition-case error (progn (f-copy src existing) :copied)
                       (error (f-test-plain error))))
        (to-contents (f-test-relative root (f-entries contents nil t))))
    (f-move (f-join root "README.md") (f-join root "docs/README.md"))
    (let ((after-move (list :gone (f-exists-p (f-join root "README.md"))
                            :arrived (f-exists-p (f-join root "docs/README.md"))
                            :text (f-read-text (f-join root "docs/README.md")
                                               'utf-8))))
      (f-delete fresh t)
      (f-touch (f-join root "fresh.txt"))
      (list :copy-to-a-new-name to-fresh
            :copy-onto-an-existing-name to-existing
            :copy-contents-only to-contents
            :after-move after-move
            :deleted-target-still-exists (f-exists-p fresh)
            ;; A non-recursive delete of a populated directory refuses.
            :non-recursive-delete
            (condition-case error (f-delete contents)
              (error (f-test-plain error)))
            :touched (list (f-exists-p (f-join root "fresh.txt"))
                           (f-size (f-join root "fresh.txt")))
            :final (f-test-relative root (f-entries root))))))
"##,
        expect![[
            r##"OK (:copy-to-a-new-name ("fresh-target/core" "fresh-target/core/engine-test.el" "fresh-target/core/engine.el" "fresh-target/util" "fresh-target/util/strings.el") :copy-onto-an-existing-name (file-already-exists "File exists" "[ORACLE-SANDBOX]/tree/existing-target") :copy-contents-only ("contents-target/core" "contents-target/core/engine-test.el" "contents-target/core/engine.el" "contents-target/util" "contents-target/util/strings.el") :after-move (:gone nil :arrived t :text "# Project\n") :deleted-target-still-exists nil :non-recursive-delete (file-error "Removing directory" "Directory not empty" "[ORACLE-SANDBOX]/tree/contents-target") :touched (t 0) :final (".git" ".hidden" "contents-target" "docs" "existing-target" "fresh.txt" "src"))"##
        ]],
    )
}

fn the_predicates_that_compare_two_paths_and_two_timestamps() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_predicates_that_compare_two_paths_and_two_timestamps",
        r##"
(let* ((root (f-test-build))
       (older (f-join root "older.txt"))
       (newer (f-join root "newer.txt")))
  (f-touch older)
  (f-write-text "wait\n" 'utf-8 newer)
  ;; Set the times explicitly rather than racing the clock: a fixed hour
  ;; apart is stable where `sleep-for' is not.
  (set-file-times older (seconds-to-time 1700000000))
  (set-file-times newer (seconds-to-time 1700003600))
  (list
   :relationships
   (list :ancestor (f-ancestor-of-p "/a" "/a/b/c")
         :not-its-own-ancestor (f-ancestor-of-p "/a" "/a")
         :descendant (f-descendant-of-p "/a/b/c" "/a")
         :parent (f-parent-of-p "/a" "/a/b")
         :child (f-child-of-p "/a/b" "/a")
         :grandchild-is-not-a-child (f-child-of-p "/a/b/c" "/a")
         :same (f-same-p "/a/b" "/a/b/")
         :same-through-dots (f-same-p "/a/b" "/a/x/../b"))
   :kinds
   (list :hidden (f-hidden-p ".hidden")
         :not-hidden (f-hidden-p "README.md")
         :empty-directory (progn (f-mkdir (f-join root "void"))
                                 (f-empty-p (f-join root "void")))
         :empty-file (progn (f-touch (f-join root "void.txt"))
                            (f-empty-p (f-join root "void.txt")))
         :populated (f-empty-p (f-join root "docs"))
         :ext-p (list (f-ext-p "a/b.el" "el") (f-ext-p "a/b.el" "elc")
                      (f-ext-p "a/b.el")))
   :times
   (list :older (f-older-p older newer)
         :newer (f-newer-p newer older)
         :not-older-than-itself (f-older-p older older)
         :same-time (f-same-time-p older older)
         ;; An hour apart, asserted as the delta rather than the clock.
         :seconds-apart
         (round (- (float-time (f-modification-time newer))
                   (float-time (f-modification-time older)))))))
"##,
        expect![
            "OK (:relationships (:ancestor t :not-its-own-ancestor nil :descendant t :parent t :child t :grandchild-is-not-a-child nil :same t :same-through-dots t) :kinds (:hidden t :not-hidden nil :empty-directory t :empty-file t :populated nil :ext-p (t nil t)) :times (:older t :newer t :not-older-than-itself nil :same-time t :seconds-apart 3600))"
        ],
    )
}

fn the_sandbox_guard_refuses_every_destructive_call_that_would_escape() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_sandbox_guard_refuses_every_destructive_call_that_would_escape",
        r##"
(let* ((root (f-test-build))
       (allowed (f-join root "allowed"))
       (outside (f-join root "outside.txt")))
  (f-mkdir allowed)
  (list
   ;; Inside the sandbox the operation happens normally.
   :inside
   (progn (f-with-sandbox allowed
            (f-write-text "kept\n" 'utf-8 (f-join allowed "kept.txt")))
          (list (f-exists-p (f-join allowed "kept.txt"))
                (f-read-text (f-join allowed "kept.txt") 'utf-8)))
   ;; Outside it, the call signals and the file is never created.
   :outside
   (list :signalled (condition-case error
                        (f-with-sandbox allowed
                          (f-write-text "escaped\n" 'utf-8 outside))
                      (error (f-test-plain error)))
         :file-created (f-exists-p outside))
   ;; Deleting outside the sandbox is refused the same way.
   :deleting-outside
   (list :signalled (condition-case error
                        (f-with-sandbox allowed
                          (f-delete (f-join root "docs") t))
                      (error (f-test-plain error)))
         :docs-still-there (f-exists-p (f-join root "docs")))
   ;; Reading is not a destructive operation and is not guarded.
   :reading-outside
   (f-with-sandbox allowed (f-read-text (f-join root "docs/guide.md") 'utf-8))
   ;; Several directories can be allowed at once.
   :two-sandboxes
   (progn (f-with-sandbox (list allowed (f-join root "docs"))
            (f-write-text "ok\n" 'utf-8 (f-join root "docs/extra.txt")))
          (f-exists-p (f-join root "docs/extra.txt")))))
"##,
        expect![[
            r#"OK (:inside (t "kept\n") :outside (:signalled (f-guard-error "[ORACLE-SANDBOX]/tree/outside.txt" ("[ORACLE-SANDBOX]/tree/allowed")) :file-created nil) :deleting-outside (:signalled (f-guard-error "[ORACLE-SANDBOX]/tree/docs" ("[ORACLE-SANDBOX]/tree/allowed")) :docs-still-there t) :reading-outside "guide\n" :two-sandboxes t)"#
        ]],
    )
}

fn climbing_out_of_a_nested_directory_to_find_the_project_root() -> ParityBatchCase {
    ParityBatchCase::value(
        "climbing_out_of_a_nested_directory_to_find_the_project_root",
        r##"
(let* ((root (f-test-build))
       (deep (f-join root "src/core")))
  (f-touch (f-join root ".projectroot"))
  (list
   ;; The classic use: walk up until a marker file appears.
   :found
   (f-relative (f-traverse-upwards
                (lambda (path) (f-exists-p (f-join path ".projectroot")))
                deep)
               root)
   ;; The search includes the directory it starts from.
   :starts-where-it-is
   (f-relative (f-traverse-upwards
                (lambda (path) (f-exists-p (f-join path "engine.el")))
                deep)
               root)
   ;; A marker that is nowhere above returns nil rather than climbing for
   ;; ever, and stops at the filesystem root.
   :never-found
   (f-traverse-upwards
    (lambda (path) (f-exists-p (f-join path ".nothing-is-here"))) deep)
   :root (f-root)
   ;; The root has no parent at all, which is what stops the climb.
   :root-has-no-parent (f-dirname (f-root))))
"##,
        expect![[
            r#"OK (:found "." :starts-where-it-is "src/core" :never-found nil :root "/" :root-has-no-parent nil)"#
        ]],
    )
}

pub(super) fn f_practical_workflows_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        walking_a_real_project_tree_and_selecting_the_files_that_matter(),
        taking_paths_apart_and_putting_them_back_together(),
        writing_reading_and_appending_text_and_bytes_round_trip(),
        copying_moving_and_deleting_a_subtree(),
        the_predicates_that_compare_two_paths_and_two_timestamps(),
        the_sandbox_guard_refuses_every_destructive_call_that_would_escape(),
        climbing_out_of_a_nested_directory_to_find_the_project_root(),
    ]
}
