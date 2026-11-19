use neovm_value::quit::QuitRequest;
use neovm_value::quit::quit_detach::detach_quit_activation;

fn main() {
    let request = QuitRequest::new();
    let _receipt = detach_quit_activation(&request);
}
