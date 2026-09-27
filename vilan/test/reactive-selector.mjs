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
function dispose(self, $I) {
	const $J = $I;
	let $K = null;
	if ($J[0] === 0) {
		const established = $J[1];
		$K = [ 0, established ];
	} else {
		$K = $q(draining_turns.v);
	}
	const ambient = $K;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $L = [ 0, handle[0] ];
	let $M = null;
	if ($L[0] === 0) {
		const subscribers = $L[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$M = undefined;
	} else {
		$M = undefined;
	}
	$M;
	const $N = ambient;
	let $O = null;
	if ($N[0] === 0) {
		const turn = $N[1];
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
		$O = undefined;
	} else {
		$O = undefined;
	}
	$O;
	const $P = handle[3].v;
	let $Q = null;
	if ($P[0] === 0) {
		const release = $P[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$Q = undefined;
	} else {
		$Q = undefined;
	}
	return $Q;
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
	let $ar = null;
	if (!(self[1].v)) {
		self[1].v = true;
		let failure = [ 1 ];
		for (const cleanup of self[0].v) {
			const $al = __guarded(cleanup);
			let $am = null;
			if ($al[0] === 0) {
				const message = $al[1];
				if ($an(failure)) {
					failure = [ 0, message ];
				}
				$am = undefined;
			} else {
				$am = undefined;
			}
			$am;
		}
		self[0].v = [  ];
		const $ap = failure;
		let $aq = null;
		if ($ap[0] === 0) {
			const message2 = $ap[1];
			$aq = (() => {
				throw message2;
			})();
		} else {
			$aq = undefined;
		}
		$ar = $aq;
	}
	return $ar;
}
function register_with_owner(subscription, $C, $D) {
	const $E = $D;
	let $F = null;
	if ($E[0] === 0) {
		const owner = $E[1];
		$F = $G(owner, subscription, $C);
	} else {
		$F = __clone(subscription);
	}
	return $F;
}
function defer_to_owner(cleanup, $X) {
	const $Y = $X;
	let $Z = null;
	if ($Y[0] === 0) {
		const owner = $Y[1];
		$Z = defer(owner, cleanup);
	} else {
		$Z = undefined;
	}
	return $Z;
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
		$n = enqueue(turn, self[1].v);
	} else {
		const $t = $q(draining_turns.v);
		let $u = null;
		if ($t[0] === 0) {
			const draining = $t[1];
			$u = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
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
function $B(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $y(signal, observer) {
	const cell = signal[0];
	return $B(signal, mint_subscriber(() => {
		const $z = [ 0, cell ];
		let $A = null;
		if ($z[0] === 0) {
			const live = $z[1];
			$A = observer(live.v);
		} else {
			$A = undefined;
		}
		return $A;
	}));
}
function $x(self, observer) {
	return $y(self, observer);
}
function $G(self, item, $H) {
	if (self[1].v) {
		dispose(item, $H);
	} else {
		self[0].v.push(() => {
			dispose(item, $H);
			return;
		});
	}
	return __clone(item);
}
function $c(source, $d, $e) {
	const cells = __shared_new(new Map());
	const current2 = __shared_new($f(source));
	as_derivation();
	register_with_owner($x(source, (value) => {
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
function $S(self, key, $T) {
	const hash2 = hash(key);
	const $U = __map_get(self[0].v, hash2);
	let $V = null;
	if ($U[0] === 0) {
		const existing = $U[1];
		$V = existing;
	} else {
		const cell = $b(key === self[1].v);
		self[0].v.set(hash2, cell);
		defer_to_owner(() => {
			self[0].v.delete(hash2);
			return;
		}, $T);
		$V = cell;
	}
	return $V;
}
function $aa(owner, body) {
	return body(owner);
}
function $af(self, $l) {
	const $ag = $l;
	let $ah = null;
	if ($ag[0] === 0) {
		const turn = $ag[1];
		$ah = enqueue(turn, self[1].v);
	} else {
		const $ai = $q(draining_turns.v);
		let $aj = null;
		if ($ai[0] === 0) {
			const draining = $ai[1];
			$aj = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aj = undefined;
		}
		$ah = $aj;
	}
	return $ah;
}
function $ae(self, value, $j) {
	self[0].v = __clone(value);
	$af(self, $j);
}
function $an(self) {
	const $ao = self;
	return $ao[0] === 1;
}
function $at(body, $au) {
	const $av = $au;
	let $aw = null;
	if ($av[0] === 0) {
		const current2 = $av[1];
		$aw = body(current2);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aw = result;
	}
	return $aw;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const current = $a(1);
const selected = $c(current, [ 1 ], [ 1 ]);
const rows = new3();
const one = $aa(rows, ($R) => {
	return $S(selected, 1, [ 0, $R ]);
});
const two = $aa(rows, ($ab) => {
	return $S(selected, 2, [ 0, $ab ]);
});
const three = $aa(rows, ($ac) => {
	return $S(selected, 3, [ 0, $ac ]);
});
console.log("seeded " + $f(one) + " " + $f(two) + " " + $f(three));
$ae(current, 2, [ 1 ]);
console.log("after 2: " + $f(one) + " " + $f(two) + " " + $f(three));
$ae(current, 9, [ 1 ]);
console.log("after 9: " + $f(one) + " " + $f(two) + " " + $f(three));
const again = $aa(rows, ($ak) => {
	return $S(selected, 2, [ 0, $ak ]);
});
$ae(current, 2, [ 1 ]);
console.log("same cell=" + $f(again) + " entries=" + selected[0].v.size);
dispose2(rows);
console.log("after dispose=" + selected[0].v.size);
const counted = $a(0);
let hits = 0;
const watch = $x(counted, (_) => {
	hits = hits + 1;
	return;
});
$at(($as) => {
	$ae(counted, 1, [ 0, $as ]);
	$ae(counted, 2, [ 0, $as ]);
	return;
}, [ 1 ]);
console.log("hits=" + hits);
dispose(watch, [ 1 ]);
