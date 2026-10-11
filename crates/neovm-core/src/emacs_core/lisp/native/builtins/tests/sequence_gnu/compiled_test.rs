use super::assert_gnu;

#[test]
fn compiled_sequence_edges() {
    assert_gnu(
        "compiled_sequence_edges",
        r#"(progn (require 'bytecomp) (defun gdl--take (n l) (take n l)) (defun gdl--ntake (n l) (ntake n l)) (defun gdl--value (a b) (value< a b)) (byte-compile 'gdl--take) (byte-compile 'gdl--ntake) (byte-compile 'gdl--value) (list (gdl--take (expt 2 100) '(1 2)) (gdl--ntake (- (expt 2 100)) (list 1 2)) (condition-case e (gdl--ntake 2 (cons 1 2)) (error e)) (gdl--value 9007199254740992.0 9007199254740993)))"#,
        include_str!("compiled_sequence_edges.expect"),
    );
}
