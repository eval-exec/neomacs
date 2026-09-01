//! Oracle parity tests for advanced property list patterns:
//! deep plist-get/plist-put chains, plists as lightweight records,
//! plist-member existence checking, plist<->alist conversion,
//! plist merging, schema validation, and message serialization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// plist-get/plist-put deep chains building complex objects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_deep_chain_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((build-person
             (lambda (name age email hobbies address)
               (let* ((pl nil)
                      (pl (plist-put pl :name name))
                      (pl (plist-put pl :age age))
                      (pl (plist-put pl :email email))
                      (pl (plist-put pl :hobbies hobbies))
                      ;; address is itself a plist
                      (pl (plist-put pl :address address))
                      ;; derived fields
                      (pl (plist-put pl :adult (>= age 18)))
                      (pl (plist-put pl :hobby-count (length hobbies)))
                      ;; computed display name
                      (pl (plist-put pl :display
                                    (format "%s <%s>" name email))))
                 pl)))
            (deep-get
             (lambda (pl &rest keys)
               ;; Navigate nested plists with a key path
               (let ((current pl))
                 (dolist (k keys)
                   (setq current (plist-get current k)))
                 current))))
        ;; Build nested objects
        (let* ((addr1 (list :street "123 Main St" :city "Boston" :zip "02101"))
               (addr2 (list :street "456 Oak Ave" :city "NYC" :zip "10001"))
               (p1 (funcall build-person "Alice" 30 "alice@ex.com"
                            '(hiking coding) addr1))
               (p2 (funcall build-person "Bob" 17 "bob@ex.com"
                            '(gaming) addr2)))
          ;; Deep access via nested plist-get
          (list
           (plist-get p1 :name)
           (plist-get (plist-get p1 :address) :city)
           (funcall deep-get p1 :address :zip)
           (funcall deep-get p2 :address :street)
           (plist-get p1 :adult)
           (plist-get p2 :adult)
           (plist-get p1 :hobby-count)
           (plist-get p1 :display)
           ;; Mutate via plist-put chain
           (let* ((updated (plist-put (copy-sequence p2) :age 18))
                  (updated (plist-put updated :adult t)))
             (list (plist-get updated :age)
                   (plist-get updated :adult))))))
    (fmakunbound 'neovm--test-plist-deep-dummy)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" \"Boston\" \"02101\" \"456 Oak Ave\" t nil 2 \"Alice <alice@ex.com>\" (18 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Using plists as lightweight records/structs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_as_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ()
        ;; Define constructors, accessors, and predicate via closures
        (let ((make-point
               (lambda (x y)
                 (list :type 'point :x x :y y)))
              (make-rect
               (lambda (x y w h)
                 (list :type 'rect :x x :y y :width w :height h)))
              (shape-type (lambda (s) (plist-get s :type)))
              (point-x (lambda (p) (plist-get p :x)))
              (point-y (lambda (p) (plist-get p :y)))
              (rect-area
               (lambda (r)
                 (* (plist-get r :width) (plist-get r :height))))
              (rect-contains-point
               (lambda (r p)
                 (and (>= (plist-get p :x) (plist-get r :x))
                      (<= (plist-get p :x)
                          (+ (plist-get r :x) (plist-get r :width)))
                      (>= (plist-get p :y) (plist-get r :y))
                      (<= (plist-get p :y)
                          (+ (plist-get r :y) (plist-get r :height)))))))
          (let ((p1 (funcall make-point 5 10))
                (p2 (funcall make-point 50 50))
                (r1 (funcall make-rect 0 0 20 30)))
            (list
             (funcall shape-type p1)
             (funcall shape-type r1)
             (funcall point-x p1)
             (funcall point-y p1)
             (funcall rect-area r1)
             ;; p1 is inside r1, p2 is not
             (if (funcall rect-contains-point r1 p1) 'inside 'outside)
             (if (funcall rect-contains-point r1 p2) 'inside 'outside)
             ;; Translate a point by creating a new plist
             (let ((translated
                    (let* ((np (copy-sequence p1))
                           (np (plist-put np :x (+ (plist-get np :x) 100)))
                           (np (plist-put np :y (+ (plist-get np :y) 200))))
                      np)))
               (list (funcall point-x translated)
                     (funcall point-y translated)))))))
    (fmakunbound 'neovm--test-plist-struct-dummy)))"#;
    let expect = expect_test::expect![[r#""OK (point rect 5 10 600 inside outside (105 210))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-member for existence checking (distinguishing nil value from absent)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_member_existence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pl '(:a 1 :b nil :c 3 :d nil :e 5)))
  ;; plist-get cannot distinguish nil-valued key from absent key
  ;; plist-member can (returns tail or nil)
  (let ((has-key
         (lambda (pl k)
           (if (plist-member pl k) t nil)))
        (safe-get
         ;; Get with explicit absent sentinel
         (lambda (pl k default)
           (let ((tail (plist-member pl k)))
             (if tail (cadr tail) default)))))
    (list
     ;; plist-get ambiguity: both absent and nil-valued return nil
     (plist-get pl :b)   ;; nil (present but nil)
     (plist-get pl :z)   ;; nil (absent)
     ;; plist-member disambiguates
     (plist-member pl :b)  ;; (:b nil :c 3 :d nil :e 5)
     (plist-member pl :z)  ;; nil
     ;; has-key helper
     (funcall has-key pl :a)  ;; t
     (funcall has-key pl :b)  ;; t (nil-valued but present)
     (funcall has-key pl :z)  ;; nil (absent)
     ;; safe-get with default
     (funcall safe-get pl :b 'fallback)  ;; nil (present, value is nil)
     (funcall safe-get pl :z 'fallback)  ;; fallback (absent)
     ;; Count present keys using plist-member
     (let ((count 0))
       (dolist (k '(:a :b :c :d :e :f :g))
         (when (plist-member pl k)
           (setq count (1+ count))))
       count))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil (:b nil :c 3 :d nil :e 5) nil t t nil nil fallback 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Converting between plist and alist representations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_alist_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist-to-alist
         (lambda (pl)
           (let ((result nil)
                 (remaining pl))
             (while remaining
               (setq result (cons (cons (car remaining) (cadr remaining))
                                  result))
               (setq remaining (cddr remaining)))
             (nreverse result))))
        (alist-to-plist
         (lambda (al)
           (let ((result nil))
             (dolist (pair (reverse al))
               (setq result (cons (cdr pair) (cons (car pair) result))))
             result))))
  ;; Roundtrip: plist -> alist -> plist
  (let* ((original '(:name "Alice" :age 30 :active t :score 95.5))
         (as-alist (funcall plist-to-alist original))
         (back-to-plist (funcall alist-to-plist as-alist)))
    (list
     ;; Alist form
     as-alist
     ;; Back to plist
     back-to-plist
     ;; Verify roundtrip preserves values
     (equal original back-to-plist)
     ;; Convert alist to plist
     (let ((al '((x . 10) (y . 20) (z . 30))))
       (funcall alist-to-plist al))
     ;; Handle empty lists
     (funcall plist-to-alist nil)
     (funcall alist-to-plist nil)
     ;; Nested: plist of plists -> alist of alists
     (let* ((nested-pl '(:a (:x 1 :y 2) :b (:x 3 :y 4)))
            (outer-al (funcall plist-to-alist nested-pl)))
       ;; The values are still plists; convert them too
       (mapcar (lambda (pair)
                 (cons (car pair)
                       (funcall plist-to-alist (cdr pair))))
               outer-al)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((:name . \"Alice\") (:age . 30) (:active . t) (:score . 95.5)) (\"Alice\" :name 30 :age t :active 95.5 :score) nil (10 x 20 y 30 z) nil nil ((:a (:x . 1) (:y . 2)) (:b (:x . 3) (:y . 4))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Merging two plists (later values override)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_merge_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist-merge
         ;; Merge overlay onto base; overlay values win
         (lambda (base overlay)
           (let ((result (copy-sequence base))
                 (remaining overlay))
             (while remaining
               (setq result (plist-put result
                                       (car remaining)
                                       (cadr remaining)))
               (setq remaining (cddr remaining)))
             result)))
        (plist-merge-deep
         ;; Deep merge: if both values are plists, merge recursively
         (lambda (base overlay)
           (let ((result (copy-sequence base))
                 (remaining overlay))
             (while remaining
               (let* ((k (car remaining))
                      (new-v (cadr remaining))
                      (old-v (plist-get result k)))
                 ;; If both old and new are plists (even-length keyword lists),
                 ;; merge recursively. For simplicity, check if both are lists
                 ;; with a keyword first element.
                 (if (and (consp old-v) (consp new-v)
                          (keywordp (car old-v)) (keywordp (car new-v)))
                     (setq result (plist-put result k
                                             ;; Can't recurse easily without named fn,
                                             ;; so do one level of merge
                                             (let ((merged (copy-sequence old-v))
                                                   (r new-v))
                                               (while r
                                                 (setq merged (plist-put merged (car r) (cadr r)))
                                                 (setq r (cddr r)))
                                               merged)))
                   (setq result (plist-put result k new-v))))
               (setq remaining (cddr remaining)))
             result))))
  ;; Shallow merge tests
  (let* ((defaults '(:host "localhost" :port 8080 :debug nil :workers 4))
         (custom   '(:port 3000 :debug t :name "myapp"))
         (merged   (funcall plist-merge defaults custom)))
    (list
     ;; Merged values
     (plist-get merged :host)      ;; "localhost" (from defaults)
     (plist-get merged :port)      ;; 3000 (overridden)
     (plist-get merged :debug)     ;; t (overridden)
     (plist-get merged :workers)   ;; 4 (from defaults)
     (plist-get merged :name)      ;; "myapp" (new from custom)
     ;; Deep merge test
     (let* ((base '(:db (:host "localhost" :port 5432 :name "mydb")
                    :cache (:enabled t :ttl 300)))
            (overlay '(:db (:port 5433 :ssl t)
                       :cache (:ttl 600)))
            (deep (funcall plist-merge-deep base overlay)))
       (list
        ;; db should have all fields from both
        (plist-get (plist-get deep :db) :host)  ;; "localhost"
        (plist-get (plist-get deep :db) :port)  ;; 5433
        (plist-get (plist-get deep :db) :name)  ;; "mydb"
        (plist-get (plist-get deep :db) :ssl)   ;; t
        ;; cache should be merged
        (plist-get (plist-get deep :cache) :enabled)  ;; t
        (plist-get (plist-get deep :cache) :ttl))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"localhost\" 3000 t 4 \"myapp\" (\"localhost\" 5433 \"mydb\" t t 600))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Schema validation on plists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_schema_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((validate-field
             (lambda (pl key pred required)
               (let ((tail (plist-member pl key)))
                 (cond
                  ;; Missing required field
                  ((and required (not tail))
                   (format "missing required field %s" key))
                  ;; Missing optional field is OK
                  ((not tail) nil)
                  ;; Present but wrong type
                  ((not (funcall pred (cadr tail)))
                   (format "field %s: expected %s, got %S"
                           key pred (cadr tail)))
                  ;; Valid
                  (t nil)))))
            (validate-schema
             (lambda (schema pl)
               ;; schema is alist of (key . (pred . required))
               (let ((errors nil))
                 (dolist (field schema)
                   (let* ((key (car field))
                          (pred (cadr field))
                          (required (cddr field))
                          (err (funcall validate-field pl key pred required)))
                     (when err (setq errors (cons err errors)))))
                 (if errors
                     (list nil (nreverse errors))
                   (list t nil))))))
        (let ((user-schema
               '((:name   stringp . t)
                 (:age    integerp . t)
                 (:email  stringp . t)
                 (:bio    stringp . nil)
                 (:score  numberp . nil))))
          ;; Valid complete record
          (let ((r1 '(:name "Alice" :age 30 :email "a@b.com" :bio "Hi" :score 95)))
            ;; Valid minimal record (no optional fields)
            (let ((r2 '(:name "Bob" :age 25 :email "b@b.com")))
              ;; Invalid: wrong types
              (let ((r3 '(:name 42 :age "old" :email nil)))
                ;; Invalid: missing required fields
                (let ((r4 '(:bio "Just a bio" :score 50)))
                  (list
                   (funcall validate-schema user-schema r1)
                   (funcall validate-schema user-schema r2)
                   (funcall validate-schema user-schema r3)
                   (funcall validate-schema user-schema r4))))))))
    (fmakunbound 'neovm--test-plist-schema-dummy)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable validate-field)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Property list as message format (serialize/deserialize)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_message_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((encode-message
             ;; Encode a plist message as a formatted string
             (lambda (msg)
               (let ((parts nil)
                     (remaining msg))
                 (while remaining
                   (let ((k (substring (symbol-name (car remaining)) 1))
                         (v (cadr remaining)))
                     (setq parts
                           (cons (format "%s=%S" k v) parts)))
                   (setq remaining (cddr remaining)))
                 (mapconcat #'identity (nreverse parts) "|"))))
            (decode-message
             ;; Decode a pipe-separated key=value string back to plist
             (lambda (str)
               (let ((result nil)
                     (pairs (split-string str "|")))
                 (dolist (pair pairs)
                   (let* ((eqpos (string-match "=" pair))
                          (key (intern (concat ":" (substring pair 0 eqpos))))
                          (val-str (substring pair (1+ eqpos)))
                          (val (car (read-from-string val-str))))
                     (setq result (cons val (cons key result)))))
                 (nreverse result)))))
        ;; Roundtrip: plist -> string -> plist
        (let* ((msg '(:type "request" :id 42 :method "get" :path "/api/users"))
               (encoded (funcall encode-message msg))
               (decoded (funcall decode-message encoded)))
          ;; Build a conversation of messages
          (let* ((request '(:type "request" :id 1 :action "login"
                            :user "alice" :pass "secret"))
                 (response '(:type "response" :id 1 :status 200
                             :body "OK" :token "abc123"))
                 (req-enc (funcall encode-message request))
                 (resp-enc (funcall encode-message response))
                 (req-dec (funcall decode-message req-enc))
                 (resp-dec (funcall decode-message resp-enc)))
            (list
             encoded
             ;; Verify roundtrip
             (equal (plist-get msg :type) (plist-get decoded :type))
             (equal (plist-get msg :id) (plist-get decoded :id))
             (equal (plist-get msg :method) (plist-get decoded :method))
             ;; Request/response roundtrip
             (plist-get req-dec :action)
             (plist-get resp-dec :status)
             (plist-get resp-dec :token)
             ;; Message routing: dispatch based on :type
             (let ((dispatch
                    (lambda (msg)
                      (let ((type (plist-get msg :type)))
                        (cond
                         ((equal type "request")
                          (format "Processing request #%d: %s"
                                  (plist-get msg :id)
                                  (plist-get msg :action)))
                         ((equal type "response")
                          (format "Response #%d status=%d"
                                  (plist-get msg :id)
                                  (plist-get msg :status)))
                         (t "unknown message type"))))))
               (list (funcall dispatch req-dec)
                     (funcall dispatch resp-dec)))))))
    (fmakunbound 'neovm--test-plist-msg-dummy)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"type=\\\"request\\\"|id=42|method=\\\"get\\\"|path=\\\"/api/users\\\"\" t t t \"login\" 200 \"abc123\" (\"Processing request #1: login\" \"Response #1 status=200\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist-based event system with handlers and bubbling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_event_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((make-event
             (lambda (type data)
               (list :type type :data data :handled nil :log nil)))
            (handle-event
             ;; Run handler, mark event as handled, append to log
             (lambda (event handler-name handler-fn)
               (let* ((result (funcall handler-fn event))
                      (ev (plist-put (copy-sequence event)
                                     :handled t))
                      (ev (plist-put ev :log
                                     (append (plist-get ev :log)
                                             (list (format "%s: %s"
                                                           handler-name
                                                           result))))))
                 ev)))
            (pipeline
             ;; Run event through a pipeline of handlers
             (lambda (event handlers)
               (let ((current event))
                 (dolist (h handlers)
                   (setq current
                         (funcall handle-event current
                                  (car h) (cdr h))))
                 current))))
        ;; Define handlers
        (let ((validate
               (cons "validate"
                     (lambda (ev)
                       (if (plist-get (plist-get ev :data) :valid)
                           "passed" "failed"))))
              (transform
               (cons "transform"
                     (lambda (ev)
                       (let ((d (plist-get ev :data)))
                         (format "processed %s"
                                 (plist-get d :name))))))
              (log-handler
               (cons "log"
                     (lambda (ev)
                       (format "logged event type=%s"
                               (plist-get ev :type))))))
          ;; Run events through pipeline
          (let* ((ev1 (funcall make-event 'click
                               '(:name "button1" :valid t :x 100 :y 200)))
                 (ev2 (funcall make-event 'submit
                               '(:name "form1" :valid nil :fields 3)))
                 (r1 (funcall pipeline ev1
                              (list validate transform log-handler)))
                 (r2 (funcall pipeline ev2
                              (list validate transform log-handler))))
            (list
             (plist-get r1 :handled)
             (plist-get r1 :log)
             (plist-get r2 :log)
             (plist-get r1 :type)
             (plist-get r2 :type)))))
    (fmakunbound 'neovm--test-plist-event-dummy)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable handle-event)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
