use super::*;

#[test]
fn test_find_drm_render_nodes() {
    let nodes = find_drm_render_nodes();
    // Should find at least one on a system with GPU
    println!("Found {} render nodes", nodes.len());
    for node in &nodes {
        println!("  {:?}", node);
    }
}
