function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __shared_new(value) {
	return { v: value };
}
function hash(self) {
	return __hash(self);
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		if (!(turn[1].v.has(key))) {
			turn[1].v.set(key, true);
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[3].v && !(turn[4].v) && !(turn[2].v)) {
		turn[4].v = true;
		queueMicrotask(() => {
			turn[4].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[2].v)) {
		turn[2].v = true;
		draining_turns.v.push(__clone(turn));
		let budget = 100000;
		while (!($m(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			turn[1].v = new Map();
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[2].v = false;
	}
}
function dispose(self, $q) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $r = $q;
	let $s = null;
	if ($r[0] === 0) {
		const turn = $r[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	const $t = self[2].v;
	let $u = null;
	if ($t[0] === 0) {
		const release = $t[1];
		self[2].v = [ 1 ];
		release();
		$u = undefined;
	} else {
		$u = undefined;
	}
	return $u;
}
function new2() {
	return [ __shared_new([  ]) ];
}
function dispose2(self) {
	for (const cleanup of self[0].v) {
		cleanup();
	}
	self[0].v = [  ];
}
function get_owner($z) {
	return $z;
}
function register_with_owner(subscription, $R, $S) {
	const $T = $S;
	let $U = null;
	if ($T[0] === 0) {
		const owner = $T[1];
		$U = $A(owner, subscription, $R);
	} else {
		$U = __clone(subscription);
	}
	return $U;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $d(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $e(self) {
	return self[0].v;
}
function $c(self, observer) {
	const subscription = $d(self, observer);
	observer($e(self));
	return subscription;
}
function $f(self, observer) {
	return $d(self, observer);
}
function $m(self) {
	return self.length === 0;
}
function $n(self) {
	return __list_get(self, self.length - 1);
}
function $i(self, $j) {
	const $k = $j;
	let $l = null;
	if ($k[0] === 0) {
		const turn = $k[1];
		$l = enqueue(turn, self[1].v);
	} else {
		const $o = $n(draining_turns.v);
		let $p = null;
		if ($o[0] === 0) {
			const draining = $o[1];
			$p = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$p = undefined;
		}
		$l = $p;
	}
	return $l;
}
function $g(self, value, $h) {
	self[0].v = value;
	$i(self, $h);
}
function $A(self, item, $B) {
	self[0].v.push(() => {
		dispose(item, $B);
		return;
	});
	return __clone(item);
}
function $w(self, observer, $x, $y) {
	$A(get_owner($y), $f(self, observer), $x);
}
function $C(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $F(self, observer) {
	return $c(self[0], observer);
}
function $E(self, observer) {
	const seeded = __shared_new(false);
	return $F(self, (value) => {
		if (seeded.v) {
			observer(value);
		} else {
			seeded.v = true;
		}
		return;
	});
}
function $J(self) {
	return $e(self[0]);
}
function $M(self, $j) {
	const $N = $j;
	let $O = null;
	if ($N[0] === 0) {
		const turn = $N[1];
		$O = enqueue(turn, self[1].v);
	} else {
		const $P = $n(draining_turns.v);
		let $Q = null;
		if ($P[0] === 0) {
			const draining = $P[1];
			$Q = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$Q = undefined;
		}
		$O = $Q;
	}
	return $O;
}
function $L(self, value, $h) {
	self[0].v = value;
	$M(self, $h);
}
function $G(self, transform, $H, $I) {
	const derived = $b(transform($J(self)));
	register_with_owner($E(self, (value) => {
		$L(derived, transform(value), $H);
		return;
	}), $H, $I);
	return derived;
}
function $W(self, observer) {
	const subscription = $d(self, observer);
	observer($e(self));
	return subscription;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const count = $a(1);
const eager = $c(count, (value) => {
	return console.log("sub " + value);
});
const lazy = $f(count, (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$g(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(lazy, [ 1 ]);
$g(count, 3, [ 1 ]);
const $D = $C(($v) => {
	$w(count, (value) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $v);
	return;
});
const _built = $D[0];
const scope = $D[1];
$g(count, 4, [ 1 ]);
dispose2(scope);
$g(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const watched = $E(stored, (value) => {
	return console.log("stored " + value);
});
$g(stored[0], 11, [ 1 ]);
$g(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $G(stored, (value) => {
	return "n=" + value;
}, [ 1 ], [ 1 ]);
console.log($e(labelled));
const shown = $W(labelled, (value) => {
	return console.log("label " + value);
});
$g(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
