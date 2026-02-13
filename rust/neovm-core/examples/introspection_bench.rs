use neovm_core::elisp::{parse_forms, Evaluator};
use std::time::Instant;

const INTROSPECTION_FORMS: &[&str] = &[
    "(fboundp 'car)",
    "(fboundp 'when)",
    "(symbol-function 'car)",
    "(symbol-function 'when)",
    "(indirect-function 'car)",
    "(functionp 'car)",
    "(macrop 'when)",
    "(special-form-p 'if)",
    "(func-arity 'car)",
    "(condition-case err (funcall nil) (void-function (car err)))",
];

#[derive(Clone, Copy, Debug)]
struct BenchOptions {
    iterations: usize,
    per_form: bool,
}

fn parse_iterations(raw: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|e| format!("invalid iterations '{}': {}", raw, e))
        .and_then(|n| {
            if n == 0 {
                Err("iterations must be > 0".to_string())
            } else {
                Ok(n)
            }
        })
}

fn parse_args(args: &[String]) -> Result<BenchOptions, String> {
    let mut iterations = 100_000usize;
    let mut per_form = false;

    for arg in args {
        if arg == "--per-form" {
            per_form = true;
            continue;
        }
        iterations = parse_iterations(arg)?;
    }

    Ok(BenchOptions {
        iterations,
        per_form,
    })
}

fn eval_or_exit(evaluator: &mut Evaluator, form: &neovm_core::elisp::Expr) {
    if let Err(err) = evaluator.eval_expr(form) {
        eprintln!("benchmark form evaluation failed: {err}");
        std::process::exit(1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 3 {
        eprintln!("usage: introspection_bench [iterations] [--per-form]");
        std::process::exit(2);
    }

    let options = match parse_args(&args[1..]) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("{err}");
            eprintln!("usage: introspection_bench [iterations] [--per-form]");
            std::process::exit(2);
        }
    };

    let source = INTROSPECTION_FORMS.join("\n");
    let forms = match parse_forms(&source) {
        Ok(forms) => forms,
        Err(err) => {
            eprintln!("failed to parse benchmark forms: {err}");
            std::process::exit(1);
        }
    };

    let mut evaluator = Evaluator::new();
    let total_ops = options.iterations.saturating_mul(forms.len());
    let start = Instant::now();

    for _ in 0..options.iterations {
        for form in &forms {
            eval_or_exit(&mut evaluator, form);
        }
    }

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let ns_per_op = (elapsed.as_secs_f64() * 1_000_000_000.0) / total_ops as f64;
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();

    println!("iterations: {}", options.iterations);
    println!("forms_per_iteration: {}", forms.len());
    println!("total_ops: {total_ops}");
    println!("elapsed_ms: {:.3}", elapsed_ms);
    println!("ns_per_op: {:.1}", ns_per_op);
    println!("ops_per_sec: {:.0}", ops_per_sec);

    if options.per_form {
        println!("per_form_ns_per_op:");
        for (idx, form) in forms.iter().enumerate() {
            let mut per_form_evaluator = Evaluator::new();
            let per_form_start = Instant::now();
            for _ in 0..options.iterations {
                eval_or_exit(&mut per_form_evaluator, form);
            }
            let per_form_elapsed = per_form_start.elapsed();
            let per_form_ns =
                (per_form_elapsed.as_secs_f64() * 1_000_000_000.0) / options.iterations as f64;
            println!("{}\t{:.1}\t{}", idx + 1, per_form_ns, INTROSPECTION_FORMS[idx]);
        }
    }
}
