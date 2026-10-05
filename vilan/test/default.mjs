function new2(n) {
	return [ n ];
}
function default2() {
	return new2(0);
}
function default3() {
	return default2();
}
const some_id = default3();
console.log(some_id);
