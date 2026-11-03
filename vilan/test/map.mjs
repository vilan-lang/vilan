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
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_has(table, hash) {
	if (hash !== table[1]) return table[0].has(hash);
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0;
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
function __nmap_keys(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(__clone(entry[0]));
	return out;
}
function __nmap_len(table) {
	const hash = table[1];
	if (hash !== undefined) {
		const entry = table[0].get(hash);
		if (entry !== undefined && entry[1][0] === 1) return table[0].size - 1;
	}
	return table[0].size;
}
function __nmap_remove(table, hash) {
	__nmap_settle(table);
	table[0].delete(hash);
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function __nmap_values(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(__clone(entry[1][1]));
	return out;
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function eq(self, b) {
	return self === b;
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert(self, key2, value2) {
	__nmap_insert(self[0], hash(key2), __clone(key2), __clone(value2));
}
function len(self) {
	return __nmap_len(self[0]);
}
function contains_key(self, key2) {
	return __nmap_has(self[0], hash(key2));
}
function get(self, key2) {
	return __nmap_get(self[0], hash(key2));
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = __clone(fallback);
	}
	return $b;
}
function is_some(self) {
	const $c = self;
	return $c[0] === 0;
}
function remove(self, key2) {
	__nmap_remove(self[0], hash(key2));
}
function is_empty(self) {
	return len(self) === 0;
}
function insert2(self, key2, value2) {
	__nmap_insert(self[0], hash2(key2), __clone(key2), __clone(value2));
}
function get2(self, key2) {
	return __nmap_get(self[0], hash2(key2));
}
function keys(self) {
	return __nmap_keys(self[0]);
}
function values(self) {
	return __nmap_values(self[0]);
}
function entries(self) {
	const keys2 = __nmap_keys(self[0]);
	const values2 = __nmap_values(self[0]);
	let result = [  ];
	let index = 0;
	while (index < keys2.length) {
		result.push([ __clone(__at(keys2, index, "std/src/hash_map.vl:125:17")), __clone(__at(values2, index, "std/src/hash_map.vl:125:30")) ]);
		index = index + 1;
	}
	return result;
}
function contains_value(self, value2) {
	for (const held of __nmap_values(self[0])) {
		if (eq(held, value2)) {
			return true;
		}
	}
	return false;
}
let scores = new2();
insert(scores, "alice", 1);
insert(scores, "bob", 2);
insert(scores, "carol", 3);
console.log(String(len(scores)));
console.log(contains_key(scores, "bob"));
console.log(contains_key(scores, "dave"));
console.log(String(unwrap_or(get(scores, "bob"), 0)));
console.log(String(unwrap_or(get(scores, "dave"), -(1))));
console.log(is_some(get(scores, "alice")));
insert(scores, "bob", 22);
console.log(String(unwrap_or(get(scores, "bob"), 0)));
console.log(String(len(scores)));
remove(scores, "bob");
console.log(contains_key(scores, "bob"));
console.log(String(len(scores)));
console.log(is_empty(scores));
let copy = __clone(scores);
insert(copy, "dave", 4);
console.log(contains_key(scores, "dave"));
console.log(contains_key(copy, "dave"));
let names = new2();
insert2(names, 1, "one");
insert2(names, 2, "two");
console.log(unwrap_or(get2(names, 1), "?"));
console.log(unwrap_or(get2(names, 9), "?"));
let letters = new2();
insert(letters, "a", 10);
insert(letters, "b", 20);
insert(letters, "c", 30);
let key_count = 0;
for (const key of keys(letters)) {
	key_count = key_count + 1;
}
console.log(String(key_count));
let sum = 0;
for (const value of values(letters)) {
	sum = sum + value;
}
console.log(String(sum));
console.log(String(keys(letters).length));
let entry_order = "";
let entry_total = 0;
for (const entry of entries(letters)) {
	entry_order = entry_order + entry[0];
	entry_total = entry_total + entry[1];
}
console.log(entry_order);
console.log(String(entry_total));
console.log(String(entries(letters).length));
console.log(contains_value(letters, 20));
console.log(contains_value(letters, 99));
let empty = new2();
console.log(is_empty(empty));
console.log(String(entries(empty).length));
console.log(contains_value(empty, 0));
//# sourceMappingURL=map.mjs.map
