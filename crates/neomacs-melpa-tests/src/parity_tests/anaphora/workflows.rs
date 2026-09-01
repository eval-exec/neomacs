use expect_test::expect;

use super::ParityBatchCase;

/// The reason to reach for `aif' at all: look something up once, then use the
/// result without naming it.  The lookup counter is the point -- four forms,
/// four lookups, so no macro evaluates its tested form twice -- and the else
/// branches show what plain `if' cannot give you, `it' bound to the nil the
/// lookup returned.  `awhen' returns nil for a missing project and the value of
/// its last body form for a found one, including when that project is empty.
fn looking_a_project_up_binds_it_in_both_branches_and_evaluates_the_lookup_once() -> ParityBatchCase
{
    ParityBatchCase::value(
        "looking_a_project_up_binds_it_in_both_branches_and_evaluates_the_lookup_once",
        r##"(let ((lookups 0))
  (cl-flet ((project (name)
              (setq lookups (1+ lookups))
              (anaphora-test-project name)))
    (list :found (aif (project "neomacs")
                     (list (plist-get it :name) (length (plist-get it :tasks)))
                   :missing)
          :missing (aif (project "nope")
                       (plist-get it :name)
                     (list :fallback it))
          :when-found (awhen (project "scratch")
                        (list (plist-get it :name) (plist-get it :tasks)))
          :when-missing (awhen (project "nope") :never)
          :lookups lookups)))"##,
        expect![[
            r#"OK (:found ("neomacs" 3) :missing (:fallback nil) :when-found ("scratch" nil) :when-missing nil :lookups 4)"#
        ]],
    )
}

fn nested_anaphoric_forms_shadow_it_and_restore_the_outer_binding() -> ParityBatchCase {
    ParityBatchCase::value(
        "nested_anaphoric_forms_shadow_it_and_restore_the_outer_binding",
        r##"(alet (anaphora-test-project "neomacs")
  (list :outer (plist-get it :name)
        :inner (awhen (plist-get it :owner)
                 (list :login (plist-get it :login)
                       :deeper (aif (plist-get it :email)
                                   (upcase it)
                                 :none)))
        :restored (plist-get it :name)
        :captured (funcall (lambda () (plist-get it :name)))
        :second-project (alet (anaphora-test-project "scratch")
                          (list (plist-get it :name)
                                (awhen (plist-get it :owner)
                                  (aif (plist-get it :email)
                                      (upcase it)
                                    (list :no-email (plist-get it :login))))))
        :restored-again (plist-get it :name)))"##,
        expect![[
            r#"OK (:outer "neomacs" :inner (:login "eval-exec" :deeper "EXEC@EXAMPLE.COM") :restored "neomacs" :captured "neomacs" :second-project ("scratch" (:no-email nil)) :restored-again "neomacs")"#
        ]],
    )
}

fn aand_walks_into_nested_data_and_stops_at_the_first_missing_step() -> ParityBatchCase {
    ParityBatchCase::value(
        "aand_walks_into_nested_data_and_stops_at_the_first_missing_step",
        r##"(let ((steps nil))
  (cl-flet ((note (label value) (push label steps) value))
    (let ((full (aand (anaphora-test-project "neomacs")
                      (note :owner (plist-get it :owner))
                      (note :email (plist-get it :email))
                      (note :upcase (upcase it))))
          (short (aand (anaphora-test-project "scratch")
                       (note :owner (plist-get it :owner))
                       (note :email (plist-get it :email))
                       (note :upcase (upcase it))))
          (none (aand (anaphora-test-project "nope")
                      (note :never (plist-get it :owner)))))
      (list :full full
            :short short
            :none none
            :steps (nreverse steps)))))"##,
        expect![[
            r#"OK (:full "EXEC@EXAMPLE.COM" :short nil :none nil :steps (:owner :email :upcase :owner :email))"#
        ]],
    )
}

fn acond_classifies_each_record_with_the_value_its_own_clause_tested() -> ParityBatchCase {
    ParityBatchCase::value(
        "acond_classifies_each_record_with_the_value_its_own_clause_tested",
        r##"(let ((tests 0))
  (cl-flet ((points (task) (setq tests (1+ tests)) (plist-get task :points)))
    (list :classified
          (mapcar (lambda (task)
                    (acond
                     ((eq (plist-get task :state) 'done) (list :done it))
                     ((points task) (list :estimated it (* it 2)))
                     ((plist-get task :title))
                     (t :unknown)))
                  (anaphora-test-tasks "neomacs"))
          :tests tests
          :no-clause-matches (acond (nil :never) ((cdr nil) :never-either))
          :bare-clause-value (acond ((plist-get (car (anaphora-test-tasks "neomacs")) :title))))))"##,
        expect![[
            r#"OK (:classified ((:done t) (:estimated 8 16) "write docs") :tests 2 :no-clause-matches nil :bare-clause-value "port isearch")"#
        ]],
    )
}

fn a_recursive_walk_a_work_queue_and_the_arithmetic_macros_build_a_report() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_recursive_walk_a_work_queue_and_the_arithmetic_macros_build_a_report",
        r##"(let* ((points (mapcar (lambda (task) (or (plist-get task :points) 0))
                       (anaphora-test-tasks "neomacs")))
       (tree (list 1 (list 2 (list 3 4)) 5))
       (total (funcall (alambda (node)
                         (cond ((null node) 0)
                               ((numberp node) node)
                               (t (+ (self (car node)) (self (cdr node))))))
                       tree))
       (queue (mapcar (lambda (task) (plist-get task :title))
                      (anaphora-test-tasks "neomacs")))
       (seen nil))
  (awhile (pop queue)
    (push (list it (length it)) seen))
  (list :tree-total total
        :titles (nreverse seen)
        :points points
        :sum (a+ 2 (* it 3) (- it 1))
        :product (a* 3 (+ it 1) it)
        :difference (a- 10 (+ 2 2) it)
        :quotient (a/ 100 5 it)
        :no-it-in-the-dividend (condition-case error (a- 10 (/ it 2) it) (error error))
        :empty-sums (list (a+) (a*) (a- 4))))"##,
        expect![[
            r#"OK (:tree-total 15 :titles (("port isearch" 12) ("fix the collector" 17) ("write docs" 10)) :points (5 8 0) :sum 13 :product 48 :difference 2 :quotient 4 :no-it-in-the-dividend (void-variable it) :empty-sums (0 1 -4))"#
        ]],
    )
}

fn the_long_names_are_the_same_macros_and_the_short_aliases_carry_their_metadata() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_long_names_are_the_same_macros_and_the_short_aliases_carry_their_metadata",
        r##"(let ((installed
       (list :same-result
             (list (anaphoric-if (anaphora-test-project "neomacs") (plist-get it :name) :none)
                   (aif (anaphora-test-project "neomacs") (plist-get it :name) :none))
             :alias (symbol-function 'aif)
             :indent (list (get 'aif 'lisp-indent-function)
                           (get 'anaphoric-if 'lisp-indent-function)
                           (get 'awhen 'lisp-indent-function))
             :edebug (list (get 'aif 'edebug-form-spec)
                           (get 'acond 'edebug-form-spec)
                           (get 'alambda 'edebug-form-spec))
             :long-names-only anaphora-use-long-names-only)))
  (anaphora--install-traditional-aliases -1)
  (let ((removed (list :short (list (fboundp 'aif) (fboundp 'acond) (fboundp 'a+))
                       :long (list (fboundp 'anaphoric-if) (fboundp 'anaphoric-cond))
                       :long-still-works
                       (anaphoric-when (anaphora-test-project "scratch")
                         (plist-get it :name)))))
    (anaphora--install-traditional-aliases)
    (list :installed installed
          :after-removal removed
          :after-reinstall (list (fboundp 'aif)
                                 (symbol-function 'a+)
                                 (aif (anaphora-test-project "scratch")
                                     (plist-get it :name)
                                   :none)))))"##,
        expect![[
            r#"OK (:installed (:same-result ("neomacs" "neomacs") :alias anaphoric-if :indent (2 2 1) :edebug (t cond lambda) :long-names-only nil) :after-removal (:short (nil nil nil) :long (t t) :long-still-works "scratch") :after-reinstall (t anaphoric-+ "scratch"))"#
        ]],
    )
}

fn byte_compiling_the_same_anaphoric_code_gives_the_same_answers() -> ParityBatchCase {
    ParityBatchCase::value(
        "byte_compiling_the_same_anaphoric_code_gives_the_same_answers",
        r##"(let* ((source '(lambda (projects)
                    (list :login (aand (car projects)
                                       (plist-get it :owner)
                                       (plist-get it :login)
                                       (upcase it))
                          :states (mapcar (lambda (task)
                                            (acond
                                             ((eq (plist-get task :state) 'done) (list :done it))
                                             ((plist-get task :points) (* it 10))
                                             (t :none)))
                                          (plist-get (car projects) :tasks))
                          :depth (funcall (alambda (node)
                                            (if (consp node)
                                                (1+ (self (car node)))
                                              0))
                                          '(((1))))
                          :drained (let ((queue (list 3 2 1)) (seen nil))
                                     (awhile (pop queue)
                                       (push (* it it) seen))
                                     seen))))
       (interpreted (eval source t))
       (compiled (byte-compile (eval source t)))
       (interpreted-result (funcall interpreted anaphora-test-projects))
       (compiled-result (funcall compiled anaphora-test-projects)))
  (list :interpreted interpreted-result
        :agree (equal interpreted-result compiled-result)
        :compiled-is-byte-code (byte-code-function-p compiled)
        :interpreted-is-byte-code (byte-code-function-p interpreted)))"##,
        expect![[
            r#"OK (:interpreted (:login "EVAL-EXEC" :states ((:done t) 80 :none) :depth 3 :drained (1 4 9)) :agree t :compiled-is-byte-code t :interpreted-is-byte-code nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        looking_a_project_up_binds_it_in_both_branches_and_evaluates_the_lookup_once(),
        nested_anaphoric_forms_shadow_it_and_restore_the_outer_binding(),
        aand_walks_into_nested_data_and_stops_at_the_first_missing_step(),
        acond_classifies_each_record_with_the_value_its_own_clause_tested(),
        a_recursive_walk_a_work_queue_and_the_arithmetic_macros_build_a_report(),
        the_long_names_are_the_same_macros_and_the_short_aliases_carry_their_metadata(),
        byte_compiling_the_same_anaphoric_code_gives_the_same_answers(),
    ]
}
