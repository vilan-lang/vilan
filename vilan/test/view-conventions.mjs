function bump(c2) {
	c2[0] = c2[0] + 1;
}
function peek(c2) {
	return c2[0];
}
let c = [ 10 ];
bump(c);
console.log(c[0]);
console.log(peek(c));
