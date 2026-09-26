function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_view(list, index) {
	if (index >= 0 && index < list.length) return [ list, index ];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $a = null;
	if (truncated > 0) {
		$a = truncated;
	} else {
		$a = 0;
	}
	return $a;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function next_mut(self) {
	let $b = null;
	if (as_usize(self[1]) < self[0].length) {
		const index = self[1];
		self[1] = self[1] + 1;
		$b = [ 0, __at_view(self[0], as_usize(index)) ];
	} else {
		$b = [ 1 ];
	}
	return $b;
}
let counter = [ [ 1, 2, 3 ], 0 ];
const $c = counter;
while (true) {
	const $d = next_mut($c);
	if ($d[0] !== 0) {
		break;
	}
	const element = $d[1];
	element[0][element[1]] = element[0][element[1]] * 10;
}
console.log(__at(counter[0], 0));
console.log(__at(counter[0], 1));
console.log(__at(counter[0], 2));
