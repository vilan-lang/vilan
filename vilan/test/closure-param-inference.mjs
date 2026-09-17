function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + cell.value;
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
}
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
}
function $a(self, fn) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = $b[1];
		$c = [ 0, fn(x) ];
	} else {
		$c = [ 1 ];
	}
	return $c;
}
function $d(self, fallback) {
	const $e = self;
	let $f = null;
	if ($e[0] === 0) {
		const x = __clone($e[1]);
		$f = x;
	} else {
		$f = __clone(__force(fallback));
	}
	return $f;
}
function $g(self, fn) {
	const $h = self;
	let $i = null;
	if ($h[0] === 0) {
		const x = $h[1];
		$i = fn(x);
	} else {
		$i = false;
	}
	return $i;
}
function $j(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function $k(self, init, fn) {
	let accumulator = __clone(init);
	for (const item of self) {
		accumulator = fn(accumulator, item);
	}
	return accumulator;
}
function $l(self, predicate) {
	let result = [  ];
	for (const item of self) {
		if (predicate(item)) {
			result.push(__clone(item));
		}
	}
	return result;
}
const p = [ 0, [ 3, 4 ] ];
console.log($d($a(p, (q) => {
	return q[0] + q[1];
}), __lazy("fallback", () => {
	return 0;
})));
console.log($g(p, (q) => {
	return q[0] === 3;
}));
let pts = [  ];
pts.push([ 1, 10 ]);
pts.push([ 2, 20 ]);
console.log($k($j(pts, (pt) => {
	return pt[0];
}), 0, (a, b) => {
	return a + b;
}));
console.log($l(pts, (pt) => {
	return pt[1] > 15;
}).length);
