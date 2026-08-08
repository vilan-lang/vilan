function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function $a(self) {
	return self.length === 0;
}
function $b(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function $c(self, init, fn) {
	let accumulator = __clone(init);
	for (const item of self) {
		accumulator = fn(accumulator, item);
	}
	return accumulator;
}
function $d(self, predicate) {
	let result = [  ];
	for (const item of self) {
		if (predicate(item)) {
			result.push(__clone(item));
		}
	}
	return result;
}
function $e(self, fn) {
	for (const item of self) {
		fn(item);
	}
}
let xs = [  ];
xs.push(1);
xs.push(2);
xs.push(3);
xs.push(4);
console.log(xs.length);
console.log($a(xs));
console.log($c($b(xs, (n) => {
	return n * 10;
}), 0, (a, b) => {
	return a + b;
}));
console.log($d(xs, (n) => {
	return n > 2;
}).length);
console.log($d(xs, (n) => {
	return n > 5;
}).length);
$e(xs, (n) => {
	return console.log(n);
});
