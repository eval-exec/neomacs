//! Buffer-facing Lisp entries that stay in the core command layer.
//!
//! These bodies bind frame/window display state or raw specpdl (R0), so the
//! editor layer stays free of those policies; the buffer-text machinery they
//! drive stays in `emacs_core::buffer`.

use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use std::time::{Duration, Instant};

use crate::buffer::{
    BufferId, CharLen, CharPos0, CharRange, EmacsBytePos, EmacsByteRange, LispCharPos1, TextChange,
    TextPositionAnchor,
};
use crate::emacs_core::buffer::{
    BufferMultibyteConversionMode, buffer_insert_lisp_string_from_lisp_string,
    buffer_insert_piece_from_string, buffer_slice_value_range, build_multibyte_conversion_byte_map,
    builtin_narrow_to_region, convert_buffer_string_for_multibyte, current_buffer_multibyte,
    decode_chars_with_byte_offsets, eq_fixnum_zero, expect_integer_or_marker_in_buffers,
    is_hidden_buffer, lisp_string_byte_to_char, lisp_string_from_buffer_bytes,
    other_buffer_designator, remap_old_byte_to_new_boundary, remap_text_property_table,
    replace_diff, replace_region_max_costs, replace_region_source_value_in_state,
    resolve_buffer_designator_allow_nil_current,
};
use crate::emacs_core::error::{
    EvalResult, LispCondition, expect_args, expect_args_range, expect_max_args, signal,
};
use crate::emacs_core::value::{Value, ValueKind, VecLikeType};
use neovm_edit::emacs_core::edit_host::EditHost;

pub(crate) fn builtin_replace_region_contents(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("replace-region-contents", &args, 3, 6)?;
    let current_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let start = expect_integer_or_marker_in_buffers(&eval.edit.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.edit.buffers, &args[1])?;

    // GNU editfns.c `Freplace_region_contents`: if SOURCE is a function, call
    // it with no arguments and the current buffer narrowed to BEG..END, and use
    // its return value (a buffer or string) as the actual source.  This is
    // wrapped in `save-excursion` + `save-restriction` so the narrowing and
    // point are restored afterwards.  (This SOURCE form is deprecated in GNU.)
    let mut source = args[2];
    if crate::emacs_core::builtins::types::builtin_functionp_1(eval, source)?.is_truthy() {
        let count = eval.specpdl().len();
        eval.record_save_excursion();
        if let Some(state) = eval.edit.buffers.save_current_restriction_state() {
            // SAFETY: this is the owner thread. The binding is pushed inside the
            // `save-excursion` form this code emits below, so the matching
            // unwinding pops exactly this entry; no early return escapes the
            // form, and the Vec is never replaced or moved.
            unsafe {
                eval.specpdl_mut()
                    .push(crate::emacs_core::eval::SpecBinding::save_restriction(
                        state,
                    ));
            }
        }
        let narrow_result =
            builtin_narrow_to_region(eval, vec![Value::fixnum(start), Value::fixnum(end)])
                .and_then(|_| eval.funcall_general(source, Vec::<Value>::new()));
        let narrow_result = eval.unbind_to_with_result(count, narrow_result);
        source = narrow_result?;
    }

    let source_value =
        replace_region_source_value_in_state(&eval.edit.buffers, &source, current_id)?;
    // When SOURCE is a buffer, source_value is a FRESH substring held only
    // in this Rust local while modification hooks run arbitrary Lisp during
    // the replacement below; root it for the rest of the function. Normal
    // exits restore explicitly; error unwinds pop the GcRoot with the frame.
    let source_root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(source_value);

    let read_only_buffer_name = eval.edit.buffers.current_buffer().and_then(|buf| {
        if crate::emacs_core::editfns::buffer_read_only_active_in_state(eval.variables(), &[], buf)
        {
            Some(buf.name_value())
        } else {
            None
        }
    });
    if let Some(name) = read_only_buffer_name {
        return Err(signal(LispCondition::BufferReadOnly, vec![name]));
    }

    let byte_range = {
        let buf = eval
            .edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let start_byte = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(start));
        let end_byte = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(end));

        if start_byte <= end_byte {
            EmacsByteRange::new(start_byte, end_byte)
        } else {
            EmacsByteRange::new(end_byte, start_byte)
        }
    };
    let old_range = crate::emacs_core::editfns::buffer_edit_range_for_byte_range_in_manager(
        &eval.edit.buffers,
        current_id,
        byte_range,
    )?;
    let target_multibyte = current_buffer_multibyte(&eval.edit.buffers)?;
    // GNU 31's Freplace_region_contents currently passes its Lisp INHERIT
    // argument to replace_range's adjust-match-data parameter and hard-codes
    // the distinct property-inheritance parameter to false (editfns.c).
    // Neomacs does not yet expose that internal match-data switch, but it must
    // keep the two semantic axes separate: replacement SOURCE properties are
    // copied, while adjoining destination properties are not inherited.
    let _adjust_match_data = args.get(5).is_some_and(|value| value.is_truthy());

    // GNU `Freplace_region_contents` falls back to a plain `delete-region` +
    // `insert` when MAX-SECS or MAX-COSTS is exactly 0 (the comparison step is
    // disabled).  Detect that here so callers retain the escape hatch.
    let comparison_disabled = matches!(args.get(3), Some(v) if eq_fixnum_zero(v))
        || matches!(args.get(4), Some(v) if eq_fixnum_zero(v));

    // Decode the destination region (A) and the source (B) into codepoint
    // sequences so we can compute a minimal edit.  The source is first
    // converted into the destination buffer's representation so both sides use
    // the same encoding.
    let region_bytes = eval
        .edit
        .buffers
        .get(current_id)
        .map(|buf| buf.buffer_substring_bytes_range(byte_range))
        .unwrap_or_default();
    let source_string = source_value
        .as_lisp_string()
        .map(|ls| buffer_insert_lisp_string_from_lisp_string(ls, target_multibyte))
        .unwrap_or_else(|| lisp_string_from_buffer_bytes(Vec::new(), target_multibyte));

    let a_decoded = decode_chars_with_byte_offsets(&region_bytes);
    let b_decoded = decode_chars_with_byte_offsets(source_string.as_bytes());
    // Strip the trailing sentinel for the codepoint comparison.
    let a_codes: Vec<u32> = a_decoded[..a_decoded.len() - 1]
        .iter()
        .map(|&(c, _)| c)
        .collect();
    let b_codes: Vec<u32> = b_decoded[..b_decoded.len() - 1]
        .iter()
        .map(|&(c, _)| c)
        .collect();

    let trivial = comparison_disabled || a_codes.is_empty() || b_codes.is_empty();
    let runs = if trivial {
        None
    } else {
        let max_costs = replace_region_max_costs(args.get(4))?;
        // GNU accepts all Lisp time representations, not just float seconds.
        // An integer zero takes the trivial path above; an expired nonzero
        // or float-zero budget takes the timeout path and returns nil.
        let deadline = if let Some(max_secs) = args.get(3).filter(|value| !value.is_nil()) {
            let (secs, nanos) = crate::emacs_core::timefns::time_value_seconds_and_nanos(max_secs)?;
            let now = Instant::now();
            if secs < 0 {
                Some(now)
            } else {
                now.checked_add(Duration::new(secs as u64, nanos as u32))
            }
        } else {
            None
        };
        replace_diff::change_runs(&a_codes, &b_codes, deadline, max_costs)
    };

    // Timeout must use the same whole-region replacement as GNU, rather
    // than applying a partial diff.  Preserve source properties and run the
    // normal modification hooks in both this and the trivial empty-side path.
    if runs.is_none() {
        let replacement = buffer_insert_piece_from_string(source_value, target_multibyte)?
            .into_replacement_text();
        // GNU `replace_range (min_a, min_a + size_a, source, true, ...)`
        // (editfns.c:2044): the range is re-measured after the callbacks.
        let pending = crate::emacs_core::editfns::PendingTextEdit::new(
            old_range,
            crate::emacs_core::editfns::RemeasureRule::ReplaceRange,
        );
        if let Some(prepared) = pending.prepare(eval)? {
            let new_extent = prepared
                .lease(&mut eval.edit.buffers)
                .and_then(|lease| lease.replace(&replacement))
                .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
            crate::emacs_core::editfns::signal_after_text_change(
                eval,
                TextChange::new(prepared.range(), new_extent),
            )?;
        }
        eval.restore_specpdl_roots(source_root_scope);
        return Ok(if trivial { Value::T } else { Value::NIL });
    }

    let runs = runs.expect("completed comparison");

    // GNU `Freplace_region_contents` (editfns.c) calls `Fundo_boundary` once
    // compareseq has succeeded, before walking the change runs — including
    // when there is nothing to change.  Besides the boundary itself this sets
    // `point_before_last_command_or_undo`, which the per-run `record_point`
    // then conses ahead of the first change record.  The trivial
    // empty-side/comparison-disabled paths above return before reaching it,
    // matching GNU.
    // GNU's `Freplace_buffer_contents' calls `Fundo_boundary' (src/editfns.c:2139),
    // so it sets `undo-auto--last-boundary-cause' by exactly the same route.
    if eval
        .edit
        .buffers
        .add_undo_boundary(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?
        == crate::buffer::buffer::UndoBoundaryOutcome::Recorded
    {
        crate::emacs_core::undo::set_last_boundary_cause_explicit(eval)?;
    }

    if runs.is_empty() {
        // Buffers already identical within the region: nothing to do, and no
        // markers/point are disturbed.  GNU returns t in this case.
        eval.restore_specpdl_roots(source_root_scope);
        return Ok(Value::T);
    }

    // Announce a single modification spanning the whole region, exactly like
    // GNU which calls `prepare_to_modify_buffer` once and binds
    // `inhibit-modification-hooks` while issuing the per-run replacements.
    let new_extent = crate::emacs_core::editfns::lisp_string_text_extent(&source_string);
    let change = TextChange::new(old_range, new_extent);
    crate::emacs_core::editfns::signal_before_text_change(eval, change)?;

    // GNU computes every run as `min_a + i` in characters before the
    // callbacks ran, and `replace_range` clamps each one to BEGV..ZV of the
    // live buffer (editfns.c:2165-2183, insdel.c:1509-1513). Measuring the
    // runs in the live text keeps a callback's edit from leaving them past
    // the text or inside a multibyte sequence.
    let region_start = old_range.char_start();
    // Apply the change runs back-to-front so that earlier positions stay
    // valid as we edit (mirrors GNU walking the change lists backwards).
    for run in runs.iter().rev() {
        let Some((target, del_range)) = eval.edit.buffers.current_buffer().map(|buf| {
            let begv = buf.point_min_char_pos();
            let zv = buf.point_max_char_pos();
            let total = buf.total_char_end_pos();
            let from = region_start
                .add_len(CharLen::new(run.a_start()))
                .max(begv)
                .min(total);
            let to = region_start
                .add_len(CharLen::new(run.a_end()))
                .min(zv)
                .clamp(from, total.max(from));
            (
                buf.id(),
                buf.edit_range_for_char_range(CharRange::new(from, to)),
            )
        }) else {
            break;
        };

        // Replacement text for this run: characters [b_start, b_end) of the
        // source, sliced from the original SOURCE value so text properties are
        // preserved.
        let replacement = if run.b_start() == run.b_end() {
            Value::string("")
        } else {
            crate::strings_core::substring_impl(
                "substring",
                &[
                    source_value,
                    Value::fixnum(run.b_start() as i64),
                    Value::fixnum(run.b_end() as i64),
                ],
                true,
            )?
        };
        let replacement =
            buffer_insert_piece_from_string(replacement, target_multibyte)?.into_replacement_text();
        eval.edit
            .buffers
            .replace_buffer_measured_region_lisp_string(target, del_range, &replacement)
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    }

    // GNU `signal_after_change (min_a, size_a, size_b)` (editfns.c:2189).
    crate::emacs_core::editfns::signal_after_change_chars(
        eval,
        region_start,
        old_range.char_len(),
        new_extent.chars(),
    )?;

    eval.restore_specpdl_roots(source_root_scope);
    Ok(Value::T)
}

/// `(replace-buffer-contents SOURCE &optional MAX-SECS MAX-COSTS)` -> t
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_replace_buffer_contents(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("replace-buffer-contents", &args, 1, 3)?;
    // GNU 31's `replace-buffer-contents` (subr.el) is a thin wrapper:
    //   (replace-region-contents (point-min) (point-max) (get-buffer source)
    //                            max-secs max-costs)
    // Delegating keeps the minimal (Myers-diff) non-destructive replacement,
    // so markers, point, properties and overlays in the accessible portion
    // outside the changed runs are preserved exactly as in GNU.
    let source_id = resolve_buffer_designator_allow_nil_current(&mut eval.edit.buffers, &args[0])?;
    let source_buffer = match source_id {
        Some(id) => Value::make_buffer(id),
        None => Value::NIL,
    };

    let (point_min, point_max) = {
        let buf = eval
            .edit
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            buf.point_min_lisp_char_pos().as_i64(),
            buf.point_max_lisp_char_pos().as_i64(),
        )
    };

    let mut region_args = vec![
        Value::fixnum(point_min),
        Value::fixnum(point_max),
        source_buffer,
    ];
    // Forward MAX-SECS / MAX-COSTS positionally so the optional behavior
    // (immediate delete/insert fallback) matches GNU.
    if let Some(max_secs) = args.get(1) {
        region_args.push(*max_secs);
        if let Some(max_costs) = args.get(2) {
            region_args.push(*max_costs);
        }
    }

    builtin_replace_region_contents(eval, region_args)
}

pub(crate) fn builtin_set_buffer_multibyte(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-multibyte", &args, 1)?;
    let flag = args[0];
    let target_multibyte = !flag.is_nil();
    let current_id = eval
        .edit
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let (already_multibyte, narrowed, base_buffer, shared_ids) = {
        let current = eval
            .edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (
            current.get_multibyte(),
            current.is_narrowed(),
            current.base_buffer(),
            eval.edit.buffers.shared_text_buffer_ids(current_id),
        )
    };
    let old_undo_list = eval
        .edit
        .buffers
        .get(current_id)
        .map(|buffer| buffer.get_undo_list())
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    if base_buffer.is_some() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Cannot do `set-buffer-multibyte' on an indirect buffer",
            )],
        ));
    }

    if already_multibyte == target_multibyte {
        return Ok(flag);
    }

    if narrowed {
        return Err(signal(
            "error",
            vec![Value::string("Changing multibyteness in a narrowed buffer")],
        ));
    }

    #[derive(Clone, Copy)]
    struct OverlaySnapshot {
        overlay: Value,
        start_old_emacs_byte: EmacsBytePos,
        end_old_emacs_byte: EmacsBytePos,
        old_begin: CharPos0,
    }

    struct BufferSnapshot {
        id: BufferId,
        pt_old_emacs_byte: EmacsBytePos,
        begv_old_emacs_byte: EmacsBytePos,
        zv_old_emacs_byte: EmacsBytePos,
        mark_old_emacs_byte: Option<EmacsBytePos>,
        last_window_start_old_emacs_byte: EmacsBytePos,
        overlays: Vec<OverlaySnapshot>,
    }

    let snapshots = {
        let mut snapshots = Vec::with_capacity(shared_ids.len());
        for id in &shared_ids {
            let buffer = eval
                .edit
                .buffers
                .get(*id)
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
            let overlays = buffer
                .overlays()
                .dump_overlays()
                .into_iter()
                .filter_map(|overlay| {
                    let data = overlay.as_overlay_data()?;
                    let (start, end) = data.current_range();
                    let total_end = buffer.total_emacs_byte_end_pos();
                    Some(OverlaySnapshot {
                        overlay,
                        start_old_emacs_byte: EmacsBytePos::new(start).min(total_end),
                        end_old_emacs_byte: EmacsBytePos::new(end).min(total_end),
                        old_begin: buffer.emacs_byte_pos_to_char_pos_clamped(
                            EmacsBytePos::new(start).min(total_end),
                        ),
                    })
                })
                .collect();
            let last_window_start = buffer.last_window_start();
            let total_end = buffer.total_emacs_byte_end_pos();
            snapshots.push(BufferSnapshot {
                id: *id,
                pt_old_emacs_byte: buffer.point_emacs_byte_pos().min(total_end),
                begv_old_emacs_byte: buffer.point_min_emacs_byte_pos().min(total_end),
                zv_old_emacs_byte: buffer.point_max_emacs_byte_pos().min(total_end),
                mark_old_emacs_byte: buffer.mark_emacs_byte_pos().map(|mark| mark.min(total_end)),
                last_window_start_old_emacs_byte: buffer
                    .lisp_pos_to_full_buffer_emacs_byte_pos(last_window_start),
                overlays,
            });
        }
        snapshots
    };

    let source_value = {
        let buffer = eval
            .edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buffer_slice_value_range(buffer, buffer.full_emacs_byte_range())
    };
    let old_bytes: Vec<u8> = source_value
        .as_lisp_string()
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();
    let old_byte_len = old_bytes.len();
    let (converted_value, mode) = convert_buffer_string_for_multibyte(source_value, flag)?;
    let piece = buffer_insert_piece_from_string(converted_value, target_multibyte)?;
    let new_storage = piece.into_text();
    let new_total_bytes = new_storage.sbytes();
    let conversion_byte_map = build_multibyte_conversion_byte_map(&old_bytes, mode);

    let new_props = {
        let buffer = eval
            .edit
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let old_props = buffer.text_props_snapshot();
        match mode {
            BufferMultibyteConversionMode::ToMultibyte => old_props,
            BufferMultibyteConversionMode::AsUnibyte
            | BufferMultibyteConversionMode::AsMultibyte => {
                remap_text_property_table(&old_props, |char_pos| {
                    let logical_byte = buffer
                        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
                        .get();
                    let boundary = remap_old_byte_to_new_boundary(
                        &conversion_byte_map,
                        old_byte_len,
                        &new_storage,
                        new_total_bytes,
                        logical_byte,
                    );
                    lisp_string_byte_to_char(&new_storage, boundary)
                })
            }
        }
    };

    // Walk the intrusive marker chain and remap each logical byte position
    // through the same boundary arithmetic used for point, narrowing and
    // overlays.
    {
        let buffer = eval
            .edit
            .buffers
            .get_mut(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buffer.remap_text_marker_anchors(|old_position| {
            let old_byte = old_position.emacs_byte_pos().get();
            let boundary = remap_old_byte_to_new_boundary(
                &conversion_byte_map,
                old_byte_len,
                &new_storage,
                new_total_bytes,
                old_byte,
            );
            let new_char = lisp_string_byte_to_char(&new_storage, boundary);
            TextPositionAnchor::new(CharPos0::new(new_char), EmacsBytePos::new(boundary))
        });
        buffer.replace_lisp_string_with_text_props(&new_storage, new_props);
    }

    // Set the multibyte flag BEFORE remapping positions. GNU does
    // `bset_enable_multibyte_characters` first ("so that chars_in_text asks the
    // right question"): the buffer's byte<->char conversion used by point /
    // narrowing / marker remapping must see the new multibyteness, otherwise it
    // counts every byte as a character.
    for id in &shared_ids {
        eval.edit
            .buffers
            .set_buffer_multibyte_flag(*id, target_multibyte)
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
    }

    for snapshot in snapshots {
        let map_boundary = |logical_byte: usize| {
            remap_old_byte_to_new_boundary(
                &conversion_byte_map,
                old_byte_len,
                &new_storage,
                new_total_bytes,
                logical_byte,
            )
        };

        let pt_byte = map_boundary(snapshot.pt_old_emacs_byte.get());
        let begv_byte = map_boundary(snapshot.begv_old_emacs_byte.get());
        let zv_byte = map_boundary(snapshot.zv_old_emacs_byte.get());
        let mark_byte = snapshot
            .mark_old_emacs_byte
            .map(|old_byte| map_boundary(old_byte.get()));
        let last_window_start_byte = map_boundary(snapshot.last_window_start_old_emacs_byte.get());

        eval.edit
            .buffers
            .restore_buffer_emacs_byte_restriction(
                snapshot.id,
                EmacsByteRange::new(EmacsBytePos::new(begv_byte), EmacsBytePos::new(zv_byte)),
            )
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        eval.edit
            .buffers
            .goto_buffer_emacs_byte_pos(snapshot.id, EmacsBytePos::new(pt_byte))
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;

        if let Some(mark_byte) = mark_byte {
            eval.edit
                .buffers
                .set_buffer_mark_emacs_byte_pos(snapshot.id, EmacsBytePos::new(mark_byte))
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        } else {
            eval.edit
                .buffers
                .clear_buffer_mark(snapshot.id)
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        }

        eval.edit
            .buffers
            .set_buffer_last_window_start(
                snapshot.id,
                LispCharPos1::from_one_based_usize(
                    lisp_string_byte_to_char(&new_storage, last_window_start_byte) + 1,
                ),
            )
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;

        // GNU buffer.c:1044 visits an ascending snapshot, then itree.c:750
        // compares numeric begins while nodes change coordinate spaces. Build
        // the complete remap before publication, rather than moving by bytes.
        let remap_buffer = eval
            .edit
            .buffers
            .get(snapshot.id)
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        let remaps: Vec<_> = snapshot
            .overlays
            .into_iter()
            .map(|overlay| {
                let start_byte = map_boundary(overlay.start_old_emacs_byte.get());
                let end_byte = map_boundary(overlay.end_old_emacs_byte.get());
                crate::buffer::overlay::OverlayPositionRemap {
                    overlay: overlay.overlay,
                    old_range: EmacsByteRange::new(
                        overlay.start_old_emacs_byte,
                        overlay.end_old_emacs_byte,
                    ),
                    range: EmacsByteRange::new(
                        EmacsBytePos::new(start_byte),
                        EmacsBytePos::new(end_byte),
                    ),
                    old_begin: overlay.old_begin,
                    // Replaced buffer text owns indexed byte/character lookup;
                    // rescanning a Lisp string prefix per overlay is quadratic.
                    new_begin: remap_buffer
                        .emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(start_byte)),
                }
            })
            .collect();
        eval.edit
            .buffers
            .get_mut(snapshot.id)
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?
            .overlays_mut()
            .remap_overlay_positions(&remaps);
        for _ in &remaps {
            eval.edit
                .buffers
                .note_overlay_modification(snapshot.id)
                .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        }

        eval.edit
            .buffers
            .set_buffer_multibyte_flag(snapshot.id, target_multibyte)
            .ok_or_else(|| signal("error", vec![Value::string("Missing shared buffer")]))?;
        // The remap above moved every marker in the chain, the windows'
        // start/point markers included, without a change signal: refresh
        // the windows' cached positions from them here, as an edit would.
        // GNU reads `w->start` through the marker (`Fwindow_start`), so
        // `(window-start)` right after this call already answers with the
        // remapped position.
        eval.sync_window_positions(snapshot.id);
    }

    // GNU buffer.c:2985-2989: a real displayed-buffer conversion raises
    // windows_or_buffers_changed ALL after remapping text and overlays.
    if crate::emacs_core::redisplay_policy::gnu_redisplay_hooks_enabled()
        && eval
            .frame_manager()
            .buffer_window_count(&eval.edit.buffers, current_id)
            > 0
    {
        eval.gnu_mark_windows_all();
        eval.request_menu_bar_rebuild(
            crate::emacs_core::eval::MenuBarRebuildReason::WindowsOrBuffersChanged,
        );
    }

    if !old_undo_list.is_t() {
        let restore_flag = if flag.is_nil() { Value::T } else { Value::NIL };
        let undo_entry = Value::list(vec![
            Value::symbol("apply"),
            Value::symbol("set-buffer-multibyte"),
            restore_flag,
        ]);
        let _ = eval
            .edit
            .buffers
            .configure_buffer_undo_list(current_id, Value::cons(undo_entry, old_undo_list));
    }
    Ok(flag)
}

pub(crate) fn builtin_buffer_list(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("buffer-list", &args, 1)?;
    let ids = args
        .first()
        .and_then(|value| match value.kind() {
            ValueKind::Veclike(VecLikeType::Frame) => {
                let fid = crate::window::FrameId(value.as_frame_id().unwrap());
                let frame = eval.frame_manager().get(fid)?;

                // Global buffer list (Vbuffer_alist equivalent).
                let mut general = eval.edit.buffers.buffer_list();

                // Frame-specific lists (GNU buffer.c:438-460).
                // f->buffer_list: most-recently-shown first.
                let framelist: Vec<BufferId> = frame.buffer_list.clone();
                // f->buried_buffer_list: most-recently-buried first;
                // GNU reverses it so most-recently-buried comes last.
                let mut prevlist: Vec<BufferId> = frame.buried_buffer_list.clone();
                prevlist.reverse();

                // Remove duplicates from general.
                general.retain(|bid| !framelist.contains(bid) && !prevlist.contains(bid));

                // GNU buffer.c:457: CALLN(Fnconc, framelist, general, prevlist).
                // Framelist first, then remaining (deduped) general, then
                // reversed buried list last so buried buffers are at the
                // absolute end.
                let mut ids = Vec::with_capacity(framelist.len() + prevlist.len() + general.len());
                ids.extend(framelist);
                ids.extend(general);
                ids.extend(prevlist);
                Some(ids)
            }
            _ => None,
        })
        .unwrap_or_else(|| eval.edit.buffers.buffer_list());
    let vals: Vec<Value> = ids.into_iter().map(Value::make_buffer).collect();
    Ok(Value::list(vals))
}

pub(crate) fn builtin_other_buffer(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    other_buffer_impl_in_state(frames, buffers, args)
}

fn buffer_visible_in_visible_frame(
    frames: &crate::window::FrameManager,
    buffer_id: crate::buffer::BufferId,
) -> bool {
    frames.frame_list().into_iter().any(|fid| {
        let Some(frame) = frames.get(fid) else {
            return false;
        };
        frame.visibility.is_visible()
            && frame.window_list().into_iter().any(|wid| {
                frame
                    .find_window(wid)
                    .and_then(crate::window::Window::buffer_id)
                    == Some(buffer_id)
            })
    })
}

fn other_buffer_candidate(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    buffer_id: crate::buffer::BufferId,
    avoid_id: Option<crate::buffer::BufferId>,
    visible_ok: bool,
    notsogood: &mut Option<crate::buffer::BufferId>,
) -> Option<crate::buffer::BufferId> {
    if Some(buffer_id) == avoid_id || is_hidden_buffer(buffers, buffer_id) {
        return None;
    }
    if visible_ok || !buffer_visible_in_visible_frame(frames, buffer_id) {
        Some(buffer_id)
    } else {
        if notsogood.is_none() {
            *notsogood = Some(buffer_id);
        }
        None
    }
}

pub(crate) fn other_buffer_impl_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("other-buffer", &args, 3)?;

    let frame_id = crate::emacs_core::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.get(2),
        crate::emacs_core::window_cmds::FrameDomain::Live,
    )?;
    let frame_buffer_list = frames
        .get(frame_id)
        .map(|frame| frame.buffer_list.clone())
        .unwrap_or_default();
    let avoid_id = other_buffer_designator(buffers, args.first());
    let visible_ok = args.get(1).is_some_and(|arg| !arg.is_nil());
    let mut notsogood = None;

    for id in frame_buffer_list {
        if let Some(candidate) =
            other_buffer_candidate(frames, buffers, id, avoid_id, visible_ok, &mut notsogood)
        {
            return Ok(Value::make_buffer(candidate));
        }
    }

    for id in buffers.buffer_list() {
        if let Some(candidate) =
            other_buffer_candidate(frames, buffers, id, avoid_id, visible_ok, &mut notsogood)
        {
            return Ok(Value::make_buffer(candidate));
        }
    }

    if let Some(id) = notsogood {
        return Ok(Value::make_buffer(id));
    }

    let scratch = buffers
        .find_buffer_by_name("*scratch*")
        .unwrap_or_else(|| buffers.create_buffer("*scratch*"));
    Ok(Value::make_buffer(scratch))
}

/// GNU `(other-buffer BUFFER)` for a dying buffer, behind the one cold PC2
/// table the evaluator's green buffer-kill notification uses. The frame and
/// window authority stays here; the killed buffer arrives as the exact Lisp
/// object `kill-buffer` received.
fn other_buffer_for_killed_buffer(
    ctx: &mut crate::emacs_core::eval::Context,
    _killed: crate::buffer::BufferId,
    killed_value: Value,
) -> Result<Option<crate::buffer::BufferId>, crate::emacs_core::error::Flow> {
    let (frames, buffers) = ctx.frame_manager_and_buffers_mut();
    let other = other_buffer_impl_in_state(frames, buffers, vec![killed_value])?;
    Ok(other.as_buffer_id())
}

static CANONICAL_BUFFER_KILL_HOOKS: crate::emacs_core::eval::buffer_kill::BufferKillHooks =
    crate::emacs_core::eval::buffer_kill::BufferKillHooks::new(other_buffer_for_killed_buffer);

/// The one canonical green buffer-kill descriptor (published only by the one
/// core boot entry, peer follow-up F2).
pub(crate) fn canonical_buffer_kill_hooks()
-> &'static crate::emacs_core::eval::buffer_kill::BufferKillHooks {
    &CANONICAL_BUFFER_KILL_HOOKS
}
