function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function new2(start, end) {
	return [ start, end ];
}
function next(self) {
	let $a = null;
	if (self[0] < self[1]) {
		const value = self[0];
		self[0] = self[0] + 1;
		$a = [ 0, value ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function square(n) {
	return n * n;
}
function scale(count) {
	let out = [  ];
	const $b = new2(0, count);
	while (true) {
		const $c = next($b);
		if ($c[0] !== 0) {
			break;
		}
		const index = $c[1];
		out.push(index * 3);
	}
	return out;
}
function offset(base) {
	const shifted2 = base + 100;
	return shifted2;
}
function announce() {
	console.log("built");
	return 5;
}
const a = 1 + 2 * 3;
const b = a * 2;
const squared = square(b);
const steps = scale(4);
const shifted = offset(a);
const announced = announce();
console.log(a + b + squared + __at(steps, 2, "src/main.vl:53:26") + shifted + announced);
