//! Unchanged-input capture costs, not complete redisplay or GPU frame times.
//! Run: cargo bench -p neovm-core --bench layout_input_capture
use criterion::{Criterion, criterion_group, criterion_main};
use neovm_core::emacs_core::eval_hook_installers;
use neovm_core::window::DisplayFrameManagerExt;
use neovm_core::window::display::WindowDisplayContextExt;
use std::{hint::black_box, time::Duration};

fn capture_benchmarks(c: &mut Criterion) {
    let cases = [
        ("empty", "nil".to_owned()),
        (
            "text_16",
            "(setq line-prefix (make-string 16 32))".to_owned(),
        ),
        (
            "text_4096",
            "(setq line-prefix (make-string 4096 32))".to_owned(),
        ),
        (
            "faces_128",
            r##"(progn (setq line-prefix (make-string 128 32) i 0)
            (while (< i 128)
              (put-text-property i (+ i 1) 'face (list :height (+ 100 i)) line-prefix)
              (setq i (+ i 1))))"##
                .to_owned(),
        ),
        (
            "arithmetic_64",
            r##"(progn (setq expr '(1) i 0)
            (while (< i 64) (setq expr (list '+ expr 1) i (+ i 1)))
            (setq line-prefix (list 'space :width expr)))"##
                .to_owned(),
        ),
        ("image_4k", image_setup(4096)),
        ("image_1m", image_setup(1024 * 1024)),
        ("image_shared_8", shared_image_setup(8)),
        ("image_shared_12", shared_image_setup(12)),
    ];
    let mut group = c.benchmark_group("layout_input_capture");
    group.sample_size(20);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    for (name, setup) in cases {
        let mut eval = eval_hook_installers::new_context();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .insert("hello\n");
        let frame = eval
            .frame_manager_mut()
            .create_frame("capture-bench", 800, 600, buffer);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.eval_str(&setup).unwrap();
        let prefix = eval.layout_prefix_inputs(buffer).unwrap();
        let freshness = eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap();
        // Lisp setup is outside timing. Retain the previous owned snapshot,
        // recapture unchanged inputs, compare, then drop the new snapshot.
        group.bench_function(format!("prefix/{name}"), |b| {
            b.iter(|| {
                let current = eval.layout_prefix_inputs(black_box(buffer)).unwrap();
                black_box(current == prefix)
            })
        });
        group.bench_function(format!("freshness/{name}"), |b| {
            b.iter(|| {
                let current = eval
                    .window_layout_attempt_freshness(
                        black_box(frame),
                        black_box(window),
                        black_box(buffer),
                    )
                    .unwrap();
                black_box(current == freshness)
            })
        });
    }
    group.finish();
}

fn image_setup(bytes: usize) -> String {
    format!(
        r##"(progn (setq line-prefix (copy-sequence " "))
      (put-text-property 0 1 'display
        (list 'image :type 'png :data (make-string {bytes} 0)) line-prefix))"##
    )
}

fn shared_image_setup(depth: usize) -> String {
    format!(
        r##"(progn (setq metadata (vector "x") i 0 line-prefix (copy-sequence " "))
          (while (< i {depth}) (setq metadata (vector metadata metadata) i (+ i 1)))
          (put-text-property 0 1 'display
            (list 'image :file "unused.png" :metadata metadata) line-prefix))"##
    )
}

criterion_group!(benches, capture_benchmarks);
criterion_main!(benches);
