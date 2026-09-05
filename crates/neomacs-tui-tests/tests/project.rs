#![cfg(unix)]
//! TUI comparisons for common project.el workflows.
//!
//! GNU behavior here is driven by `lisp/progmodes/project.el`:
//! `project-find-file` reads a file from the current project and
//! `project-dired` opens Dired at `project-root`.

use crate::support;

use neomacs_tui_tests::TuiTempDirectory;
use std::fs;
use std::process::Command;
use std::time::Duration;
use support::*;

fn make_git_project_fixture(label: &str) -> TuiTempDirectory {
    let root = TuiTempDirectory::new(&format!("neomacs-project-root-{label}-"));
    let src = root.join("src");
    fs::create_dir_all(&src).expect("create project fixture source directory");
    fs::write(root.join("README.md"), "# Neo project probe\n").expect("write project readme");
    fs::write(src.join("alpha.el"), "(defun neo-project-alpha () 1)\n")
        .expect("write alpha source");
    fs::write(src.join("beta.el"), "(defun neo-project-beta () 2)\n").expect("write beta source");

    let status = Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(root.path())
        .status()
        .expect("run git init for project fixture");
    assert!(
        status.success(),
        "git init should succeed for project fixture"
    );

    let status = Command::new("git")
        .arg("-C")
        .arg(root.path())
        .arg("add")
        .arg("README.md")
        .arg("src/alpha.el")
        .arg("src/beta.el")
        .status()
        .expect("run git add for project fixture");
    assert!(
        status.success(),
        "git add should succeed for project fixture"
    );

    root
}

#[test]
fn git_project_fixture_removes_its_tree_when_its_owner_drops() {
    let fixture = make_git_project_fixture("drop-contract");
    let path = fixture.path().to_path_buf();

    drop(fixture);

    assert!(
        !path.exists(),
        "temporary project fixture survived its owning value: {}",
        path.display()
    );
}

#[test]
fn project_find_file_via_mx_opens_file_relative_to_git_root() {
    let (mut gnu, mut neo) = boot_pair("");
    let root = make_git_project_fixture("find-file");
    let alpha = root.join("src/alpha.el");

    open_file_path(
        &mut gnu,
        &mut neo,
        &alpha,
        "(defun neo-project-alpha",
        "C-x C-f",
    );
    invoke_mx_command(&mut gnu, &mut neo, "project-find-file");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.last().is_some_and(|row| row.contains("Find file"))
    });
    gnu.send(b"src/beta.el");
    neo.send(b"src/beta.el");
    send_both(&mut gnu, &mut neo, "RET");

    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("beta.el"))
            && grid
                .iter()
                .any(|row| row.contains("(defun neo-project-beta"))
    });
    // `show-paren-mode` paints through GNU's `show-paren-delay` idle timer.
    // File contents becoming visible is therefore not yet a stable display
    // boundary: allow both editors to reach the same post-idle presentation
    // before comparing their complete cell grids.
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    assert_pair_exact_display(
        "project_find_file_via_mx_opens_file_relative_to_git_root",
        &gnu,
        &neo,
    );
}

#[test]
fn project_dired_via_mx_opens_project_root_listing() {
    let (mut gnu, mut neo) = boot_pair("");
    let root = make_git_project_fixture("dired");
    let alpha = root.join("src/alpha.el");

    open_file_path(
        &mut gnu,
        &mut neo,
        &alpha,
        "(defun neo-project-alpha",
        "C-x C-f",
    );
    invoke_mx_command(&mut gnu, &mut neo, "project-dired");

    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("Dired by name"))
            && grid.iter().any(|row| row.contains("README.md"))
            && grid.iter().any(|row| row.contains("src"))
    });
    assert_pair_exact_display(
        "project_dired_via_mx_opens_project_root_listing",
        &gnu,
        &neo,
    );
}
