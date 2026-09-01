//! Oracle parity tests for physics simulation implemented in pure Elisp.
//!
//! Covers: 2D vector physics (position, velocity, acceleration),
//! Euler and Verlet integration, collision detection (circle-circle,
//! point-in-rect), gravitational N-body simulation, spring-mass system,
//! projectile motion with drag, and elastic/inelastic collision response.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 2D vector operations and basic kinematics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_2d_vector_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; 2D vector as (x . y)
  (fset 'neovm--phys-vec2 (lambda (x y) (cons x y)))
  (fset 'neovm--phys-vx (lambda (v) (car v)))
  (fset 'neovm--phys-vy (lambda (v) (cdr v)))

  ;; Vector arithmetic
  (fset 'neovm--phys-vadd
    (lambda (a b) (cons (+ (car a) (car b)) (+ (cdr a) (cdr b)))))
  (fset 'neovm--phys-vsub
    (lambda (a b) (cons (- (car a) (car b)) (- (cdr a) (cdr b)))))
  (fset 'neovm--phys-vscale
    (lambda (v s) (cons (* (car v) s) (* (cdr v) s))))
  (fset 'neovm--phys-vdot
    (lambda (a b) (+ (* (car a) (car b)) (* (cdr a) (cdr b)))))
  (fset 'neovm--phys-vmag-sq
    (lambda (v) (+ (* (car v) (car v)) (* (cdr v) (cdr v)))))
  (fset 'neovm--phys-vneg
    (lambda (v) (cons (- (car v)) (- (cdr v)))))

  ;; Kinematic update: pos += vel*dt + 0.5*acc*dt^2, vel += acc*dt
  ;; Using integer arithmetic scaled by 1000 (milli-units)
  (fset 'neovm--phys-kinematic-step
    (lambda (pos vel acc dt)
      "Return (new-pos . new-vel) using Euler integration. All values in milli-units."
      (let* ((half-acc-dt2 (funcall 'neovm--phys-vscale acc (/ (* dt dt) 2000)))
             (vel-dt (funcall 'neovm--phys-vscale vel (/ dt 1)))
             (new-pos (funcall 'neovm--phys-vadd pos
                        (funcall 'neovm--phys-vadd
                          (funcall 'neovm--phys-vscale vel dt)
                          (funcall 'neovm--phys-vscale acc (/ (* dt dt) 2)))))
             (new-vel (funcall 'neovm--phys-vadd vel
                        (funcall 'neovm--phys-vscale acc dt))))
        (cons new-pos new-vel))))

  (unwind-protect
      (list
       ;; Basic vector ops
       (funcall 'neovm--phys-vadd '(3 . 4) '(1 . 2))
       (funcall 'neovm--phys-vsub '(10 . 20) '(3 . 7))
       (funcall 'neovm--phys-vscale '(3 . 4) 5)
       (funcall 'neovm--phys-vdot '(3 . 4) '(1 . 2))   ;; 3+8=11
       (funcall 'neovm--phys-vmag-sq '(3 . 4))          ;; 9+16=25
       (funcall 'neovm--phys-vneg '(5 . -3))
       ;; Kinematic: start at (0,0), vel (10,0), acc (0,-1), dt=1
       (let* ((result (funcall 'neovm--phys-kinematic-step
                        '(0 . 0) '(10 . 0) '(0 . -1) 1))
              (pos (car result))
              (vel (cdr result)))
         (list pos vel))
       ;; Multiple steps of free fall
       (let ((pos '(0 . 1000))
             (vel '(0 . 0))
             (acc '(0 . -10))
             (dt 1)
             (steps 5)
             (trajectory nil))
         (dotimes (i steps)
           (setq trajectory (cons pos trajectory))
           (let ((result (funcall 'neovm--phys-kinematic-step pos vel acc dt)))
             (setq pos (car result))
             (setq vel (cdr result))))
         (setq trajectory (cons pos trajectory))
         (nreverse trajectory)))
    (fmakunbound 'neovm--phys-vec2)
    (fmakunbound 'neovm--phys-vx)
    (fmakunbound 'neovm--phys-vy)
    (fmakunbound 'neovm--phys-vadd)
    (fmakunbound 'neovm--phys-vsub)
    (fmakunbound 'neovm--phys-vscale)
    (fmakunbound 'neovm--phys-vdot)
    (fmakunbound 'neovm--phys-vmag-sq)
    (fmakunbound 'neovm--phys-vneg)
    (fmakunbound 'neovm--phys-kinematic-step)))"####;
    let expect = expect_test::expect![[
        r#""OK ((4 . 6) (7 . 13) (15 . 20) 11 25 (-5 . 3) ((10 . 0) (10 . -1)) ((0 . 1000) (0 . 1000) (0 . 990) (0 . 970) (0 . 940) (0 . 900)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Verlet integration for stable simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_verlet_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Stormer-Verlet: x(t+dt) = 2*x(t) - x(t-dt) + a(t)*dt^2
  ;; More stable than Euler for oscillatory systems
  (fset 'neovm--verlet-step
    (lambda (x-curr x-prev acc dt)
      "Verlet integration step. Returns new position."
      (let ((two-x (cons (* 2 (car x-curr)) (* 2 (cdr x-curr))))
            (neg-prev (cons (- (car x-prev)) (- (cdr x-prev))))
            (acc-dt2 (cons (* (car acc) dt dt) (* (cdr acc) dt dt))))
        (cons (+ (car two-x) (car neg-prev) (car acc-dt2))
              (+ (cdr two-x) (cdr neg-prev) (cdr acc-dt2))))))

  ;; Velocity from Verlet positions
  (fset 'neovm--verlet-velocity
    (lambda (x-next x-prev dt)
      "Estimate velocity: (x(t+dt) - x(t-dt)) / (2*dt)"
      (cons (/ (- (car x-next) (car x-prev)) (* 2 dt))
            (/ (- (cdr x-next) (cdr x-prev)) (* 2 dt)))))

  (unwind-protect
      (let ((x-prev '(0 . 100))      ;; initial position at t=-dt
            (x-curr '(5 . 100))       ;; position at t=0 (implies initial vel ~(5,0))
            (acc '(0 . -10))           ;; gravity
            (dt 1)
            (positions nil))
        ;; Run 8 Verlet steps
        (dotimes (i 8)
          (setq positions (cons x-curr positions))
          (let ((x-next (funcall 'neovm--verlet-step x-curr x-prev acc dt)))
            (setq x-prev x-curr)
            (setq x-curr x-next)))
        (setq positions (cons x-curr positions))
        (let ((traj (nreverse positions)))
          (list
           ;; Full trajectory
           traj
           ;; X should increase linearly (no x-acceleration)
           (let ((xs (mapcar 'car traj)))
             (list (car xs) (nth 4 xs) (nth 8 xs)))
           ;; Y should form a parabola (constant downward acc)
           (let ((ys (mapcar 'cdr traj)))
             (list (car ys) (nth 4 ys) (nth 8 ys)))
           ;; Velocity estimate at middle
           (funcall 'neovm--verlet-velocity
                    (nth 5 traj) (nth 3 traj) dt))))
    (fmakunbound 'neovm--verlet-step)
    (fmakunbound 'neovm--verlet-velocity)))"####;
    let expect = expect_test::expect![[
        r#""OK (((5 . 100) (10 . 90) (15 . 70) (20 . 40) (25 . 0) (30 . -50) (35 . -110) (40 . -180) (45 . -260)) (5 25 45) (100 0 -260) (5 . -45))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Collision detection: circle-circle and point-in-rect
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_collision_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Circle: (cx cy radius)
  ;; Rect: (x y width height)

  ;; Circle-circle: collide if dist^2 <= (r1+r2)^2
  (fset 'neovm--phys-circle-collide
    (lambda (c1 c2)
      (let* ((dx (- (nth 0 c1) (nth 0 c2)))
             (dy (- (nth 1 c1) (nth 1 c2)))
             (dist-sq (+ (* dx dx) (* dy dy)))
             (r-sum (+ (nth 2 c1) (nth 2 c2)))
             (r-sum-sq (* r-sum r-sum)))
        (<= dist-sq r-sum-sq))))

  ;; Point-in-rect: (px, py) inside (rx, ry, rw, rh)
  (fset 'neovm--phys-point-in-rect
    (lambda (px py rect)
      (let ((rx (nth 0 rect))
            (ry (nth 1 rect))
            (rw (nth 2 rect))
            (rh (nth 3 rect)))
        (and (>= px rx) (< px (+ rx rw))
             (>= py ry) (< py (+ ry rh))))))

  ;; Circle-rect collision (simplified: check center + radius against rect expanded by radius)
  (fset 'neovm--phys-circle-rect-collide
    (lambda (circle rect)
      (let* ((cx (nth 0 circle))
             (cy (nth 1 circle))
             (cr (nth 2 circle))
             (expanded (list (- (nth 0 rect) cr)
                             (- (nth 1 rect) cr)
                             (+ (nth 2 rect) (* 2 cr))
                             (+ (nth 3 rect) (* 2 cr)))))
        (funcall 'neovm--phys-point-in-rect cx cy expanded))))

  ;; Sweep test: check if moving circle hits stationary circle
  ;; Simple discrete check at N substeps
  (fset 'neovm--phys-sweep-collide
    (lambda (c1-start c1-end c2 steps)
      "Check if circle c1 moving from c1-start to c1-end collides with c2."
      (let ((hit nil)
            (i 0)
            (r1 (nth 2 c1-start)))
        (while (and (< i steps) (not hit))
          (let* ((t-frac (if (= steps 1) 0 (/ (* i 1000) (1- steps))))
                 (cx (+ (nth 0 c1-start) (/ (* (- (nth 0 c1-end) (nth 0 c1-start)) t-frac) 1000)))
                 (cy (+ (nth 1 c1-start) (/ (* (- (nth 1 c1-end) (nth 1 c1-start)) t-frac) 1000)))
                 (test-circle (list cx cy r1)))
            (when (funcall 'neovm--phys-circle-collide test-circle c2)
              (setq hit i)))
          (setq i (1+ i)))
        hit)))

  (unwind-protect
      (list
       ;; Circle-circle: overlapping
       (funcall 'neovm--phys-circle-collide '(0 0 5) '(3 4 5))  ;; dist=5, r-sum=10 -> t
       ;; Circle-circle: just touching
       (funcall 'neovm--phys-circle-collide '(0 0 5) '(10 0 5)) ;; dist=10, r-sum=10 -> t
       ;; Circle-circle: separated
       (funcall 'neovm--phys-circle-collide '(0 0 5) '(20 0 5)) ;; dist=20, r-sum=10 -> nil
       ;; Circle-circle: concentric
       (funcall 'neovm--phys-circle-collide '(5 5 10) '(5 5 3)) ;; dist=0 -> t
       ;; Point-in-rect
       (funcall 'neovm--phys-point-in-rect 5 5 '(0 0 10 10))    ;; inside
       (funcall 'neovm--phys-point-in-rect 15 5 '(0 0 10 10))   ;; outside right
       (funcall 'neovm--phys-point-in-rect 0 0 '(0 0 10 10))    ;; corner (inside)
       (funcall 'neovm--phys-point-in-rect 10 10 '(0 0 10 10))  ;; far corner (outside)
       ;; Circle-rect collision
       (funcall 'neovm--phys-circle-rect-collide '(5 5 3) '(0 0 10 10))  ;; inside
       (funcall 'neovm--phys-circle-rect-collide '(15 5 3) '(0 0 10 10)) ;; outside
       (funcall 'neovm--phys-circle-rect-collide '(12 5 3) '(0 0 10 10)) ;; touching edge
       ;; Sweep test
       (funcall 'neovm--phys-sweep-collide '(0 0 2) '(100 0 2) '(50 0 5) 20))
    (fmakunbound 'neovm--phys-circle-collide)
    (fmakunbound 'neovm--phys-point-in-rect)
    (fmakunbound 'neovm--phys-circle-rect-collide)
    (fmakunbound 'neovm--phys-sweep-collide)))"####;
    let expect = expect_test::expect![[r#""OK (t t nil t t nil t nil t nil t 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gravitational N-body simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_nbody_gravity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Body: (mass x y vx vy)
  (fset 'neovm--nbody-make (lambda (m x y vx vy) (list m x y vx vy)))
  (fset 'neovm--nbody-mass (lambda (b) (nth 0 b)))
  (fset 'neovm--nbody-x (lambda (b) (nth 1 b)))
  (fset 'neovm--nbody-y (lambda (b) (nth 2 b)))
  (fset 'neovm--nbody-vx (lambda (b) (nth 3 b)))
  (fset 'neovm--nbody-vy (lambda (b) (nth 4 b)))

  ;; Gravitational acceleration on body i from body j
  ;; a = G * mj * (rj - ri) / |rj - ri|^3
  ;; Using integer math: G=1, distances in units, avoid division by zero with softening
  (fset 'neovm--nbody-accel
    (lambda (bi bj)
      "Acceleration on bi due to bj. Returns (ax . ay)."
      (let* ((dx (- (funcall 'neovm--nbody-x bj) (funcall 'neovm--nbody-x bi)))
             (dy (- (funcall 'neovm--nbody-y bj) (funcall 'neovm--nbody-y bi)))
             (dist-sq (+ (* dx dx) (* dy dy) 1))  ;; +1 softening
             ;; Approximate: a = m * d / dist^3 ~ m * d / (dist-sq * sqrt(dist-sq))
             ;; Simplify: just use m * d / dist-sq for 2D
             (mj (funcall 'neovm--nbody-mass bj))
             (ax (/ (* mj dx) dist-sq))
             (ay (/ (* mj dy) dist-sq)))
        (cons ax ay))))

  ;; Total acceleration on body i from all other bodies
  (fset 'neovm--nbody-total-accel
    (lambda (body-idx bodies)
      (let ((ax 0) (ay 0)
            (bi (nth body-idx bodies))
            (j 0)
            (n (length bodies)))
        (while (< j n)
          (unless (= j body-idx)
            (let ((a (funcall 'neovm--nbody-accel bi (nth j bodies))))
              (setq ax (+ ax (car a)))
              (setq ay (+ ay (cdr a)))))
          (setq j (1+ j)))
        (cons ax ay))))

  ;; Step all bodies forward by dt using Euler integration
  (fset 'neovm--nbody-step
    (lambda (bodies dt)
      (let ((n (length bodies))
            (new-bodies nil)
            (i 0))
        (while (< i n)
          (let* ((b (nth i bodies))
                 (acc (funcall 'neovm--nbody-total-accel i bodies))
                 (nvx (+ (funcall 'neovm--nbody-vx b) (* (car acc) dt)))
                 (nvy (+ (funcall 'neovm--nbody-vy b) (* (cdr acc) dt)))
                 (nx (+ (funcall 'neovm--nbody-x b) (* nvx dt)))
                 (ny (+ (funcall 'neovm--nbody-y b) (* nvy dt))))
            (setq new-bodies
                  (cons (funcall 'neovm--nbody-make
                          (funcall 'neovm--nbody-mass b) nx ny nvx nvy)
                        new-bodies)))
          (setq i (1+ i)))
        (nreverse new-bodies))))

  ;; Compute total kinetic energy: sum of 0.5 * m * v^2
  (fset 'neovm--nbody-kinetic-energy
    (lambda (bodies)
      (let ((total 0))
        (dolist (b bodies)
          (let ((vx (funcall 'neovm--nbody-vx b))
                (vy (funcall 'neovm--nbody-vy b))
                (m (funcall 'neovm--nbody-mass b)))
            (setq total (+ total (/ (* m (+ (* vx vx) (* vy vy))) 2)))))
        total)))

  (unwind-protect
      (let ((bodies (list
                     (funcall 'neovm--nbody-make 1000 0 0 0 0)     ;; heavy center
                     (funcall 'neovm--nbody-make 1 100 0 0 3)      ;; orbiter 1
                     (funcall 'neovm--nbody-make 1 -100 0 0 -3)))) ;; orbiter 2
        ;; Run 5 steps
        (let ((snapshots (list (mapcar (lambda (b)
                                         (list (funcall 'neovm--nbody-x b)
                                               (funcall 'neovm--nbody-y b)))
                                       bodies))))
          (dotimes (step 5)
            (setq bodies (funcall 'neovm--nbody-step bodies 1))
            (setq snapshots
                  (cons (mapcar (lambda (b)
                                  (list (funcall 'neovm--nbody-x b)
                                        (funcall 'neovm--nbody-y b)))
                                bodies)
                        snapshots)))
          (let ((final-ke (funcall 'neovm--nbody-kinetic-energy bodies)))
            (list
             ;; Trajectory snapshots (reversed)
             (nreverse snapshots)
             ;; Center body should barely move (much heavier)
             (let ((center-pos (list (funcall 'neovm--nbody-x (car bodies))
                                     (funcall 'neovm--nbody-y (car bodies)))))
               center-pos)
             ;; Final kinetic energy
             final-ke
             ;; System has 3 bodies
             (length bodies)))))
    (fmakunbound 'neovm--nbody-make)
    (fmakunbound 'neovm--nbody-mass)
    (fmakunbound 'neovm--nbody-x)
    (fmakunbound 'neovm--nbody-y)
    (fmakunbound 'neovm--nbody-vx)
    (fmakunbound 'neovm--nbody-vy)
    (fmakunbound 'neovm--nbody-accel)
    (fmakunbound 'neovm--nbody-total-accel)
    (fmakunbound 'neovm--nbody-step)
    (fmakunbound 'neovm--nbody-kinetic-energy)))"####;
    let expect = expect_test::expect![[
        r#""OK ((((0 0) (100 0) (-100 0)) ((0 0) (91 3) (-91 -3)) ((0 0) (72 6) (-72 -6)) ((0 0) (40 8) (-40 -8)) ((0 0) (-16 6) (16 -6)) ((0 0) (-18 -16) (18 16))) (0 0) 488 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Spring-mass system (Hooke's law + damping)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_spring_mass() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; 1D spring-mass-damper: F = -k*x - c*v
  ;; mass, spring constant k, damping c
  ;; Using integer arithmetic scaled by 100

  (fset 'neovm--spring-step
    (lambda (x v mass k c dt)
      "Step spring-mass system. x,v in units*100. Returns (new-x . new-v)."
      (let* ((force (- (- (* k x)) (* c v)))
             (accel (/ (* force 100) mass))   ;; force/mass, scaled
             (new-v (+ v (/ (* accel dt) 100)))
             (new-x (+ x (/ (* new-v dt) 100))))
        (cons new-x new-v))))

  ;; Simulate spring-mass for N steps, return trajectory
  (fset 'neovm--spring-simulate
    (lambda (x0 v0 mass k c dt steps)
      (let ((x x0) (v v0) (traj nil))
        (dotimes (i steps)
          (setq traj (cons (cons x v) traj))
          (let ((result (funcall 'neovm--spring-step x v mass k c dt)))
            (setq x (car result))
            (setq v (cdr result))))
        (setq traj (cons (cons x v) traj))
        (nreverse traj))))

  ;; Compute total energy: 0.5*k*x^2 + 0.5*m*v^2 (scaled)
  (fset 'neovm--spring-energy
    (lambda (x v mass k)
      (+ (/ (* k x x) 200) (/ (* mass v v) 200))))

  (unwind-protect
      (list
       ;; Undamped oscillation: mass=100, k=1, c=0, x0=1000, v0=0
       (let ((traj (funcall 'neovm--spring-simulate 1000 0 100 1 0 10 20)))
         (list
          ;; Should oscillate: first position, middle, end
          (car (nth 0 traj))
          (car (nth 5 traj))
          (car (nth 10 traj))
          (car (nth 15 traj))
          ;; Velocity at start should be 0
          (cdr (nth 0 traj))))
       ;; Damped oscillation: c=1
       (let ((traj (funcall 'neovm--spring-simulate 1000 0 100 1 1 10 20)))
         ;; Amplitude should decrease over time
         (let ((amp-early (abs (car (nth 3 traj))))
               (amp-late (abs (car (nth 18 traj)))))
           (list amp-early amp-late (>= amp-early amp-late))))
       ;; Energy conservation in undamped system
       (let ((traj (funcall 'neovm--spring-simulate 1000 0 100 1 0 10 10)))
         (let ((e0 (funcall 'neovm--spring-energy
                     (car (nth 0 traj)) (cdr (nth 0 traj)) 100 1))
               (e5 (funcall 'neovm--spring-energy
                     (car (nth 5 traj)) (cdr (nth 5 traj)) 100 1)))
           (list e0 e5)))
       ;; Stiff spring: k=10
       (let ((traj (funcall 'neovm--spring-simulate 500 0 100 10 0 5 15)))
         ;; Higher frequency oscillation
         (mapcar 'car traj)))
    (fmakunbound 'neovm--spring-step)
    (fmakunbound 'neovm--spring-simulate)
    (fmakunbound 'neovm--spring-energy)))"####;
    let expect = expect_test::expect![[
        r#""OK ((1000 855 502 27 0) (946 208 t) (5000 118855) (500 488 464 428 381 325 261 190 115 37 -42 -120 -195 -265 -329 -384))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Projectile motion with quadratic drag
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_projectile_with_drag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Projectile motion with air drag: F_drag = -drag_coeff * v * |v|
  ;; Using integer arithmetic: positions in units*10, velocities in units*10

  (fset 'neovm--proj-step
    (lambda (x y vx vy gravity drag dt)
      "Step projectile. Returns (x y vx vy)."
      (let* (;; Speed squared (avoiding sqrt, use v*|v| approximation)
             (speed-sq (+ (* vx vx) (* vy vy)))
             ;; Drag force components: -drag * v_component * sqrt(speed_sq)
             ;; Approximate: just use -drag * v_component (linear drag for simplicity)
             (drag-fx (- (/ (* drag vx) 100)))
             (drag-fy (- (/ (* drag vy) 100)))
             ;; Acceleration
             (ax drag-fx)
             (ay (+ (- gravity) drag-fy))
             ;; Update velocity
             (new-vx (+ vx (* ax dt)))
             (new-vy (+ vy (* ay dt)))
             ;; Update position
             (new-x (+ x (* new-vx dt)))
             (new-y (+ y (* new-vy dt))))
        (list new-x new-y new-vx new-vy))))

  ;; Simulate until projectile hits ground (y <= 0) or max steps
  (fset 'neovm--proj-simulate
    (lambda (x0 y0 vx0 vy0 gravity drag dt max-steps)
      (let ((x x0) (y y0) (vx vx0) (vy vy0)
            (traj nil) (step 0) (landed nil))
        (while (and (< step max-steps) (not landed))
          (setq traj (cons (list x y vx vy) traj))
          (let ((result (funcall 'neovm--proj-step x y vx vy gravity drag dt)))
            (setq x (nth 0 result))
            (setq y (nth 1 result))
            (setq vx (nth 2 result))
            (setq vy (nth 3 result)))
          (when (and (> step 0) (<= y 0))
            (setq landed t))
          (setq step (1+ step)))
        (setq traj (cons (list x y vx vy) traj))
        (nreverse traj))))

  ;; Compute range (max x distance when y hits 0)
  (fset 'neovm--proj-range
    (lambda (traj)
      (let ((max-x 0))
        (dolist (pt traj)
          (when (> (nth 0 pt) max-x)
            (setq max-x (nth 0 pt))))
        max-x)))

  ;; Max height
  (fset 'neovm--proj-max-height
    (lambda (traj)
      (let ((max-y 0))
        (dolist (pt traj)
          (when (> (nth 1 pt) max-y)
            (setq max-y (nth 1 pt))))
        max-y)))

  (unwind-protect
      (list
       ;; No drag: 45-degree launch (vx=vy=100), gravity=10
       (let ((traj (funcall 'neovm--proj-simulate 0 0 100 100 10 0 1 50)))
         (list (length traj)
               (funcall 'neovm--proj-range traj)
               (funcall 'neovm--proj-max-height traj)))
       ;; With drag: same launch
       (let ((traj (funcall 'neovm--proj-simulate 0 0 100 100 10 5 1 50)))
         (list (length traj)
               (funcall 'neovm--proj-range traj)
               (funcall 'neovm--proj-max-height traj)))
       ;; Drag reduces range
       (let ((range-no-drag (funcall 'neovm--proj-range
                              (funcall 'neovm--proj-simulate 0 0 100 100 10 0 1 50)))
             (range-drag (funcall 'neovm--proj-range
                           (funcall 'neovm--proj-simulate 0 0 100 100 10 10 1 50))))
         (> range-no-drag range-drag))
       ;; Vertical launch: vx=0, vy=200
       (let ((traj (funcall 'neovm--proj-simulate 0 0 0 200 10 0 1 50)))
         (list (funcall 'neovm--proj-range traj)  ;; should be 0
               (funcall 'neovm--proj-max-height traj)))
       ;; Heavy drag stops quickly
       (let ((traj (funcall 'neovm--proj-simulate 0 0 100 100 10 50 1 50)))
         (length traj)))
    (fmakunbound 'neovm--proj-step)
    (fmakunbound 'neovm--proj-simulate)
    (fmakunbound 'neovm--proj-range)
    (fmakunbound 'neovm--proj-max-height)))"####;
    let expect = expect_test::expect![[r#""OK ((20 1900 450) (18 1164 331) t (0 1900) 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Elastic and inelastic collision response (1D)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_physics_collision_response() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; 1D elastic collision: conservation of momentum + kinetic energy
  ;; v1' = ((m1-m2)*v1 + 2*m2*v2) / (m1+m2)
  ;; v2' = ((m2-m1)*v2 + 2*m1*v1) / (m1+m2)
  (fset 'neovm--coll-elastic
    (lambda (m1 v1 m2 v2)
      "Returns (v1-new . v2-new) for 1D elastic collision."
      (let ((total-m (+ m1 m2)))
        (cons (/ (+ (* (- m1 m2) v1) (* 2 m2 v2)) total-m)
              (/ (+ (* (- m2 m1) v2) (* 2 m1 v1)) total-m)))))

  ;; Perfectly inelastic collision: objects stick together
  ;; v' = (m1*v1 + m2*v2) / (m1+m2)
  (fset 'neovm--coll-inelastic
    (lambda (m1 v1 m2 v2)
      "Returns combined velocity for perfectly inelastic collision."
      (/ (+ (* m1 v1) (* m2 v2)) (+ m1 m2))))

  ;; Partially inelastic: coefficient of restitution e (0 <= e <= 1)
  ;; v1' = ((m1 - e*m2)*v1 + (1+e)*m2*v2) / (m1+m2)
  ;; v2' = ((m2 - e*m1)*v2 + (1+e)*m1*v1) / (m1+m2)
  ;; Using e*100 to keep integer math (e100=100 means e=1.0, fully elastic)
  (fset 'neovm--coll-partial
    (lambda (m1 v1 m2 v2 e100)
      "Returns (v1-new . v2-new) for collision with restitution e100/100."
      (let ((total-m (+ m1 m2)))
        (cons (/ (+ (* (- (* m1 100) (* e100 m2)) v1)
                    (* (+ 100 e100) m2 v2))
                 (* total-m 100))
              (/ (+ (* (- (* m2 100) (* e100 m1)) v2)
                    (* (+ 100 e100) m1 v1))
                 (* total-m 100))))))

  ;; Momentum calculator
  (fset 'neovm--coll-momentum
    (lambda (m v) (* m v)))

  (unwind-protect
      (list
       ;; Equal mass elastic: velocities swap
       (funcall 'neovm--coll-elastic 10 50 10 -30)
       ;; Heavy hits light: heavy barely changes, light speeds up
       (funcall 'neovm--coll-elastic 100 10 1 0)
       ;; Light hits heavy: light bounces back
       (funcall 'neovm--coll-elastic 1 100 100 0)
       ;; Head-on equal mass: velocities swap
       (funcall 'neovm--coll-elastic 5 100 5 -100)
       ;; Inelastic: equal mass head-on -> stop
       (funcall 'neovm--coll-inelastic 10 50 10 -50)
       ;; Inelastic: unequal mass
       (funcall 'neovm--coll-inelastic 30 20 10 -40)
       ;; Momentum conservation in elastic collision
       (let* ((m1 15) (v1 40) (m2 25) (v2 -20)
              (result (funcall 'neovm--coll-elastic m1 v1 m2 v2))
              (p-before (+ (funcall 'neovm--coll-momentum m1 v1)
                           (funcall 'neovm--coll-momentum m2 v2)))
              (p-after (+ (funcall 'neovm--coll-momentum m1 (car result))
                          (funcall 'neovm--coll-momentum m2 (cdr result)))))
         (list p-before p-after (= p-before p-after)))
       ;; Partial inelastic with e=50% (e100=50)
       (funcall 'neovm--coll-partial 10 100 10 0 50)
       ;; Partial with e=100 should match elastic
       (let ((elastic (funcall 'neovm--coll-elastic 10 100 10 0))
             (partial (funcall 'neovm--coll-partial 10 100 10 0 100)))
         (list (= (car elastic) (car partial))
               (= (cdr elastic) (cdr partial))))
       ;; Partial with e=0 should match inelastic
       (let ((inelastic (funcall 'neovm--coll-inelastic 10 100 10 0))
             (partial (funcall 'neovm--coll-partial 10 100 10 0 0)))
         (list (= inelastic (car partial))
               (= inelastic (cdr partial)))))
    (fmakunbound 'neovm--coll-elastic)
    (fmakunbound 'neovm--coll-inelastic)
    (fmakunbound 'neovm--coll-partial)
    (fmakunbound 'neovm--coll-momentum)))"####;
    let expect = expect_test::expect![[
        r#""OK ((-30 . 50) (9 . 19) (-98 . 1) (-100 . 100) 0 5 (100 100 t) (25 . 75) (t t) (t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
