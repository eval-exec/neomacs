use neovm_core::elisp::load::load_file;
use neovm_core::elisp::Evaluator;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn timed_load(path: &Path) -> Result<Duration, String> {
    let mut evaluator = Evaluator::new();
    let start = Instant::now();
    load_file(&mut evaluator, path).map_err(|e| format!("{e:?}"))?;
    Ok(start.elapsed())
}

fn parse_iterations(arg: Option<&String>) -> Result<usize, String> {
    match arg {
        None => Ok(20),
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|e| format!("invalid iterations '{}': {}", raw, e))
            .and_then(|n| {
                if n == 0 {
                    Err("iterations must be > 0".to_string())
                } else {
                    Ok(n)
                }
            }),
    }
}

fn cache_path(source: &Path) -> PathBuf {
    source.with_extension("neoc")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: load_cache_bench <source.el> [iterations]");
        std::process::exit(2);
    }

    let source = PathBuf::from(&args[1]);
    let iterations = match parse_iterations(args.get(2)) {
        Ok(n) => n,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let cache = cache_path(&source);
    let _ = fs::remove_file(&cache);

    let cold = match timed_load(&source) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("cold load failed for {}: {}", source.display(), err);
            std::process::exit(1);
        }
    };

    let warm_single = match timed_load(&source) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("warm load failed for {}: {}", source.display(), err);
            std::process::exit(1);
        }
    };

    let mut warm_total = Duration::ZERO;
    for _ in 0..iterations {
        match timed_load(&source) {
            Ok(d) => warm_total += d,
            Err(err) => {
                eprintln!("warm iteration failed for {}: {}", source.display(), err);
                std::process::exit(1);
            }
        }
    }

    let warm_avg_ms = (warm_total.as_secs_f64() * 1000.0) / iterations as f64;

    println!("source: {}", source.display());
    println!("cache: {}", cache.display());
    println!("cold_load_ms: {:.3}", cold.as_secs_f64() * 1000.0);
    println!("warm_load_ms: {:.3}", warm_single.as_secs_f64() * 1000.0);
    println!("warm_avg_ms(iterations={}): {:.3}", iterations, warm_avg_ms);
}
