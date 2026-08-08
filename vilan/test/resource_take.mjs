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
	console.log("take-data taken=" + $a(taken, 0) + " left_none=" + $d(a));
	let b = [ 0, 1 ];
	const old = __option_replace(b, 2);
	console.log("replace-data old=" + $a(old, 0) + " now=" + $a(b, 0));
}
function take_resource() {
	let opt = [ 0, [ "taken" ] ];
	try {
		const moved = __option_take(opt);
		try {
			console.log("take-res in-block");
		} finally {
			$f(moved);
		}
		console.log("take-res after-block");
	} finally {
		$f(opt);
	}
}
function conditional_teardown() {
	let full = [ 0, [ "cond" ] ];
	try {
		const $j = __option_take(full);
		let $k = null;
		if ($j[0] === 0) {
			const c = $j[1];
			$k = $h(c);
		} else {
			$k = undefined;
		}
		$k;
		console.log("cond after-some");
		let empty = [ 1 ];
		try {
			const $l = __option_take(empty);
			let $m = null;
			if ($l[0] === 0) {
				const c2 = $l[1];
				$m = $h(c2);
			} else {
				$m = console.log("cond none-arm");
			}
			return $m;
		} finally {
			$f(empty);
		}
	} finally {
		$f(full);
	}
}
function sink(r) {
	try {
		console.log("sink " + r[0]);
	} finally {
		$h(r);
	}
}
function passthrough(r) {
	console.log("passthrough");
	return r;
}
function match_move() {
	const holder = [ 0, [ "held" ] ];
	const $n = holder;
	let $o = null;
	if ($n[0] === 0) {
		const inner = $n[1];
		$o = inner;
	} else {
		$o = [ "default" ];
	}
	const extracted = $o;
	console.log("match extracted " + extracted[0]);
	$h(extracted);
}
function match_leg_drop() {
	const held = [ 0, [ "leg" ] ];
	const $p = held;
	let $q = null;
	if ($p[0] === 0) {
		const r = $p[1];
		try {
			$q = console.log("leg " + r[0]);
		} finally {
			$h(r);
		}
	} else {
		$q = console.log("leg none");
	}
	$q;
	console.log("leg after");
}
function match_leg_pair() {
	const both = [ 0, [ "left" ], [ "right" ] ];
	const $r = both;
	let $s = null;
	if ($r[0] === 0) {
		const first = $r[1];
		const second = $r[2];
		try {
			$s = console.log("pair " + first[0] + " " + second[0]);
		} finally {
			$h(second);
			$h(first);
		}
	} else {
		$s = console.log("pair none");
	}
	$s;
	console.log("pair after");
}
function match_leg_guard(want) {
	const held = [ 0, [ "kept" ] ];
	const $t = held;
	let $u = null;
	if ($t[0] === 0 && $t[1][0] === want) {
		try {
			$u = console.log("guard-yes " + $t[1][0]);
		} finally {
			$h($t[1]);
		}
	} else if ($t[0] === 0) {
		const r = $t[1];
		try {
			$u = console.log("guard-no " + r[0]);
		} finally {
			$h(r);
		}
	} else {
		$u = console.log("guard none");
	}
	$u;
	console.log("guard after");
}
function destructure_drop() {
	const pair = [ [ "destructured" ], 3 ];
	const $v = pair;
	const r = $v[0];
	const n = $v[1];
	try {
		console.log("destructure " + r[0] + " " + n);
		console.log("destructure after");
	} finally {
		$h(r);
	}
}
function $a(self, fallback) {
	const $b = self;
	let $c = null;
	if ($b[0] === 0) {
		const x = __clone($b[1]);
		$c = x;
	} else {
		$c = __clone(fallback);
	}
	return $c;
}
function $d(self) {
	const $e = self;
	return $e[0] === 1;
}
function $h($i) {
	drop($i);
}
function $f($g) {
	if ($g[0] === 0) {
		$h($g[1]);
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
const back = passthrough([ "through" ]);
console.log("passthrough returned");
$h(back);
console.log("--");
$h(passthrough([ "unbound" ]));
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
const db = [ "immediate" ];
console.log("before drop");
$h(db);
console.log("after drop");
