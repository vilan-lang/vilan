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
function __map_keys(map) {
	return [ ...map.keys() ].map(__clone);
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
	const keys2 = new Map();
	const values2 = new Map();
	return [ keys2, values2 ];
}
function insert(self, key2, value2) {
	const hash3 = hash(key2);
	self[0].set(hash3, __clone(key2));
	self[1].set(hash3, __clone(value2));
}
function len(self) {
	return self[1].size;
}
function contains_key(self, key2) {
	return self[1].has(hash(key2));
}
function get(self, key2) {
	return __map_get(self[1], hash(key2));
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = $a[1];
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
	const hash3 = hash(key2);
	self[0].delete(hash3);
	self[1].delete(hash3);
}
function is_empty(self) {
	return len(self) === 0;
}
function insert2(self, key2, value2) {
	const hash3 = hash2(key2);
	self[0].set(hash3, __clone(key2));
	self[1].set(hash3, __clone(value2));
}
function get2(self, key2) {
	return __map_get(self[1], hash2(key2));
}
function keys(self) {
	return __map_values(self[0]);
}
function values(self) {
	return __map_values(self[1]);
}
function entries(self) {
	let result = [  ];
	for (const hash3 of __map_keys(self[1])) {
		const $f = __map_get(self[0], hash3);
		let $g = null;
		if ($f[0] === 0) {
			const key2 = $f[1];
			const $h = __map_get(self[1], hash3);
			let $i = null;
			if ($h[0] === 0) {
				const value2 = $h[1];
				$i = result.push([ key2, value2 ]);
			} else {
				$i = undefined;
			}
			$g = $i;
		} else {
			$g = undefined;
		}
		$g;
	}
	return result;
}
function contains_value(self, value2) {
	for (const held of __map_values(self[1])) {
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
