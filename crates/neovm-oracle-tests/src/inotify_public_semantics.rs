//! Oracle parity tests for the public GNU inotify primitives.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

crate::common::platform_twin_test!(
    TwinHost::Linux,
    oracle_inotify_public_lifecycle_and_error_semantics,
    {
        return_if_neovm_enable_oracle_proptest_not_set!();

        let form = r#"
(let ((missing "/tmp/neomacs-oracle-inotify-missing-path"))
  (list
   (featurep 'inotify)
   (fboundp 'inotify-add-watch)
   (fboundp 'inotify-rm-watch)
   (fboundp 'inotify-valid-p)
   (inotify-valid-p 0)
   (condition-case err
       (inotify-rm-watch 0)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (inotify-add-watch 0 nil nil)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (inotify-add-watch missing t #'ignore)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (inotify-add-watch "/tmp" 'neomacs-unknown-aspect #'ignore)
     (error (cons (car err) (cdr err))))
   ;; These are event-report symbols, not add-watch aspect symbols.
   (condition-case err
       (inotify-add-watch "/tmp" 'isdir #'ignore)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (inotify-add-watch "/tmp" 'q-overflow #'ignore)
     (error (cons (car err) (cdr err))))
   (mapcar (lambda (descriptor)
             (condition-case err
                 (inotify-rm-watch descriptor)
               (error (cons (car err) (cdr err)))))
           '((-1 . 0) (0 . -1) (0 . "id") ("wd" . 0) (0 0)))
   (let ((w (inotify-add-watch "/tmp" '(modify) #'ignore)))
     (list (consp w)
           (integerp (car w))
           (integerp (cdr w))
           (inotify-valid-p w)
           (inotify-rm-watch w)
           (inotify-valid-p w)
           ;; GNU validates descriptor shape but treats removing an already
           ;; inactive valid-shaped descriptor as a successful no-op.
           (inotify-rm-watch w)
           (inotify-rm-watch (cons 0 0))))))
"#;

        let expect = expect_test::expect![[
            r#""OK (t t t t nil (file-notify-error \"Invalid descriptor \" \"No such file or directory\" 0) (wrong-type-argument stringp 0) (file-notify-error \"Could not add watch for file\" \"No such file or directory\" \"/tmp/neomacs-oracle-inotify-missing-path\") (file-notify-error \"Unknown aspect\" \"Invalid argument\" neomacs-unknown-aspect) (file-notify-error \"Unknown aspect\" \"Invalid argument\" isdir) (file-notify-error \"Unknown aspect\" \"Invalid argument\" q-overflow) ((file-notify-error \"Invalid descriptor \" \"Invalid argument\" -1 . 0) (file-notify-error \"Invalid descriptor \" \"Invalid argument\" 0 . -1) (file-notify-error \"Invalid descriptor \" \"Invalid argument\" 0 . \"id\") (file-notify-error \"Invalid descriptor \" \"Invalid argument\" \"wd\" . 0) (file-notify-error \"Invalid descriptor \" \"Invalid argument\" 0 0)) (t t t t t nil t t))""#
        ]];
        crate::common::assert_oracle_parity_expect(form, expect);
    }
);

// The macos-batch reference uses kqueue, not inotify: the feature and all
// four subrs are absent, and calling one is void-function.  Neo's mac build
// matches (measured live); the Linux lifecycle test is cfg(linux).
crate::common::platform_twin_test!(TwinHost::Macos, oracle_inotify_absent_on_macos, {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (featurep 'inotify)
      (fboundp 'inotify-add-watch)
      (fboundp 'inotify-rm-watch)
      (fboundp 'inotify-valid-p)
      (condition-case err
          (inotify-valid-p 0)
        (error (list (car err) (cdr err))))
      (condition-case err
          (inotify-add-watch "/tmp" '(modify) #'ignore)
        (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil (void-function (inotify-valid-p)) (void-function (inotify-add-watch)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
});
