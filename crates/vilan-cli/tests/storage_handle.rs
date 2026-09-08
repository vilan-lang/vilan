//! The `std::storage` handle's runtime gate (tracker A57).
//!
//! `std::storage` shipped as six free functions over the two host stores, with
//! a missing key flattened to `""`. That is right for a one-key read and wrong
//! for the three things an app doing anything larger needs: counting the store,
//! walking its keys, and asking whether a key is present at all — the last of
//! which `!get(key).is_empty()` answers WRONGLY for a key whose stored value is
//! legitimately `""`. kolt hand-wrote the whole handle five FIXMEs deep to get
//! them (`src/lib/storage.vl:6-41`), and its `StorageKeyAllocator::seal` is the
//! customer: `delete_unknown_keys` walks `len()` down to 0 calling
//! `key_at(i - 1)`, and `initialize_unset_keys` needs `has`.
//!
//! None of that is observable by compiling — `len` is a host property, `key`
//! and `getItem` return null through a binding typed `str`, and whether the
//! `Option` wrappers really turn that null into `None` is a fact about a
//! running program. So these are e2e legs in `dom_events.rs`'s shape: a
//! browser-target app built with the real CLI, run under node against a Web
//! Storage stub that behaves as a browser's does — `getItem` and `key` return
//! **null**, not `undefined` and not `""`, which is the whole thing being
//! tested.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_storage_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Web Storage as a browser implements it, which is the point of the stub:
/// `getItem` on an absent key is `null` (not `undefined`, not `""`), `key(i)`
/// out of range is `null`, insertion order is the key order, `length` counts,
/// and every value is coerced to a string. `localStorage` and `sessionStorage`
/// are two separate instances, so a write to one must not be visible in the
/// other.
const STORAGE_STUB: &str = r#"class StubStorage {
    constructor() { this.map = new Map(); }
    get length() { return this.map.size; }
    key(index) {
        const keys = [ ...this.map.keys() ];
        return index >= 0 && index < keys.length ? keys[index] : null;
    }
    getItem(key) { return this.map.has(key) ? this.map.get(key) : null; }
    setItem(key, value) { this.map.set(String(key), String(value)); }
    removeItem(key) { this.map.delete(key); }
    clear() { this.map.clear(); }
}

global.localStorage = new StubStorage();
global.sessionStorage = new StubStorage();
global.window = { localStorage: global.localStorage, sessionStorage: global.sessionStorage };

let failures = 0;
const assert = (condition, message) => {
    if (!condition) { failures += 1; console.log("FAIL - " + message); }
    else console.log("ok   - " + message);
};
const done = () => process.exit(failures === 0 ? 0 : 1);
"#;

/// Builds `app.vl` for the browser with the real CLI and runs `harness.js` under
/// node, returning its stdout. Fails loudly with both streams.
fn build_and_run(tag: &str, app: &str, harness: &str) -> String {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"storage_{tag}\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n"
        ),
    );
    write(&dir, "app.vl", app);
    write(&dir, "harness.js", harness);

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "storage harness failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

// --- 1. Every method, including the missing key as `None` --------------------

/// The whole handle, on a store the program itself fills. The load-bearing line
/// is `empty` — a key stored as `""` — because it is the one case where `has`
/// and `get` disagree with the six free functions, and the reason the handle
/// exists at all.
const EVERY_METHOD: &str = r#"import std::dom::window;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::storage;

fun main() {
	let store = window().local_storage();
	print(i"len-empty {store.len()}");

	store.set("alpha", "one");
	store.set("empty", "");
	print(i"len {store.len()}");

	// A present key, and the key whose value is "" — `Some("")`, not `None`.
	let alpha = store.get("alpha").unwrap_or("MISSING");
	print(i"get-alpha {alpha}");
	let empty_is_some = store.get("empty").is_some();
	let empty_value = store.get("empty").unwrap_or("MISSING");
	print(i"get-empty-some {empty_is_some}");
	print(i"get-empty-value [{empty_value}]");

	// The missing key is `None` — the flattening "" lives only in the free form.
	let missing_is_some = store.get("nope").is_some();
	let free_missing = storage::get("nope");
	print(i"get-missing {missing_is_some}");
	print(i"free-missing [{free_missing}]");

	// Presence, including the case `!get(key).is_empty()` gets wrong.
	let has_alpha = store.has("alpha");
	let has_empty = store.has("empty");
	let has_missing = store.has("nope");
	print(i"has-alpha {has_alpha}");
	print(i"has-empty {has_empty}");
	print(i"has-missing {has_missing}");

	// Enumeration, counted DOWN as a removing sweep must be.
	mut index = store.len();
	for index > 0 {
		match store.key_at(index - 1) {
			Some(let key) => print(i"key {key}"),
			None => print("key MISSING"),
		}
		index -= 1;
	}
	let past_the_end = store.key_at(99).is_some();
	let before_the_start = store.key_at(0 - 1).is_some();
	print(i"key-at-out-of-range {past_the_end}");
	print(i"key-at-negative {before_the_start}");

	// kolt's `initialize_unset_keys`, on std surface alone. The second pass must
	// not rewrite the key, which is only true because `has` reads presence and
	// not emptiness.
	if !store.has("seeded") {
		store.set("seeded", "");
	}
	let seeded = store.has("seeded");
	print(i"seeded-once {seeded}");
	if !store.has("seeded") {
		store.set("seeded", "REWRITTEN");
	}
	let seeded_value = store.get("seeded").unwrap_or("MISSING");
	print(i"seeded-value [{seeded_value}]");

	store.remove("alpha");
	let removed = store.has("alpha");
	let after_remove = store.len();
	print(i"after-remove {removed} {after_remove}");
	store.remove("alpha");
	let after_absent_remove = store.len();
	print(i"remove-absent-is-a-no-op {after_absent_remove}");

	store.clear();
	let cleared = store.len();
	let cleared_key = store.key_at(0).is_some();
	print(i"after-clear {cleared}");
	print(i"after-clear-key-at {cleared_key}");
}
main();
"#;

#[test]
fn the_handle_counts_enumerates_presence_tests_and_clears() {
    let harness = format!(
        r#"{STORAGE_STUB}
require("./app.js");
assert(localStorage.length === 0, "the store is empty when the program is done");
done();
"#
    );
    let stdout = build_and_run("every_method", EVERY_METHOD, &harness);
    for claim in [
        "len-empty 0",
        "len 2",
        "get-alpha one",
        // The reason the handle exists: a stored "" is present.
        "get-empty-some true",
        "get-empty-value []",
        // …and a missing key is not, where the free form flattens it to "".
        "get-missing false",
        "free-missing []",
        "has-alpha true",
        "has-empty true",
        "has-missing false",
        // Counted down, so the order is the reverse of insertion.
        "key empty",
        "key alpha",
        "key-at-out-of-range false",
        "key-at-negative false",
        // `has` is what makes an "initialize if unset" pass idempotent — and it
        // would not be, if presence were `!get(key).is_empty()`.
        "seeded-once true",
        "seeded-value []",
        "after-remove false 2",
        "remove-absent-is-a-no-op 2",
        "after-clear 0",
        "after-clear-key-at false",
    ] {
        assert!(
            stdout.contains(claim),
            "the handle must print `{claim}`; got:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("key MISSING"),
        "every index below `len` must name a key; got:\n{stdout}"
    );
    assert!(
        stdout.contains("ok   - the store is empty when the program is done"),
        "`clear` must reach the host store; got:\n{stdout}"
    );
}

// --- 2. Two stores, and the free functions unchanged beside them -------------

/// `local_storage()` and `session_storage()` are two stores, not two names for
/// one — and the six free functions still address `localStorage` /
/// `sessionStorage` exactly as they did, flattening a missing key to `""`. The
/// handle is an addition, not a replacement.
const TWO_STORES: &str = r#"import std::dom::window;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::storage;

fun main() {
	let local = window().local_storage();
	let session = window().session_storage();

	local.set("k", "local-value");
	session.set("k", "session-value");
	let local_value = local.get("k").unwrap_or("MISSING");
	let session_value = session.get("k").unwrap_or("MISSING");
	print(i"local {local_value}");
	print(i"session {session_value}");

	// The free functions reach the same two host stores.
	let free_local = storage::get("k");
	let free_session = storage::session_get("k");
	print(i"free-local {free_local}");
	print(i"free-session {free_session}");

	// …and a write through a free function is visible through the handle.
	storage::set("via-free", "yes");
	storage::session_set("via-free", "yes");
	let via_free_local = local.get("via-free").unwrap_or("MISSING");
	let via_free_session = session.get("via-free").unwrap_or("MISSING");
	print(i"free-write-local {via_free_local}");
	print(i"free-write-session {via_free_session}");
	let local_len = local.len();
	let session_len = session.len();
	print(i"local-len {local_len} session-len {session_len}");

	// Clearing one store leaves the other whole.
	session.clear();
	let local_after = local.len();
	let session_after = session.len();
	print(i"after-session-clear {local_after} {session_after}");

	// The one-key form still flattens, which is its documented job.
	let free_absent = storage::get("absent");
	let handle_absent = local.get("absent").is_some();
	print(i"free-missing [{free_absent}]");
	print(i"handle-missing {handle_absent}");
}
main();
"#;

#[test]
fn the_two_stores_are_distinct_and_the_free_functions_still_reach_them() {
    let harness = format!(
        r#"{STORAGE_STUB}
require("./app.js");
assert(localStorage.getItem("k") === "local-value", "the local store kept its own value");
assert(sessionStorage.getItem("k") === null, "clearing the session store emptied it");
done();
"#
    );
    let stdout = build_and_run("two_stores", TWO_STORES, &harness);
    for claim in [
        "local local-value",
        "session session-value",
        "free-local local-value",
        "free-session session-value",
        "free-write-local yes",
        "free-write-session yes",
        "local-len 2 session-len 2",
        "after-session-clear 2 0",
        "free-missing []",
        "handle-missing false",
    ] {
        assert!(
            stdout.contains(claim),
            "the two-store exhibit must print `{claim}`; got:\n{stdout}"
        );
    }
    for claim in [
        "the local store kept its own value",
        "clearing the session store emptied it",
    ] {
        assert!(
            stdout.contains(&format!("ok   - {claim}")),
            "the two-store exhibit must hold `{claim}`; got:\n{stdout}"
        );
    }
}
