function next(self) {
	let $c = null;
	if (self[0] < self[1]) {
		self[0] = self[0] + 1;
		$c = [ 0, self[0] ];
	} else {
		$c = [ 1 ];
	}
	return $c;
}
function $a(self, count) {
	return [ self, count ];
}
function $b(self) {
	if (self[1] <= 0) {
		return [ 1 ];
	}
	self[1] = self[1] - 1;
	return next(self[0]);
}
function $f(it) {
	let total = 0;
	const $g = it;
	while (true) {
		const $h = $b($g);
		if ($h[0] !== 0) {
			break;
		}
		const v2 = $h[1];
		total = total + v2;
	}
	return total;
}
function $i(self) {
	let n = 0;
	const $j = self;
	while (true) {
		const $k = $b($j);
		if ($k[0] !== 0) {
			break;
		}
		const _v = $k[1];
		n = n + 1;
	}
	return n;
}
let taken = $a([ 0, 5 ], 3);
const $d = taken;
while (true) {
	const $e = $b($d);
	if ($e[0] !== 0) {
		break;
	}
	const v = $e[1];
	console.log(v);
}
console.log($f($a([ 0, 5 ], 3)));
console.log($i($a([ 0, 9 ], 4)));
