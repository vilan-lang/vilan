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
function dispose(self, $J) {
	const $K = $J;
	let $L = null;
	if ($K[0] === 0) {
		const established = $K[1];
		$L = [ 0, established ];
	} else {
		$L = $q(draining_turns.v);
	}
	const ambient = $L;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $M = [ 0, handle[0] ];
	let $N = null;
	if ($M[0] === 0) {
		const subscribers = $M[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$N = undefined;
	} else {
		$N = undefined;
	}
	$N;
	const $O = ambient;
	let $P = null;
	if ($O[0] === 0) {
		const turn = $O[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(handle[1]));
		$P = undefined;
	} else {
		$P = undefined;
	}
	$P;
	const $Q = handle[3].v;
	let $R = null;
	if ($Q[0] === 0) {
		const release = $Q[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$R = undefined;
	} else {
		$R = undefined;
	}
	return $R;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $S = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		if (self[0].v[2]) {
			self[0].v[1].v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = __shared_new([ cleanup ]);
			self[0].v[2] = true;
		}
		$S = undefined;
	}
	return $S;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $ay = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $an = held[3];
		let $ao = null;
		if ($an[0] === 0) {
			const nursery = $an[1];
			if ($ap(carried)) {
				nursery.cancel();
			}
			$ao = undefined;
		} else {
			$ao = undefined;
		}
		$ao;
		let $ax = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $ar = __guarded(cleanup);
				let $as = null;
				if ($ar[0] === 0) {
					const message = $ar[1];
					if ($ap(failure)) {
						failure = [ 0, message ];
					}
					$as = undefined;
				} else {
					$as = undefined;
				}
				$as;
			}
			const $av = failure;
			let $aw = null;
			if ($av[0] === 0) {
				const message2 = $av[1];
				$aw = (() => {
					throw message2;
				})();
			} else {
				$aw = undefined;
			}
			$ax = $aw;
		}
		$ay = $ax;
	}
	return $ay;
}
function register_with_owner(subscription, $D, $E) {
	const $F = $E;
	let $G = null;
	if ($F[0] === 0) {
		const owner = $F[1];
		$G = $H(owner, subscription, $D);
	} else {
		$G = __clone(subscription);
	}
	return $G;
}
function defer_to_owner(cleanup, $Z) {
	const $aa = $Z;
	let $ab = null;
	if ($aa[0] === 0) {
		const owner = $aa[1];
		$ab = defer(owner, cleanup);
	} else {
		$ab = undefined;
	}
	return $ab;
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
	let $s = null;
	if ($p(self)) {
		$s = [ 1 ];
	} else {
		$s = __list_get(self, self.length - 1);
	}
	return $s;
}
function $k(self, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		$n = enqueue(turn, __clone(self[1].v));
	} else {
		const $t = $q(draining_turns.v);
		let $u = null;
		if ($t[0] === 0) {
			const draining = $t[1];
			$u = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$u = undefined;
		}
		$n = $u;
	}
	return $n;
}
function $i(self, value, $j) {
	self[0].v = __clone(value);
	$k(self, $j);
}
function $C(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $z(signal, observer) {
	const cell = signal[0];
	return $C(signal, mint_subscriber(() => {
		const $A = [ 0, cell ];
		let $B = null;
		if ($A[0] === 0) {
			const live = $A[1];
			$B = observer(live.v);
		} else {
			$B = undefined;
		}
		return $B;
	}));
}
function $y(self, observer, immediately) {
	const subscription = $z(self, observer);
	if (immediately) {
		observer($f(self));
	}
	return subscription;
}
function $x(self, observer) {
	return $y(self, observer, false);
}
function $H(self, item, $I) {
	defer(self, () => {
		dispose(item, $I);
		return;
	});
	return __clone(item);
}
function $c(source, $d, $e) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new($f(source));
	as_derivation();
	register_with_owner($x(__clone(source), (value) => {
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
		const $v = __map_get(cells.v, hash(value));
		let $w = null;
		if ($v[0] === 0) {
			const arriving = $v[1];
			$w = $i(arriving, true, $d);
		} else {
			$w = undefined;
		}
		return $w;
	}), $d, $e);
	return [ cells, current2 ];
}
function $U(self, key, $V) {
	const hash2 = hash(key);
	const $W = __map_get(self[0].v, hash2);
	let $X = null;
	if ($W[0] === 0) {
		const existing = $W[1];
		$X = existing;
	} else {
		const cell = $b(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $V);
		$X = cell;
	}
	return $X;
}
function $ac(owner, body) {
	return body(owner);
}
function $ah(self, $l) {
	const $ai = $l;
	let $aj = null;
	if ($ai[0] === 0) {
		const turn = $ai[1];
		$aj = enqueue(turn, __clone(self[1].v));
	} else {
		const $ak = $q(draining_turns.v);
		let $al = null;
		if ($ak[0] === 0) {
			const draining = $ak[1];
			$al = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$al = undefined;
		}
		$aj = $al;
	}
	return $aj;
}
function $ag(self, value, $j) {
	self[0].v = __clone(value);
	$ah(self, $j);
}
function $ap(self) {
	const $aq = self;
	return $aq[0] === 1;
}
function $az(self, observer) {
	return $y(self, observer, false);
}
function $aB(body, $aC) {
	const $aD = $aC;
	let $aE = null;
	if ($aD[0] === 0) {
		const current2 = $aD[1];
		$aE = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aE = result;
	}
	return $aE;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const current = $a(1);
const selected = $c(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = $ac(rows, ($T) => {
	return $U(selected, 1, [ 0, $T ]);
});
const two = $ac(rows, ($ad) => {
	return $U(selected, 2, [ 0, $ad ]);
});
const three = $ac(rows, ($ae) => {
	return $U(selected, 3, [ 0, $ae ]);
});
console.log("seeded " + $f(one) + " " + $f(two) + " " + $f(three));
$ag(current, 2, [ 1 ]);
console.log("after 2: " + $f(one) + " " + $f(two) + " " + $f(three));
$ag(current, 9, [ 1 ]);
console.log("after 9: " + $f(one) + " " + $f(two) + " " + $f(three));
const again = $ac(rows, ($am) => {
	return $U(selected, 2, [ 0, $am ]);
});
$ag(current, 2, [ 1 ]);
console.log("same cell=" + $f(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = $a(0);
let hits = 0;
const watch = $az(__clone(counted), (_) => {
	hits = hits + 1;
	return;
});
$aB(($aA) => {
	$ag(counted, 1, [ 0, $aA ]);
	$ag(counted, 2, [ 0, $aA ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
