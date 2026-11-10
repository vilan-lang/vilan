import { DatabaseSync } from "node:sqlite";
function __db_all(statement, parameters) {
	return statement.all(...__db_bind(parameters));
}
function __db_bind(parameters) {
	return parameters.map((value, index) => {
		switch (typeof value) {
			case "number":
			case "bigint":
			case "string":
				return value;
			case "boolean":
				return value ? 1 : 0;
		}
		if (value === null || value instanceof Uint8Array) {
			return value;
		}
		const shape = Array.isArray(value) ? "a struct, tuple, list or enum value" : typeof value === "function" ? "a closure" : value === undefined ? "an absent value" : "a host object";
		throw new Error("std::db: cannot bind the parameter at index " + index + " of " + parameters.length + ": " + shape + " is not a SQLite value (bind a number, a str, a bool, or an Option of one)");
	});
}
function __db_close(database) {
	database.close();
}
function __db_column(row, name) {
	return row[name];
}
function __db_get(statement, parameters) {
	const row = statement.get(...__db_bind(parameters));
	return row === undefined ? [ 1 ] : [ 0, row ];
}
function __db_is_null(row, name) {
	return row[name] === null || row[name] === undefined;
}
function __db_run(statement, parameters) {
	const result = statement.run(...__db_bind(parameters));
	return Number(result.lastInsertRowid ?? 0);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function drop(self) {
	__db_close(self);
}
function run(self, parameters) {
	return __db_run(self, parameters);
}
function all(self, parameters) {
	return __db_all(self, parameters);
}
function first(self, parameters) {
	return __db_get(self, parameters);
}
function refuse_null(row4, name, reader, caller) {
	if (__db_is_null(row4, name)) {
		(() => {
			throw __panic("std::db: column \'" + name + "\' is NULL (or not in the row) and `Row::" + reader + "` reads a value; read a nullable column with `Row::optional_" + reader + "`", caller);
		})();
	}
}
function text(self, name, caller) {
	refuse_null(self, name, "text", caller);
	return __db_column(self, name);
}
function integer(self, name, caller) {
	refuse_null(self, name, "integer", caller);
	return __db_column(self, name);
}
function big_integer(self, name, caller) {
	refuse_null(self, name, "big_integer", caller);
	return __db_column(self, name);
}
function $e($f) {
	drop($f);
}
const db = new DatabaseSync(":memory:");
try {
	db.exec("CREATE TABLE task (id INTEGER PRIMARY KEY, title TEXT, done INTEGER)");
	const insert = db.prepare("INSERT INTO task (title, done) VALUES (?, ?)");
	run(insert, [ "write the pilot", 0 ]);
	run(insert, [ "ship std::db", 1 ]);
	const open_tasks = db.prepare("SELECT title FROM task WHERE done = ? ORDER BY id");
	for (const row of all(open_tasks, [ 0 ])) {
		console.log("todo: " + text(row, "title", "db.vl:18:22"));
	}
	const $a = first(db.prepare("SELECT COUNT(*) AS n FROM task"), [  ]);
	let $b = null;
	if ($a[0] === 0) {
		const row2 = $a[1];
		$b = console.log("" + integer(row2, "n", "db.vl:21:33") + " tasks total");
	} else {
		$b = console.log("no rows");
	}
	$b;
	db.exec("CREATE TABLE stamp (id INTEGER PRIMARY KEY, at INTEGER)");
	run(db.prepare("INSERT INTO stamp (at) VALUES (?)"), [ 1720656000000 ]);
	const $c = first(db.prepare("SELECT at FROM stamp"), [  ]);
	let $d = null;
	if ($c[0] === 0) {
		const row3 = $c[1];
		$d = console.log("stamp at " + big_integer(row3, "at", "db.vl:29:42"));
	} else {
		$d = console.log("no stamp");
	}
	$d;
} finally {
	$e(db);
}
//# sourceMappingURL=db.mjs.map
