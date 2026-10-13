//! Issue #559: TAB observes the minibuffer's local case policy.
#![cfg(unix)]
use crate::support::*;
use std::time::Duration;

#[test]
fn tab_completion_observes_a_buffer_local_case_policy() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(minibuffer-with-setup-hook (lambda () (setq-local completion-ignore-case t)) (completing-read "Case: " '("*Messages*") nil t))"#,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("Case: "))
    });
    gnu.send(b"*mes");
    neo.send(b"*mes");
    send_both(&mut gnu, &mut neo, "TAB");
    let completed = |grid: &[String]| grid.iter().any(|row| row.contains("Case: *Messages*"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), completed);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        assert!(
            completed(&grid),
            "{label} did not expand the input: {}",
            grid.join("\n")
        );
    }
    send_both(&mut gnu, &mut neo, "RET");
    let returned = |grid: &[String]| {
        grid.last()
            .is_some_and(|row| row.contains("\"*Messages*\"") && !row.contains("Case:"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), returned);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            returned(&session.text_grid()),
            "{label} did not accept the completion"
        );
    }
}
