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

#[test]
fn a6_ht_make_copy_serial_twins_match_gnu_oracle() {
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
    let (lines, dumps) = run_ht_session(&setup, programs, 0, &env, &[]);
    assert!(dumps.is_empty());
    let oracle = oracle_lines();
    assert_eq!(lines.len(), 3);
    for (index, line) in lines.iter().enumerate() {
        assert_eq!(line, oracle[35 + index], "B7 make/copy twin {index}");
    }
}

// Kept identical to the fourth B7 form in restricted_lisp_oracle.el. Both
// hash and compare callbacks copy a guarded source, mutate the copy and
// observe that the original remains guarded. The copy preserves metadata
// and the custom test callbacks; each later custom lookup exercises them.
const CUSTOM_COPY_FORM: &str = r#"(let ((answers nil) (callbacks '(compare hash)))
  (while callbacks
    (let ((callback (car callbacks))
          (source nil) (armed nil) (fired nil) (copy nil) (observed nil))
      (let ((action
             (lambda ()
               (if armed
                   (if fired nil
                     (progn
                       (setq fired t copy (copy-hash-table source))
                       (setq observed
                             (list (eq (hash-table-test source) (hash-table-test copy))
                                   (= (hash-table-size source) (hash-table-size copy))
                                   (= (hash-table-rehash-size source)
                                      (hash-table-rehash-size copy))
                                   (= (hash-table-rehash-threshold source)
                                      (hash-table-rehash-threshold copy))
                                   (eq (hash-table-weakness source)
                                       (hash-table-weakness copy))
                                   (puthash "copy-only" 2 copy)
                                   (condition-case err (puthash "source-only" 3 source)
                                     (error (list (car err) (car (cdr err))
                                                  (eq (nth 2 err) source))))))))))))
        (define-hash-table-test
         'a6-b7-copy-test
         (lambda (a b)
           (if (eq callback 'compare) (funcall action))
           (equal a b))
         (lambda (_key)
           (if (eq callback 'hash) (funcall action))
           0))
        (setq source (make-hash-table :test 'a6-b7-copy-test :size 32
                                      :rehash-size 2.0 :rehash-threshold 0.8))
        (puthash (copy-sequence "key") 1 source)
        (setq armed t)
        (let ((answer (gethash (copy-sequence "key") source)))
          (setq armed nil)
          (setq answers
                (cons (list callback answer observed
                            (hash-table-count source) (hash-table-count copy)
                            (gethash "key" copy) (gethash "copy-only" source 'absent))
                      answers)))))
    (setq callbacks (cdr callbacks)))
  answers)"#;

#[test]
fn a6_ht_copy_metadata_and_custom_callbacks_match_gnu_oracle() {
    let mut context = Context::new();
    let answer = context
        .eval_str(CUSTOM_COPY_FORM)
        .unwrap_or_else(|error| panic!("B7 custom copy twin eval: {error:?}"));
    assert_eq!(crate::emacs_core::print_value(&answer), oracle_lines()[38]);
}
