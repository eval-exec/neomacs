//! Oracle parity tests for a ray tracer simulation in Elisp:
//! 3D vector operations (add, sub, scale, dot, cross, normalize, length),
//! ray-sphere intersection, ray-plane intersection, basic Lambertian shading,
//! shadow ray casting, scene with multiple objects, color mixing/clamping.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 3D vector primitive operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_vec3_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; vec3 is (x y z) list
  (fset 'neovm--rt-vec3 (lambda (x y z) (list x y z)))
  (fset 'neovm--rt-vx (lambda (v) (car v)))
  (fset 'neovm--rt-vy (lambda (v) (cadr v)))
  (fset 'neovm--rt-vz (lambda (v) (caddr v)))

  (fset 'neovm--rt-vadd
    (lambda (a b)
      (list (+ (car a) (car b))
            (+ (cadr a) (cadr b))
            (+ (caddr a) (caddr b)))))

  (fset 'neovm--rt-vsub
    (lambda (a b)
      (list (- (car a) (car b))
            (- (cadr a) (cadr b))
            (- (caddr a) (caddr b)))))

  (fset 'neovm--rt-vscale
    (lambda (v s)
      (list (* (car v) s) (* (cadr v) s) (* (caddr v) s))))

  (fset 'neovm--rt-vdot
    (lambda (a b)
      (+ (* (car a) (car b))
         (* (cadr a) (cadr b))
         (* (caddr a) (caddr b)))))

  (fset 'neovm--rt-vcross
    (lambda (a b)
      (list (- (* (cadr a) (caddr b)) (* (caddr a) (cadr b)))
            (- (* (caddr a) (car b)) (* (car a) (caddr b)))
            (- (* (car a) (cadr b)) (* (cadr a) (car b))))))

  (fset 'neovm--rt-vlength
    (lambda (v) (sqrt (funcall 'neovm--rt-vdot v v))))

  (fset 'neovm--rt-vnormalize
    (lambda (v)
      (let ((len (funcall 'neovm--rt-vlength v)))
        (if (> len 0.0)
            (funcall 'neovm--rt-vscale v (/ 1.0 len))
          (list 0.0 0.0 0.0)))))

  (unwind-protect
      (let ((a (funcall 'neovm--rt-vec3 1.0 2.0 3.0))
            (b (funcall 'neovm--rt-vec3 4.0 -1.0 2.0)))
        (list
          ;; Add
          (funcall 'neovm--rt-vadd a b)
          ;; Subtract
          (funcall 'neovm--rt-vsub a b)
          ;; Scale
          (funcall 'neovm--rt-vscale a 2.5)
          ;; Dot product
          (funcall 'neovm--rt-vdot a b)
          ;; Cross product: a x b
          (funcall 'neovm--rt-vcross a b)
          ;; Cross product anti-commutativity: b x a = -(a x b)
          (funcall 'neovm--rt-vcross b a)
          ;; Length
          (funcall 'neovm--rt-vlength a)
          ;; Normalize
          (let ((n (funcall 'neovm--rt-vnormalize a)))
            ;; length of normalized should be ~1.0
            (list n (< (abs (- (funcall 'neovm--rt-vlength n) 1.0)) 0.0001)))
          ;; Normalize zero vector
          (funcall 'neovm--rt-vnormalize (list 0.0 0.0 0.0))
          ;; Dot product of orthogonal vectors
          (funcall 'neovm--rt-vdot (list 1.0 0.0 0.0) (list 0.0 1.0 0.0))))
    (fmakunbound 'neovm--rt-vec3)
    (fmakunbound 'neovm--rt-vx)
    (fmakunbound 'neovm--rt-vy)
    (fmakunbound 'neovm--rt-vz)
    (fmakunbound 'neovm--rt-vadd)
    (fmakunbound 'neovm--rt-vsub)
    (fmakunbound 'neovm--rt-vscale)
    (fmakunbound 'neovm--rt-vdot)
    (fmakunbound 'neovm--rt-vcross)
    (fmakunbound 'neovm--rt-vlength)
    (fmakunbound 'neovm--rt-vnormalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5.0 1.0 5.0) (-3.0 3.0 1.0) (2.5 5.0 7.5) 8.0 (7.0 10.0 -9.0) (-7.0 -10.0 9.0) 3.7416573867739413 ((0.2672612419124244 0.5345224838248488 0.8017837257372732) t) (0.0 0.0 0.0) 0.0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ray-sphere intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_sphere_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt2-vadd (lambda (a b) (list (+ (car a) (car b)) (+ (cadr a) (cadr b)) (+ (caddr a) (caddr b)))))
  (fset 'neovm--rt2-vsub (lambda (a b) (list (- (car a) (car b)) (- (cadr a) (cadr b)) (- (caddr a) (caddr b)))))
  (fset 'neovm--rt2-vscale (lambda (v s) (list (* (car v) s) (* (cadr v) s) (* (caddr v) s))))
  (fset 'neovm--rt2-vdot (lambda (a b) (+ (* (car a) (car b)) (* (cadr a) (cadr b)) (* (caddr a) (caddr b)))))

  ;; Ray-sphere intersection
  ;; Ray: origin + t*direction
  ;; Sphere: center, radius
  ;; Returns t-value of nearest hit, or nil if no hit
  (fset 'neovm--rt2-intersect-sphere
    (lambda (ray-origin ray-dir sphere-center sphere-radius)
      (let* ((oc (funcall 'neovm--rt2-vsub ray-origin sphere-center))
             (a (funcall 'neovm--rt2-vdot ray-dir ray-dir))
             (b (* 2.0 (funcall 'neovm--rt2-vdot oc ray-dir)))
             (c (- (funcall 'neovm--rt2-vdot oc oc) (* sphere-radius sphere-radius)))
             (discriminant (- (* b b) (* 4.0 a c))))
        (when (>= discriminant 0.0)
          (let ((t1 (/ (- (- b) (sqrt discriminant)) (* 2.0 a)))
                (t2 (/ (+ (- b) (sqrt discriminant)) (* 2.0 a))))
            ;; Return nearest positive t
            (cond
              ((> t1 0.001) t1)
              ((> t2 0.001) t2)
              (t nil)))))))

  (unwind-protect
      (let ((origin (list 0.0 0.0 0.0))
            (dir-z (list 0.0 0.0 -1.0))
            (dir-miss (list 1.0 0.0 0.0))
            (sphere-center (list 0.0 0.0 -5.0))
            (sphere-radius 1.0))
        (list
          ;; Direct hit: ray along -z, sphere at z=-5
          (let ((t-val (funcall 'neovm--rt2-intersect-sphere
                          origin dir-z sphere-center sphere-radius)))
            (list (and t-val t) (when t-val (< (abs (- t-val 4.0)) 0.01))))
          ;; Miss: ray along +x
          (funcall 'neovm--rt2-intersect-sphere origin dir-miss sphere-center sphere-radius)
          ;; Grazing hit: ray just touches sphere edge
          (let ((dir-graze (list 0.0 1.0 -5.0)))
            (and (funcall 'neovm--rt2-intersect-sphere
                   origin dir-graze (list 0.0 0.0 -5.0) 1.0) t))
          ;; Ray origin inside sphere: should find exit point
          (let ((t-val (funcall 'neovm--rt2-intersect-sphere
                          (list 0.0 0.0 -5.0) dir-z (list 0.0 0.0 -5.0) 2.0)))
            (and t-val (> t-val 0.0)))
          ;; Large sphere
          (let ((t-val (funcall 'neovm--rt2-intersect-sphere
                          origin dir-z (list 0.0 0.0 -10.0) 5.0)))
            (and t-val (< (abs (- t-val 5.0)) 0.01)))))
    (fmakunbound 'neovm--rt2-vadd)
    (fmakunbound 'neovm--rt2-vsub)
    (fmakunbound 'neovm--rt2-vscale)
    (fmakunbound 'neovm--rt2-vdot)
    (fmakunbound 'neovm--rt2-intersect-sphere)))"#;
    let expect = expect_test::expect![[r#""OK ((t t) nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ray-plane intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_plane_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt3-vsub (lambda (a b) (list (- (car a) (car b)) (- (cadr a) (cadr b)) (- (caddr a) (caddr b)))))
  (fset 'neovm--rt3-vdot (lambda (a b) (+ (* (car a) (car b)) (* (cadr a) (cadr b)) (* (caddr a) (caddr b)))))
  (fset 'neovm--rt3-vscale (lambda (v s) (list (* (car v) s) (* (cadr v) s) (* (caddr v) s))))
  (fset 'neovm--rt3-vadd (lambda (a b) (list (+ (car a) (car b)) (+ (cadr a) (cadr b)) (+ (caddr a) (caddr b)))))

  ;; Ray-plane intersection
  ;; Plane: point + normal
  ;; Returns t-value or nil
  (fset 'neovm--rt3-intersect-plane
    (lambda (ray-origin ray-dir plane-point plane-normal)
      (let ((denom (funcall 'neovm--rt3-vdot plane-normal ray-dir)))
        (when (> (abs denom) 0.0001)
          (let* ((diff (funcall 'neovm--rt3-vsub plane-point ray-origin))
                 (t-val (/ (funcall 'neovm--rt3-vdot diff plane-normal) denom)))
            (when (> t-val 0.001) t-val))))))

  ;; Hit point on ray
  (fset 'neovm--rt3-ray-at
    (lambda (origin dir t-val)
      (funcall 'neovm--rt3-vadd origin (funcall 'neovm--rt3-vscale dir t-val))))

  (unwind-protect
      (let ((origin (list 0.0 2.0 0.0))
            (dir-down (list 0.0 -1.0 0.0))
            (floor-pt (list 0.0 0.0 0.0))
            (floor-norm (list 0.0 1.0 0.0)))
        (list
          ;; Ray pointing down hits floor plane at y=0
          (let ((t-val (funcall 'neovm--rt3-intersect-plane
                          origin dir-down floor-pt floor-norm)))
            (list t-val
                  (when t-val
                    (funcall 'neovm--rt3-ray-at origin dir-down t-val))))
          ;; Ray parallel to plane: no hit
          (funcall 'neovm--rt3-intersect-plane
            origin (list 1.0 0.0 0.0) floor-pt floor-norm)
          ;; Ray pointing away from plane: no hit
          (funcall 'neovm--rt3-intersect-plane
            origin (list 0.0 1.0 0.0) floor-pt floor-norm)
          ;; Diagonal ray hitting a tilted plane
          (let ((t-val (funcall 'neovm--rt3-intersect-plane
                          (list 0.0 0.0 0.0)
                          (list 1.0 1.0 0.0)
                          (list 5.0 5.0 0.0)
                          (list -1.0 1.0 0.0))))
            ;; The plane x-y=0 tilted at 45 degrees
            (and t-val (> t-val 0.0)))))
    (fmakunbound 'neovm--rt3-vsub)
    (fmakunbound 'neovm--rt3-vdot)
    (fmakunbound 'neovm--rt3-vscale)
    (fmakunbound 'neovm--rt3-vadd)
    (fmakunbound 'neovm--rt3-intersect-plane)
    (fmakunbound 'neovm--rt3-ray-at)))"#;
    let expect = expect_test::expect![[r#""OK ((2.0 (0.0 0.0 0.0)) nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambertian (diffuse) shading
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_lambertian_shading() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt4-vdot (lambda (a b) (+ (* (car a) (car b)) (* (cadr a) (cadr b)) (* (caddr a) (caddr b)))))
  (fset 'neovm--rt4-vsub (lambda (a b) (list (- (car a) (car b)) (- (cadr a) (cadr b)) (- (caddr a) (caddr b)))))
  (fset 'neovm--rt4-vscale (lambda (v s) (list (* (car v) s) (* (cadr v) s) (* (caddr v) s))))
  (fset 'neovm--rt4-vlength (lambda (v) (sqrt (funcall 'neovm--rt4-vdot v v))))
  (fset 'neovm--rt4-vnormalize
    (lambda (v)
      (let ((len (funcall 'neovm--rt4-vlength v)))
        (if (> len 0.0) (funcall 'neovm--rt4-vscale v (/ 1.0 len)) (list 0.0 0.0 0.0)))))

  ;; Lambertian shading: intensity = max(0, dot(normal, light_dir)) * light_intensity
  (fset 'neovm--rt4-lambertian
    (lambda (surface-normal light-dir light-intensity)
      (let* ((n (funcall 'neovm--rt4-vnormalize surface-normal))
             (l (funcall 'neovm--rt4-vnormalize light-dir))
             (cos-angle (funcall 'neovm--rt4-vdot n l))
             (factor (max 0.0 cos-angle)))
        (* factor light-intensity))))

  ;; Color scaling: multiply RGB color by intensity, clamp to [0,1]
  (fset 'neovm--rt4-color-scale
    (lambda (color intensity)
      (list (min 1.0 (max 0.0 (* (car color) intensity)))
            (min 1.0 (max 0.0 (* (cadr color) intensity)))
            (min 1.0 (max 0.0 (* (caddr color) intensity))))))

  (unwind-protect
      (list
        ;; Surface facing directly toward light: full intensity
        (funcall 'neovm--rt4-lambertian
          (list 0.0 1.0 0.0) (list 0.0 1.0 0.0) 1.0)
        ;; Surface at 45 degrees to light
        (let ((val (funcall 'neovm--rt4-lambertian
                     (list 0.0 1.0 0.0) (list 1.0 1.0 0.0) 1.0)))
          (< (abs (- val 0.7071)) 0.01))
        ;; Surface facing away: zero intensity
        (funcall 'neovm--rt4-lambertian
          (list 0.0 1.0 0.0) (list 0.0 -1.0 0.0) 1.0)
        ;; Color scaling
        (funcall 'neovm--rt4-color-scale (list 0.8 0.2 0.5) 0.5)
        ;; Color clamping: high intensity
        (funcall 'neovm--rt4-color-scale (list 0.8 0.9 0.7) 1.5)
        ;; Color at zero intensity
        (funcall 'neovm--rt4-color-scale (list 0.8 0.9 0.7) 0.0))
    (fmakunbound 'neovm--rt4-vdot)
    (fmakunbound 'neovm--rt4-vsub)
    (fmakunbound 'neovm--rt4-vscale)
    (fmakunbound 'neovm--rt4-vlength)
    (fmakunbound 'neovm--rt4-vnormalize)
    (fmakunbound 'neovm--rt4-lambertian)
    (fmakunbound 'neovm--rt4-color-scale)))"#;
    let expect =
        expect_test::expect![[r#""OK (1.0 t 0.0 (0.4 0.1 0.25) (1.0 1.0 1.0) (0.0 0.0 0.0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shadow ray casting: determine if a point is in shadow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_shadow_rays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt5-vsub (lambda (a b) (list (- (car a) (car b)) (- (cadr a) (cadr b)) (- (caddr a) (caddr b)))))
  (fset 'neovm--rt5-vdot (lambda (a b) (+ (* (car a) (car b)) (* (cadr a) (cadr b)) (* (caddr a) (caddr b)))))
  (fset 'neovm--rt5-vscale (lambda (v s) (list (* (car v) s) (* (cadr v) s) (* (caddr v) s))))
  (fset 'neovm--rt5-vadd (lambda (a b) (list (+ (car a) (car b)) (+ (cadr a) (cadr b)) (+ (caddr a) (caddr b)))))
  (fset 'neovm--rt5-vlength (lambda (v) (sqrt (funcall 'neovm--rt5-vdot v v))))
  (fset 'neovm--rt5-vnormalize
    (lambda (v) (let ((l (funcall 'neovm--rt5-vlength v))) (if (> l 0.0) (funcall 'neovm--rt5-vscale v (/ 1.0 l)) v))))

  (fset 'neovm--rt5-intersect-sphere
    (lambda (ro rd sc sr)
      (let* ((oc (funcall 'neovm--rt5-vsub ro sc))
             (a (funcall 'neovm--rt5-vdot rd rd))
             (b (* 2.0 (funcall 'neovm--rt5-vdot oc rd)))
             (c (- (funcall 'neovm--rt5-vdot oc oc) (* sr sr)))
             (disc (- (* b b) (* 4.0 a c))))
        (when (>= disc 0.0)
          (let ((t1 (/ (- (- b) (sqrt disc)) (* 2.0 a))))
            (when (> t1 0.001) t1))))))

  ;; Check if point is in shadow from light, given a list of spheres
  (fset 'neovm--rt5-in-shadow
    (lambda (hit-point light-pos spheres)
      (let* ((to-light (funcall 'neovm--rt5-vsub light-pos hit-point))
             (light-dist (funcall 'neovm--rt5-vlength to-light))
             (shadow-dir (funcall 'neovm--rt5-vnormalize to-light))
             ;; Offset origin slightly to avoid self-intersection
             (shadow-origin (funcall 'neovm--rt5-vadd hit-point
                              (funcall 'neovm--rt5-vscale shadow-dir 0.01)))
             (blocked nil))
        (dolist (sphere spheres)
          (let ((t-val (funcall 'neovm--rt5-intersect-sphere
                          shadow-origin shadow-dir (car sphere) (cadr sphere))))
            (when (and t-val (< t-val light-dist))
              (setq blocked t))))
        blocked)))

  (unwind-protect
      (let ((light (list 10.0 10.0 0.0))
            ;; Two spheres: one at z=-5, one at z=-10
            (spheres (list (list (list 0.0 0.0 -5.0) 1.0)
                          (list (list 3.0 0.0 -8.0) 1.5))))
        (list
          ;; Point on top of sphere 1, facing light: not in shadow
          (funcall 'neovm--rt5-in-shadow (list 0.0 1.0 -5.0) light spheres)
          ;; Point behind sphere 1 from light's perspective: in shadow
          (funcall 'neovm--rt5-in-shadow (list 0.0 -1.5 -5.0) light spheres)
          ;; Point far from any sphere: not in shadow
          (funcall 'neovm--rt5-in-shadow (list 20.0 0.0 0.0) light spheres)
          ;; Point between two spheres: check occlusion
          (funcall 'neovm--rt5-in-shadow (list 1.5 0.0 -6.5) light spheres)))
    (fmakunbound 'neovm--rt5-vsub)
    (fmakunbound 'neovm--rt5-vdot)
    (fmakunbound 'neovm--rt5-vscale)
    (fmakunbound 'neovm--rt5-vadd)
    (fmakunbound 'neovm--rt5-vlength)
    (fmakunbound 'neovm--rt5-vnormalize)
    (fmakunbound 'neovm--rt5-intersect-sphere)
    (fmakunbound 'neovm--rt5-in-shadow)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-object scene: find nearest intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_scene_nearest_hit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt6-vsub (lambda (a b) (list (- (car a) (car b)) (- (cadr a) (cadr b)) (- (caddr a) (caddr b)))))
  (fset 'neovm--rt6-vdot (lambda (a b) (+ (* (car a) (car b)) (* (cadr a) (cadr b)) (* (caddr a) (caddr b)))))
  (fset 'neovm--rt6-vscale (lambda (v s) (list (* (car v) s) (* (cadr v) s) (* (caddr v) s))))
  (fset 'neovm--rt6-vadd (lambda (a b) (list (+ (car a) (car b)) (+ (cadr a) (cadr b)) (+ (caddr a) (caddr b)))))

  (fset 'neovm--rt6-intersect-sphere
    (lambda (ro rd sc sr)
      (let* ((oc (funcall 'neovm--rt6-vsub ro sc))
             (a (funcall 'neovm--rt6-vdot rd rd))
             (b (* 2.0 (funcall 'neovm--rt6-vdot oc rd)))
             (c (- (funcall 'neovm--rt6-vdot oc oc) (* sr sr)))
             (disc (- (* b b) (* 4.0 a c))))
        (when (>= disc 0.0)
          (let ((t1 (/ (- (- b) (sqrt disc)) (* 2.0 a))))
            (when (> t1 0.001) t1))))))

  ;; Scene: list of (center radius color-name)
  ;; Find nearest hit, return (t-value color-name) or nil
  (fset 'neovm--rt6-trace-scene
    (lambda (ray-origin ray-dir scene)
      (let ((nearest-t nil)
            (nearest-color nil))
        (dolist (obj scene)
          (let ((center (nth 0 obj))
                (radius (nth 1 obj))
                (color (nth 2 obj)))
            (let ((t-val (funcall 'neovm--rt6-intersect-sphere
                            ray-origin ray-dir center radius)))
              (when (and t-val (or (null nearest-t) (< t-val nearest-t)))
                (setq nearest-t t-val
                      nearest-color color)))))
        (when nearest-t
          (list nearest-t nearest-color)))))

  (unwind-protect
      (let ((scene (list
                     (list (list 0.0 0.0 -5.0) 1.0 'red)
                     (list (list 2.0 0.0 -7.0) 1.5 'green)
                     (list (list -2.0 1.0 -4.0) 0.5 'blue)
                     (list (list 0.0 -101.0 0.0) 100.0 'gray)))
            (eye (list 0.0 0.0 0.0)))
        (list
          ;; Ray straight ahead: hits red sphere
          (let ((hit (funcall 'neovm--rt6-trace-scene eye (list 0.0 0.0 -1.0) scene)))
            (cadr hit))
          ;; Ray toward green sphere
          (let ((hit (funcall 'neovm--rt6-trace-scene eye (list 0.3 0.0 -1.0) scene)))
            (cadr hit))
          ;; Ray toward blue sphere
          (let ((hit (funcall 'neovm--rt6-trace-scene eye (list -0.5 0.3 -1.0) scene)))
            (cadr hit))
          ;; Ray downward: hits gray floor sphere
          (let ((hit (funcall 'neovm--rt6-trace-scene eye (list 0.0 -1.0 -0.5) scene)))
            (cadr hit))
          ;; Ray upward: misses everything
          (funcall 'neovm--rt6-trace-scene eye (list 0.0 1.0 0.0) scene)))
    (fmakunbound 'neovm--rt6-vsub)
    (fmakunbound 'neovm--rt6-vdot)
    (fmakunbound 'neovm--rt6-vscale)
    (fmakunbound 'neovm--rt6-vadd)
    (fmakunbound 'neovm--rt6-intersect-sphere)
    (fmakunbound 'neovm--rt6-trace-scene)))"#;
    let expect = expect_test::expect![[r#""OK (red green blue gray nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Color mixing and clamping utilities
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_combination_ray_tracer_color_mixing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Color operations: colors are (r g b) with values in [0.0, 1.0]
  (fset 'neovm--rt7-color-add
    (lambda (a b)
      (list (+ (car a) (car b)) (+ (cadr a) (cadr b)) (+ (caddr a) (caddr b)))))

  (fset 'neovm--rt7-color-mul
    (lambda (a b)
      (list (* (car a) (car b)) (* (cadr a) (cadr b)) (* (caddr a) (caddr b)))))

  (fset 'neovm--rt7-color-scale
    (lambda (c s)
      (list (* (car c) s) (* (cadr c) s) (* (caddr c) s))))

  (fset 'neovm--rt7-color-clamp
    (lambda (c)
      (list (min 1.0 (max 0.0 (car c)))
            (min 1.0 (max 0.0 (cadr c)))
            (min 1.0 (max 0.0 (caddr c))))))

  (fset 'neovm--rt7-color-lerp
    (lambda (a b t-val)
      "Linear interpolation between colors A and B."
      (list (+ (* (car a) (- 1.0 t-val)) (* (car b) t-val))
            (+ (* (cadr a) (- 1.0 t-val)) (* (cadr b) t-val))
            (+ (* (caddr a) (- 1.0 t-val)) (* (caddr b) t-val)))))

  ;; Convert float color [0,1] to integer [0,255]
  (fset 'neovm--rt7-color-to-int
    (lambda (c)
      (let ((clamped (funcall 'neovm--rt7-color-clamp c)))
        (list (round (* (car clamped) 255))
              (round (* (cadr clamped) 255))
              (round (* (caddr clamped) 255))))))

  (unwind-protect
      (let ((red (list 1.0 0.0 0.0))
            (green (list 0.0 1.0 0.0))
            (blue (list 0.0 0.0 1.0))
            (white (list 1.0 1.0 1.0))
            (ambient (list 0.1 0.1 0.1)))
        (list
          ;; Add red + green = yellow (unclamped)
          (funcall 'neovm--rt7-color-add red green)
          ;; Multiply red * green = black
          (funcall 'neovm--rt7-color-mul red green)
          ;; Scale blue by 0.5
          (funcall 'neovm--rt7-color-scale blue 0.5)
          ;; Clamp overflowed color
          (funcall 'neovm--rt7-color-clamp (list 1.5 -0.2 0.8))
          ;; Lerp red to blue at t=0.0, 0.5, 1.0
          (funcall 'neovm--rt7-color-lerp red blue 0.0)
          (funcall 'neovm--rt7-color-lerp red blue 0.5)
          (funcall 'neovm--rt7-color-lerp red blue 1.0)
          ;; Composite: ambient + diffuse + specular
          (let* ((diffuse (funcall 'neovm--rt7-color-scale (list 0.8 0.2 0.2) 0.7))
                 (specular (funcall 'neovm--rt7-color-scale white 0.3))
                 (total (funcall 'neovm--rt7-color-add
                          ambient
                          (funcall 'neovm--rt7-color-add diffuse specular))))
            (funcall 'neovm--rt7-color-clamp total))
          ;; Convert to integer RGB
          (funcall 'neovm--rt7-color-to-int (list 0.5 0.75 1.0))
          (funcall 'neovm--rt7-color-to-int (list 1.5 -0.1 0.333))))
    (fmakunbound 'neovm--rt7-color-add)
    (fmakunbound 'neovm--rt7-color-mul)
    (fmakunbound 'neovm--rt7-color-scale)
    (fmakunbound 'neovm--rt7-color-clamp)
    (fmakunbound 'neovm--rt7-color-lerp)
    (fmakunbound 'neovm--rt7-color-to-int)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1.0 1.0 0.0) (0.0 0.0 0.0) (0.0 0.0 0.5) (1.0 0.0 0.8) (1.0 0.0 0.0) (0.5 0.0 0.5) (0.0 0.0 1.0) (0.9599999999999999 0.5399999999999999 0.5399999999999999) (128 191 255) (255 0 85))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
