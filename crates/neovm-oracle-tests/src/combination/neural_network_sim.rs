//! Oracle parity tests for neural network simulation in pure Elisp:
//! matrix operations for forward pass, sigmoid/relu activation functions,
//! loss computation (MSE), gradient computation (numerical differentiation),
//! weight update (gradient descent step), multi-layer network, and batch
//! processing. All matrices represented as lists of lists.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Matrix primitives: creation, transpose, multiply
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_matrix_primitives() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Matrix as list-of-rows, each row is a list of numbers

  (fset 'neovm--nn-mat-rows (lambda (m) (length m)))
  (fset 'neovm--nn-mat-cols (lambda (m) (length (car m))))

  (fset 'neovm--nn-mat-transpose
    (lambda (m)
      (let ((ncols (funcall 'neovm--nn-mat-cols m))
            (result nil))
        (dotimes (j ncols)
          (let ((col nil))
            (dolist (row m)
              (push (nth j row) col))
            (push (nreverse col) result)))
        (nreverse result))))

  (fset 'neovm--nn-dot
    (lambda (v1 v2)
      (let ((sum 0.0))
        (while v1
          (setq sum (+ sum (* (car v1) (car v2))))
          (setq v1 (cdr v1) v2 (cdr v2)))
        sum)))

  (fset 'neovm--nn-mat-mul
    (lambda (a b)
      (let ((bt (funcall 'neovm--nn-mat-transpose b))
            (result nil))
        (dolist (row-a a)
          (let ((new-row nil))
            (dolist (col-b bt)
              (push (funcall 'neovm--nn-dot row-a col-b) new-row))
            (push (nreverse new-row) result)))
        (nreverse result))))

  (fset 'neovm--nn-mat-add
    (lambda (a b)
      (let ((result nil))
        (while a
          (let ((ra (car a)) (rb (car b)) (row nil))
            (while ra
              (push (+ (car ra) (car rb)) row)
              (setq ra (cdr ra) rb (cdr rb)))
            (push (nreverse row) result))
          (setq a (cdr a) b (cdr b)))
        (nreverse result))))

  (fset 'neovm--nn-mat-scale
    (lambda (m s)
      (mapcar (lambda (row) (mapcar (lambda (x) (* x s)) row)) m)))

  (unwind-protect
      (let ((a '((1.0 2.0) (3.0 4.0)))
            (b '((5.0 6.0) (7.0 8.0)))
            (v '((1.0 2.0 3.0))))
        (list
         ;; Transpose of 2x2
         (funcall 'neovm--nn-mat-transpose a)
         ;; Transpose of 1x3
         (funcall 'neovm--nn-mat-transpose v)
         ;; Matrix multiply 2x2 * 2x2
         (funcall 'neovm--nn-mat-mul a b)
         ;; Identity property: I * A = A
         (let ((eye '((1.0 0.0) (0.0 1.0))))
           (equal (funcall 'neovm--nn-mat-mul eye a) a))
         ;; Matrix addition
         (funcall 'neovm--nn-mat-add a b)
         ;; Scalar multiplication
         (funcall 'neovm--nn-mat-scale a 2.0)
         ;; Dot product
         (funcall 'neovm--nn-dot '(1.0 2.0 3.0) '(4.0 5.0 6.0))))
    (fmakunbound 'neovm--nn-mat-rows)
    (fmakunbound 'neovm--nn-mat-cols)
    (fmakunbound 'neovm--nn-mat-transpose)
    (fmakunbound 'neovm--nn-dot)
    (fmakunbound 'neovm--nn-mat-mul)
    (fmakunbound 'neovm--nn-mat-add)
    (fmakunbound 'neovm--nn-mat-scale)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1.0 3.0) (2.0 4.0)) ((1.0) (2.0) (3.0)) ((19.0 22.0) (43.0 50.0)) t ((6.0 8.0) (10.0 12.0)) ((2.0 4.0) (6.0 8.0)) 32.0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Activation functions: sigmoid and relu
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_activation_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--nn-sigmoid
    (lambda (x)
      (/ 1.0 (+ 1.0 (exp (- x))))))

  (fset 'neovm--nn-relu
    (lambda (x)
      (if (> x 0.0) x 0.0)))

  (fset 'neovm--nn-sigmoid-deriv
    (lambda (x)
      (let ((s (funcall 'neovm--nn-sigmoid x)))
        (* s (- 1.0 s)))))

  (fset 'neovm--nn-relu-deriv
    (lambda (x)
      (if (> x 0.0) 1.0 0.0)))

  ;; Apply activation elementwise to a matrix
  (fset 'neovm--nn-mat-apply
    (lambda (m f)
      (mapcar (lambda (row) (mapcar f row)) m)))

  (unwind-protect
      (list
       ;; Sigmoid of 0 = 0.5
       (funcall 'neovm--nn-sigmoid 0.0)
       ;; Sigmoid of large positive ~= 1.0
       (> (funcall 'neovm--nn-sigmoid 10.0) 0.99)
       ;; Sigmoid of large negative ~= 0.0
       (< (funcall 'neovm--nn-sigmoid -10.0) 0.01)
       ;; Sigmoid derivative at 0 = 0.25
       (funcall 'neovm--nn-sigmoid-deriv 0.0)
       ;; ReLU of positive
       (funcall 'neovm--nn-relu 3.5)
       ;; ReLU of negative
       (funcall 'neovm--nn-relu -2.0)
       ;; ReLU of zero
       (funcall 'neovm--nn-relu 0.0)
       ;; ReLU derivative
       (list (funcall 'neovm--nn-relu-deriv 5.0)
             (funcall 'neovm--nn-relu-deriv -3.0)
             (funcall 'neovm--nn-relu-deriv 0.0))
       ;; Apply sigmoid to matrix
       (funcall 'neovm--nn-mat-apply '((0.0 10.0) (-10.0 0.0))
                'neovm--nn-sigmoid)
       ;; Apply relu to matrix
       (funcall 'neovm--nn-mat-apply '((-1.0 2.0) (3.0 -4.0))
                'neovm--nn-relu))
    (fmakunbound 'neovm--nn-sigmoid)
    (fmakunbound 'neovm--nn-relu)
    (fmakunbound 'neovm--nn-sigmoid-deriv)
    (fmakunbound 'neovm--nn-relu-deriv)
    (fmakunbound 'neovm--nn-mat-apply)))"#;
    let expect = expect_test::expect![[
        r#""OK (0.5 t t 0.25 3.5 0.0 0.0 (1.0 0.0 0.0) ((0.5 0.9999546021312976) (4.5397868702434395e-05 0.5)) ((0.0 2.0) (3.0 0.0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// MSE loss computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_mse_loss() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Mean Squared Error: (1/n) * sum((predicted - actual)^2)
  (fset 'neovm--nn-mse
    (lambda (predicted actual)
      (let ((sum-sq 0.0) (n 0))
        (while predicted
          (let ((p (car predicted)) (a (car actual)))
            (while p
              (let ((diff (- (car p) (car a))))
                (setq sum-sq (+ sum-sq (* diff diff)))
                (setq n (1+ n)))
              (setq p (cdr p) a (cdr a))))
          (setq predicted (cdr predicted) actual (cdr actual)))
        (/ sum-sq (float n)))))

  ;; Element-wise subtraction
  (fset 'neovm--nn-mat-sub
    (lambda (a b)
      (let ((result nil))
        (while a
          (let ((ra (car a)) (rb (car b)) (row nil))
            (while ra
              (push (- (car ra) (car rb)) row)
              (setq ra (cdr ra) rb (cdr rb)))
            (push (nreverse row) result))
          (setq a (cdr a) b (cdr b)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Perfect prediction: MSE = 0
       (funcall 'neovm--nn-mse '((1.0 2.0 3.0)) '((1.0 2.0 3.0)))
       ;; Uniform error of 1.0: MSE = 1.0
       (funcall 'neovm--nn-mse '((2.0 3.0 4.0)) '((1.0 2.0 3.0)))
       ;; Single element
       (funcall 'neovm--nn-mse '((5.0)) '((3.0)))
       ;; Matrix subtraction for error
       (funcall 'neovm--nn-mat-sub '((3.0 2.0) (1.0 4.0)) '((1.0 1.0) (1.0 1.0)))
       ;; MSE with multi-row output
       (funcall 'neovm--nn-mse '((1.0 0.0) (0.0 1.0)) '((0.5 0.5) (0.5 0.5)))
       ;; MSE is always non-negative
       (>= (funcall 'neovm--nn-mse '((-3.0 7.0)) '((2.0 -1.0))) 0.0))
    (fmakunbound 'neovm--nn-mse)
    (fmakunbound 'neovm--nn-mat-sub)))"#;
    let expect = expect_test::expect![[r#""OK (0.0 1.0 4.0 ((2.0 1.0) (0.0 3.0)) 0.25 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Numerical gradient computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_numerical_gradient() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Numerical differentiation: df/dx ~ (f(x+h) - f(x-h)) / (2h)
  ;; Compute gradient of a scalar loss w.r.t. each weight in a matrix

  (fset 'neovm--nn-sigmoid-g
    (lambda (x)
      (/ 1.0 (+ 1.0 (exp (- x))))))

  ;; Forward pass: output = sigmoid(input * weights)
  ;; input: 1xN, weights: NxM, output: 1xM
  (fset 'neovm--nn-forward
    (lambda (input weights)
      (let ((result nil))
        (dolist (row (list input))
          (let ((new-row nil))
            ;; Multiply input vector by each column of weights
            (let ((wt-cols nil) (ncols (length (car weights))))
              (dotimes (j ncols)
                (let ((col nil))
                  (dolist (wr weights) (push (nth j wr) col))
                  (push (nreverse col) wt-cols)))
              (setq wt-cols (nreverse wt-cols))
              (dolist (col wt-cols)
                (let ((sum 0.0))
                  (let ((a input) (b col))
                    (while a
                      (setq sum (+ sum (* (car a) (car b))))
                      (setq a (cdr a) b (cdr b))))
                  (push (funcall 'neovm--nn-sigmoid-g sum) new-row))))
            (push (nreverse new-row) result)))
        (nreverse result))))

  (fset 'neovm--nn-loss-fn
    (lambda (pred target)
      (let ((sum 0.0))
        (let ((p (car pred)) (a (car target)))
          (while p
            (let ((d (- (car p) (car a))))
              (setq sum (+ sum (* d d))))
            (setq p (cdr p) a (cdr a))))
        (/ sum (float (length (car pred)))))))

  ;; Copy a matrix, replacing element (i,j) with val
  (fset 'neovm--nn-mat-set
    (lambda (m i j val)
      (let ((result nil) (ri 0))
        (dolist (row m)
          (if (= ri i)
              (let ((new-row nil) (cj 0))
                (dolist (x row)
                  (push (if (= cj j) val x) new-row)
                  (setq cj (1+ cj)))
                (push (nreverse new-row) result))
            (push (copy-sequence row) result))
          (setq ri (1+ ri)))
        (nreverse result))))

  ;; Numerical gradient of loss w.r.t. weights
  (fset 'neovm--nn-numerical-grad
    (lambda (input weights target h)
      (let ((grad nil) (nrows (length weights)) (ncols (length (car weights))))
        (dotimes (i nrows)
          (let ((grad-row nil))
            (dotimes (j ncols)
              (let* ((w-plus (funcall 'neovm--nn-mat-set weights i j
                                      (+ (nth j (nth i weights)) h)))
                     (w-minus (funcall 'neovm--nn-mat-set weights i j
                                       (- (nth j (nth i weights)) h)))
                     (loss-plus (funcall 'neovm--nn-loss-fn
                                         (funcall 'neovm--nn-forward input w-plus)
                                         target))
                     (loss-minus (funcall 'neovm--nn-loss-fn
                                          (funcall 'neovm--nn-forward input w-minus)
                                          target)))
                (push (/ (- loss-plus loss-minus) (* 2.0 h)) grad-row)))
            (push (nreverse grad-row) grad)))
        (nreverse grad))))

  (unwind-protect
      (let* ((input '(1.0 0.5))
             (weights '((0.3 0.7) (0.5 0.2)))
             (target '((0.9 0.1)))
             (grad (funcall 'neovm--nn-numerical-grad input weights target 0.0001)))
        (list
         ;; Gradient is a 2x2 matrix (same shape as weights)
         (length grad)
         (length (car grad))
         ;; Gradient values are finite numbers
         (numberp (car (car grad)))
         ;; Forward pass gives sensible output
         (funcall 'neovm--nn-forward input weights)
         ;; Loss is non-negative
         (>= (funcall 'neovm--nn-loss-fn
                       (funcall 'neovm--nn-forward input weights)
                       target)
              0.0)
         ;; mat-set works correctly
         (funcall 'neovm--nn-mat-set '((1.0 2.0) (3.0 4.0)) 0 1 99.0)))
    (fmakunbound 'neovm--nn-sigmoid-g)
    (fmakunbound 'neovm--nn-forward)
    (fmakunbound 'neovm--nn-loss-fn)
    (fmakunbound 'neovm--nn-mat-set)
    (fmakunbound 'neovm--nn-numerical-grad)))"#;
    let expect = expect_test::expect![[
        r#""OK (2 2 t ((0.6341355910108007 0.6899744811276125)) t ((1.0 99.0) (3.0 4.0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gradient descent weight update step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_gradient_descent_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; weight_new = weight_old - learning_rate * gradient

  (fset 'neovm--nn-update-weights
    (lambda (weights gradients lr)
      (let ((result nil))
        (let ((w weights) (g gradients))
          (while w
            (let ((wr (car w)) (gr (car g)) (row nil))
              (while wr
                (push (- (car wr) (* lr (car gr))) row)
                (setq wr (cdr wr) gr (cdr gr)))
              (push (nreverse row) result))
            (setq w (cdr w) g (cdr g))))
        (nreverse result))))

  ;; Check that all elements of updated are different from original
  ;; (unless gradient is zero)
  (fset 'neovm--nn-matrices-different
    (lambda (a b)
      (let ((diff nil))
        (let ((ra a) (rb b))
          (while ra
            (let ((rra (car ra)) (rrb (car rb)))
              (while rra
                (unless (= (car rra) (car rrb))
                  (setq diff t))
                (setq rra (cdr rra) rrb (cdr rrb))))
            (setq ra (cdr ra) rb (cdr rb))))
        diff)))

  (unwind-protect
      (let* ((w '((0.5 0.3) (0.7 0.1)))
             (g '((0.1 -0.2) (0.05 0.15)))
             (lr 0.01)
             (w-new (funcall 'neovm--nn-update-weights w g lr)))
        (list
         ;; Updated weights have same shape
         (length w-new)
         (length (car w-new))
         ;; Specific values: 0.5 - 0.01*0.1 = 0.499
         (car (car w-new))
         ;; 0.3 - 0.01*(-0.2) = 0.302
         (nth 1 (car w-new))
         ;; Weights changed
         (funcall 'neovm--nn-matrices-different w w-new)
         ;; Zero gradient means no change
         (equal w (funcall 'neovm--nn-update-weights w '((0.0 0.0) (0.0 0.0)) lr))
         ;; Large learning rate amplifies change
         (let* ((w-big (funcall 'neovm--nn-update-weights w g 1.0))
                (d-small (abs (- (car (car w-new)) (car (car w)))))
                (d-big (abs (- (car (car w-big)) (car (car w))))))
           (> d-big d-small))))
    (fmakunbound 'neovm--nn-update-weights)
    (fmakunbound 'neovm--nn-matrices-different)))"#;
    let expect = expect_test::expect![[r#""OK (2 2 0.499 0.302 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-layer forward pass
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_multi_layer_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--nn-sigmoid-ml
    (lambda (x) (/ 1.0 (+ 1.0 (exp (- x))))))

  (fset 'neovm--nn-relu-ml
    (lambda (x) (if (> x 0.0) x 0.0)))

  ;; Vector-matrix multiply: vec (1D list) * mat -> vec
  (fset 'neovm--nn-vec-mat-mul
    (lambda (vec mat)
      (let ((ncols (length (car mat)))
            (result nil))
        (dotimes (j ncols)
          (let ((sum 0.0) (v vec) (rows mat))
            (while v
              (setq sum (+ sum (* (car v) (nth j (car rows)))))
              (setq v (cdr v) rows (cdr rows)))
            (push sum result)))
        (nreverse result))))

  ;; Add bias vector to result vector
  (fset 'neovm--nn-vec-add
    (lambda (a b)
      (let ((result nil))
        (while a
          (push (+ (car a) (car b)) result)
          (setq a (cdr a) b (cdr b)))
        (nreverse result))))

  ;; Multi-layer forward pass
  ;; layers: list of (weights bias activation) triples
  ;; Returns final output vector
  (fset 'neovm--nn-multi-forward
    (lambda (input layers)
      (let ((current input))
        (dolist (layer layers)
          (let ((w (nth 0 layer))
                (b (nth 1 layer))
                (act (nth 2 layer)))
            (setq current (funcall 'neovm--nn-vec-mat-mul current w))
            (setq current (funcall 'neovm--nn-vec-add current b))
            (setq current (mapcar act current))))
        current)))

  (unwind-protect
      (let* (;; 2-input, 3-hidden, 1-output network
             (w1 '((0.1 0.2 0.3) (0.4 0.5 0.6)))    ;; 2x3
             (b1 '(0.01 0.02 0.03))                    ;; 3
             (w2 '((0.7) (0.8) (0.9)))                 ;; 3x1
             (b2 '(0.04))                               ;; 1
             (layers (list (list w1 b1 'neovm--nn-sigmoid-ml)
                           (list w2 b2 'neovm--nn-sigmoid-ml)))
             (input '(1.0 0.5)))
        (list
         ;; Single-layer forward
         (funcall 'neovm--nn-vec-mat-mul input w1)
         ;; With bias
         (funcall 'neovm--nn-vec-add
                  (funcall 'neovm--nn-vec-mat-mul input w1) b1)
         ;; Full forward pass gives 1-element output
         (length (funcall 'neovm--nn-multi-forward input layers))
         ;; Output is between 0 and 1 (sigmoid output)
         (let ((out (car (funcall 'neovm--nn-multi-forward input layers))))
           (and (> out 0.0) (< out 1.0)))
         ;; Different inputs give different outputs
         (let ((out1 (funcall 'neovm--nn-multi-forward '(1.0 0.0) layers))
               (out2 (funcall 'neovm--nn-multi-forward '(0.0 1.0) layers)))
           (not (equal out1 out2)))
         ;; ReLU layer
         (let ((relu-layers (list (list w1 b1 'neovm--nn-relu-ml))))
           (funcall 'neovm--nn-multi-forward input relu-layers))
         ;; Zero input
         (funcall 'neovm--nn-multi-forward '(0.0 0.0) layers)))
    (fmakunbound 'neovm--nn-sigmoid-ml)
    (fmakunbound 'neovm--nn-relu-ml)
    (fmakunbound 'neovm--nn-vec-mat-mul)
    (fmakunbound 'neovm--nn-vec-add)
    (fmakunbound 'neovm--nn-multi-forward)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0.30000000000000004 0.45 0.6) (0.31000000000000005 0.47000000000000003 0.63) 1 t t (0.31000000000000005 0.47000000000000003 0.63) (0.7777322100392836))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Batch processing: run multiple samples through the network
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nn_batch_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--nn-sigmoid-bp
    (lambda (x) (/ 1.0 (+ 1.0 (exp (- x))))))

  (fset 'neovm--nn-vec-mat-mul-bp
    (lambda (vec mat)
      (let ((ncols (length (car mat))) (result nil))
        (dotimes (j ncols)
          (let ((sum 0.0) (v vec) (rows mat))
            (while v
              (setq sum (+ sum (* (car v) (nth j (car rows)))))
              (setq v (cdr v) rows (cdr rows)))
            (push sum result)))
        (nreverse result))))

  (fset 'neovm--nn-vec-add-bp
    (lambda (a b)
      (let ((result nil))
        (while a
          (push (+ (car a) (car b)) result)
          (setq a (cdr a) b (cdr b)))
        (nreverse result))))

  (fset 'neovm--nn-predict
    (lambda (input w b)
      (mapcar 'neovm--nn-sigmoid-bp
              (funcall 'neovm--nn-vec-add-bp
                       (funcall 'neovm--nn-vec-mat-mul-bp input w) b))))

  ;; Process a batch and compute average loss
  (fset 'neovm--nn-batch-loss
    (lambda (batch-inputs batch-targets w b)
      (let ((total-loss 0.0) (n 0))
        (let ((ins batch-inputs) (tgts batch-targets))
          (while ins
            (let* ((pred (funcall 'neovm--nn-predict (car ins) w b))
                   (tgt (car tgts))
                   (p pred) (t1 tgt))
              (while p
                (let ((d (- (car p) (car t1))))
                  (setq total-loss (+ total-loss (* d d))))
                (setq p (cdr p) t1 (cdr t1))))
            (setq n (1+ n))
            (setq ins (cdr ins) tgts (cdr tgts))))
        (/ total-loss (float n)))))

  ;; Classify: threshold at 0.5
  (fset 'neovm--nn-classify
    (lambda (output)
      (mapcar (lambda (x) (if (>= x 0.5) 1 0)) output)))

  ;; Accuracy over batch
  (fset 'neovm--nn-accuracy
    (lambda (batch-inputs batch-targets w b)
      (let ((correct 0) (total 0))
        (let ((ins batch-inputs) (tgts batch-targets))
          (while ins
            (let* ((pred (funcall 'neovm--nn-classify
                                  (funcall 'neovm--nn-predict (car ins) w b)))
                   (tgt (mapcar (lambda (x) (if (>= x 0.5) 1 0)) (car tgts)))
                   (p pred) (t1 tgt))
              (while p
                (when (= (car p) (car t1))
                  (setq correct (1+ correct)))
                (setq total (1+ total))
                (setq p (cdr p) t1 (cdr t1))))
            (setq ins (cdr ins) tgts (cdr tgts))))
        (/ (float correct) (float total)))))

  (unwind-protect
      (let* ((w '((0.5 -0.5) (0.3 0.7)))  ;; 2x2 weights
             (b '(0.1 -0.1))                ;; 2 biases
             (inputs '((1.0 0.0) (0.0 1.0) (1.0 1.0) (0.0 0.0)))
             (targets '((0.9 0.1) (0.1 0.9) (0.9 0.9) (0.1 0.1))))
        (list
         ;; Single prediction
         (funcall 'neovm--nn-predict '(1.0 0.0) w b)
         ;; Classification output
         (funcall 'neovm--nn-classify
                  (funcall 'neovm--nn-predict '(1.0 0.0) w b))
         ;; Batch loss is non-negative
         (>= (funcall 'neovm--nn-batch-loss inputs targets w b) 0.0)
         ;; Batch loss value
         (funcall 'neovm--nn-batch-loss inputs targets w b)
         ;; Accuracy is between 0 and 1
         (let ((acc (funcall 'neovm--nn-accuracy inputs targets w b)))
           (and (>= acc 0.0) (<= acc 1.0)))
         ;; All predictions have length 2
         (let ((all-len-2 t))
           (dolist (inp inputs)
             (unless (= (length (funcall 'neovm--nn-predict inp w b)) 2)
               (setq all-len-2 nil)))
           all-len-2)
         ;; Batch of size 1 works
         (funcall 'neovm--nn-batch-loss '((0.5 0.5)) '((0.5 0.5)) w b)))
    (fmakunbound 'neovm--nn-sigmoid-bp)
    (fmakunbound 'neovm--nn-vec-mat-mul-bp)
    (fmakunbound 'neovm--nn-vec-add-bp)
    (fmakunbound 'neovm--nn-predict)
    (fmakunbound 'neovm--nn-batch-loss)
    (fmakunbound 'neovm--nn-classify)
    (fmakunbound 'neovm--nn-accuracy)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0.6456563062257954 0.35434369377420455) (1 0) t 0.23509753650811427 t t 0.014996287798405518)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
