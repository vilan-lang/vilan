function __shared_new(value) {
	return { v: value };
}
function bump(xs2) {
	xs2.push(1);
	return xs2.length;
}
function note(value) {
	tally.v = tally.v + value;
	return value;
}
function $d(values) {
	values.map((value) => {
		return note(1);
	});
	return tally.v;
}
function $e(values) {
	values.map((value) => {
		note(10);
		return value;
	});
	return tally.v;
}
function $f(values) {
	values.map((value) => {
		let $g = null;
		if (tally.v < 100000) {
			$g = note(100);
		} else {
			$g = 0;
		}
		return $g;
	});
	return tally.v;
}
function $h(values) {
	values.map((value) => {
		const $i = tally.v < 100000;
		let $j = null;
		if ($i === true) {
			$j = note(1000);
		} else {
			$j = 0;
		}
		return $j;
	});
	return tally.v;
}
function $k(values) {
	return tally.v;
}
function $l(values) {
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
let $a = null;
if (tally.v < 100000) {
	$a = note(10);
} else {
	$a = 0;
}
$a;
const $b = tally.v < 100000;
let $c = null;
if ($b === true) {
	$c = note(100);
} else {
	$c = 0;
}
$c;
console.log(tally.v);
tally.v = 0;
console.log($d([ 1, 2, 3 ]));
tally.v = 0;
console.log($e([ 1, 2, 3 ]));
tally.v = 0;
console.log($f([ 1, 2, 3 ]));
tally.v = 0;
console.log($h([ 1, 2, 3 ]));
tally.v = 0;
console.log($k([ 1, 2, 3 ]));
tally.v = 0;
console.log($l([ 4, 5, 6 ]));
