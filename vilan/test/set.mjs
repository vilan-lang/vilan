function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
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
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[0]);
	return out.map(__clone);
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
function __set_iter(set) {
	const out = [];
	for (const entry of set[0][0].values()) if (entry[1][0] === 0) out.push(entry[0]);
	return out;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert(self, value2) {
	__nmap_insert(self[0], hash2(value2), __clone(value2), undefined);
}
function len(self) {
	return __nmap_len(self[0]);
}
function contains(self, value2) {
	return __nmap_has(self[0], hash2(value2));
}
function remove(self, value2) {
	__nmap_remove(self[0], hash2(value2));
}
function is_empty(self) {
	return len(self) === 0;
}
function insert2(self, value2) {
	__nmap_insert(self[0], hash(value2), __clone(value2), undefined);
}
function contains2(self, value2) {
	return __nmap_has(self[0], hash(value2));
}
function union(self, other) {
	let result = new2();
	for (const value2 of __nmap_keys(self[0])) {
		insert(result, value2);
	}
	for (const value3 of __nmap_keys(other[0])) {
		insert(result, value3);
	}
	return result;
}
function intersection(self, other) {
	let result = new2();
	for (const value2 of __nmap_keys(self[0])) {
		if (contains(other, value2)) {
			insert(result, value2);
		}
	}
	return result;
}
function difference(self, other) {
	let result = new2();
	for (const value2 of __nmap_keys(self[0])) {
		if (!(contains(other, value2))) {
			insert(result, value2);
		}
	}
	return result;
}
let numbers = new2();
insert(numbers, 1);
insert(numbers, 2);
insert(numbers, 2);
insert(numbers, 3);
console.log(String(len(numbers)));
console.log(contains(numbers, 2));
console.log(contains(numbers, 9));
remove(numbers, 2);
console.log(contains(numbers, 2));
console.log(String(len(numbers)));
console.log(is_empty(numbers));
let total = 0;
for (const value of __set_iter(numbers)) {
	total = total + value;
}
console.log(String(total));
let copy = __clone(numbers);
insert(copy, 100);
console.log(contains(numbers, 100));
console.log(contains(copy, 100));
let words = new2();
insert2(words, "hi");
insert2(words, "hi");
insert2(words, "bye");
console.log(String(len(words)));
console.log(contains2(words, "hi"));
let empty = new2();
console.log(is_empty(empty));
let left = new2();
insert(left, 1);
insert(left, 2);
insert(left, 3);
let right = new2();
insert(right, 2);
insert(right, 3);
insert(right, 4);
console.log(String(len(union(left, right))));
console.log(String(len(intersection(left, right))));
console.log(String(len(difference(left, right))));
console.log(String(len(union(left, empty))));
console.log(String(len(intersection(left, empty))));
console.log(String(len(difference(left, empty))));
//# sourceMappingURL=set.mjs.map
