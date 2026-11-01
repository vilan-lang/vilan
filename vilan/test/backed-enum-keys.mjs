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
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function hash3(self) {
	return __hash(self);
}
function new2() {
	const keys = new Map();
	const values = new Map();
	return [ keys, values ];
}
function insert(self, key, value) {
	const hash4 = hash(key);
	self[0].set(hash4, __clone(key));
	self[1].set(hash4, __clone(value));
}
function get(self, key) {
	return __map_get(self[1], hash(key));
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
function contains_key(self, key) {
	return self[1].has(hash(key));
}
function len(self) {
	return self[1].size;
}
function new3() {
	const table = new Map();
	return [ table ];
}
function insert2(self, value) {
	self[0].set(hash2(value), __clone(value));
}
function contains(self, value) {
	return self[0].has(hash2(value));
}
function len2(self) {
	return self[0].size;
}
function insert3(self, value) {
	self[0].set(hash3(value), __clone(value));
}
function contains2(self, value) {
	return self[0].has(hash3(value));
}
let widths = new2();
insert(widths, "flex-start", 1);
insert(widths, "flex-end", 2);
console.log(String(unwrap_or(get(widths, "flex-start"), 0)));
console.log(String(unwrap_or(get(widths, "flex-end"), 0)));
console.log(contains_key(widths, "flex-start"));
console.log(String(len(widths)));
let levels = new3();
insert2(levels, 1);
insert2(levels, 1);
console.log(contains(levels, 1));
console.log(contains(levels, 0));
console.log(String(len2(levels)));
let walked = new3();
insert3(walked, 6);
console.log(contains2(walked, 6));
console.log(contains2(walked, 7));
//# sourceMappingURL=backed-enum-keys.mjs.map
