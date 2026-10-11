//! The bignum paths of `+`, `-`, `*`, the comparisons and the two-argument
//! rounding functions read bignum operands by reference and multiply by a
//! fixnum through a local one-limb kernel. Every result here is held to a
//! reference computed independently with owned malachite arithmetic — for
//! the rounding family, the remainder-adjusting algorithm those builtins
//! used before (itself checked against GNU) — over signs, the fixnum
//! boundary, the i64/u64 boundaries and multi-limb magnitudes. Each result
//! must also be a fixnum exactly when it fits one (`eq` to itself when
//! computed twice).

use crate::emacs_core::{Context, format_eval_result};
use malachite::integer::Integer;
use std::str::FromStr;

fn pool() -> Vec<Integer> {
    let fixnum_max = Integer::from((1i64 << 61) - 1);
    let fixnum_min = Integer::from(-(1i64 << 61));
    let two = Integer::from(2);
    let pow = |base: i64, e: u32| -> Integer {
        let mut acc = Integer::from(1);
        for _ in 0..e {
            acc *= Integer::from(base);
        }
        acc
    };
    let mut v = vec![
        Integer::from(0),
        Integer::from(1),
        Integer::from(-1),
        Integer::from(3),
        Integer::from(-7),
        Integer::from(10),
        fixnum_max.clone(),
        fixnum_min.clone(),
        &fixnum_max + Integer::from(1),
        &fixnum_min - Integer::from(1),
        Integer::from(i64::MAX),
        Integer::from(i64::MIN),
        Integer::from(u64::MAX),
        -Integer::from(u64::MAX),
        pow(2, 64),
        -pow(2, 64),
        pow(2, 64) + Integer::from(1),
        pow(2, 128) - Integer::from(1),
        pow(3, 100),
        -pow(3, 100),
        pow(10, 40) + Integer::from(1),
        -(pow(10, 40)),
        pow(7, 200) + &two,
        Integer::from_str("340282366920938463463374607431768211455").unwrap(),
    ];
    // A multiple and a near-multiple of a big divisor, for exact and
    // half-way quotients.
    v.push(pow(3, 100) * Integer::from(5));
    v.push(pow(3, 100) * Integer::from(5) + pow(3, 100) / Integer::from(2));
    v
}

/// The rounding builtins' previous algorithm: truncate, then adjust from
/// the remainder.
fn reference_round(name: &str, a: &Integer, d: &Integer) -> Integer {
    let q = a / d;
    let r = a - &q * d;
    let one = Integer::from(1);
    match name {
        "truncate" => q,
        "floor" if r != 0 && (r < 0) != (*d < 0) => q - one,
        "ceiling" if r != 0 && (r < 0) == (*d < 0) => q + one,
        "round" => {
            let abs_r2 = (&r * Integer::from(2)).abs_ref();
            let abs_d = d.abs_ref();
            let away = |q: Integer| {
                if (r < 0) == (*d < 0) {
                    q + Integer::from(1)
                } else {
                    q - Integer::from(1)
                }
            };
            match abs_r2.cmp(&abs_d) {
                std::cmp::Ordering::Greater => away(q),
                std::cmp::Ordering::Equal if (&q % Integer::from(2)) != 0 => away(q),
                _ => q,
            }
        }
        _ => q,
    }
}

trait AbsRef {
    fn abs_ref(&self) -> Integer;
}
impl AbsRef for Integer {
    fn abs_ref(&self) -> Integer {
        if *self < 0 {
            -self.clone()
        } else {
            self.clone()
        }
    }
}

fn is_fixnum(x: &Integer) -> bool {
    *x >= -(1i64 << 61) && *x < (1i64 << 61)
}

/// `(let ((x FORM) (y FORM)) (list x (eq x y)))` printed, as the builtins
/// must answer it for the exact result `want`.
fn shape(want: &Integer) -> String {
    format!("({} {})", want, if is_fixnum(want) { "t" } else { "nil" })
}

fn check(ev: &mut Context, form: &str, want: &str) {
    let src = format!("(let ((x {form}) (y {form})) (list x (eq x y)))");
    let got = format_eval_result(&ev.eval_str(&src));
    assert_eq!(got, format!("OK {want}"), "{form}");
}

#[test]
fn bignum_add_sub_mul_match_owned_arithmetic() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let pool = pool();
    for a in &pool {
        for b in &pool {
            check(&mut ev, &format!("(* {a} {b})"), &shape(&(a * b)));
            check(&mut ev, &format!("(+ {a} {b})"), &shape(&(a + b)));
            check(&mut ev, &format!("(- {a} {b})"), &shape(&(a - b)));
            check(
                &mut ev,
                &format!("(* {a} {b} -3)"),
                &shape(&(a * b * Integer::from(-3))),
            );
            check(
                &mut ev,
                &format!("(* 5 {a} {b})"),
                &shape(&(Integer::from(5) * a * b)),
            );
            check(&mut ev, &format!("(+ {a} {b} {a})"), &shape(&(a + b + a)));
            check(
                &mut ev,
                &format!("(+ 9 {a} {b})"),
                &shape(&(Integer::from(9) + a + b)),
            );
            check(&mut ev, &format!("(- {a} {b} {a})"), &shape(&(a - b - a)));
            check(
                &mut ev,
                &format!("(- {a} 4 {b})"),
                &shape(&(a - Integer::from(4) - b)),
            );
        }
    }
}

#[test]
fn bignum_comparisons_match_owned_arithmetic() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let pool = pool();
    for a in &pool {
        for b in &pool {
            let src = format!(
                "(list (< {a} {b}) (<= {a} {b}) (= {a} {b}) (/= {a} {b}) (> {a} {b}) (>= {a} {b}))"
            );
            let t = |x: bool| if x { "t" } else { "nil" };
            let want = format!(
                "OK ({} {} {} {} {} {})",
                t(a < b),
                t(a <= b),
                t(a == b),
                t(a != b),
                t(a > b),
                t(a >= b)
            );
            assert_eq!(format_eval_result(&ev.eval_str(&src)), want, "{src}");
        }
        // Against floats: exact, never through a double of the integer.
        for f in [
            "0.0", "-1.5", "1e19", "-1e19", "1e40", "1.0e+INF", "0.0e+NaN",
        ] {
            let src = format!("(list (< {a} {f}) (= {a} {f}) (> {f} {a}))");
            let fv: f64 = match f {
                "1.0e+INF" => f64::INFINITY,
                "0.0e+NaN" => f64::NAN,
                _ => f.parse().unwrap(),
            };
            let ord = a.partial_cmp(&fv);
            let t = |x: bool| if x { "t" } else { "nil" };
            let want = format!(
                "OK ({} {} {})",
                t(ord == Some(std::cmp::Ordering::Less)),
                t(ord == Some(std::cmp::Ordering::Equal)),
                t(ord == Some(std::cmp::Ordering::Less)),
            );
            assert_eq!(format_eval_result(&ev.eval_str(&src)), want, "{src}");
        }
    }
}

#[test]
fn bignum_rounding_matches_the_remainder_algorithm() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let pool = pool();
    for a in &pool {
        for d in &pool {
            if *d == 0 {
                for name in ["truncate", "floor", "ceiling", "round"] {
                    let got = format_eval_result(&ev.eval_str(&format!("({name} {a} {d})")));
                    assert!(got.contains("arith-error"), "({name} {a} 0): {got}");
                }
                continue;
            }
            for name in ["truncate", "floor", "ceiling", "round"] {
                check(
                    &mut ev,
                    &format!("({name} {a} {d})"),
                    &shape(&reference_round(name, a, d)),
                );
            }
        }
    }
}
