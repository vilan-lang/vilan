function __enum_trap(name, value) {
	throw name + ": " + JSON.stringify(value) + " is not one of its values";
}
function classify(s) {
	const $a = s;
	let $b = null;
	if ($a === "quit") {
		$b = "leaving";
	} else if ($a === "y") {
		$b = "affirmative";
	} else if ($a === "") {
		$b = "affirmative";
	} else {
		$b = "other";
	}
	return $b;
}
function describe(signal) {
	const $c = signal;
	let $d = null;
	if ($c[0] === 0) {
		$d = "quit";
	} else {
		$d = "finished";
	}
	return $d;
}
function temperature(distance) {
	const $e = distance;
	let $f = null;
	if ($e <= 2) {
		$f = "very hot";
	} else if ($e <= 10) {
		$f = "warm";
	} else {
		$f = "cold";
	}
	return $f;
}
function ranked(signal, count) {
	const $g = signal;
	let $h = null;
	if ($g[0] === 0) {
		$h = "quit";
	} else if ($g[0] === 1) {
		$h = "finished";
	} else if ($g[0] === 1 && count > 0) {
		$h = "counted";
	}
	return $h;
}
function aligned(align, count) {
	const $i = align;
	let $j = null;
	if ($i === "start") {
		$j = "start";
	} else if ($i === "end") {
		$j = "end";
	} else if ($i === "end" && count > 0) {
		$j = "counted";
	} else {
		__enum_trap("Align", $i);
	}
	return $j;
}
function wrapped(signal, wrap) {
	const $k = signal;
	let $l = null;
	let $n = false;
	if ($k[0] === 0) {
		$n = true;
		$l = "quit";
	}
	if (!($n) && $k[0] === 1) {
		$n = true;
		$l = "finished";
	}
	if (!($n) && $k[0] === 1) {
		const $m = wrap;
		if ($m[0] === 0 && $m[1] > 0) {
			$l = "counted";
		}
	}
	return $l;
}
console.log(classify("quit"));
console.log(classify("y"));
console.log(classify(""));
console.log(classify("maybe"));
console.log(describe([ 0 ]));
console.log(describe([ 1 ]));
console.log(temperature(1));
console.log(temperature(7));
console.log(temperature(40));
console.log(ranked([ 1 ], 0));
console.log(ranked([ 1 ], 5));
console.log(aligned("end", 0));
console.log(aligned("end", 5));
console.log(wrapped([ 1 ], [ 1 ]));
console.log(wrapped([ 1 ], [ 0, 3 ]));
