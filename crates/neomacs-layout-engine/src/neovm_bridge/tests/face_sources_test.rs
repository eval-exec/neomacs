use super::*;

#[test]
fn retained_face_sources_survive_collection_after_parent_is_dropped() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let mut attributes = Face::new("retained-source");
    attributes.family = Some(Value::string("retained-family"));
    attributes.foundry = Some(Value::string("retained-foundry"));
    attributes.stipple = Some(Value::list(vec![Value::string("retained-stipple")]));
    let parent = RootedFace::new(attributes).expect("installed mutator heap");

    let mut overlay = Face::new("weight-only");
    overlay.weight = Some(neovm_core::face::FontWeight::BOLD);
    let retained = RootedFace::with_base(parent.attributes().merge(&overlay), Some(&parent))
        .expect("merge inherits live roots");
    assert!(Arc::ptr_eq(&parent.roots, &retained.roots));
    drop(parent);

    for _ in 0..3 {
        context.gc_collect_exact();
        assert_eq!(
            retained
                .attributes()
                .family_runtime_string_owned()
                .as_deref(),
            Some("retained-family")
        );
        assert_eq!(
            retained
                .attributes()
                .foundry_runtime_string_owned()
                .as_deref(),
            Some("retained-foundry")
        );
        assert_eq!(
            retained
                .attributes()
                .stipple
                .expect("retained stipple")
                .cons_car()
                .as_str_owned()
                .as_deref(),
            Some("retained-stipple")
        );
    }
}

#[test]
fn resolver_definition_roots_preserve_an_unused_face_after_live_table_replacement() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let mut live_table = FaceTable::new();
    let mut later = Face::new("not-yet-displayed");
    later.family = Some(Value::string("later-family"));
    later.foundry = Some(Value::string("later-foundry"));
    later.stipple = Some(Value::list(vec![Value::string("later-stipple")]));
    later.inherit = Some(Value::list(vec![Value::string("default")]));
    live_table.define("not-yet-displayed", later);
    let snapshot = live_table.clone();
    let _definition_roots = FaceTableRoots::new(&snapshot).expect("installed mutator heap");
    drop(live_table);

    for _ in 0..3 {
        context.gc_collect_exact();
        let retained = snapshot.resolve("not-yet-displayed");
        assert_eq!(
            retained.family_runtime_string_owned().as_deref(),
            Some("later-family")
        );
        assert_eq!(
            retained.foundry_runtime_string_owned().as_deref(),
            Some("later-foundry")
        );
        assert_eq!(
            retained.stipple.expect("stipple").cons_car().as_utf8_str(),
            Some("later-stipple")
        );
        let inherit = retained.inherit.expect("inheritance list");
        assert_eq!(inherit.cons_car().as_utf8_str(), Some("default"));
    }
}
