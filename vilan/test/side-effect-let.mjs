function __shared_new(value) {
	return { v: value };
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $a = null;
	if (wrapped < 0) {
		$a = wrapped + modulus;
	} else {
		$a = wrapped;
	}
	return $a;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $b = null;
	if (wrapped >= half) {
		$b = wrapped - modulus;
	} else {
		$b = wrapped;
	}
	return $b;
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function bump(xs2) {
	xs2.push(1);
	return as_i32(xs2.length);
}
function note(value) {
	tally.v = tally.v + value;
	return value;
}
function $f(values) {
	values.map((value) => {
		return note(1);
	});
	return tally.v;
}
function $g(values) {
	values.map((value) => {
		note(10);
		return value;
	});
	return tally.v;
}
function $h(values) {
	values.map((value) => {
		let $i = null;
		if (tally.v < 100000) {
			$i = note(100);
		} else {
			$i = 0;
		}
		return $i;
	});
	return tally.v;
}
function $j(values) {
	values.map((value) => {
		const $k = tally.v < 100000;
		let $l = null;
		if ($k === true) {
			$l = note(1000);
		} else {
			$l = 0;
		}
		return $l;
	});
	return tally.v;
}
function $m(values) {
	return tally.v;
}
function $n(values) {
	const mapped = values.map((value) => {
		note(1);
		return 2;
	});
	mapped.map((doubled) => {
		return note(doubled);
	});
	return tally.v;
}
const tally = __shared_new(0);
let xs = [  ];
const a = bump(xs);
bump(xs);
console.log(a);
console.log(xs.length);
note(1);
0;
let $c = null;
if (tally.v < 100000) {
	$c = note(10);
} else {
	$c = 0;
}
$c;
const $d = tally.v < 100000;
let $e = null;
if ($d === true) {
	$e = note(100);
} else {
	$e = 0;
}
$e;
console.log(tally.v);
tally.v = 0;
console.log($f([ 1, 2, 3 ]));
tally.v = 0;
console.log($g([ 1, 2, 3 ]));
tally.v = 0;
console.log($h([ 1, 2, 3 ]));
tally.v = 0;
console.log($j([ 1, 2, 3 ]));
tally.v = 0;
console.log($m([ 1, 2, 3 ]));
tally.v = 0;
console.log($n([ 4, 5, 6 ]));
