function combine(self, b) {
	return [ self[0] + b[0] ];
}
function combine_twice(self) {
	return combine(combine(self, self), self);
}
const c = [ 5 ];
console.log(combine_twice(c)[0]);
