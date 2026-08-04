function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function eq(self, b) {
	return self === b;
}
function eq2(self, b) {
	return self === b;
}
function $a(self, predicate) {
	for (const item of self) {
		if (predicate(item)) {
			return [ 0, __clone(item) ];
		}
	}
	return [ 1 ];
}
function $b(self, fallback) {
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
function $e(self) {
	const $f = self;
	return $f[0] === 1;
}
function $g(self, value) {
	for (const item of self) {
		if (eq2(item, value)) {
			return true;
		}
	}
	return false;
}
function $h(self, value) {
	let index = 0;
	for (const item of self) {
		if (eq2(item, value)) {
			return [ 0, index ];
		}
		index = index + 1;
	}
	return [ 1 ];
}
function $i(self, value) {
	for (const item of self) {
		if (eq(item, value)) {
			return true;
		}
	}
	return false;
}
function $j(self, value) {
	let index = 0;
	for (const item of self) {
		if (eq(item, value)) {
			return [ 0, index ];
		}
		index = index + 1;
	}
	return [ 1 ];
}
const xs = [ 10, 20, 30, 20 ];
console.log($b($a(xs, (n) => {
	return n > 15;
}), 0));
console.log($e($a(xs, (n) => {
	return n > 90;
})));
console.log($g(xs, 20));
console.log($g(xs, 25));
console.log($b($h(xs, 20), -(1)));
console.log($e($h(xs, 99)));
const words = [ "alpha", "beta" ];
console.log($i(words, "beta"));
console.log($b($j(words, "alpha"), -(1)));
let empty = [  ];
console.log($e($a(empty, (n) => {
	return n > 0;
})));
console.log($g(empty, 1));
console.log($e($h(empty, 1)));
