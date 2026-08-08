function eq(self, b) {
	const $a = self;
	let $d = null;
	if ($a[0] === 0) {
		const $b = b;
		let $c = null;
		if ($b[0] === 0) {
			$c = $a[1] === $b[1];
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
function eq2(self, b) {
	const $f = [ self, b ];
	let $g = null;
	if ($f[0][0] === 0 && $f[1][0] === 0) {
		const x = $f[0][1];
		const y = $f[1][1];
		$g = x === y;
	} else if ($f[0][0] === 1 && $f[1][0] === 1) {
		const x2 = $f[0][1];
		const y2 = $f[1][1];
		$g = x2 === y2;
	} else {
		$g = false;
	}
	return $g;
}
function eq3(self, b) {
	return self[0] === b[0] && self[1] === b[1];
}
const p1 = [ 1, 2 ];
const p2 = [ 1, 2 ];
const p3 = [ 3, 4 ];
console.log(eq3(p1, p2));
console.log(eq3(p1, p3));
console.log(!(eq3(p1, p3)));
const a = [ 0, 5 ];
console.log(eq(a, [ 0, 5 ]));
console.log(eq(a, [ 1 ]));
console.log(!(eq(a, [ 0, 7 ])));
const r = [ 0, 1 ];
console.log(eq2(r, [ 0, 1 ]));
console.log(eq2(r, [ 1, "x" ]));
console.log(5 === 5);
console.log("a" === "b");
console.log(true === true);
