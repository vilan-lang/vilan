function eq(self, b) {
	return self[0] === b[0] && self[1] === b[1];
}
function equal(a, b) {
	return eq(a, b);
}
function equal_method(a, b) {
	return eq(a, b);
}
function not_equal(a, b) {
	return !(eq(a, b));
}
function equal2(a, b) {
	return a === b;
}
function not_equal2(a, b) {
	return a !== b;
}
function eq2(self, b) {
	const $a = self;
	let $d = null;
	if ($a[0] === 0) {
		const $b = b;
		let $c = null;
		if ($b[0] === 0) {
			$c = eq($a[1], $b[1]);
		} else {
			$c = false;
		}
		$d = $c;
	} else {
		const $e = b;
		$d = $e[0] === 1;
	}
	return $d;
}
const p1 = [ 1, 2 ];
const p2 = [ 1, 2 ];
const p3 = [ 3, 4 ];
console.log(equal(p1, p2));
console.log(equal(p1, p3));
console.log(equal_method(p1, p2));
console.log(not_equal(p1, p3));
console.log(equal2(5, 5));
console.log(equal2(5, 9));
console.log(not_equal2(5, 9));
const some_a = [ 0, p1 ];
const some_b = [ 0, p2 ];
const some_c = [ 0, p3 ];
console.log(eq2(some_a, some_b));
console.log(eq2(some_a, some_c));
console.log(!(eq2(some_a, some_c)));
console.log(eq2(some_a, [ 1 ]));
