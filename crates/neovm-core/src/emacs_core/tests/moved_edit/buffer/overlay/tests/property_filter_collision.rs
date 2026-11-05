use super::*;

#[test]
fn colliding_property_names_keep_public_lisp_queries_exact() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (property, collision) = colliding_properties();
    let property_name = neovm_value::intern::resolve_sym(property.as_bare_symbol_id().unwrap());
    let collision_name = neovm_value::intern::resolve_sym(collision.as_bare_symbol_id().unwrap());
    let result = eval
        .eval_str(&format!(
            r#"(progn
                 (defun collision-query-values (p q)
                   (list (get-char-property 3 p) (get-char-property 5 p)
                         (get-char-property 3 q) (get-char-property 5 q)
                         (next-single-char-property-change 1 p)
                         (next-single-char-property-change 4 p)
                         (previous-single-char-property-change 13 p)
                         (previous-single-char-property-change 10 p)))
                 (insert "xxxxxxxxxxxx")
                 (let* ((p '{property_name}) (q '{collision_name})
                        (carrier (make-overlay 4 10))
                        (unrelated (make-overlay 2 12))
                        result)
                   (overlay-put carrier 'priority 20)
                   (overlay-put carrier p 11)
                   (overlay-put unrelated 'priority 10)
                   (overlay-put unrelated q 99)
                   (push (collision-query-values p q) result)
                   (overlay-put carrier q 22)
                   (push (collision-query-values p q) result)
                   (overlay-put carrier p nil)
                   (push (collision-query-values p q) result)
                   (overlay-put carrier q nil)
                   (overlay-put carrier p 33)
                   (push (collision-query-values p q) result)
                   (nreverse result)))"#
        ))
        .expect("colliding-name property queries");

    // Captured from GNU 31.1. The signature only filters candidates; exact
    // property lookup and eq value comparisons are textprop.c:649,843-845,
    // and 921-940 for the reverse scan.
    assert_eq!(
        format!("{result}"),
        "((nil 11 99 99 4 10 10 4) (nil 11 99 22 4 10 10 4) (nil nil 99 22 13 13 1 1) (nil 33 99 99 4 10 10 4))"
    );
}

fn colliding_properties() -> (Value, Value) {
    let mut signatures: Vec<(OverlayPropertyFilter, Value)> = Vec::new();
    // Each single-name signature selects one of 256 bits. Comparing the
    // public filters finds a pair even if symbol allocation order changes.
    for index in 0..=256 {
        let property = Value::symbol(format!("overlay-filter-collision-{index}"));
        let filter = OverlayPropertyFilter::for_properties([property]);
        if let Some((_, previous)) = signatures
            .iter()
            .find(|(signature, _)| *signature == filter)
        {
            assert_ne!(property.bits(), previous.bits());
            return (*previous, property);
        }
        signatures.push((filter, property));
    }
    panic!("257 single-property signatures must contain a collision");
}
