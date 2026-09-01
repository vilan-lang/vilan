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
function $a() {
	const table = new Map();
	return [ table ];
}
function $b(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $c(self, key) {
	const $d = __map_get(self[0], hash(key));
	let $e = null;
	if ($d[0] === 0) {
		const entry = $d[1];
		$e = [ 0, __clone(entry[1]) ];
	} else {
		$e = [ 1 ];
	}
	return $e;
}
function $f(self, fallback) {
	const $g = self;
	let $h = null;
	if ($g[0] === 0) {
		const x = __clone($g[1]);
		$h = x;
	} else {
		$h = __clone(fallback);
	}
	return $h;
}
function $i(self, key) {
	return self[0].has(hash(key));
}
function $j(self) {
	return self[0].size;
}
function $k() {
	const table = new Map();
	return [ table ];
}
function $l(self, value) {
	self[0].set(hash2(value), value);
}
function $m(self, value) {
	return self[0].has(hash2(value));
}
function $n(self) {
	return self[0].size;
}
function $p(self, value) {
	self[0].set(hash3(value), value);
}
function $q(self, value) {
	return self[0].has(hash3(value));
}
let widths = $a();
$b(widths, "flex-start", 1);
$b(widths, "flex-end", 2);
console.log($f($c(widths, "flex-start"), 0));
console.log($f($c(widths, "flex-end"), 0));
console.log($i(widths, "flex-start"));
console.log($j(widths));
let levels = $k();
$l(levels, 1);
$l(levels, 1);
console.log($m(levels, 1));
console.log($m(levels, 0));
console.log($n(levels));
let walked = $k();
$p(walked, 6);
console.log($q(walked, 6));
console.log($q(walked, 7));
