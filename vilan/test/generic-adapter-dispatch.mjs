function next(self) {
	let $a = null;
	if (self[0] < self[1]) {
		self[0] = self[0] + 1;
		$a = [ 0, self[0] ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function taken(self, count) {
	return [ self, count ];
}
function next2(self) {
	if (self[1] <= 0) {
		return [ 1 ];
	}
	self[1] = self[1] - 1;
	return next(self[0]);
}
function sum_of(it) {
	let total = 0;
	const $d = it;
	while (true) {
		const $e = next2($d);
		if ($e[0] !== 0) {
			break;
		}
		const v2 = $e[1];
		total = total + v2;
	}
	return total;
}
function count_them(self) {
	let n = 0;
	const $f = self;
	while (true) {
		const $g = next2($f);
		if ($g[0] !== 0) {
			break;
		}
		const _v = $g[1];
		n = n + 1;
	}
	return n;
}
let taken2 = taken([ 0, 5 ], 3);
const $b = taken2;
while (true) {
	const $c = next2($b);
	if ($c[0] !== 0) {
		break;
	}
	const v = $c[1];
	console.log(String(v));
}
console.log(String(sum_of(taken([ 0, 5 ], 3))));
console.log(String(count_them(taken([ 0, 9 ], 4))));
