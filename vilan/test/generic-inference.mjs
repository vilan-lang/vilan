function pick(a, b) {
	return a + b;
}
function pick2(a, b) {
	return [ a[0] + b[0] ];
}
function pick3(a, b) {
	return pick(a, b);
}
function pick4(a, b) {
	return pick2(a, b);
}
console.log(pick3(2, 3));
console.log(pick4([ 10 ], [ 20 ]));
