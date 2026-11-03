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
function __nmap_len(table) {
	const hash = table[1];
	if (hash !== undefined) {
		const entry = table[0].get(hash);
		if (entry !== undefined && entry[1][0] === 1) return table[0].size - 1;
	}
	return table[0].size;
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
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
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
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
	return __nmap_has(self[0], hash(key));
}
function len(self) {
	return __nmap_len(self[0]);
}
function new3() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert2(self, value) {
	__nmap_insert(self[0], hash2(value), __clone(value), undefined);
}
function contains(self, value) {
	return __nmap_has(self[0], hash2(value));
}
function len2(self) {
	return __nmap_len(self[0]);
}
function insert3(self, value) {
	__nmap_insert(self[0], hash3(value), __clone(value), undefined);
}
function contains2(self, value) {
	return __nmap_has(self[0], hash3(value));
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
