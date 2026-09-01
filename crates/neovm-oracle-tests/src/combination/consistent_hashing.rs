//! Oracle parity tests for consistent hashing ring implemented in Elisp:
//! hash ring with virtual nodes, node addition/removal, key-to-node mapping,
//! load balancing measurement, virtual node distribution, ring visualization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Helper: hash ring infrastructure used by all tests
// ---------------------------------------------------------------------------

/// Returns the Elisp source that defines the consistent hashing ring functions.
/// All functions use fset with 'neovm--chr-' prefix.
fn hash_ring_prelude() -> &'static str {
    r#"
  ;; Simple hash function: sum of char codes * prime + rotate
  (fset 'neovm--chr-hash
    (lambda (key)
      (let ((h 0) (i 0) (len (length key)))
        (while (< i len)
          (setq h (% (+ (* h 31) (aref key i)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) 1000000))))

  ;; Create empty hash ring: (:nodes nil :vnodes nil :vnode-count N)
  (fset 'neovm--chr-make-ring
    (lambda (vnode-count)
      (list :nodes nil :vnodes nil :vnode-count vnode-count)))

  ;; Sorted insert into vnodes list (sorted by hash)
  (fset 'neovm--chr-sorted-insert
    (lambda (vnodes entry)
      (let ((hash (car entry))
            (result nil)
            (rest vnodes)
            (inserted nil))
        (while rest
          (if (and (not inserted) (< hash (car (car rest))))
              (progn
                (setq result (cons entry result))
                (setq inserted t)))
          (setq result (cons (car rest) result))
          (setq rest (cdr rest)))
        (unless inserted
          (setq result (cons entry result)))
        (nreverse result))))

  ;; Add a node to the ring: creates vnode-count virtual nodes
  (fset 'neovm--chr-add-node
    (lambda (ring node-name)
      (let ((nodes (plist-get ring :nodes))
            (vnodes (plist-get ring :vnodes))
            (vcount (plist-get ring :vnode-count))
            (i 0))
        (while (< i vcount)
          (let* ((vnode-key (format "%s#%d" node-name i))
                 (hash (funcall 'neovm--chr-hash vnode-key)))
            (setq vnodes (funcall 'neovm--chr-sorted-insert
                                   vnodes (list hash node-name i))))
          (setq i (1+ i)))
        (list :nodes (cons node-name nodes) :vnodes vnodes :vnode-count vcount))))

  ;; Remove a node from the ring
  (fset 'neovm--chr-remove-node
    (lambda (ring node-name)
      (let ((nodes (delq node-name (copy-sequence (plist-get ring :nodes))))
            (vnodes (plist-get ring :vnodes))
            (vcount (plist-get ring :vnode-count))
            (new-vnodes nil))
        ;; Filter out vnodes belonging to this node
        (dolist (vn vnodes)
          (unless (string= (cadr vn) node-name)
            (setq new-vnodes (cons vn new-vnodes))))
        (list :nodes nodes :vnodes (nreverse new-vnodes) :vnode-count vcount))))

  ;; Find which node a key maps to (first vnode with hash >= key hash)
  (fset 'neovm--chr-get-node
    (lambda (ring key)
      (let* ((hash (funcall 'neovm--chr-hash key))
             (vnodes (plist-get ring :vnodes))
             (found nil)
             (rest vnodes))
        ;; Find first vnode with hash >= key hash
        (while (and rest (not found))
          (when (>= (car (car rest)) hash)
            (setq found (cadr (car rest))))
          (setq rest (cdr rest)))
        ;; Wrap around to first vnode if no match
        (or found (cadr (car vnodes))))))

  ;; Count how many keys map to each node
  (fset 'neovm--chr-distribution
    (lambda (ring keys)
      (let ((counts nil))
        (dolist (key keys)
          (let* ((node (funcall 'neovm--chr-get-node ring key))
                 (entry (assoc node counts)))
            (if entry
                (setcdr entry (1+ (cdr entry)))
              (setq counts (cons (cons node 1) counts)))))
        ;; Sort by node name for deterministic output
        (sort counts (lambda (a b) (string< (car a) (car b)))))))

  ;; Get ring statistics
  (fset 'neovm--chr-stats
    (lambda (ring)
      (list :node-count (length (plist-get ring :nodes))
            :vnode-count-total (length (plist-get ring :vnodes))
            :vnodes-per-node (plist-get ring :vnode-count))))
"#
}

fn hash_ring_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--chr-hash)
    (fmakunbound 'neovm--chr-make-ring)
    (fmakunbound 'neovm--chr-sorted-insert)
    (fmakunbound 'neovm--chr-add-node)
    (fmakunbound 'neovm--chr-remove-node)
    (fmakunbound 'neovm--chr-get-node)
    (fmakunbound 'neovm--chr-distribution)
    (fmakunbound 'neovm--chr-stats)
"#
}

// ---------------------------------------------------------------------------
// Test: hash function determinism and basic ring creation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consistent_hash_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (list
       ;; Hash determinism
       (= (funcall 'neovm--chr-hash "hello")
          (funcall 'neovm--chr-hash "hello"))
       ;; Different strings give different hashes (usually)
       (not (= (funcall 'neovm--chr-hash "hello")
               (funcall 'neovm--chr-hash "world")))
       ;; Hash is always positive
       (>= (funcall 'neovm--chr-hash "test") 0)
       (>= (funcall 'neovm--chr-hash "") 0)
       ;; Various hash values
       (list (funcall 'neovm--chr-hash "a")
             (funcall 'neovm--chr-hash "b")
             (funcall 'neovm--chr-hash "server1")
             (funcall 'neovm--chr-hash "server2"))
       ;; Empty ring
       (let ((ring (funcall 'neovm--chr-make-ring 3)))
         (list (plist-get ring :nodes)
               (plist-get ring :vnodes)
               (plist-get ring :vnode-count)))
       ;; Ring stats after adding nodes
       (let* ((ring (funcall 'neovm--chr-make-ring 5))
              (ring (funcall 'neovm--chr-add-node ring "node-A"))
              (ring (funcall 'neovm--chr-add-node ring "node-B"))
              (ring (funcall 'neovm--chr-add-node ring "node-C")))
         (funcall 'neovm--chr-stats ring)))
    {cleanup}))"#,
        prelude = hash_ring_prelude(),
        cleanup = hash_ring_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: node addition and key mapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consistent_hash_node_addition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (let* ((ring (funcall 'neovm--chr-make-ring 10))
             (ring (funcall 'neovm--chr-add-node ring "server-1"))
             (ring (funcall 'neovm--chr-add-node ring "server-2"))
             (ring (funcall 'neovm--chr-add-node ring "server-3")))
        (list
         ;; All keys should map to some server
         (funcall 'neovm--chr-get-node ring "user:alice")
         (funcall 'neovm--chr-get-node ring "user:bob")
         (funcall 'neovm--chr-get-node ring "user:charlie")
         (funcall 'neovm--chr-get-node ring "session:1234")
         (funcall 'neovm--chr-get-node ring "session:5678")
         ;; Same key always maps to same node
         (string= (funcall 'neovm--chr-get-node ring "key-x")
                  (funcall 'neovm--chr-get-node ring "key-x"))
         ;; Node count
         (length (plist-get ring :nodes))
         ;; Vnode count = nodes * vnodes-per-node
         (length (plist-get ring :vnodes))
         ;; Vnodes are sorted by hash
         (let ((vnodes (plist-get ring :vnodes))
               (sorted t)
               (prev -1))
           (dolist (vn vnodes)
             (when (< (car vn) prev)
               (setq sorted nil))
             (setq prev (car vn)))
           sorted)))
    {cleanup}))"#,
        prelude = hash_ring_prelude(),
        cleanup = hash_ring_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: node removal with minimal key remapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consistent_hash_node_removal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (let* ((ring (funcall 'neovm--chr-make-ring 10))
             (ring (funcall 'neovm--chr-add-node ring "A"))
             (ring (funcall 'neovm--chr-add-node ring "B"))
             (ring (funcall 'neovm--chr-add-node ring "C"))
             (ring (funcall 'neovm--chr-add-node ring "D"))
             ;; Map keys before removal
             (keys '("k1" "k2" "k3" "k4" "k5" "k6" "k7" "k8" "k9" "k10"
                     "k11" "k12" "k13" "k14" "k15" "k16" "k17" "k18" "k19" "k20"))
             (before (mapcar (lambda (k) (funcall 'neovm--chr-get-node ring k)) keys))
             ;; Remove node B
             (ring-after (funcall 'neovm--chr-remove-node ring "B"))
             (after (mapcar (lambda (k) (funcall 'neovm--chr-get-node ring-after k)) keys))
             ;; Count how many keys changed node
             (changes 0))
        (let ((i 0))
          (while (< i (length keys))
            (unless (string= (nth i before) (nth i after))
              (setq changes (1+ changes)))
            (setq i (1+ i))))
        (list
         ;; No keys should map to removed node B after removal
         (let ((any-b nil))
           (dolist (n after)
             (when (string= n "B") (setq any-b t)))
           any-b)
         ;; Stats before and after
         (funcall 'neovm--chr-stats ring)
         (funcall 'neovm--chr-stats ring-after)
         ;; Number of key reassignments (should be relatively small)
         changes
         ;; Keys not mapping to B should be unchanged
         (let ((stable 0))
           (let ((i 0))
             (while (< i (length keys))
               (when (and (not (string= (nth i before) "B"))
                          (string= (nth i before) (nth i after)))
                 (setq stable (1+ stable)))
               (setq i (1+ i))))
           stable)))
    {cleanup}))"#,
        prelude = hash_ring_prelude(),
        cleanup = hash_ring_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: load distribution measurement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consistent_hash_load_distribution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (let* ((ring (funcall 'neovm--chr-make-ring 20))
             (ring (funcall 'neovm--chr-add-node ring "alpha"))
             (ring (funcall 'neovm--chr-add-node ring "beta"))
             (ring (funcall 'neovm--chr-add-node ring "gamma"))
             ;; Generate 100 keys
             (keys nil))
        (dotimes (i 100)
          (setq keys (cons (format "item-%d" i) keys)))
        (setq keys (nreverse keys))
        (let ((dist (funcall 'neovm--chr-distribution ring keys)))
          (list
           ;; Distribution: each node gets some keys
           dist
           ;; All nodes should have at least 1 key
           (let ((all-nonzero t))
             (dolist (d dist)
               (when (<= (cdr d) 0) (setq all-nonzero nil)))
             all-nonzero)
           ;; Total should be 100
           (let ((total 0))
             (dolist (d dist)
               (setq total (+ total (cdr d))))
             total)
           ;; Add a fourth node and re-measure
           (let* ((ring2 (funcall 'neovm--chr-add-node ring "delta"))
                  (dist2 (funcall 'neovm--chr-distribution ring2 keys)))
             (list
              dist2
              ;; Still 100 total
              (let ((total 0))
                (dolist (d dist2)
                  (setq total (+ total (cdr d))))
                total)
              ;; Now 4 nodes in distribution
              (length dist2))))))
    {cleanup}))"#,
        prelude = hash_ring_prelude(),
        cleanup = hash_ring_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: virtual node count impact on distribution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consistent_hash_vnode_impact() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}
  (unwind-protect
      (let ((keys nil))
        ;; Generate 60 keys
        (dotimes (i 60)
          (setq keys (cons (format "data-%d" i) keys)))
        (setq keys (nreverse keys))
        ;; Test with different vnode counts
        (let ((results nil))
          (dolist (vcount '(1 3 10 50))
            (let* ((ring (funcall 'neovm--chr-make-ring vcount))
                   (ring (funcall 'neovm--chr-add-node ring "N1"))
                   (ring (funcall 'neovm--chr-add-node ring "N2"))
                   (ring (funcall 'neovm--chr-add-node ring "N3"))
                   (dist (funcall 'neovm--chr-distribution ring keys))
                   ;; Compute max/min counts
                   (counts (mapcar #'cdr dist))
                   (max-count (apply #'max counts))
                   (min-count (apply #'min counts)))
              (setq results
                    (cons (list :vnodes vcount
                                :dist dist
                                :max max-count
                                :min min-count
                                :spread (- max-count min-count)
                                :total-vnodes (length (plist-get ring :vnodes)))
                          results))))
          (nreverse results)))
    {cleanup}))"#,
        prelude = hash_ring_prelude(),
        cleanup = hash_ring_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test: ring visualization and full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_consistent_hash_full_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {prelude}

  ;; Ring to sorted list of (hash . node-name) for visualization
  (fset 'neovm--chr-ring-view
    (lambda (ring)
      (mapcar (lambda (vn) (cons (car vn) (cadr vn)))
              (plist-get ring :vnodes))))

  (unwind-protect
      (let* (;; Start empty
             (ring (funcall 'neovm--chr-make-ring 3))
             (s0 (funcall 'neovm--chr-stats ring))
             ;; Add nodes one by one, track state
             (ring (funcall 'neovm--chr-add-node ring "web-1"))
             (s1 (funcall 'neovm--chr-stats ring))
             (v1 (funcall 'neovm--chr-ring-view ring))
             (ring (funcall 'neovm--chr-add-node ring "web-2"))
             (s2 (funcall 'neovm--chr-stats ring))
             (ring (funcall 'neovm--chr-add-node ring "web-3"))
             (s3 (funcall 'neovm--chr-stats ring))
             ;; Map some keys
             (keys '("session-A" "session-B" "session-C" "session-D" "session-E"))
             (mapping1 (mapcar (lambda (k) (cons k (funcall 'neovm--chr-get-node ring k))) keys))
             ;; Remove web-2
             (ring (funcall 'neovm--chr-remove-node ring "web-2"))
             (s4 (funcall 'neovm--chr-stats ring))
             (mapping2 (mapcar (lambda (k) (cons k (funcall 'neovm--chr-get-node ring k))) keys))
             ;; Add web-4
             (ring (funcall 'neovm--chr-add-node ring "web-4"))
             (s5 (funcall 'neovm--chr-stats ring))
             (mapping3 (mapcar (lambda (k) (cons k (funcall 'neovm--chr-get-node ring k))) keys)))
        (list s0 s1 s2 s3 s4 s5
              ;; Initial view with just web-1
              (length v1)
              ;; Mappings at each stage
              mapping1 mapping2 mapping3))
    (fmakunbound 'neovm--chr-ring-view)
    {cleanup}))"#,
        prelude = hash_ring_prelude(),
        cleanup = hash_ring_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}
