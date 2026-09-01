use super::ParityBatchCase;
use expect_test::expect;

fn add_item_registers_the_exact_file_watch_and_custom_label() -> ParityBatchCase {
    ParityBatchCase::value(
        "add_item_registers_the_exact_file_watch_and_custom_label",
        r##"(let ((id (async-status-req-id "compile"))
      calls)
  (unwind-protect
      (cl-letf (((symbol-function 'file-notify-add-watch)
                 (lambda (path flags callback)
                   (push
                    (list
                     (equal
                      path
                      (async-status--get-absolute-path-by-id id))
                     flags
                     callback)
                    calls)
                   '(watch . 17))))
        (setq async-status--shown-items nil)
        (async-status-add-item-to-bar id "Compile project")
        (let ((item (car async-status--shown-items)))
          (list
           (async-status--item-p item)
           (equal (async-status--item-msg-id item) id)
           (async-status--item-fs-watcher-id item)
           (equal
            (async-status--item-file-path item)
            (async-status--get-absolute-path-by-id id))
           (async-status--item-progress item)
           (async-status--item-label item)
           calls)))
    (setq async-status--shown-items nil)
    (async-status-clean-up id)))"##,
        expect![[
            r#"OK (t t (watch . 17) t 0 "Compile project" ((t (change) async-status--update-items)))"#
        ]],
    )
}

fn add_item_defaults_the_label_to_the_allocated_message_id() -> ParityBatchCase {
    ParityBatchCase::value(
        "add_item_defaults_the_label_to_the_allocated_message_id",
        r##"(let ((id (async-status-req-id "default-label")))
  (unwind-protect
      (cl-letf (((symbol-function 'file-notify-add-watch)
                 (lambda (&rest _arguments) :watch)))
        (setq async-status--shown-items nil)
        (async-status-add-item-to-bar id)
        (let ((item (car async-status--shown-items)))
          (list
           (equal (async-status--item-label item) id)
           (equal (async-status--item-msg-id item) id)
           (async-status--item-progress item)
           (async-status--item-fs-watcher-id item))))
    (setq async-status--shown-items nil)
    (async-status-clean-up id)))"##,
        expect!["OK (t t 0 :watch)"],
    )
}

fn newly_added_items_are_displayed_in_reverse_registration_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "newly_added_items_are_displayed_in_reverse_registration_order",
        r##"(let ((first (async-status-req-id "first"))
      (second (async-status-req-id "second"))
      (watch-id 0))
  (unwind-protect
      (cl-letf (((symbol-function 'file-notify-add-watch)
                 (lambda (&rest _arguments)
                   (setq watch-id (1+ watch-id)))))
        (setq async-status--shown-items nil)
        (async-status-add-item-to-bar first "First")
        (async-status-add-item-to-bar second "Second")
        (mapcar
         (lambda (item)
           (list
            (async-status--item-label item)
            (async-status--item-fs-watcher-id item)
            (equal
             (async-status--item-msg-id item)
             (if
                 (string= (async-status--item-label item) "First")
                 first
               second))))
         async-status--shown-items))
    (setq async-status--shown-items nil)
    (async-status-clean-up first)
    (async-status-clean-up second)))"##,
        expect!["OK ((\"Second\" 2 t) (\"First\" 1 t))"],
    )
}

fn find_item_skips_non_items_and_returns_the_first_matching_duplicate() -> ParityBatchCase {
    ParityBatchCase::value(
        "find_item_skips_non_items_and_returns_the_first_matching_duplicate",
        r##"(let* ((first
        (make-async-status--item
         :msg-id "same"
         :label "first"))
       (second
        (make-async-status--item
         :msg-id "same"
         :label "second"))
       (other
        (make-async-status--item
         :msg-id "other"
         :label "other")))
  (setq async-status--shown-items
        (list nil '(not an item) first other second))
  (prog1
      (list
       (eq
        (async-status--find-item-by-msgid "same")
        first)
       (eq
        (async-status--find-item-by-msgid "other")
        other)
       (async-status--find-item-by-msgid "missing"))
    (setq async-status--shown-items nil)))"##,
        expect!["OK (t t nil)"],
    )
}

fn remove_item_uses_identity_and_removes_every_occurrence_of_that_object() -> ParityBatchCase {
    ParityBatchCase::value(
        "remove_item_uses_identity_and_removes_every_occurrence_of_that_object",
        r##"(let* ((target
        (make-async-status--item
         :msg-id "same"
         :label "target"))
       (equal-but-distinct
        (copy-async-status--item target))
       (other
        (make-async-status--item
         :msg-id "other")))
  (setq async-status--shown-items
        (list target equal-but-distinct target other target))
  (async-status--remove-item target)
  (prog1
      (list
       (length async-status--shown-items)
       (eq (car async-status--shown-items)
           equal-but-distinct)
       (mapcar #'async-status--item-msg-id
               async-status--shown-items))
    (setq async-status--shown-items nil)))"##,
        expect!["OK (2 t (\"same\" \"other\"))"],
    )
}

fn remove_from_bar_unregisters_the_watch_removes_item_and_refreshes_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "remove_from_bar_unregisters_the_watch_removes_item_and_refreshes_once",
        r##"(let* ((target
        (make-async-status--item
         :msg-id "target"
         :fs-watcher-id '(watch . 9)))
       (other
        (make-async-status--item
         :msg-id "other"
         :fs-watcher-id '(watch . 10)))
       calls)
  (setq async-status--shown-items
        (list target other))
  (cl-letf (((symbol-function 'file-notify-rm-watch)
             (lambda (watch)
               (push (list :remove watch) calls)))
            ((symbol-function 'async-status--refresh-status-bar)
             (lambda ()
               (push :refresh calls))))
    (async-status-remove-item-from-bar "target"))
  (prog1
      (list
       (nreverse calls)
       (mapcar #'async-status--item-msg-id
               async-status--shown-items))
    (setq async-status--shown-items nil)))"##,
        expect!["OK (((:remove (watch . 9)) :refresh) (\"other\"))"],
    )
}

fn removing_an_unknown_id_has_no_watch_refresh_or_state_side_effect() -> ParityBatchCase {
    ParityBatchCase::value(
        "removing_an_unknown_id_has_no_watch_refresh_or_state_side_effect",
        r##"(let ((item
       (make-async-status--item
        :msg-id "kept"
        :fs-watcher-id :watch))
      calls)
  (setq async-status--shown-items (list item))
  (cl-letf (((symbol-function 'file-notify-rm-watch)
             (lambda (&rest arguments)
               (push (cons :remove arguments) calls)))
            ((symbol-function 'async-status--refresh-status-bar)
             (lambda ()
               (push :refresh calls))))
    (async-status-remove-item-from-bar "missing"))
  (prog1
      (list calls
            (eq (car async-status--shown-items) item))
    (setq async-status--shown-items nil)))"##,
        expect!["OK (nil t)"],
    )
}

fn file_event_updates_only_the_matching_item_then_refreshes_and_shows() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_event_updates_only_the_matching_item_then_refreshes_and_shows",
        r##"(let ((first-id (async-status-req-id "first"))
      (second-id (async-status-req-id "second"))
      calls)
  (unwind-protect
      (let ((first
             (make-async-status--item
              :msg-id first-id
              :progress 0
              :label "First"))
            (second
             (make-async-status--item
              :msg-id second-id
              :progress 0
              :label "Second")))
        (setq async-status--shown-items
              (list first second))
        (async-status-set-msg-val second-id 0.625)
        (cl-letf (((symbol-function 'async-status--refresh-status-bar)
                   (lambda () (push :refresh calls)))
                  ((symbol-function 'async-status-show)
                   (lambda () (push :show calls))))
          (async-status--update-items
           (list :watch :changed
                 (async-status--get-absolute-path-by-id second-id))))
        (list
         (async-status--item-progress first)
         (async-status--item-progress second)
         (nreverse calls)))
    (setq async-status--shown-items nil)
    (async-status-clean-up first-id)
    (async-status-clean-up second-id)))"##,
        expect!["OK (0 \"0.625\" (:refresh :show))"],
    )
}

fn file_event_for_an_unknown_message_surfaces_the_struct_type_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_event_for_an_unknown_message_surfaces_the_struct_type_error",
        r##"(let ((id (async-status-req-id "unknown-event")))
  (unwind-protect
      (progn
        (setq async-status--shown-items nil)
        (let ((outcome
               (async-status-test-error
                (lambda ()
                  (async-status--update-items
                   (list :watch :changed
                         (async-status--get-absolute-path-by-id id)))))))
          (list
           (car outcome)
           (cadr outcome)
           (and
            (string-match-p
             "async-status--item"
             (format "%S" outcome))
            t))))
    (setq async-status--shown-items nil)
    (async-status-clean-up id)))"##,
        expect!["OK (:error wrong-type-argument t)"],
    )
}

pub(super) fn items_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        add_item_registers_the_exact_file_watch_and_custom_label(),
        add_item_defaults_the_label_to_the_allocated_message_id(),
        newly_added_items_are_displayed_in_reverse_registration_order(),
        find_item_skips_non_items_and_returns_the_first_matching_duplicate(),
        remove_item_uses_identity_and_removes_every_occurrence_of_that_object(),
        remove_from_bar_unregisters_the_watch_removes_item_and_refreshes_once(),
        removing_an_unknown_id_has_no_watch_refresh_or_state_side_effect(),
        file_event_updates_only_the_matching_item_then_refreshes_and_shows(),
        file_event_for_an_unknown_message_surfaces_the_struct_type_error(),
    ]
}
