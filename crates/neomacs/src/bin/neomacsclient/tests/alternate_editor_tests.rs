use super::alternate_editor_tokens;

#[test]
fn tokenization_matches_gnu_source_not_shell_syntax() {
    for (input, expected) in [
        ("  editor  fixed  ", vec!["editor", "fixed"]),
        (
            "\"editor with space\" \"fixed with space\"",
            vec!["editor with space", "fixed with space"],
        ),
        (
            "editor 'a b' a\\ b $HOME ; *.txt",
            vec!["editor", "'a", "b'", "a\\", "b", "$HOME", ";", "*.txt"],
        ),
        ("editor\targ next\narg", vec!["editor\targ", "next\narg"]),
        ("\"a b\"tail \"a\"\"b\"", vec!["a b", "tail", "a", "b"]),
        (
            "editor \"unterminated value",
            vec!["editor", "unterminated value"],
        ),
        ("editor \"\" next", vec!["editor", "", "next"]),
        ("   ", vec![]),
    ] {
        assert_eq!(alternate_editor_tokens(input), expected, "{input:?}");
    }
}
