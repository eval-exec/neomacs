use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_min_args};
use crate::emacs_core::eval::LispArgVec;
use smallvec::SmallVec;

type MapResultVec = SmallVec<[Value; 8]>;

pub(crate) fn gnu_mapconcat_unfilled_slot_value() -> Value {
    // GNU `Fmapconcat` allocates the concat argument vector before calling
    // `mapcar1`.  In non-checking builds, a callback that shortens a list
    // leaves the later slot observable when `concat` type-checks it.
    Value::fixnum(35_184_318_513_152)
}

pub(crate) fn map_sequence_length(sequence: Value) -> Result<usize, Flow> {
    if super::chartable::is_char_table(&sequence) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), sequence],
        ));
    }

    match sequence.kind() {
        ValueKind::Nil => Ok(0),
        ValueKind::Cons => super::cons_list::proper_list_length_or_signal(sequence),
        ValueKind::String => Ok(sequence.as_lisp_string().expect("string").schars()),
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::ByteCode) => {
            super::cons_list::closure_vector_length(&sequence)
                .and_then(|len| usize::try_from(len).ok())
                .ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("sequencep"), sequence],
                    )
                })
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if let Some(len) = super::chartable::bool_vector_length(&sequence) {
                usize::try_from(len).map_err(|_| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("sequencep"), sequence],
                    )
                })
            } else {
                Ok(sequence.as_vector_data().expect("vector").len())
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), sequence],
        )),
    }
}

pub(crate) fn map_sequence_element(sequence: Value, index: usize) -> Result<Value, Flow> {
    match sequence.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            if let Some(value) = super::chartable::bool_vector_ref_value(&sequence, index) {
                Ok(value)
            } else {
                Ok(sequence.as_vector_data().expect("vector")[index])
            }
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            super::cons_list::lambda_to_closure_vector(&sequence)
                .get(index)
                .copied()
                .ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("sequencep"), sequence],
                    )
                })
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            super::cons_list::bytecode_to_closure_vector(&sequence)
                .get(index)
                .copied()
                .ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("sequencep"), sequence],
                    )
                })
        }
        ValueKind::String => {
            let string = sequence.as_lisp_string().expect("string");
            super::lisp_string_char_at(string, index)
                .map(|code| Value::fixnum(code as i64))
                .ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("sequencep"), sequence],
                    )
                })
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), sequence],
        )),
    }
}

fn mapcar1_eval<F>(
    eval: &mut super::eval::Context,
    len: usize,
    values: Option<&mut MapResultVec>,
    sequence: Value,
    mut call: F,
) -> Result<usize, Flow>
where
    F: FnMut(&mut super::eval::Context, Value) -> Result<Value, Flow>,
{
    let mut values = values;
    match sequence.kind() {
        ValueKind::Nil => Ok(0),
        ValueKind::Cons => {
            let mut cursor = sequence;
            let mut mapped = 0usize;
            for _ in 0..len {
                if !cursor.is_cons() {
                    return Ok(mapped);
                }
                eval.push_vm_frame_root(cursor);
                let item = cursor.cons_car();
                let value = call(eval, item)?;
                if let Some(results) = values.as_deref_mut() {
                    eval.push_vm_frame_root(value);
                    results.push(value);
                }
                mapped += 1;
                cursor = cursor.cons_cdr();
            }
            Ok(mapped)
        }
        _ => {
            for index in 0..len {
                let item = map_sequence_element(sequence, index)?;
                let value = call(eval, item)?;
                if let Some(results) = values.as_deref_mut() {
                    eval.push_vm_frame_root(value);
                    results.push(value);
                }
            }
            Ok(len)
        }
    }
}

fn list_from_map_results(eval: &mut super::eval::Context, results: &[Value]) -> Value {
    let mut acc = Value::NIL;
    let acc_root = eval.push_vm_frame_root_slot(acc);
    for value in results.iter().rev().copied() {
        acc = Value::cons(value, acc);
        eval.set_vm_frame_root_slot(acc_root, acc);
    }
    acc
}

#[inline]
fn apply0(eval: &mut super::eval::Context, func: Value) -> EvalResult {
    eval.apply(func, crate::emacs_core::eval::LispArgVec::new())
}

#[inline]
fn apply1(eval: &mut super::eval::Context, func: Value, arg: Value) -> EvalResult {
    let mut args = crate::emacs_core::eval::LispArgVec::new();
    args.push(arg);
    eval.apply(func, args)
}
pub(crate) fn builtin_apply_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    // GNU eval.c Fapply: with one argument, the argument itself is the spread
    // list.  Its first element is the function and the remaining elements are
    // the arguments.
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("apply"), Value::fixnum(args.len() as i64)],
        ));
    }

    let last = args[args.len() - 1];
    let mut call_args = LispArgVec::new();

    if args.len() == 1 {
        let mut cursor = last;
        let func = match cursor.kind() {
            ValueKind::Nil => args[0],
            ValueKind::Cons => {
                let func = cursor.cons_car();
                cursor = cursor.cons_cdr();
                func
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), last],
                ));
            }
        };
        while cursor.is_cons() {
            call_args.push(cursor.cons_car());
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), cursor],
            ));
        }
        eval.apply_from_lisp_funcall(func, call_args)
    } else {
        call_args.extend_from_slice(&args[1..args.len() - 1]);
        let mut cursor = last;
        loop {
            match cursor.kind() {
                ValueKind::Nil => break,
                ValueKind::Cons => {
                    call_args.push(cursor.cons_car());
                    cursor = cursor.cons_cdr();
                }
                _ => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("listp"), cursor],
                    ));
                }
            }
        }
        eval.apply_from_lisp_funcall(args[0], call_args)
    }
}

pub(crate) fn builtin_funcall_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_min_args("funcall", args, 1)?;
    let func = args[0];
    let mut call_args = LispArgVec::new();
    call_args.extend_from_slice(&args[1..]);
    eval.apply_from_lisp_funcall(func, call_args)
}

pub(crate) fn builtin_funcall_interactively_slice(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_min_args("funcall-interactively", args, 1)?;
    let func = args[0];
    let mut call_args = LispArgVec::new();
    call_args.extend_from_slice(&args[1..]);
    eval.interactive.push_interactive_call(true);
    let result = eval.apply(func, call_args);
    eval.interactive.pop_interactive_call();
    result
}

pub(crate) fn builtin_funcall_with_delayed_message(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("funcall-with-delayed-message", &args, 3)?;
    let _delay = expect_number(&args[0])?;
    let _message = expect_lisp_string(&args[1])?;
    apply0(eval, args[2])
}

// ===========================================================================
// Higher-order
// ===========================================================================

pub(crate) fn builtin_mapcar_2(
    eval: &mut super::eval::Context,
    func: Value,
    seq: Value,
) -> EvalResult {
    let roots = eval.save_vm_roots();
    eval.push_vm_frame_root(func);
    eval.push_vm_frame_root(seq);
    let len = match map_sequence_length(seq) {
        Ok(len) => len,
        Err(flow) => {
            eval.restore_vm_roots(roots);
            return Err(flow);
        }
    };
    let mut results = MapResultVec::with_capacity(len);
    let map_result = mapcar1_eval(eval, len, Some(&mut results), seq, |eval, item| {
        apply1(eval, func, item)
    });
    if let Err(flow) = map_result {
        eval.restore_vm_roots(roots);
        return Err(flow);
    }
    let result_list = list_from_map_results(eval, &results);
    eval.restore_vm_roots(roots);
    Ok(result_list)
}

pub(crate) fn builtin_mapc_2(
    eval: &mut super::eval::Context,
    func: Value,
    seq: Value,
) -> EvalResult {
    let roots = eval.save_vm_roots();
    eval.push_vm_frame_root(func);
    eval.push_vm_frame_root(seq);
    let len = match map_sequence_length(seq) {
        Ok(len) => len,
        Err(flow) => {
            eval.restore_vm_roots(roots);
            return Err(flow);
        }
    };
    let result = mapcar1_eval(eval, len, None, seq, |eval, item| apply1(eval, func, item));
    eval.restore_vm_roots(roots);
    result.map(|_| ())?;
    Ok(seq)
}

pub(crate) fn builtin_mapconcat(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("mapconcat", &args, 2, 3)?;
    let func = args[0];
    let sequence = args[1];
    // Emacs 30: separator is optional, defaults to ""
    let separator = args.get(2).copied().unwrap_or_else(|| Value::string(""));

    let roots = eval.save_vm_roots();
    eval.push_vm_frame_root(func);
    eval.push_vm_frame_root(sequence);
    eval.push_vm_frame_root(separator);
    let len = match map_sequence_length(sequence) {
        Ok(len) => len,
        Err(flow) => {
            eval.restore_vm_roots(roots);
            return Err(flow);
        }
    };
    if len == 0 {
        eval.restore_vm_roots(roots);
        return Ok(Value::string(""));
    }
    let mut parts = MapResultVec::with_capacity(len);
    let mapconcat_result = mapcar1_eval(eval, len, Some(&mut parts), sequence, |eval, item| {
        apply1(eval, func, item)
    });
    let mapped = match mapconcat_result {
        Ok(mapped) => mapped,
        Err(flow) => {
            eval.restore_vm_roots(roots);
            return Err(flow);
        }
    };

    let mut concat_args = Vec::with_capacity(len * 2 - 1);
    for index in 0..len {
        if index > 0 {
            concat_args.push(separator);
        }
        concat_args.push(if index < mapped {
            parts[index]
        } else {
            gnu_mapconcat_unfilled_slot_value()
        });
    }

    let result = builtin_concat(concat_args);
    eval.restore_vm_roots(roots);
    result
}

pub(crate) fn builtin_mapcan(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if args.len() != 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("mapcan"), Value::fixnum(args.len() as i64)],
        ));
    }
    let func = args[0];
    let sequence = args[1];
    let roots = eval.save_vm_roots();
    eval.push_vm_frame_root(func);
    eval.push_vm_frame_root(sequence);
    let len = match map_sequence_length(sequence) {
        Ok(len) => len,
        Err(flow) => {
            eval.restore_vm_roots(roots);
            return Err(flow);
        }
    };
    let mut mapped = MapResultVec::with_capacity(len);
    let mapcan_result = mapcar1_eval(eval, len, Some(&mut mapped), sequence, |eval, item| {
        apply1(eval, func, item)
    });
    if let Err(flow) = mapcan_result {
        eval.restore_vm_roots(roots);
        return Err(flow);
    }
    let mapped: Vec<Value> = mapped.into_iter().collect();
    eval.restore_vm_roots(roots);
    builtin_nconc(mapped)
}

pub(crate) struct SortOptions {
    pub(crate) key_fn: Value,
    pub(crate) lessp_fn: Value,
    pub(crate) reverse: bool,
    pub(crate) in_place: bool,
}

pub(crate) trait SortRuntime {
    fn call_sort_function1(&mut self, function: Value, arg: Value) -> Result<Value, Flow>;
    fn call_sort_function2(
        &mut self,
        function: Value,
        arg0: Value,
        arg1: Value,
    ) -> Result<Value, Flow>;
    fn root_sort_value(&mut self, value: Value);
    fn compare_sort_keys(
        &mut self,
        left: &Value,
        right: &Value,
    ) -> Result<std::cmp::Ordering, Flow>;
}

impl SortRuntime for super::eval::Context {
    fn call_sort_function1(&mut self, function: Value, arg: Value) -> Result<Value, Flow> {
        let mut args = LispArgVec::new();
        args.push(arg);
        self.apply(function, args)
    }

    fn call_sort_function2(
        &mut self,
        function: Value,
        arg0: Value,
        arg1: Value,
    ) -> Result<Value, Flow> {
        let mut args = LispArgVec::new();
        args.push(arg0);
        args.push(arg1);
        self.apply(function, args)
    }

    fn root_sort_value(&mut self, value: Value) {
        self.push_specpdl_root(value);
    }

    fn compare_sort_keys(
        &mut self,
        left: &Value,
        right: &Value,
    ) -> Result<std::cmp::Ordering, Flow> {
        super::symbols::compare_value_lt(self, left, right)
    }
}

pub(crate) fn parse_sort_options(args: &[Value]) -> Result<SortOptions, Flow> {
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("sort"), Value::fixnum(0)],
        ));
    }

    // Emacs 30 sort: (sort SEQ &key :key :lessp :reverse :in-place)
    // Old form: (sort SEQ PRED) — still supported, always in-place.
    let mut key_fn = Value::NIL;
    let mut lessp_fn = Value::NIL;
    let mut reverse = false;
    let mut in_place = false;

    if args.len() == 2 {
        lessp_fn = args[1];
        in_place = true;
    } else if args.len().is_multiple_of(2) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid argument list")],
        ));
    } else if args.len() > 1 {
        let mut i = 1;
        while i < args.len() - 1 {
            match args[i].as_symbol_name() {
                Some(":key") => key_fn = args[i + 1],
                Some(":lessp") => lessp_fn = args[i + 1],
                Some(":reverse") => reverse = args[i + 1].is_truthy(),
                Some(":in-place") => in_place = args[i + 1].is_truthy(),
                _ => {
                    return Err(signal(
                        "error",
                        vec![Value::string("Invalid keyword argument"), args[i]],
                    ));
                }
            }
            i += 2;
        }
    }

    if matches!(key_fn.as_symbol_name(), Some("identity")) {
        key_fn = Value::NIL;
    }
    if matches!(lessp_fn.as_symbol_name(), Some("value<")) {
        lessp_fn = Value::NIL;
    }

    Ok(SortOptions {
        key_fn,
        lessp_fn,
        reverse,
        in_place,
    })
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_sort(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_sort_slice(eval, &args)
}

pub(crate) fn builtin_sort_slice(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    let SortOptions {
        key_fn,
        lessp_fn,
        reverse,
        in_place,
    } = parse_sort_options(args)?;

    match args[0].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => {
            let values = super::cons_list::collect_proper_list_items(args[0])?;

            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(args[0]);
            eval.push_specpdl_root(lessp_fn);
            eval.push_specpdl_root(key_fn);
            for value in &values {
                eval.push_specpdl_root(*value);
            }
            let sorted_result = stable_sort_values_with(eval, &values, key_fn, lessp_fn, reverse);
            eval.restore_specpdl_roots(roots);
            let mut sorted_values = sorted_result?;
            if in_place {
                // Re-walk the (rooted) chain at write-back time instead of
                // caching interior cells across the Lisp predicate calls: a
                // predicate that setcdr's the list would leave cached cells
                // unrooted, and a GC during a later comparison frees them —
                // making this write-back a store into swept memory.
                let mut cursor = args[0];
                for value in sorted_values.into_iter() {
                    if !cursor.is_cons() {
                        break;
                    }
                    cursor.set_car(value);
                    cursor = cursor.cons_cdr();
                }
                Ok(args[0])
            } else {
                Ok(Value::list(std::mem::take(&mut sorted_values)))
            }
        }
        ValueKind::Veclike(VecLikeType::Vector)
            if !super::chartable::is_bool_vector(&args[0])
                && !super::chartable::is_char_table(&args[0]) =>
        {
            let values = args[0].as_vector_data().unwrap().clone();
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(args[0]);
            eval.push_specpdl_root(lessp_fn);
            eval.push_specpdl_root(key_fn);
            for value in &values {
                eval.push_specpdl_root(*value);
            }
            let sorted_result = stable_sort_values_with(eval, &values, key_fn, lessp_fn, reverse);
            eval.restore_specpdl_roots(roots);
            let sorted_values = sorted_result?;

            if in_place {
                assert!(args[0].replace_vectorlike_sequence_data(sorted_values));
                Ok(args[0])
            } else {
                Ok(Value::vector(sorted_values))
            }
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("list-or-vector-p"), args[0]],
        )),
    }
}

#[derive(Clone, Copy)]
struct SortItem {
    value: Value,
    key: Value,
}

pub(crate) fn stable_sort_values_with(
    runtime: &mut impl SortRuntime,
    values: &[Value],
    key_fn: Value,
    lessp_fn: Value,
    reverse: bool,
) -> Result<Vec<Value>, Flow> {
    if values.len() < 2 {
        return Ok(values.to_vec());
    }

    let mut items: Vec<SortItem> = values
        .iter()
        .copied()
        .map(|value| SortItem {
            value,
            key: Value::NIL,
        })
        .collect();

    if !key_fn.is_nil() {
        for item in &mut items {
            let key = runtime.call_sort_function1(key_fn, item.value)?;
            runtime.root_sort_value(key);
            item.key = key;
        }
    } else {
        for item in &mut items {
            item.key = item.value;
        }
    }

    if reverse {
        items.reverse();
    }

    gnu_style_sort_items(runtime, &mut items, lessp_fn)?;

    if reverse {
        items.reverse();
    }

    Ok(items.into_iter().map(|item| item.value).collect())
}

#[derive(Clone, Copy)]
struct PendingRun {
    base: usize,
    len: usize,
    power: i32,
}

const GALLOP_WIN_MIN: usize = 7;

fn gnu_style_sort_items(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    lessp_fn: Value,
) -> Result<(), Flow> {
    let len = items.len();
    if len < 2 {
        return Ok(());
    }

    let minrun = merge_compute_minrun(len);
    let mut pending: Vec<PendingRun> = Vec::new();
    let mut min_gallop = GALLOP_WIN_MIN;
    let mut base = 0;
    let mut remaining = len;

    while remaining > 0 {
        let (mut run_len, descending) = count_run(runtime, items, base, len, lessp_fn)?;
        if descending {
            items[base..base + run_len].reverse();
        }
        if run_len < minrun {
            let force = remaining.min(minrun);
            binarysort(runtime, items, base, base + force, base + run_len, lessp_fn)?;
            run_len = force;
        }

        found_new_run(
            runtime,
            items,
            &mut pending,
            run_len,
            len,
            lessp_fn,
            &mut min_gallop,
        )?;
        pending.push(PendingRun {
            base,
            len: run_len,
            power: 0,
        });

        base += run_len;
        remaining -= run_len;
    }

    merge_force_collapse(runtime, items, &mut pending, lessp_fn, &mut min_gallop)
}

fn sort_item_less(
    runtime: &mut impl SortRuntime,
    left: SortItem,
    right: SortItem,
    lessp_fn: Value,
) -> Result<bool, Flow> {
    if lessp_fn.is_nil() {
        return Ok(matches!(
            runtime.compare_sort_keys(&left.key, &right.key)?,
            std::cmp::Ordering::Less
        ));
    }

    Ok(runtime
        .call_sort_function2(lessp_fn, left.key, right.key)?
        .is_truthy())
}

fn binarysort(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    lo: usize,
    hi: usize,
    mut start: usize,
    lessp_fn: Value,
) -> Result<(), Flow> {
    if lo == start {
        start += 1;
    }
    while start < hi {
        let pivot = items[start];
        let mut left = lo;
        let mut right = start;
        while left < right {
            let mid = left + ((right - left) >> 1);
            if sort_item_less(runtime, pivot, items[mid], lessp_fn)? {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        items.copy_within(left..start, left + 1);
        items[left] = pivot;
        start += 1;
    }
    Ok(())
}

fn count_run(
    runtime: &mut impl SortRuntime,
    items: &[SortItem],
    lo: usize,
    hi: usize,
    lessp_fn: Value,
) -> Result<(usize, bool), Flow> {
    debug_assert!(lo < hi);
    if lo + 1 == hi {
        return Ok((1, false));
    }

    let mut run_len = 2;
    if sort_item_less(runtime, items[lo + 1], items[lo], lessp_fn)? {
        while lo + run_len < hi
            && sort_item_less(
                runtime,
                items[lo + run_len],
                items[lo + run_len - 1],
                lessp_fn,
            )?
        {
            run_len += 1;
        }
        Ok((run_len, true))
    } else {
        while lo + run_len < hi
            && !sort_item_less(
                runtime,
                items[lo + run_len],
                items[lo + run_len - 1],
                lessp_fn,
            )?
        {
            run_len += 1;
        }
        Ok((run_len, false))
    }
}

fn merge_compute_minrun(mut n: usize) -> usize {
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

fn powerloop(s1: usize, n1: usize, n2: usize, n: usize) -> i32 {
    debug_assert!(n1 > 0 && n2 > 0);
    debug_assert!(s1 + n1 + n2 <= n);

    let mut a = 2 * s1 + n1;
    let mut b = a + n1 + n2;
    let mut result = 0;
    loop {
        result += 1;
        if a >= n {
            a -= n;
            b -= n;
        } else if b >= n {
            break;
        }
        a <<= 1;
        b <<= 1;
    }
    result
}

fn found_new_run(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    pending: &mut Vec<PendingRun>,
    new_len: usize,
    total_len: usize,
    lessp_fn: Value,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    if pending.is_empty() {
        return Ok(());
    }

    let prev = *pending.last().expect("pending run");
    let power = powerloop(prev.base, prev.len, new_len, total_len);
    while pending.len() > 1 && pending[pending.len() - 2].power > power {
        let index = pending.len() - 2;
        merge_at(runtime, items, pending, index, lessp_fn, min_gallop)?;
    }
    let last = pending.len() - 1;
    pending[last].power = power;
    Ok(())
}

fn merge_force_collapse(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    pending: &mut Vec<PendingRun>,
    lessp_fn: Value,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    while pending.len() > 1 {
        let mut index = pending.len() - 2;
        if index > 0 && pending[index - 1].len < pending[index + 1].len {
            index -= 1;
        }
        merge_at(runtime, items, pending, index, lessp_fn, min_gallop)?;
    }
    Ok(())
}

fn merge_at(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    pending: &mut Vec<PendingRun>,
    index: usize,
    lessp_fn: Value,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let left = pending[index];
    let right = pending[index + 1];
    debug_assert_eq!(left.base + left.len, right.base);

    merge_runs(
        runtime, items, left.base, left.len, right.len, lessp_fn, min_gallop,
    )?;
    pending[index].len = left.len + right.len;
    pending.remove(index + 1);
    Ok(())
}

fn gallop_left(
    runtime: &mut impl SortRuntime,
    key: SortItem,
    items: &[SortItem],
    hint: usize,
    lessp_fn: Value,
) -> Result<usize, Flow> {
    debug_assert!(!items.is_empty());
    debug_assert!(hint < items.len());

    let n = items.len() as isize;
    let hint = hint as isize;
    let mut last_offset = 0isize;
    let mut offset = 1isize;

    if sort_item_less(runtime, items[hint as usize], key, lessp_fn)? {
        let max_offset = n - hint;
        while offset < max_offset {
            if sort_item_less(runtime, items[(hint + offset) as usize], key, lessp_fn)? {
                last_offset = offset;
                offset = (offset << 1) + 1;
            } else {
                break;
            }
        }
        if offset > max_offset {
            offset = max_offset;
        }
        last_offset += hint;
        offset += hint;
    } else {
        let max_offset = hint + 1;
        while offset < max_offset {
            if sort_item_less(runtime, items[(hint - offset) as usize], key, lessp_fn)? {
                break;
            }
            last_offset = offset;
            offset = (offset << 1) + 1;
        }
        if offset > max_offset {
            offset = max_offset;
        }
        let k = last_offset;
        last_offset = hint - offset;
        offset = hint - k;
    }

    last_offset += 1;
    while last_offset < offset {
        let mid = last_offset + ((offset - last_offset) >> 1);
        if sort_item_less(runtime, items[mid as usize], key, lessp_fn)? {
            last_offset = mid + 1;
        } else {
            offset = mid;
        }
    }
    Ok(offset as usize)
}

fn gallop_right(
    runtime: &mut impl SortRuntime,
    key: SortItem,
    items: &[SortItem],
    hint: usize,
    lessp_fn: Value,
) -> Result<usize, Flow> {
    debug_assert!(!items.is_empty());
    debug_assert!(hint < items.len());

    let n = items.len() as isize;
    let hint = hint as isize;
    let mut last_offset = 0isize;
    let mut offset = 1isize;

    if sort_item_less(runtime, key, items[hint as usize], lessp_fn)? {
        let max_offset = hint + 1;
        while offset < max_offset {
            if sort_item_less(runtime, key, items[(hint - offset) as usize], lessp_fn)? {
                last_offset = offset;
                offset = (offset << 1) + 1;
            } else {
                break;
            }
        }
        if offset > max_offset {
            offset = max_offset;
        }
        let k = last_offset;
        last_offset = hint - offset;
        offset = hint - k;
    } else {
        let max_offset = n - hint;
        while offset < max_offset {
            if sort_item_less(runtime, key, items[(hint + offset) as usize], lessp_fn)? {
                break;
            }
            last_offset = offset;
            offset = (offset << 1) + 1;
        }
        if offset > max_offset {
            offset = max_offset;
        }
        last_offset += hint;
        offset += hint;
    }

    last_offset += 1;
    while last_offset < offset {
        let mid = last_offset + ((offset - last_offset) >> 1);
        if sort_item_less(runtime, key, items[mid as usize], lessp_fn)? {
            offset = mid;
        } else {
            last_offset = mid + 1;
        }
    }
    Ok(offset as usize)
}

fn merge_runs(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    base: usize,
    left_len: usize,
    right_len: usize,
    lessp_fn: Value,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let mut left_base = base;
    let mut left_len = left_len;
    let right_base = base + left_len;
    let mut right_len = right_len;

    let skipped = gallop_right(
        runtime,
        items[right_base],
        &items[left_base..left_base + left_len],
        0,
        lessp_fn,
    )?;
    left_base += skipped;
    left_len -= skipped;
    if left_len == 0 {
        return Ok(());
    }

    right_len = gallop_left(
        runtime,
        items[left_base + left_len - 1],
        &items[right_base..right_base + right_len],
        right_len - 1,
        lessp_fn,
    )?;
    if right_len == 0 {
        return Ok(());
    }

    if left_len <= right_len {
        merge_lo(
            runtime, items, left_base, left_len, right_base, right_len, lessp_fn, min_gallop,
        )
    } else {
        merge_hi(
            runtime, items, left_base, left_len, right_base, right_len, lessp_fn, min_gallop,
        )
    }
}

#[allow(clippy::too_many_arguments)] // TimSort merge state follows the reference algorithm directly
fn merge_lo(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    left_base: usize,
    mut left_len: usize,
    right_base: usize,
    mut right_len: usize,
    lessp_fn: Value,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let left = items[left_base..left_base + left_len].to_vec();
    let mut left_index = 0;
    let mut right_index = right_base;
    let mut dest = left_base;

    items[dest] = items[right_index];
    dest += 1;
    right_index += 1;
    right_len -= 1;
    if right_len == 0 {
        items[dest..dest + left_len].copy_from_slice(&left[left_index..left_index + left_len]);
        return Ok(());
    }
    if left_len == 1 {
        if right_len > 0 {
            items.copy_within(right_index..right_index + right_len, dest);
            dest += right_len;
        }
        items[dest] = left[left_index];
        return Ok(());
    }

    let mut threshold = *min_gallop;
    loop {
        let mut acount = 0;
        let mut bcount = 0;

        loop {
            if sort_item_less(runtime, items[right_index], left[left_index], lessp_fn)? {
                items[dest] = items[right_index];
                dest += 1;
                right_index += 1;
                right_len -= 1;
                bcount += 1;
                acount = 0;
                if right_len == 0 {
                    if left_len > 0 {
                        items[dest..dest + left_len]
                            .copy_from_slice(&left[left_index..left_index + left_len]);
                    }
                    return Ok(());
                }
                if bcount >= threshold {
                    break;
                }
            } else {
                items[dest] = left[left_index];
                dest += 1;
                left_index += 1;
                left_len -= 1;
                acount += 1;
                bcount = 0;
                if left_len == 1 {
                    if right_len > 0 {
                        items.copy_within(right_index..right_index + right_len, dest);
                        dest += right_len;
                    }
                    items[dest] = left[left_index];
                    return Ok(());
                }
                if acount >= threshold {
                    break;
                }
            }
        }

        threshold += 1;
        loop {
            if threshold > 1 {
                threshold -= 1;
            }
            *min_gallop = threshold;

            let k = gallop_right(
                runtime,
                items[right_index],
                &left[left_index..left_index + left_len],
                0,
                lessp_fn,
            )?;
            acount = k;
            if k != 0 {
                items[dest..dest + k].copy_from_slice(&left[left_index..left_index + k]);
                dest += k;
                left_index += k;
                left_len -= k;
                if left_len == 1 {
                    if right_len > 0 {
                        items.copy_within(right_index..right_index + right_len, dest);
                        dest += right_len;
                    }
                    items[dest] = left[left_index];
                    return Ok(());
                }
                if left_len == 0 {
                    return Ok(());
                }
            }

            items[dest] = items[right_index];
            dest += 1;
            right_index += 1;
            right_len -= 1;
            if right_len == 0 {
                if left_len > 0 {
                    items[dest..dest + left_len]
                        .copy_from_slice(&left[left_index..left_index + left_len]);
                }
                return Ok(());
            }

            let k = gallop_left(
                runtime,
                left[left_index],
                &items[right_index..right_index + right_len],
                0,
                lessp_fn,
            )?;
            bcount = k;
            if k != 0 {
                items.copy_within(right_index..right_index + k, dest);
                dest += k;
                right_index += k;
                right_len -= k;
                if right_len == 0 {
                    if left_len > 0 {
                        items[dest..dest + left_len]
                            .copy_from_slice(&left[left_index..left_index + left_len]);
                    }
                    return Ok(());
                }
            }

            items[dest] = left[left_index];
            dest += 1;
            left_index += 1;
            left_len -= 1;
            if left_len == 1 {
                if right_len > 0 {
                    items.copy_within(right_index..right_index + right_len, dest);
                    dest += right_len;
                }
                items[dest] = left[left_index];
                return Ok(());
            }

            if acount < GALLOP_WIN_MIN && bcount < GALLOP_WIN_MIN {
                break;
            }
        }

        threshold += 1;
        *min_gallop = threshold;
    }
}

#[allow(clippy::too_many_arguments)] // TimSort merge state follows the reference algorithm directly
fn merge_hi(
    runtime: &mut impl SortRuntime,
    items: &mut [SortItem],
    left_base: usize,
    mut left_len: usize,
    right_base: usize,
    mut right_len: usize,
    lessp_fn: Value,
    min_gallop: &mut usize,
) -> Result<(), Flow> {
    let right = items[right_base..right_base + right_len].to_vec();
    let mut dest = (right_base + right_len - 1) as isize;
    let mut left_index = (left_base + left_len - 1) as isize;
    let mut right_index = (right_len - 1) as isize;

    items[dest as usize] = items[left_index as usize];
    dest -= 1;
    left_index -= 1;
    left_len -= 1;
    if left_len == 0 {
        items[left_base..left_base + right_len].copy_from_slice(&right[..right_len]);
        return Ok(());
    }
    if right_len == 1 {
        let dest_end = dest as usize;
        let dest_start = dest_end + 1 - left_len;
        let src_end = left_index as usize;
        let src_start = src_end + 1 - left_len;
        items.copy_within(src_start..src_start + left_len, dest_start);
        items[dest_start - 1] = right[right_index as usize];
        return Ok(());
    }

    let mut threshold = *min_gallop;
    loop {
        let mut acount = 0;
        let mut bcount = 0;

        loop {
            if sort_item_less(
                runtime,
                right[right_index as usize],
                items[left_index as usize],
                lessp_fn,
            )? {
                items[dest as usize] = items[left_index as usize];
                dest -= 1;
                left_index -= 1;
                left_len -= 1;
                acount += 1;
                bcount = 0;
                if left_len == 0 {
                    if right_len > 0 {
                        let dest_end = dest as usize;
                        let dest_start = dest_end + 1 - right_len;
                        let right_start = right_index as usize + 1 - right_len;
                        items[dest_start..dest_start + right_len]
                            .copy_from_slice(&right[right_start..right_start + right_len]);
                    }
                    return Ok(());
                }
                if acount >= threshold {
                    break;
                }
            } else {
                items[dest as usize] = right[right_index as usize];
                dest -= 1;
                right_index -= 1;
                right_len -= 1;
                bcount += 1;
                acount = 0;
                if right_len == 1 {
                    let dest_end = dest as usize;
                    let dest_start = dest_end + 1 - left_len;
                    let src_end = left_index as usize;
                    let src_start = src_end + 1 - left_len;
                    items.copy_within(src_start..src_start + left_len, dest_start);
                    items[dest_start - 1] = right[right_index as usize];
                    return Ok(());
                }
                if bcount >= threshold {
                    break;
                }
            }
        }

        threshold += 1;
        loop {
            if threshold > 1 {
                threshold -= 1;
            }
            *min_gallop = threshold;

            let k = left_len
                - gallop_right(
                    runtime,
                    right[right_index as usize],
                    &items[left_base..left_base + left_len],
                    left_len - 1,
                    lessp_fn,
                )?;
            acount = k;
            if k != 0 {
                let dest_start = dest as usize + 1 - k;
                let src_start = left_index as usize + 1 - k;
                items.copy_within(src_start..src_start + k, dest_start);
                dest -= k as isize;
                left_index -= k as isize;
                left_len -= k;
                if left_len == 0 {
                    if right_len > 0 {
                        let dest_end = dest as usize;
                        let dest_start = dest_end + 1 - right_len;
                        let right_start = right_index as usize + 1 - right_len;
                        items[dest_start..dest_start + right_len]
                            .copy_from_slice(&right[right_start..right_start + right_len]);
                    }
                    return Ok(());
                }
            }

            items[dest as usize] = right[right_index as usize];
            dest -= 1;
            right_index -= 1;
            right_len -= 1;
            if right_len == 1 {
                let dest_end = dest as usize;
                let dest_start = dest_end + 1 - left_len;
                let src_end = left_index as usize;
                let src_start = src_end + 1 - left_len;
                items.copy_within(src_start..src_start + left_len, dest_start);
                items[dest_start - 1] = right[right_index as usize];
                return Ok(());
            }

            let k = right_len
                - gallop_left(
                    runtime,
                    items[left_index as usize],
                    &right[..right_len],
                    right_len - 1,
                    lessp_fn,
                )?;
            bcount = k;
            if k != 0 {
                let dest_start = dest as usize + 1 - k;
                let right_start = right_index as usize + 1 - k;
                items[dest_start..dest_start + k]
                    .copy_from_slice(&right[right_start..right_start + k]);
                dest -= k as isize;
                right_index -= k as isize;
                right_len -= k;
                if right_len == 1 {
                    let dest_end = dest as usize;
                    let dest_start = dest_end + 1 - left_len;
                    let src_end = left_index as usize;
                    let src_start = src_end + 1 - left_len;
                    items.copy_within(src_start..src_start + left_len, dest_start);
                    items[dest_start - 1] = right[right_index as usize];
                    return Ok(());
                }
                if right_len == 0 {
                    return Ok(());
                }
            }

            items[dest as usize] = items[left_index as usize];
            dest -= 1;
            left_index -= 1;
            left_len -= 1;
            if left_len == 0 {
                if right_len > 0 {
                    let dest_end = dest as usize;
                    let dest_start = dest_end + 1 - right_len;
                    let right_start = right_index as usize + 1 - right_len;
                    items[dest_start..dest_start + right_len]
                        .copy_from_slice(&right[right_start..right_start + right_len]);
                }
                return Ok(());
            }

            if acount < GALLOP_WIN_MIN && bcount < GALLOP_WIN_MIN {
                break;
            }
        }

        threshold += 1;
        *min_gallop = threshold;
    }
}
