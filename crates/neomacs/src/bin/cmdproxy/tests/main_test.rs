use super::*;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn unix_c_switch_becomes_a_typed_command_invocation() {
    assert_eq!(
        parse_invocation(args(&["-c", "whoami"])),
        Ok(Invocation::Command {
            command: OsString::from("whoami"),
            pass_through: Vec::new(),
        })
    );
}

#[test]
fn windows_c_switch_preserves_the_complete_shell_command() {
    assert_eq!(
        parse_invocation(args(&["/c", "echo one | findstr one"])),
        Ok(Invocation::Command {
            command: OsString::from("echo one | findstr one"),
            pass_through: Vec::new(),
        })
    );
}

#[test]
fn no_command_is_an_interactive_invocation_with_shell_switches() {
    assert_eq!(
        parse_invocation(args(&["/q"])),
        Ok(Invocation::Interactive {
            pass_through: args(&["/q"]),
        })
    );
}

#[test]
fn command_dot_com_environment_size_is_accepted_but_other_e_switches_pass_through() {
    assert_eq!(
        parse_invocation(args(&["-e:4096", "-example"])),
        Ok(Invocation::Interactive {
            pass_through: args(&["-example"]),
        })
    );
}

#[test]
fn c_without_a_command_is_rejected_before_launch() {
    assert_eq!(
        parse_invocation(args(&["-c"])),
        Err("cmdproxy: expecting a command after -c".to_owned())
    );
}

#[test]
fn command_processor_plan_requires_comspec() {
    let invocation = parse_invocation(args(&["-c", "whoami"])).unwrap();
    assert_eq!(
        plan_command_processor(invocation, None),
        Err("cmdproxy: COMSPEC is not set".to_owned())
    );
}

#[test]
fn command_processor_plan_keeps_comspec_and_command_separate() {
    let invocation = parse_invocation(args(&["-c", "echo %USERNAME%"])).unwrap();
    assert_eq!(
        plan_command_processor(invocation, Some(OsString::from(r"C:\Windows\cmd.exe"))),
        Ok(CommandProcessorPlan {
            program: OsString::from(r"C:\Windows\cmd.exe"),
            pass_through: Vec::new(),
            command: Some(OsString::from("echo %USERNAME%")),
        })
    );
}
