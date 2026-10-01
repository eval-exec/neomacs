mod harness;
mod multi_buffer;
mod preludes;
mod single_buffer;

use harness::*;
use multi_buffer::*;
use single_buffer::*;

#[test]
fn helm_css_scss_public_tui_workflows_match_gnu() {
    helm_css_scss_unadapted_public_command_preserves_exact_helm_arity_failure();
    helm_css_scss_named_display_adapter_drives_real_single_buffer_helm();
    helm_css_scss_named_display_adapter_drives_real_multi_buffer_helm();
}
