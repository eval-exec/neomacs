#[path = "support/activation_owner.rs"]
mod owner;

use owner::activation::OwnedRoutedWorker;

fn main() {
    let _forged = OwnedRoutedWorker::new();
}
