use expect_test::expect;

use super::ParityBatchCase;

fn updating_a_moved_project_file_repairs_its_namespace() -> ParityBatchCase {
    ParityBatchCase::value(
        "updating_a_moved_project_file_repairs_its_namespace",
        r##"
(let* ((root (expand-file-name "inventory-service/"
                               (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (source-dir (expand-file-name "src/acme/" root))
       (path (expand-file-name "order_service.clj" source-dir)))
  (make-directory (expand-file-name ".git" root) t)
  (make-directory source-dir t)
  (with-temp-file (expand-file-name "deps.edn" root)
    (insert "{:paths [\"src\"]}"))
  (with-temp-file path
    (insert "(ns legacy.orders)\n\n(defn pending-orders [orders]\n  (filter :pending? orders))\n"))
  (let ((buffer (find-file-noselect path)))
    (unwind-protect
        (with-current-buffer buffer
          (clojure-mode)
          (let* ((project (clojure-project-dir))
                 (expected (clojure-expected-ns))
                 (inside-project
                  (equal (file-truename project) (file-truename root))))
            (clojure-update-ns)
            (list :project-found inside-project
                  :expected expected
                  :cached clojure-cached-ns
                  :text (buffer-substring-no-properties
                         (point-min) (point-max)))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##,
        expect![[
            r##"OK (:project-found t :expected "acme.order-service" :cached "acme.order-service" :text "(ns acme.order-service)\n\n(defn pending-orders [orders]\n  (filter :pending? orders))\n")"##
        ]],
    )
}

fn formatting_a_real_order_pipeline_applies_clojure_structure_and_alignment() -> ParityBatchCase {
    ParityBatchCase::value(
        "formatting_a_real_order_pipeline_applies_clojure_structure_and_alignment",
        r##"
(with-temp-buffer
  (insert "(ns warehouse.orders)\n\n(defn prepare-orders\n\"Prepare paid orders for dispatch.\"\n[orders]\n(let [ready (filter :paid? orders)\ntotals (map :total ready)]\n{:count (count ready)\n:total (reduce + totals)\n:orders (->> ready\n(map #(assoc % :ready true))\n(sort-by :id))}))\n\n#?(:clj (println \"server ready\")\n:cljs (js/console.log \"browser ready\"))\n")
  (clojure-mode)
  (font-lock-ensure)
  (let ((clojure-align-forms-automatically t)
        (clojure-align-reader-conditionals t))
    (indent-region (point-min) (point-max)))
  (goto-char (point-min))
  (search-forward "assoc")
  (list :defun (clojure-current-defun-name)
        :text (buffer-substring-no-properties (point-min) (point-max))))
"##,
        expect![[
            r##"OK (:defun "prepare-orders" :text "(ns warehouse.orders)\n\n(defn prepare-orders\n  \"Prepare paid orders for dispatch.\"\n  [orders]\n  (let [ready  (filter :paid? orders)\n        totals (map :total ready)]\n    {:count  (count ready)\n     :total  (reduce + totals)\n     :orders (->> ready\n                  (map #(assoc % :ready true))\n                  (sort-by :id))}))\n\n#?(:clj  (println \"server ready\")\n   :cljs (js/console.log \"browser ready\"))\n")"##
        ]],
    )
}

fn threading_and_unwinding_an_order_query_preserves_the_pipeline() -> ParityBatchCase {
    ParityBatchCase::value(
        "threading_and_unwinding_an_order_query_preserves_the_pipeline",
        r##"
(with-temp-buffer
  (insert "(map :id (filter :paid? (sort-by :created-at orders)))")
  (clojure-mode)
  (goto-char (point-min))
  (clojure-thread-last-all nil)
  (let ((threaded (buffer-substring-no-properties (point-min) (point-max))))
    (goto-char (point-max))
    (clojure-unwind-all)
    (list :threaded threaded
          :unwound (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect![[
            r##"OK (:threaded "(->> orders\n     (sort-by :created-at)\n     (filter :paid?)\n     (map :id))" :unwound "(map :id (filter :paid? (sort-by :created-at orders)))")"##
        ]],
    )
}

fn migrating_service_configuration_combines_public_editing_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "migrating_service_configuration_combines_public_editing_commands",
        r##"
(with-temp-buffer
  (insert "(def service-config\n  {:environment \"production\"\n   :retry-policy [1 3 5]\n   :features #{:audit :metrics}})")
  (clojure-mode)
  (goto-char (point-min))
  (clojure-cycle-privacy)
  (goto-char (point-min))
  (search-forward ":environment")
  (goto-char (match-beginning 0))
  (clojure-toggle-keyword-string)
  (goto-char (point-min))
  (search-forward "[1 3 5]")
  (goto-char (match-beginning 0))
  (clojure-convert-collection-to-set)
  (goto-char (point-min))
  (search-forward "#{:audit")
  (backward-char (length ":audit"))
  (clojure-convert-collection-to-quoted-list)
  (indent-region (point-min) (point-max))
  (list :private (save-excursion
                   (goto-char (point-min))
                   (looking-at "(def \\^:private"))
        :text (buffer-substring-no-properties (point-min) (point-max))))
"##,
        expect![[
            r##"OK (:private t :text "(def ^:private service-config\n  {\"environment\" \"production\"\n   :retry-policy #{1 3 5}\n   :features '(:audit :metrics)})")"##
        ]],
    )
}

fn navigating_and_fontifying_a_mixed_platform_source_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "navigating_and_fontifying_a_mixed_platform_source_file",
        r##"
(with-temp-buffer
  (insert "(ns alerts.delivery)\n\n#?(:clj\n   (defn send-alert [message]\n     (println message))\n   :cljs\n   (defn send-alert [message]\n     (js/console.log message)))\n\n#_(defn obsolete-alert [message]\n    (println \"obsolete\" message))\n\n(defn active-alert [message]\n  (send-alert message))\n")
  (clojure-mode)
  (font-lock-ensure)
  (let ((faces
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (list needle
                  (get-text-property (- (point) (length needle)) 'face)))
          '("alerts.delivery" "send-alert" "obsolete-alert" "active-alert"))))
    (goto-char (point-min))
    (search-forward "(send-alert message)")
    (list :current-defun (clojure-current-defun-name)
          :faces faces
          :discarded-syntax-comment
          (save-excursion
            (goto-char (point-min))
            (search-forward "obsolete")
            (nth 4 (syntax-ppss)))
          :balanced-end (scan-sexps (point-min) 1))))
"##,
        expect![[
            r##"OK (:current-defun "active-alert" :faces (("alerts.delivery" font-lock-type-face) ("send-alert" font-lock-function-name-face) ("obsolete-alert" clojure-discard-face) ("active-alert" font-lock-function-name-face)) :discarded-syntax-comment nil :balanced-end 21)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        updating_a_moved_project_file_repairs_its_namespace(),
        formatting_a_real_order_pipeline_applies_clojure_structure_and_alignment(),
        threading_and_unwinding_an_order_query_preserves_the_pipeline(),
        migrating_service_configuration_combines_public_editing_commands(),
        navigating_and_fontifying_a_mixed_platform_source_file(),
    ]
}
