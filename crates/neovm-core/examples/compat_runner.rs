use neovm_core::emacs_core::{
    Context, format_eval_result_bytes_with_eval, print::print_value, symbol::Obarray, value_reader,
};
use std::fs;
use std::io::{self, Write};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: compat_runner <forms-file>");
        std::process::exit(2);
    };

    let source = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("failed to read {path}: {err}");
            std::process::exit(2);
        }
    };

    let forms = match value_reader::read_all(&source, &Obarray::new()) {
        Ok(forms) => forms,
        Err(err) => {
            eprintln!("failed to parse forms: {err}");
            std::process::exit(2);
        }
    };

    let mut evaluator = Context::new();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for (index, form) in forms.iter().enumerate() {
        let result = evaluator.eval_form(*form);
        out.write_all((index + 1).to_string().as_bytes())
            .expect("write index");
        out.write_all(b"\t").expect("write tab");
        out.write_all(print_value(form).as_bytes())
            .expect("write form");
        out.write_all(b"\t").expect("write tab");
        out.write_all(&format_eval_result_bytes_with_eval(&evaluator, &result))
            .expect("write result");
        out.write_all(b"\n").expect("write newline");
    }
    out.flush().expect("flush output");
}
