use super::*;

fn p(name: &str) -> PathBuf {
    PathBuf::from(format!("/lisp/{name}.el"))
}

fn deps(edges: &[(&str, &str)]) -> BTreeMap<PathBuf, BTreeSet<PathBuf>> {
    let mut map = BTreeMap::<PathBuf, BTreeSet<PathBuf>>::new();
    for (from, to) in edges {
        map.entry(p(from)).or_default().insert(p(to));
    }
    map
}

fn waves_of(sources: &[&str], edges: &[(&str, &str)]) -> Vec<Vec<PathBuf>> {
    dependency_waves(sources.iter().map(|s| p(s)).collect(), &deps(edges))
}

#[test]
fn a_file_compiles_one_wave_after_its_deepest_dependency() {
    let waves = waves_of(
        &["app", "lib", "base", "solo"],
        &[("app", "lib"), ("lib", "base"), ("app", "base")],
    );
    assert_eq!(
        waves,
        vec![vec![p("base"), p("solo")], vec![p("lib")], vec![p("app")]]
    );
}

#[test]
fn dependencies_outside_the_set_do_not_delay_a_file() {
    // `preloaded` was compiled by an earlier stage: it is not a source here.
    let waves = waves_of(&["a", "b"], &[("a", "preloaded"), ("b", "b")]);
    assert_eq!(waves, vec![vec![p("a"), p("b")]]);
}

#[test]
fn a_cycle_drops_its_edges_to_later_members_by_path() {
    let waves = waves_of(
        &["z-user", "m-cycle", "b-cycle", "k-cycle"],
        &[
            ("b-cycle", "k-cycle"),
            ("k-cycle", "m-cycle"),
            ("m-cycle", "b-cycle"),
            ("z-user", "k-cycle"),
        ],
    );
    assert_eq!(
        waves,
        vec![
            vec![p("b-cycle"), p("k-cycle")],
            vec![p("m-cycle"), p("z-user")],
        ]
    );
}

#[test]
fn waves_do_not_depend_on_input_order() {
    let edges = [("c", "a"), ("d", "c"), ("b", "d"), ("d", "b")];
    let forward = waves_of(&["a", "b", "c", "d"], &edges);
    let backward = waves_of(&["d", "c", "b", "a"], &edges);
    assert_eq!(forward, backward);
}

#[cfg(unix)]
#[test]
fn scanner_output_keeps_only_edges_between_sources() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a.el");
    let b = root.path().join("b.el");
    let outside = root.path().join("outside.el");
    for file in [&a, &b, &outside] {
        fs::write(file, b"").unwrap();
    }
    // The scanner prints dependencies as true names.
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(root.path(), &alias).unwrap();
    let output = format!(
        "{a}\t{b_alias}\n{a}\t{outside}\n{b}\t{b}\nmalformed line\n{b}\t/nonexistent.el\n",
        a = a.display(),
        b = b.display(),
        b_alias = alias.join("b.el").display(),
        outside = outside.display(),
    );
    let edges = parse_load_dependencies(&output, &[a.clone(), b.clone()]);
    assert_eq!(edges, BTreeMap::from([(a, BTreeSet::from([b]))]));
}
