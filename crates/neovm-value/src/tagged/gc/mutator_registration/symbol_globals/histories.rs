//! Native finite-attempt histories on real existing World symbols. No timers.
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::{PlainSymbolBacking, RegistrationError};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use std::num::NonZeroU64;

#[derive(Debug)]
struct Row {
    answer: Option<usize>,
    read: Option<Option<i64>>,
    final_value: Option<Option<i64>>,
    error: Option<GlobalError>,
}

fn sample<'era>(
    run: &mut RunAccess<'_, 'era>,
    handle: &SymbolHandle,
    dst: &mut OwnerRootSlot<'era>,
) -> Result<Option<i64>, GlobalError> {
    match run.sym_value_into(handle, dst) {
        Ok(()) => Value::from_bits(dst.word_for_test(run))
            .as_fixnum()
            .map(Some)
            .ok_or(GlobalError::Authority(RegistrationError::Publication)),
        Err(GlobalError::VoidVariable) => Ok(None),
        Err(error) => Err(error),
    }
}

#[test]
fn p3sym_c3_two_to_four_owner_real_global_histories() {
    for kind in [
        PlainSymbolBacking::Unprojected,
        PlainSymbolBacking::Projected,
    ] {
        for count in 2..=4 {
            for unbind in [false, true] {
                let (rows, symbol) = Context::with_test_symbol_backing_era(kind, |era, id| {
                    let handle = era.admit_plain_symbol(id).unwrap();
                    let mut stopped = era.stop().unwrap();
                    let roots = era
                        .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                            // SAFETY: matching stopped heap, scalar setup only.
                            [
                                SharedRoot::new(heap, Value::fixnum(11)),
                                SharedRoot::new(heap, Value::fixnum(22)),
                                SharedRoot::new(heap, Value::NIL),
                            ]
                        })
                        .unwrap();
                    stopped.trace(|_, _| Ok(())).unwrap();
                    stopped.complete().unwrap();
                    let permits: Vec<_> =
                        (1..=count).map(|n| era.register(n, 3).unwrap()).collect();
                    let ready = std::sync::Barrier::new(count as usize);
                    let done = std::sync::Barrier::new(count as usize);
                    let heap = era.heap_identity();
                    let rows = std::thread::scope(|scope| {
                        let mut jobs = Vec::new();
                        for (index, permit) in permits.into_iter().enumerate() {
                            let (roots, ready, done) = (&roots, &ready, &done);
                            jobs.push(scope.spawn(move || {
                                let mut owner = super::super::tests::activate_retry(
                                    permit,
                                    heap,
                                    index as u64 + 1,
                                );
                                ready.wait();
                                let mut row = Row {
                                    answer: None,
                                    read: None,
                                    final_value: None,
                                    error: None,
                                };
                                let (src, mut dst) = {
                                    let mut run = owner.run_access().unwrap();
                                    (
                                        run.root_slot(&roots[index.min(1)]).unwrap(),
                                        run.root_slot(&roots[2]).unwrap(),
                                    )
                                };
                                for _ in 0..3 {
                                    let mut run = owner.run_access().unwrap();
                                    let outcome = if index < 2 {
                                        let result = if unbind && index == 1 {
                                            run.sym_makunbound_into(&handle, &mut dst)
                                        } else {
                                            run.sym_set_into(&handle, &src, &mut dst)
                                        };
                                        result.map(|()| row.answer = Some(dst.word_for_test(&run)))
                                    } else {
                                        sample(&mut run, &handle, &mut dst)
                                            .map(|value| row.read = Some(value))
                                    };
                                    match outcome {
                                        Ok(()) => break,
                                        Err(GlobalError::Authority(RegistrationError::Busy)) => {}
                                        Err(error) => {
                                            row.error = Some(error);
                                            break;
                                        }
                                    }
                                    drop(run);
                                    std::thread::yield_now();
                                }
                                done.wait();
                                {
                                    let mut run = owner.run_access().unwrap();
                                    if index == 0 {
                                        match sample(&mut run, &handle, &mut dst) {
                                            Ok(value) => row.final_value = Some(value),
                                            Err(error) => row.error = Some(error),
                                        }
                                    }
                                    run.clear_root_slots().unwrap();
                                }
                                super::super::tests::finish_retry(owner, NonZeroU64::MIN);
                                row
                            }));
                        }
                        jobs.into_iter()
                            .map(|job| job.join().unwrap())
                            .collect::<Vec<_>>()
                    });
                    drop(roots);
                    let (collector, keeper) = era.finish().unwrap();
                    drop(collector);
                    drop(keeper);
                    (rows, id)
                });
                assert!(
                    rows.iter().all(|row| row.error.is_none()),
                    "{kind:?}/{count}/{unbind}: {rows:?}"
                );
                let mut legal = vec![Some(73)];
                for (index, row) in rows[..2].iter().enumerate() {
                    if let Some(answer) = row.answer {
                        let value = if unbind && index == 1 {
                            None
                        } else {
                            Some(if index == 0 { 11 } else { 22 })
                        };
                        let expected = value.map_or(Value::from_sym_id(symbol), Value::fixnum);
                        assert_eq!(
                            answer,
                            expected.bits(),
                            "set/makunbound returns its own answer"
                        );
                        legal.push(value);
                    }
                }
                for row in &rows[2..] {
                    if let Some(value) = row.read {
                        assert!(legal.contains(&value));
                    }
                }
                let final_value = rows[0].final_value.expect("final quiescent observation");
                if legal.len() > 1 {
                    assert!(legal[1..].contains(&final_value));
                } else {
                    assert_eq!(final_value, Some(73));
                }
            }
        }
    }
}
