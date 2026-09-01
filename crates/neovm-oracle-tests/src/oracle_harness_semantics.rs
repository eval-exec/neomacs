#[test]
fn load_root_normalization_precedes_project_root_normalization() {
    let load_root = crate::common::oracle_sandbox::OracleSandbox::create_fixture_tempdir()
        .expect("external oracle load root");
    let expect =
        expect_test::expect![[r#""OK (\"[ORACLE-LOAD-ROOT]\" \"[ORACLE-PROJECT-ROOT]\")""#]];

    crate::common::assert_oracle_parity_with_load_root_expect(
        r#"(list (getenv "NEOVM_ORACLE_LOAD_ROOT")
                  (getenv "NEOVM_ORACLE_PROJECT_ROOT"))"#,
        &[],
        load_root.path(),
        expect,
    );
}

#[test]
fn oracle_sandbox_keeps_case_files_under_workspace_tmp() {
    let expect = expect_test::expect![[r#""OK (t t)""#]];

    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((scratch (file-name-as-directory
                          (getenv "NEOVM_ORACLE_SCRATCH_ROOT"))))
            (list (file-in-directory-p
                   (getenv "NEOVM_ORACLE_FORM_FILE") scratch)
                  (file-in-directory-p
                   (getenv "NEOVM_ORACLE_TEST_TMPDIR") scratch)))"#,
        expect,
    );
}

#[test]
fn oracle_sandbox_preserves_explicit_child_tmpdir() {
    let expect = expect_test::expect![[r#""OK (t t)""#]];

    crate::common::assert_oracle_parity_with_env_expect(
        r#"(let ((scratch (file-name-as-directory
                          (getenv "NEOVM_ORACLE_SCRATCH_ROOT"))))
            (list (equal (getenv "TMPDIR") "/should-win")
                  (file-in-directory-p
                   (getenv "NEOVM_ORACLE_FORM_FILE") scratch)))"#,
        &[("TMPDIR", "/should-win")],
        expect,
    );
}

#[test]
fn oracle_case_workdir_is_also_the_implicit_child_tmpdir() {
    let project_root = crate::common::oracle_sandbox::project_root();
    let sandbox =
        crate::common::oracle_sandbox::OracleSandbox::new("nil", &[], &project_root.join("lisp"))
            .expect("oracle sandbox")
            .with_case_working_directory_and_tmpdir();
    let mut command = std::process::Command::new("emacs");
    sandbox.configure(&mut command);

    let case_workdir = command.get_current_dir().expect("case working directory");
    let child_tmpdir = command
        .get_envs()
        .find_map(|(name, value)| (name == "TMPDIR").then_some(value).flatten())
        .expect("isolated child TMPDIR");

    assert_eq!(child_tmpdir, case_workdir.as_os_str());
}

#[test]
fn oracle_sandbox_pins_snapshot_locale() {
    let expect = expect_test::expect![[r#""OK (\"en_US.UTF-8\" \"en_US.UTF-8\")""#]];

    crate::common::assert_oracle_parity_expect(
        r#"(list (getenv "LANG") (getenv "LC_ALL"))"#,
        expect,
    );
}

#[test]
fn oracle_sandbox_pins_snapshot_time_zone() {
    let expect = expect_test::expect![[r#""OK (\"America/New_York\" (-18000 \"EST\"))""#]];

    crate::common::assert_oracle_parity_expect(
        r#"(list (getenv "TZ")
                  (current-time-zone (encode-time 0 0 12 15 1 2026 t)))"#,
        expect,
    );
}

#[test]
fn oracle_sandbox_isolates_home_and_identity() {
    let expect = expect_test::expect![[
        r#""OK (\"[ORACLE-HOME]\" \"exec\" \"exec\" \"oracle-host\" \"exec@oracle-host\" \"oracle-host\" \"oracle-host\" \"exec@oracle-host\" t t)""#
    ]];

    crate::common::assert_oracle_parity_expect(
        r#"(let ((home (getenv "HOME"))
                  (scratch (getenv "NEOVM_ORACLE_SCRATCH_ROOT")))
              (list home
                    (getenv "USER")
                    (getenv "LOGNAME")
                    (getenv "HOSTNAME")
                    (getenv "EMAIL")
                    system-name
                    (system-name)
                    user-mail-address
                    (file-directory-p home)
                    (file-in-directory-p home scratch)))"#,
        expect,
    );
}

#[test]
fn oracle_sandbox_blocks_parent_repository_discovery() {
    let expect = expect_test::expect![[r#""OK t""#]];

    crate::common::assert_oracle_parity_expect(
        r#"(equal (getenv "GIT_CEILING_DIRECTORIES")
                  (getenv "NEOVM_ORACLE_PROJECT_ROOT"))"#,
        expect,
    );
}

#[test]
fn oracle_string_property_coalescing_is_explicit() {
    let expect = expect_test::expect![[r#""OK #(\"abc\" 0 3 (face bold))""#]];

    crate::common::assert_oracle_parity_expect(
        r#"(let ((s (copy-sequence "abc")))
              (put-text-property 0 1 'face 'bold s)
              (put-text-property 1 3 'face 'bold s)
              (neovm--oracle-coalesce-string-properties s))"#,
        expect,
    );
}

#[test]
fn volatile_fontification_normalization_is_explicit_and_preserves_semantic_properties() {
    let expect =
        expect_test::expect![[r#""OK #(\"abc\" 0 3 (face bold org-todo-head \"TODO\"))""#]];

    crate::common::assert_oracle_parity_ignoring_volatile_fontification_expect(
        r#"(let ((s (propertize "abc"
                                'fontified nil
                                'face 'bold
                                'org-todo-head "TODO")))
              s)"#,
        expect,
    );
}
