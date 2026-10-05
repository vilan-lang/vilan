function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function default2() {
	return 0;
}
function sum(self) {
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
function product(self) {
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
console.log(String(sum(numbers)));
console.log(String(product(numbers)));
const empty = [  ];
console.log(String(sum(empty)));
for (const n of numbers) {
	console.log(String(n));
}
