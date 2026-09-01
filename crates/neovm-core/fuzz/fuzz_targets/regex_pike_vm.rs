#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use neovm_core::fuzz_support::RegexDifferential;
use neovm_core_fuzz::{ArbitraryRegexCase, check};

fuzz_target!(|case: ArbitraryRegexCase<'_>| -> Corpus { check(case, RegexDifferential::PikeVm) });
