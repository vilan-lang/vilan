function eq(self, b) {
	return self[0] === b[0] && self[1] === b[1];
}
function $a(a, b) {
	return eq(a, b);
}
function $b(a, b) {
	return eq(a, b);
}
function $c(a, b) {
	return !(eq(a, b));
}
function $d(a, b) {
	return a === b;
}
function $e(a, b) {
	return a !== b;
}
function $f(self, b) {
	const $g = self;
	let $j = null;
	if ($g[0] === 0) {
		const $h = b;
		let $i = null;
		if ($h[0] === 0) {
			$i = eq($g[1], $h[1]);
		} else {
			$i = false;
		}
		$j = $i;
	} else {
		const $k = b;
		$j = $k[0] === 1;
	}
	return $j;
}
const p1 = [ 1, 2 ];
const p2 = [ 1, 2 ];
const p3 = [ 3, 4 ];
console.log($a(p1, p2));
console.log($a(p1, p3));
console.log($b(p1, p2));
console.log($c(p1, p3));
console.log($d(5, 5));
console.log($d(5, 9));
console.log($e(5, 9));
const some_a = [ 0, p1 ];
const some_b = [ 0, p2 ];
const some_c = [ 0, p3 ];
console.log($f(some_a, some_b));
console.log($f(some_a, some_c));
console.log(!($f(some_a, some_c)));
console.log($f(some_a, [ 1 ]));
