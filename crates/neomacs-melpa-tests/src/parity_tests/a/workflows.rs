use expect_test::expect;

use super::ParityBatchCase;

/// All public `a` workflows in one dual-editor process pair (multi-probe batch).
///
/// Setup (`package-initialize` + load `a.el`) runs once per editor; each case
/// keeps its own expect-test snapshot and OK/ERR expectation.
fn reads_nested_player_profile() -> ParityBatchCase {
    ParityBatchCase::value(
        "reads_nested_player_profile",
        r##"(let ((profile
       (a-list
        :name "Ärne Brasseur"
        :handle "plexus"
        :stats (a-list :score 99 :streak 4)
        :log (a-hash-table
              "2021-09-29" (a-list :puzzle "λ-calculus" :points 42)
              "2021-09-30" (a-list :puzzle "α-conversion" :points 17))
        :tags ["clojure" "emacs" "λ"])))
  (list
   (a-associative? profile)
   (a-associative? (a-get profile :log))
   (a-associative? (a-get profile :tags))
   (a-count profile)
   (a-count (a-get profile :log))
   (a-count (a-get profile :tags))
   (a-keys profile)
   (a-vals (a-get profile :stats))
   (a-get profile :handle)
   (a-get profile :nickname)
   (a-get profile :nickname :anonymous)
   (a-get-in profile [:stats :score])
   (a-get* profile :log "2021-09-29" :points)
   (a-get-in profile [:log "2021-09-30" :puzzle])
   (a-get-in profile [:tags 2])
   (a-get-in profile [:stats :level] :unranked)
   (a-get-in profile [:log "2021-10-01" :points] :no-entry)
   (a-has-key? profile :stats)
   (a-has-key? profile :level)
   (a-has-key? (a-get profile :tags) 2)
   (a-has-key? (a-get profile :tags) 3)
   (sort (a-keys (a-get profile :log)) #'string<)
   (a-reduce-kv
    (lambda (acc key value)
      (cons (format "%s=%d" (substring (symbol-name key) 1) value) acc))
    nil
    (a-get profile :stats))))"##,
        expect![[
            r#"OK (t t nil 5 2 3 (:name :handle :stats :log :tags) (99 4) "plexus" nil :anonymous 99 42 "α-conversion" "λ" :unranked :no-entry t nil t nil ("2021-09-29" "2021-09-30") ("streak=4" "score=99"))"#
        ]],
    )
}

fn immutable_edit_pipeline() -> ParityBatchCase {
    ParityBatchCase::value(
        "immutable_edit_pipeline",
        r##"(let* ((board
        (a-list
         :title "Sprint 42"
         :columns (a-list
                   "todo" ["write docs" "fix parser"]
                   "doing" ["review α patch"])
         :owners (a-list :lead "Ärne")))
       (renamed (a-update board :title #'concat " (final)"))
       (promoted (a-update-in board [:columns "todo" 1] #'upcase))
       (assigned (a-assoc-in board [:owners :reviewer] "Bo"))
       (created (a-assoc-in board [:metrics :velocity :median] 21))
       (tagged (a-assoc board :status :active :title "Sprint 43"))
       (counted (a-update board :reviews #'ignore))
       (unchanged (a-assoc-in board [] :ignored)))
  (list
   (a-get renamed :title)
   (a-get-in renamed [:columns "todo"])
   (a-get-in promoted [:columns "todo"])
   (a-get-in promoted [:columns "doing"])
   (a-get promoted :title)
   (a-get assigned :owners)
   (a-keys assigned)
   (a-get created :metrics)
   (a-keys created)
   (a-keys tagged)
   (a-get tagged :title)
   (a-get tagged :status)
   (a-keys counted)
   (a-has-key? counted :reviews)
   (a-get counted :reviews)
   (eq unchanged board)
   board
   (equal board
          (a-list
           :title "Sprint 42"
           :columns (a-list
                     "todo" ["write docs" "fix parser"]
                     "doing" ["review α patch"])
           :owners (a-list :lead "Ärne")))))"##,
        expect![[
            r#"OK ("Sprint 42 (final)" #1=["write docs" "fix parser"] ["write docs" "FIX PARSER"] #2=["review α patch"] "Sprint 42" ((:reviewer . "Bo") . #3=((:lead . "Ärne"))) (:title :columns :owners) ((:velocity (:median . 21))) (:metrics :title :columns :owners) (:status :title :columns :owners) "Sprint 43" :active (:reviews :title :columns :owners) t nil t ((:title . "Sprint 42") (:columns ("todo" . #1#) ("doing" . #2#)) (:owners . #3#)) t)"#
        ]],
    )
}

fn merges_layered_configuration() -> ParityBatchCase {
    ParityBatchCase::value(
        "merges_layered_configuration",
        r##"(let* ((defaults (a-list :indent 2 :theme "light" :plugins ["core"]))
       (user (a-hash-table :theme "dark" :font "Iosevka"))
       (project (a-list :indent 4 :strict t))
       (settings (a-merge defaults user project))
       (timings-a (a-list :parse 12 :render 5))
       (timings-b (a-list :parse 7 :gc 3))
       (combined (a-merge-with #'+ timings-a timings-b))
       (notes (a-merge-with #'concat (a-list :release "α") (a-list :release "β"))))
  (list
   settings
   (a-keys settings)
   (a-get settings :theme)
   (a-get settings :indent)
   combined
   notes
   (a-merge)
   (a-merge nil defaults)
   (a-merge defaults)
   defaults
   project
   timings-a
   (sort (a-keys user) #'string<)
   (a-reduce-kv
    (lambda (acc key value)
      (concat acc (format "%s=%s;" key value)))
    ""
    (a-dissoc settings :plugins))))"##,
        expect![[
            r#"OK (((:strict . t) (:font . "Iosevka") (:indent . 4) (:theme . "dark") #2=(:plugins . #1=["core"])) (:strict :font :indent :theme :plugins) "dark" 4 ((:gc . 3) (:parse . 19) #4=(:render . 5)) ((:release . "βα")) nil ((:plugins . #1#) (:theme . "light") (:indent . 2)) #3=((:indent . 2) (:theme . "light") #2#) #3# ((:indent . 4) (:strict . t)) ((:parse . 12) #4#) (:font :theme) ":theme=dark;:indent=4;:font=Iosevka;:strict=t;")"#
        ]],
    )
}

fn redacts_secret_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "redacts_secret_keys",
        r##"(let* ((request
        (a-list
         :url "https://example.invalid/résumé"
         :token "s3cr3t"
         :retries 3
         :headers (a-list "Accept" "application/json" "Authorization" "Bearer s3cr3t")))
       (audit (a-hash-table :user "ärne" :token "s3cr3t" :ip "203.0.113.7"))
       (safe-request (a-dissoc request :token))
       (safe-headers (a-dissoc (a-get request :headers) "Authorization"))
       (safe-audit (a-dissoc audit :token)))
  (list
   safe-request
   safe-headers
   (a-count safe-request)
   (a-has-key? safe-request :token)
   (a-get safe-request :token :redacted)
   (a-get safe-request :url)
   (sort (a-keys safe-audit) #'string<)
   (a-count safe-audit)
   (hash-table-test safe-audit)
   (a-get safe-audit :token :redacted)
   (a-get safe-audit :user)
   (a-count audit)
   (a-get audit :token)
   request
   (a-dissoc request :nope)
   (a-dissoc request)
   (a-dissoc request :url :token :retries :headers)
   (a-dissoc ["keep" "me"] 0)))"##,
        expect![[
            r#"OK (((:headers . #1=(("Accept" . "application/json") ("Authorization" . "Bearer s3cr3t"))) (:retries . 3) (:url . "https://example.invalid/résumé")) (("Accept" . "application/json")) 3 nil :redacted "https://example.invalid/résumé" (:ip :user) 2 equal :redacted "ärne" 3 "s3cr3t" ((:url . "https://example.invalid/résumé") (:token . "s3cr3t") (:retries . 3) (:headers . #1#)) ((:headers . #1#) (:retries . 3) (:token . "s3cr3t") (:url . "https://example.invalid/résumé")) ((:headers . #1#) (:retries . 3) (:token . "s3cr3t") (:url . "https://example.invalid/résumé")) nil nil)"#
        ]],
    )
}

fn deep_equality_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "deep_equality_shapes",
        r##"(let* ((expected
        (a-list
         :id 42
         :labels ["bug" "α"]
         :author (a-list :name "Ärne" :roles '("dev" "maintainer"))))
       (decoded
        (a-hash-table
         :author (a-hash-table :roles ["dev" "maintainer"] :name "Ärne")
         :labels '("bug" "α")
         :id 42))
       (renumbered (a-assoc expected :id 43))
       (relabelled (a-assoc expected :labels ["bug" "β"]))
       (extended (a-assoc expected :draft nil)))
  (list
   (a-equal expected decoded)
   (a-equal decoded expected)
   (a-equal? expected decoded)
   (equal expected decoded)
   (a-equal expected renumbered)
   (a-equal expected relabelled)
   (a-equal expected extended)
   (a-equal expected (a-dissoc expected :labels))
   (a-equal '((:a . 1) (:b . 2)) '((:b . 2) (:a . 1)))
   (a-equal '(((:position . 5))) '(((:position . 15))))
   (a-equal '(1 2 3) [1 2 3])
   (a-equal '(1 2 3 4) [1 2 3])
   (a-equal "αβ" "αβ")
   (a-equal "αβ" "αγ")
   (a-equal 42 42)
   (a-equal nil nil)
   (a-equal nil '())
   (a-count expected)
   (a-count decoded)))"##,
        expect![[r#"OK (t t t nil nil nil nil nil t nil t nil t nil t t t 3 3)"#]],
    )
}

fn misuse_reports_exact_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "misuse_reports_exact_errors",
        r##"(let ((observed nil))
  (dolist (probe
           (list
            (cons 'get-integer (lambda () (a-get 5 :nope)))
            (cons 'get-string (lambda () (a-get "config" :nope)))
            (cons 'has-key-integer (lambda () (a-has-key? 1 :nope)))
            (cons 'get-in-leaf (lambda () (a-get-in (a-list :path "/etc") [:path :missing])))
            (cons 'assoc-odd (lambda () (a-assoc (a-list :a 1) :b)))))
    (push (cons (car probe)
                (condition-case error
                    (list 'value (funcall (cdr probe)))
                  (error (list 'signal (car error) (cdr error)))))
          observed))
  (list
   (nreverse observed)
   (a-get [1 2 3] 1)
   (a-get [1 2 3] 5)
   (a-get [1 2 3] 5 :fallback)
   (a-has-key [1 2 3] 2)
   (a-has-key [1 2 3] 3)
   (a-has-key [1 2 3] -1)
   (a-has-key [1 2 3] :label)
   (a-assoc [1 2 3] 5 :late)
   (a-assoc [1 2 3] -1 :never)
   (a-get (a-list :flag :not-found) :flag)
   (a-has-key (a-list :flag :not-found) :flag)
   (a-get-in [] [])
   (a-get-in [] [2] :missing)
   (a-keys ["not" "associative"])
   (a-vals ["not" "associative"])))"##,
        expect![[
            r#"OK (((get-integer signal user-error ("Not associative: 5")) (get-string signal user-error ("Not associative: \"config\"")) (has-key-integer signal user-error ("Not associative: 1")) (get-in-leaf signal user-error ("Not associative: \"/etc\"")) (assoc-odd signal user-error ("a-assoc requires an even number of arguments!"))) 2 nil :fallback t nil nil nil [1 2 3 nil nil :late] nil :not-found nil [] :missing nil nil)"#
        ]],
    )
}

fn walking_past_leaf_signals() -> ParityBatchCase {
    ParityBatchCase::signal(
        "walking_past_leaf_signals",
        r##"(a-get-in (a-list :config "/etc/α.conf") [:config :missing])"##,
        expect![[r#"ERR (user-error "Not associative: \"/etc/α.conf\"")"#]],
    )
}

pub(super) fn a_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        reads_nested_player_profile(),
        immutable_edit_pipeline(),
        merges_layered_configuration(),
        redacts_secret_keys(),
        deep_equality_shapes(),
        misuse_reports_exact_errors(),
        walking_past_leaf_signals(),
    ]
}
