function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function bump() {
	calls = calls + 1;
	return 0;
}
let calls = 0;
let ys = [ 10, 20 ];
const $a = bump();
__at_put(ys, $a, __at(ys, $a) + 1);
console.log(__at(ys, 0));
console.log(calls);
let cells = [ [ 10 ], [ 20 ] ];
const $b = bump();
__at(cells, $b)[0] = __at(cells, $b)[0] + 1;
console.log(__at(cells, 0)[0]);
console.log(calls);
let grid = [ [ 1, 2 ], [ 3, 4 ] ];
const $c = bump();
const $d = bump();
__at_put(__at(grid, $c), $d, __at(__at(grid, $c), $d) + 100);
console.log(__at(__at(grid, 0), 0));
console.log(calls);
const index = 1;
__at_put(ys, index, __at(ys, index) + 5);
console.log(__at(ys, 1));
console.log(calls);
