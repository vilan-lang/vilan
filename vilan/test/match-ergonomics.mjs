function classify(p2) {
	const $a = p2[0];
	let $b = null;
	if ($a === 0) {
		const base = p2[1];
		$b = base;
	} else if ($a === 1) {
		$b = p2[1] * 10;
	} else {
		const other = $a;
		$b = other + p2[1];
	}
	return $b;
}
const x = 2;
const y = 5;
const p = [ x, y ];
console.log(String(classify(p)));
console.log(String(classify([ 0, 9 ])));
console.log(String(classify([ 1, 4 ])));
