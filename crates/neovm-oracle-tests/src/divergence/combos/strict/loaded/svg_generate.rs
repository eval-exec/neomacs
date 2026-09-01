//! Strict combo oracle probes, batch 84: SVG generation (svg-create,
//! svg-rectangle, svg-line, svg-circle, svg-text, svg-print to XML string).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p8_svg_basic_shapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 45)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((svg (svg-create 100 50)))
  (svg-rectangle svg 10 10 80 30 :fill "red" :stroke "black")
  (svg-line svg 0 0 100 50 :stroke "blue")
  (svg-circle svg 50 25 10 :fill "green")
  (svg-text svg 5 45 "Hello" :fill "white" :font-size 12)
  (with-temp-buffer
    (svg-print svg)
    (buffer-string)))
"##,
        &["svg.el"],
        expect,
    );
}
