//! B519's generic check over emitted JavaScript: every A134 mirror table a
//! file READS is DECLARED in it.
//!
//! `[service(Client)]` writes one module-level `let __mirrors_<Client>_<method>`
//! per handle stub, and the stub reads it by name. A table the stub reads and
//! the file never declares is a `ReferenceError` at the stub's first call — in
//! the browser, after the build said nothing. Stated over the emitted text
//! rather than over one service, so it holds for every leg a gate builds.

use std::collections::BTreeSet;
use std::path::Path;

/// Every `__mirrors_*` identifier `js` names, and the subset it declares
/// (`const`/`let`/`var` immediately before the name).
pub fn mirror_tables(js: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut used = BTreeSet::new();
    let mut declared = BTreeSet::new();
    for (start, _) in js.match_indices("__mirrors_") {
        let name: String = js[start..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        let before = js[..start].trim_end();
        if ["const", "let", "var"]
            .iter()
            .any(|keyword| before.ends_with(keyword))
        {
            declared.insert(name.clone());
        }
        used.insert(name);
    }
    (used, declared)
}

/// The mirror tables `js` reads and never declares, sorted.
pub fn dangling(js: &str) -> Vec<String> {
    let (used, declared) = mirror_tables(js);
    used.difference(&declared).cloned().collect()
}

/// Every emitted `.js`/`.mjs` under `root` that reads a mirror table it does
/// not declare, with the dangling names — one line per file, empty when the
/// tree is clean.
pub fn dangling_in_tree(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_script = matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("js" | "mjs")
            );
            if !is_script {
                continue;
            }
            let Ok(js) = std::fs::read_to_string(&path) else {
                continue;
            };
            let names = dangling(&js);
            if !names.is_empty() {
                found.push(format!("{}: {names:?}", path.display()));
            }
        }
    }
    found.sort();
    found
}
