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
	let $c = null;
	if (self >= other) {
		$c = self - other;
	} else {
		$c = 0;
	}
	return $c;
}
function step_back(at) {
	return at - 1;
}
function is_none(self) {
	const $b = self;
	return $b[0] === 1;
}
const first = 0;
const under = step_back(first);
console.log("" + under);
console.log(under < first);
console.log(is_none(checked_sub(first, 1)));
const floor = saturating_sub(first, 1);
console.log("" + floor);
