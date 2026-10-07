//! The bignum limb kernels in `arithmetic.rs` against malachite: products
//! by one limb (the BMI2 path and the portable one), sums, differences, the
//! sign-aware integer wrappers, and the top-128-bit truncating quotient —
//! over every length pair up to 13 limbs plus 64 and 150, random limbs and
//! the carry-saturating patterns (all ones, all zeros, alternating).

use super::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

const LENS: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 64, 150];

fn patterns(rng: &mut Rng, len: usize) -> Vec<Vec<u64>> {
    let mut random: Vec<u64> = (0..len).map(|_| rng.next()).collect();
    if let Some(top) = random.last_mut() {
        *top |= 1; // keep the length significant
    }
    vec![
        random,
        vec![u64::MAX; len],
        (0..len)
            .map(|i| if i % 2 == 0 { u64::MAX } else { 0 })
            .collect(),
        {
            let mut v = vec![0u64; len];
            if let Some(top) = v.last_mut() {
                *top = 1;
            }
            v
        },
    ]
}

fn nat(xs: &[u64]) -> Natural {
    Natural::from_limbs_asc(xs)
}

#[test]
fn product_by_one_limb_matches_malachite() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    #[cfg(target_arch = "x86_64")]
    let bmi2 = std::arch::is_x86_feature_detected!("bmi2");
    for &len in LENS {
        for xs in patterns(&mut rng, len) {
            for m in [0, 1, 2, 10, 3407, 1 << 63, u64::MAX, rng.next()] {
                let want = nat(&xs) * Natural::from(m);
                let got = natural_from_kernel(xs.len(), |d| limbs_mul_limb_to(&xs, m, d));
                assert_eq!(got, want, "len {len} m {m}");
                let generic =
                    natural_from_kernel(xs.len(), |d| limbs_mul_limb_to_generic(&xs, m, d, 0));
                assert_eq!(generic, want, "generic len {len} m {m}");
                #[cfg(target_arch = "x86_64")]
                if bmi2 {
                    // SAFETY: BMI2 detected above.
                    let fast = natural_from_kernel(xs.len(), |d| unsafe {
                        limbs_mul_limb_to_bmi2(&xs, m, d)
                    });
                    assert_eq!(fast, want, "bmi2 len {len} m {m}");
                }
            }
        }
    }
}

#[test]
fn sums_and_differences_match_malachite() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for &la in LENS {
        for &lb in LENS {
            for a in patterns(&mut rng, la) {
                for b in patterns(&mut rng, lb) {
                    let (na, nb) = (nat(&a), nat(&b));
                    assert_eq!(natural_add_limbs(&a, &b), &na + &nb, "add {la} {lb}");
                    // Difference of the larger minus the smaller, normalized
                    // to significant limbs first (a kernel operand's top limb
                    // may be zero here, which the kernel must still accept).
                    let (big, small) = if na >= nb { (&a, &b) } else { (&b, &a) };
                    let (nbig, nsmall) = (nat(big), nat(small));
                    let small_sig = nsmall.to_limbs_asc();
                    let big_sig = nbig.to_limbs_asc();
                    assert_eq!(
                        natural_sub_limbs(&big_sig, &small_sig),
                        &nbig - &nsmall,
                        "sub {la} {lb}"
                    );
                    for (sa, sb) in [(false, false), (false, true), (true, false), (true, true)] {
                        let ia = Integer::from_sign_and_abs(!sa, na.clone());
                        let ib = Integer::from_sign_and_abs(!sb, nb.clone());
                        assert_eq!(
                            integer_add_ref(&ia, &ib),
                            &ia + &ib,
                            "iadd {la} {lb} {sa} {sb}"
                        );
                        assert_eq!(
                            integer_sub_ref(&ia, &ib),
                            &ia - &ib,
                            "isub {la} {lb} {sa} {sb}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn integer_product_by_i64_matches_malachite() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for &len in LENS {
        for xs in patterns(&mut rng, len) {
            for negative in [false, true] {
                let x = Integer::from_sign_and_abs(!negative, nat(&xs));
                for n in [0, 1, -1, 2, -3, i64::MAX, i64::MIN, rng.next() as i64] {
                    assert_eq!(
                        integer_mul_i64(&x, n),
                        &x * Integer::from(n),
                        "len {len} n {n}"
                    );
                }
            }
        }
    }
}

#[test]
fn small_quotient_is_exact_or_declines() {
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let mut tries = 0usize;
    let mut hits = 0usize;
    for &ld in &[1usize, 2, 3, 4, 7, 64, 150] {
        for (pattern, d) in patterns(&mut rng, ld).into_iter().enumerate() {
            let nd = nat(&d);
            if nd == 0 {
                continue;
            }
            let quotients: [u128; 9] = [
                0,
                1,
                2,
                9,
                1000,
                1 << 32,
                1 << 63,
                u128::from(u64::MAX),
                (1u128 << 64) + 5,
            ];
            for q in quotients {
                let one = Natural::from(1u32);
                let random_r = &nat(&(0..ld).map(|_| rng.next()).collect::<Vec<_>>()) % &nd;
                for (which_r, r) in [Natural::from(0u32), one.clone(), &nd - &one, random_r]
                    .into_iter()
                    .enumerate()
                {
                    let n = Natural::from(q) * &nd + &r;
                    let got = natural_div_small_quotient(&n.to_limbs_asc(), &d);
                    if let Some(got) = got {
                        assert_eq!(Natural::from(got), &n / &nd, "d len {ld} q {q}");
                    }
                    // Hit rate over the realistic case only: a random divisor
                    // (pattern 0) and a random remainder (which_r 3). The
                    // remainders 1 and d-1 sit next to a quotient boundary,
                    // where declining is the correct answer.
                    if q <= 1000 && ld >= 3 && pattern == 0 && which_r == 3 {
                        tries += 1;
                        hits += usize::from(got.is_some());
                    }
                }
            }
        }
        // Unrelated operands: never a wrong answer.
        for _ in 0..200 {
            let ln = ld + (rng.next() % 3) as usize;
            let n: Vec<u64> = (0..ln).map(|_| rng.next()).collect();
            let dd: Vec<u64> = (0..ld).map(|_| rng.next()).collect();
            if nat(&dd) == 0 {
                continue;
            }
            if let Some(got) = natural_div_small_quotient(&n, &dd) {
                assert_eq!(Natural::from(got), nat(&n) / nat(&dd));
            }
        }
    }
    // The shortcut must actually take the common case, or it is dead weight.
    assert!(hits * 10 >= tries * 9, "fast path took {hits} of {tries}");
}

/// The value-returning signed sum (the `+`/`-` unit's body) against
/// malachite, over every length pair and sign pair: exact, and a fixnum
/// exactly when the result fits one.
#[test]
fn signed_add_limbs_value_matches_malachite() {
    crate::test_utils::init_test_tracing();
    let fixnum = |x: &Integer| {
        *x >= Integer::from(Value::MOST_NEGATIVE_FIXNUM)
            && *x <= Integer::from(Value::MOST_POSITIVE_FIXNUM)
    };
    let mut rng = Rng(0x0bad_5eed_1234_4321);
    for &la in LENS {
        for &lb in LENS {
            for a in patterns(&mut rng, la) {
                for b in patterns(&mut rng, lb) {
                    // Significant limbs, as the callers pass them.
                    let (na, nb) = (nat(&a), nat(&b));
                    let (sa, sb) = (na.to_limbs_asc(), nb.to_limbs_asc());
                    for (a_neg, b_neg) in
                        [(false, false), (false, true), (true, false), (true, true)]
                    {
                        let ia = Integer::from_sign_and_abs(!a_neg, na.clone());
                        let ib = Integer::from_sign_and_abs(!b_neg, nb.clone());
                        let want = &ia + &ib;
                        let got = signed_add_limbs_value(a_neg, &sa, b_neg, &sb);
                        let got_exact = match got.as_fixnum() {
                            Some(n) => Integer::from(n),
                            None => got.as_bignum().expect("integer").clone(),
                        };
                        assert_eq!(got_exact, want, "{la} {lb} {a_neg} {b_neg}");
                        assert_eq!(got.is_fixnum(), fixnum(&want), "{la} {lb} demotion");
                    }
                }
            }
        }
    }
}
