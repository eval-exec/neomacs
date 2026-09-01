use super::*;

#[test]
fn runtime_owned_system_name_refreshes_but_a_lisp_replacement_does_not() {
    let mut eval = Context::new();
    refresh_system_name_from(&mut eval, "host-after-refresh".to_string());
    assert_eq!(
        eval.obarray()
            .symbol_value("system-name")
            .and_then(|value| value.as_utf8_str()),
        Some("host-after-refresh")
    );

    // GNU compares object identity, not string contents: replacing the
    // variable with an equal, freshly allocated string is still a Lisp
    // override and must stop automatic refresh.
    eval.set_variable("system-name", Value::string("host-after-refresh"));
    refresh_system_name_from(&mut eval, "host-after-second-refresh".to_string());
    assert_eq!(
        eval.obarray()
            .symbol_value("system-name")
            .and_then(|value| value.as_utf8_str()),
        Some("host-after-refresh")
    );
}

std::cfg_select! {
    unix => {
        #[test]
        fn process_credentials_come_directly_from_the_os() {
            assert_eq!(effective_uid(), unsafe { libc::geteuid() as i64 });
            assert_eq!(
                i64::from(process_user_id(CredentialScope::Effective)),
                unsafe { libc::geteuid() as i64 }
            );
            assert_eq!(
                i64::from(process_user_id(CredentialScope::Real)),
                unsafe { libc::getuid() as i64 }
            );
            assert_eq!(
                i64::from(process_group_id(CredentialScope::Effective)),
                unsafe { libc::getegid() as i64 }
            );
            assert_eq!(
                i64::from(process_group_id(CredentialScope::Real)),
                unsafe { libc::getgid() as i64 }
            );
        }
    }
    _ => {}
}
