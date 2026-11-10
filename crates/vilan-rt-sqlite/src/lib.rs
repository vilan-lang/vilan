//! `vilan-rt-sqlite` — `std::db` on the native backend (tracker F18 slice 2;
//! Order 39's **R1**).
//!
//! # What this is a twin OF
//!
//! `vilan/std/src/db.vl` binds `node:sqlite`'s `DatabaseSync`:
//! `open`/`exec`/`prepare` on the database, `run`/`all`/`get` on a prepared
//! statement, `row[name]` through the typed column accessors, a guarded
//! `exec`/`run` for the migrator, and `close()` from `Database`'s `Drop`.
//! Those ten ARE the contract; each item below names the one it exists for.
//!
//! # Why a crate of its own
//!
//! `vilan-rt` takes no crates.io dependencies and that is a ruling, not an
//! accident — it is what lets `--backend rust` work from a machine with
//! nothing but a Rust toolchain. SQLite does not fit inside it. So the
//! dependency lives here, and the cargo project `vilan build --backend rust`
//! writes names this crate ONLY when the program reaches `std::db`.
//!
//! # The lifetime problem, and what it forced
//!
//! `rusqlite::Statement<'conn>` and `rusqlite::Row<'stmt>` both BORROW what
//! they came from; vilan's `Statement` and `Row` are free-standing handles a
//! program can hold for as long as it likes. So neither is wrapped directly:
//!
//! - a [`Statement`] holds the connection and the SQL TEXT, and prepares
//!   through rusqlite's own statement cache at each use — the cache is what
//!   keeps that from being a re-parse per call, and it is the same object
//!   `node:sqlite` hands back from `prepare`;
//! - a [`Row`] is MATERIALIZED at the query, as its column names paired with
//!   their values, which is what `row[name]` reads on the other backend anyway.
//!
//! # Failure
//!
//! `node:sqlite` THROWS, and a throw nothing catches ends the program with its
//! message. So a failure here is [`vilan_rt::panic_with`] carrying SQLite's own
//! sentence, and the two guarded entry points ([`exec_guarded`],
//! [`run_guarded`]) answer `Option<Str>` exactly as `transformer.rs`'s
//! `__db_exec_guarded`/`__db_run_guarded` do.

use std::cell::RefCell;
use std::rc::Rc;

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use vilan_rt::bytes::Bytes;
use vilan_rt::{Any, Js, Json, Str, str_new};

/// `DatabaseSync` — an open connection. A vilan `resource`: it moves rather
/// than copies, and its `Drop` closes the handle at the owner's scope end.
///
/// The `Rc` is not a second owner of the RESOURCE — vilan's move semantics are
/// what say there is one — it is what lets a [`Statement`] outlive the
/// expression that made it while still naming the connection it was prepared
/// against, which is the same thing `node:sqlite` does with its own handle.
#[derive(Clone)]
pub struct Database(Rc<RefCell<Option<Connection>>>);

impl Database {
    /// `new DatabaseSync(path)` — opens, creating the file if it is missing.
    /// `":memory:"` is an ephemeral in-process store, as it is there.
    pub fn open(path: Str) -> Database {
        match Connection::open(&*path) {
            Ok(connection) => Database(Rc::new(RefCell::new(Some(connection)))),
            Err(error) => fail(&format!("cannot open the database at {path}"), &error),
        }
    }

    /// `database.exec(sql)` — DDL, pragmas, one-off statements with no results.
    pub fn exec(&self, sql: Str) {
        if let Some(reason) = self.exec_guarded(sql) {
            vilan_rt::panic_with(&reason);
        }
    }

    /// `database.prepare(sql)`. Nothing is prepared HERE: the text is kept and
    /// prepared through the connection's statement cache at each use (see this
    /// module's header). A syntactically invalid statement therefore surfaces
    /// at the first `run`/`all`/`first` rather than at `prepare`, which is the
    /// one place this diverges from `node:sqlite` — written down rather than
    /// paid for with a second prepared handle nothing can hold.
    pub fn prepare(&self, sql: Str) -> Statement {
        Statement {
            database: self.clone(),
            sql,
        }
    }

    /// `__db_exec_guarded` — `None` on success, `Some(message)` on a throw.
    pub fn exec_guarded(&self, sql: Str) -> Option<Str> {
        self.with(|connection| match connection.execute_batch(&sql) {
            Ok(()) => None,
            Err(error) => Some(str_new(&error.to_string())),
        })
    }

    /// `__db_close` — reachable only from `Database`'s `Drop` in `db.vl`, so a
    /// handle closes exactly once. Idempotent here anyway: a second close finds
    /// the slot already empty.
    pub fn close(&self) {
        let taken = self.0.borrow_mut().take();
        if let Some(connection) = taken {
            // A close that fails has nothing to report to — the owner's scope
            // is already ending — so the connection is dropped either way.
            let _ = connection.close();
        }
    }

    fn with<T>(&self, body: impl FnOnce(&Connection) -> T) -> T {
        let borrowed = self.0.borrow();
        match borrowed.as_ref() {
            Some(connection) => body(connection),
            None => vilan_rt::panic_with(
                "this `Database` is closed: its owner's scope ended (or `drop(db)` ran) and \
                 `Drop` closed the handle",
            ),
        }
    }
}

impl PartialEq for Database {
    fn eq(&self, other: &Database) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Js for Database {
    fn js(&self) -> String {
        vilan_rt::panic_with(
            "printing a `Database` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

impl Json for Database {
    /// `JSON.stringify` of a host handle answers `{}` — it has no own
    /// enumerable properties — which is what the executor's four handles and
    /// the five HTTP ones already answer.
    fn json(&self) -> String {
        "{}".to_string()
    }
}

/// A prepared statement. See this module's header for why it holds text rather
/// than a `rusqlite::Statement`.
#[derive(Clone)]
pub struct Statement {
    database: Database,
    sql: Str,
}

impl Statement {
    /// `__db_run` — executes with `parameters` bound positionally and answers
    /// the inserted rowid (`0` when the statement inserts nothing), which is
    /// what `lastInsertRowid ?? 0` answers there.
    pub fn run(&self, parameters: Vec<Any>) -> i32 {
        match self.run_checked(parameters) {
            Ok(rowid) => rowid,
            Err(error) => fail(&format!("cannot run {}", self.sql), &error),
        }
    }

    /// `__db_run_guarded` — the migrator's guard.
    pub fn run_guarded(&self, parameters: Vec<Any>) -> Option<Str> {
        match self.run_checked(parameters) {
            Ok(_) => None,
            Err(error) => Some(str_new(&error.to_string())),
        }
    }

    fn run_checked(&self, parameters: Vec<Any>) -> rusqlite::Result<i32> {
        self.database.with(|connection| {
            let bound = bind(parameters);
            let mut statement = connection.prepare_cached(&self.sql)?;
            statement.execute(rusqlite::params_from_iter(bound.iter()))?;
            Ok(connection.last_insert_rowid() as i32)
        })
    }

    /// `__db_all` — every matching row, materialized.
    pub fn all(&self, parameters: Vec<Any>) -> Vec<Row> {
        match self.query(parameters) {
            Ok(rows) => rows,
            Err(error) => fail(&format!("cannot run {}", self.sql), &error),
        }
    }

    /// `__db_get` — the first matching row, or `None`.
    pub fn first(&self, parameters: Vec<Any>) -> Option<Row> {
        self.all(parameters).into_iter().next()
    }

    fn query(&self, parameters: Vec<Any>) -> rusqlite::Result<Vec<Row>> {
        self.database.with(|connection| {
            let bound = bind(parameters);
            let mut statement = connection.prepare_cached(&self.sql)?;
            let names: Vec<Str> = statement.column_names().map(str_new).collect::<Vec<_>>();
            let mut rows = statement.query(rusqlite::params_from_iter(bound.iter()))?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                let mut columns = Vec::with_capacity(names.len());
                for (index, name) in names.iter().enumerate() {
                    columns.push((Str::clone(name), read(row.get_ref(index)?)));
                }
                out.push(Row(Rc::new(columns)));
            }
            Ok(out)
        })
    }
}

impl PartialEq for Statement {
    fn eq(&self, other: &Statement) -> bool {
        self.database == other.database && self.sql == other.sql
    }
}

impl Js for Statement {
    fn js(&self) -> String {
        vilan_rt::panic_with(
            "printing a `Statement` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

impl Json for Statement {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

/// One result row, materialized as its column names paired with their values —
/// which is what `row[name]` reads on the other backend.
#[derive(Clone, PartialEq)]
pub struct Row(Rc<Vec<(Str, Any)>>);

impl Row {
    /// `__db_column` at `str`. The four typed column readers share one host
    /// helper and the vilan signature supplies the type, so each reader here
    /// coerces the same way `String(row[name])` would. A NULL never reaches a
    /// typed reader: `db.vl`'s `Row` members refuse it first, naming the column
    /// (F149, ruling S5), so the arm here is that refusal's backstop.
    pub fn text(&self, name: Str) -> Str {
        match self.at(&name) {
            Any::Text(text) => text,
            Any::Null => refuse_null(&name),
            Any::Integer(value) => str_new(&value.to_string()),
            Any::Float(value) => str_new(&vilan_rt::js_number(value)),
            Any::Bool(value) => str_new(if value { "true" } else { "false" }),
            // A BLOB read as text is the bytes decoded, as `String(buffer)`
            // decodes them there.
            Any::Blob(bytes) => str_new(&String::from_utf8_lossy(&bytes.as_slice())),
        }
    }

    /// `__db_bytes` — a BLOB column as the `Bytes` it holds (M195), and any
    /// other column as the UTF-8 of its text, SQLite's own
    /// `CAST(value AS BLOB)`, which is what the JS helper answers too.
    pub fn bytes(&self, name: Str) -> Bytes {
        match self.at(&name) {
            Any::Blob(bytes) => bytes,
            Any::Null => refuse_null(&name),
            _ => Bytes::from_vec(self.text(name).as_bytes().to_vec()),
        }
    }

    /// `__db_column` at `i32`.
    pub fn integer(&self, name: Str) -> i32 {
        self.number(&name) as i32
    }

    /// `__db_column` at `i53`, whose native width is `i64`.
    pub fn big_integer(&self, name: Str) -> i64 {
        self.number(&name) as i64
    }

    /// `__db_column` at `f64`.
    pub fn real(&self, name: Str) -> f64 {
        self.number(&name)
    }

    /// `__db_is_null` — `row[name] === null || === undefined`, so a column the
    /// row does not HAVE answers true, as an absent property does there.
    pub fn is_null(&self, name: Str) -> bool {
        match self.0.iter().find(|(column, _)| *column == name) {
            Some((_, Any::Null)) | None => true,
            Some(_) => false,
        }
    }

    fn at(&self, name: &str) -> Any {
        self.0
            .iter()
            .find(|(column, _)| &**column == name)
            .map(|(_, value)| value.clone())
            .unwrap_or(Any::Null)
    }

    fn number(&self, name: &str) -> f64 {
        match self.at(name) {
            Any::Null => refuse_null(name),
            Any::Integer(value) => value as f64,
            Any::Float(value) => value,
            Any::Bool(value) => {
                if value {
                    1.0
                } else {
                    0.0
                }
            }
            // `Number("12")` is `12` — the coercion the JS reader performs on
            // the same column.
            Any::Text(text) => text.trim().parse::<f64>().unwrap_or(f64::NAN),
            Any::Blob(_) => f64::NAN,
        }
    }
}

impl Js for Row {
    /// node prints a `node:sqlite` row as the plain object it is. The columns
    /// are in the query's own order, which is what `Object.keys` answers there.
    fn js(&self) -> String {
        if self.0.is_empty() {
            return "{}".to_string();
        }
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|(name, value)| format!("{name}: {}", value.js_nested()))
            .collect();
        format!("{{ {} }}", parts.join(", "))
    }
}

impl Json for Row {
    fn json(&self) -> String {
        let parts: Vec<String> = self
            .0
            .iter()
            .map(|(name, value)| format!("{}:{}", vilan_rt::json_string(name), value.json()))
            .collect();
        format!("{{{}}}", parts.join(","))
    }
}

/// A bind list, as `rusqlite` wants to see it.
///
/// `vilan_rt::Any`'s arms are SQLite's five storage classes plus `bool`, which
/// binds as 0 or 1. Building owned
/// `Value`s rather than implementing `ToSql` on a borrowed view is the shorter
/// road and costs one small allocation per TEXT parameter per call — against a
/// statement prepare and a query, which is not where a bind list's cost is.
fn bind(parameters: Vec<Any>) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    parameters
        .into_iter()
        .map(|parameter| match parameter {
            Any::Null => Value::Null,
            Any::Bool(value) => Value::Integer(value as i64),
            Any::Integer(value) => Value::Integer(value),
            Any::Float(value) => Value::Real(value),
            Any::Text(value) => Value::Text(value.to_string()),
            Any::Blob(value) => Value::Blob(value.to_vec()),
        })
        .collect()
}

/// One column, read out of a borrowed row into an owned [`Any`].
fn read(value: ValueRef<'_>) -> Any {
    match value {
        ValueRef::Null => Any::Null,
        ValueRef::Integer(value) => Any::Integer(value),
        ValueRef::Real(value) => Any::Float(value),
        ValueRef::Text(bytes) => Any::Text(str_new(&String::from_utf8_lossy(bytes))),
        // M195: a BLOB stays one, as node answers a `Uint8Array`; `Row::bytes`
        // reads it back.
        ValueRef::Blob(bytes) => Any::Blob(Bytes::from_vec(bytes.to_vec())),
    }
}

/// A typed reader met a NULL — the backstop of `db.vl`'s own refusal, which
/// runs first and says which reader to use instead.
fn refuse_null(name: &str) -> ! {
    vilan_rt::panic_with(&format!("std::db: column '{name}' is NULL"))
}

/// A failure, in the shape a `node:sqlite` throw takes: the driver's own
/// sentence, with the operation that raised it in front.
fn fail(what: &str, error: &rusqlite::Error) -> ! {
    vilan_rt::panic_with(&format!("{what}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened() -> Database {
        let database = Database::open(str_new(":memory:"));
        database.exec(str_new(
            "CREATE TABLE account (id INTEGER PRIMARY KEY, name TEXT NOT NULL, score REAL, note TEXT)",
        ));
        database
    }

    #[test]
    fn a_statement_runs_queries_and_reads_its_columns_back_typed() {
        let database = opened();
        let insert = database.prepare(str_new("INSERT INTO account (name, score) VALUES (?, ?)"));
        assert_eq!(insert.run(vec![Any::from("ada"), Any::from(1.5)]), 1);
        assert_eq!(insert.run(vec![Any::from("grace"), Any::from(2.5)]), 2);
        let all = database
            .prepare(str_new(
                "SELECT id, name, score, note FROM account ORDER BY id",
            ))
            .all(Vec::new());
        assert_eq!(all.len(), 2);
        assert_eq!(&*all[0].text(str_new("name")), "ada");
        assert_eq!(all[0].integer(str_new("id")), 1);
        assert_eq!(all[0].real(str_new("score")), 1.5);
        // A NULL column reads as null, and a column the row does not have
        // reads as null too — `row[name] === undefined` answers the same
        // question there.
        assert!(all[0].is_null(str_new("note")));
        assert!(all[0].is_null(str_new("nothing_like_this")));
        assert!(!all[0].is_null(str_new("name")));
        let first = database
            .prepare(str_new("SELECT name FROM account WHERE name = ?"))
            .first(vec![Any::from("grace")]);
        assert_eq!(
            first.map(|row| row.text(str_new("name"))),
            Some(str_new("grace"))
        );
        let missing = database
            .prepare(str_new("SELECT name FROM account WHERE name = ?"))
            .first(vec![Any::from("nobody")]);
        assert!(missing.is_none());
    }

    /// The migrator's guard: `None` on success, the driver's own sentence on a
    /// failure — and NOT a panic, which is what makes a failed step reportable.
    #[test]
    fn the_guarded_entry_points_answer_a_message_instead_of_throwing() {
        let database = opened();
        assert!(database.exec_guarded(str_new("SELECT 1")).is_none());
        let refused = database
            .exec_guarded(str_new("CREATE TABLE account (id INTEGER)"))
            .expect("a second table of the same name is an error");
        assert!(
            refused.contains("already exists"),
            "the driver's own sentence: {refused}"
        );
        let insert = database.prepare(str_new("INSERT INTO account (name) VALUES (?)"));
        assert!(insert.run_guarded(vec![Any::from("ada")]).is_none());
        // NOT NULL on `name`, so a null bind is refused rather than stored.
        let refused = insert
            .run_guarded(vec![Any::Null])
            .expect("a NOT NULL violation is an error");
        assert!(refused.contains("NOT NULL"), "{refused}");
    }

    /// M195: a `Bytes` binds as a BLOB and reads back as the same bytes — a
    /// NUL and a high byte included — where it was read as lossy TEXT; a TEXT
    /// column read through `bytes` is its UTF-8, SQLite's own cast.
    #[test]
    fn a_blob_round_trips_through_bind_and_read() {
        let database = Database::open(str_new(":memory:"));
        database.exec(str_new("CREATE TABLE blobs (data BLOB, label TEXT)"));
        let stored = Bytes::from_vec(vec![0, 1, 255, 0x80, 0]);
        database
            .prepare(str_new("INSERT INTO blobs (data, label) VALUES (?, ?)"))
            .run(vec![Any::from(stored), Any::from("hé")]);
        let row = database
            .prepare(str_new(
                "SELECT data, typeof(data) AS kind, label FROM blobs",
            ))
            .first(Vec::new())
            .expect("the row just inserted");
        assert_eq!(
            row.bytes(str_new("data")).to_vec(),
            vec![0, 1, 255, 0x80, 0]
        );
        assert_eq!(&*row.text(str_new("kind")), "blob");
        assert_eq!(
            row.bytes(str_new("label")).to_vec(),
            "hé".as_bytes().to_vec()
        );
    }

    /// F149 / S5: a typed reader never answers a NULL as `0` or `""`; `db.vl`
    /// refuses first, and these arms are its backstop.
    #[test]
    fn a_typed_reader_refuses_a_null() {
        let database = opened();
        database
            .prepare(str_new("INSERT INTO account (name) VALUES (?)"))
            .run(vec![Any::from("ada")]);
        let row = database
            .prepare(str_new("SELECT score, note FROM account"))
            .first(Vec::new())
            .expect("the row just inserted");
        for read in [
            Box::new(|row: &Row| {
                row.real(str_new("score"));
            }) as Box<dyn Fn(&Row)>,
            Box::new(|row: &Row| {
                row.text(str_new("note"));
            }),
            Box::new(|row: &Row| {
                row.bytes(str_new("note"));
            }),
        ] {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&row)));
            assert!(outcome.is_err(), "a NULL is refused");
        }
    }

    /// A closed database answers the sentence rather than a use-after-free, and
    /// closing twice is not an error — `Drop` is the only caller and the early
    /// `drop(db)` form reaches the same code.
    #[test]
    fn a_closed_database_names_itself_and_closes_idempotently() {
        let database = opened();
        database.close();
        database.close();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            database.exec(str_new("SELECT 1"))
        }));
        assert!(outcome.is_err(), "a closed handle refuses");
    }
}
