//! `infra` — materialize and inspect shared test environment fixtures.

use std::process::exit;

fn main() {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("materialize") => match args.next().as_deref().and_then(|arg| arg.to_str()) {
            Some(name) if neomacs_infra::config_env::NAMES.contains(&name) => {
                let source = match args.next() {
                    Some(path) => operator_source(name, path),
                    None => default_source(name),
                };
                match materialize(name, source) {
                    Ok(()) => println!("{name} fixture ready"),
                    Err(error) => {
                        eprintln!("error: {error}");
                        exit(1);
                    }
                }
            }
            other => {
                eprintln!(
                    "usage: infra materialize <{}> [operator-checkout]",
                    neomacs_infra::config_env::NAMES.join("|")
                );
                if let Some(other) = other {
                    eprintln!("unknown environment: {other}");
                }
                exit(2);
            }
        },
        Some("verify") => match args.next().as_deref().and_then(|arg| arg.to_str()) {
            Some("doom") => match neomacs_infra::config_env::DoomEnvironment::open() {
                Some(environment) => {
                    match neomacs_infra::config_env::ConfigEnvironment::verify_deep(&environment) {
                        Ok(drift) if drift.is_clean() => println!("doom: verified clean"),
                        Ok(drift) => {
                            eprintln!("doom fixture DRIFTED: {drift:?}");
                            exit(1);
                        }
                        Err(error) => {
                            eprintln!("doom verify error: {error}");
                            exit(1);
                        }
                    }
                }
                None => {
                    eprintln!("doom: not materialized");
                    exit(1);
                }
            },
            other => {
                eprintln!("unknown environment: {other:?}");
                exit(2);
            }
        },
        Some("status") => {
            for name in neomacs_infra::config_env::NAMES {
                let status = match name.as_ref() {
                    "doom" => neomacs_infra::config_env::doom::doom_status().map(|_| ()),
                    "spacemacs" => {
                        neomacs_infra::config_env::spacemacs::spacemacs_status().map(|_| ())
                    }
                    _ => unreachable!("NAMES and status arms must stay in sync"),
                };
                match status {
                    Ok(()) => println!("{name}: materialized and sealed"),
                    Err(reason) => println!("{name}: {reason}"),
                }
            }
        }
        _ => {
            eprintln!("usage: infra <materialize {} [path]|status>", {
                let mut usage = String::new();
                for name in neomacs_infra::config_env::NAMES {
                    usage.push_str(&format!("<{name}>|"));
                }
                usage
            });
            exit(2);
        }
    }
}

fn default_source(name: &str) -> MaterializeSource {
    match name {
        "doom" => MaterializeSource::Doom(
            neomacs_infra::config_env::DoomSource::resolve()
                .unwrap_or(neomacs_infra::config_env::DoomSource::Pinned),
        ),
        "spacemacs" => MaterializeSource::Spacemacs(
            neomacs_infra::config_env::SpacemacsSource::resolve()
                .unwrap_or(neomacs_infra::config_env::SpacemacsSource::Pinned),
        ),
        _ => unreachable!(),
    }
}

fn operator_source(name: &str, path: std::ffi::OsString) -> MaterializeSource {
    match name {
        "doom" => {
            MaterializeSource::Doom(neomacs_infra::config_env::DoomSource::Operator(path.into()))
        }
        "spacemacs" => MaterializeSource::Spacemacs(
            neomacs_infra::config_env::SpacemacsSource::Operator(path.into()),
        ),
        _ => unreachable!(),
    }
}

enum MaterializeSource {
    Doom(neomacs_infra::config_env::DoomSource),
    Spacemacs(neomacs_infra::config_env::SpacemacsSource),
}

fn materialize(name: &str, source: MaterializeSource) -> Result<(), String> {
    match (name, source) {
        ("doom", MaterializeSource::Doom(source)) => {
            neomacs_infra::config_env::DoomEnvironment::materialize(source).map(|_| ())
        }
        ("spacemacs", MaterializeSource::Spacemacs(source)) => {
            neomacs_infra::config_env::SpacemacsEnvironment::materialize(source).map(|_| ())
        }
        _ => unreachable!(),
    }
}
