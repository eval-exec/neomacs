//! Oracle parity tests for a blockchain simulation in Elisp.
//!
//! Covers: block structure (index, timestamp, data, prev-hash, hash),
//! hash computation (simple hash function), chain validation,
//! proof-of-work mining, transaction pool, Merkle tree construction,
//! chain fork resolution (longest chain rule).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Block structure and simple hash function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_block_structure_and_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple hash: sum of char codes modulo a large prime, output as hex string
  (fset 'neovm--bc-simple-hash
    (lambda (data)
      (let ((h 5381)
            (s (format "%s" data)))
        (dotimes (i (length s))
          (setq h (% (+ (* h 33) (aref s i)) 4294967291)))
        (format "%08x" h))))

  ;; Block: (index timestamp data prev-hash hash)
  (fset 'neovm--bc-make-block
    (lambda (index timestamp data prev-hash)
      (let* ((content (format "%d:%d:%s:%s" index timestamp data prev-hash))
             (hash (funcall 'neovm--bc-simple-hash content)))
        (list index timestamp data prev-hash hash))))

  (fset 'neovm--bc-block-index (lambda (b) (nth 0 b)))
  (fset 'neovm--bc-block-timestamp (lambda (b) (nth 1 b)))
  (fset 'neovm--bc-block-data (lambda (b) (nth 2 b)))
  (fset 'neovm--bc-block-prev-hash (lambda (b) (nth 3 b)))
  (fset 'neovm--bc-block-hash (lambda (b) (nth 4 b)))

  (unwind-protect
      (let* ((genesis (funcall 'neovm--bc-make-block 0 1000 "Genesis" "0000000000"))
             (b1 (funcall 'neovm--bc-make-block 1 1001 "Transfer:A->B:50"
                          (funcall 'neovm--bc-block-hash genesis)))
             (b2 (funcall 'neovm--bc-make-block 2 1002 "Transfer:B->C:25"
                          (funcall 'neovm--bc-block-hash b1))))
        (list
          ;; Genesis block fields
          (funcall 'neovm--bc-block-index genesis)
          (funcall 'neovm--bc-block-data genesis)
          (stringp (funcall 'neovm--bc-block-hash genesis))
          (= (length (funcall 'neovm--bc-block-hash genesis)) 8)
          ;; Chain linkage: b1.prev-hash == genesis.hash
          (string= (funcall 'neovm--bc-block-prev-hash b1)
                   (funcall 'neovm--bc-block-hash genesis))
          ;; Chain linkage: b2.prev-hash == b1.hash
          (string= (funcall 'neovm--bc-block-prev-hash b2)
                   (funcall 'neovm--bc-block-hash b1))
          ;; Hashes are deterministic
          (string= (funcall 'neovm--bc-block-hash genesis)
                   (funcall 'neovm--bc-block-hash
                            (funcall 'neovm--bc-make-block 0 1000 "Genesis" "0000000000")))
          ;; Different data -> different hash
          (not (string= (funcall 'neovm--bc-block-hash genesis)
                        (funcall 'neovm--bc-block-hash b1)))))
    (fmakunbound 'neovm--bc-simple-hash)
    (fmakunbound 'neovm--bc-make-block)
    (fmakunbound 'neovm--bc-block-index)
    (fmakunbound 'neovm--bc-block-timestamp)
    (fmakunbound 'neovm--bc-block-data)
    (fmakunbound 'neovm--bc-block-prev-hash)
    (fmakunbound 'neovm--bc-block-hash)))"#;
    let expect = expect_test::expect![[r#""OK (0 \"Genesis\" t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chain validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_chain_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bc2-hash
    (lambda (data)
      (let ((h 5381)
            (s (format "%s" data)))
        (dotimes (i (length s))
          (setq h (% (+ (* h 33) (aref s i)) 4294967291)))
        (format "%08x" h))))

  (fset 'neovm--bc2-make-block
    (lambda (index ts data prev-hash)
      (let* ((content (format "%d:%d:%s:%s" index ts data prev-hash))
             (hash (funcall 'neovm--bc2-hash content)))
        (list index ts data prev-hash hash))))

  (fset 'neovm--bc2-validate-chain
    (lambda (chain)
      "Validate chain: each block's prev-hash matches previous block's hash,
       and each block's hash is correctly computed."
      (let ((valid t)
            (prev nil))
        (dolist (block chain)
          (when prev
            ;; Check prev-hash linkage
            (unless (string= (nth 3 block) (nth 4 prev))
              (setq valid nil))
            ;; Verify hash integrity
            (let* ((content (format "%d:%d:%s:%s" (nth 0 block) (nth 1 block)
                                    (nth 2 block) (nth 3 block)))
                   (expected (funcall 'neovm--bc2-hash content)))
              (unless (string= (nth 4 block) expected)
                (setq valid nil))))
          (setq prev block))
        valid)))

  (unwind-protect
      (let* ((g (funcall 'neovm--bc2-make-block 0 100 "genesis" "0"))
             (b1 (funcall 'neovm--bc2-make-block 1 101 "tx1" (nth 4 g)))
             (b2 (funcall 'neovm--bc2-make-block 2 102 "tx2" (nth 4 b1)))
             (valid-chain (list g b1 b2))
             ;; Tampered chain: modify b1's data but keep old hash
             (b1-tampered (list 1 101 "TAMPERED" (nth 3 b1) (nth 4 b1)))
             (tampered-chain (list g b1-tampered b2)))
        (list
          (funcall 'neovm--bc2-validate-chain valid-chain)
          (funcall 'neovm--bc2-validate-chain tampered-chain)
          ;; Single block chain is valid
          (funcall 'neovm--bc2-validate-chain (list g))
          ;; Empty chain is valid
          (funcall 'neovm--bc2-validate-chain nil)))
    (fmakunbound 'neovm--bc2-hash)
    (fmakunbound 'neovm--bc2-make-block)
    (fmakunbound 'neovm--bc2-validate-chain)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Proof-of-work mining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_proof_of_work() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bc3-hash
    (lambda (data)
      (let ((h 5381)
            (s (format "%s" data)))
        (dotimes (i (length s))
          (setq h (% (+ (* h 33) (aref s i)) 4294967291)))
        (format "%08x" h))))

  ;; Mine a block: find nonce such that hash starts with given prefix
  (fset 'neovm--bc3-mine
    (lambda (index ts data prev-hash prefix)
      (let ((nonce 0)
            (found nil))
        (while (and (not found) (< nonce 10000))
          (let* ((content (format "%d:%d:%s:%s:%d" index ts data prev-hash nonce))
                 (hash (funcall 'neovm--bc3-hash content)))
            (if (string-prefix-p prefix hash)
                (setq found (list nonce hash))
              (setq nonce (1+ nonce)))))
        found)))

  (unwind-protect
      (let* ((result (funcall 'neovm--bc3-mine 1 200 "test-data" "abcdef00" "0"))
             (nonce (car result))
             (hash (cadr result)))
        (list
          ;; Mining found a result
          (not (null result))
          ;; Nonce is a non-negative integer
          (>= nonce 0)
          ;; Hash starts with required prefix
          (string-prefix-p "0" hash)
          ;; Hash is 8 chars
          (= (length hash) 8)
          ;; Verify: re-hashing with the found nonce gives same hash
          (string= hash
                   (funcall 'neovm--bc3-hash
                            (format "%d:%d:%s:%s:%d" 1 200 "test-data" "abcdef00" nonce)))))
    (fmakunbound 'neovm--bc3-hash)
    (fmakunbound 'neovm--bc3-mine)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transaction pool with priority
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_transaction_pool() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Transaction: (id from to amount fee timestamp)
  ;; Pool: sorted by fee descending (miners pick highest fees first)
  (defvar neovm--bc4-pool nil)

  (fset 'neovm--bc4-add-tx
    (lambda (id from to amount fee ts)
      (let ((tx (list id from to amount fee ts)))
        (setq neovm--bc4-pool (cons tx neovm--bc4-pool))
        ;; Keep sorted by fee descending
        (setq neovm--bc4-pool
              (sort neovm--bc4-pool
                    (lambda (a b) (> (nth 4 a) (nth 4 b))))))))

  (fset 'neovm--bc4-pick-txs
    (lambda (max-count)
      "Pick top MAX-COUNT transactions by fee."
      (let ((result nil)
            (remaining neovm--bc4-pool)
            (count 0))
        (while (and remaining (< count max-count))
          (setq result (cons (car remaining) result))
          (setq remaining (cdr remaining))
          (setq count (1+ count)))
        (setq neovm--bc4-pool remaining)
        (nreverse result))))

  (fset 'neovm--bc4-total-fees
    (lambda (txs)
      (let ((total 0))
        (dolist (tx txs)
          (setq total (+ total (nth 4 tx))))
        total)))

  (unwind-protect
      (progn
        (setq neovm--bc4-pool nil)
        (funcall 'neovm--bc4-add-tx "tx1" "alice" "bob" 100 5 1000)
        (funcall 'neovm--bc4-add-tx "tx2" "bob" "charlie" 50 15 1001)
        (funcall 'neovm--bc4-add-tx "tx3" "charlie" "dave" 200 3 1002)
        (funcall 'neovm--bc4-add-tx "tx4" "dave" "eve" 75 20 1003)
        (funcall 'neovm--bc4-add-tx "tx5" "eve" "alice" 30 8 1004)

        (let* ((pool-size (length neovm--bc4-pool))
               ;; Pick top 3 by fee
               (picked (funcall 'neovm--bc4-pick-txs 3))
               (picked-ids (mapcar #'car picked))
               (picked-fees (mapcar (lambda (tx) (nth 4 tx)) picked))
               (total-fee (funcall 'neovm--bc4-total-fees picked))
               (remaining (length neovm--bc4-pool)))
          (list
            pool-size          ;; 5
            picked-ids         ;; (tx4 tx2 tx5) - top 3 by fee
            picked-fees        ;; (20 15 8)
            total-fee          ;; 43
            remaining)))       ;; 2
    (fmakunbound 'neovm--bc4-add-tx)
    (fmakunbound 'neovm--bc4-pick-txs)
    (fmakunbound 'neovm--bc4-total-fees)
    (makunbound 'neovm--bc4-pool)))"#;
    let expect = expect_test::expect![[r#""OK (5 (\"tx4\" \"tx2\" \"tx5\") (20 15 8) 43 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Merkle tree construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_merkle_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bc5-hash
    (lambda (data)
      (let ((h 5381)
            (s (format "%s" data)))
        (dotimes (i (length s))
          (setq h (% (+ (* h 33) (aref s i)) 4294967291)))
        (format "%08x" h))))

  ;; Build Merkle tree from list of data items
  ;; Returns tree as nested list: (hash left right) or (hash data) for leaf
  (fset 'neovm--bc5-merkle-leaf
    (lambda (data)
      (list (funcall 'neovm--bc5-hash data) data)))

  (fset 'neovm--bc5-merkle-node
    (lambda (left right)
      (let ((combined-hash
             (funcall 'neovm--bc5-hash (concat (car left) (car right)))))
        (list combined-hash left right))))

  (fset 'neovm--bc5-build-merkle
    (lambda (items)
      "Build Merkle tree bottom-up. If odd count, duplicate last."
      (if (null items) nil
        (let ((nodes (mapcar (lambda (d) (funcall 'neovm--bc5-merkle-leaf d)) items)))
          ;; Repeatedly pair up nodes until one root remains
          (while (> (length nodes) 1)
            (let ((next nil)
                  (rest nodes))
              (while rest
                (let ((left (car rest))
                      (right (or (cadr rest) (car rest))))  ;; duplicate if odd
                  (setq next (cons (funcall 'neovm--bc5-merkle-node left right) next))
                  (setq rest (cddr rest))))
              (setq nodes (nreverse next))))
          (car nodes)))))

  (fset 'neovm--bc5-merkle-root
    (lambda (tree)
      (car tree)))

  (unwind-protect
      (let* ((tree4 (funcall 'neovm--bc5-build-merkle '("tx1" "tx2" "tx3" "tx4")))
             (tree3 (funcall 'neovm--bc5-build-merkle '("tx1" "tx2" "tx3")))
             (tree1 (funcall 'neovm--bc5-build-merkle '("tx1")))
             ;; Rebuild with same data should give same root
             (tree4b (funcall 'neovm--bc5-build-merkle '("tx1" "tx2" "tx3" "tx4")))
             ;; Different data -> different root
             (tree4c (funcall 'neovm--bc5-build-merkle '("tx1" "tx2" "tx3" "tx5"))))
        (list
          ;; Root hash is a string
          (stringp (funcall 'neovm--bc5-merkle-root tree4))
          ;; Deterministic
          (string= (funcall 'neovm--bc5-merkle-root tree4)
                   (funcall 'neovm--bc5-merkle-root tree4b))
          ;; Different data -> different root
          (not (string= (funcall 'neovm--bc5-merkle-root tree4)
                        (funcall 'neovm--bc5-merkle-root tree4c)))
          ;; Tree structure: root has 3 elements (hash left right)
          (= (length tree4) 3)
          ;; Single item tree is a leaf
          (= (length tree1) 2)
          ;; Odd-count tree still works
          (stringp (funcall 'neovm--bc5-merkle-root tree3))))
    (fmakunbound 'neovm--bc5-hash)
    (fmakunbound 'neovm--bc5-merkle-leaf)
    (fmakunbound 'neovm--bc5-merkle-node)
    (fmakunbound 'neovm--bc5-build-merkle)
    (fmakunbound 'neovm--bc5-merkle-root)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chain fork resolution: longest chain wins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_fork_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bc6-hash
    (lambda (data)
      (let ((h 5381)
            (s (format "%s" data)))
        (dotimes (i (length s))
          (setq h (% (+ (* h 33) (aref s i)) 4294967291)))
        (format "%08x" h))))

  (fset 'neovm--bc6-make-block
    (lambda (idx ts data prev-hash)
      (let* ((content (format "%d:%d:%s:%s" idx ts data prev-hash))
             (hash (funcall 'neovm--bc6-hash content)))
        (list idx ts data prev-hash hash))))

  (fset 'neovm--bc6-chain-length (lambda (chain) (length chain)))

  (fset 'neovm--bc6-chain-valid-p
    (lambda (chain)
      (let ((valid t) (prev nil))
        (dolist (blk chain)
          (when prev
            (unless (string= (nth 3 blk) (nth 4 prev))
              (setq valid nil)))
          (setq prev blk))
        valid)))

  ;; Resolve fork: pick longest valid chain
  (fset 'neovm--bc6-resolve-fork
    (lambda (chains)
      (let ((best nil)
            (best-len 0))
        (dolist (chain chains)
          (when (and (funcall 'neovm--bc6-chain-valid-p chain)
                     (> (funcall 'neovm--bc6-chain-length chain) best-len))
            (setq best chain)
            (setq best-len (funcall 'neovm--bc6-chain-length chain))))
        best)))

  (unwind-protect
      (let* ((g (funcall 'neovm--bc6-make-block 0 0 "genesis" "0"))
             ;; Chain A: genesis -> a1 -> a2 (length 3)
             (a1 (funcall 'neovm--bc6-make-block 1 1 "a-tx1" (nth 4 g)))
             (a2 (funcall 'neovm--bc6-make-block 2 2 "a-tx2" (nth 4 a1)))
             (chain-a (list g a1 a2))
             ;; Chain B: genesis -> b1 -> b2 -> b3 (length 4, longer)
             (b1 (funcall 'neovm--bc6-make-block 1 1 "b-tx1" (nth 4 g)))
             (b2 (funcall 'neovm--bc6-make-block 2 2 "b-tx2" (nth 4 b1)))
             (b3 (funcall 'neovm--bc6-make-block 3 3 "b-tx3" (nth 4 b2)))
             (chain-b (list g b1 b2 b3))
             ;; Chain C: invalid (broken linkage)
             (c1 (funcall 'neovm--bc6-make-block 1 1 "c-tx1" "WRONG-HASH"))
             (chain-c (list g c1))
             ;; Resolve
             (winner (funcall 'neovm--bc6-resolve-fork
                              (list chain-a chain-b chain-c))))
        (list
          ;; Chain lengths
          (funcall 'neovm--bc6-chain-length chain-a)
          (funcall 'neovm--bc6-chain-length chain-b)
          ;; Validity
          (funcall 'neovm--bc6-chain-valid-p chain-a)
          (funcall 'neovm--bc6-chain-valid-p chain-b)
          (funcall 'neovm--bc6-chain-valid-p chain-c)
          ;; Winner is chain-b (longest valid)
          (funcall 'neovm--bc6-chain-length winner)
          (string= (nth 4 (car (last winner))) (nth 4 b3))
          ;; Winner shares genesis with chain-a
          (string= (nth 4 (car winner)) (nth 4 (car chain-a)))))
    (fmakunbound 'neovm--bc6-hash)
    (fmakunbound 'neovm--bc6-make-block)
    (fmakunbound 'neovm--bc6-chain-length)
    (fmakunbound 'neovm--bc6-chain-valid-p)
    (fmakunbound 'neovm--bc6-resolve-fork)))"#;
    let expect = expect_test::expect![[r#""OK (3 4 t t nil 4 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full mini-blockchain: genesis, add transactions, mine blocks, validate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_blockchain_full_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--bc7-chain nil)
  (defvar neovm--bc7-pending nil)

  (fset 'neovm--bc7-hash
    (lambda (data)
      (let ((h 5381) (s (format "%s" data)))
        (dotimes (i (length s))
          (setq h (% (+ (* h 33) (aref s i)) 4294967291)))
        (format "%08x" h))))

  (fset 'neovm--bc7-make-block
    (lambda (idx ts data prev)
      (let* ((c (format "%d:%d:%s:%s" idx ts data prev))
             (hash (funcall 'neovm--bc7-hash c)))
        (list idx ts data prev hash))))

  (fset 'neovm--bc7-latest-hash
    (lambda () (nth 4 (car (last neovm--bc7-chain)))))

  (fset 'neovm--bc7-add-pending
    (lambda (from to amount)
      (setq neovm--bc7-pending
            (cons (format "%s->%s:%d" from to amount) neovm--bc7-pending))))

  (fset 'neovm--bc7-mine-block
    (lambda (ts)
      "Mine a block with all pending transactions."
      (let* ((data (mapconcat #'identity (nreverse neovm--bc7-pending) ";"))
             (idx (length neovm--bc7-chain))
             (prev (funcall 'neovm--bc7-latest-hash))
             (blk (funcall 'neovm--bc7-make-block idx ts data prev)))
        (setq neovm--bc7-chain (append neovm--bc7-chain (list blk)))
        (setq neovm--bc7-pending nil)
        blk)))

  (fset 'neovm--bc7-balances
    (lambda ()
      "Compute balances from chain (skip genesis). Each tx: 'from->to:amount'"
      (let ((bal nil))
        (dolist (blk (cdr neovm--bc7-chain))
          (let ((txs (split-string (nth 2 blk) ";")))
            (dolist (tx txs)
              (when (string-match "\\`\\([^-]+\\)->\\([^:]+\\):\\([0-9]+\\)\\'" tx)
                (let ((from (match-string 1 tx))
                      (to (match-string 2 tx))
                      (amt (string-to-number (match-string 3 tx))))
                  (let ((fb (assoc from bal)))
                    (if fb (setcdr fb (- (cdr fb) amt))
                      (setq bal (cons (cons from (- amt)) bal))))
                  (let ((tb (assoc to bal)))
                    (if tb (setcdr tb (+ (cdr tb) amt))
                      (setq bal (cons (cons to amt) bal)))))))))
        (sort bal (lambda (a b) (string< (car a) (car b)))))))

  (unwind-protect
      (progn
        (setq neovm--bc7-chain nil)
        (setq neovm--bc7-pending nil)
        ;; Genesis
        (setq neovm--bc7-chain
              (list (funcall 'neovm--bc7-make-block 0 0 "GENESIS" "0")))
        ;; Add transactions and mine block 1
        (funcall 'neovm--bc7-add-pending "alice" "bob" 50)
        (funcall 'neovm--bc7-add-pending "alice" "charlie" 30)
        (funcall 'neovm--bc7-mine-block 100)
        ;; Mine block 2
        (funcall 'neovm--bc7-add-pending "bob" "charlie" 20)
        (funcall 'neovm--bc7-mine-block 200)

        (let ((balances (funcall 'neovm--bc7-balances)))
          (list
            ;; Chain length
            (length neovm--bc7-chain)
            ;; Balances
            balances
            ;; Alice: -50 -30 = -80
            (cdr (assoc "alice" balances))
            ;; Bob: +50 -20 = 30
            (cdr (assoc "bob" balances))
            ;; Charlie: +30 +20 = 50
            (cdr (assoc "charlie" balances))
            ;; No pending
            (null neovm--bc7-pending))))
    (fmakunbound 'neovm--bc7-hash)
    (fmakunbound 'neovm--bc7-make-block)
    (fmakunbound 'neovm--bc7-latest-hash)
    (fmakunbound 'neovm--bc7-add-pending)
    (fmakunbound 'neovm--bc7-mine-block)
    (fmakunbound 'neovm--bc7-balances)
    (makunbound 'neovm--bc7-chain)
    (makunbound 'neovm--bc7-pending)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 ((\"alice\" . -80) (\"bob\" . 30) (\"charlie\" . 50)) -80 30 50 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
