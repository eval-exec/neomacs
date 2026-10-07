//! Complete cold-frame controls precede direct-demand quality assertions.
use super::*;
#[path = "opt_sink_numeric_fixture.rs"]
mod fixture;
use fixture::*;
use std::collections::{HashMap, HashSet};

// Count only literal normalized true branches inside cold blocks whose two
// targets are cold. Source arithmetic in this fixture has no static-Float
// seed, so its numeric dispatch predicates remain actual runtime predicates.
// The actual current pc5 cold frame contains one fresh numeric identity in
// two direct stack slots; its outer demand uses exactly this constant branch.
fn direct_cold_demand_branches(clif: &str) -> usize {
    let mut cold = HashSet::new();
    let mut constants = HashMap::new();
    let mut aliases = HashMap::new();
    let mut branches = Vec::new();
    let mut block = String::new();
    for line in clif.lines() {
        let line = line.split(';').next().unwrap().trim();
        if line.starts_with("block") {
            block = line
                .split(|c: char| c == '(' || c == ':' || c.is_ascii_whitespace())
                .next()
                .unwrap()
                .to_owned();
            if line.split_whitespace().any(|part| part == "cold:") {
                cold.insert(block.clone());
            }
        } else if let Some((from, to)) = line.split_once(" -> ") {
            if from.starts_with('v') {
                aliases.insert(from.to_owned(), to.to_owned());
            }
        } else if let Some((value, instruction)) = line.split_once(" = ") {
            let mut fields = instruction.split_whitespace();
            if let (Some(op), Some(bits)) = (fields.next(), fields.next()) {
                if let Some(ty) = op.strip_prefix("iconst.") {
                    let bits = bits.replace('_', "");
                    let parsed = if let Some(hex) = bits.strip_prefix("0x") {
                        i64::from_str_radix(hex, 16).ok()
                    } else {
                        bits.parse::<i64>().ok()
                    };
                    if let Some(bits) = parsed {
                        constants.insert(value.to_owned(), (ty.to_owned(), bits));
                    }
                }
            }
        } else if line.starts_with("brif ") {
            branches.push((block.clone(), line.to_owned()));
        }
    }
    branches
        .into_iter()
        .filter(|(block, line)| {
            if !cold.contains(block) {
                return false;
            }
            let Some((flag, targets)) = line.strip_prefix("brif ").unwrap().split_once(',') else {
                return false;
            };
            let mut flag = flag.trim();
            let mut seen = HashSet::new();
            while let Some(next) = aliases.get(flag) {
                if !seen.insert(flag.to_owned()) {
                    return false;
                }
                flag = next;
            }
            if !matches!(constants.get(flag), Some((ty, 1)) if ty == "i8") {
                return false;
            }
            // Block arguments can contain commas. Split only at the outer
            // target separator, then retain each exact target block name.
            let mut depth = 0usize;
            let separator = targets.char_indices().find_map(|(at, ch)| match ch {
                '(' => {
                    depth += 1;
                    None
                }
                ')' => {
                    depth = depth.saturating_sub(1);
                    None
                }
                ',' if depth == 0 => Some(at),
                _ => None,
            });
            let Some(separator) = separator else {
                return false;
            };
            let target = |text: &str| {
                text.trim()
                    .split(|c: char| c == '(' || c.is_ascii_whitespace())
                    .next()
                    .unwrap()
                    .to_owned()
            };
            cold.contains(&target(&targets[..separator]))
                && cold.contains(&target(&targets[separator + 1..]))
        })
        .count()
}

#[test]
fn opt_sink_cold_direct_slot_demand_keeps_complete_alias_frame_without_constant_outer_branches() {
    // Parser controls are part of this isolated test, not extra registered
    // tests. The actual counter assertion below remains after all semantics.
    let positive = "block0:\n v0 = iconst.i8 1\n v1 -> v0\n jump block1\n\
                    block1 cold:\n brif v1, block2(v5, v6), block3(v7)\n\
                    block2 cold:\n jump block3\n block3(v8: i64) cold:\n return v8\n";
    assert_eq!(direct_cold_demand_branches(positive), 1);
    assert_eq!(
        direct_cold_demand_branches(&positive.replace("iconst.i8 1", "iconst.i8 0")),
        0
    );
    assert_eq!(
        direct_cold_demand_branches(&positive.replace("block1 cold:", "block1:")),
        0
    );
    assert_eq!(
        direct_cold_demand_branches(&positive.replace("brif v1,", "brif v9,")),
        0
    );
    assert_eq!(
        direct_cold_demand_branches(&positive.replace("iconst.i8 1", "iconst.i64 1")),
        0
    );

    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let source = program(
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::Dup,
            Op::StackRef(2),
            Op::Add,
            Op::Return,
        ],
        Vec::new(),
        3,
    );
    let cases = vec![
        vec![
            Value::make_float(2.0),
            Value::make_float(3.0),
            Value::make_float(4.0),
        ],
        vec![Value::make_int(2), Value::make_int(3), Value::make_int(4)],
        vec![
            Value::make_float(-0.0),
            Value::make_float(3.0),
            Value::make_float(-0.0),
        ],
    ];
    for args in &cases {
        roots.add(args);
        let answer = tier0(&mut ctx, &source, args).unwrap();
        roots.add(&[answer]);
    }
    // Both actual leaves compile before any wrong-type input poisons numeric
    // feedback. All success/error observations still precede the quality check.
    let (baseline, _) = compile(&ctx, &source, false, &roots);
    let (selected, clif) = compile(&ctx, &source, true, &roots);
    check_semantics(&mut ctx, &source, &baseline, &cases, &roots);
    check_semantics(&mut ctx, &source, &selected, &cases, &roots);
    let bad = [
        Value::make_float(2.0),
        Value::make_float(3.0),
        Value::string("not-a-number"),
    ];
    roots.add(&bad);
    let expected = tier0(&mut ctx, &source, &bad);
    let prefix = program(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Mul, Op::Return],
        Vec::new(),
        2,
    );
    let product = tier0(&mut ctx, &prefix, &bad[..2]).unwrap();
    roots.add(&[product]);
    for leaf in [&baseline, &selected] {
        let NativeRun::DeoptAt(exit) = leaf.call_consts(
            &mut ctx as *mut Context as *mut u8,
            source.constants.as_ptr(),
            &bad,
        ) else {
            panic!("wrong third operand must recover original Add pc5");
        };
        roots.add(&exit.stack);
        assert_eq!(exit.pc, 5);
        assert_eq!(exit.stack.len(), 6);
        assert_eq!(exit.handlers, 0);
        for slot in 0..3 {
            assert_eq!(exit.stack[slot].bits(), bad[slot].bits());
        }
        assert_eq!(
            exit.stack[3].bits(),
            exit.stack[4].bits(),
            "one actual alias identity"
        );
        assert_ne!(exit.stack[3].bits(), bad[0].bits());
        assert_ne!(exit.stack[3].bits(), bad[1].bits());
        assert_eq!(exit.stack[3].xfloat().to_bits(), product.xfloat().to_bits());
        assert_eq!(exit.stack[5].bits(), bad[2].bits());
        let actual = resume(&mut ctx, &source, &exit);
        assert_eq!(observe(&actual), observe(&expected));
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
    assert_eq!(
        direct_cold_demand_branches(&clif),
        0,
        "validated direct frame-slot demand must not emit an outer constant-true diamond: {clif}"
    );
}
