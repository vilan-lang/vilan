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
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function sum_from(arena, handle) {
	const $l = get2(arena, handle);
	let $m = null;
	if ($l[0] === 0) {
		const node = $l[1];
		let total = node[0];
		for (const edge of node[1]) {
			total = total + sum_from(arena, edge);
		}
		$m = total;
	} else {
		$m = 0;
	}
	return $m;
}
function new2() {
	return [ [  ], [  ], 0 ];
}
function insert(self, value) {
	const $a = __list_pop(self[1]);
	let $b = null;
	if ($a[0] === 0) {
		const index = $a[1];
		__at(self[0], index, "std/src/arena.vl:77:5")[1] = __clone(value);
		$b = [ index, __at(self[0], index, "std/src/arena.vl:78:34")[0] ];
	} else {
		const index2 = self[0].length;
		self[0].push([ self[2], __clone(value) ]);
		$b = [ index2, self[2] ];
	}
	return $b;
}
function len(self) {
	return self[0].length - self[1].length;
}
function contains(self, handle) {
	return handle[0] < self[0].length && __at(self[0], handle[0], "std/src/arena.vl:95:38")[0] === handle[1];
}
function get(self, handle) {
	let $c = null;
	if (contains(self, handle)) {
		$c = [ 0, __at(self[0], handle[0], "std/src/arena.vl:104:10")[1] ];
	} else {
		$c = [ 1 ];
	}
	return $c;
}
function unwrap_or(self, fallback) {
	const $d = self;
	let $e = null;
	if ($d[0] === 0) {
		const x = __clone($d[1]);
		$e = x;
	} else {
		$e = __clone(fallback);
	}
	return $e;
}
function set(self, handle, value) {
	let $f = null;
	if (contains(self, handle)) {
		__at(self[0], handle[0], "std/src/arena.vl:114:4")[1] = __clone(value);
		$f = true;
	} else {
		$f = false;
	}
	return $f;
}
function remove(self, handle) {
	let $g = null;
	if (contains(self, handle)) {
		const removed = __clone(__at(self[0], handle[0], "std/src/arena.vl:125:18")[1]);
		__at(self[0], handle[0], "std/src/arena.vl:126:4")[0] = __at(self[0], handle[0], "std/src/arena.vl:126:42")[0] + 1;
		self[1].push(handle[0]);
		$g = [ 0, removed ];
	} else {
		$g = [ 1 ];
	}
	return $g;
}
function is_some(self) {
	const $h = self;
	return $h[0] === 0;
}
function get2(self, handle) {
	let $k = null;
	if (contains(self, handle)) {
		$k = [ 0, __at(self[0], handle[0], "std/src/arena.vl:104:10")[1] ];
	} else {
		$k = [ 1 ];
	}
	return $k;
}
let numbers = new2();
const a = insert(numbers, 10);
const b = insert(numbers, 20);
console.log(String(len(numbers)));
console.log(String(unwrap_or(get(numbers, a), -(1))));
set(numbers, b, 99);
console.log(String(unwrap_or(get(numbers, b), -(1))));
console.log(String(unwrap_or(remove(numbers, b), -(1))));
console.log(is_some(get(numbers, b)));
const c = insert(numbers, 30);
console.log(String(unwrap_or(get(numbers, c), -(1))));
console.log(is_some(get(numbers, b)));
console.log(String(unwrap_or(get(numbers, a), -(1))));
let graph = new2();
const leaf1 = insert(graph, [ 2, [  ] ]);
const leaf2 = insert(graph, [ 3, [  ] ]);
let root_edges = [  ];
root_edges.push(leaf1);
root_edges.push(leaf2);
const root = insert(graph, [ 1, root_edges ]);
console.log(String(sum_from(graph, root)));
