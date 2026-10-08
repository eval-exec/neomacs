use malachite::base::num::arithmetic::traits::Pow;
use malachite::base::num::logic::traits::SignificantBits;
use malachite::integer::Integer;

/// A power whose predicted limb count fits GNU's GMP allocation bound.
/// The owned integer and exponent carry no Lisp state and may cross mutators.
#[derive(Debug)]
pub(super) struct BoundedPower {
    base: Integer,
    exponent: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum PowerError {
    #[error("bignum power exceeds GNU's limb limit")]
    Overflow,
}

impl TryFrom<(Integer, u64)> for BoundedPower {
    type Error = PowerError;

    #[inline]
    fn try_from((base, exponent): (Integer, u64)) -> Result<Self, Self::Error> {
        // GNU bignum.c:emacs_mpz_pow_ui checks nbase*exp against
        // min(NLIMBS_LIMIT, GMP_NLIMBS_MAX - 5) before entering GMP.
        let limbs = base.significant_bits().div_ceil(super::GMP_NUMB_BITS);
        let limit = super::GMP_NLIMBS_MAX - 5;
        if limbs
            .checked_mul(exponent)
            .is_none_or(|count| count > limit)
        {
            return Err(PowerError::Overflow);
        }
        Ok(Self { base, exponent })
    }
}

impl From<BoundedPower> for Integer {
    #[inline]
    fn from(power: BoundedPower) -> Self {
        power.base.pow(power.exponent)
    }
}

impl BoundedPower {
    /// A lower bound on the result width, safe after limb-growth admission.
    /// GNU's limb limit bounds the product below 2^37 on this 64-bit target.
    #[inline]
    pub(super) fn minimum_bits(&self) -> u64 {
        let bits = self.base.significant_bits();
        if bits == 0 && self.exponent != 0 {
            0
        } else {
            bits.saturating_sub(1) * self.exponent + 1
        }
    }
}

static_assertions::assert_impl_all!(BoundedPower: Send, Sync);

#[cfg(test)]
#[path = "../tests/bounded_power.rs"]
mod tests;
