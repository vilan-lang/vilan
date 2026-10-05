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
function square(n) {
	return n * n;
}
const STEPS = [ 1, 2, 4 ];
const folded = 7;
console.log(folded);
const narrowed = 16 + square(2);
console.log(narrowed);
const base = 9;
const doubled = 18;
console.log(doubled);
const total = 7;
console.log(total);
console.log(__at(STEPS, 0, "const.vl:48:8") + __at(STEPS, 1, "const.vl:48:19") + __at(STEPS, 2, "const.vl:48:30"));
let cache = 100;
cache = cache + 1;
console.log(cache);
