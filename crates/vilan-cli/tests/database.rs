//! End-to-end: `Database` is an affine `resource` that closes its `node:sqlite`
//! handle on drop (destruction.md §9). A file-backed database is written in an
//! inner scope that ends — the scope-end drop closes the handle — and again via
//! an explicit `drop(db)`, then the same file is reopened and read back. The
//! round-trip returning the written rows proves each writer's `drop` ran to
//! completion (it did not throw) and the file is usable afterward. The emitted
//! `db.close()` in the finally is pinned separately by the `db.vl` corpus golden;
//! this drives it against the real host database (node ships `node:sqlite`).

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for the test's project tree (and its `.db` files).
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_db_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Lays down a one-file project at `dir` and returns its manifest's directory.
/// Every migration pin below drives the same shape: `vilan run <dir>` with the
/// child's cwd pinned to `dir`, so the relative `app.db` never touches the repo.
fn migration_project(tag: &str, source: &str) -> PathBuf {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    write(&dir, "src/main.vl", source);
    dir
}

/// One `vilan run` of `dir`, with `variables` in the child's environment (the
/// phase switch: the same program boots twice against the same database file,
/// which is what "a re-run applies nothing" and both drift refusals mean).
fn run(dir: &Path, variables: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .args(["run", dir.to_str().unwrap()])
        .current_dir(dir);
    for (key, value) in variables {
        command.env(key, value);
    }
    command.output().expect("run vilan")
}

/// The run's stdout, asserting it exited 0.
fn stdout_of(output: &std::process::Output, what: &str) -> String {
    assert!(
        output.status.success(),
        "{what} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The run's stderr, asserting it FAILED — the refusals and the failing-step
/// stop are `panic`s, so the process must not survive them. `node:sqlite`'s
/// ExperimentalWarning also lands on stderr, which is why the assertions on it
/// are `contains` rather than equality.
fn stderr_of_failure(output: &std::process::Output, what: &str) -> String {
    assert!(
        !output.status.success(),
        "{what} was expected to stop the boot, but it exited 0:\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_dropped_database_closes_and_the_file_reopens() {
    let dir = temp_project("close");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\ntarget = \"node\"\n",
    );
    // `write_scoped` closes on return (scope-end drop); `write_early` closes at
    // `drop(db)` before it returns. `read_back` reopens the same file — reading
    // the written row proves the writer's teardown was clean.
    write(
        &dir,
        "src/main.vl",
        r#"import std::print;
import std::db::Database;
import std::drop::drop;
import std::option::Option::{ self, Some, None };

fun write_scoped(path: str) {
	let db = Database::open(path);
	db.exec("CREATE TABLE IF NOT EXISTS t (v TEXT)");
	db.prepare("INSERT INTO t VALUES (?)").run(["scope-end"]);
}

fun write_early(path: str) {
	let db = Database::open(path);
	db.exec("CREATE TABLE IF NOT EXISTS t (v TEXT)");
	db.prepare("INSERT INTO t VALUES (?)").run(["early"]);
	drop(db);
}

fun read_back(path: str): str {
	let db = Database::open(path);
	match db.prepare("SELECT v FROM t").first([]) {
		Some(let row) => row.text("v"),
		None => "MISSING",
	}
}

fun main() {
	write_scoped("scoped.db");
	print(read_back("scoped.db"));
	write_early("early.db");
	print(read_back("early.db"));
}
main();
"#,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", dir.to_str().unwrap()])
        // Relative `.db` paths resolve against the child's cwd — pin it to the
        // temp project so the files never touch the repo.
        .current_dir(&dir)
        .output()
        .expect("run vilan");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `node:sqlite` prints an ExperimentalWarning to stderr — expected, not a
    // failure — so the assertion is on stdout and the exit status only.
    assert!(
        output.status.success(),
        "vilan run failed:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout, "scope-end\nearly\n",
        "the reopen-and-read round-trip did not return the written rows — a writer's drop did not close cleanly"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Migrations (db-migrations.md, kolt.local 036) ----------------------------
//
// `Database.migrate` applies named steps in list order and records each in the
// `vilan_migrations` table it owns. Every pin below drives a REAL SQLite file
// in its own temp directory through the CLI, because the whole surface is about
// what survives between two boots — a re-run applying nothing, a failure
// leaving no record, a drifted database being refused untouched. None of that
// is observable inside one process's memory.

#[test]
fn migrations_apply_once_and_a_re_run_applies_nothing() {
    let dir = migration_project(
        "migrate_idempotent",
        r#"import std::print;
import std::db::{ Database, Migration };
import std::display::Display;

fun steps(): List<Migration> {
	[
		Migration { name = "001-create-task", sql = "CREATE TABLE task (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)" },
		Migration { name = "002-task-description", sql = "ALTER TABLE task ADD COLUMN description TEXT" },
	]
}

fun main() {
	let db = Database::open("app.db");
	let applied = db.migrate(steps()).join(",");
	print(i"applied: {applied}");

	mut recorded: List<str> = [];
	mut stamped = 0;
	for row in db.prepare("SELECT name, applied_at_ms FROM vilan_migrations ORDER BY rowid").all([]) {
		recorded.push(row.text("name"));
		if row.big_integer("applied_at_ms") > 0i53 {
			stamped += 1;
		}
	}
	let names = recorded.join(",");
	print(i"records: {names}");
	print(i"stamped: {stamped}");

	// The column 002 added must really be there: a migration that records
	// itself without changing the schema is the bug this surface exists for.
	db.prepare("INSERT INTO task (name, description) VALUES (?, ?)").run(["write docs", "the 002 column"]);
	let tasks = db.prepare("SELECT * FROM task").all([]).len();
	print(i"tasks: {tasks}");
}
main();
"#,
    );

    let first = run(&dir, &[]);
    assert_eq!(
        stdout_of(&first, "the first migrate"),
        "applied: 001-create-task,002-task-description\n\
         records: 001-create-task,002-task-description\n\
         stamped: 2\n\
         tasks: 1\n",
        "a fresh database did not apply both steps in order, record both with a timestamp, and end up with 002's column"
    );

    // A second BOOT against the same file — the steady state a server is in on
    // every restart after the first.
    let second = run(&dir, &[]);
    assert_eq!(
        stdout_of(&second, "the re-run"),
        "applied: \n\
         records: 001-create-task,002-task-description\n\
         stamped: 2\n\
         tasks: 2\n",
        "the re-run was not a no-op — `migrate` is idempotent by construction, so the applied list must be empty and the table unchanged"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_extended_migration_list_applies_only_the_tail() {
    let dir = migration_project(
        "migrate_tail",
        r#"import std::print;
import std::db::{ Database, Migration };
import std::display::Display;
import std::option::Option::{ self, Some, None };
import std::process::env;

fun main() {
	let db = Database::open("app.db");
	mut steps: List<Migration> = [
		Migration { name = "001-a", sql = "CREATE TABLE a (x TEXT)" },
		Migration { name = "002-b", sql = "CREATE TABLE b (x TEXT)" },
	];
	match env("EXTENDED") {
		Some(let _value) => steps.push(Migration { name = "003-c", sql = "CREATE TABLE c (x TEXT)" }),
		None => {},
	}
	let applied = db.migrate(steps).join(",");
	print(i"applied: {applied}");
}
main();
"#,
    );

    assert_eq!(
        stdout_of(&run(&dir, &[]), "the first migrate"),
        "applied: 001-a,002-b\n"
    );
    assert_eq!(
        stdout_of(&run(&dir, &[("EXTENDED", "1")]), "the extended migrate"),
        "applied: 003-c\n",
        "appending a step must apply only the tail — the two already recorded steps must not run again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failing_migration_stops_the_boot_naming_it_and_records_nothing() {
    let dir = migration_project(
        "migrate_failure",
        r#"import std::print;
import std::db::{ Database, Migration };
import std::display::Display;
import std::option::Option::{ self, Some, None };
import std::process::env;

fun main() {
	let db = Database::open("app.db");
	match env("INSPECT") {
		// The post-mortem boot: what the failed run left behind. It does NOT
		// migrate — it reads the schema and the record directly.
		Some(let _value) => {
			mut tables: List<str> = [];
			for row in db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all([]) {
				tables.push(row.text("name"));
			}
			let table_names = tables.join(",");
			print(i"tables: {table_names}");

			mut recorded: List<str> = [];
			for row in db.prepare("SELECT name FROM vilan_migrations ORDER BY rowid").all([]) {
				recorded.push(row.text("name"));
			}
			let recorded_names = recorded.join(",");
			print(i"records: {recorded_names}");
		},
		// 002's SQL creates a table and THEN references one that does not
		// exist: the step half-succeeds before it throws, which is exactly the
		// case the per-step transaction has to undo.
		None => {
			let _ = db.migrate([
				Migration { name = "001-task", sql = "CREATE TABLE task (id INTEGER PRIMARY KEY)" },
				Migration { name = "002-bad", sql = "CREATE TABLE half (x TEXT); INSERT INTO taks VALUES (1)" },
				Migration { name = "003-never", sql = "CREATE TABLE never (x TEXT)" },
			]);
			print("the boot survived a failing migration");
		},
	}
}
main();
"#,
    );

    let failure = run(&dir, &[]);
    let stderr = stderr_of_failure(&failure, "a migration whose SQL is wrong");
    assert!(
        stderr.contains("migration '002-bad' failed and was not applied"),
        "the stop must name the step that failed — it is the only thing that says which file to open. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("no such table: taks"),
        "the stop must quote the host's own message — it is the only thing that says what was wrong with the SQL. stderr:\n{stderr}"
    );

    assert_eq!(
        stdout_of(&run(&dir, &[("INSPECT", "1")]), "the post-mortem boot"),
        "tables: task,vilan_migrations\n\
         records: 001-task\n",
        "the failed step must leave NOTHING: no `half` table (its transaction rolled back), no record of `002-bad`, and no `never` table (the run stopped). `001-task` stays applied and recorded, which is what lets a re-run resume at the failure"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_database_ahead_of_the_migration_list_is_refused() {
    let dir = migration_project(
        "migrate_drift_ahead",
        r#"import std::print;
import std::db::{ Database, Migration };
import std::display::Display;
import std::option::Option::{ self, Some, None };
import std::process::env;

fun main() {
	let db = Database::open("app.db");
	mut steps: List<Migration> = [ Migration { name = "001-a", sql = "CREATE TABLE a (x TEXT)" } ];
	match env("NEWER_BUILD") {
		Some(let _value) => steps.push(Migration { name = "002-b", sql = "CREATE TABLE b (x TEXT)" }),
		None => {},
	}
	let applied = db.migrate(steps).join(",");
	print(i"applied: {applied}");
}
main();
"#,
    );

    // The newer build migrates the shared database, then the older one — whose
    // queries predate 002 — boots against it.
    assert_eq!(
        stdout_of(&run(&dir, &[("NEWER_BUILD", "1")]), "the newer build"),
        "applied: 001-a,002-b\n"
    );

    let refusal = run(&dir, &[]);
    let stderr = stderr_of_failure(&refusal, "an older build over a newer schema");
    assert!(
        stderr.contains(
            "the database has migration '002-b' applied, but it is not among the 1 migrations given"
        ),
        "drift (a) must name the recorded step the list is missing. stderr:\n{stderr}"
    );

    // And it refused without touching anything: the newer build boots again
    // and finds nothing left to do.
    assert_eq!(
        stdout_of(&run(&dir, &[("NEWER_BUILD", "1")]), "the newer build again"),
        "applied: \n",
        "the refusal must leave the database exactly as it found it"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_migration_inserted_before_the_applied_history_is_refused() {
    let dir = migration_project(
        "migrate_drift_inserted",
        r#"import std::print;
import std::db::{ Database, Migration };
import std::display::Display;
import std::option::Option::{ self, Some, None };
import std::process::env;

fun main() {
	let db = Database::open("app.db");
	mut steps: List<Migration> = [];
	steps.push(Migration { name = "001-a", sql = "CREATE TABLE a (x TEXT)" });
	match env("MERGED") {
		Some(let _value) => steps.push(Migration { name = "001-5-inserted", sql = "CREATE TABLE inserted (x TEXT)" }),
		None => {},
	}
	steps.push(Migration { name = "002-b", sql = "CREATE TABLE b (x TEXT)" });
	let applied = db.migrate(steps).join(",");
	print(i"applied: {applied}");
}
main();
"#,
    );

    assert_eq!(
        stdout_of(&run(&dir, &[]), "the first migrate"),
        "applied: 001-a,002-b\n"
    );

    // The merge orders a colleague's new step BEHIND one this database already
    // ran. Applying it here would produce a schema no fresh database can reach.
    let refusal = run(&dir, &[("MERGED", "1")]);
    let stderr = stderr_of_failure(&refusal, "a step inserted into the past");
    assert!(
        stderr.contains(
            "migration '001-5-inserted' is not applied, but '002-b' after it in the list is"
        ),
        "drift (b) must name both the unapplied step and the applied one it sits behind. stderr:\n{stderr}"
    );

    assert_eq!(
        stdout_of(&run(&dir, &[]), "the unmerged list again"),
        "applied: \n",
        "the refusal must leave the database exactly as it found it — `inserted` must never have been created"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
