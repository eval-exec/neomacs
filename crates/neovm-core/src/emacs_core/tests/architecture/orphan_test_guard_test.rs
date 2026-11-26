//! Fail when a test `.rs` file is reachable from no crate root.
//!
//! Roots are Cargo targets (`src/lib.rs`, `src/main.rs`, explicit and
//! autodiscovered bins/tests/examples/benches, `build.rs`) plus trybuild
//! fixture entries. Edges are `mod`, `#[path]`, and `include!`. A `cfg` on
//! the `mod` does not drop the edge. `#[path]` children resolve next to that
//! file, which is rustc's historical rule for a path-mounted file. Implicit
//! `name.rs` children still use the `name/` directory.
//!
//! Exclusions, each with a reason:
//! * Cargo and trybuild roots are crate roots of their own rustc invocation.
//!   A compile-fail fixture must not be `mod`ed into its runner.
//! * `include_str!` targets are string fixtures, not modules.
//! * `target/`, `tmp/`, `.git`, and `vendor/` are not source.
//!
//! The recorded debt in `orphan_test_guard_known.tsv` is the sweep of this
//! tree. The guard fails if a path appears or disappears, so a new orphan or
//! a silent delete is visible. Wiring or deleting a debt row is a follow-up
//! that updates the file in the same commit.

use std::fs;
use std::path::{Path, PathBuf};

use super::architecture_production_source_roots::workspace_root;

const SKIP: &[&str] = &["target", "tmp", ".git", "vendor", "node_modules"];

struct Hit {
    start: usize,
    end: usize,
    name: String,
    kind: u8,
}

struct Scanner {
    root: PathBuf,
    reached: std::collections::BTreeSet<PathBuf>,
    trybuild: std::collections::BTreeSet<PathBuf>,
    string_fixtures: std::collections::BTreeSet<PathBuf>,
    visiting: std::collections::BTreeSet<(PathBuf, PathBuf)>,
    cargo_roots: std::collections::BTreeSet<PathBuf>,
}

pub(crate) fn orphan_test_files(root: &Path) -> Vec<String> {
    // Visits and candidates are canonical. A root that still contains a
    // symlink makes `strip_prefix` reject every file.
    let root = normalize(root);
    let mut scanner = Scanner {
        root: root.clone(),
        reached: std::collections::BTreeSet::new(),
        trybuild: std::collections::BTreeSet::new(),
        string_fixtures: std::collections::BTreeSet::new(),
        visiting: std::collections::BTreeSet::new(),
        cargo_roots: std::collections::BTreeSet::new(),
    };
    let mut roots = Vec::new();
    walk_manifests(&root, &mut roots);
    for path in &roots {
        scanner.cargo_roots.insert(normalize(path));
    }
    for path in roots {
        scanner.visit(&path, path.parent().unwrap_or(&path).to_path_buf(), 0);
    }
    let mut orphans = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if SKIP.iter().any(|skip| name == *skip) {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "rs") {
                let path = normalize(&path);
                if scanner.is_candidate(&path) {
                    orphans.push(rel_string(&scanner.root, &path));
                }
            }
        }
    }
    orphans.sort();
    orphans
}

fn rel_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn inside(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    !rel.components()
        .any(|part| SKIP.iter().any(|skip| part.as_os_str() == *skip))
}

impl Scanner {
    fn is_candidate(&self, path: &Path) -> bool {
        let path = normalize(path);
        if self.cargo_roots.contains(&path)
            || self.trybuild.contains(&path)
            || self.string_fixtures.contains(&path)
            || !inside(&self.root, &path)
        {
            return false;
        }
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return false;
        };
        let parts: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if parts.iter().any(|part| part == "core_tests") {
            return !self.reached.contains(&path);
        }
        if let Some(src) = parts.iter().position(|part| part == "src") {
            if parts[src + 1..].iter().any(|part| part == "tests")
                || path.file_name().is_some_and(|n| n == "tests.rs")
            {
                return !self.reached.contains(&path);
            }
        }
        if parts
            .first()
            .is_some_and(|part| part == "crates" || part == "tests")
            && parts.iter().any(|part| part == "tests")
        {
            return !self.reached.contains(&path);
        }
        false
    }

    fn visit(&mut self, path: &Path, mod_dir: PathBuf, depth: u32) {
        if depth > 60 {
            return;
        }
        let Ok(path) = path.canonicalize() else {
            return;
        };
        if !path.is_file() || !inside(&self.root, &path) {
            return;
        }
        let key = (path.clone(), mod_dir.clone());
        if !self.visiting.insert(key) {
            return;
        }
        self.reached.insert(path.clone());
        let Ok(original) = fs::read_to_string(&path) else {
            return;
        };
        let blanked = blank(&original);
        self.parse(&original, &blanked, &path, &mod_dir, depth);
    }

    fn parse(&mut self, original: &str, text: &str, file_path: &Path, mod_dir: &Path, depth: u32) {
        if depth > 60 {
            return;
        }
        for (start, end) in find_literal(original, "include_str!") {
            if !live_span(text, start, end) {
                continue;
            }
            if let Some(lit) = string_after(original, end) {
                let target = file_path.parent().unwrap_or(file_path).join(lit);
                if target.extension().is_some_and(|ext| ext == "rs") {
                    if let Ok(target) = target.canonicalize() {
                        self.string_fixtures.insert(target);
                    }
                }
            }
        }
        for (start, end) in find_literal(original, "include!") {
            if !live_span(text, start, end) {
                continue;
            }
            if let Some(lit) = string_after(original, end) {
                let target = file_path.parent().unwrap_or(file_path).join(lit);
                self.visit(&target, mod_dir.to_path_buf(), depth + 1);
            }
        }
        self.discover_trybuild(original, text, file_path, depth);
        for hit in find_mods(text) {
            let attr_at = attr_start(text, hit.start);
            let attrs = &original[attr_at..hit.start];
            if hit.kind == b';' {
                if let Some((child, child_dir)) =
                    resolve_child(file_path, mod_dir, &hit.name, attrs)
                {
                    let via_path = path_attr(attrs).is_some();
                    self.visit(&child, child_dir, depth + 1);
                    if !via_path {
                        let plain = mod_dir.join(format!("{}.rs", hit.name));
                        let nested = mod_dir.join(&hit.name).join("mod.rs");
                        if plain.is_file() && nested.is_file() {
                            let other = if child == normalize(&plain) {
                                nested
                            } else {
                                plain
                            };
                            let other_dir = if other.file_name().is_some_and(|n| n == "mod.rs") {
                                other.parent().unwrap_or(&other).to_path_buf()
                            } else {
                                other
                                    .parent()
                                    .unwrap_or(&other)
                                    .join(other.file_stem().unwrap_or_default())
                            };
                            self.visit(&other, other_dir, depth + 1);
                        }
                    }
                }
            } else {
                let open_at = hit.end - 1;
                let close_at = matching_brace(text, open_at);
                let body_orig = &original[open_at + 1..close_at];
                let body = &text[open_at + 1..close_at];
                self.parse(
                    body_orig,
                    body,
                    file_path,
                    &mod_dir.join(&hit.name),
                    depth + 1,
                );
            }
        }
    }

    fn discover_trybuild(&mut self, original: &str, text: &str, file_path: &Path, depth: u32) {
        let pkg = package_dir(file_path);
        let mut found = Vec::new();
        let bytes = original.as_bytes();
        let mut i = 0;
        while let Some(rel) = find_sub(bytes, b"fixtures", i) {
            if live_string(text, rel) {
                if let Some((dir, overlay, next)) =
                    two_strings_after(original, rel + "fixtures".len())
                {
                    found.extend(direct_rs(&pkg.join(dir)));
                    found.extend(direct_rs(&pkg.join(overlay)));
                    i = next;
                    continue;
                }
            }
            i = rel + "fixtures".len();
        }
        for keyword in ["compile_fail", "pass"] {
            let mut i = 0;
            let needle = keyword.as_bytes();
            while let Some(rel) = find_sub(bytes, needle, i) {
                let end = rel + needle.len();
                if live_span(text, rel, end) {
                    if let Some((lit, next)) = one_string_after(original, end) {
                        if lit.contains('*') {
                            found.extend(glob_rs(&pkg, &lit));
                        } else {
                            found.push(pkg.join(lit));
                        }
                        i = next;
                        continue;
                    }
                }
                i = end;
            }
        }
        let mut i = 0;
        while let Some(rel) = find_sub(bytes, b"\"", i) {
            if let Some(end) = original[rel + 1..].find('"') {
                let lit = &original[rel + 1..rel + 1 + end];
                let abs_end = rel + 1 + end + 1;
                if lit.contains('*') && lit.ends_with(".rs") && live_string(text, rel) {
                    found.extend(glob_rs(&pkg, lit));
                }
                i = abs_end;
            } else {
                break;
            }
        }
        for child in found {
            let Ok(child) = child.canonicalize() else {
                continue;
            };
            if child.is_file() && inside(&self.root, &child) {
                self.trybuild.insert(child.clone());
                let parent = child.parent().unwrap_or(&child).to_path_buf();
                self.visit(&child, parent, depth + 1);
            }
        }
    }
}

fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn package_dir(path: &Path) -> PathBuf {
    let mut cursor = path.to_path_buf();
    loop {
        if cursor.join("Cargo.toml").is_file() {
            return cursor;
        }
        if !cursor.pop() {
            return path.parent().unwrap_or(path).to_path_buf();
        }
    }
}

fn direct_rs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
        .collect()
}

fn glob_rs(base: &Path, pattern: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = pattern.split('/').filter(|part| !part.is_empty()).collect();
    let mut dirs = vec![base.to_path_buf()];
    for (index, part) in parts.iter().enumerate() {
        let last = index + 1 == parts.len();
        let mut next = Vec::new();
        for dir in dirs {
            if !part.contains('*') {
                let child = dir.join(part);
                if last {
                    if child.is_file() {
                        next.push(child);
                    }
                } else if child.is_dir() {
                    next.push(child);
                }
            } else if last {
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        if wild_match(part, &name) && entry.path().is_file() {
                            next.push(entry.path());
                        }
                    }
                }
            }
        }
        dirs = next;
    }
    dirs
}

fn wild_match(pattern: &str, name: &str) -> bool {
    let Some(star) = pattern.find('*') else {
        return pattern == name;
    };
    let (prefix, rest) = pattern.split_at(star);
    let suffix = &rest[1..];
    name.starts_with(prefix) && name.ends_with(suffix) && name.len() >= prefix.len() + suffix.len()
}

fn resolve_child(
    file_path: &Path,
    mod_dir: &Path,
    name: &str,
    attrs: &str,
) -> Option<(PathBuf, PathBuf)> {
    if let Some(rel) = path_attr(attrs) {
        let mut child = file_path.parent().unwrap_or(file_path).join(rel);
        if child.extension().is_none_or(|ext| ext != "rs") {
            if child.is_dir() && child.join("mod.rs").is_file() {
                child = child.join("mod.rs");
            } else if !child.exists() {
                child.set_extension("rs");
            }
        }
        // #[path] children sit next to the mounted file, not in a stem directory.
        let dir = child
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| child.clone());
        return Some((child, dir));
    }
    let plain = mod_dir.join(format!("{name}.rs"));
    let nested = mod_dir.join(name).join("mod.rs");
    if plain.is_file() {
        let dir = plain
            .parent()
            .unwrap_or(mod_dir)
            .join(plain.file_stem().unwrap_or_default());
        return Some((plain, dir));
    }
    if nested.is_file() {
        return Some((
            nested.clone(),
            nested.parent().unwrap_or(mod_dir).to_path_buf(),
        ));
    }
    None
}

fn path_attr(attrs: &str) -> Option<&str> {
    let bytes = attrs.as_bytes();
    let mut last = None;
    let mut i = 0;
    while let Some(rel) = find_sub(bytes, b"path", i) {
        let mut j = rel + 4;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'=' {
            if let Some((lit, next)) = one_string_after(attrs, j + 1) {
                last = Some(lit);
                i = next;
                continue;
            }
        }
        i = rel + 4;
    }
    last
}

fn walk_manifests(root: &Path, roots: &mut Vec<PathBuf>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if SKIP.iter().any(|skip| name == *skip) {
                    continue;
                }
                stack.push(path);
            } else if name == "Cargo.toml" {
                roots.extend(package_roots(&path));
            }
        }
    }
}

struct Item {
    name: Option<String>,
    path: Option<String>,
}

fn package_roots(toml_path: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(toml_path) else {
        return Vec::new();
    };
    let base = toml_path.parent().unwrap_or(toml_path);
    #[derive(Clone, Copy, PartialEq)]
    enum Sec {
        None,
        Package,
        Lib,
        Bin,
        Test,
        Example,
        Bench,
    }
    let mut section = Sec::None;
    let mut has_package = false;
    let mut autotests = true;
    let mut autobins = true;
    let mut autoexamples = true;
    let mut autobenches = true;
    let mut lib_path = None;
    let mut has_lib = false;
    let mut bins = Vec::new();
    let mut tests = Vec::new();
    let mut examples = Vec::new();
    let mut benches = Vec::new();
    for raw in text.lines() {
        let line = strip_toml_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("[[") {
            let name = rest.trim_end_matches(']').trim();
            section = match name {
                "bin" => Sec::Bin,
                "test" => Sec::Test,
                "example" => Sec::Example,
                "bench" => Sec::Bench,
                _ => Sec::None,
            };
            let item = Item {
                name: None,
                path: None,
            };
            match section {
                Sec::Bin => bins.push(item),
                Sec::Test => tests.push(item),
                Sec::Example => examples.push(item),
                Sec::Bench => benches.push(item),
                _ => {}
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest.trim_end_matches(']').trim();
            section = match name {
                "package" => Sec::Package,
                "lib" => {
                    has_lib = true;
                    Sec::Lib
                }
                _ => Sec::None,
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').to_string();
        match section {
            Sec::Package => {
                has_package = true;
                match key {
                    "autotests" => autotests = value != "false",
                    "autobins" => autobins = value != "false",
                    "autoexamples" => autoexamples = value != "false",
                    "autobenches" => autobenches = value != "false",
                    _ => {}
                }
            }
            Sec::Lib if key == "path" => lib_path = Some(value),
            Sec::Bin => assign_item(bins.last_mut(), key, value),
            Sec::Test => assign_item(tests.last_mut(), key, value),
            Sec::Example => assign_item(examples.last_mut(), key, value),
            Sec::Bench => assign_item(benches.last_mut(), key, value),
            _ => {
                if key == "name" && section == Sec::Package {
                    has_package = true;
                }
            }
        }
    }
    if !has_package && text.contains("[package]") {
        has_package = true;
    }
    if !has_package {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(path) = lib_path {
        push_if_file(base, &path, &mut out);
    } else if base.join("src/lib.rs").is_file() {
        push_if_file(base, "src/lib.rs", &mut out);
    }
    if autobins {
        if base.join("src/main.rs").is_file() {
            push_if_file(base, "src/main.rs", &mut out);
        }
        add_auto(base, "src/bin", &mut out);
    }
    for item in &bins {
        if let Some(path) = &item.path {
            push_if_file(base, path, &mut out);
        } else if let Some(name) = &item.name {
            push_if_file(base, &format!("src/bin/{name}.rs"), &mut out);
        }
    }
    if autotests {
        add_auto(base, "tests", &mut out);
    }
    for item in &tests {
        if let Some(path) = &item.path {
            push_if_file(base, path, &mut out);
        } else if let Some(name) = &item.name {
            push_if_file(base, &format!("tests/{name}.rs"), &mut out);
        }
    }
    for (auto, folder, items) in [
        (autoexamples, "examples", &examples),
        (autobenches, "benches", &benches),
    ] {
        if auto {
            add_auto(base, folder, &mut out);
        }
        for item in items {
            if let Some(path) = &item.path {
                push_if_file(base, path, &mut out);
            } else if let Some(name) = &item.name {
                push_if_file(base, &format!("{folder}/{name}.rs"), &mut out);
            }
        }
    }
    if base.join("build.rs").is_file() {
        push_if_file(base, "build.rs", &mut out);
    }
    let _ = has_lib;
    out
}

fn push_if_file(base: &Path, rel: &str, out: &mut Vec<PathBuf>) {
    let path = base.join(rel);
    if path.is_file() {
        out.push(path);
    }
}

fn assign_item(item: Option<&mut Item>, key: &str, value: String) {
    let Some(item) = item else {
        return;
    };
    match key {
        "name" => item.name = Some(value),
        "path" => item.path = Some(value),
        _ => {}
    }
}

fn add_auto(base: &Path, folder: &str, out: &mut Vec<PathBuf>) {
    let dir = base.join(folder);
    if !dir.is_dir() {
        return;
    }
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            } else if path.is_dir() && path.join("main.rs").is_file() {
                out.push(path.join("main.rs"));
            }
        }
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut quote = false;
    for (index, byte) in line.bytes().enumerate() {
        if byte == b'"' {
            quote = !quote;
        } else if byte == b'#' && !quote {
            return &line[..index];
        }
    }
    line
}

fn blank(src: &str) -> String {
    blank_masked(src)
}

fn blank_masked(src: &str) -> String {
    let b = src.as_bytes();
    let n = b.len();
    let mut keep = vec![true; n];
    let mut i = 0;
    while i < n {
        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let start = i;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            for slot in keep.iter_mut().take(i).skip(start) {
                *slot = false;
            }
            continue;
        }
        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let start = i;
            i += 2;
            let mut depth = 1;
            while i < n && depth > 0 {
                if i + 1 < n && b[i] == b'/' && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                    continue;
                }
                if i + 1 < n && b[i] == b'*' && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                    continue;
                }
                i += 1;
            }
            for slot in keep.iter_mut().take(i).skip(start) {
                *slot = false;
            }
            continue;
        }
        let raw = b[i] == b'r' || (matches!(b[i], b'b' | b'c') && i + 1 < n && b[i + 1] == b'r');
        if raw {
            let start = i;
            let mut j = i + if b[i] == b'r' { 1 } else { 2 };
            let mut hashes = 0;
            while j < n && b[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < n && b[j] == b'"' {
                j += 1;
                while j < n {
                    if b[j] == b'"' {
                        let mut t = j + 1;
                        let mut seen = 0;
                        while t < n && b[t] == b'#' && seen < hashes {
                            seen += 1;
                            t += 1;
                        }
                        if seen == hashes {
                            j = t;
                            break;
                        }
                    }
                    j += 1;
                }
                for slot in keep.iter_mut().take(j).skip(start) {
                    *slot = false;
                }
                i = j;
                continue;
            }
        }
        let byte_string = matches!(b[i], b'b' | b'c') && i + 1 < n && b[i + 1] == b'"';
        if b[i] == b'"' || byte_string {
            let start = i;
            i += if byte_string { 2 } else { 1 };
            while i < n {
                if b[i] == b'\\' {
                    i = (i + 2).min(n);
                    continue;
                }
                if b[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            for slot in keep.iter_mut().take(i).skip(start) {
                *slot = false;
            }
            continue;
        }
        if b[i] == b'\'' {
            let mut j = i + 1;
            if j < n && b[j] == b'\\' {
                j += 2;
                if j < n && b[j] == b'\'' {
                    for slot in keep.iter_mut().take(j + 1).skip(i) {
                        *slot = false;
                    }
                    i = j + 1;
                    continue;
                }
            } else if j + 1 < n && b[j + 1] == b'\'' {
                for slot in keep.iter_mut().take(j + 2).skip(i) {
                    *slot = false;
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    let mut out = b.to_vec();
    for (index, kept) in keep.iter().enumerate() {
        if !kept && out[index] != b'\n' {
            out[index] = b' ';
        }
    }
    String::from_utf8(out).expect("blanked rust source stays utf-8")
}

fn attr_start(text: &str, mod_at: usize) -> usize {
    let b = text.as_bytes();
    let mut j = mod_at;
    loop {
        let mut k = j;
        while k > 0 && matches!(b[k - 1], b' ' | b'\t' | b'\r' | b'\n') {
            k -= 1;
        }
        if k == 0 || b[k - 1] != b']' {
            return j;
        }
        let mut depth = 0i32;
        let mut p = k - 1;
        let mut start = None;
        loop {
            if b[p] == b']' {
                depth += 1;
            } else if b[p] == b'[' {
                depth -= 1;
                if depth == 0 {
                    if p > 0 && b[p - 1] == b'#' {
                        start = Some(p - 1);
                    }
                    break;
                }
            }
            if p == 0 {
                break;
            }
            p -= 1;
        }
        let Some(found) = start else {
            return j;
        };
        j = found;
    }
}

fn matching_brace(text: &str, open_at: usize) -> usize {
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut i = open_at;
    while i < b.len() {
        if b[i] == b'{' {
            depth += 1;
        } else if b[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    b.len().saturating_sub(1)
}

fn find_mods(text: &str) -> Vec<Hit> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < b.len() {
        let boundary = i == 0 || !is_ident(b[i - 1]);
        if boundary && b[i..].starts_with(b"mod") && (i + 3 == b.len() || !is_ident(b[i + 3])) {
            let mut j = i + 3;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && (b[j].is_ascii_alphabetic() || b[j] == b'_') {
                let name_at = j;
                j += 1;
                while j < b.len() && is_ident(b[j]) {
                    j += 1;
                }
                let name = text[name_at..j].to_string();
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < b.len() && (b[j] == b';' || b[j] == b'{') {
                    out.push(Hit {
                        // `pub` / `pub(...)` sits between the attributes and `mod`.
                        // Attribute walk must start before that qualifier.
                        start: visibility_start(text, i),
                        end: j + 1,
                        name,
                        kind: b[j],
                    });
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Byte index of `pub` or `pub(...)` immediately before `mod`, else `mod_at`.
fn visibility_start(text: &str, mod_at: usize) -> usize {
    let b = text.as_bytes();
    let mut k = mod_at;
    while k > 0 && b[k - 1].is_ascii_whitespace() {
        k -= 1;
    }
    if k == 0 {
        return mod_at;
    }
    if b[k - 1] == b')' {
        let mut depth = 0i32;
        let mut p = k - 1;
        let mut open = None;
        loop {
            if b[p] == b')' {
                depth += 1;
            } else if b[p] == b'(' {
                depth -= 1;
                if depth == 0 {
                    open = Some(p);
                    break;
                }
            }
            if p == 0 {
                break;
            }
            p -= 1;
        }
        let Some(open) = open else {
            return mod_at;
        };
        let mut q = open;
        while q > 0 && b[q - 1].is_ascii_whitespace() {
            q -= 1;
        }
        return pub_word_at(b, q).unwrap_or(mod_at);
    }
    pub_word_at(b, k).unwrap_or(mod_at)
}

fn pub_word_at(bytes: &[u8], end: usize) -> Option<usize> {
    if end >= 3 && bytes[end - 3..end] == *b"pub" && (end == 3 || !is_ident(bytes[end - 4])) {
        Some(end - 3)
    } else {
        None
    }
}

fn find_literal(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let bytes = hay.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = find_sub(bytes, needle.as_bytes(), i) {
        let end = rel + needle.len();
        out.push((rel, end));
        i = end;
    }
    out
}

fn find_sub(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|pos| from + pos)
}

fn live_span(blanked: &str, start: usize, end: usize) -> bool {
    blanked
        .get(start..end)
        .is_some_and(|span| span.chars().any(|ch| !ch.is_whitespace()))
}

fn live_string(blanked: &str, start: usize) -> bool {
    let b = blanked.as_bytes();
    if start >= b.len() {
        return false;
    }
    let mut i = start;
    while i > 0 && matches!(b[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    i > 0 && !matches!(b[i - 1], b' ' | b'\n')
}

fn string_after(text: &str, from: usize) -> Option<&str> {
    one_string_after(text, from).map(|(lit, _)| lit)
}

fn one_string_after(text: &str, from: usize) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    let mut i = from;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < b.len() && b[i] == b'(' {
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    let start = i + 1;
    let end = text[start..].find('"')?;
    Some((&text[start..start + end], start + end + 1))
}

fn two_strings_after(text: &str, from: usize) -> Option<(&str, &str, usize)> {
    let (first, next) = one_string_after(text, from)?;
    let b = text.as_bytes();
    let mut i = next;
    while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
        i += 1;
    }
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    let (second, after) = one_string_after(text, i)?;
    Some((first, second, after))
}

fn known_paths() -> Vec<String> {
    let text =
        fs::read_to_string(workspace_root().join(
            "crates/neovm-core/src/emacs_core/tests/architecture/orphan_test_guard_known.tsv",
        ))
        .expect("orphan debt list");
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').next().unwrap_or(line).to_string())
        .collect()
}

#[test]
fn orphan_guard_names_a_file_no_facade_reaches() {
    let root = std::env::temp_dir().join(format!("neomacs-orphan-guard-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("core_tests")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"red_proof\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "#[path = \"../core_tests/wired.rs\"]\npub(crate) mod wired;\n",
    )
    .unwrap();
    fs::write(
        root.join("core_tests/wired.rs"),
        "#[path = \"sibling.rs\"]\npub mod sibling;\n",
    )
    .unwrap();
    fs::write(root.join("core_tests/sibling.rs"), "fn reached() {}\n").unwrap();
    fs::write(
        root.join("core_tests/red_proof_orphan.rs"),
        "fn orphan() {}\n",
    )
    .unwrap();
    let orphans = orphan_test_files(&root);
    let rendered = orphans.join("\n");
    assert!(
        rendered.contains("core_tests/red_proof_orphan.rs"),
        "guard failed to name the orphan: {rendered}"
    );
    assert!(
        !rendered.contains("sibling.rs"),
        "a #[path] sibling was reported as orphan: {rendered}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn orphan_guard_matches_recorded_debt() {
    let found = orphan_test_files(&workspace_root());
    let known = known_paths();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for path in &found {
        if !known.iter().any(|row| row == path) {
            added.push(path.clone());
        }
    }
    for path in &known {
        if !found.iter().any(|row| row == path) {
            removed.push(path.clone());
        }
    }
    assert!(
        added.is_empty() && removed.is_empty(),
        "orphan set changed\nadded:\n{}\nremoved:\n{}",
        added.join("\n"),
        removed.join("\n")
    );
}
