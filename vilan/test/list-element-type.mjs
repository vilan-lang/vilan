function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function default2() {
	return 0;
}
function $a(self) {
	let total = default2();
	let seeded = false;
	for (const item of self) {
		if (seeded) {
			total = total + item;
		} else {
			total = __clone(item);
			seeded = true;
		}
	}
	return total;
}
function $b(self) {
	let total = default2();
	let seeded = false;
	for (const item of self) {
		if (seeded) {
			total = total * item;
		} else {
			total = __clone(item);
			seeded = true;
		}
	}
	return total;
}
let numbers = [  ];
numbers.push(2);
numbers.push(3);
numbers.push(4);
console.log($a(numbers));
console.log($b(numbers));
const empty = [  ];
console.log($a(empty));
for (const n of numbers) {
	console.log(n);
}
