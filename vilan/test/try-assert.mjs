function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + (cell.value && cell.value.location !== undefined ? cell.value.message : cell.value);
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
}
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
}
function __parse_i32(text) {
	const trimmed = text.trim();
	const value = Number(trimmed);
	return /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];
}
function lookup(key) {
	let $a = null;
	if (key === "hit") {
		$a = [ 0, 21 ];
	} else {
		$a = [ 1 ];
	}
	return $a;
}
function doubled(key) {
	const $b = lookup(key);
	if ($b[0] === 1) {
		return $b;
	}
	const value = $b[1];
	return [ 0, value * 2 ];
}
function to_number(text) {
	const $e = __parse_i32(text);
	let $f = null;
	if ($e[0] === 0) {
		const value = $e[1];
		$f = [ 0, value ];
	} else {
		$f = [ 1, text ];
	}
	return $f;
}
function sum(a2, b2) {
	const $g = to_number(a2);
	if ($g[0] === 1) {
		return $g;
	}
	const left = $g[1];
	const $h = to_number(b2);
	if ($h[0] === 1) {
		return $h;
	}
	const right = $h[1];
	return [ 0, left + right ];
}
function verdict(self) {
	const $n = self;
	let $o = null;
	if ($n[0] === 0) {
		const lane3 = $n[1];
		$o = [ 0, lane3 ];
	} else {
		const why3 = $n[1];
		$o = [ 1, why3 ];
	}
	return $o;
}
function from_bad(bad) {
	return [ 1, bad ];
}
function pass(gate) {
	const $m = gate;
	const $p = verdict($m);
	if ($p[0] === 1) {
		return from_bad($p[1]);
	}
	const lane3 = $p[1];
	return [ 0, lane3 + 1 ];
}
function is_twenty_one() {
	const $u = lookup("hit");
	if ($u[0] === 1) {
		return $u;
	}
	return [ 0, $u[1] === 21 ];
}
function unwrap_or(self, fallback) {
	const $c = self;
	let $d = null;
	if ($c[0] === 0) {
		const x = __clone($c[1]);
		$d = x;
	} else {
		$d = __clone(__force(fallback));
	}
	return $d;
}
console.log(unwrap_or(doubled("hit"), __lazy("fallback", () => {
	return 0 - 1;
})));
console.log(unwrap_or(doubled("miss"), __lazy("fallback", () => {
	return 0 - 1;
})));
const $i = sum("40", "2");
let $j = null;
if ($i[0] === 0) {
	const v = $i[1];
	$j = console.log(v);
} else {
	const e = $i[1];
	$j = console.log(e);
}
$j;
const $k = sum("40", "two");
let $l = null;
if ($k[0] === 0) {
	const v2 = $k[1];
	$l = console.log(v2);
} else {
	const e2 = $k[1];
	$l = console.log(e2);
}
$l;
const $q = pass([ 0, 6 ]);
let $r = null;
if ($q[0] === 0) {
	const lane = $q[1];
	$r = console.log(lane);
} else {
	const why = $q[1];
	$r = console.log(why);
}
$r;
const $s = pass([ 1, "closed" ]);
let $t = null;
if ($s[0] === 0) {
	const lane2 = $s[1];
	$t = console.log(lane2);
} else {
	const why2 = $s[1];
	$t = console.log(why2);
}
$t;
const a = 1;
const b = 2;
console.log(a !== b);
console.log(unwrap_or(is_twenty_one(), __lazy("fallback", () => {
	return false;
})));
