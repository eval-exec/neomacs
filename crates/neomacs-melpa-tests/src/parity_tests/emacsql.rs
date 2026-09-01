use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EMACSQL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'emacsql)

(defclass neomacs-emacsql-test-connection (emacsql-connection)
  ((events :initarg :events :initform nil)
   (rows :initarg :rows :initform nil)
   (lock-once :initarg :lock-once :initform nil)))

(cl-defmethod emacsql ((connection neomacs-emacsql-test-connection) sql &rest args)
  (let ((compiled (apply #'emacsql-compile nil sql args)))
    (oset connection events
          (append (oref connection events) (list compiled)))
    (if (and (oref connection lock-once)
             (memq (and (vectorp sql) (aref sql 0))
                   '(:insert :insert-into)))
        (progn
          (oset connection lock-once nil)
          (signal 'emacsql-locked '("simulated busy database")))
      (when (eq (and (vectorp sql) (aref sql 0)) :select)
        (copy-tree (oref connection rows))))))

(cl-defmethod emacsql-close ((connection neomacs-emacsql-test-connection))
  (oset connection events
        (append (oref connection events) '("CLOSE"))))

(defclass neomacs-emacsql-test-protocol
  (emacsql-protocol-mixin emacsql-connection)
  ((test-buffer :initarg :test-buffer)))

(cl-defmethod emacsql-buffer ((connection neomacs-emacsql-test-protocol))
  (oref connection test-buffer))
"####;

fn compiles_a_people_directory_schema_and_crud_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(let ((schema
       '([(id integer :primary-key)
          (email :unique :not-null)
          display-name
          teams
          active]
         (:check (> id 0)))))
  (list
   :create
   (emacsql-compile nil [:create-table :if-not-exists $i1 $S2]
                    'people schema)
   :insert
   (emacsql-compile
    nil
    [:insert-into $i1 [id email display-name teams active] :values $v2]
    'people
    '([1001 "ada@example.test" "Ada Lovelace" (compiler research) t]
      [1002 "grace@example.test" "Grace Hopper" [navy compilers] nil]))
   :lookup
   (emacsql-compile
    nil
    [:select [id display-name teams]
     :from $i1
     :where (and (= active $s2) (in id $v3))
     :order-by [(asc display-name)]
     :limit $s4]
    'people t [1002 1001] 25)
   :promote
   (emacsql-compile
    nil
    [:update $i1 :set [(= teams $s2) (= active $s3)] :where (= id $s4)]
    'people '(leadership compilers) t 1002)
   :remove
   (emacsql-compile nil [:delete-from $i1 :where (= id $s2)] 'people 1001)))
"####;
    let expected = expect![[
        r#"OK (:create "CREATE TABLE IF NOT EXISTS people (id &INTEGER PRIMARY KEY, email &NONE UNIQUE NOT NULL, display_name &NONE, teams &NONE, active &NONE, CHECK (id > 0));" :insert "INSERT INTO people (id, email, display_name, teams, active) VALUES (1001, '\"ada@example.test\"', '\"Ada Lovelace\"', '(compiler research)', 't'), (1002, '\"grace@example.test\"', '\"Grace Hopper\"', '[navy compilers]', NULL);" :lookup "SELECT id, display_name, teams FROM people WHERE active = 't' AND id IN (1002, 1001) ORDER BY display_name ASC LIMIT 25;" :promote "UPDATE people SET teams = '(leadership compilers)', active = 't' WHERE id = 1002;" :remove "DELETE FROM people WHERE id = 1001;")"#
    ]];
    ParityBatchCase::value(
        "compiles_a_people_directory_schema_and_crud_workflow",
        elisp_form,
        expected,
    )
}

fn prepared_templates_escape_untrusted_release_inputs() -> ParityBatchCase {
    let elisp_form = r####"
(let ((saved-reserved emacsql-reserved))
  (unwind-protect
      (progn
        (setq emacsql-reserved (copy-hash-table emacsql-reserved))
        (emacsql-register-reserved '(select release))
        (list
         :identifiers
         (mapcar #'emacsql-escape-identifier
                 '(release release:id deployment-log strange\ table foo$))
         :query
         (emacsql-compile
          nil
          [:insert-into $i1 [release-id actor payload notes]
           :values [$s2 $s3 $s4 $s5]]
          'deployment-log
          "v2.4.0"
          "Robert'); DROP TABLE deployment-log;--"
          '(:status shipped :artifacts ["linux" "windows"])
          "line one\n50% 'ready' — 東京")
         :path-query
         (emacsql-compile nil [:attach $r1 :as $i2]
                          "/srv/releases/it's live.db" 'release-db)))
    (setq emacsql-reserved saved-reserved)))
"####;
    let expected = expect![[
        r#"OK (:identifiers ("\"release\"" "\"release\".id" "deployment_log" "\"strange\\ table\"" "foo$") :query "INSERT INTO deployment_log (release_id, actor, payload, notes) VALUES ('\"v2.4.0\"', '\"Robert''); DROP TABLE deployment-log;--\"', '(:status shipped :artifacts [\"linux\" \"windows\"])', '\"line one\\n50% ''ready'' — 東京\"');" :path-query "ATTACH '/srv/releases/it''s live.db' AS release_db;")"#
    ]];
    ParityBatchCase::value(
        "prepared_templates_escape_untrusted_release_inputs",
        elisp_form,
        expected,
    )
}

fn compiles_a_nested_release_health_report_with_exact_precedence() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :report
 (emacsql-compile
  nil
  [:select [service
            (funcall count :distinct deploy-id)
            (funcall max duration-ms)]
   :from [(as deployments d)
          (as [:select [service-id owner]
               :from services
               :where (in owner $v1)] s)]
   :where
   (and (= d:service-id s:service-id)
        (<= $s2 d:started-at $s3)
        (or (= status 'failed)
            (and (= status 'succeeded) (> duration-ms $s4))))
   :group-by service
   :order-by [(desc (funcall count :distinct deploy-id))
              (asc service)]
   :limit [$s5 $s6]]
  '[alice bob] 1700000000 1700086400 300000 20 10)
 :precedence
 (emacsql-compile
  nil
  [:select
   [(+ (* successful 100) (/ total 2))
    (not (is last-error nil))
    (>= lower score upper)]
   :from release-health]))
"####;
    let expected = expect![[
        r#"OK (:report "SELECT service, count(DISTINCT deploy_id), max(duration_ms) FROM deployments AS d, (SELECT service_id, owner FROM services WHERE owner IN ('alice', 'bob')) AS s WHERE d.service_id = s.service_id AND d.started_at BETWEEN 1700000000 AND 1700086400 AND (status = 'failed' OR status = 'succeeded' AND duration_ms > 300000) GROUP BY service ORDER BY count(DISTINCT deploy_id) DESC, service ASC LIMIT 20, 10;" :precedence "SELECT successful * 100 + total / 2, NOT last_error IS NULL, score BETWEEN upper AND lower FROM release_health;")"#
    ]];
    ParityBatchCase::value(
        "compiles_a_nested_release_health_report_with_exact_precedence",
        elisp_form,
        expected,
    )
}

fn retries_locked_transactions_and_rolls_back_unfinished_work() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((retry-db (neomacs-emacsql-test-connection :lock-once t))
       (attempts 0)
       (result
        (emacsql-with-transaction retry-db
          (setq attempts (1+ attempts))
          (emacsql retry-db [:insert-into jobs :values [$s1]] attempts)
          (emacsql-with-transaction retry-db
            (emacsql retry-db [:update jobs :set (= state 'done)]))
          (list :committed attempts)))
       (failure-db (neomacs-emacsql-test-connection))
       (failure
        (condition-case condition
            (emacsql-with-transaction failure-db
              (emacsql failure-db [:insert-into audit :values ["started"]])
              (error "publisher failed"))
          (error (list (car condition) (error-message-string condition)))))
       (close-db (neomacs-emacsql-test-connection :rows '(("Ada") ("Grace"))))
       (names
        (emacsql-with-connection (db close-db)
          (emacsql-with-bind db [:select [name] :from people :order-by name]
            (upcase name)))))
  (list :result result
        :attempts attempts
        :retry-events (oref retry-db events)
        :failure failure
        :failure-events (oref failure-db events)
        :last-bound-name names
        :close-events (oref close-db events)))
"####;
    let expected = expect![[
        r#"OK (:result (:committed 2) :attempts 2 :retry-events ("BEGIN;" "INSERT INTO jobs VALUES (1);" "ROLLBACK;" "BEGIN;" "INSERT INTO jobs VALUES (2);" "UPDATE jobs SET state = 'done';" "COMMIT;") :failure (error "publisher failed") :failure-events ("BEGIN;" "INSERT INTO audit VALUES ('\"started\"');" "ROLLBACK;") :last-bound-name "GRACE" :close-events ("SELECT name FROM people ORDER BY name;" "CLOSE"))"#
    ]];
    ParityBatchCase::value(
        "retries_locked_transactions_and_rolls_back_unfinished_work",
        elisp_form,
        expected,
    )
}

fn parses_backend_results_and_preserves_database_error_details() -> ParityBatchCase {
    let elisp_form = r####"
(let ((buffer (generate-new-buffer " *neomacs-emacsql-protocol*")))
  (unwind-protect
      (let ((db (neomacs-emacsql-test-protocol :test-buffer buffer)))
        (with-current-buffer buffer
          (insert "((1001 \"Ada\" (compiler research)) (1002 \"Grace\" nil)) success\n#\n"))
        (let ((waiting (emacsql-waiting-p db))
              (rows (emacsql-parse db)))
          (with-current-buffer buffer
            (erase-buffer)
            (insert "error 19 \"UNIQUE constraint failed: people.email\"\n#\n"))
          (list
           :waiting waiting
           :rows rows
           :error
           (condition-case condition
               (progn (emacsql-parse db) :accepted)
             (emacsql-error
              (list (car condition)
                    (cdr condition)
                    (error-message-string condition)))))))
    (kill-buffer buffer)))
"####;
    let expected = expect![[
        r#"OK (:waiting t :rows ((1001 "Ada" (compiler research)) (1002 "Grace" nil)) :error (emacsql-error ("UNIQUE constraint failed: people.email" 19) "EmacSQL had an unhandled condition: \"UNIQUE constraint failed: people.email\", 19"))"#
    ]];
    ParityBatchCase::value(
        "parses_backend_results_and_preserves_database_error_details",
        elisp_form,
        expected,
    )
}

fn authoring_tools_flatten_and_insert_prepared_sql_at_point() -> ParityBatchCase {
    let elisp_form = r####"
(let ((statement
       [:select [release-id status]
        :from deployments
        :where (and (= owner $s1) (> finished-at $s2))
        :order-by [(desc finished-at)]]))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert ";; inspect the release query\n" (prin1-to-string statement))
    (goto-char (point-max))
    (emacsql-show-last-sql t)
    (list :flattened (emacsql-flatten-sql statement)
          :buffer (buffer-string)
          :point (point)
          :mode major-mode
          :modified (buffer-modified-p))))
"####;
    let expected = expect![[
        r#"OK (:flattened "SELECT release_id, status FROM deployments WHERE owner = $1 AND finished_at > $2 ORDER BY finished_at DESC;" :buffer ";; inspect the release query\n[:select [release-id status] :from deployments :where (and (= owner $s1) (> finished-at $s2)) :order-by [(desc finished-at)]]SELECT release_id, status FROM deployments WHERE owner = $1 AND finished_at > $2 ORDER BY finished_at DESC;" :point 262 :mode emacs-lisp-mode :modified t)"#
    ]];
    ParityBatchCase::value(
        "authoring_tools_flatten_and_insert_prepared_sql_at_point",
        elisp_form,
        expected,
    )
}

fn malformed_statements_report_specific_conditions_and_cache_boundaries() -> ParityBatchCase {
    let elisp_form = r####"
(let ((saved-cache emacsql-prepare-cache))
  (unwind-protect
      (progn
        (setq emacsql-prepare-cache (make-hash-table :test 'equal :weakness 'key))
        (let ((before (hash-table-count emacsql-prepare-cache)))
          (list
           :failures
           (mapcar
            (lambda (operation)
              (condition-case condition
                  (progn (funcall operation) :accepted)
                (error (list (car condition) (error-message-string condition)))))
            (list
             (lambda () (emacsql-compile nil [:insert-into people :values 1]))
             (lambda () (emacsql-compile nil [:select * :from $i1] "people"))
             (lambda () (emacsql-compile nil [:where (escape name "x")]))
             (lambda () (emacsql-compile nil [:create-table people ([nil])]))))
           :cache-before before
           :cache-after (hash-table-count emacsql-prepare-cache))))
    (setq emacsql-prepare-cache saved-cache)))
"####;
    let expected = expect![[
        r#"OK (:failures ((emacsql-syntax "Invalid SQL statement: \"Invalid vector: 1\"") (emacsql-syntax "Invalid SQL statement: \"Invalid identifier: \\\"people\\\"\"") (emacsql-syntax "Invalid SQL statement: \"Second operand of escape has to be a character, got x\"") (emacsql-syntax "Invalid SQL statement: \"Invalid identifier: nil\"")) :cache-before 0 :cache-after 1)"#
    ]];
    ParityBatchCase::value(
        "malformed_statements_report_specific_conditions_and_cache_boundaries",
        elisp_form,
        expected,
    )
}

fn emacsql_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EMACSQL_MELPA_PIN, "emacsql.el")
        .expect("prepare pinned EmacSQL source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn emacsql_practical_workflows_batch() {
    let cases = vec![
        compiles_a_people_directory_schema_and_crud_workflow(),
        prepared_templates_escape_untrusted_release_inputs(),
        compiles_a_nested_release_health_report_with_exact_precedence(),
        retries_locked_transactions_and_rolls_back_unfinished_work(),
        parses_backend_results_and_preserves_database_error_details(),
        authoring_tools_flatten_and_insert_prepared_sql_at_point(),
        malformed_statements_report_specific_conditions_and_cache_boundaries(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("emacsql parity batch");
    assert_oracle_batch_cases(emacsql_oracle(), test_name, "emacsql parity", &cases);
}
