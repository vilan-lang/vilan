function value_of(self) {
	return self[0];
}
function mix(self, b2) {
	return value_of(self) + value_of(b2);
}
const a = [ 4 ];
const b = [ 6 ];
console.log(String(mix(a, b)));
