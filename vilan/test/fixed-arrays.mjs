function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_view(list, index, location) {
	if (index >= 0 && index < list.length) return [ list, index ];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __repeat(value, n) {
	return typeof value === "object" && value !== null
		? Array.from({ length: n }, () => __clone(value))
		: new Array(n).fill(value);
}
function total(values) {
	let sum = 0;
	for (const value of values) {
		sum = sum + value;
	}
	return sum;
}
function make() {
	return __repeat(5, 2);
}
function bump(view2) {
	view2[0][view2[1]] = view2[0][view2[1]] + 100;
}
const zeros = __repeat(0, 4);
console.log(String(__at(zeros, 0, "fixed-arrays.vl:31:8")));
let buf = [ 1, 2, 3 ];
__at_put(buf, 1, 20, "fixed-arrays.vl:34:2");
console.log(String(__at(buf, 1, "fixed-arrays.vl:35:8")));
console.log(String(total(buf)));
const view = __at_view(buf, 2, "fixed-arrays.vl:38:18");
bump(view);
console.log(String(__at(buf, 2, "fixed-arrays.vl:40:8")));
const copy = __clone(buf);
__at_put(buf, 0, 99, "fixed-arrays.vl:43:2");
console.log(String(__at(copy, 0, "fixed-arrays.vl:44:8")));
let cells = __repeat([ 7 ], 3);
__at(cells, 0, "fixed-arrays.vl:47:2")[0] = 42;
console.log(String(__at(cells, 0, "fixed-arrays.vl:48:8")[0]));
console.log(String(__at(cells, 1, "fixed-arrays.vl:49:8")[0]));
const two = make();
console.log(String(__at(two, 0, "fixed-arrays.vl:52:8") + __at(two, 1, "fixed-arrays.vl:52:17")));
const grid = [ [ 1, 2 ], [ 3, 4 ] ];
console.log(String(__at(__at(grid, 1, "fixed-arrays.vl:55:8"), 0, "fixed-arrays.vl:55:8")));
console.log(String(3));
console.log(String(__at(grid, 0, "fixed-arrays.vl:58:8").length));
console.log(String(make().length));
const $a = grid;
const left = __clone($a[0]);
const right = __clone($a[1]);
const $b = left;
const l0 = __clone($b[0]);
const l1 = __clone($b[1]);
console.log(String(l0 + l1));
const $c = right;
let r0 = __clone($c[0]);
let r1 = __clone($c[1]);
r0 = r0 + 100;
console.log(String(r0));
console.log(String(__at(__at(grid, 1, "fixed-arrays.vl:67:8"), 0, "fixed-arrays.vl:67:8")));
console.log(String(r1));
