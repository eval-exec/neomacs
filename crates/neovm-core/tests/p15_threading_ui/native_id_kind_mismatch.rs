#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

fn accept_thread(_: threading::ThreadId) {}
fn accept_mutex(_: threading::MutexId) {}

fn cross_kinds(mut ids: threading::ThreadIds) {
    accept_thread(ids.issue_mutex().unwrap());
    accept_mutex(ids.issue_condvar().unwrap());
}

fn main() {}
