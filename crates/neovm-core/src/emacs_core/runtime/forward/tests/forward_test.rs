use super::*;

#[test]
fn lisp_fwd_type_codes_match_gnu_lisp_fwd_type() {
    let cases = [
        (LispFwdType::Int, 0),
        (LispFwdType::Bool, 1),
        (LispFwdType::Obj, 2),
        (LispFwdType::BufferObj, 3),
        (LispFwdType::KboardObj, 4),
    ];

    for (ty, code) in cases {
        assert_eq!(ty.gnu_code(), code);
        assert_eq!(LispFwdType::from_gnu_code(code), Some(ty));
    }
    assert_eq!(LispFwdType::from_gnu_code(5), None);
}
