use neovm_value::quit::quit_detach::DetachedQuitActivation;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireSend::<DetachedQuitActivation>::check();
}
