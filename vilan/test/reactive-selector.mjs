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
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
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
function new2() {
	return [ __shared_new([  ]), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
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
		while (!($o(turn[0].v)) && budget > 0) {
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
function dispose(self, $C) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $D = $C;
	let $E = null;
	if ($D[0] === 0) {
		const turn = $D[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$E = undefined;
	} else {
		$E = undefined;
	}
	$E;
	const $F = self[2].v;
	let $G = null;
	if ($F[0] === 0) {
		const release = $F[1];
		self[2].v = [ 1 ];
		release();
		$G = undefined;
	} else {
		$G = undefined;
	}
	return $G;
}
function new3() {
	return [ __shared_new([  ]) ];
}
function defer(self, cleanup) {
	self[0].v.push(cleanup);
}
function dispose2(self) {
	for (const cleanup of self[0].v) {
		cleanup();
	}
	self[0].v = [  ];
}
function register_with_owner(subscription, $w, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const owner = $y[1];
		$z = $A(owner, subscription, $w);
	} else {
		$z = __clone(subscription);
	}
	return $z;
}
function defer_to_owner(cleanup, $N) {
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const owner = $O[1];
		$P = defer(owner, cleanup);
	} else {
		$P = undefined;
	}
	return $P;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $f(self) {
	return self[0].v;
}
function $o(self) {
	return self.length === 0;
}
function $p(self) {
	return __list_get(self, self.length - 1);
}
function $k(self, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		$n = enqueue(turn, self[1].v);
	} else {
		const $q = $p(draining_turns.v);
		let $r = null;
		if ($q[0] === 0) {
			const draining = $q[1];
			$r = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$r = undefined;
		}
		$n = $r;
	}
	return $n;
}
function $i(self, value, $j) {
	self[0].v = value;
	$k(self, $j);
}
function $v(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $u(self, observer) {
	return $v(self, observer);
}
function $A(self, item, $B) {
	self[0].v.push(() => {
		dispose(item, $B);
		return;
	});
	return __clone(item);
}
function $c(source, $d, $e) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new($f(source));
	register_with_owner($u(source, (value) => {
		const previous = current2.v;
		current2.v = value;
		const $g = __map_get(cells.v, hash(previous));
		let $h = null;
		if ($g[0] === 0) {
			const leaving = $g[1];
			$h = $i(leaving, false, $d);
		} else {
			$h = undefined;
		}
		$h;
		const $s = __map_get(cells.v, hash(value));
		let $t = null;
		if ($s[0] === 0) {
			const arriving = $s[1];
			$t = $i(arriving, true, $d);
		} else {
			$t = undefined;
		}
		return $t;
	}), $d, $e);
	return [ cells, current2 ];
}
function $I(self, key, $J) {
	const hash2 = hash(key);
	const $K = __map_get(self[0].v, hash2);
	let $L = null;
	if ($K[0] === 0) {
		const existing = $K[1];
		$L = existing;
	} else {
		const cell = $b(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $J);
		$L = cell;
	}
	return $L;
}
function $Q(owner, body) {
	return body(owner);
}
function $V(self, $l) {
	const $W = $l;
	let $X = null;
	if ($W[0] === 0) {
		const turn = $W[1];
		$X = enqueue(turn, self[1].v);
	} else {
		const $Y = $p(draining_turns.v);
		let $Z = null;
		if ($Y[0] === 0) {
			const draining = $Y[1];
			$Z = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$Z = undefined;
		}
		$X = $Z;
	}
	return $X;
}
function $U(self, value, $j) {
	self[0].v = value;
	$V(self, $j);
}
function $ac(body, $ad) {
	const $ae = $ad;
	let $af = null;
	if ($ae[0] === 0) {
		const current2 = $ae[1];
		$af = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[3].v = true;
		$af = result;
	}
	return $af;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const current = $a(1);
const selected = $c(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = $Q(rows, ($H) => {
	return $I(selected, 1, [ 0, $H ]);
});
const two = $Q(rows, ($R) => {
	return $I(selected, 2, [ 0, $R ]);
});
const three = $Q(rows, ($S) => {
	return $I(selected, 3, [ 0, $S ]);
});
console.log("seeded " + $f(one) + " " + $f(two) + " " + $f(three));
$U(current, 2, [ 1 ]);
console.log("after 2: " + $f(one) + " " + $f(two) + " " + $f(three));
$U(current, 9, [ 1 ]);
console.log("after 9: " + $f(one) + " " + $f(two) + " " + $f(three));
const again = $Q(rows, ($aa) => {
	return $I(selected, 2, [ 0, $aa ]);
});
$U(current, 2, [ 1 ]);
console.log("same cell=" + $f(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = $a(0);
let hits = 0;
const watch = $u(counted, (_) => {
	hits = hits + 1;
	return;
});
$ac(($ab) => {
	$U(counted, 1, [ 0, $ab ]);
	$U(counted, 2, [ 0, $ab ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
