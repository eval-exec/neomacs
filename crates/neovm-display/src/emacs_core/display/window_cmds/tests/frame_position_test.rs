use super::eval_one_with_frame;

#[test]
fn set_frame_position_validates_native_coordinate_range_like_gnu() {
    assert_eq!(
        eval_one_with_frame(
            r#"(list
                (condition-case err (set-frame-position nil 2147483648 0) (error err))
                (condition-case err (set-frame-position nil 0 -2147483649) (error err))
                (condition-case err (set-frame-position nil 0 1.5) (error err)))"#
        ),
        "OK ((args-out-of-range 2147483648 -2147483648 2147483647) (args-out-of-range -2147483649 -2147483648 2147483647) (wrong-type-argument integerp 1.5))"
    );
}
