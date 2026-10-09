function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function is_empty(self) {
	return self.length === 0;
}
function map(self, fn) {
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
function for_each(self, fn) {
	for (const item of self) {
		fn(item);
	}
}
let xs = [  ];
xs.push(1);
xs.push(2);
xs.push(3);
xs.push(4);
console.log(String(xs.length));
console.log(is_empty(xs));
console.log(String(fold(map(xs, (n) => {
	return n * 10;
}), 0, (a, b) => {
	return a + b;
})));
console.log(String(filter(xs, (n) => {
	return n > 2;
}).length));
console.log(String(filter(xs, (n) => {
	return n > 5;
}).length));
for_each(xs, (n) => {
	return console.log(String(n));
});
