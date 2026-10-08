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
function over_a_call(values) {
	values.map((value) => {
		return note(1);
	});
	return tally.v;
}
function over_a_block(values) {
	values.map((value) => {
		note(10);
		return value;
	});
	return tally.v;
}
function over_an_if(values) {
	values.map((value) => {
		let $f = null;
		if (tally.v < 100000) {
			$f = note(100);
		} else {
			$f = 0;
		}
		return $f;
	});
	return tally.v;
}
function over_a_match(values) {
	values.map((value) => {
		const $g = tally.v < 100000;
		let $h = null;
		if ($g === true) {
			$h = note(1000);
		} else {
			$h = 0;
		}
		return $h;
	});
	return tally.v;
}
function over_a_literal(values) {
	return tally.v;
}
function chained(values) {
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
console.log(String(a));
console.log(String(xs.length));
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
console.log(String(tally.v));
tally.v = 0;
console.log(String(over_a_call([ 1, 2, 3 ])));
tally.v = 0;
console.log(String(over_a_block([ 1, 2, 3 ])));
tally.v = 0;
console.log(String(over_an_if([ 1, 2, 3 ])));
tally.v = 0;
console.log(String(over_a_match([ 1, 2, 3 ])));
tally.v = 0;
console.log(String(over_a_literal([ 1, 2, 3 ])));
tally.v = 0;
console.log(String(chained([ 4, 5, 6 ])));
