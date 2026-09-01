//! Oracle parity tests for a URL router using a trie in Elisp.
//!
//! Implements route registration with path parameters (:param),
//! wildcard routes (*splat), method-based routing (GET, POST, etc.),
//! middleware chains, and route priority (exact > param > wildcard).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic trie router: registration and exact matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_exact_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trie node: (handler . ((segment . child-node) ...))
    // handler is nil for intermediate nodes.
    let form = r#"
(progn
  (fset 'neovm--tr-make-node
    (lambda () (cons nil nil)))

  (fset 'neovm--tr-split-path
    (lambda (path)
      "Split /a/b/c into (\"a\" \"b\" \"c\"). Ignores leading/trailing slashes."
      (let ((parts nil) (start 0) (len (length path)))
        ;; Skip leading slash
        (when (and (> len 0) (= (aref path 0) ?/))
          (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start)
                (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr-add-route
    (lambda (root method path handler)
      "Add a route. METHOD is a symbol (get, post, etc.), PATH is a string."
      (let ((segments (funcall 'neovm--tr-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        ;; Set handler for method at this node
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry
                    (setcdr entry handler)
                  (setcar node (cons (cons method handler) existing))))
            (setcar node (list (cons method handler))))))
      root))

  (fset 'neovm--tr-match-exact
    (lambda (root method path)
      "Match a route exactly. Returns (handler . params-alist) or nil."
      (let ((segments (funcall 'neovm--tr-split-path path))
            (node root)
            (found t))
        (dolist (seg segments)
          (when found
            (let ((child (assoc seg (cdr node))))
              (if child
                  (setq node (cdr child))
                (setq found nil)))))
        (when found
          (let ((handlers (car node)))
            (when (listp handlers)
              (let ((h (cdr (assq method handlers))))
                (when h (cons h nil)))))))))

  (unwind-protect
      (let ((router (funcall 'neovm--tr-make-node)))
        ;; Register routes
        (funcall 'neovm--tr-add-route router 'get "/" 'home-handler)
        (funcall 'neovm--tr-add-route router 'get "/users" 'users-list)
        (funcall 'neovm--tr-add-route router 'post "/users" 'users-create)
        (funcall 'neovm--tr-add-route router 'get "/users/new" 'users-new)
        (funcall 'neovm--tr-add-route router 'get "/api/v1/status" 'api-status)
        (list
         ;; Exact matches
         (funcall 'neovm--tr-match-exact router 'get "/")
         (funcall 'neovm--tr-match-exact router 'get "/users")
         (funcall 'neovm--tr-match-exact router 'post "/users")
         (funcall 'neovm--tr-match-exact router 'get "/users/new")
         (funcall 'neovm--tr-match-exact router 'get "/api/v1/status")
         ;; No match
         (funcall 'neovm--tr-match-exact router 'get "/nonexistent")
         ;; Wrong method
         (funcall 'neovm--tr-match-exact router 'delete "/users")
         ;; Partial path no match
         (funcall 'neovm--tr-match-exact router 'get "/api")))
    (fmakunbound 'neovm--tr-make-node)
    (fmakunbound 'neovm--tr-split-path)
    (fmakunbound 'neovm--tr-add-route)
    (fmakunbound 'neovm--tr-match-exact)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((home-handler) (users-list) (users-create) (users-new) (api-status) nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Route matching with path parameters (:param)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_path_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Segments starting with ":" are parameter placeholders.
    // They match any single segment and capture the value.
    let form = r#"
(progn
  (fset 'neovm--tr2-make-node (lambda () (cons nil nil)))

  (fset 'neovm--tr2-split-path
    (lambda (path)
      (let ((parts nil) (start 0) (len (length path)))
        (when (and (> len 0) (= (aref path 0) ?/)) (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start) (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr2-is-param-p
    (lambda (seg) (and (> (length seg) 1) (= (aref seg 0) ?:))))

  (fset 'neovm--tr2-add-route
    (lambda (root method path handler)
      (let ((segments (funcall 'neovm--tr2-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr2-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry (setcdr entry handler)
                  (setcar node (cons (cons method handler) existing))))
            (setcar node (list (cons method handler))))))
      root))

  (fset 'neovm--tr2-match
    (lambda (root method path)
      "Match route with parameter support. Returns (handler . params-alist) or nil."
      (let ((segments (funcall 'neovm--tr2-split-path path)))
        (let ((result nil))
          (fset 'neovm--tr2-match-rec
            (lambda (node segs params)
              (if (null segs)
                  ;; At end of path, check for handler
                  (let ((handlers (car node)))
                    (when (listp handlers)
                      (let ((h (cdr (assq method handlers))))
                        (when h
                          (setq result (cons h (nreverse params)))))))
                (let ((seg (car segs)) (rest (cdr segs)))
                  ;; Try exact match first
                  (let ((exact (assoc seg (cdr node))))
                    (when exact
                      (funcall 'neovm--tr2-match-rec (cdr exact) rest params)))
                  ;; Try parameter match if no result yet
                  (unless result
                    (dolist (child (cdr node))
                      (unless result
                        (when (funcall 'neovm--tr2-is-param-p (car child))
                          (let ((param-name (substring (car child) 1)))
                            (funcall 'neovm--tr2-match-rec
                                     (cdr child) rest
                                     (cons (cons param-name seg) params)))))))))))
          (funcall 'neovm--tr2-match-rec root segments nil)
          result))))

  (unwind-protect
      (let ((router (funcall 'neovm--tr2-make-node)))
        (funcall 'neovm--tr2-add-route router 'get "/users/:id" 'user-show)
        (funcall 'neovm--tr2-add-route router 'put "/users/:id" 'user-update)
        (funcall 'neovm--tr2-add-route router 'get "/users/:id/posts/:post_id" 'user-post)
        (funcall 'neovm--tr2-add-route router 'get "/repos/:owner/:repo" 'repo-show)
        (list
         ;; Match with param extraction
         (funcall 'neovm--tr2-match router 'get "/users/42")
         (funcall 'neovm--tr2-match router 'put "/users/99")
         ;; Nested params
         (funcall 'neovm--tr2-match router 'get "/users/7/posts/123")
         ;; Multiple params
         (funcall 'neovm--tr2-match router 'get "/repos/octocat/hello-world")
         ;; No match
         (funcall 'neovm--tr2-match router 'get "/users")
         (funcall 'neovm--tr2-match router 'get "/users/42/unknown")))
    (fmakunbound 'neovm--tr2-make-node)
    (fmakunbound 'neovm--tr2-split-path)
    (fmakunbound 'neovm--tr2-is-param-p)
    (fmakunbound 'neovm--tr2-add-route)
    (fmakunbound 'neovm--tr2-match)
    (fmakunbound 'neovm--tr2-match-rec)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((user-show (\"id\" . \"42\")) (user-update (\"id\" . \"99\")) (user-post (\"id\" . \"7\") (\"post_id\" . \"123\")) (repo-show (\"owner\" . \"octocat\") (\"repo\" . \"hello-world\")) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Wildcard routes (*splat)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_wildcard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Segments starting with "*" match the rest of the path.
    let form = r#"
(progn
  (fset 'neovm--tr3-make-node (lambda () (cons nil nil)))

  (fset 'neovm--tr3-split-path
    (lambda (path)
      (let ((parts nil) (start 0) (len (length path)))
        (when (and (> len 0) (= (aref path 0) ?/)) (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start) (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr3-is-param-p
    (lambda (seg) (and (> (length seg) 1) (= (aref seg 0) ?:))))

  (fset 'neovm--tr3-is-wildcard-p
    (lambda (seg) (and (> (length seg) 1) (= (aref seg 0) ?*))))

  (fset 'neovm--tr3-add-route
    (lambda (root method path handler)
      (let ((segments (funcall 'neovm--tr3-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr3-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry (setcdr entry handler)
                  (setcar node (cons (cons method handler) existing))))
            (setcar node (list (cons method handler))))))
      root))

  (fset 'neovm--tr3-match
    (lambda (root method path)
      (let ((segments (funcall 'neovm--tr3-split-path path))
            (result nil))
        (fset 'neovm--tr3-match-rec
          (lambda (node segs params)
            (if (null segs)
                (let ((handlers (car node)))
                  (when (listp handlers)
                    (let ((h (cdr (assq method handlers))))
                      (when h (setq result (cons h (nreverse params)))))))
              (let ((seg (car segs)) (rest (cdr segs)))
                ;; Exact match
                (let ((exact (assoc seg (cdr node))))
                  (when exact
                    (funcall 'neovm--tr3-match-rec (cdr exact) rest params)))
                ;; Param match
                (unless result
                  (dolist (child (cdr node))
                    (unless result
                      (when (funcall 'neovm--tr3-is-param-p (car child))
                        (funcall 'neovm--tr3-match-rec
                                 (cdr child) rest
                                 (cons (cons (substring (car child) 1) seg) params))))))
                ;; Wildcard match: consumes all remaining segments
                (unless result
                  (dolist (child (cdr node))
                    (unless result
                      (when (funcall 'neovm--tr3-is-wildcard-p (car child))
                        (let ((splat-name (substring (car child) 1))
                              (splat-val (mapconcat #'identity (cons seg rest) "/")))
                          ;; Wildcard node should have handler
                          (let ((handlers (car (cdr child))))
                            (when (listp handlers)
                              (let ((h (cdr (assq method handlers))))
                                (when h
                                  (setq result
                                        (cons h (nreverse
                                                 (cons (cons splat-name splat-val)
                                                       params)))))))))))))))))
        (funcall 'neovm--tr3-match-rec root segments nil)
        result)))

  (unwind-protect
      (let ((router (funcall 'neovm--tr3-make-node)))
        (funcall 'neovm--tr3-add-route router 'get "/files/*filepath" 'serve-file)
        (funcall 'neovm--tr3-add-route router 'get "/api/:version/*rest" 'api-proxy)
        (funcall 'neovm--tr3-add-route router 'get "/static/*path" 'static-handler)
        (list
         ;; Wildcard matches
         (funcall 'neovm--tr3-match router 'get "/files/css/style.css")
         (funcall 'neovm--tr3-match router 'get "/files/js/app.js")
         (funcall 'neovm--tr3-match router 'get "/files/img/logo.png")
         ;; Param + wildcard
         (funcall 'neovm--tr3-match router 'get "/api/v2/users/list")
         (funcall 'neovm--tr3-match router 'get "/api/v1/deep/nested/path")
         ;; Single segment wildcard
         (funcall 'neovm--tr3-match router 'get "/static/favicon.ico")
         ;; No match
         (funcall 'neovm--tr3-match router 'get "/unknown/path")))
    (fmakunbound 'neovm--tr3-make-node)
    (fmakunbound 'neovm--tr3-split-path)
    (fmakunbound 'neovm--tr3-is-param-p)
    (fmakunbound 'neovm--tr3-is-wildcard-p)
    (fmakunbound 'neovm--tr3-add-route)
    (fmakunbound 'neovm--tr3-match)
    (fmakunbound 'neovm--tr3-match-rec)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((serve-file (\"filepath\" . \"css/style.css\")) (serve-file (\"filepath\" . \"js/app.js\")) (serve-file (\"filepath\" . \"img/logo.png\")) (api-proxy (\"version\" . \"v2\") (\"rest\" . \"users/list\")) (api-proxy (\"version\" . \"v1\") (\"rest\" . \"deep/nested/path\")) (static-handler (\"path\" . \"favicon.ico\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Method-based routing with multiple handlers per path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_method_routing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Same path can have different handlers for different HTTP methods.
    let form = r#"
(progn
  (fset 'neovm--tr4-make-node (lambda () (cons nil nil)))

  (fset 'neovm--tr4-split-path
    (lambda (path)
      (let ((parts nil) (start 0) (len (length path)))
        (when (and (> len 0) (= (aref path 0) ?/)) (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start) (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr4-add-route
    (lambda (root method path handler)
      (let ((segments (funcall 'neovm--tr4-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr4-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry (setcdr entry handler)
                  (setcar node (cons (cons method handler) existing))))
            (setcar node (list (cons method handler))))))
      root))

  (fset 'neovm--tr4-match
    (lambda (root method path)
      (let ((segments (funcall 'neovm--tr4-split-path path))
            (node root) (found t))
        (dolist (seg segments)
          (when found
            (let ((child (assoc seg (cdr node))))
              (if child (setq node (cdr child)) (setq found nil)))))
        (when found
          (let ((handlers (car node)))
            (when (listp handlers)
              (cdr (assq method handlers))))))))

  (fset 'neovm--tr4-allowed-methods
    (lambda (root path)
      "Return list of methods allowed for PATH."
      (let ((segments (funcall 'neovm--tr4-split-path path))
            (node root) (found t))
        (dolist (seg segments)
          (when found
            (let ((child (assoc seg (cdr node))))
              (if child (setq node (cdr child)) (setq found nil)))))
        (when found
          (let ((handlers (car node)))
            (when (listp handlers)
              (mapcar #'car handlers)))))))

  (unwind-protect
      (let ((router (funcall 'neovm--tr4-make-node)))
        ;; RESTful resource
        (funcall 'neovm--tr4-add-route router 'get "/articles" 'articles-index)
        (funcall 'neovm--tr4-add-route router 'post "/articles" 'articles-create)
        (funcall 'neovm--tr4-add-route router 'get "/articles/featured" 'articles-featured)
        (funcall 'neovm--tr4-add-route router 'put "/articles/featured" 'articles-set-featured)
        (funcall 'neovm--tr4-add-route router 'delete "/articles/featured" 'articles-clear-featured)
        (list
         ;; GET /articles
         (funcall 'neovm--tr4-match router 'get "/articles")
         ;; POST /articles
         (funcall 'neovm--tr4-match router 'post "/articles")
         ;; Different methods on same sub-path
         (funcall 'neovm--tr4-match router 'get "/articles/featured")
         (funcall 'neovm--tr4-match router 'put "/articles/featured")
         (funcall 'neovm--tr4-match router 'delete "/articles/featured")
         ;; Method not registered
         (funcall 'neovm--tr4-match router 'patch "/articles")
         ;; Allowed methods
         (sort (funcall 'neovm--tr4-allowed-methods router "/articles")
               (lambda (a b) (string-lessp (symbol-name a) (symbol-name b))))
         (sort (funcall 'neovm--tr4-allowed-methods router "/articles/featured")
               (lambda (a b) (string-lessp (symbol-name a) (symbol-name b))))
         ;; Override handler
         (progn
           (funcall 'neovm--tr4-add-route router 'get "/articles" 'articles-index-v2)
           (funcall 'neovm--tr4-match router 'get "/articles"))))
    (fmakunbound 'neovm--tr4-make-node)
    (fmakunbound 'neovm--tr4-split-path)
    (fmakunbound 'neovm--tr4-add-route)
    (fmakunbound 'neovm--tr4-match)
    (fmakunbound 'neovm--tr4-allowed-methods)))
"#;
    let expect = expect_test::expect![[
        r#""OK (articles-index articles-create articles-featured articles-set-featured articles-clear-featured nil (get post) (delete get put) articles-index-v2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Middleware chain with route matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_middleware_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Routes can have middleware: a list of functions applied before the handler.
    // Middleware can modify the request context (an alist) and decide to proceed
    // or halt (return an error).
    let form = r#"
(progn
  (fset 'neovm--tr5-make-node (lambda () (cons nil nil)))

  (fset 'neovm--tr5-split-path
    (lambda (path)
      (let ((parts nil) (start 0) (len (length path)))
        (when (and (> len 0) (= (aref path 0) ?/)) (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start) (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr5-add-route
    (lambda (root method path handler middlewares)
      "Add route with handler and middleware list."
      (let ((segments (funcall 'neovm--tr5-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr5-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry (setcdr entry (cons handler middlewares))
                  (setcar node (cons (cons method (cons handler middlewares)) existing))))
            (setcar node (list (cons method (cons handler middlewares)))))))
      root))

  (fset 'neovm--tr5-dispatch
    (lambda (root method path context)
      "Match route, run middleware chain, then handler. Returns result."
      (let ((segments (funcall 'neovm--tr5-split-path path))
            (node root) (found t))
        (dolist (seg segments)
          (when found
            (let ((child (assoc seg (cdr node))))
              (if child (setq node (cdr child)) (setq found nil)))))
        (if (not found)
            (list :status 404 :body "not found")
          (let ((handlers (car node)))
            (if (not (listp handlers))
                (list :status 404 :body "not found")
              (let ((entry (assq method handlers)))
                (if (not entry)
                    (list :status 405 :body "method not allowed")
                  (let* ((route-data (cdr entry))
                         (handler (car route-data))
                         (mws (cdr route-data))
                         (ctx context)
                         (halted nil))
                    ;; Run middleware chain
                    (dolist (mw mws)
                      (unless halted
                        (let ((result (funcall mw ctx)))
                          (if (eq (car result) :halt)
                              (setq halted (cdr result))
                            (setq ctx result)))))
                    (if halted
                        halted
                      (funcall handler ctx)))))))))))

  (unwind-protect
      (let ((router (funcall 'neovm--tr5-make-node)))
        ;; Middleware: add timestamp
        (let ((add-timestamp
               (lambda (ctx) (cons (cons 'timestamp 12345) ctx)))
              ;; Middleware: require auth
              (require-auth
               (lambda (ctx)
                 (if (cdr (assq 'auth-token ctx))
                     ctx
                   (cons :halt (list :status 401 :body "unauthorized")))))
              ;; Handlers
              (public-handler
               (lambda (ctx)
                 (list :status 200 :body "public" :ctx ctx)))
              (private-handler
               (lambda (ctx)
                 (list :status 200 :body "private" :ctx ctx))))
          ;; Public route with timestamp middleware
          (funcall 'neovm--tr5-add-route router 'get "/public" public-handler
                   (list add-timestamp))
          ;; Private route with auth + timestamp middleware
          (funcall 'neovm--tr5-add-route router 'get "/private" private-handler
                   (list add-timestamp require-auth))
          (list
           ;; Public route: no auth needed
           (funcall 'neovm--tr5-dispatch router 'get "/public" '((user . "guest")))
           ;; Private route without auth: blocked
           (funcall 'neovm--tr5-dispatch router 'get "/private" '((user . "guest")))
           ;; Private route with auth: passes
           (funcall 'neovm--tr5-dispatch router 'get "/private"
                    '((user . "admin") (auth-token . "secret")))
           ;; 404
           (funcall 'neovm--tr5-dispatch router 'get "/missing" nil)
           ;; 405
           (funcall 'neovm--tr5-dispatch router 'post "/public" nil))))
    (fmakunbound 'neovm--tr5-make-node)
    (fmakunbound 'neovm--tr5-split-path)
    (fmakunbound 'neovm--tr5-add-route)
    (fmakunbound 'neovm--tr5-dispatch)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:status 200 :body \"public\" :ctx ((timestamp . 12345) (user . \"guest\"))) (:status 401 :body \"unauthorized\") (:status 200 :body \"private\" :ctx ((timestamp . 12345) (user . \"admin\") (auth-token . \"secret\"))) (:status 404 :body \"not found\") (:status 405 :body \"method not allowed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Route priority: exact > param > wildcard
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When multiple routes could match, exact match takes priority over
    // parameter match, which takes priority over wildcard match.
    let form = r#"
(progn
  (fset 'neovm--tr6-make-node (lambda () (cons nil nil)))

  (fset 'neovm--tr6-split-path
    (lambda (path)
      (let ((parts nil) (start 0) (len (length path)))
        (when (and (> len 0) (= (aref path 0) ?/)) (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start) (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr6-is-param-p
    (lambda (seg) (and (> (length seg) 1) (= (aref seg 0) ?:))))
  (fset 'neovm--tr6-is-wildcard-p
    (lambda (seg) (and (> (length seg) 1) (= (aref seg 0) ?*))))

  (fset 'neovm--tr6-add-route
    (lambda (root method path handler)
      (let ((segments (funcall 'neovm--tr6-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr6-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry (setcdr entry handler)
                  (setcar node (cons (cons method handler) existing))))
            (setcar node (list (cons method handler))))))
      root))

  (fset 'neovm--tr6-match
    (lambda (root method path)
      "Match with priority: exact > param > wildcard."
      (let ((segments (funcall 'neovm--tr6-split-path path))
            (best nil))
        ;; Collect all possible matches with priority scores
        ;; exact segment = 3, param = 2, wildcard = 1
        (fset 'neovm--tr6-match-rec
          (lambda (node segs params priority)
            (if (null segs)
                (let ((handlers (car node)))
                  (when (listp handlers)
                    (let ((h (cdr (assq method handlers))))
                      (when h
                        (when (or (null best) (> priority (car best)))
                          (setq best (list priority h (nreverse (copy-sequence params)))))))))
              (let ((seg (car segs)) (rest (cdr segs)))
                ;; Exact match (priority +3)
                (let ((exact (assoc seg (cdr node))))
                  (when exact
                    (funcall 'neovm--tr6-match-rec (cdr exact) rest params (+ priority 3))))
                ;; Param match (priority +2)
                (dolist (child (cdr node))
                  (when (funcall 'neovm--tr6-is-param-p (car child))
                    (funcall 'neovm--tr6-match-rec
                             (cdr child) rest
                             (cons (cons (substring (car child) 1) seg) params)
                             (+ priority 2))))
                ;; Wildcard match (priority +1)
                (dolist (child (cdr node))
                  (when (funcall 'neovm--tr6-is-wildcard-p (car child))
                    (let ((splat-name (substring (car child) 1))
                          (splat-val (mapconcat #'identity (cons seg rest) "/")))
                      (let ((wc-handlers (car (cdr child))))
                        (when (listp wc-handlers)
                          (let ((h (cdr (assq method wc-handlers))))
                            (when h
                              (when (or (null best) (> (+ priority 1) (car best)))
                                (setq best (list (+ priority 1) h
                                                 (nreverse (cons (cons splat-name splat-val)
                                                                 (copy-sequence params)))))))))))))))))
        (funcall 'neovm--tr6-match-rec root segments nil 0)
        (when best (cons (cadr best) (caddr best))))))

  (unwind-protect
      (let ((router (funcall 'neovm--tr6-make-node)))
        ;; Overlapping routes
        (funcall 'neovm--tr6-add-route router 'get "/users/admin" 'admin-handler)
        (funcall 'neovm--tr6-add-route router 'get "/users/:id" 'user-handler)
        (funcall 'neovm--tr6-add-route router 'get "/users/*rest" 'user-wildcard)
        (funcall 'neovm--tr6-add-route router 'get "/docs/api" 'docs-api-exact)
        (funcall 'neovm--tr6-add-route router 'get "/docs/:page" 'docs-page)
        (funcall 'neovm--tr6-add-route router 'get "/docs/*path" 'docs-wildcard)
        (list
         ;; /users/admin -> exact match (admin-handler), not param
         (car (funcall 'neovm--tr6-match router 'get "/users/admin"))
         ;; /users/42 -> param match (user-handler)
         (funcall 'neovm--tr6-match router 'get "/users/42")
         ;; /users/42/posts -> wildcard match (user-wildcard)
         (funcall 'neovm--tr6-match router 'get "/users/42/posts")
         ;; /docs/api -> exact match (docs-api-exact)
         (car (funcall 'neovm--tr6-match router 'get "/docs/api"))
         ;; /docs/tutorial -> param match (docs-page)
         (funcall 'neovm--tr6-match router 'get "/docs/tutorial")
         ;; /docs/guides/advanced/tips -> wildcard match (docs-wildcard)
         (funcall 'neovm--tr6-match router 'get "/docs/guides/advanced/tips")))
    (fmakunbound 'neovm--tr6-make-node)
    (fmakunbound 'neovm--tr6-split-path)
    (fmakunbound 'neovm--tr6-is-param-p)
    (fmakunbound 'neovm--tr6-is-wildcard-p)
    (fmakunbound 'neovm--tr6-add-route)
    (fmakunbound 'neovm--tr6-match)
    (fmakunbound 'neovm--tr6-match-rec)))
"#;
    let expect = expect_test::expect![[
        r#""OK (admin-handler (user-handler (\"id\" . \"42\")) (user-wildcard (\"rest\" . \"42/posts\")) docs-api-exact (docs-page (\"page\" . \"tutorial\")) (docs-wildcard (\"path\" . \"guides/advanced/tips\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Route listing and debugging
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_router_list_routes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Traverse the trie to list all registered routes with their methods.
    let form = r#"
(progn
  (fset 'neovm--tr7-make-node (lambda () (cons nil nil)))

  (fset 'neovm--tr7-split-path
    (lambda (path)
      (let ((parts nil) (start 0) (len (length path)))
        (when (and (> len 0) (= (aref path 0) ?/)) (setq start 1))
        (let ((i start))
          (while (<= i len)
            (when (or (= i len) (= (aref path i) ?/))
              (when (> i start) (setq parts (cons (substring path start i) parts)))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (nreverse parts))))

  (fset 'neovm--tr7-add-route
    (lambda (root method path handler)
      (let ((segments (funcall 'neovm--tr7-split-path path))
            (node root))
        (dolist (seg segments)
          (let ((child (assoc seg (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (funcall 'neovm--tr7-make-node)))
                (setcdr node (cons (cons seg new-node) (cdr node)))
                (setq node new-node)))))
        (let ((existing (car node)))
          (if (and existing (listp existing))
              (let ((entry (assq method existing)))
                (if entry (setcdr entry handler)
                  (setcar node (cons (cons method handler) existing))))
            (setcar node (list (cons method handler))))))
      root))

  (fset 'neovm--tr7-list-routes
    (lambda (root)
      "Return sorted list of (method path handler) for all routes."
      (let ((routes nil))
        (fset 'neovm--tr7-walk
          (lambda (node path-parts)
            ;; Check handlers at this node
            (let ((handlers (car node)))
              (when (listp handlers)
                (dolist (h handlers)
                  (let ((path-str (if (null path-parts) "/"
                                    (concat "/" (mapconcat #'identity (nreverse (copy-sequence path-parts)) "/")))))
                    (setq routes (cons (list (car h) path-str (cdr h)) routes))))))
            ;; Recurse into children
            (dolist (child (cdr node))
              (funcall 'neovm--tr7-walk (cdr child) (cons (car child) path-parts)))))
        (funcall 'neovm--tr7-walk root nil)
        (sort routes (lambda (a b)
                       (or (string-lessp (cadr a) (cadr b))
                           (and (string-equal (cadr a) (cadr b))
                                (string-lessp (symbol-name (car a))
                                              (symbol-name (car b))))))))))

  (unwind-protect
      (let ((router (funcall 'neovm--tr7-make-node)))
        (funcall 'neovm--tr7-add-route router 'get "/" 'root-handler)
        (funcall 'neovm--tr7-add-route router 'get "/api/users" 'api-users-get)
        (funcall 'neovm--tr7-add-route router 'post "/api/users" 'api-users-post)
        (funcall 'neovm--tr7-add-route router 'get "/api/posts" 'api-posts-get)
        (funcall 'neovm--tr7-add-route router 'delete "/api/posts" 'api-posts-delete)
        (funcall 'neovm--tr7-add-route router 'get "/health" 'health-check)
        (let ((routes (funcall 'neovm--tr7-list-routes router)))
          (list
           ;; Total number of routes
           (length routes)
           ;; All routes sorted
           routes
           ;; Extract just paths (unique)
           (let ((paths nil))
             (dolist (r routes)
               (unless (member (cadr r) paths)
                 (setq paths (cons (cadr r) paths))))
             (sort (nreverse paths) #'string-lessp))
           ;; Count routes per path
           (let ((counts nil))
             (dolist (r routes)
               (let ((existing (assoc (cadr r) counts)))
                 (if existing
                     (setcdr existing (1+ (cdr existing)))
                   (setq counts (cons (cons (cadr r) 1) counts)))))
             (sort counts (lambda (a b) (string-lessp (car a) (car b))))))))
    (fmakunbound 'neovm--tr7-make-node)
    (fmakunbound 'neovm--tr7-split-path)
    (fmakunbound 'neovm--tr7-add-route)
    (fmakunbound 'neovm--tr7-list-routes)
    (fmakunbound 'neovm--tr7-walk)))
"#;
    let expect = expect_test::expect![[
        r#""OK (6 ((get \"/\" root-handler) (delete \"/api/posts\" api-posts-delete) (get \"/api/posts\" api-posts-get) (get \"/api/users\" api-users-get) (post \"/api/users\" api-users-post) (get \"/health\" health-check)) (\"/\" \"/api/posts\" \"/api/users\" \"/health\") ((\"/\" . 1) (\"/api/posts\" . 2) (\"/api/users\" . 2) (\"/health\" . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
