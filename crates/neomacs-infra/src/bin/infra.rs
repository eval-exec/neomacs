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
            Some(name) if neomacs_infra::config_env::NAMES.contains(&name) => {
                match neomacs_infra::config_env::open_by_name(name) {
                    Some(environment) => match environment.verify_deep() {
                        Ok(drift) if drift.is_clean() => println!("{name}: verified clean"),
                        Ok(drift) => {
                            eprintln!("{name} fixture DRIFTED: {drift:?}");
                            exit(1);
                        }
                        Err(error) => {
                            eprintln!("{name} verify error: {error}");
                            exit(1);
                        }
                    },
                    None => {
                        eprintln!("{name}: not materialized");
                        exit(1);
                    }
                }
            }
            other => {
                eprintln!("unknown environment: {other:?}");
                exit(2);
            }
        },
        Some("packages") => match args.next().as_deref().and_then(|arg| arg.to_str()) {
            Some("preflight") => {
                let pins = parse_package_pins(std::env::args().skip(3));
                let driver = packages_driver();
                match neomacs_infra::packages::preflight_locked_melpa_packages(
                    &driver,
                    &pins.iter().map(|p| p.as_pair()).collect::<Vec<_>>(),
                ) {
                    Ok(paths) => {
                        for path in paths {
                            println!("prepared: {}", path.display());
                        }
                    }
                    Err(error) => {
                        eprintln!("preflight failed: {error}");
                        exit(1);
                    }
                }
            }
            Some("verify") => {
                let pins = parse_package_pins(std::env::args().skip(3));
                let driver = packages_driver();
                for pin in &pins {
                    match neomacs_infra::packages::provision(pin, &driver) {
                        Ok(provisioned) => {
                            let report =
                                neomacs_infra::packages::seal::verify_provisioned(&provisioned);
                            match report {
                                Ok(report) => match neomacs_infra::packages::seal::drifted(&report)
                                {
                                    Some(drift) => {
                                        eprintln!("{} {}: DRIFT {drift:?}", pin.name, pin.version);
                                        exit(1);
                                    }
                                    None => println!(
                                        "{} {}: verified clean ({})",
                                        pin.name,
                                        pin.version,
                                        provisioned.package_dir().display()
                                    ),
                                },
                                Err(error) => {
                                    eprintln!(
                                        "{} {}: verify error: {error}",
                                        pin.name, pin.version
                                    );
                                    exit(1);
                                }
                            }
                        }
                        Err(error) => {
                            eprintln!("{} {}: provision failed: {error}", pin.name, pin.version);
                            exit(1);
                        }
                    }
                }
            }
            _ => {
                eprintln!("usage: infra packages <preflight|verify> <name>@<version> ...");
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
            eprintln!(
                "usage: infra <materialize {} [path]|status|packages <preflight|verify> <name>@<version> ...>",
                {
                    let mut usage = String::new();
                    for name in neomacs_infra::config_env::NAMES {
                        usage.push_str(&format!("<{name}>|"));
                    }
                    usage
                }
            );
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

fn parse_package_pins(
    args: impl Iterator<Item = String>,
) -> Vec<neomacs_infra::packages::PinnedPackage> {
    let pins: Vec<_> = args
        .map(|argument| {
            let text = argument;
            let (name, version) = text
                .split_once('@')
                .unwrap_or_else(|| panic!("package pin must be <name>@<version>, got `{text}`"));
            neomacs_infra::packages::PinnedPackage::new(name, version)
        })
        .collect();
    if pins.is_empty() {
        eprintln!("usage: infra packages <preflight|verify> <name>@<version> ...");
        exit(2);
    }
    pins
}

fn packages_driver() -> neomacs_infra::packages::PathGnuDriver {
    neomacs_infra::packages::PathGnuDriver::resolve().unwrap_or_else(|error| {
        eprintln!("{error}");
        exit(1);
    })
}
