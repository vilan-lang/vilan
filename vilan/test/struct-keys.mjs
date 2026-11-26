function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __key_at(value, key) {
	const parts = key[0][0];
	for (let at = 0; at < parts.length; at++) value = value[parts[at][1]];
	return value;
}
function __key_put(value, key, field) {
	const parts = key[0][0];
	const last = parts.length - 1;
	for (let at = 0; at < last; at++) value = value[parts[at][1]];
	return value[parts[last][1]] = field;
}
function __key_view(value, key) {
	const parts = key[0][0];
	const last = parts.length - 1;
	for (let at = 0; at < last; at++) value = value[parts[at][1]];
	return [ value, parts[last][1] ];
}
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_insert(table, hash, key, value) {
	__nmap_settle(table);
	const entry = table[0].get(hash);
	if (entry === undefined) table[0].set(hash, [ key, [ 0, value ] ]);
	else {
		entry[0] = key;
		if (entry[1][0] !== 0 || entry[1][1] !== value) entry[1] = [ 0, value ];
	}
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function eq(self, b) {
	return self === b;
}
function assert(condition, message, caller) {
	let $a = null;
	if (!(condition)) {
		$a = (() => {
			throw __panic(message, caller);
		})();
	}
	return $a;
}
function name(self) {
	let name3 = "";
	for (const step of self[0]) {
		if (name3 !== "") {
			name3 = name3 + ".";
		}
		name3 = name3 + step[2];
	}
	return name3;
}
function starts_with(self, prefix) {
	if (prefix[0].length > self[0].length) {
		return false;
	}
	let at3 = 0;
	for (const step of prefix[0]) {
		const own = __at(self[0], at3, "std/src/key.vl:112:14");
		if (step[1] !== own[1] || step[0] !== own[0]) {
			return false;
		}
		at3 = at3 + 1;
	}
	return true;
}
function then(self, next) {
	let parts = [  ];
	for (const step of self[0]) {
		parts.push(__clone(step));
	}
	for (const step2 of next[0]) {
		parts.push(__clone(step2));
	}
	return [ parts ];
}
function eq2(self, other) {
	return eq(__hash(self[0]), __hash(other[0]));
}
const $b = [ [ [ [ "struct-keys.vl::Task", 0, "title" ] ] ] ];
function write(task2, key, value) {
	__key_put(task2, key, __clone(value));
}
function read(task2, key) {
	return __clone(__key_at(task2, key));
}
const $c = [ [ [ [ "struct-keys.vl::Task", 1, "meta" ], [ "struct-keys.vl::Meta", 1, "stars" ] ] ] ];
function at(task2, key) {
	return __key_view(task2, key);
}
const $e = [ [ [ [ "struct-keys.vl::Task", 1, "meta" ] ] ] ];
function at2(task2, key) {
	return __key_at(task2, key);
}
const $f = [ [ [ [ "struct-keys.vl::Task", 1, "meta" ], [ "struct-keys.vl::Meta", 0, "owner" ] ] ] ];
const $g = [ [ [ [ "struct-keys.vl::Meta", 0, "owner" ] ] ] ];
function then2(self, next) {
	return [ then(self[0], next[0]) ];
}
function eq3(self, other) {
	return eq2(self[0], other[0]);
}
function name2(self) {
	return name(self[0]);
}
function starts_with2(self, prefix) {
	return starts_with(self[0], prefix[0]);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function hash(self) {
	return __hash(self[0][0]);
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function unwrap_or(self, fallback) {
	const $h = self;
	let $i = null;
	if ($h[0] === 0) {
		const x = $h[1];
		$i = x;
	} else {
		$i = __clone(fallback);
	}
	return $i;
}
let task = [ "a", [ "ada", 1 ] ];
task[1][0] = "bob";
task[1][1] = task[1][1] + 2;
assert(task[1][0] === "bob", "a literal key writes the field", "struct-keys.vl:36:2");
write(task, $b, "b");
assert(read(task, $b) === "b", "a key held in a value reads and writes", "struct-keys.vl:38:2");
const $d = at(task, $c);
$d[0][$d[1]] = 7;
assert(task[1][1] === 7, "a view return through a key at a scalar", "struct-keys.vl:40:2");
at2(task, $e)[0] = "cy";
assert(task[1][0] === "cy", "and at a struct", "struct-keys.vl:42:2");
const owner = $f;
assert(eq3(owner, then2($e, $g)), "equal paths are equal keys", "struct-keys.vl:44:2");
assert(name2(owner) === "meta.owner", name2(owner), "struct-keys.vl:45:2");
assert(starts_with2(owner, $e), "a path starts with its prefix", "struct-keys.vl:46:2");
let labels = new2();
insert(labels, owner, "owner");
assert(unwrap_or(get(labels, then2($e, $g)), "") === "owner", "a key keys a map", "struct-keys.vl:49:2");
console.log(__key_at(task, owner));
//# sourceMappingURL=struct-keys.mjs.map
