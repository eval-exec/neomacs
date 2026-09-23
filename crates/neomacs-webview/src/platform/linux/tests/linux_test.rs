use std::path::Path;

use super::file_navigation_uri;

#[test]
fn local_file_navigation_uses_a_percent_encoded_file_uri() {
    assert_eq!(
        file_navigation_uri(Path::new("/tmp/web view#1.html")).as_deref(),
        Ok("file:///tmp/web%20view%231.html")
    );
}
