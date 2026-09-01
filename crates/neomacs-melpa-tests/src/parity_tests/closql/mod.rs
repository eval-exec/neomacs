use std::time::Duration;

use expect_test::expect;

use crate::{CLOSQL_MELPA_PIN, CachedMelpaOracle, SQLITE3_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CLOSQL_TEST_TIMEOUT: Duration = Duration::from_secs(30);

const CLOSQL_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(let ((load-suffixes (append load-suffixes (list module-file-suffix))))
  (require 'sqlite3))
(require 'closql)

(defvar neomacs-closql-test-database-file nil)

(defconst neomacs-closql-test-schemata
  '((work-item
     [(class :not-null)
      (id integer :not-null :primary-key)
      title
      state
      priority
      metadata
      (tags :default eieio-unbound)
      (channels :default eieio-unbound)
      (tasks :default eieio-unbound)
      (reviewers :default eieio-unbound)])
    (work-item-tag
     [(work-item integer :not-null)
      (tag :not-null)]
     (:foreign-key [work-item] :references work-item [id]
      :on-delete :cascade))
    (work-item-channel
     [(work-item integer :not-null)
      (channel :not-null)
      status]
     (:foreign-key [work-item] :references work-item [id]
      :on-delete :cascade))
    (task
     [(class :not-null)
      (id integer :not-null :primary-key)
      (work-item integer :not-null)
      sequence
      summary
      status]
     (:foreign-key [work-item] :references work-item [id]
      :on-delete :cascade))
    (reviewer
     [workspace
      (id :not-null :primary-key)
      name
      role])
    (work-item-reviewer
     [(work-item integer :not-null)
      (id :not-null)]
     (:foreign-key [work-item] :references work-item [id]
      :on-delete :cascade)
     (:foreign-key [id] :references reviewer [id]
      :on-delete :cascade))))

(defclass neomacs-closql-test-work-item (closql-object)
  ((closql-class-prefix :initform "neomacs-closql-test-" :allocation :class)
   (closql-table :initform 'work-item :allocation :class)
   (closql-primary-key :initform 'id :allocation :class)
   (closql-order-by :initform [(asc priority) (asc id)] :allocation :class)
   (id :initarg :id)
   (title :initarg :title)
   (state :initarg :state)
   (priority :initarg :priority)
   (metadata :initarg :metadata)
   (tags :initarg :tags :closql-table work-item-tag)
   (channels :initarg :channels :closql-table work-item-channel)
   (tasks :initarg :tasks :closql-class neomacs-closql-test-task)
   (reviewers :initarg :reviewers
              :closql-tables (work-item-reviewer reviewer)))
  :abstract t)

(defclass neomacs-closql-test-release
  (neomacs-closql-test-work-item) () :abstract t)
(defclass neomacs-closql-test-release-production
  (neomacs-closql-test-release) ())
(defclass neomacs-closql-test-release-canary
  (neomacs-closql-test-release) ())
(defclass neomacs-closql-test-incident
  (neomacs-closql-test-work-item) ())

(defclass neomacs-closql-test-task (closql-object)
  ((closql-class-prefix :initform "neomacs-closql-test-" :allocation :class)
   (closql-table :initform 'task :allocation :class)
   (closql-primary-key :initform 'id :allocation :class)
   (closql-foreign-key :initform 'work-item :allocation :class)
   (closql-order-by :initform [(asc sequence) (asc id)] :allocation :class)
   (id :initarg :id)
   (work-item :initarg :work-item)
   (sequence :initarg :sequence)
   (summary :initarg :summary)
   (status :initarg :status)))

(defclass neomacs-closql-test-database (closql-database)
  ((name :initform "Release Store")
   (object-class :initform 'neomacs-closql-test-work-item)
   (file :initform 'neomacs-closql-test-database-file)
   (schemata :initform 'neomacs-closql-test-schemata)
   (version :initform 3)))

(defun neomacs-closql-test-db (&optional livep)
  "Return the test database using the exact module-backed SQLite backend."
  (closql-db 'neomacs-closql-test-database
             livep
             (emacsql-sqlite-default-connection t)))

(defun neomacs-closql-test-close ()
  "Close the test singleton without creating a connection."
  (when-let ((db (ignore-errors (neomacs-closql-test-db t))))
    (when (emacsql-live-p db)
      (emacsql-close db))))

(defun neomacs-closql-test-reset-class-state ()
  "Restore mutable class-allocation state shared by batch cases."
  (neomacs-closql-test-close)
  (oset-default 'neomacs-closql-test-database disabled nil)
  (oset-default 'neomacs-closql-test-database version 3)
  (setq neomacs-closql-test-database-file nil))

(defun neomacs-closql-test-root (name)
  "Create deterministic sandbox root NAME after closing the prior database."
  (neomacs-closql-test-reset-class-state)
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "closql-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-closql-test-cleanup (root)
  "Close the test database, reset class state, and delete ROOT."
  (neomacs-closql-test-reset-class-state)
  (when (and root (file-exists-p root))
    (delete-directory root t)))

(defun neomacs-closql-test-raw (db sql &rest args)
  "Run SQL against DB's real connection without Closql row conversion."
  (apply #'emacsql (oref db connection) sql args))

(defun neomacs-closql-test-describe-work-item (object)
  "Describe the public scalar state of a work-item OBJECT."
  (list :class (eieio-object-class-name object)
        :id (oref object id)
        :title (oref object title)
        :state (oref object state)
        :priority (oref object priority)
        :metadata (oref object metadata)))

(defun neomacs-closql-test-describe-task (object)
  "Describe the public state of a deployment task OBJECT."
  (list :class (eieio-object-class-name object)
        :id (oref object id)
        :work-item (oref object work-item)
        :sequence (oref object sequence)
        :summary (oref object summary)
        :status (oref object status)))

(defun neomacs-closql-test-cache-bound-p (object slot)
  "Report whether indirect SLOT is cached, without triggering its load."
  ;; Closql advises `eieio-oref', which `slot-boundp' calls internally, so
  ;; `slot-boundp' itself resolves an uncached relation and always reports it
  ;; as bound.  Use Closql's raw accessor to observe the pre-load sentinel.
  (not (eq (closql--oref object slot) eieio--unbound)))

(defun neomacs-closql-test-condition (condition)
  "Describe CONDITION without discarding its exact data or message."
  (list :condition (car condition)
        :data (cdr condition)
        :message (error-message-string condition)))

(defun neomacs-closql-test-object-tag (object)
  "Describe OBJECT's raw EIEIO class tag without printing the class object."
  (let ((tag (aref object 0)))
    (list :kind (cond ((symbolp tag) 'symbol)
                      ((eieio--class-p tag) 'class-object)
                      (t (type-of tag)))
          :class (eieio-object-class-name object)
          :object-p (eieio-object-p object))))

(defun neomacs-closql-test-table-counts (db)
  "Return deterministic row counts for the release aggregate tables in DB."
  (mapcar
   (lambda (table)
     (list table
           (caar (emacsql db
                          [:select (funcall count *) :from $i1]
                          table))))
   '(work-item work-item-tag work-item-channel task work-item-reviewer)))
"####;

fn closql_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CLOSQL_MELPA_PIN, "closql.el")
        .expect("prepare exact Closql source and manifest dependencies below ./tmp")
        .with_melpa_dependency(SQLITE3_MELPA_PIN)
        .expect("prepare the exact SQLite3 module backend below ./tmp")
        .with_prelude(CLOSQL_TEST_PRELUDE)
        .with_timeout(CLOSQL_TEST_TIMEOUT)
}

fn creates_and_reopens_a_release_store() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-closql-test-root "create-and-reopen"))
       (file (expand-file-name "state/releases.sqlite" root))
       before-live db same title item created after-close reopened loaded)
  (unwind-protect
      (progn
        (setq neomacs-closql-test-database-file file
              before-live
              (and (neomacs-closql-test-db t) t)
              db (neomacs-closql-test-db)
              same (eq db (neomacs-closql-test-db))
              title (propertize "Deploy 東京 β" 'face 'bold 'release "R-42")
              item
              (neomacs-closql-test-release-production
               :id 42 :title title :state 'queued :priority 20
               :metadata '(:owner "Ada" :artifacts [linux windows])))
        (closql-insert db item)
        (setq created
              (list
               :before-live before-live
               :directory (file-directory-p (file-name-directory file))
               :file (file-exists-p file)
               :tables
               (sort (mapcar #'symbol-name (emacsql-sqlite-list-tables db))
                     #'string<)
               :version (caar (emacsql db [:pragma user-version]))
               :foreign-keys (caar (emacsql db [:pragma foreign-keys]))
               :connection-class
               (eieio-object-class-name (oref db connection))
               :same-singleton same
               :raw
               (neomacs-closql-test-raw
                db [:select * :from work-item :order-by [(asc id)]])
               :caller-title-properties (text-properties-at 0 title)))
        (emacsql-close db)
        (setq after-close
              (list :live (emacsql-live-p db)
                    :connection (oref db connection)
                    :live-only
                    (and (neomacs-closql-test-db t) t))
              reopened (neomacs-closql-test-db)
              loaded (closql-get reopened 42))
        (let ((formatted
               (closql-format loaded "%s#%s:%s" 'state 'id 'title)))
          (list
           :created created
           :after-close after-close
           :reopened
           (list
            :same-singleton (eq db reopened)
            :live (emacsql-live-p reopened)
            :item (neomacs-closql-test-describe-work-item loaded)
            :title-properties (text-properties-at 0 (oref loaded title))
            :formatted
            (list
             :text (substring-no-properties formatted)
             :property-range
             (list (next-property-change 0 formatted (length formatted))
                   (next-property-change 10 formatted (length formatted)))
             :properties (text-properties-at 10 formatted)
             :release-value-shared
             (eq (get-text-property 0 'release (oref loaded title))
                 (get-text-property 10 'release formatted)))))))
    (neomacs-closql-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:created (:before-live nil :directory t :file t :tables ("reviewer" "task" "work_item" "work_item_channel" "work_item_reviewer" "work_item_tag") :version 3 :foreign-keys 1 :connection-class emacsql-sqlite-module-connection :same-singleton t :raw ((release-production 42 #("Deploy 東京 β" 0 11 (face bold release "R-42")) queued 20 (:owner "Ada" :artifacts [linux windows]) eieio-unbound eieio-unbound eieio-unbound eieio-unbound)) :caller-title-properties (face bold release "R-42")) :after-close (:live nil :connection nil :live-only nil) :reopened (:same-singleton t :live t :item (:class neomacs-closql-test-release-production :id 42 :title #("Deploy 東京 β" 0 11 (face bold release "R-42")) :state queued :priority 20 :metadata (:owner "Ada" :artifacts [linux windows])) :title-properties (face bold release "R-42") :formatted (:text "queued#42:Deploy 東京 β" :property-range (10 21) :properties (release "R-42" face bold) :release-value-shared t)))"#
    ]];
    ParityBatchCase::value("creates_and_reopens_a_release_store", elisp_form, expected)
}

fn queries_a_polymorphic_release_queue_in_declared_order() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-closql-test-root "polymorphic-query"))
       (file (expand-file-name "state/releases.sqlite" root))
       db)
  (unwind-protect
      (progn
        (setq neomacs-closql-test-database-file file
              db (neomacs-closql-test-db))
        (dolist
            (object
             (list
              (neomacs-closql-test-release-production
               :id 501 :title "Production rollout" :state 'ready :priority 30
               :metadata '(:owner "Ada" :region "東京"))
              (neomacs-closql-test-release-canary
               :id 502 :title "Canary rollout" :state 'running :priority 10
               :metadata '(:owner "Grace" :percentage 5))
              (neomacs-closql-test-incident
               :id 503 :title "Rollback payments" :state 'open :priority 20
               :metadata '(:severity high :service payments))
              (neomacs-closql-test-release-production
               :id 504 :title "Documentation publish" :state 'queued :priority 10
               :metadata '(:owner "Lin" :locale "fr-FR"))))
          (closql-insert db object))
        (let* ((entries (closql-query db))
               (no-db-error
                (condition-case condition
                    (progn
                      (closql-where-class-in '[release*])
                      :unexpected-success)
                  (error
                   (neomacs-closql-test-condition condition)))))
          (list
           :entries (mapcar #'neomacs-closql-test-describe-work-item entries)
           :titles (closql-query db 'title)
           :rows (closql-select db '[id title priority])
           :production
           (closql-query
            db '[id title]
            '(neomacs-closql-test-release-production-p))
           :all-releases
           (closql-query
            db '[id class]
            '(neomacs-closql-test-release--eieio-childp))
           :production-by-vector
           (closql-query db '[id class] '[release* !release-canary])
           :vector-abbreviations
           (closql-where-class-in '[release* !release-canary] db)
           :raw-classes
           (neomacs-closql-test-raw
            db [:select [id class] :from work-item :order-by [(asc id)]])
           :formatted
           (closql-format (car entries) "%s | P%s | %s"
                          'state 'priority 'title)
           :no-db-error no-db-error)))
    (neomacs-closql-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:entries ((:class neomacs-closql-test-release-canary :id 502 :title "Canary rollout" :state running :priority 10 :metadata (:owner "Grace" :percentage 5)) (:class neomacs-closql-test-release-production :id 504 :title "Documentation publish" :state queued :priority 10 :metadata (:owner "Lin" :locale "fr-FR")) (:class neomacs-closql-test-incident :id 503 :title "Rollback payments" :state open :priority 20 :metadata (:severity high :service payments)) (:class neomacs-closql-test-release-production :id 501 :title "Production rollout" :state ready :priority 30 :metadata (:owner "Ada" :region "東京"))) :titles ("Canary rollout" "Documentation publish" "Rollback payments" "Production rollout") :rows ((502 "Canary rollout" 10) (504 "Documentation publish" 10) (503 "Rollback payments" 20) (501 "Production rollout" 30)) :production ((504 "Documentation publish") (501 "Production rollout")) :all-releases ((502 release-canary) (504 release-production) (501 release-production)) :production-by-vector ((504 release-production) (501 release-production)) :vector-abbreviations [release-production] :raw-classes ((501 release-production) (502 release-canary) (503 incident) (504 release-production)) :formatted "running | P10 | Canary rollout" :no-db-error (:condition error :data ("closql-where-class-in: DB cannot be nil if ARGS is a vector") :message "closql-where-class-in: DB cannot be nil if ARGS is a vector"))"#
    ]];
    ParityBatchCase::value(
        "queries_a_polymorphic_release_queue_in_declared_order",
        elisp_form,
        expected,
    )
}

fn loads_and_reconciles_release_relations_lazily() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-closql-test-root "lazy-relations"))
       (file (expand-file-name "state/releases.sqlite" root))
       db item loaded first task-oset-error task-cache-after-insert
       reviewer-error persisted)
  (unwind-protect
      (progn
        (setq neomacs-closql-test-database-file file
              db (neomacs-closql-test-db)
              item
              (neomacs-closql-test-release-production
               :id 100 :title "Release 2.4.1" :state 'queued :priority 10
               :metadata '(:train "August" :owner "Ada")
               :tags '("ui" "backend")
               :channels '(("stable" active) ("canary" paused))))
        (closql-insert db item)
        (dolist
            (task
             (list
              (neomacs-closql-test-task
               :id 1002 :work-item 100 :sequence 2
               :summary "Publish artifacts" :status 'waiting)
              (neomacs-closql-test-task
               :id 1001 :work-item 100 :sequence 1
               :summary "Run smoke tests" :status 'done)))
          (closql-insert db task))
        (emacsql
         db [:insert-into reviewer :values $v1]
         '([release "alice" "Alice Ops" approver]
           [release "bob" "Bob Reviewer" reviewer]))
        (oset item reviewers '("bob" "alice"))
        (emacsql-close db)
        (setq db (neomacs-closql-test-db)
              loaded (closql-get db 100))
        (let* ((cache-before
                (mapcar
                 (lambda (slot)
                   (list slot
                         (neomacs-closql-test-cache-bound-p loaded slot)))
                 '(tags channels tasks reviewers)))
               (tags (oref loaded tags))
               (channels (oref loaded channels))
               (tasks (oref loaded tasks))
               (reviewers (oref loaded reviewers)))
          (setq first
                (list
                 :cache-before cache-before
                 :tags (copy-tree tags)
                 :channels (copy-tree channels)
                 :tasks (mapcar #'neomacs-closql-test-describe-task tasks)
                 :reviewers (copy-tree reviewers)
                 :same-cache
                 (list (eq tags (oref loaded tags))
                       (eq channels (oref loaded channels))
                       (eq tasks (oref loaded tasks))
                       (eq reviewers (oref loaded reviewers)))))
          (setq task-oset-error
                (condition-case condition
                    (progn
                      (oset loaded tasks tasks)
                      :unexpected-success)
                  (error
                   (neomacs-closql-test-condition condition))))
          ;; Mutate the caller's cached tag list before assigning it.  Closql
          ;; must reload persisted rows before computing the secondary-table
          ;; diff, or the rename below would be lost.
          (setcar tags "api")
          (oset loaded tags (append tags '("security")))
          (oset loaded channels
                '(("edge" warming) ("canary" active)))
          (oset loaded state 'shipped)
          (closql-insert
           db
           (neomacs-closql-test-task
            :id 1003 :work-item 100 :sequence 3
            :summary "Notify customers" :status 'ready))
          (setq task-cache-after-insert
                (mapcar #'neomacs-closql-test-describe-task (oref loaded tasks))
                reviewer-error
                (condition-case condition
                    (progn
                      (closql-dset loaded 'reviewers '("alice" "missing"))
                      :unexpected-success)
                  (error
                   (neomacs-closql-test-condition condition))))
          (closql-dset loaded 'reviewers '("alice" "missing") t))
        (emacsql-close db)
        (setq db (neomacs-closql-test-db)
              loaded (closql-get db 100 nil t)
              persisted
              (list
               :item (neomacs-closql-test-describe-work-item loaded)
               :cache-after-resolve
               (mapcar
                (lambda (slot)
                  (list slot
                        (neomacs-closql-test-cache-bound-p loaded slot)))
                '(tags channels tasks reviewers))
               :tags (oref loaded tags)
               :channels (oref loaded channels)
               :tasks
               (mapcar #'neomacs-closql-test-describe-task (oref loaded tasks))
               :reviewers (oref loaded reviewers)
               :raw
               (list
                :tags
                (neomacs-closql-test-raw
                 db [:select * :from work-item-tag
                     :order-by [(asc work-item) (asc tag)]])
                :channels
                (neomacs-closql-test-raw
                 db [:select * :from work-item-channel
                     :order-by [(asc work-item) (asc channel)]])
                :tasks
                (neomacs-closql-test-raw
                 db [:select [id sequence status] :from task
                     :order-by [(asc sequence) (asc id)]])
                :reviewers
                (neomacs-closql-test-raw
                 db [:select * :from work-item-reviewer
                     :order-by [(asc work-item) (asc id)]]))))
        (list :first first
              :task-oset-error task-oset-error
              :task-cache-after-insert task-cache-after-insert
              :reviewer-error reviewer-error
              :persisted persisted))
    (neomacs-closql-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:first (:cache-before ((tags nil) (channels nil) (tasks nil) (reviewers nil)) :tags ("backend" "ui") :channels (("canary" paused) ("stable" active)) :tasks ((:class neomacs-closql-test-task :id 1001 :work-item 100 :sequence 1 :summary "Run smoke tests" :status done) (:class neomacs-closql-test-task :id 1002 :work-item 100 :sequence 2 :summary "Publish artifacts" :status waiting)) :reviewers (("alice" "Alice Ops" approver) ("bob" "Bob Reviewer" reviewer)) :same-cache (t t t t)) :task-oset-error (:condition error :data ("Not implemented for closql-class slots: oset") :message "Not implemented for closql-class slots: oset") :task-cache-after-insert ((:class neomacs-closql-test-task :id 1001 :work-item 100 :sequence 1 :summary "Run smoke tests" :status done) (:class neomacs-closql-test-task :id 1002 :work-item 100 :sequence 2 :summary "Publish artifacts" :status waiting)) :reviewer-error (:condition error :data ("Invalid error symbol" emacsql-constraint) :message "Invalid error symbol: emacsql-constraint") :persisted (:item (:class neomacs-closql-test-release-production :id 100 :title "Release 2.4.1" :state shipped :priority 10 :metadata (:train "August" :owner "Ada")) :cache-after-resolve ((tags t) (channels t) (tasks t) (reviewers t)) :tags ("api" "security" "ui") :channels (("canary" active) ("edge" warming)) :tasks ((:class neomacs-closql-test-task :id 1001 :work-item 100 :sequence 1 :summary "Run smoke tests" :status done) (:class neomacs-closql-test-task :id 1002 :work-item 100 :sequence 2 :summary "Publish artifacts" :status waiting) (:class neomacs-closql-test-task :id 1003 :work-item 100 :sequence 3 :summary "Notify customers" :status ready)) :reviewers (("alice" "Alice Ops" approver)) :raw (:tags ((100 "api") (100 "security") (100 "ui")) :channels ((100 "canary" active) (100 "edge" warming)) :tasks ((1001 1 done) (1002 2 waiting) (1003 3 ready)) :reviewers ((100 "alice")))))"#
    ]];
    ParityBatchCase::value(
        "loads_and_reconciles_release_relations_lazily",
        elisp_form,
        expected,
    )
}

fn replaces_reloads_and_deletes_a_synced_release() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-closql-test-root "replace-reload-delete"))
       (file (expand-file-name "state/releases.sqlite" root))
       db stale duplicate replacement current compatibility reload-error)
  (unwind-protect
      (progn
        (setq neomacs-closql-test-database-file file
              db (neomacs-closql-test-db)
              stale
              (neomacs-closql-test-release-production
               :id 700 :title "Release candidate" :state 'queued :priority 20
               :metadata '(:revision "abc123")
               :tags '("candidate" "linux")
               :channels '(("canary" active))))
        (closql-insert db stale)
        (closql-insert
         db
         (neomacs-closql-test-task
          :id 7001 :work-item 700 :sequence 1
          :summary "Verify checksums" :status 'done))
        (closql-insert
         db
         (neomacs-closql-test-incident
          :id 701 :title "Unrelated incident" :state 'open :priority 5
          :metadata '(:service auth)))
        (setq duplicate
              (condition-case condition
                  (progn
                    (closql-insert
                     db
                     (neomacs-closql-test-release-production
                      :id 700 :title "Duplicate" :state 'blocked :priority 99
                      :metadata nil))
                    :unexpected-success)
                (error
                 (neomacs-closql-test-condition condition)))
              replacement
              (neomacs-closql-test-release-production
               :id 700 :title "Release published" :state 'shipped :priority 1
               :metadata '(:revision "def456" :artifacts 4)))
        (closql-insert db replacement t)
        (setq current (closql-reload stale)
              compatibility
              (list
               :modern
               (let* ((eieio-backward-compatibility nil)
                      (object (closql-get db 700)))
                 (list :tag (neomacs-closql-test-object-tag object)
                       :item
                       (neomacs-closql-test-describe-work-item object)))
               :legacy
               (let* ((eieio-backward-compatibility t)
                      (object (closql-get db 700)))
                 (list :tag (neomacs-closql-test-object-tag object)
                       :item
                       (neomacs-closql-test-describe-work-item object)))))
        (closql-delete current)
        (setq reload-error
              (condition-case condition
                  (progn (closql-reload current) :unexpected-success)
                (error
                 (neomacs-closql-test-condition condition))))
        (list
         :duplicate duplicate
         :stale (neomacs-closql-test-describe-work-item stale)
         :reloaded (neomacs-closql-test-describe-work-item current)
         :compatibility compatibility
         :after-delete
         (list :get (closql-get db 700)
               :unrelated
               (neomacs-closql-test-describe-work-item (closql-get db 701))
               :counts (neomacs-closql-test-table-counts db)
               :reload-error reload-error)))
    (neomacs-closql-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:duplicate (:condition error :data ("Invalid error symbol" emacsql-constraint) :message "Invalid error symbol: emacsql-constraint") :stale (:class neomacs-closql-test-release-production :id 700 :title "Release candidate" :state queued :priority 20 :metadata (:revision "abc123")) :reloaded (:class neomacs-closql-test-release-production :id 700 :title "Release published" :state shipped :priority 1 :metadata (:revision "def456" :artifacts 4)) :compatibility (:modern (:tag (:kind class-object :class neomacs-closql-test-release-production :object-p t) :item (:class neomacs-closql-test-release-production :id 700 :title "Release published" :state shipped :priority 1 :metadata (:revision "def456" :artifacts 4))) :legacy (:tag (:kind symbol :class neomacs-closql-test-release-production :object-p t) :item (:class neomacs-closql-test-release-production :id 700 :title "Release published" :state shipped :priority 1 :metadata (:revision "def456" :artifacts 4)))) :after-delete (:get nil :unrelated (:class neomacs-closql-test-incident :id 701 :title "Unrelated incident" :state open :priority 5 :metadata (:service auth)) :counts ((work-item 1) (work-item-tag 0) (work-item-channel 0) (task 0) (work-item-reviewer 0)) :reload-error (:condition error :data ("Cannot reload object") :message "Cannot reload object")))"#
    ]];
    ParityBatchCase::value(
        "replaces_reloads_and_deletes_a_synced_release",
        elisp_form,
        expected,
    )
}

fn rolls_back_an_atomic_release_import() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-closql-test-root "transaction"))
       (file (expand-file-name "state/releases.sqlite" root))
       db failure rolled-back committed)
  (unwind-protect
      (progn
        (setq neomacs-closql-test-database-file file
              db (neomacs-closql-test-db))
        (emacsql db [:insert-into reviewer :values $v1]
                 [release "alice" "Alice Ops" approver])
        (setq failure
              (condition-case condition
                  (closql-with-transaction db
                    (let ((release
                           (neomacs-closql-test-release-canary
                            :id 800 :title "Atomic canary" :state 'queued
                            :priority 2 :metadata '(:region "eu-west")
                            :tags '("canary" "database")
                            :channels '(("canary" paused)))))
                      (closql-insert db release)
                      (oset release reviewers '("alice"))
                      (closql-insert
                       db
                       (neomacs-closql-test-task
                        :id 8001 :work-item 800 :sequence 1
                        :summary "Apply migrations" :status 'running))
                      (oset release state 'validating)
                      (error "deployment validation failed")))
                (error
                 (neomacs-closql-test-condition condition)))
              rolled-back (neomacs-closql-test-table-counts db))
        (closql-with-transaction db
          (let ((release
                 (neomacs-closql-test-release-canary
                  :id 801 :title "Atomic canary" :state 'queued
                  :priority 2 :metadata '(:region "eu-west")
                  :tags '("canary" "database")
                  :channels '(("canary" paused)))))
            (closql-insert db release)
            (oset release reviewers '("alice"))
            (closql-insert
             db
             (neomacs-closql-test-task
              :id 8011 :work-item 801 :sequence 1
              :summary "Apply migrations" :status 'done))
            (closql-insert
             db
             (neomacs-closql-test-task
              :id 8012 :work-item 801 :sequence 2
              :summary "Publish traffic" :status 'ready))
            (oset release state 'shipped)))
        (let ((release (closql-get db 801 nil t)))
          (setq committed
                (list
                 :counts (neomacs-closql-test-table-counts db)
                 :item (neomacs-closql-test-describe-work-item release)
                 :tags (oref release tags)
                 :channels (oref release channels)
                 :tasks
                 (mapcar #'neomacs-closql-test-describe-task (oref release tasks))
                 :reviewers (oref release reviewers)
                 :connection-live (emacsql-live-p db))))
        (list :failure failure
              :rolled-back rolled-back
              :committed committed))
    (neomacs-closql-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:failure (:condition error :data ("deployment validation failed") :message "deployment validation failed") :rolled-back ((work-item 0) (work-item-tag 0) (work-item-channel 0) (task 0) (work-item-reviewer 0)) :committed (:counts ((work-item 1) (work-item-tag 2) (work-item-channel 1) (task 2) (work-item-reviewer 1)) :item (:class neomacs-closql-test-release-canary :id 801 :title "Atomic canary" :state shipped :priority 2 :metadata (:region "eu-west")) :tags ("canary" "database") :channels (("canary" paused)) :tasks ((:class neomacs-closql-test-task :id 8011 :work-item 801 :sequence 1 :summary "Apply migrations" :status done) (:class neomacs-closql-test-task :id 8012 :work-item 801 :sequence 2 :summary "Publish traffic" :status ready)) :reviewers (("alice" "Alice Ops" approver)) :connection-live t))"#
    ]];
    ParityBatchCase::value("rolls_back_an_atomic_release_import", elisp_form, expected)
}

fn reports_exact_newer_and_older_schema_gate_outcomes() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-closql-test-root "schema-versions"))
       (ahead-file (expand-file-name "ahead/releases.sqlite" root))
       (behind-file (expand-file-name "behind/releases.sqlite" root))
       ahead-db ahead-result ahead-message ahead-state
       behind-db behind-error behind-state)
  (unwind-protect
      (cl-labels
          ((inspect-file
            (file)
            (let ((connection (emacsql-sqlite-module file)))
              (unwind-protect
                  (list
                   :version (caar (emacsql connection [:pragma user-version]))
                   :rows
                   (emacsql connection
                            [:select [id title] :from work-item
                             :order-by [(asc id)]]))
                (emacsql-close connection)))))
        (setq neomacs-closql-test-database-file ahead-file
              ahead-db (neomacs-closql-test-db))
        (closql-insert
         ahead-db
         (neomacs-closql-test-release-production
          :id 901 :title "Future schema row" :state 'ready :priority 1
          :metadata nil))
        (emacsql ahead-db [:pragma (= user-version 4)])
        (emacsql-close ahead-db)
        (cl-letf (((symbol-function 'message)
                   (lambda (format-string &rest arguments)
                     (setq ahead-message
                           (apply #'format-message format-string arguments)))))
          (setq ahead-result (neomacs-closql-test-db)))
        (setq
              ahead-state
              (list
               :result ahead-result
               :message ahead-message
               :disabled
               (oref-default 'neomacs-closql-test-database disabled)
               :live-only
               (and (neomacs-closql-test-db t) t)
               :disk (inspect-file ahead-file)))
        (oset-default 'neomacs-closql-test-database disabled nil)
        (setq neomacs-closql-test-database-file behind-file
              behind-db (neomacs-closql-test-db))
        (closql-insert
         behind-db
         (neomacs-closql-test-release-canary
          :id 902 :title "Old schema row" :state 'queued :priority 2
          :metadata nil))
        (emacsql behind-db [:pragma (= user-version 2)])
        (emacsql-close behind-db)
        (setq behind-error
              (condition-case condition
                  (progn
                    (neomacs-closql-test-db)
                    :unexpected-success)
                (error
                 (list
                  :condition (car condition)
                  :expected-type (nth 1 condition)
                  :object-class
                  (and (eieio-object-p (nth 2 condition))
                       (eieio-object-class-name (nth 2 condition)))
                  :checked-form (nth 3 condition))))
              behind-state
              (list
               :error behind-error
               :disabled
               (oref-default 'neomacs-closql-test-database disabled)
               :live-only
               (and (neomacs-closql-test-db t) t)
               :disk (inspect-file behind-file)))
        (list :ahead ahead-state :behind behind-state))
    (neomacs-closql-test-cleanup root)))
"####;
    let expected = expect![[
        r#"OK (:ahead (:result nil :message "Please update Release Store package (database schema version 3 < 4)" :disabled t :live-only nil :disk (:version 4 :rows ((901 "Future schema row")))) :behind (:error (:condition wrong-type-argument :expected-type eieio--class :object-class neomacs-closql-test-database :checked-form class) :disabled nil :live-only t :disk (:version 2 :rows ((902 "Old schema row")))))"#
    ]];
    ParityBatchCase::value(
        "reports_exact_newer_and_older_schema_gate_outcomes",
        elisp_form,
        expected,
    )
}

#[test]
fn closql_package_batch() {
    let cases = [
        creates_and_reopens_a_release_store(),
        queries_a_polymorphic_release_queue_in_declared_order(),
        loads_and_reconciles_release_relations_lazily(),
        replaces_reloads_and_deletes_a_synced_release(),
        rolls_back_an_atomic_release_import(),
        reports_exact_newer_and_older_schema_gate_outcomes(),
    ];
    assert_oracle_batch_cases(
        closql_oracle(),
        "closql-package-batch",
        "closql parity",
        &cases,
    );
}
