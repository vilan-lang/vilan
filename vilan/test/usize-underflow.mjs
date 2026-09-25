function checked_sub(self, other) {
	let $a = null;
	if (self >= other) {
		$a = [ 0, self - other ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function saturating_sub(self, other) {
	let $d = null;
	if (self >= other) {
		$d = self - other;
	} else {
		$d = 0;
	}
	return $d;
}
function step_back(at) {
	return at - 1;
}
function $b(self) {
	const $c = self;
	return $c[0] === 1;
}
const first = 0;
const under = step_back(first);
console.log("" + under);
console.log(under < first);
console.log($b(checked_sub(first, 1)));
const floor = saturating_sub(first, 1);
console.log("" + floor);
