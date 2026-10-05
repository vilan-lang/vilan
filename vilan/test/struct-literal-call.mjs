function sum(self) {
	return self[0] + self[1];
}
function shifted(self) {
	return [ self[0] + 1, self[1] + 1 ];
}
console.log(String(sum([ 3, 4 ])));
console.log(String([ 3, 4 ][0]));
console.log(String(sum(shifted([ 10, 20 ]))));
