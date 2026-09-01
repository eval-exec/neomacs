use expect_test::expect;

use super::ParityBatchCase;

fn summarising_an_order_book_by_region_through_one_threaded_pipeline() -> ParityBatchCase {
    ParityBatchCase::value(
        "summarising_an_order_book_by_region_through_one_threaded_pipeline",
        r##"
(dash-test-on-fresh
 ;; What a report actually looks like: group the orders, total each group,
 ;; drop the ones below a threshold, and rank what is left.
 (->> orders
      (--group-by (plist-get it :region))
      (--map (let ((region (car it))
                   (rows (cdr it)))
               (list :region region
                     :orders (length rows)
                     :cents (-sum (--map (plist-get it :cents) rows))
                     :customers (-uniq (--map (plist-get it :customer) rows)))))
      (--remove (< (plist-get it :cents) 1000))
      (--sort (> (plist-get it :cents) (plist-get other :cents)))))
"##,
        expect![[
            r#"OK (:result ((:region east :orders 2 :cents 12900 :customers ("Katherine" "Ada")) (:region north :orders 2 :cents 6075 :customers ("Ada")) (:region south :orders 2 :cents 5150 :customers ("Grace"))) :source-unchanged t :source ((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen")) (:id 1042 :customer "Grace" :region south :cents 950 :items ("pen")) (:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk")) (:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug")) (:id 1045 :customer "Grace" :region south :cents 4200 :items nil) (:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen"))))"#
        ]],
    )
}

fn destructuring_each_record_with_let_and_lambda_patterns() -> ParityBatchCase {
    ParityBatchCase::value(
        "destructuring_each_record_with_let_and_lambda_patterns",
        r##"
(dash-test-on-fresh
 (list
  ;; `-let' with a plist pattern reads the fields out by keyword.
  :one-record
  (-let (((&plist :id id :customer who :items items) (car orders)))
    (list id who (length items)))
  ;; `-lambda' destructures each element as it maps.
  :every-line
  (-map (-lambda ((&plist :id id :customer who :cents cents))
          (format "%d %s %.2f" id who (/ cents 100.0)))
        orders)
  ;; Nested patterns, a list pattern with a rest, and `&as' keeping the
  ;; whole value alongside its parts.
  :nested
  (-let* (((first second . rest) orders)
          ((&plist :items (top . others)) (car (last orders)))
          ((whole &as &plist :region region) second))
    (list :first-id (plist-get first :id)
          :second-region region
          :second-is-the-whole (equal whole second)
          :rest-ids (--map (plist-get it :id) rest)
          :last-items (cons top others)))
  ;; A pattern that does not match returns nil for the missing parts
  ;; rather than signalling.
  :absent-key
  (-let (((&plist :discount discount :cents cents) (car orders)))
    (list discount cents))))
"##,
        expect![[
            r#"OK (:result (:one-record (1041 "Ada" 2) :every-line ("1041 Ada 42.00" "1042 Grace 9.50" "1043 Ada 18.75" "1044 Katherine 123.00" "1045 Grace 42.00" "1046 Ada 6.00") :nested (:first-id 1041 :second-region south :second-is-the-whole t :rest-ids (1043 1044 1045 1046) :last-items ("pen" "pen")) :absent-key (nil 4200)) :source-unchanged t :source ((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen")) (:id 1042 :customer "Grace" :region south :cents 950 :items ("pen")) (:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk")) (:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug")) (:id 1045 :customer "Grace" :region south :cents 4200 :items nil) (:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen"))))"#
        ]],
    )
}

fn splitting_a_run_of_orders_into_windows_and_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "splitting_a_run_of_orders_into_windows_and_runs",
        r##"
(dash-test-on-fresh
 (let ((cents (--map (plist-get it :cents) orders)))
   (list
    :in-pairs (-partition 2 cents)
    ;; `-partition' drops a trailing short group; `-partition-all' keeps it.
    :threes (-partition 3 cents)
    :fours (-partition 4 cents)
    :fours-all (-partition-all 4 cents)
    :sliding (-partition-in-steps 3 1 cents)
    ;; Group consecutive orders by a property rather than by value.
    :runs-by-region (--partition-by (plist-get it :region) orders)
    :split-on-a-big-one (--split-when (> (plist-get it :cents) 10000) orders)
    ;; Pair each order with the next one.
    :consecutive (-zip-pair (-butlast cents) (cdr cents))
    :running (-running-sum cents))))
"##,
        expect![[
            r#"OK (:result (:in-pairs ((4200 950) (1875 12300) (4200 600)) :threes ((4200 950 1875) (12300 4200 600)) :fours ((4200 950 1875 12300)) :fours-all ((4200 950 1875 12300) (4200 600)) :sliding ((4200 950 1875) (950 1875 12300) (1875 12300 4200) (12300 4200 600)) :runs-by-region (((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen"))) ((:id 1042 :customer "Grace" :region south :cents 950 :items ("pen"))) ((:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk"))) ((:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug"))) ((:id 1045 :customer "Grace" :region south :cents 4200 :items nil)) ((:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen")))) :split-on-a-big-one (((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen")) (:id 1042 :customer "Grace" :region south :cents 950 :items ("pen")) (:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk"))) ((:id 1045 :customer "Grace" :region south :cents 4200 :items nil) (:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen")))) :consecutive ((4200 . 950) (950 . 1875) (1875 . 12300) (12300 . 4200) (4200 . 600)) :running (4200 5150 7025 19325 23525 24125)) :source-unchanged t :source ((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen")) (:id 1042 :customer "Grace" :region south :cents 950 :items ("pen")) (:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk")) (:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug")) (:id 1045 :customer "Grace" :region south :cents 4200 :items nil) (:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen"))))"#
        ]],
    )
}

fn folding_over_the_book_to_build_a_ledger_and_pick_extremes() -> ParityBatchCase {
    ParityBatchCase::value(
        "folding_over_the_book_to_build_a_ledger_and_pick_extremes",
        r##"
(dash-test-on-fresh
 (list
  ;; Build up an alist of per-customer totals in one pass.
  :ledger
  (-reduce-from (lambda (acc order)
                  (let* ((who (plist-get order :customer))
                         (cell (assoc who acc)))
                    (if cell
                        (progn (setcdr cell (+ (cdr cell)
                                               (plist-get order :cents)))
                               acc)
                      (cons (cons who (plist-get order :cents)) acc))))
                nil orders)
  ;; Every intermediate state of a running total, not just the last.
  :reductions (-reductions-from #'+ 0 (--map (plist-get it :cents) orders))
  ;; The same fold from the right, which visits the book in the other order.
  :from-the-right
  (-reduce-r-from (lambda (order acc) (cons (plist-get order :id) acc))
                  nil orders)
  ;; Extremes by a computed key, and what happens when two tie: 4200 appears
  ;; twice, so this pins which of the two the library returns.
  :largest (plist-get (--max-by (> (plist-get it :cents)
                                   (plist-get other :cents))
                                orders)
                      :id)
  :smallest (plist-get (--min-by (> (plist-get it :cents)
                                    (plist-get other :cents))
                                 orders)
                       :id)
  :ties (--map (plist-get it :id)
               (--filter (= (plist-get it :cents) 4200) orders))
  ;; Annotate keeps the original beside its key.
  :annotated (--map (cons (car it) (plist-get (cdr it) :id))
                    (--annotate (plist-get it :region) orders))))
"##,
        expect![[
            r#"OK (:result (:ledger (("Katherine" . 12300) ("Grace" . 5150) ("Ada" . 6675)) :reductions (0 4200 5150 7025 19325 23525 24125) :from-the-right (1041 1042 1043 1044 1045 1046) :largest 1044 :smallest 1046 :ties (1041 1045) :annotated ((north . 1041) (south . 1042) (north . 1043) (east . 1044) (south . 1045) (east . 1046))) :source-unchanged t :source ((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen")) (:id 1042 :customer "Grace" :region south :cents 950 :items ("pen")) (:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk")) (:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug")) (:id 1045 :customer "Grace" :region south :cents 4200 :items nil) (:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen"))))"#
        ]],
    )
}

fn the_destructive_operations_rewrite_their_argument_and_the_others_do_not() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_destructive_operations_rewrite_their_argument_and_the_others_do_not",
        r##"
(list
 ;; `-insert-at', `-remove-at' and `-replace-at' are documented as
 ;; returning a new list.  The source must survive them intact.
 :pure
 (let* ((source (list 'a 'b 'c 'd))
        (results (list (-insert-at 2 'x source)
                       (-remove-at 1 source)
                       (-replace-at 0 'z source)
                       (-update-at 3 #'symbol-name source)
                       (-remove-at-indices '(0 3) source))))
   (list :results results
         :source-after source
         :source-unchanged (equal source '(a b c d))))
 ;; `-splice' replaces matching elements with a list of elements.
 :splice
 (let* ((source (list 1 2 3 4))
        (spliced (--splice (= 0 (% it 2)) (list it it) source)))
   (list :result spliced
         :source-after source
         :source-unchanged (equal source '(1 2 3 4))))
 ;; `!cons' and `!cdr' are the destructive pair, and they say so with the
 ;; bang: they modify the place they are given.
 :destructive
 (let* ((stack (list 'b 'c))
        (before (copy-sequence stack)))
   (!cons 'a stack)
   (let ((after-push (copy-sequence stack)))
     (!cdr stack)
     (list :before before
           :after-push after-push
           :after-pop (copy-sequence stack))))
 ;; Sorting is the one to watch: dash promises not to disturb the input.
 :sorting
 (let* ((source (list 3 1 2))
        (sorted (-sort #'< source)))
   (list :sorted sorted
         :source-after source
         :source-unchanged (equal source '(3 1 2)))))
"##,
        expect![[
            r#"OK (:pure (:results ((a b x . #1=(c d)) (a . #1#) (z . #2=(b . #1#)) (a b c "d") (b c)) :source-after (a . #2#) :source-unchanged t) :splice (:result (1 2 2 3 4 4) :source-after (1 2 3 4) :source-unchanged t) :destructive (:before (b c) :after-push (a b c) :after-pop (b c)) :sorting (:sorted (1 2 3) :source-after (3 1 2) :source-unchanged t))"#
        ]],
    )
}

fn threading_and_short_circuiting_over_a_record_that_may_be_missing() -> ParityBatchCase {
    ParityBatchCase::value(
        "threading_and_short_circuiting_over_a_record_that_may_be_missing",
        r##"
(dash-test-on-fresh
 (cl-flet ((find-order (id) (--find (= id (plist-get it :id)) orders)))
   (list
    ;; `-some->' stops at the first nil instead of signalling.
    :present (-some-> (find-order 1044)
                      (plist-get :items)
                      (car)
                      (upcase))
    :missing (-some-> (find-order 9999)
                      (plist-get :items)
                      (car)
                      (upcase))
    ;; An order with an empty item list stops the chain at `car'.
    :empty-items (-some-> (find-order 1045)
                          (plist-get :items)
                          (car)
                          (upcase))
    ;; `-as->' names the value so it can sit in any argument position.
    :named (-as-> (find-order 1041) order
                  (plist-get order :customer)
                  (concat "order for " order))
    ;; `-if-let' and `-when-let' bind and branch in one form.
    :if-let (-if-let (found (find-order 1042))
                (plist-get found :customer)
              "no such order")
    :if-let-else (-if-let (found (find-order 9999))
                     (plist-get found :customer)
                   "no such order")
    ;; `-when-let*' abandons the whole body as soon as one binding is nil.
    :when-let* (-when-let* ((found (find-order 1046))
                            (items (plist-get found :items))
                            (first (car items)))
                 (list first (length items)))
    :when-let*-stops (-when-let* ((found (find-order 1045))
                                  (items (plist-get found :items))
                                  (first (car items)))
                       (list first (length items))))))
"##,
        expect![[
            r#"OK (:result (:present "DESK" :missing nil :empty-items nil :named "order for Ada" :if-let "Grace" :if-let-else "no such order" :when-let* ("pen" 2) :when-let*-stops nil) :source-unchanged t :source ((:id 1041 :customer "Ada" :region north :cents 4200 :items ("book" "pen")) (:id 1042 :customer "Grace" :region south :cents 950 :items ("pen")) (:id 1043 :customer "Ada" :region north :cents 1875 :items ("desk")) (:id 1044 :customer "Katherine" :region east :cents 12300 :items ("desk" "lamp" "rug")) (:id 1045 :customer "Grace" :region south :cents 4200 :items nil) (:id 1046 :customer "Ada" :region east :cents 600 :items ("pen" "pen"))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        summarising_an_order_book_by_region_through_one_threaded_pipeline(),
        destructuring_each_record_with_let_and_lambda_patterns(),
        splitting_a_run_of_orders_into_windows_and_runs(),
        folding_over_the_book_to_build_a_ledger_and_pick_extremes(),
        the_destructive_operations_rewrite_their_argument_and_the_others_do_not(),
        threading_and_short_circuiting_over_a_record_that_may_be_missing(),
    ]
}
