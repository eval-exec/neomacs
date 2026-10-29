//! Exercise the real owned production boundary, including in cfg(test) builds.
//! No proof wrapper or environment-string substitute participates here.
use super::*;
use crate::tagged::gc::MutatorKey;

const VALUE_NAME: &str = "nmjit-n2-owned-context-value";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActualMode {
    Gen0,
    Gen1,
}

impl ActualMode {
    fn read(context: &Context) -> Self {
        match context.tagged_heap.generational_enabled() {
            false => Self::Gen0,
            true => Self::Gen1,
        }
    }

    fn configure(self, context: &mut Context) {
        context.tagged_heap.set_generational_enabled(match self {
            Self::Gen0 => false,
            Self::Gen1 => true,
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyVisit {
    NotEntered,
    Entered,
}

#[derive(Debug)]
struct CancelledOwners {
    keys: [MutatorKey; 2],
}

struct Attempt {
    visit: BodyVisit,
    outcome: Result<Result<CancelledOwners, String>, OwnedParallelError>,
}

fn attempt(context: &mut Context) -> Attempt {
    let mut visit = BodyVisit::NotEntered;
    let outcome = context.admit_owned_parallel(|projection| {
        visit = BodyVisit::Entered;
        // Every early return is confined to evidence production. Each permit
        // remains an ordinary RAII local and is cancelled before finish.
        let evidence = (|| {
            let era = projection.era();
            let first = era
                .register(0x4e32_01, 0)
                .map_err(|error| format!("first production permit: {error:?}"))?;
            let second = era
                .register(0x4e32_02, 0)
                .map_err(|error| format!("second production permit: {error:?}"))?;
            let keys = [first.key(), second.key()];
            drop(second);
            drop(first);
            let census = era
                .stopped_summaries()
                .map_err(|error| format!("cancelled production summaries: {error:?}"))?
                .census()
                .map_err(|error| format!("cancelled production census: {error:?}"))?;
            if census.cancelled_starts != 2 || census.entries != 3 {
                return Err(format!("two cancelled production permits: {census:?}"));
            }
            if keys[0].heap_identity() != era.heap_identity()
                || keys[1].heap_identity() != era.heap_identity()
                || keys[0].owner() == keys[1].owner()
                || keys[0] == keys[1]
            {
                return Err(format!("production permit identities: {keys:?}"));
            }
            Ok(CancelledOwners { keys })
        })();
        // There are no owner activations, root homes or outstanding permits.
        // Evidence errors travel back only AFTER ownership has been returned.
        let (collector, keeper) = projection.finish().unwrap_or_else(|error| {
            panic!("NMJIT_N2_CONTEXT_FINISH_ERROR after permit cancellation: {error:?}")
        });
        (collector, keeper, evidence)
    });
    Attempt { visit, outcome }
}

fn check_serial_recovery(context: &mut Context, expected: i64) -> Result<(), String> {
    let value = context.variable_value_by_name(VALUE_NAME);
    if value != Some(TaggedValue::fixnum(expected)) {
        return Err(format!(
            "serial global after admission: {value:?}, expected {expected}"
        ));
    }
    // Use the serial TLS allocation leaf as well as explicit Context access.
    // No collection, callback or further allocation occurs before inspection.
    let pair = TaggedValue::cons(TaggedValue::fixnum(expected), TaggedValue::NIL);
    if pair.cons_car() != TaggedValue::fixnum(expected)
        || !context.tagged_heap.owns_heap_value_for_test(pair)
    {
        return Err("serial allocation did not recover the actual Context heap".to_owned());
    }
    Ok(())
}

fn exercise_actual_context(mut context: Context) -> Result<(), String> {
    let actual = ActualMode::read(&context);
    context.set_variable(VALUE_NAME, TaggedValue::fixnum(41));
    let first = attempt(&mut context);
    match actual {
        ActualMode::Gen0 => {
            if first.visit != BodyVisit::NotEntered {
                return Err("GEN0 entered the production multi-mutator body".to_owned());
            }
            match first.outcome {
                Err(OwnedParallelError::GenerationalRequired) => {}
                Err(error) => return Err(format!("GEN0 refusal lost its typed reason: {error:?}")),
                Ok(evidence) => {
                    return Err(format!("GEN0 admitted production authority: {evidence:?}"));
                }
            }
            check_serial_recovery(&mut context, 41)?;
            context.set_variable(VALUE_NAME, TaggedValue::fixnum(42));
            let second = attempt(&mut context);
            if second.visit != BodyVisit::NotEntered {
                return Err("GEN0 retry entered the production multi-mutator body".to_owned());
            }
            match second.outcome {
                Err(OwnedParallelError::GenerationalRequired) => {}
                Err(error) => return Err(format!("GEN0 retry lost its typed reason: {error:?}")),
                Ok(evidence) => {
                    return Err(format!(
                        "GEN0 retry admitted production authority: {evidence:?}"
                    ));
                }
            }
            check_serial_recovery(&mut context, 42)?;
            eprintln!("NMJIT_N2_CONTEXT_GEN0_TYPED_REFUSAL_CLEAN_RECOVERY");
        }
        ActualMode::Gen1 => {
            if first.visit != BodyVisit::Entered {
                return Err(format!(
                    "GEN1 production body not entered: {:?}",
                    first.outcome
                ));
            }
            let first = first
                .outcome
                .map_err(|error| format!("GEN1 admission: {error:?}"))??;
            check_serial_recovery(&mut context, 41)?;
            context.set_variable(VALUE_NAME, TaggedValue::fixnum(42));
            let second = attempt(&mut context);
            if second.visit != BodyVisit::Entered {
                return Err(format!(
                    "GEN1 production re-admission not entered: {:?}",
                    second.outcome
                ));
            }
            let second = second
                .outcome
                .map_err(|error| format!("GEN1 re-admission: {error:?}"))??;
            if first.keys[0].generation() == second.keys[0].generation()
                || first.keys[1].generation() == second.keys[1].generation()
            {
                return Err(
                    "GEN1 production re-admission reused registration generations".to_owned(),
                );
            }
            check_serial_recovery(&mut context, 42)?;
            eprintln!("NMJIT_N2_CONTEXT_GEN1_TWO_PERMITS_FINISH_AND_READMIT");
        }
    }
    if ActualMode::read(&context) != actual {
        return Err("production admission changed the actual collector mode".to_owned());
    }
    Ok(())
}

impl Context {
    /// The N1 manifest uses the actual production capability boundary and the
    /// current heap mode; it does not infer admission from an environment flag.
    pub(crate) fn nmjit_n2_production_boundary_for_test() -> Result<(), String> {
        exercise_actual_context(Self::new_minimal_vm_harness())
    }
}

#[test]
fn nmjit_n2_owned_context_actual_mode_boundary_and_recovery() {
    Context::nmjit_n2_production_boundary_for_test().expect("actual production Context boundary");
}

#[test]
fn nmjit_n2_owned_context_gen0_refuses_without_entering_and_recovers() {
    let mut context = Context::new_minimal_vm_harness();
    ActualMode::Gen0.configure(&mut context);
    exercise_actual_context(context).expect("actual GEN0 production refusal and recovery");
}

#[test]
fn nmjit_n2_owned_context_gen1_two_permits_finish_and_readmit() {
    let mut context = Context::new_minimal_vm_harness();
    ActualMode::Gen1.configure(&mut context);
    exercise_actual_context(context).expect("actual GEN1 production admission and recovery");
}
