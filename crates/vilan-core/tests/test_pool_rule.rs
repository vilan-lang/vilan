//! N159: the rule for the pool size of a test that fans out inside itself
//! (`tests/test_pool/mod.rs`), and the scan that keeps a new pool from
//! slipping past it.

mod test_pool;

use std::path::Path;

#[test]
fn the_pool_is_what_the_test_asked_for_outside_nextest_and_two_under_it() {
    use test_pool::{DEFAULT_UNDER_NEXTEST, cap_from};
    assert_eq!(
        cap_from(None, false),
        None,
        "cargo test: the test's own width"
    );
    assert_eq!(cap_from(None, true), Some(DEFAULT_UNDER_NEXTEST));
    assert_eq!(DEFAULT_UNDER_NEXTEST, 2);
    assert_eq!(
        cap_from(Some("5"), true),
        Some(5),
        "VILAN_TEST_POOL wins under nextest"
    );
    assert_eq!(cap_from(Some(" 3 "), false), Some(3), "and outside it");
    assert_eq!(cap_from(Some("0"), true), Some(2), "zero is ignored");
    assert_eq!(cap_from(Some("many"), false), None, "garbage is ignored");
}

#[test]
fn map_answers_in_the_items_order_on_any_pool_width() {
    let items: Vec<usize> = (0..50).collect();
    for wanted in [1, 3, 16, 100] {
        let answers = test_pool::map(&items, wanted, |item| item * 2);
        assert_eq!(
            answers,
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
    }
    assert!(test_pool::map(&Vec::<u8>::new(), 4, |item| *item).is_empty());
}

/// A test that chunks its work over a worker pool (`.chunks(n.div_ceil(W))`) names
/// the width through `test_pool::workers`, or it is one more slot-sized test
/// holding sixteen threads. The scan reads every test source of the two crates
/// that run corpus legs.
#[test]
fn every_chunked_pool_in_the_suite_goes_through_the_cap() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut offenders = Vec::new();
    for krate in ["vilan-core", "vilan-cli"] {
        let tests = crates.join(krate).join("tests");
        let mut stack = vec![tests];
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(&directory)
                .into_iter()
                .flatten()
                .flatten()
            {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    if path.ends_with("test_pool_rule.rs") {
                        continue;
                    }
                    for (number, line) in text.lines().enumerate() {
                        let code = line.split("//").next().unwrap_or("");
                        if code.contains(".div_ceil(") && !code.contains("test_pool::") {
                            offenders.push(format!("{}:{}", path.display(), number + 1));
                        }
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a worker pool sized without `test_pool::workers` (N159): {offenders:#?}"
    );
}
