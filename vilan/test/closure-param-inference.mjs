function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function map(self, fn) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = $a[1];
		$b = [ 0, fn(x) ];
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
function is_some_and(self, fn) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const x = $e[1];
		$f = fn(x);
	} else {
		$f = false;
	}
	return $f;
}
function map2(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function fold(self, init, fn) {
	let accumulator = __clone(init);
	for (const item of self) {
		accumulator = fn(accumulator, item);
	}
	return accumulator;
}
function filter(self, predicate) {
	let result = [  ];
	for (const item of self) {
		if (predicate(item)) {
			result.push(__clone(item));
		}
	}
	return result;
}
const p = [ 0, [ 3, 4 ] ];
console.log(unwrap_or(map(p, (q) => {
	return q[0] + q[1];
}), 0));
console.log(is_some_and(p, (q) => {
	return q[0] === 3;
}));
let pts = [  ];
pts.push([ 1, 10 ]);
pts.push([ 2, 20 ]);
console.log(fold(map2(pts, (pt) => {
	return pt[0];
}), 0, (a, b) => {
	return a + b;
}));
console.log(filter(pts, (pt) => {
	return pt[1] > 15;
}).length);
