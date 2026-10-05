function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
}
function __set_iter(set) {
	return [ ...set[0].values() ];
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table = new Map();
	return [ table ];
}
function insert(self, value2) {
	self[0].set(hash2(value2), value2);
}
function len(self) {
	return self[0].size;
}
function contains(self, value2) {
	return self[0].has(hash2(value2));
}
function remove(self, value2) {
	self[0].delete(hash2(value2));
}
function is_empty(self) {
	return len(self) === 0;
}
function insert2(self, value2) {
	self[0].set(hash(value2), value2);
}
function contains2(self, value2) {
	return self[0].has(hash(value2));
}
function union(self, other) {
	let result = new2();
	for (const value2 of __map_values(self[0])) {
		insert(result, value2);
	}
	for (const value3 of __map_values(other[0])) {
		insert(result, value3);
	}
	return result;
}
function intersection(self, other) {
	let result = new2();
	for (const value2 of __map_values(self[0])) {
		if (contains(other, value2)) {
			insert(result, value2);
		}
	}
	return result;
}
function difference(self, other) {
	let result = new2();
	for (const value2 of __map_values(self[0])) {
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
console.log(len(numbers));
console.log(contains(numbers, 2));
console.log(contains(numbers, 9));
remove(numbers, 2);
console.log(contains(numbers, 2));
console.log(len(numbers));
console.log(is_empty(numbers));
let total = 0;
for (const value of __set_iter(numbers)) {
	total = total + value;
}
console.log(total);
let copy = __clone(numbers);
insert(copy, 100);
console.log(contains(numbers, 100));
console.log(contains(copy, 100));
let words = new2();
insert2(words, "hi");
insert2(words, "hi");
insert2(words, "bye");
console.log(len(words));
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
console.log(len(union(left, right)));
console.log(len(intersection(left, right)));
console.log(len(difference(left, right)));
console.log(len(union(left, empty)));
console.log(len(intersection(left, empty)));
console.log(len(difference(left, empty)));
