use expect_test::expect;

use super::ParityBatchCase;

fn read_str_parses_scalars_vector_and_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "read_str_parses_scalars_vector_and_map",
        r####"
(let* ((vec (parseedn-read-str "[1 2 :a]"))
       (mp (parseedn-read-str "{:x 10 :y \"hi\"}"))
       (keys (sort (mapcar #'symbol-name (map-keys mp)) #'string-lessp)))
  (list :vector (append vec nil)
        :map-keys keys
        :map-x (map-elt mp :x)
        :map-y (map-elt mp :y)
        :nil (parseedn-read-str "nil")
        :true (parseedn-read-str "true")
        :kw (parseedn-read-str ":foo/bar")))
"####,
        expect![[
            r#"OK (:vector (1 2 :a) :map-keys (":x" ":y") :map-x 10 :map-y "hi" :nil nil :true t :kw :foo/bar)"#
        ]],
    )
}

fn print_str_round_trips_common_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "print_str_round_trips_common_values",
        r####"
(list :num (parseedn-print-str 42)
      :str (parseedn-print-str "hello")
      :kw (parseedn-print-str :alpha)
      :vec (parseedn-print-str [1 2 3])
      :list (parseedn-print-str '(a b))
      :nil (parseedn-print-str nil)
      :true (parseedn-print-str t))
"####,
        expect![[
            r#"OK (:num "42" :str "\"hello\"" :kw ":alpha" :vec "[1 2 3]" :list "(a b)" :nil "nil" :true "true")"#
        ]],
    )
}

fn tagged_literal_reader_is_applied() -> ParityBatchCase {
    ParityBatchCase::value(
        "tagged_literal_reader_is_applied",
        r####"
(let* ((readers `((my/tag . ,(lambda (form) (list 'tagged form)))))
       (value (parseedn-read-str "#my/tag [1 2]" readers)))
  (list :value value
        :car (car value)
        :inner (append (cadr value) nil)))
"####,
        expect!["OK (:value (tagged [1 2]) :car tagged :inner (1 2))"],
    )
}

fn alist_and_plist_predicates_classify_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "alist_and_plist_predicates_classify_shapes",
        r####"
(list :alist-yes (and (parseedn-alist-p '((a . 1) (b . 2))) t)
      :alist-no (parseedn-alist-p '(1 2 3))
      :plist-yes (and (parseedn-plist-p '(:a 1 :b 2)) t)
      :plist-no (parseedn-plist-p '(a 1 b 2))
      :print-plist (parseedn-print-str '(:k "v" :n 3)))
"####,
        expect![[
            r#"OK (:alist-yes t :alist-no nil :plist-yes t :plist-no nil :print-plist "{:k \"v\", :n 3}")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        read_str_parses_scalars_vector_and_map(),
        print_str_round_trips_common_values(),
        tagged_literal_reader_is_applied(),
        alist_and_plist_predicates_classify_shapes(),
    ]
}
