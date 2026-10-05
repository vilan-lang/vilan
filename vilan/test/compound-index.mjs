function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function bump() {
	calls = calls + 1;
	return 0;
}
let calls = 0;
let ys = [ 10, 20 ];
const $a = bump();
__at_put(ys, $a, __at(ys, $a, "compound-index.vl:24:2") + 1, "compound-index.vl:24:2");
console.log(__at(ys, 0, "compound-index.vl:25:8"));
console.log(calls);
let cells = [ [ 10 ], [ 20 ] ];
const $b = bump();
__at(cells, $b, "compound-index.vl:31:2")[0] = __at(cells, $b, "compound-index.vl:31:2")[0] + 1;
console.log(__at(cells, 0, "compound-index.vl:32:8")[0]);
console.log(calls);
let grid = [ [ 1, 2 ], [ 3, 4 ] ];
const $c = bump();
const $d = bump();
__at_put(__at(grid, $c, "compound-index.vl:37:2"), $d, __at(__at(grid, $c, "compound-index.vl:37:2"), $d, "compound-index.vl:37:2") + 100, "compound-index.vl:37:2");
console.log(__at(__at(grid, 0, "compound-index.vl:38:8"), 0, "compound-index.vl:38:8"));
console.log(calls);
const index = 1;
__at_put(ys, index, __at(ys, index, "compound-index.vl:43:2") + 5, "compound-index.vl:43:2");
console.log(__at(ys, 1, "compound-index.vl:44:8"));
console.log(calls);
