use expect_test::expect;

use super::ParityBatchCase;

/// The package in one command: point anywhere inside a `let', `M-x
/// align-cljlet', and the values line up in a column one space past the longest
/// name.  The three bindings here are 5, 1 and 7 characters wide, so every one
/// of them has to move by a different amount.  Point stays where the user left
/// it -- at the same place in the same line, though not at the same character
/// number, since spaces were inserted before it -- the mark is untouched, and
/// running the command again changes nothing.
fn aligning_a_let_lines_the_values_up_into_one_column() -> ParityBatchCase {
    ParityBatchCase::value(
        "aligning_a_let_lines_the_values_up_into_one_column",
        r##"(acl-test-with-file
 "src/compute.clj"
 "(defn compute [items]\n  (let [total (reduce + items)\n        n (count items)\n        average (/ total n)]\n    average))\n"
 "n (count"
 (push-mark (point-min) t)
 (let ((before (list :text (acl-test-text)
                     :where (acl-test-where)
                     :mark (mark t)
                     :modified (buffer-modified-p))))
   (let ((outcome (acl-test-align)))
     (let ((after (list :text (acl-test-text)
                        :where (acl-test-where)
                        :mark (mark t)
                        :modified (buffer-modified-p))))
       (acl-test-align)
       (list :before before
             :outcome outcome
             :after after
             :idempotent (equal (plist-get after :text) (acl-test-text))
             :file-on-disk (with-temp-buffer
                             (insert-file-contents (acl-test-path "src/compute.clj"))
                             (buffer-string)))))))"##,
        expect![[
            r#"OK (:before (:text "(defn compute [items]\n  (let [total (reduce + items)\n        n (count items)\n        average (/ total n)]\n    average))\n" :where (:line 3 :before-point "        n (count") :mark 1 :modified nil) :outcome aligned :after (:text "(defn compute [items]\n  (let [total   (reduce + items)\n        n       (count items)\n        average (/ total n)]\n    average))\n" :where (:line 3 :before-point "        n       (count") :mark 1 :modified t) :idempotent t :file-on-disk "(defn compute [items]\n  (let [total (reduce + items)\n        n (count items)\n        average (/ total n)]\n    average))\n")"#
        ]],
    )
}

fn map_literals_and_cond_forms_align_into_the_same_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "map_literals_and_cond_forms_align_into_the_same_shape",
        r##"(list
 :map
 (acl-test-with-file
  "src/config.clj"
  "(def config\n  {:host \"localhost\"\n   :port 8080\n   :retry-count 3})\n"
  ":port"
  (let ((before (acl-test-text)))
    (list :before before
          :outcome (acl-test-align)
          :after (acl-test-text)
          :where (acl-test-where))))
 :cond
 (acl-test-with-file
  "src/classify.clj"
  "(defn classify [n]\n  (cond\n    (< n 0) :negative\n    (zero? n) :zero\n    :else :positive))\n"
  "zero?"
  (let ((before (acl-test-text)))
    (list :before before
          :outcome (acl-test-align)
          :after (acl-test-text)
          :where (acl-test-where)))))"##,
        expect![[
            r#"OK (:map (:before "(def config\n  {:host \"localhost\"\n   :port 8080\n   :retry-count 3})\n" :outcome aligned :after "(def config\n  {:host        \"localhost\"\n   :port        8080\n   :retry-count 3})\n" :where (:line 3 :before-point "   :port")) :cond (:before "(defn classify [n]\n  (cond\n    (< n 0) :negative\n    (zero? n) :zero\n    :else :positive))\n" :outcome aligned :after "(defn classify [n]\n  (cond\n    (< n 0)   :negative\n    (zero? n) :zero\n    :else     :positive))\n" :where (:line 4 :before-point "    (zero?")))"#
        ]],
    )
}

fn defroutes_alignment_follows_the_defroute_columns_setting() -> ParityBatchCase {
    ParityBatchCase::value(
        "defroutes_alignment_follows_the_defroute_columns_setting",
        r##"(let ((source "(defroutes app-routes\n  (GET \"/\" [] home)\n  (POST \"/users\" [] create-user)\n  (GET \"/users/:id\" [] show-user))\n"))
  (list
   :default-columns defroute-columns
   :one-column
   (acl-test-with-file "src/routes.clj" source "POST"
     (list :outcome (acl-test-align)
           :after (acl-test-text)))
   :two-columns
   (progn
     (setq defroute-columns 2)
     (acl-test-with-file "src/routes-two.clj" source "POST"
       (list :columns defroute-columns
             :outcome (acl-test-align)
             :after (acl-test-text))))
   :restored
   (progn
     (setq defroute-columns 1)
     (acl-test-with-file "src/routes-again.clj" source "POST"
       (list :columns defroute-columns
             :outcome (acl-test-align)
             :after (acl-test-text))))))"##,
        expect![[
            r#"OK (:default-columns 1 :one-column (:outcome aligned :after "(defroutes app-routes\n  (GET  \"/\" [] home)\n  (POST \"/users\" [] create-user)\n  (GET  \"/users/:id\" [] show-user))\n") :two-columns (:columns 2 :outcome aligned :after "(defroutes app-routes\n  (GET  \"/\"          [] home)\n  (POST \"/users\"     [] create-user)\n  (GET  \"/users/:id\" [] show-user))\n") :restored (:columns 1 :outcome aligned :after "(defroutes app-routes\n  (GET  \"/\" [] home)\n  (POST \"/users\" [] create-user)\n  (GET  \"/users/:id\" [] show-user))\n"))"#
        ]],
    )
}

fn only_the_form_point_is_standing_in_is_aligned() -> ParityBatchCase {
    ParityBatchCase::value(
        "only_the_form_point_is_standing_in_is_aligned",
        r##"(acl-test-with-file
 "src/nested.clj"
 "(defn outer [xs]\n  (let [first-value 1\n        second 2]\n    (let [inner-a 10\n          b 20]\n      (+ inner-a b))))\n"
 "inner-a 10"
 (let ((before (acl-test-text)))
   (let ((inner-outcome (acl-test-align)))
     (let ((after-inner (acl-test-text)))
       (goto-char (point-min))
       (search-forward "second 2")
       (let ((outer-outcome (acl-test-align)))
         (list :before before
               :inner-outcome inner-outcome
               :after-inner after-inner
               :outer-outcome outer-outcome
               :after-outer (acl-test-text)))))))"##,
        expect![[
            r#"OK (:before "(defn outer [xs]\n  (let [first-value 1\n        second 2]\n    (let [inner-a 10\n          b 20]\n      (+ inner-a b))))\n" :inner-outcome aligned :after-inner "(defn outer [xs]\n  (let [first-value 1\n        second 2]\n    (let [inner-a 10\n          b       20]\n      (+ inner-a b))))\n" :outer-outcome aligned :after-outer "(defn outer [xs]\n  (let [first-value 1\n        second      2]\n    (let [inner-a 10\n          b       20]\n      (+ inner-a b))))\n")"#
        ]],
    )
}

fn an_aligned_form_a_crowded_one_and_a_wrong_position_are_all_left_alone() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_aligned_form_a_crowded_one_and_a_wrong_position_are_all_left_alone",
        r##"(list
 :already-aligned
 (acl-test-with-file
  "src/aligned.clj"
  "(defn compute [items]\n  (let [total   (reduce + items)\n        n       (count items)\n        average (/ total n)]\n    average))\n"
  "n       (count"
  (let ((before (acl-test-text)))
    (let ((outcome (acl-test-align)))
      (list :outcome outcome
            :unchanged (equal before (acl-test-text))
            :modified (buffer-modified-p)
            :text (acl-test-text)))))
 :multiple-pairs-per-line
 (acl-test-with-file
  "src/crowded.clj"
  "(defn crowded [xs]\n  (let [a 1 b 2\n        c 3]\n    c))\n"
  "c 3"
  (let ((before (acl-test-text)))
    (let ((outcome (acl-test-align)))
      (list :outcome outcome
            :unchanged (equal before (acl-test-text))
            :modified (buffer-modified-p)
            :text (acl-test-text)))))
 :not-in-a-form
 (acl-test-with-file
  "src/plain.clj"
  "(defn plain [x]\n  (+ x 1))\n"
  "+ x"
  (let ((before (acl-test-text)))
    (let ((outcome (acl-test-align)))
      (list :outcome outcome
            :unchanged (equal before (acl-test-text))
            :modified (buffer-modified-p)
            :text (acl-test-text))))))"##,
        expect![[
            r#"OK (:already-aligned (:outcome aligned :unchanged t :modified nil :text "(defn compute [items]\n  (let [total   (reduce + items)\n        n       (count items)\n        average (/ total n)]\n    average))\n") :multiple-pairs-per-line (:outcome (error "multiple pairs on one line") :unchanged t :modified nil :text "(defn crowded [xs]\n  (let [a 1 b 2\n        c 3]\n    c))\n") :not-in-a-form (:outcome (error "Not in a \"let\" form") :unchanged t :modified nil :text "(defn plain [x]\n  (+ x 1))\n"))"#
        ]],
    )
}

fn an_unclosed_binding_vector_is_reindented_without_losing_any_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_unclosed_binding_vector_is_reindented_without_losing_any_text",
        r##"(acl-test-with-file
 "src/unclosed.clj"
 "(defn broken [xs]\n  (let [a 1\n        bb 2\n    a))\n"
 "bb"
 (let ((before (acl-test-text)))
   (let ((outcome (acl-test-align)))
     (list :before before
           :outcome outcome
           :after (acl-test-text)
           :modified (buffer-modified-p)
           :same-characters (equal (acl-test-visible-characters before)
                                   (acl-test-visible-characters (acl-test-text)))))))"##,
        expect![[
            r#"OK (:before "(defn broken [xs]\n  (let [a 1\n        bb 2\n    a))\n" :outcome aligned :after "(defn broken [xs]\n  (let [a  1\n        bb 2\n        a))\n" :modified t :same-characters t)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aligning_a_let_lines_the_values_up_into_one_column(),
        map_literals_and_cond_forms_align_into_the_same_shape(),
        defroutes_alignment_follows_the_defroute_columns_setting(),
        only_the_form_point_is_standing_in_is_aligned(),
        an_aligned_form_a_crowded_one_and_a_wrong_position_are_all_left_alone(),
        an_unclosed_binding_vector_is_reindented_without_losing_any_text(),
    ]
}
