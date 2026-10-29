//! Fresh projected names share VOID until the first canonical assignment.
//! Every handle below is admitted from the actual rooted intern result.
use super::*;
use crate::emacs_core::intern::parallel::identity_census_for_test;
use crate::tagged::gc::ProofEra as ParallelHeapEra;

fn with_inputs<R>(body: impl for<'heap> FnOnce(ParallelHeapEra<'heap>, Vec<SymId>) -> R) -> R {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let prefix = format!(
        "p3sym-c5-assignment-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let left = format!("{prefix}-left");
    let right = format!("{prefix}-right");
    assert!(crate::emacs_core::intern::lookup_interned(&left).is_none());
    assert!(crate::emacs_core::intern::lookup_interned(&right).is_none());
    Context::with_test_symbol_name_inputs_init_era(
        PlainSymbolBacking::VoidProjection,
        &left,
        |obarray, ids| obarray.seed_value_id(ids[2], Value::string(&right)),
        body,
    )
}

fn roots(era: &ParallelHeapEra<'_>, values: &[i64]) -> Vec<SharedRoot> {
    let mut stopped = era.stop().unwrap();
    let roots = era
        .with_stopped_heap_for_test(&stopped, |heap| {
            std::iter::once(Value::NIL)
                .chain(values.iter().map(|&n| Value::fixnum(n)))
                .map(|value| unsafe {
                    // SAFETY: matching stopped heap; only immediate seed words.
                    SharedRoot::new(heap, value)
                })
                .collect()
        })
        .unwrap();
    stopped.trace(|_, _| Ok(())).unwrap();
    stopped.complete().unwrap();
    roots
}

struct Inputs<'era> {
    names: [OwnerRootSlot<'era>; 2],
    dst: OwnerRootSlot<'era>,
    src: OwnerRootSlot<'era>,
    symbols: [(SymId, SymbolHandle); 2],
    before: InitialInternFacts,
}

fn prepare<'era>(
    owner: &mut Mutator<'era>,
    holders: &[SymbolHandle; 2],
    roots: &[SharedRoot],
    source: usize,
) -> Result<Inputs<'era>, String> {
    let mut names = [
        attempt(owner, "assignment name home 0", |run| {
            Ok(run.root_slot(&roots[0])?)
        })?,
        attempt(owner, "assignment name home 1", |run| {
            Ok(run.root_slot(&roots[0])?)
        })?,
    ];
    let mut dst = attempt(owner, "assignment output home", |run| {
        Ok(run.root_slot(&roots[0])?)
    })?;
    let src = attempt(owner, "assignment value home", |run| {
        Ok(run.root_slot(&roots[source])?)
    })?;
    let mut symbols = Vec::new();
    for (index, name) in names.iter_mut().enumerate() {
        attempt(owner, "assignment input load", |run| {
            Ok(run.sym_value_into(&holders[index], name)?)
        })?;
        symbols.push(attempt(owner, "assignment rooted intern/handle", |run| {
            run.initial_intern_into(name, &mut dst)?;
            let id = Value::from_bits(dst.word_for_test(run))
                .as_bare_symbol_id()
                .ok_or(Failure::Fixture("assignment intern returned no symbol"))?;
            Ok((id, run.initial_symbol_handle(&dst)?))
        })?);
    }
    let symbols: [(SymId, SymbolHandle); 2] =
        symbols.try_into().map_err(|_| "assignment handle count")?;
    for (_, handle) in &symbols {
        attempt(owner, "assignment initial boundp", |run| {
            run.sym_boundp_into(handle, &mut dst)?;
            if dst.word_for_test(run) != Value::NIL.bits() {
                return Err(Failure::Fixture(
                    "fresh assignment target was already bound before barrier",
                ));
            }
            Ok(())
        })?;
    }
    let before = attempt(owner, "assignment pre-write facts", |run| {
        Ok(run.initial_facts_for_test(symbols[0].0)?)
    })?;
    Ok(Inputs {
        names,
        dst,
        src,
        symbols,
        before,
    })
}

fn assert_void(facts: &InitialInternFacts, shared: (usize, usize)) {
    assert_eq!(facts.backing, Some(PlainSymbolBacking::VoidProjection));
    assert_eq!(facts.shared_void, Some(shared));
    assert_eq!(facts.projection, Some((shared.0, 0, Value::UNBOUND.bits())));
    assert_eq!(shared.1, Value::UNBOUND.bits());
}

#[test]
fn p3sym_c5_fresh_projected_first_assignment_preserves_shared_void() {
    let result = with_inputs(|era, ids| {
        let holders = [
            era.admit_plain_symbol(ids[1]).unwrap(),
            era.admit_plain_symbol(ids[2]).unwrap(),
        ];
        let roots = roots(&era, &[97]);
        let result = guarded(
            activate(era.register(1, 4).unwrap(), era.heap_identity(), 1).unwrap(),
            |owner| {
                let mut inputs = prepare(owner, &holders, &roots, 1)?;
                let [(left, handle), (right, other)] = inputs.symbols;
                assert_ne!(left, right);
                let before = inputs.before;
                let shared = before.shared_void.expect("actual shared VOID exists");
                assert_void(&before, shared);
                let second = attempt(owner, "second fresh VOID facts", |run| {
                    Ok(run.initial_facts_for_test(right)?)
                })?;
                assert_void(&second, shared);
                let registry = identity_census_for_test();
                attempt(owner, "makunbound on fresh VOID", |run| {
                    Ok(run.sym_makunbound_into(&handle, &mut inputs.dst)?)
                })?;
                let after_void = attempt(owner, "still VOID after makunbound", |run| {
                    Ok(run.initial_facts_for_test(left)?)
                })?;
                assert_void(&after_void, shared);
                assert_eq!(after_void.projections, before.projections);
                let charge = attempt(owner, "first assignment charge baseline", |run| {
                    Ok(run.owner.local.as_ref().unwrap().hot.bytes_since_gc)
                })?;
                attempt(owner, "first fresh set", |run| {
                    run.sym_set_into(&handle, &inputs.src, &mut inputs.dst)?;
                    if inputs.dst.word_for_test(run) != Value::fixnum(97).bits() {
                        return Err(Failure::Fixture("set returned another value"));
                    }
                    Ok(())
                })?;
                let assigned = attempt(owner, "private assignment facts", |run| {
                    Ok(run.initial_facts_for_test(left)?)
                })?;
                assert_eq!(assigned.backing, Some(PlainSymbolBacking::Projected));
                assert_eq!(assigned.projections, before.projections + 1);
                let descriptor = assigned.projection.expect("private Plain descriptor");
                assert_ne!(descriptor.0, shared.0);
                assert!(descriptor.1 > 0);
                assert_eq!(descriptor.2, Value::fixnum(97).bits());
                let charged = attempt(owner, "first assignment charged", |run| {
                    Ok(run.owner.local.as_ref().unwrap().hot.bytes_since_gc)
                })?;
                assert_eq!(
                    charged - charge,
                    assigned.projection_bytes - before.projection_bytes
                );
                // Match the existing GNU31.1 fresh-global twin, not a handwritten
                // expected Lisp result. These inputs/outputs remain rooted.
                let oracle: Vec<_> = include_str!("../../../symbol_intern_oracle.expected")
                    .lines()
                    .collect();
                let bound = attempt(owner, "GNU fresh boundp", |run| {
                    run.sym_boundp_into(&handle, &mut inputs.dst)?;
                    Ok(inputs.dst.word_for_test(run) == Value::T.bits())
                })?;
                let value = attempt(owner, "GNU fresh value", |run| {
                    run.sym_value_into(&handle, &mut inputs.dst)?;
                    Ok(Value::from_bits(inputs.dst.word_for_test(run)).as_fixnum())
                })?;
                assert_eq!(
                    format!("({} {})", if bound { "t" } else { "nil" }, value.unwrap()),
                    oracle[21]
                );
                let untouched = attempt(owner, "other fresh symbol still VOID", |run| {
                    Ok(run.initial_facts_for_test(right)?)
                })?;
                assert_void(&untouched, shared);
                assert_eq!(untouched.projections, assigned.projections);
                attempt(owner, "other fresh symbol still unbound", |run| {
                    run.sym_boundp_into(&other, &mut inputs.dst)?;
                    if inputs.dst.word_for_test(run) != Value::NIL.bits() {
                        return Err(Failure::Fixture("assignment changed another fresh symbol"));
                    }
                    let before = inputs.dst.word_for_test(run);
                    if run.sym_value_into(&other, &mut inputs.dst) != Err(GlobalError::VoidVariable)
                        || inputs.dst.word_for_test(run) != before
                    {
                        return Err(Failure::Fixture("other VOID read changed output"));
                    }
                    Ok(())
                })?;
                attempt(owner, "makunbound private descriptor", |run| {
                    Ok(run.sym_makunbound_into(&handle, &mut inputs.dst)?)
                })?;
                let unbound = attempt(owner, "private UNBOUND facts", |run| {
                    Ok(run.initial_facts_for_test(left)?)
                })?;
                assert_eq!(unbound.backing, Some(PlainSymbolBacking::Projected));
                assert_eq!(
                    unbound.projection,
                    Some((descriptor.0, descriptor.1, Value::UNBOUND.bits()))
                );
                assert_eq!(unbound.projections, assigned.projections);
                // Reuse the original VOID-admitted handle; metadata does not force
                // a second descriptor when this cell is assigned again.
                attempt(owner, "set again through original handle", |run| {
                    Ok(run.sym_set_into(&handle, &inputs.src, &mut inputs.dst)?)
                })?;
                let repeated = attempt(owner, "same private descriptor after reset", |run| {
                    Ok(run.initial_facts_for_test(left)?)
                })?;
                assert_eq!(repeated.projection, Some(descriptor));
                assert_eq!(repeated.projections, assigned.projections);
                assert_eq!(repeated.projection_bytes, assigned.projection_bytes);
                assert_eq!(repeated.shared_void, Some(shared));
                assert_eq!(
                    identity_census_for_test(),
                    registry,
                    "cell writes never invalidate registry identities"
                );
                // Re-intern also retains the winner after the cell arm changed.
                attempt(owner, "intern assigned name", |run| {
                    run.initial_intern_into(&inputs.names[0], &mut inputs.dst)?;
                    if Value::from_bits(inputs.dst.word_for_test(run)).as_bare_symbol_id()
                        != Some(left)
                    {
                        return Err(Failure::Fixture("assignment replaced symbol identity"));
                    }
                    Ok(())
                })?;
                eprintln!(
                    "P3SYM_C5_FRESH_ASSIGNMENT shared_void_unchanged=1 issued_delta=1 old_handle_reused=1"
                );
                Ok(())
            },
        );
        drop(roots);
        let (collector, keeper) = era.finish().expect("fresh assignment complete teardown");
        drop(collector);
        drop(keeper);
        eprintln!("P3SYM_C5_ASSIGNMENT_TEARDOWN_OK single");
        result
    });
    result.expect("fresh first assignment after clean teardown");
}

#[derive(Debug)]
struct AssignmentRow {
    symbols: [(SymId, SymbolHandle); 2],
    before: InitialInternFacts,
    input: i64,
    observed: i64,
}

#[test]
fn p3sym_c5_two_to_four_owners_issue_one_fresh_plain_descriptor() {
    for count in 2..=4 {
        let result = with_inputs(|era, ids| {
            let holders = [
                era.admit_plain_symbol(ids[1]).unwrap(),
                era.admit_plain_symbol(ids[2]).unwrap(),
            ];
            let values: Vec<_> = (0..count).map(|i| 97 + i as i64).collect();
            let roots = roots(&era, &values);
            let permits: Vec<_> = (1..=count)
                .map(|i| era.register(i as u64, 4).unwrap())
                .collect();
            let heap = era.heap_identity();
            let ready = Barrier::new(count);
            let rows = std::thread::scope(|scope| {
                let mut jobs = Vec::new();
                for (index, permit) in permits.into_iter().enumerate() {
                    let (ready, roots, holders) = (&ready, &roots, &holders);
                    jobs.push(scope.spawn(move || {
                        let mut owner = activate(permit, heap, index as u64 + 1);
                        let prepared =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match owner
                                .as_mut()
                            {
                                Ok(owner) => prepare(owner, holders, roots, index + 1),
                                Err(error) => Err(error.clone()),
                            }))
                            .unwrap_or_else(|_| Err("fresh assignment setup assertion".into()));
                        // Even an unsuccessful setup reaches this one barrier.
                        // No write begins before every pre-write check ends.
                        ready.wait();
                        match owner {
                            Ok(owner) => guarded(owner, |owner| {
                                let mut inputs = prepared?;
                                let handle = inputs.symbols[0].1;
                                let input = 97 + index as i64;
                                attempt(owner, "contending first fresh assignment", |run| {
                                    run.sym_set_into(&handle, &inputs.src, &mut inputs.dst)?;
                                    if inputs.dst.word_for_test(run) != Value::fixnum(input).bits()
                                    {
                                        return Err(Failure::Fixture(
                                            "contending set returned another actor's value",
                                        ));
                                    }
                                    Ok(())
                                })?;
                                let observed =
                                    attempt(owner, "concurrent assigned value", |run| {
                                        run.sym_value_into(&handle, &mut inputs.dst)?;
                                        Value::from_bits(inputs.dst.word_for_test(run))
                                            .as_fixnum()
                                            .ok_or(Failure::Fixture(
                                                "assignment read was not immediate",
                                            ))
                                    })?;
                                Ok(AssignmentRow {
                                    symbols: inputs.symbols,
                                    before: inputs.before,
                                    input,
                                    observed,
                                })
                            }),
                            Err(error) => Err(error),
                        }
                    }));
                }
                jobs.into_iter()
                    .map(|job| {
                        job.join()
                            .map_err(|_| "fresh assignment join assertion".to_owned())
                            .and_then(|row| row)
                    })
                    .collect::<Result<Vec<_>, _>>()
            });
            let result = rows.and_then(|rows| {
                guarded(activate(era.register(9001, 1).unwrap(), heap, 9001)?, |owner| {
                    let [(left, handle), (right, _)] = rows[0].symbols;
                    let after = attempt(owner, "final first-assignment facts", |run| Ok(run.initial_facts_for_test(left)?))?;
                    let other = attempt(owner, "final untouched VOID facts", |run| Ok(run.initial_facts_for_test(right)?))?;
                    let shared = rows[0].before.shared_void.expect("actual shared VOID");
                    for row in &rows {
                        assert_eq!(row.symbols, rows[0].symbols);
                        assert_void(&row.before, shared);
                        assert_eq!(row.before.projections, rows[0].before.projections);
                        assert!(values.contains(&row.observed));
                    }
                    let submitted: BTreeSet<_> = rows.iter().map(|row| row.input).collect();
                    assert_eq!(submitted.len(), count);
                    assert_eq!(after.projections, rows[0].before.projections + 1);
                    assert_eq!(after.backing, Some(PlainSymbolBacking::Projected));
                    let private = after.projection.expect("one private descriptor");
                    assert_ne!(private.0, shared.0);
                    assert!(private.1 > 0);
                    assert!(submitted.contains(&Value::from_bits(private.2).as_fixnum().unwrap()));
                    assert_void(&other, shared);
                    assert_eq!(other.projections, after.projections);
                    // The pre-assignment handle still addresses the current cell;
                    // a rooted read, not just a facts snapshot, must see it.
                    let mut dst = attempt(owner, "final value root", |run| Ok(run.root_slot(&roots[0])?))?;
                    attempt(owner, "final original-handle read", |run| {
                        run.sym_value_into(&handle, &mut dst)?;
                        if dst.word_for_test(run) != private.2 { return Err(Failure::Fixture("old handle missed committed assignment")); }
                        Ok(())
                    })?;
                    eprintln!("P3SYM_C5_ASSIGNMENT_HISTORY owners={count} writers={count} issued_delta=1 shared_void_unchanged=1");
                    Ok(())
                })
            });
            drop(roots);
            let (collector, keeper) = era
                .finish()
                .expect("fresh assignment history after all owners retire");
            drop(collector);
            drop(keeper);
            eprintln!("P3SYM_C5_ASSIGNMENT_TEARDOWN_OK owners={count}");
            result
        });
        result.expect("fresh assignment history after clean teardown");
    }
}
