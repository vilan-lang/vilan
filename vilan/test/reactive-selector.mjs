function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __guarded(body) {
	try {
		body();
		return [ 1 ];
	} catch (error) {
		return [ 0, error && error.message ? error.message : String(error) ];
	}
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
function __with_finally(body, after) {
	try {
		body();
	} finally {
		after();
	}
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
		__with_finally(() => {
			let budget = 100000;
			while (!($o(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					if (subscriber[2].v) {
						subscriber[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[2].v = false;
			return;
		});
	}
}
function dispose(self, $E) {
	self[2].v = false;
	const $F = [ 0, self[0] ];
	let $G = null;
	if ($F[0] === 0) {
		const subscribers = $F[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$G = undefined;
	} else {
		$G = undefined;
	}
	$G;
	const $H = $E;
	let $I = null;
	if ($H[0] === 0) {
		const established = $H[1];
		$I = [ 0, established ];
	} else {
		$I = $p(draining_turns.v);
	}
	const ambient = $I;
	const $J = ambient;
	let $K = null;
	if ($J[0] === 0) {
		const turn = $J[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$K = undefined;
	} else {
		$K = undefined;
	}
	$K;
	const $L = self[3].v;
	let $M = null;
	if ($L[0] === 0) {
		const release = $L[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$M = undefined;
	} else {
		$M = undefined;
	}
	return $M;
}
function new3() {
	return [ __shared_new([  ]), __shared_new(false) ];
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function dispose2(self) {
	let $an = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $ah = __guarded(cleanup);
			let $ai = null;
			if ($ah[0] === 0) {
				const message = $ah[1];
				if ($aj(failure)) {
					failure = [ 0, message ];
				}
				$ai = undefined;
			} else {
				$ai = undefined;
			}
			$ai;
		}
		self[0].v = [  ];
		const $al = failure;
		let $am = null;
		if ($al[0] === 0) {
			const message2 = $al[1];
			$am = (() => {
				throw message2;
			})();
		} else {
			$am = undefined;
		}
		$an = $am;
	}
	return $an;
}
function register_with_owner(subscription, $y, $z) {
	const $A = $z;
	let $B = null;
	if ($A[0] === 0) {
		const owner = $A[1];
		$B = $C(owner, subscription, $y);
	} else {
		$B = __clone(subscription);
	}
	return $B;
}
function defer_to_owner(cleanup, $T) {
	const $U = $T;
	let $V = null;
	if ($U[0] === 0) {
		const owner = $U[1];
		$V = defer(owner, cleanup);
	} else {
		$V = undefined;
	}
	return $V;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $f(self) {
	return __clone(self[0].v);
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
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$r = undefined;
		}
		$n = $r;
	}
	return $n;
}
function $i(self, value, $j) {
	self[0].v = __clone(value);
	$k(self, $j);
}
function $v(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $w = [ 0, cell ];
		let $x = null;
		if ($w[0] === 0) {
			const live2 = $w[1];
			$x = observer(live2.v);
		} else {
			$x = undefined;
		}
		return $x;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $u(self, observer) {
	return $v(self, observer);
}
function $C(self, item, $D) {
	if (self[1].v) {
		dispose(item, $D);
	} else {
		self[0].v.push(() => {
			dispose(item, $D);
			return;
		});
	}
	return __clone(item);
}
function $c(source, $d, $e) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new($f(source));
	register_with_owner($u(source, (value) => {
		const previous = current2.v;
		current2.v = __clone(value);
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
function $O(self, key, $P) {
	const hash2 = hash(key);
	const $Q = __map_get(self[0].v, hash2);
	let $R = null;
	if ($Q[0] === 0) {
		const existing = $Q[1];
		$R = existing;
	} else {
		const cell = $b(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $P);
		$R = cell;
	}
	return $R;
}
function $W(owner, body) {
	return body(owner);
}
function $ab(self, $l) {
	const $ac = $l;
	let $ad = null;
	if ($ac[0] === 0) {
		const turn = $ac[1];
		$ad = enqueue(turn, self[1].v);
	} else {
		const $ae = $p(draining_turns.v);
		let $af = null;
		if ($ae[0] === 0) {
			const draining = $ae[1];
			$af = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$af = undefined;
		}
		$ad = $af;
	}
	return $ad;
}
function $aa(self, value, $j) {
	self[0].v = __clone(value);
	$ab(self, $j);
}
function $aj(self) {
	const $ak = self;
	return $ak[0] === 1;
}
function $ap(body, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const current2 = $ar[1];
		$as = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[3].v = true;
		$as = result;
	}
	return $as;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const current = $a(1);
const selected = $c(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = $W(rows, ($N) => {
	return $O(selected, 1, [ 0, $N ]);
});
const two = $W(rows, ($X) => {
	return $O(selected, 2, [ 0, $X ]);
});
const three = $W(rows, ($Y) => {
	return $O(selected, 3, [ 0, $Y ]);
});
console.log("seeded " + $f(one) + " " + $f(two) + " " + $f(three));
$aa(current, 2, [ 1 ]);
console.log("after 2: " + $f(one) + " " + $f(two) + " " + $f(three));
$aa(current, 9, [ 1 ]);
console.log("after 9: " + $f(one) + " " + $f(two) + " " + $f(three));
const again = $W(rows, ($ag) => {
	return $O(selected, 2, [ 0, $ag ]);
});
$aa(current, 2, [ 1 ]);
console.log("same cell=" + $f(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = $a(0);
let hits = 0;
const watch = $u(counted, (_) => {
	hits = hits + 1;
	return;
});
$ap(($ao) => {
	$aa(counted, 1, [ 0, $ao ]);
	$aa(counted, 2, [ 0, $ao ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
