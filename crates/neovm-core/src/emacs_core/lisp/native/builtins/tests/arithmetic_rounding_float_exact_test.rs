//! Two-argument `floor`/`ceiling`/`round`/`truncate` with a FLOAT operand
//! are exact in GNU: `rounding_driver` (src/floatfns.c) scales both operands
//! by one power of two until both are integers (`double_integer_scale`,
//! `rescale_for_division`) and divides those. NeoMacs divided the doubles,
//! which is wrong whenever the quotient is not representable —
//! `(truncate 1e19 3)` gave 3333333333333333504.
//!
//! Every expected value below was printed by GNU Emacs 31.1 for the same
//! form (`tmp/eb/round-float-oracle.el`).

use crate::emacs_core::{Context, format_eval_result};

const CASES: &[(&str, &str)] = &[
    ("(truncate 1e+19 3)", "3333333333333333333"),
    (
        "(floor 1e+40 -7)",
        "-1428571428571428614826575489571952412965",
    ),
    ("(round 2305843009213693951 -1.5)", "-1537228672809129301"),
    ("(ceiling -2305843009213693952 -1.5)", "1537228672809129302"),
    ("(floor 7.5 2)", "3"),
    ("(round 2.5 1)", "2"),
    ("(round 3.5 1)", "4"),
    ("(round -2.5 1.0)", "-2"),
    ("(truncate 0.1 0.03)", "3"),
    ("(floor -0.1 0.03)", "-4"),
    ("(ceiling 1 3.0)", "1"),
    ("(floor 5e-324 1e-323)", "0"),
    ("(round 1e-323 2e-323)", "0"),
    (
        "(truncate 1e+300 1e-300)",
        "1000000000000000027445668419995659875244796358044313676727560640707721473677129974465030959007791523737770641007428964959882495731025663624106378316147323965003504481201666647589977342127480258076802105106130501481098580391304349935418871974203032166042985002456270442020084075063273390391540777564193110182711662115108254645395450887763669395137932406983787779369901988610709753985682440518274480101045044222319804448592891955004281158975890153655920804900126986847655461642241911870109200044413086162246654156782408345890095918627729681258275637897481180158583341416181375201704389907976207969478018",
    ),
    ("(floor 1 1.0e+INF)", "0"),
    ("(floor -1 1.0e+INF)", "0"),
    ("(floor -1.5 1.0e+INF)", "0"),
    ("(ceiling 1.5 -1.0e+INF)", "0"),
    ("(floor 1.0e+INF 2)", "(ERR overflow-error)"),
    ("(floor 1.0e+INF 1.0e+INF)", "(ERR overflow-error)"),
    ("(floor 0.0e+NaN 2)", "(ERR overflow-error)"),
    ("(floor 2 0.0e+NaN)", "(ERR overflow-error)"),
    ("(floor 1 0.0)", "(ERR arith-error)"),
    ("(floor 1 -0.0)", "(ERR arith-error)"),
    ("(floor 1.5 0)", "(ERR arith-error)"),
    ("(floor -0.0 5)", "0"),
    (
        "(round 1.5e+308 3)",
        "50000000000000000548953181472022770870246154838655923168405341451578792702455745768581664489247344449530624834860586257805795141871570044164153504599073023015635832251466513592848744849794279521669192233082500589213448813106472588814045597893353729061391985085892207552645901446603936636487442857715111559168",
    ),
    (
        "(truncate (expt 10 400) 1e+300)",
        "9999999999999999474952397447955825080453808753854334266271986350228103576374309212138831700198838451",
    ),
    (
        "(floor (- (expt 3 300)) 7.25)",
        "-18881583318425982895355314121667353926408785603494818823652440010809516852068838150868993952508819501827653026289378488663932790726302865457380",
    ),
    ("(round 5 2.0)", "2"),
    ("(round 7 2.0)", "4"),
];

#[test]
fn float_operand_rounding_is_exact_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    for &(form, gnu) in CASES {
        let got = format_eval_result(&ev.eval_str(form));
        if let Some(rest) = gnu.strip_prefix("(ERR ") {
            let condition = rest.trim_end_matches(')');
            assert!(
                got.starts_with("ERR") && got.contains(condition),
                "{form}: GNU signals {condition}, got {got}"
            );
        } else {
            assert_eq!(got, format!("OK {gnu}"), "{form}");
        }
    }
}
