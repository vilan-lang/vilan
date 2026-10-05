function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
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
	const table = new Map();
	return [ table ];
}
function insert(self, key2, value2) {
	self[0].set(hash(key2), [ __clone(key2), __clone(value2) ]);
}
function len(self) {
	return self[0].size;
}
function contains_key(self, key2) {
	return self[0].has(hash(key2));
}
function get(self, key2) {
	const $a = __map_get(self[0], hash(key2));
	let $b = null;
	if ($a[0] === 0) {
		const entry2 = $a[1];
		$b = [ 0, __clone(entry2[1]) ];
	} else {
		$b = [ 1 ];
	}
	return $b;
}
function unwrap_or(self, fallback) {
	const $c = self;
	let $d = null;
	if ($c[0] === 0) {
		const x = __clone($c[1]);
		$d = x;
	} else {
		$d = __clone(fallback);
	}
	return $d;
}
function is_some(self) {
	const $e = self;
	return $e[0] === 0;
}
function remove(self, key2) {
	self[0].delete(hash(key2));
}
function is_empty(self) {
	return len(self) === 0;
}
function insert2(self, key2, value2) {
	self[0].set(hash2(key2), [ __clone(key2), __clone(value2) ]);
}
function get2(self, key2) {
	const $f = __map_get(self[0], hash2(key2));
	let $g = null;
	if ($f[0] === 0) {
		const entry2 = $f[1];
		$g = [ 0, __clone(entry2[1]) ];
	} else {
		$g = [ 1 ];
	}
	return $g;
}
function keys(self) {
	let result = [  ];
	for (const entry2 of __map_values(self[0])) {
		result.push(__clone(entry2[0]));
	}
	return result;
}
function values(self) {
	let result = [  ];
	for (const entry2 of __map_values(self[0])) {
		result.push(__clone(entry2[1]));
	}
	return result;
}
function entries(self) {
	return __map_values(self[0]);
}
function contains_value(self, value2) {
	for (const entry2 of __map_values(self[0])) {
		if (eq(entry2[1], value2)) {
			return true;
		}
	}
	return false;
}
let scores = new2();
insert(scores, "alice", 1);
insert(scores, "bob", 2);
insert(scores, "carol", 3);
console.log(len(scores));
console.log(contains_key(scores, "bob"));
console.log(contains_key(scores, "dave"));
console.log(unwrap_or(get(scores, "bob"), 0));
console.log(unwrap_or(get(scores, "dave"), -(1)));
console.log(is_some(get(scores, "alice")));
insert(scores, "bob", 22);
console.log(unwrap_or(get(scores, "bob"), 0));
console.log(len(scores));
remove(scores, "bob");
console.log(contains_key(scores, "bob"));
console.log(len(scores));
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
console.log(key_count);
let sum = 0;
for (const value of values(letters)) {
	sum = sum + value;
}
console.log(sum);
console.log(keys(letters).length);
let entry_order = "";
let entry_total = 0;
for (const entry of entries(letters)) {
	entry_order = entry_order + entry[0];
	entry_total = entry_total + entry[1];
}
console.log(entry_order);
console.log(entry_total);
console.log(entries(letters).length);
console.log(contains_value(letters, 20));
console.log(contains_value(letters, 99));
let empty = new2();
console.log(is_empty(empty));
console.log(entries(empty).length);
console.log(contains_value(empty, 0));
