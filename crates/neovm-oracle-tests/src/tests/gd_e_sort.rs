//! GNU31.1 sort.c:1061-1174 and fns.c:2432-2439 regression pins.
//! Refresh these expectations from the pinned GNU binary, never by hand.
use crate::common::{
    assert_oracle_parity_under_envs_expect, return_if_neovm_enable_oracle_proptest_not_set,
};

const ENVS: &[&[(&str, &str)]] = &[
    &[("NEOVM_JIT", "0")],
    &[],
    &[("NEOVM_JIT_THRESHOLD", "1"), ("NEOVM_JIT_BG", "sync")],
];

#[cfg(test)]
#[path = "gd_e_sort/key_resolution_and_frames.rs"]
mod key_resolution_and_frames;
