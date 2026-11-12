//! Per-entity rescaling used by GNU font.c's listing preference score.
use super::*;
use neovm_value::list_scan::GnuTailCycle;

pub(super) struct FontRescalePolicy {
    rules: Value,
    representatives: Value,
    mode: neovm_value::tagged::SymbolMode,
}

impl FontRescalePolicy {
    pub(super) fn new(
        rules: Value,
        representatives: Value,
        mode: neovm_value::tagged::SymbolMode,
    ) -> Self {
        Self {
            rules,
            representatives,
            mode,
        }
    }

    pub(super) fn ratio(&self, entity: Value) -> Result<f64, Flow> {
        let mut tail = self.rules;
        let mut cycle = GnuTailCycle::new(tail);
        let mut name = None;
        while tail.is_cons() {
            let entry = tail.cons_car();
            if !entry.is_cons() {
                // GNU assumes an association cons here. Bound malformed
                // policy admitted by Neomacs's mutable value extensions.
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), entry],
                ));
            }
            let ratio = entry.cons_cdr();
            if ratio.is_float() {
                let key = entry.cons_car();
                let matches = if let Some(pattern) = key.as_lisp_string() {
                    if name.is_none() {
                        // GNU serializes the fixed slots here. Keep copied
                        // PRIMARY extras for font-spec matching, while legacy
                        // public-renderer aliases cannot override native XLFD
                        // fields during this internal regexp comparison.
                        let naming_entity = font_value_fields(&entity).map_or(entity, |fields| {
                            Value::vector(
                                properties::FontProperties::decode(fields).flat().to_vec(),
                            )
                        });
                        name = Some(font_xlfd_name(vec![naming_entity, Value::NIL, Value::T])?);
                    }
                    if let Some(text) = name.and_then(Value::as_utf8_str) {
                        neovm_text::emacs_core::regex::predicate_match_ignore_case(pattern, text)
                            .map_err(|error| {
                                signal("invalid-regexp", vec![Value::string(error.to_string())])
                            })?
                    } else {
                        false
                    }
                } else if is_font_spec(&key) {
                    self.entity_matches(key, entity)?
                } else {
                    false
                };
                if matches {
                    return Ok(ratio.xfloat());
                }
            }
            tail = tail.cons_cdr();
            if tail.is_cons() {
                // No Lisp runs during this pure walk; the original policy
                // continues to root every reachable tortoise cell.
                cycle.check(tail)?;
            }
        }
        Ok(1.0)
    }

    fn same(&self, left: Value, right: Value) -> bool {
        eq_value(&self.mode.unwrap(left), &self.mode.unwrap(right))
    }

    // Read only the fixed/basic lane before validating a consumed key's
    // extras. A basic identity mismatch must skip even forged cyclic extras.
    fn basic(&self, font: Value, name: &str) -> Value {
        font_value_fields(&font)
            .and_then(|fields| {
                properties::FontProperties::decode(fields)
                    .flat()
                    .get(1..)
                    .and_then(|flat| {
                        flat.chunks_exact(2)
                            .find(|pair| {
                                properties::canonical_name(pair[0], self.mode) == Some(name)
                            })
                            .map(|pair| pair[1])
                    })
            })
            .unwrap_or(Value::NIL)
    }

    fn style(&self, value: Value, name: &str) -> Option<i64> {
        if let Some(symbol) = value.as_symbol_id(self.mode) {
            let word = resolve_sym(symbol);
            font_style_table_for_key(name)?
                .iter()
                .find(|(_, aliases)| aliases.iter().any(|alias| alias.eq_ignore_ascii_case(word)))
                .map(|(number, _)| *number)
        } else {
            value.as_fixnum().map(|packed| packed >> 8)
        }
    }

    fn entity_matches(&self, spec: Value, entity: Value) -> Result<bool, Flow> {
        for key in ["foundry", "family", "adstyle", "registry"] {
            let wanted = self.basic(spec, key);
            let actual = self.basic(entity, key);
            if !wanted.is_nil() && !actual.is_nil() && !self.same(wanted, actual) {
                return Ok(false);
            }
        }
        let Some(fields) = font_value_fields(&spec) else {
            return Ok(false);
        };
        properties::validate(fields)?;
        // A SIZE-bearing key re-enters font_rescale_ratio from font_score
        // before its accumulated style score is tested. Basic mismatches above
        // legitimately avoid that recursion. Bound the otherwise nonterminating
        // fixed-entity path instead of claiming portable behavior for it.
        if !self.basic(spec, "size").is_nil() {
            return Err(signal(
                "error",
                vec![Value::string("Recursive font rescale specification")],
            ));
        }
        for key in ["weight", "slant", "width"] {
            let wanted = self.basic(spec, key);
            let actual = self.basic(entity, key);
            if !wanted.is_nil()
                && !self.same(wanted, actual)
                && let (Some(wanted), Some(actual)) =
                    (self.style(wanted, key), self.style(actual, key))
                && wanted != actual
            {
                return Ok(false);
            }
        }
        let properties = properties::FontProperties::decode(fields);
        if let Some(head) = properties.extras() {
            // Storage was validated above; association cells and the spine are
            // finite. Only the special extras participate in font_match_p.
            for entry in list_iter(head) {
                if !self.extra_matches(entry.cons_car(), entry.cons_cdr(), entity)? {
                    return Ok(false);
                }
            }
        } else {
            // Preserve the readable legacy flat representation without
            // rewriting a caller's font-spec while matching it.
            for pair in properties.flat()[1..].chunks_exact(2) {
                if !properties::basic(pair[0], self.mode)
                    && !self.extra_matches(pair[0], pair[1], entity)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn extra(&self, font: Value, key: Value) -> Option<Value> {
        let fields = font_value_fields(&font)?;
        let properties = properties::FontProperties::decode(fields);
        if let Some(head) = properties.extras() {
            properties::assoc(head, key, self.mode).map(Value::cons_cdr)
        } else {
            properties.flat()[1..]
                .chunks_exact(2)
                .find(|pair| self.same(pair[0], key))
                .map(|pair| pair[1])
        }
    }

    fn member(&self, needle: Value, list: Value) -> Result<bool, Flow> {
        neovm_value::list_scan::builtin_memq_values(
            needle,
            list,
            self.mode == neovm_value::tagged::SymbolMode::Positioned,
        )
        .map(|tail| !tail.is_nil())
    }

    fn extra_matches(&self, key: Value, value: Value, entity: Value) -> Result<bool, Flow> {
        match properties::canonical_name(key, self.mode) {
            Some("otf") => Ok(false), // font_match_p requires a FONT_OBJECT.
            Some("script") => {
                if let Some(entry) =
                    font_script_association(self.representatives, value, self.mode)?
                {
                    let representatives = entry.cons_cdr();
                    // Entity matching rejects both list/vector coverage;
                    // only opened font objects can answer it in GNU.
                    Ok(!representatives.is_cons() && !representatives.is_vector())
                } else {
                    Ok(true)
                }
            }
            Some("lang") => {
                let Some(actual) = self.extra(entity, key) else {
                    return Ok(false);
                };
                if value.is_cons() {
                    if !actual.is_cons() || !self.member(value, actual)? {
                        return Ok(false);
                    }
                    // GNU tests memq on the list itself and does not advance
                    // this successful loop. Ordinary list-valued queries fail
                    // above; bound its self-referential/nonterminating case.
                    Err(signal(
                        "error",
                        vec![Value::string("Recursive font rescale language requirement")],
                    ))
                } else if actual.is_cons() {
                    self.member(value, actual.cons_cdr())
                } else {
                    Ok(self.same(value, actual))
                }
            }
            _ => Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity() -> Value {
        let font = font_spec(vec![
            Value::keyword("family"),
            Value::string("Rescale Trade"),
            Value::keyword("size"),
            Value::fixnum(24),
            Value::keyword("weight"),
            Value::symbol("bold"),
        ])
        .unwrap();
        let mut fields = font.as_vector_data().unwrap().to_vec();
        fields[0] = Value::keyword(FONT_ENTITY_TAG);
        let _ = font.replace_vector_data(fields);
        font
    }

    fn policy(key: Value, ratio: Value) -> FontRescalePolicy {
        FontRescalePolicy::new(
            Value::list(vec![Value::cons(key, ratio)]),
            Value::NIL,
            neovm_value::tagged::SymbolMode::Bare,
        )
    }

    #[test]
    fn font_listing_rescale_matches_entity_xlfd_regexp() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        assert_eq!(
            policy(Value::string("-24-"), Value::make_float(2.0))
                .ratio(entity())
                .unwrap(),
            2.0
        );
    }

    #[test]
    fn font_listing_rescale_matches_an_entity_font_spec() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        let key = font_spec(vec![Value::keyword("weight"), Value::symbol("bold")]).unwrap();
        assert_eq!(
            policy(key, Value::make_float(2.0)).ratio(entity()).unwrap(),
            2.0
        );
    }

    #[test]
    fn font_listing_rescale_preserves_invalid_regexp_error() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        assert!(
            policy(Value::string("["), Value::make_float(2.0))
                .ratio(entity())
                .is_err()
        );
    }

    #[test]
    fn font_listing_rescale_ignores_integer_ratio_values() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        assert_eq!(
            policy(Value::string("-24-"), Value::fixnum(2))
                .ratio(entity())
                .unwrap(),
            1.0
        );
    }
    #[test]
    fn font_listing_rescale_first_float_rule_and_missing_style_follow_entity_scoring() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        let target = entity();
        let mut fields = target.as_vector_data().unwrap().to_vec();
        let weight = fields[1..]
            .chunks_exact(2)
            .position(|pair| pair[0] == Value::keyword("weight"))
            .unwrap();
        drop(fields.drain(1 + weight * 2..3 + weight * 2));
        let _ = target.replace_vector_data(fields);
        let key = font_spec(vec![Value::keyword("weight"), Value::symbol("bold")]).unwrap();
        let rules = Value::list(vec![
            Value::cons(Value::string("["), Value::fixnum(2)),
            Value::cons(key, Value::make_float(1.95)),
            Value::cons(Value::string(".*"), Value::make_float(3.0)),
        ]);
        assert_eq!(
            FontRescalePolicy::new(rules, Value::NIL, neovm_value::tagged::SymbolMode::Bare)
                .ratio(target)
                .unwrap(),
            1.95
        );
    }

    #[test]
    fn font_listing_rescale_entity_special_extras_preserve_source_presence_rules() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        let target = entity();
        for (key, value, representatives, expected) in [
            ("otf", Value::NIL, Value::NIL, 1.0),
            (
                "script",
                Value::symbol("latin"),
                Value::list(vec![Value::cons(
                    Value::symbol("latin"),
                    Value::list(vec![Value::fixnum(65)]),
                )]),
                1.0,
            ),
            (
                "script",
                Value::symbol("latin"),
                Value::list(vec![Value::cons(
                    Value::symbol("latin"),
                    Value::vector(vec![]),
                )]),
                1.0,
            ),
            ("script", Value::symbol("unknown"), Value::NIL, 2.0),
            ("lang", Value::NIL, Value::NIL, 1.0),
            ("probe", Value::fixnum(1), Value::NIL, 2.0),
            ("spacing", Value::fixnum(110), Value::NIL, 2.0),
        ] {
            let key_spec = font_spec(vec![Value::keyword(key), value]).unwrap();
            let policy = FontRescalePolicy::new(
                Value::list(vec![Value::cons(key_spec, Value::make_float(2.0))]),
                representatives,
                neovm_value::tagged::SymbolMode::Bare,
            );
            assert_eq!(policy.ratio(target).unwrap(), expected, "{key}");
        }
        let mut fields = target.as_vector_data().unwrap().to_vec();
        properties::set_user(
            &mut fields,
            Value::keyword("lang"),
            Value::NIL,
            neovm_value::tagged::SymbolMode::Bare,
        );
        let _ = target.replace_vector_data(fields);
        let key = font_spec(vec![Value::keyword("lang"), Value::NIL]).unwrap();
        assert_eq!(
            policy(key, Value::make_float(2.0)).ratio(target).unwrap(),
            2.0
        );
    }

    #[test]
    fn font_listing_rescale_bounds_forged_extras_and_size_recursion_lazily() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        let target = entity();
        let entries = Value::list(vec![Value::cons(Value::keyword("probe"), Value::fixnum(1))]);
        entries.set_cdr(entries);
        for (family, error) in [("Different", false), ("Rescale Trade", true)] {
            let key = Value::vector(vec![
                Value::keyword(FONT_SPEC_TAG),
                Value::keyword("family"),
                Value::symbol(family),
                Value::cons(Value::NIL, entries),
            ]);
            assert_eq!(
                policy(key, Value::make_float(2.0)).ratio(target).is_err(),
                error
            );
        }
        // Style mismatch must not hide GNU's recursive SIZE path.
        let key = font_spec(vec![
            Value::keyword("weight"),
            Value::symbol("regular"),
            Value::keyword("size"),
            Value::fixnum(12),
        ])
        .unwrap();
        assert!(policy(key, Value::make_float(2.0)).ratio(target).is_err());
        let rules = Value::list(vec![Value::cons(
            Value::string("never-matches"),
            Value::make_float(2.0),
        )]);
        rules.set_cdr(rules);
        assert!(
            FontRescalePolicy::new(rules, Value::NIL, neovm_value::tagged::SymbolMode::Bare)
                .ratio(target)
                .is_err()
        );
    }
    #[test]
    fn font_listing_rescale_memq_preserves_early_match_and_dotted_tail_errors() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        let policy = FontRescalePolicy::new(
            Value::NIL,
            Value::NIL,
            neovm_value::tagged::SymbolMode::Bare,
        );
        let list = Value::cons(Value::symbol("en"), Value::symbol("dotted"));
        assert!(policy.member(Value::symbol("en"), list).unwrap());
        assert!(policy.member(Value::symbol("fr"), list).is_err());
    }
    #[test]
    fn font_listing_rescale_regex_name_ignores_primary_unknown_width_aliases() {
        let _eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
        for alias in ["set-width", "setwidth"] {
            let target = entity();
            font_put_entity_field(target, "width", Value::symbol("normal"));
            let primary = font_spec(vec![Value::keyword(alias), Value::string("foo")]).unwrap();
            let extras = font_primary_extra_fields(&primary, neovm_value::tagged::SymbolMode::Bare);
            let mut fields = target.as_vector_data().unwrap().to_vec();
            properties::replace_root(&mut fields, properties::copy_outer(extras));
            let _ = target.replace_vector_data(fields);
            let rules = Value::list(vec![
                Value::cons(Value::string("-foo-"), Value::make_float(2.0)),
                Value::cons(Value::string("-normal-"), Value::make_float(3.0)),
            ]);
            let policy =
                FontRescalePolicy::new(rules, Value::NIL, neovm_value::tagged::SymbolMode::Bare);
            assert_eq!(policy.ratio(target).unwrap(), 3.0, "{alias}");
        }
    }
}
