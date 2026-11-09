//! B7: serial twins of owner-created/copied tables against GNU 31.1.
use super::*;

fn make_copy_program(test: HashTableTest) -> Vec<Op> {
    let mut ops = vec![
        Op::HtMake {
            dst: 0,
            test,
            size: 4,
        },
        Op::ConstNil { dst: 5 },
        Op::ConstFixnum { dst: 4, value: 7 },
        Op::Cons {
            dst: 2,
            car: 4,
            cdr: 5,
        },
        Op::ConstFixnum { dst: 4, value: 41 },
        Op::Cons {
            dst: 3,
            car: 4,
            cdr: 5,
        },
        Op::HtPut {
            table: 0,
            key: 2,
            value: 3,
        },
        Op::HtCopy { table: 0, dst: 1 },
        // Source replacement and a new source-only entry must not edit copy.
        Op::ConstFixnum { dst: 4, value: 99 },
        Op::HtPut {
            table: 0,
            key: 2,
            value: 4,
        },
        Op::ConstFixnum { dst: 17, value: 8 },
        Op::ConstFixnum { dst: 4, value: 88 },
        Op::HtPut {
            table: 0,
            key: 17,
            value: 4,
        },
        // The original value cons is shared shallowly with the copied entry.
        Op::ConstFixnum { dst: 4, value: 77 },
        Op::SetCar { cell: 3, value: 4 },
        Op::HtGet {
            table: 1,
            key: 2,
            found: 6,
            value: 7,
        },
        Op::HtGet {
            table: 0,
            key: 2,
            found: 8,
            value: 9,
        },
        Op::HtCount { table: 0, dst: 10 },
        Op::HtCount { table: 1, dst: 11 },
        Op::HtClear { table: 0 },
        Op::HtCount { table: 0, dst: 15 },
        Op::HtCount { table: 1, dst: 16 },
        Op::HtGet {
            table: 1,
            key: 2,
            found: 12,
            value: 13,
        },
    ];
    ops.extend(finish_line(&[10, 11, 6, 7, 8, 9, 15, 16, 12, 13]));
    ops
}

pub(super) fn a6_ht_make_copy_serial_twins_match_gnu_oracle<H: HostParts>(host: &mut H) {
    let setup = HtSetup {
        source: "(list)",
        facts: vec![],
        nslots: HT_NSLOTS,
    };
    let programs: Vec<_> = [HashTableTest::Eq, HashTableTest::Eql, HashTableTest::Equal]
        .into_iter()
        .map(make_copy_program)
        .collect();
    for ops in &programs {
        prevalidate_with(ops, setup.nslots, 0, STEP_CAP, &setup.initial())
            .unwrap_or_else(|error| panic!("B7 copy twin prevalidate: {error:?}"));
    }
    let env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    let (lines, dumps) = run_ht_session(host, &setup, programs, 0, &env, &[]);
    assert!(dumps.is_empty());
    let oracle = oracle_lines();
    assert_eq!(lines.len(), 3);
    for (index, line) in lines.iter().enumerate() {
        assert_eq!(line, oracle[35 + index], "B7 make/copy twin {index}");
    }
}
