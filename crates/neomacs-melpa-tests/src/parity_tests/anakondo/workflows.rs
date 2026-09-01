use expect_test::expect;

use super::ParityBatchCase;

fn turning_the_mode_on_analyses_the_project_with_the_documented_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "turning_the_mode_on_analyses_the_project_with_the_documented_commands",
        r##"
        ;; A user opens a namespace in a `deps.edn' project and turns
        ;; `anakondo-minor-mode' on.  The package has to find the project root
        ;; the way it documents - clojure-mode first - ask the Clojure CLI for
        ;; the classpath, hand that classpath to clj-kondo with the analysis
        ;; config, scan the jars on it for Java classes, and leave four typed
        ;; caches keyed by that root, with completion hooked in buffer-locally.
        ;; The classpath argument keeps the newline `shell-command-to-string'
        ;; returns, which is what really reaches `--lint'.
        (ak-test-with-project
         (let ((root nil))
           (list
            :major-mode major-mode
            :clojure-mode-available (and (featurep 'clojure-mode) t)
            :project-el-available (and (featurep 'project) t)
            :enabled (progn (anakondo-minor-mode 1)
                            (list :on anakondo-minor-mode
                                  :lighter (ak-test-copy anakondo-minor-mode-lighter)
                                  :capf completion-at-point-functions
                                  :capf-buffer-local (local-variable-p 'completion-at-point-functions)))
            :root (progn (setq root (car (hash-table-keys anakondo--cache)))
                         (list :cached-roots (hash-table-count anakondo--cache)
                               :is-clojure-project-dir (equal root (clojure-project-dir))
                               :root (ak-test-copy root)))
            :caches (sort (mapcar #'symbol-name
                                  (hash-table-keys (gethash root anakondo--cache)))
                          #'string<)
            :namespaces (sort (mapcar #'symbol-name
                                      (hash-table-keys
                                       (gethash :ns-def-cache (gethash root anakondo--cache))))
                              #'string<)
            :var-namespaces (sort (mapcar #'symbol-name
                                          (hash-table-keys
                                           (gethash :var-def-cache (gethash root anakondo--cache))))
                                  #'string<)
            :java-classes (let ((cache (gethash :java-classes-cache (gethash root anakondo--cache)))
                                entries)
                            (maphash (lambda (key class-map)
                                       (push (list (symbol-name key)
                                                   (ak-test-copy (gethash :name class-map))
                                                   (gethash :methods-and-fields class-map))
                                             entries))
                                     cache)
                            (sort entries (lambda (a b) (string< (car a) (car b)))))
            :commands (ak-test-commands)
            :messages (ak-test-messages "^Analysing project for completion\\.\\.\\..*$"))))
    "##,
        expect![[
            r#"OK (:major-mode clojure-mode :clojure-mode-available t :project-el-available nil :enabled (:on t :lighter " k" :capf (anakondo-completion-at-point t) :capf-buffer-local t) :root (:cached-roots 1 :is-clojure-project-dir t :root "[ORACLE-SANDBOX]/project/") :caches (":java-classes-cache" ":ns-def-cache" ":ns-usage-cache" ":var-def-cache") :namespaces (":inventory.core" ":inventory.util") :var-namespaces (":inventory.core" ":inventory.util") :java-classes ((":com.warehouse.Barcode" "com.warehouse.Barcode" lazy)) :commands (("-Spath") ("--lint" "[ORACLE-SANDBOX]/project/src:[ORACLE-SANDBOX]/project/lib/warehouse.jar\\n" "--config" "{:output {:analysis true :format :json}}" "--lang" "clj" "cwd [ORACLE-SANDBOX]/project/src/inventory") ("-Spath")) :messages ("Analysing project for completion...done"))"#
        ]],
    )
}

fn completing_a_var_pipes_the_unsaved_buffer_to_clj_kondo() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_a_var_pipes_the_unsaved_buffer_to_clj_kondo",
        r##"
        ;; The user starts a new function that is not on disk yet and asks for
        ;; completion.  anakondo must analyse the *buffer*, not the file: it
        ;; pipes the whole buffer into `clj-kondo --lint -' and offers the vars
        ;; of the namespace it finds there.  The typed text is unsaved, so the
        ;; text clj-kondo received is the proof that the right thing was sent.
        (ak-test-with-project
         (progn
           (anakondo-minor-mode 1)
           (goto-char (point-max))
           (insert "\n(defn report []\n  (tot")
           (let ((offered (ak-test-candidates)))
             (list :offered offered
                   :modified (buffer-modified-p)
                   :stdin-is-the-buffer
                   (equal (ak-test-kondo-stdin)
                          (buffer-substring-no-properties (point-min) (point-max)))
                   :stdin-tail (car (last (split-string (ak-test-kondo-stdin) "\n" t)))
                   :buffer-analysis-command
                   (car (last (seq-filter (lambda (command) (member "--lint" command))
                                          (ak-test-commands))))
                   :completed (progn (completion-at-point) (ak-test-here))
                   :buffer-tail (car (last (split-string
                                            (buffer-substring-no-properties (point-min) (point-max))
                                            "\n" t)))))))
    "##,
        expect![[
            r#"OK (:offered (:start-column 3 :prefix "tot" :candidates ("total-price")) :modified t :stdin-is-the-buffer t :stdin-tail "  (tot" :buffer-analysis-command ("--lint" "-" "--config" "{:output {:analysis true :format :json}}" "--lang" "clj" "cwd [ORACLE-SANDBOX]/project/src/inventory" "stdin-bytes 751") :completed (:line 31 :column 14 :text "  (total-price") :buffer-tail "  (total-price")"#
        ]],
    )
}

fn completing_an_aliased_namespace_qualifies_every_var_with_the_alias() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_an_aliased_namespace_qualifies_every_var_with_the_alias",
        r##"
        ;; `inventory.core' requires `inventory.util' as `util' and
        ;; `clojure.string' as `str'.  Typing `util/' must offer every var
        ;; clj-kondo found in that namespace, each prefixed with the alias the
        ;; buffer actually used - the package's headline feature.  `str/' is
        ;; required the same way but its namespace is not on the analysed
        ;; classpath, so it contributes nothing; the bare namespace names are
        ;; still offered.
        (ak-test-with-project
         (progn
           (anakondo-minor-mode 1)
           (list
            :aliased (progn (goto-char (point-max))
                            (insert "\n(defn a [] (util/")
                            (ak-test-candidates))
            :aliased-but-unanalysed (progn (goto-char (point-max))
                                           (insert "\n(defn b [] (str/")
                                           (ak-test-candidates))
            :namespace-names (progn (goto-char (point-max))
                                    (insert "\n(defn c [] (inventory.")
                                    (ak-test-candidates))
            :current-namespace-var (progn (goto-char (point-max))
                                          (insert "\n(defn d [] (describ")
                                          (ak-test-candidates)))))
    "##,
        expect![[
            r#"OK (:aliased (:start-column 12 :prefix "util/" :candidates ("util/apply-discount" "util/default-price" "util/normalize-name")) :aliased-but-unanalysed (:start-column 12 :prefix "str/" :candidates nil) :namespace-names (:start-column 12 :prefix "inventory." :candidates ("inventory.core" "inventory.util")) :current-namespace-var (:start-column 12 :prefix "describ" :candidates ("describe")))"#
        ]],
    )
}

fn java_classes_and_public_static_members_come_from_a_real_jar() -> ParityBatchCase {
    ParityBatchCase::value(
        "java_classes_and_public_static_members_come_from_a_real_jar",
        r##"
        ;; The Java half runs against a real jar built by the real JDK tools.
        ;; anakondo scans it with `jar tf' at startup, keeping each class
        ;; unresolved, and only shells out to `javap' when the user actually
        ;; types a class-qualified prefix.  What comes back must be the public
        ;; *static* members only - the instance method and the constructor of
        ;; the same class must not be offered - and the class must be resolved
        ;; once, not on every keystroke.
        (ak-test-with-project
         (let ((root nil))
           (list
            :tools (list :javac (and (executable-find "javac") t)
                         :jar (and (executable-find "jar") t)
                         :javap (and (executable-find "javap") t))
            :enabled (progn (anakondo-minor-mode 1)
                            (setq root (car (hash-table-keys anakondo--cache)))
                            anakondo-minor-mode)
            :class-prefix (progn (goto-char (point-max))
                                 (insert "\n(defn a [] (com.warehouse.Bar")
                                 (ak-test-candidates))
            :still-lazy (gethash :methods-and-fields
                                 (gethash :com.warehouse.Barcode
                                          (gethash :java-classes-cache
                                                   (gethash root anakondo--cache))))
            :members (progn (goto-char (point-max))
                            (insert "\n(defn b [] (com.warehouse.Barcode/")
                            (ak-test-candidates))
            :resolved (let ((class-map (gethash :com.warehouse.Barcode
                                                (gethash :java-classes-cache
                                                         (gethash root anakondo--cache)))))
                        (sort (mapcar (lambda (member)
                                        (list (ak-test-copy (gethash :name member))
                                              (ak-test-copy (gethash :return-type member))
                                              (ak-test-copy (gethash :signature member))
                                              (gethash :method? member)))
                                      (gethash :methods-and-fields class-map))
                              (lambda (a b) (string< (car a) (car b)))))
            :members-again (progn (goto-char (point-max))
                                  (insert "\n(defn c [] (com.warehouse.Barcode/")
                                  (ak-test-candidates)))))
    "##,
        expect![[
            r#"OK (:tools (:javac t :jar t :javap t) :enabled t :class-prefix (:start-column 12 :prefix "com.warehouse.Bar" :candidates ("com.warehouse.Barcode")) :still-lazy lazy :members (:start-column 12 :prefix "com.warehouse.Barcode/" :candidates ("com.warehouse.Barcode/LENGTH" "com.warehouse.Barcode/PREFIX" "com.warehouse.Barcode/format" "com.warehouse.Barcode/valid")) :resolved (("LENGTH" "int" nil nil) ("PREFIX" "java.lang.String" nil nil) ("format" "java.lang.String" "(java.lang.String)" t) ("valid" "boolean" "(java.lang.String)" t)) :members-again (:start-column 12 :prefix "com.warehouse.Barcode/" :candidates ("com.warehouse.Barcode/LENGTH" "com.warehouse.Barcode/PREFIX" "com.warehouse.Barcode/format" "com.warehouse.Barcode/valid")))"#
        ]],
    )
}

fn clojure_default_imports_are_offered_but_cannot_be_resolved_on_a_modern_jdk() -> ParityBatchCase {
    ParityBatchCase::value(
        "clojure_default_imports_are_offered_but_cannot_be_resolved_on_a_modern_jdk",
        r##"
        ;; anakondo ships Clojure's default `java.lang' imports, so `Integ'
        ;; offers `Integer'.  Resolving its members is a different matter: the
        ;; package reads the JVM's `sun.boot.class.path' property, which JDK 9
        ;; deleted, so on this JDK 21 there is no boot classpath to add and
        ;; `java.lang' is not on the project classpath either.  The user is
        ;; offered a class that cannot be completed through.  That is the
        ;; package's own JDK 8 assumption, identical in both editors, and the
        ;; workflow pins it rather than steering around it.
        (ak-test-with-project
         (progn
           (anakondo-minor-mode 1)
           (list
            :java-version-property-exists
            (with-temp-buffer
              (call-process "java" nil t nil "-XshowSettings:properties" "-version")
              (and (save-excursion (goto-char (point-min))
                                   (search-forward "sun.boot.class.path" nil t))
                   t))
            :default-import-offered (progn (goto-char (point-max))
                                           (insert "\n(defn a [] (Integ")
                                           (ak-test-candidates))
            :default-import-members (progn (goto-char (point-max))
                                           (insert "\n(defn b [] (Integer/")
                                           (ak-test-candidates))
            :project-class-still-resolves (progn (goto-char (point-max))
                                                 (insert "\n(defn c [] (com.warehouse.Barcode/")
                                                 (ak-test-candidates)))))
    "##,
        expect![[
            r#"OK (:java-version-property-exists nil :default-import-offered (:start-column 12 :prefix "Integ" :candidates ("Integer")) :default-import-members (:start-column 12 :prefix "Integer/" :candidates nil) :project-class-still-resolves (:start-column 12 :prefix "com.warehouse.Barcode/" :candidates ("com.warehouse.Barcode/LENGTH" "com.warehouse.Barcode/PREFIX" "com.warehouse.Barcode/format" "com.warehouse.Barcode/valid")))"#
        ]],
    )
}

fn refreshing_rebuilds_the_analysis_and_refuses_when_the_mode_is_off() -> ParityBatchCase {
    ParityBatchCase::value(
        "refreshing_rebuilds_the_analysis_and_refuses_when_the_mode_is_off",
        r##"
        ;; `anakondo-refresh-project-cache' is the documented way to resync
        ;; after editing files on disk: it must run the whole project analysis
        ;; again - classpath, clj-kondo, jars - and say so both times.  Off the
        ;; mode it must refuse with its own error rather than analysing
        ;; anything.
        (ak-test-with-project
         (progn
           (anakondo-minor-mode 1)
           (let ((after-enable (length (ak-test-commands))))
             (anakondo-refresh-project-cache)
             (let ((after-refresh (length (ak-test-commands))))
               (list
                :commands (list :after-enable after-enable :after-refresh after-refresh)
                :announced (length (ak-test-messages
                                    "^Analysing project for completion\\.\\.\\.done$"))
                :still-one-root (hash-table-count anakondo--cache)
                :interactive (and (commandp 'anakondo-refresh-project-cache) t)
                :refused-when-off
                (progn (anakondo-minor-mode -1)
                       (condition-case error
                           (progn (anakondo-refresh-project-cache) :no-signal)
                         (error (list :signal (car error) :data (cdr error)))))
                :commands-after-refusal (length (ak-test-commands)))))))
    "##,
        expect![[
            r#"OK (:commands (:after-enable 3 :after-refresh 6) :announced 2 :still-one-root 1 :interactive t :refused-when-off (:signal error :data ("Anakondo minor mode not on in current buffer")) :commands-after-refusal 6)"#
        ]],
    )
    .fresh_process()
}

fn turning_the_mode_off_releases_the_project_cache_and_unhooks_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "turning_the_mode_off_releases_the_project_cache_and_unhooks_completion",
        r##"
        ;; Turning the mode off has to give the memory back and stop answering
        ;; completion, and turning it on again has to rebuild from scratch.
        ;; A second buffer in the same project must keep its own hook while
        ;; sharing the one analysis.
        (ak-test-with-project
         (let ((second nil))
           (unwind-protect
               (progn
                 (anakondo-minor-mode 1)
                 (list
                  :with-one-buffer (list :roots (hash-table-count anakondo--cache)
                                         :capf completion-at-point-functions)
                  :second-buffer
                  (progn (setq second (ak-test-visit "src/inventory/util.clj"))
                         (anakondo-minor-mode 1)
                         (list :roots (hash-table-count anakondo--cache)
                               :capf completion-at-point-functions
                               :candidates (progn (goto-char (point-max))
                                                  (insert "\n(defn a [] (apply-disc")
                                                  (ak-test-candidates))))
                  :second-off (progn (anakondo-minor-mode -1)
                                     (list :on anakondo-minor-mode
                                           :capf completion-at-point-functions
                                           :roots (hash-table-count anakondo--cache)
                                           :candidates (ak-test-candidates)))
                  :first-still-analysed
                  (with-current-buffer (get-file-buffer
                                        (expand-file-name "src/inventory/core.clj"
                                                          (ak-test-path "project/")))
                    (list :on anakondo-minor-mode
                          :capf completion-at-point-functions))))
             (when (buffer-live-p second)
               (with-current-buffer second (set-buffer-modified-p nil))
               (kill-buffer second)))))
    "##,
        expect![[
            r#"OK (:with-one-buffer (:roots 1 :capf #1=(anakondo-completion-at-point t)) :second-buffer (:roots 1 :capf (anakondo-completion-at-point t) :candidates (:start-column 12 :prefix "apply-disc" :candidates ("apply-discount"))) :second-off (:on nil :capf (tags-completion-at-point-function) :roots 0 :candidates nil) :first-still-analysed (:on t :capf #1#))"#
        ]],
    )
}

fn a_clj_kondo_that_cannot_run_leaves_the_mode_on_with_nothing_analysed() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_clj_kondo_that_cannot_run_leaves_the_mode_on_with_nothing_analysed",
        r##"
        ;; A clj-kondo that is installed but fails - a bad config, an
        ;; unreadable classpath, a broken install - writes to stderr and exits
        ;; non-zero, so the package's `json-read' meets the shell's error text.
        ;; anakondo does not catch that, and the user is left with the mode on,
        ;; completion hooked in, an empty cache and only the first half of the
        ;; progress message.  Pinning the half-open state is the point: it is
        ;; what the user actually gets.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (ak-test-setup ak-test-broken-clj-kondo-script)
                (setq buffer (ak-test-visit))
                (list
                 :enabling (condition-case error
                               (progn (anakondo-minor-mode 1) :no-signal)
                             (error (list :signal (car error) :data (cdr error))))
                 :mode-left-on anakondo-minor-mode
                 :capf completion-at-point-functions
                 :roots (hash-table-count anakondo--cache)
                 :caches-empty (let ((root (car (hash-table-keys anakondo--cache))))
                                 (mapcar (lambda (key)
                                           (list (symbol-name key)
                                                 (hash-table-count
                                                  (gethash key (gethash root anakondo--cache)))))
                                         '(:var-def-cache :ns-def-cache
                                           :ns-usage-cache :java-classes-cache)))
                 :process-buffer-cleaned (and (get-buffer "*anakondo*") t)
                 :completion (condition-case error
                                 (list :candidates (ak-test-candidates))
                               (error (list :signal (car error) :data (cdr error))))
                 :commands (ak-test-commands)
                 :messages (ak-test-messages "^Analysing project for completion\\.\\.\\..*$")))
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))
            (ak-test-teardown)))
    "##,
        expect![[
            r#"OK (:enabling (:signal json-readtable-error :data (99)) :mode-left-on t :capf (anakondo-completion-at-point t) :roots 1 :caches-empty ((":var-def-cache" 0) (":ns-def-cache" 0) (":ns-usage-cache" 0) (":java-classes-cache" 0)) :process-buffer-cleaned nil :completion (:signal json-readtable-error :data (99)) :commands (("-Spath") ("--lint" "[ORACLE-SANDBOX]/project/src:[ORACLE-SANDBOX]/project/lib/warehouse.jar\\n" "--config" "{:output {:analysis true :format :json}}" "--lang" "clj") ("--lint" "-" "--config" "{:output {:analysis true :format :json}}" "--lang" "clj")) :messages ("Analysing project for completion..."))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        turning_the_mode_on_analyses_the_project_with_the_documented_commands(),
        completing_a_var_pipes_the_unsaved_buffer_to_clj_kondo(),
        completing_an_aliased_namespace_qualifies_every_var_with_the_alias(),
        java_classes_and_public_static_members_come_from_a_real_jar(),
        clojure_default_imports_are_offered_but_cannot_be_resolved_on_a_modern_jdk(),
        refreshing_rebuilds_the_analysis_and_refuses_when_the_mode_is_off(),
        turning_the_mode_off_releases_the_project_cache_and_unhooks_completion(),
        a_clj_kondo_that_cannot_run_leaves_the_mode_on_with_nothing_analysed(),
    ]
}
