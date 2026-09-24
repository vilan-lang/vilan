function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
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
function __remove_at(list, index) {
	if (index >= 0 && index < list.length) return list.splice(index, 1)[0];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __replace(target, value) {
	if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;
	return Object.assign(target, value);
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
function fold_unsigned(value2, modulus) {
	const truncated = Math.trunc(value2);
	const wrapped = truncated % modulus;
	let $cz = null;
	if (wrapped < 0) {
		$cz = wrapped + modulus;
	} else {
		$cz = wrapped;
	}
	return $cz;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $cA = null;
	if (wrapped >= half) {
		$cA = wrapped - modulus;
	} else {
		$cA = wrapped;
	}
	return $cA;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
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
	const derived2 = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived2);
}
function subscriber_of(notify, derived2) {
	return [ fresh_id(), notify, __shared_new(true), derived2 ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return $Q(self[0].v) && $Q(self[1].v);
}
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $P = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$P = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber));
		}
		$P;
	}
	if (turn2[5].v && !(turn2[6].v) && !(turn2[4].v)) {
		turn2[6].v = true;
		queueMicrotask(() => {
			turn2[6].v = false;
			drain(turn2);
			return;
		});
	}
}
function drain(turn2) {
	if (!(turn2[4].v)) {
		turn2[4].v = true;
		draining_turns.v.push(__clone(turn2));
		__with_finally(() => {
			let budget = 100000;
			while (!(is_quiescent(turn2)) && budget > 0) {
				while (!($Q(turn2[1].v)) && budget > 0) {
					const derivations = turn2[1].v;
					turn2[1].v = [  ];
					turn2[3].v = new Map();
					for (const subscriber of derivations) {
						if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn2[0].v;
				turn2[0].v = [  ];
				turn2[2].v = new Map();
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
			turn2[4].v = false;
			return;
		});
	}
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const established = $ar[1];
		$as = [ 0, established ];
	} else {
		$as = $R(draining_turns.v);
	}
	const ambient = $as;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $at = [ 0, handle[0] ];
	let $au = null;
	if ($at[0] === 0) {
		const subscribers = $at[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$au = undefined;
	} else {
		$au = undefined;
	}
	$au;
	const $av = ambient;
	let $aw = null;
	if ($av[0] === 0) {
		const turn2 = $av[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn2[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn2[0].v = kept_pending;
		turn2[2].v.delete(hash(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn2[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn2[1].v = kept_derived;
		turn2[3].v.delete(hash(handle[1]));
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	$aw;
	const $ax = handle[3].v;
	let $ay = null;
	if ($ax[0] === 0) {
		const release = $ax[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	return $ay;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $ak, $al) {
	const $am = $al;
	let $an = null;
	if ($am[0] === 0) {
		const owner = $am[1];
		$an = $ao(owner, subscription, $ak);
	} else {
		$an = __clone(subscription);
	}
	return $an;
}
function defer_to_owner(cleanup, $aB) {
	const $aC = $aB;
	let $aD = null;
	if ($aC[0] === 0) {
		const owner = $aC[1];
		$aD = defer(owner, cleanup);
	} else {
		$aD = undefined;
	}
	return $aD;
}
function splice_start(held, at) {
	let $A = null;
	if (at < 0) {
		$A = 0;
	} else if (at > held) {
		$A = held;
	} else {
		$A = at;
	}
	return $A;
}
function splice_count(held, start, removed) {
	let $B = null;
	if (removed < 0) {
		$B = 0;
	} else if (start + removed > held) {
		$B = held - start;
	} else {
		$B = removed;
	}
	return $B;
}
function counted(text) {
	calls.v = calls.v + 1;
	return text.length;
}
function law(label, source2, derived2) {
	let naive2 = [  ];
	for (const value2 of $i(source2)) {
		naive2.push(value2.length);
	}
	const held = $aV(derived2);
	if (held.length !== naive2.length) {
		(() => {
			throw "" + label + ": derived holds " + held.length + " where the rerun holds " + naive2.length;
		})();
	}
	let index = 0;
	while (index < naive2.length) {
		if (__at(held, index) !== __at(naive2, index)) {
			(() => {
				throw "" + label + ": element " + index + " is " + __at(held, index) + ", not " + __at(naive2, index);
			})();
		}
		index = index + 1;
	}
}
function expect(label, seen, wanted) {
	if (seen !== wanted) {
		(() => {
			throw "" + label + ": " + seen + ", expected " + wanted;
		})();
	}
}
function render(values) {
	let out = "";
	for (const value2 of values) {
		out = out + ("" + value2 + " ");
	}
	return out;
}
function next_random(bound) {
	const state = seed.v * 16807 % 2147483647;
	seed.v = state;
	return as_i32(state % as_i53(bound));
}
function doubled(value2) {
	walk_calls.v = walk_calls.v + 1;
	return value2 * 2 + 1;
}
function shifted(value2) {
	chain_calls.v = chain_calls.v + 1;
	return value2 + 7;
}
function same(label, turn2, held, wanted) {
	if (held.length !== wanted.length) {
		(() => {
			throw "turn " + turn2 + ": " + label + " holds " + held.length + ", the rerun " + wanted.length;
		})();
	}
	let index = 0;
	while (index < wanted.length) {
		if (__at(held, index) !== __at(wanted, index)) {
			(() => {
				throw "turn " + turn2 + ": " + label + "[" + index + "] is " + __at(held, index) + ", not " + __at(wanted, index);
			})();
		}
		index = index + 1;
	}
}
function $b(value2) {
	let subscribers = [  ];
	return [ __shared_new(value2), __shared_new(subscribers) ];
}
function $c() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function $a() {
	return [ $b([  ]), $c() ];
}
function $h(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function $g(self) {
	return $h(self[1]);
}
function $j(self) {
	return __clone(self[0].v);
}
function $i(self) {
	return $j(self[0]);
}
function $k(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function $l(elements) {
	return [ $b(elements), $c() ];
}
function $p(self, cursor) {
	const at = cursor[1].v;
	const version = self[1].v;
	let ops = [  ];
	if (at >= version) {
		return [ 0, ops ];
	}
	cursor[1].v = version;
	const base = self[2].v;
	if (at < base) {
		return [ 1 ];
	}
	let index = at - base;
	const held = __clone(self[0].v);
	const length = held.length;
	while (index < length) {
		const $q = __list_get(held, index);
		let $r = null;
		if ($q[0] === 0) {
			const op = $q[1];
			$r = ops.push(op);
		} else {
			$r = undefined;
		}
		$r;
		index = index + 1;
	}
	return [ 0, ops ];
}
function $o(self, cursor) {
	const $s = $p(self[1], cursor);
	let $t = null;
	if ($s[0] === 1) {
		let lost = [  ];
		lost.push([ 2, $j(self[0]) ]);
		$t = lost;
	} else {
		const recorded = $s[1];
		$t = recorded;
	}
	return $t;
}
function $y(self) {
	return $j(self[0]).length;
}
function $C(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $D = __list_get(inserted, offset);
		let $E = null;
		if ($D[0] === 0) {
			const value2 = $D[1];
			$E = __insert_at(list, start + offset, value2);
		} else {
			$E = undefined;
		}
		$E;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function $G(self) {
	let lowest = self[1].v;
	for (const cursor of self[3].v) {
		const at = cursor[1].v;
		if (at < lowest) {
			lowest = at;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index = dropped;
		const held = __clone(self[0].v);
		const length = held.length;
		while (index < length) {
			const $H = __list_get(held, index);
			let $I = null;
			if ($H[0] === 0) {
				const op = $H[1];
				$I = kept.push(op);
			} else {
				$I = undefined;
			}
			$I;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function $F(self, op) {
	$G(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $Q(self) {
	return self.length === 0;
}
function $R(self) {
	return __list_get(self, self.length - 1);
}
function $L(self, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const turn2 = $N[1];
		$O = enqueue(turn2, self[1].v);
	} else {
		const $S = $R(draining_turns.v);
		let $T = null;
		if ($S[0] === 0) {
			const draining = $S[1];
			$T = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$T = undefined;
		}
		$O = $T;
	}
	return $O;
}
function $J(self, mutate, $K) {
	mutate(self[0].v);
	$L(self, $K);
}
function $w(self, at, removed, inserted, $x) {
	const held = $y(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$J(self[0], (list) => {
		$F(self[1], $C(list, start, taking, inserted));
		return;
	}, $x);
}
function $U(self, at, value2, $V) {
	const $W = __list_get($j(self[0]), at);
	let $X = null;
	if ($W[0] === 0) {
		const previous = $W[1];
		$J(self[0], (list) => {
			__at_put(list, at, __clone(value2));
			$F(self[1], [ 1, at, __clone(previous), __clone(value2) ]);
			return;
		}, $V);
		$X = undefined;
	} else {
		$X = undefined;
	}
	return $X;
}
function $Y(self, value2, $Z) {
	$J(self[0], (list) => {
		__replace(list, __clone(value2));
		$F(self[1], [ 2, __clone(value2) ]);
		return;
	}, $Z);
}
function $aa(self, from, count, to, $ab) {
	if (count <= 0 || from === to) {
		return;
	}
	$J(self[0], (list) => {
		let lifted = [  ];
		let taken = 0;
		while (taken < count) {
			lifted.push(__remove_at(list, from));
			taken = taken + 1;
		}
		let offset = 0;
		while (offset < lifted.length) {
			const $ac = __list_get(lifted, offset);
			let $ad = null;
			if ($ac[0] === 0) {
				const value2 = $ac[1];
				$ad = __insert_at(list, to + offset, value2);
			} else {
				$ad = undefined;
			}
			$ad;
			offset = offset + 1;
		}
		$F(self[1], [ 3, from, count, to ]);
		return;
	}, $ab);
}
function $aj(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $ag(signal, observer) {
	const cell = signal[0];
	return $aj(signal, mint_subscriber(() => {
		const $ah = [ 0, cell ];
		let $ai = null;
		if ($ah[0] === 0) {
			const live = $ah[1];
			$ai = observer(live.v);
		} else {
			$ai = undefined;
		}
		return $ai;
	}));
}
function $af(self, observer) {
	return $ag(self, observer);
}
function $ae(self, observer) {
	return $af(self[0], observer);
}
function $ao(self, item, $ap) {
	if (self[1].v) {
		dispose(item, $ap);
	} else {
		self[0].v.push(() => {
			dispose(item, $ap);
			return;
		});
	}
	return __clone(item);
}
function $aA(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $az(self, cursor) {
	$aA(self[1], cursor);
}
function $d(source2, g, $e, $f) {
	const cursor = $g(source2);
	const out = $l($k($i(source2), g));
	as_derivation();
	register_with_owner($ae(source2, (_published) => {
		for (const op of $o(source2, cursor)) {
			const $u = op;
			let $v = null;
			if ($u[0] === 0) {
				const at = $u[1];
				const removed = $u[2];
				const inserted = $u[3];
				$w(out, at, removed.length, $k(inserted, g), $e);
				$v = undefined;
			} else if ($u[0] === 1) {
				const at2 = $u[1];
				const _was = $u[2];
				const value2 = $u[3];
				$v = $U(out, at2, g(value2), $e);
			} else if ($u[0] === 2) {
				const items = $u[1];
				$v = $Y(out, $k(items, g), $e);
			} else {
				const from = $u[1];
				const count = $u[2];
				const to = $u[3];
				$v = $aa(out, from, count, to, $e);
			}
			$v;
		}
		return;
	}), $e, $f);
	defer_to_owner(() => {
		return $az(source2, cursor);
	}, $f);
	return out;
}
function $aL(self, op) {
	$G(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $aQ(self, $M) {
	const $aR = $M;
	let $aS = null;
	if ($aR[0] === 0) {
		const turn2 = $aR[1];
		$aS = enqueue(turn2, self[1].v);
	} else {
		const $aT = $R(draining_turns.v);
		let $aU = null;
		if ($aT[0] === 0) {
			const draining = $aT[1];
			$aU = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aU = undefined;
		}
		$aS = $aU;
	}
	return $aS;
}
function $aP(self, mutate, $K) {
	mutate(self[0].v);
	$aQ(self, $K);
}
function $aH(self, at, removed, inserted, $x) {
	const held = $y(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aP(self[0], (list) => {
		$aL(self[1], $C(list, start, taking, inserted));
		return;
	}, $x);
}
function $aE(self, value2, $aF) {
	return $aH(self, $y(self), 0, [ __clone(value2) ], $aF);
}
function $aV(self) {
	return $j(self[0]);
}
function $aW(self, at, $aX) {
	return $aH(self, at, 1, [  ], $aX);
}
function $aY(self, values, $aZ) {
	return $aH(self, $y(self), 0, values, $aZ);
}
function $ba(self, at, value2, $bb) {
	return $aH(self, at, 0, [ __clone(value2) ], $bb);
}
function $bc(self, value2, $bd) {
	return $aH(self, 0, 0, [ __clone(value2) ], $bd);
}
function $be(self, at, value2, $V) {
	const $bf = __list_get($j(self[0]), at);
	let $bg = null;
	if ($bf[0] === 0) {
		const previous = $bf[1];
		$aP(self[0], (list) => {
			__at_put(list, at, __clone(value2));
			$aL(self[1], [ 1, at, __clone(previous), __clone(value2) ]);
			return;
		}, $V);
		$bg = undefined;
	} else {
		$bg = undefined;
	}
	return $bg;
}
function $bh(self, $bi) {
	const size = $y(self);
	let $bj = null;
	if (size > 0) {
		$bj = $aH(self, size - 1, 1, [  ], $bi);
	}
	return $bj;
}
function $bk(self, at, count, $bl) {
	return $aH(self, at, count, [  ], $bl);
}
function $bm(self, length, $bn) {
	const size = $y(self);
	let $bo = null;
	if (size > length) {
		$bo = $aH(self, length, size - length, [  ], $bn);
	}
	return $bo;
}
function $bp(self, at, values, $bq) {
	return $aH(self, at, 0, values, $bq);
}
function $br(self, values, $bs) {
	return $aH(self, 0, $y(self), values, $bs);
}
function $bt(self, $bu) {
	return $aH(self, 0, $y(self), [  ], $bu);
}
function $bv(self) {
	return $y(self) === 0;
}
function $by(self) {
	return self[0].length;
}
function $bz(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($C(self[0], start, taking, inserted));
}
function $bw(self, value2, $bx) {
	return $bz(self, $by(self), 0, [ __clone(value2) ], $bx);
}
function $bA(self, at, $bB) {
	return $bz(self, at, 1, [  ], $bB);
}
function $bE(elements) {
	return [ __clone(elements), [  ] ];
}
function $bC(self, body, $bD) {
	let recorder = $bE($j(self[0]));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	$aP(self[0], (list) => {
		__replace(list, __clone(produced));
		for (const op of recorded) {
			$aL(self[1], op);
		}
		return;
	}, $bD);
}
function $bF(target, $bG) {
	$bw(target, "from-fill-1", $bG);
	$bw(target, "from-fill-2", $bG);
}
function $bH(self, value2, $Z) {
	$aP(self[0], (list) => {
		__replace(list, __clone(value2));
		$aL(self[1], [ 2, __clone(value2) ]);
		return;
	}, $Z);
}
function $bI(self, items, $bJ) {
	const held = $j(self[0]);
	const old_length = held.length;
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(held, prefix) !== __at(items, prefix)) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(held, old_length - 1 - suffix) !== __at(items, new_length - 1 - suffix)) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $bK = __list_get(items, index);
		let $bL = null;
		if ($bK[0] === 0) {
			const value2 = $bK[1];
			$bL = arriving.push(value2);
		} else {
			$bL = undefined;
		}
		$bL;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aH(self, prefix, removed, arriving, $bJ);
}
function $bM(self, from, count, to, $ab) {
	if (count <= 0 || from === to) {
		return;
	}
	$aP(self[0], (list) => {
		let lifted = [  ];
		let taken = 0;
		while (taken < count) {
			lifted.push(__remove_at(list, from));
			taken = taken + 1;
		}
		let offset = 0;
		while (offset < lifted.length) {
			const $bN = __list_get(lifted, offset);
			let $bO = null;
			if ($bN[0] === 0) {
				const value2 = $bN[1];
				$bO = __insert_at(list, to + offset, value2);
			} else {
				$bO = undefined;
			}
			$bO;
			offset = offset + 1;
		}
		$aL(self[1], [ 3, from, count, to ]);
		return;
	}, $ab);
}
function $bQ(body, $bR) {
	const $bS = $bR;
	let $bT = null;
	if ($bS[0] === 0) {
		const current = $bS[1];
		$bT = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$bT = result;
	}
	return $bT;
}
function $bW(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $bU(elements, limit) {
	return [ $b(elements), $bW(limit) ];
}
function $bZ(self) {
	return self[0].v.length;
}
function $bY(self) {
	return $bZ(self[1]);
}
function $cb(self, at, count, $cc) {
	return $bz(self, at, count, [  ], $cc);
}
function $cd(self, at, value2, $ce) {
	return $bz(self, at, 0, [ __clone(value2) ], $ce);
}
function $cg(self) {
	return $h(self[1]);
}
function $cj(self, cursor) {
	const $cn = $p(self[1], cursor);
	let $co = null;
	if ($cn[0] === 1) {
		let lost = [  ];
		lost.push([ 2, $j(self[0]) ]);
		$co = lost;
	} else {
		const recorded = $cn[1];
		$co = recorded;
	}
	return $co;
}
function $ct(signal, observer) {
	const cell = signal[0];
	return $aj(signal, mint_subscriber(() => {
		const $cu = [ 0, cell ];
		let $cv = null;
		if ($cu[0] === 0) {
			const live = $cu[1];
			$cv = observer(live.v);
		} else {
			$cv = undefined;
		}
		return $cv;
	}));
}
function $cs(self, observer) {
	return $ct(self, observer);
}
function $cr(self, observer) {
	return $cs(self[0], observer);
}
function $cx(self, cursor) {
	$aA(self[1], cursor);
}
function $cf(source2, g, $e, $f) {
	const cursor = $cg(source2);
	const out = $l($k($aV(source2), g));
	as_derivation();
	register_with_owner($cr(source2, (_published) => {
		for (const op of $cj(source2, cursor)) {
			const $cp = op;
			let $cq = null;
			if ($cp[0] === 0) {
				const at = $cp[1];
				const removed = $cp[2];
				const inserted = $cp[3];
				$w(out, at, removed.length, $k(inserted, g), $e);
				$cq = undefined;
			} else if ($cp[0] === 1) {
				const at2 = $cp[1];
				const _was = $cp[2];
				const value2 = $cp[3];
				$cq = $U(out, at2, g(value2), $e);
			} else if ($cp[0] === 2) {
				const items = $cp[1];
				$cq = $Y(out, $k(items, g), $e);
			} else {
				const from = $cp[1];
				const count = $cp[2];
				const to = $cp[3];
				$cq = $aa(out, from, count, to, $e);
			}
			$cq;
		}
		return;
	}), $e, $f);
	defer_to_owner(() => {
		return $cx(source2, cursor);
	}, $f);
	return out;
}
function $cC(self, value2, $aF) {
	return $w(self, $y(self), 0, [ __clone(value2) ], $aF);
}
function $cD(self, at, value2, $bb) {
	return $w(self, at, 0, [ __clone(value2) ], $bb);
}
function $cE(self, at, $aX) {
	return $w(self, at, 1, [  ], $aX);
}
function $cF(self, $bi) {
	const size = $y(self);
	let $cG = null;
	if (size > 0) {
		$cG = $w(self, size - 1, 1, [  ], $bi);
	}
	return $cG;
}
function $cH(self, at, count, $bl) {
	return $w(self, at, count, [  ], $bl);
}
function $cI(self, length, $bn) {
	const size = $y(self);
	let $cJ = null;
	if (size > length) {
		$cJ = $w(self, length, size - length, [  ], $bn);
	}
	return $cJ;
}
function $cK(self, items, $bJ) {
	const held = $j(self[0]);
	const old_length = held.length;
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(held, prefix) !== __at(items, prefix)) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(held, old_length - 1 - suffix) !== __at(items, new_length - 1 - suffix)) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $cL = __list_get(items, index);
		let $cM = null;
		if ($cL[0] === 0) {
			const value2 = $cL[1];
			$cM = arriving.push(value2);
		} else {
			$cM = undefined;
		}
		$cM;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$w(self, prefix, removed, arriving, $bJ);
}
function $cQ(self, value2, $bx) {
	return $bz(self, $by(self), 0, [ __clone(value2) ], $bx);
}
function $cS(self, body, $bD) {
	let recorder = $bE($j(self[0]));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	$J(self[0], (list) => {
		__replace(list, __clone(produced));
		for (const op of recorded) {
			$F(self[1], op);
		}
		return;
	}, $bD);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const delta_log_limit = 1024;
const calls = __shared_new(0);
const notifications = __shared_new(0);
const seed = __shared_new(7);
const walk_calls = __shared_new(0);
const chain_calls = __shared_new(0);
const my_list = $a();
const my_nums = $d(my_list, (x) => {
	console.log("ran");
	return x.length;
}, [ 1 ], [ 1 ]);
$aE(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($aV(my_nums), 0));
const source = $a();
const derived = $d(source, counted, [ 1 ], [ 1 ]);
$ae(source, (_list) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aE(source, "aa", [ 1 ]);
$aE(source, "bbb", [ 1 ]);
$aE(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$aW(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$aY(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$ba(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$bc(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$be(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bh(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bk(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bm(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bp(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$br(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$bt(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($bv(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$bC(source, (list) => {
	$bw(list, "kk", [ 1 ]);
	$bw(list, "lll", [ 1 ]);
	$bA(list, 0, [ 1 ]);
	$bw(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($i(source)));
$bC(source, (list) => {
	$bF(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($i(source)));
const before_set = calls.v;
$bH(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$bI(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$bI(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$bM(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($i(source)));
const before_batch = notifications.v;
$bQ(($bP) => {
	$aE(source, "r", [ 0, $bP ]);
	$aE(source, "ss", [ 0, $bP ]);
	$aW(source, 0, [ 0, $bP ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $bU([ "seed" ], 3);
const mirror = $d(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$bQ(($bX) => {
	let round = 0;
	while (round < 8) {
		$aE(lagging, "row-" + round, [ 0, $bX ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $bY(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $l([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $d(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$bC(clamped, (list) => {
	$cb(list, 1, 99, [ 1 ]);
	$bA(list, 50, [ 1 ]);
	$cd(list, -(3), "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bk(clamped, 1, 99, [ 1 ]);
$aW(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($i(clamped)));
const walk = $l([ 1, 2, 3 ]);
const first = $cf(walk, doubled, [ 1 ], [ 1 ]);
const second = $cf(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$cr(walk, (_list) => {
	walk_notifications.v = walk_notifications.v + 1;
	return;
});
walk_calls.v = 0;
chain_calls.v = 0;
let naive = 0;
let silent = 0;
let turn = 1;
while (turn <= 300) {
	const before = walk_notifications.v;
	const op_count = 1 + next_random(4);
	$bQ(($cB) => {
		let made = 0;
		while (made < op_count) {
			const size = $y(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$cC(walk, next_random(100), [ 0, $cB ]);
			} else if (choice < 11) {
				$cD(walk, next_random(size + 1), next_random(100), [ 0, $cB ]);
			} else if (choice < 13) {
				$cE(walk, next_random(size), [ 0, $cB ]);
			} else if (choice < 15) {
				$U(walk, next_random(size), next_random(100), [ 0, $cB ]);
			} else if (choice < 17) {
				const from = next_random(size);
				const count = 1 + next_random(size - from);
				$aa(walk, from, count, next_random(size - count + 1), [ 0, $cB ]);
			} else if (choice < 18) {
				$cF(walk, [ 0, $cB ]);
			} else if (choice < 19 && size > 12) {
				$cH(walk, next_random(size), 1 + next_random(4), [ 0, $cB ]);
			} else if (choice < 20 && size > 20) {
				$cI(walk, next_random(size), [ 0, $cB ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$Y(walk, fresh, [ 0, $cB ]);
			} else if (choice < 22) {
				let edited = $aV(walk);
				__at_put(edited, next_random(size), next_random(100));
				edited.push(next_random(100));
				$cK(walk, edited, [ 0, $cB ]);
			} else {
				const at = next_random(size);
				$cS(walk, (list) => {
					$cd(list, at, next_random(100), [ 0, $cB ]);
					$bA(list, 0, [ 0, $cB ]);
					$cQ(list, next_random(100), [ 0, $cB ]);
					return;
				}, [ 0, $cB ]);
			}
			made = made + 1;
		}
		return;
	}, [ 1 ]);
	const waves = walk_notifications.v - before;
	if (waves > 1) {
		(() => {
			throw "turn " + turn + ": " + waves + " notifications, expected one per turn";
		})();
	}
	if (waves === 0) {
		silent = silent + 1;
	}
	let first_wanted = [  ];
	let second_wanted = [  ];
	for (const value of $aV(walk)) {
		first_wanted.push(value * 2 + 1);
		second_wanted.push(value * 2 + 8);
		naive = naive + 1;
	}
	same("first", turn, $aV(first), first_wanted);
	same("second", turn, $aV(second), second_wanted);
	turn = turn + 1;
}
if (walk_calls.v + chain_calls.v >= naive) {
	(() => {
		throw "walk: " + walk_calls.v + " + " + chain_calls.v + " calls against a rerun of " + naive;
	})();
}
console.log("walk: turns=300 silent=" + silent + " length=" + $aV(walk).length + " g=" + walk_calls.v + " h=" + chain_calls.v + " rerun=" + naive);
