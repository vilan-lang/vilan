function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __option_replace(slot, value) {
	const old = slot.slice();
	slot[0] = 0;
	slot[1] = value;
	slot.length = 2;
	return old;
}
function __option_take(slot) {
	const old = slot.slice();
	slot.length = 1;
	slot[0] = 1;
	return old;
}
function drop(self) {
	console.log("drop " + self[0]);
}
function data_take_replace() {
	let a = [ 0, 5 ];
	const taken = __option_take(a);
	console.log("take-data taken=" + unwrap_or(taken, 0) + " left_none=" + is_none(a));
	let b = [ 0, 1 ];
	const old = __option_replace(b, 2);
	console.log("replace-data old=" + unwrap_or(old, 0) + " now=" + unwrap_or(b, 0));
}
function take_resource() {
	let opt = [ 0, [ "taken" ] ];
	try {
		const moved = __option_take(opt);
		$d(moved);
		console.log("take-res in-block");
	} finally {
		$d(opt);
	}
	console.log("take-res after-block");
}
function conditional_teardown() {
	let full = [ 0, [ "cond" ] ];
	try {
		const $h = __option_take(full);
		let $i = null;
		if ($h[0] === 0) {
			let c = $h[1];
			try {
				$f(c);
				$i = c = null;
			} finally {
				if (c !== null) {
					$f(c);
				}
			}
		} else {
			$i = undefined;
		}
		$i;
	} finally {
		$d(full);
	}
	console.log("cond after-some");
	let empty = [ 1 ];
	try {
		const $j = __option_take(empty);
		let $k = null;
		if ($j[0] === 0) {
			let c2 = $j[1];
			try {
				$f(c2);
				$k = c2 = null;
			} finally {
				if (c2 !== null) {
					$f(c2);
				}
			}
		} else {
			$k = console.log("cond none-arm");
		}
		return $k;
	} finally {
		$d(empty);
	}
}
function sink(r) {
	try {
		console.log("sink " + r[0]);
	} finally {
		$f(r);
	}
}
function passthrough(r) {
	console.log("passthrough");
	return r;
}
function match_move() {
	const holder = [ 0, [ "held" ] ];
	const $l = holder;
	let $m = null;
	if ($l[0] === 0) {
		const inner = $l[1];
		$m = inner;
	} else {
		$m = [ "default" ];
	}
	let extracted = $m;
	try {
		console.log("match extracted " + extracted[0]);
		$f(extracted);
		extracted = null;
	} finally {
		if (extracted !== null) {
			$f(extracted);
		}
	}
}
function match_leg_drop() {
	const held = [ 0, [ "leg" ] ];
	const $n = held;
	let $o = null;
	if ($n[0] === 0) {
		const r = $n[1];
		try {
			$o = console.log("leg " + r[0]);
		} finally {
			$f(r);
		}
	} else {
		$o = console.log("leg none");
	}
	$o;
	console.log("leg after");
}
function match_leg_pair() {
	const both = [ 0, [ "left" ], [ "right" ] ];
	const $p = both;
	let $q = null;
	if ($p[0] === 0) {
		const first = $p[1];
		const second = $p[2];
		try {
			$q = console.log("pair " + first[0] + " " + second[0]);
		} finally {
			$f(second);
			$f(first);
		}
	} else {
		$q = console.log("pair none");
	}
	$q;
	console.log("pair after");
}
function match_leg_guard(want) {
	const held = [ 0, [ "kept" ] ];
	const $r = held;
	let $s = null;
	if ($r[0] === 0 && $r[1][0] === want) {
		try {
			$s = console.log("guard-yes " + $r[1][0]);
		} finally {
			$f($r[1]);
		}
	} else if ($r[0] === 0) {
		const r = $r[1];
		try {
			$s = console.log("guard-no " + r[0]);
		} finally {
			$f(r);
		}
	} else {
		$s = console.log("guard none");
	}
	$s;
	console.log("guard after");
}
function destructure_drop() {
	const pair = [ [ "destructured" ], 3 ];
	const $t = pair;
	const r = $t[0];
	const n = $t[1];
	try {
		console.log("destructure " + r[0] + " " + n);
	} finally {
		$f(r);
	}
	console.log("destructure after");
}
function unwrap_or(self, fallback) {
	const $a = self;
	let $b = null;
	if ($a[0] === 0) {
		const x = __clone($a[1]);
		$b = x;
	} else {
		$b = __clone(fallback);
	}
	return $b;
}
function is_none(self) {
	const $c = self;
	return $c[0] === 1;
}
function $f($g) {
	drop($g);
}
function $d($e) {
	if ($e[0] === 0) {
		$f($e[1]);
	}
}
data_take_replace();
console.log("--");
take_resource();
console.log("--");
conditional_teardown();
console.log("--");
sink([ "sunk" ]);
console.log("sink returned");
let back = passthrough([ "through" ]);
try {
	console.log("passthrough returned");
	$f(back);
	back = null;
} finally {
	if (back !== null) {
		$f(back);
	}
}
console.log("--");
$f(passthrough([ "unbound" ]));
console.log("unbound dropped");
console.log("--");
match_move();
console.log("--");
match_leg_drop();
console.log("--");
match_leg_pair();
console.log("--");
match_leg_guard("kept");
console.log("--");
match_leg_guard("other");
console.log("--");
destructure_drop();
console.log("--");
let db = [ "immediate" ];
try {
	console.log("before drop");
	$f(db);
	db = null;
} finally {
	if (db !== null) {
		$f(db);
	}
}
console.log("after drop");
