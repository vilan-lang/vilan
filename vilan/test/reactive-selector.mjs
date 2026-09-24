function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
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
function __insert_at(list, index, value) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function as_derivation() {
	minting_derivation.v = true;
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return $p(self[0].v) && $p(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $o = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$o = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$o;
	}
	if (turn[5].v && !(turn[6].v) && !(turn[4].v)) {
		turn[6].v = true;
		queueMicrotask(() => {
			turn[6].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[4].v)) {
		turn[4].v = true;
		draining_turns.v.push(__clone(turn));
		__with_finally(() => {
			let budget = 100000;
			while (!(is_quiescent(turn)) && budget > 0) {
				while (!($p(turn[1].v)) && budget > 0) {
					const derivations = turn[1].v;
					turn[1].v = [  ];
					turn[3].v = new Map();
					for (const subscriber of derivations) {
						if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
				for (const subscriber2 of wave) {
					if (subscriber2[2].v) {
						subscriber2[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[4].v = false;
			return;
		});
	}
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $G) {
	self[2].v = false;
	const $H = [ 0, self[0] ];
	let $I = null;
	if ($H[0] === 0) {
		const subscribers = $H[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$I = undefined;
	} else {
		$I = undefined;
	}
	$I;
	const $J = $G;
	let $K = null;
	if ($J[0] === 0) {
		const established = $J[1];
		$K = [ 0, established ];
	} else {
		$K = $q(draining_turns.v);
	}
	const ambient = $K;
	const $L = ambient;
	let $M = null;
	if ($L[0] === 0) {
		const turn = $L[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(self[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== self[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(self[1]));
		$M = undefined;
	} else {
		$M = undefined;
	}
	$M;
	const $N = self[3].v;
	let $O = null;
	if ($N[0] === 0) {
		const release = $N[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$O = undefined;
	} else {
		$O = undefined;
	}
	return $O;
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
	let $ap = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $aj = __guarded(cleanup);
			let $ak = null;
			if ($aj[0] === 0) {
				const message = $aj[1];
				if ($al(failure)) {
					failure = [ 0, message ];
				}
				$ak = undefined;
			} else {
				$ak = undefined;
			}
			$ak;
		}
		self[0].v = [  ];
		const $an = failure;
		let $ao = null;
		if ($an[0] === 0) {
			const message2 = $an[1];
			$ao = (() => {
				throw message2;
			})();
		} else {
			$ao = undefined;
		}
		$ap = $ao;
	}
	return $ap;
}
function register_with_owner(subscription, $A, $B) {
	const $C = $B;
	let $D = null;
	if ($C[0] === 0) {
		const owner = $C[1];
		$D = $E(owner, subscription, $A);
	} else {
		$D = __clone(subscription);
	}
	return $D;
}
function defer_to_owner(cleanup, $V) {
	const $W = $V;
	let $X = null;
	if ($W[0] === 0) {
		const owner = $W[1];
		$X = defer(owner, cleanup);
	} else {
		$X = undefined;
	}
	return $X;
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
function $p(self) {
	return self.length === 0;
}
function $q(self) {
	return __list_get(self, self.length - 1);
}
function $k(self, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		$n = enqueue(turn, self[1].v);
	} else {
		const $r = $q(draining_turns.v);
		let $s = null;
		if ($r[0] === 0) {
			const draining = $r[1];
			$s = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$s = undefined;
		}
		$n = $s;
	}
	return $n;
}
function $i(self, value, $j) {
	self[0].v = __clone(value);
	$k(self, $j);
}
function $z(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $w(signal, observer) {
	const cell = signal[0];
	return $z(signal, mint_subscriber(() => {
		const $x = [ 0, cell ];
		let $y = null;
		if ($x[0] === 0) {
			const live = $x[1];
			$y = observer(live.v);
		} else {
			$y = undefined;
		}
		return $y;
	}));
}
function $v(self, observer) {
	return $w(self, observer);
}
function $E(self, item, $F) {
	if (self[1].v) {
		dispose(item, $F);
	} else {
		self[0].v.push(() => {
			dispose(item, $F);
			return;
		});
	}
	return __clone(item);
}
function $c(source, $d, $e) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new($f(source));
	as_derivation();
	register_with_owner($v(source, (value) => {
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
		const $t = __map_get(cells.v, hash(value));
		let $u = null;
		if ($t[0] === 0) {
			const arriving = $t[1];
			$u = $i(arriving, true, $d);
		} else {
			$u = undefined;
		}
		return $u;
	}), $d, $e);
	return [ cells, current2 ];
}
function $Q(self, key, $R) {
	const hash2 = hash(key);
	const $S = __map_get(self[0].v, hash2);
	let $T = null;
	if ($S[0] === 0) {
		const existing = $S[1];
		$T = existing;
	} else {
		const cell = $b(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $R);
		$T = cell;
	}
	return $T;
}
function $Y(owner, body) {
	return body(owner);
}
function $ad(self, $l) {
	const $ae = $l;
	let $af = null;
	if ($ae[0] === 0) {
		const turn = $ae[1];
		$af = enqueue(turn, self[1].v);
	} else {
		const $ag = $q(draining_turns.v);
		let $ah = null;
		if ($ag[0] === 0) {
			const draining = $ag[1];
			$ah = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ah = undefined;
		}
		$af = $ah;
	}
	return $af;
}
function $ac(self, value, $j) {
	self[0].v = __clone(value);
	$ad(self, $j);
}
function $al(self) {
	const $am = self;
	return $am[0] === 1;
}
function $ar(body, $as) {
	const $at = $as;
	let $au = null;
	if ($at[0] === 0) {
		const current2 = $at[1];
		$au = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$au = result;
	}
	return $au;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const current = $a(1);
const selected = $c(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = $Y(rows, ($P) => {
	return $Q(selected, 1, [ 0, $P ]);
});
const two = $Y(rows, ($Z) => {
	return $Q(selected, 2, [ 0, $Z ]);
});
const three = $Y(rows, ($aa) => {
	return $Q(selected, 3, [ 0, $aa ]);
});
console.log("seeded " + $f(one) + " " + $f(two) + " " + $f(three));
$ac(current, 2, [ 1 ]);
console.log("after 2: " + $f(one) + " " + $f(two) + " " + $f(three));
$ac(current, 9, [ 1 ]);
console.log("after 9: " + $f(one) + " " + $f(two) + " " + $f(three));
const again = $Y(rows, ($ai) => {
	return $Q(selected, 2, [ 0, $ai ]);
});
$ac(current, 2, [ 1 ]);
console.log("same cell=" + $f(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = $a(0);
let hits = 0;
const watch = $v(counted, (_) => {
	hits = hits + 1;
	return;
});
$ar(($aq) => {
	$ac(counted, 1, [ 0, $aq ]);
	$ac(counted, 2, [ 0, $aq ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
