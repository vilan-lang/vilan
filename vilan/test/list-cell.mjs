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
	let $cx = null;
	if (wrapped < 0) {
		$cx = wrapped + modulus;
	} else {
		$cx = wrapped;
	}
	return $cx;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $cy = null;
	if (wrapped >= half) {
		$cy = wrapped - modulus;
	} else {
		$cy = wrapped;
	}
	return $cy;
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
function dispose(self, $ap) {
	self[2].v = false;
	const $aq = [ 0, self[0] ];
	let $ar = null;
	if ($aq[0] === 0) {
		const subscribers = $aq[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== self[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ar = undefined;
	} else {
		$ar = undefined;
	}
	$ar;
	const $as = $ap;
	let $at = null;
	if ($as[0] === 0) {
		const established = $as[1];
		$at = [ 0, established ];
	} else {
		$at = $R(draining_turns.v);
	}
	const ambient = $at;
	const $au = ambient;
	let $av = null;
	if ($au[0] === 0) {
		const turn2 = $au[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn2[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn2[0].v = kept_pending;
		turn2[2].v.delete(hash(self[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn2[1].v) {
			if (subscriber3[0] !== self[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn2[1].v = kept_derived;
		turn2[3].v.delete(hash(self[1]));
		$av = undefined;
	} else {
		$av = undefined;
	}
	$av;
	const $aw = self[3].v;
	let $ax = null;
	if ($aw[0] === 0) {
		const release = $aw[1];
		self[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ax = undefined;
	} else {
		$ax = undefined;
	}
	return $ax;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $aj, $ak) {
	const $al = $ak;
	let $am = null;
	if ($al[0] === 0) {
		const owner = $al[1];
		$am = $an(owner, subscription, $aj);
	} else {
		$am = __clone(subscription);
	}
	return $am;
}
function defer_to_owner(cleanup, $aA) {
	const $aB = $aA;
	let $aC = null;
	if ($aB[0] === 0) {
		const owner = $aB[1];
		$aC = defer(owner, cleanup);
	} else {
		$aC = undefined;
	}
	return $aC;
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
	const held = $aU(derived2);
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
function $ag(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	const derived2 = minting_derivation.v;
	minting_derivation.v = false;
	signal[1].v.push([ id, () => {
		const $ah = [ 0, cell ];
		let $ai = null;
		if ($ah[0] === 0) {
			const live2 = $ah[1];
			$ai = observer(live2.v);
		} else {
			$ai = undefined;
		}
		return $ai;
	}, live, derived2 ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $af(self, observer) {
	return $ag(self, observer);
}
function $ae(self, observer) {
	return $af(self[0], observer);
}
function $an(self, item, $ao) {
	if (self[1].v) {
		dispose(item, $ao);
	} else {
		self[0].v.push(() => {
			dispose(item, $ao);
			return;
		});
	}
	return __clone(item);
}
function $az(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $ay(self, cursor) {
	$az(self[1], cursor);
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
		return $ay(source2, cursor);
	}, $f);
	return out;
}
function $aK(self, op) {
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
function $aP(self, $M) {
	const $aQ = $M;
	let $aR = null;
	if ($aQ[0] === 0) {
		const turn2 = $aQ[1];
		$aR = enqueue(turn2, self[1].v);
	} else {
		const $aS = $R(draining_turns.v);
		let $aT = null;
		if ($aS[0] === 0) {
			const draining = $aS[1];
			$aT = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aT = undefined;
		}
		$aR = $aT;
	}
	return $aR;
}
function $aO(self, mutate, $K) {
	mutate(self[0].v);
	$aP(self, $K);
}
function $aG(self, at, removed, inserted, $x) {
	const held = $y(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aO(self[0], (list) => {
		$aK(self[1], $C(list, start, taking, inserted));
		return;
	}, $x);
}
function $aD(self, value2, $aE) {
	return $aG(self, $y(self), 0, [ __clone(value2) ], $aE);
}
function $aU(self) {
	return $j(self[0]);
}
function $aV(self, at, $aW) {
	return $aG(self, at, 1, [  ], $aW);
}
function $aX(self, values, $aY) {
	return $aG(self, $y(self), 0, values, $aY);
}
function $aZ(self, at, value2, $ba) {
	return $aG(self, at, 0, [ __clone(value2) ], $ba);
}
function $bb(self, value2, $bc) {
	return $aG(self, 0, 0, [ __clone(value2) ], $bc);
}
function $bd(self, at, value2, $V) {
	const $be = __list_get($j(self[0]), at);
	let $bf = null;
	if ($be[0] === 0) {
		const previous = $be[1];
		$aO(self[0], (list) => {
			__at_put(list, at, __clone(value2));
			$aK(self[1], [ 1, at, __clone(previous), __clone(value2) ]);
			return;
		}, $V);
		$bf = undefined;
	} else {
		$bf = undefined;
	}
	return $bf;
}
function $bg(self, $bh) {
	const size = $y(self);
	let $bi = null;
	if (size > 0) {
		$bi = $aG(self, size - 1, 1, [  ], $bh);
	}
	return $bi;
}
function $bj(self, at, count, $bk) {
	return $aG(self, at, count, [  ], $bk);
}
function $bl(self, length, $bm) {
	const size = $y(self);
	let $bn = null;
	if (size > length) {
		$bn = $aG(self, length, size - length, [  ], $bm);
	}
	return $bn;
}
function $bo(self, at, values, $bp) {
	return $aG(self, at, 0, values, $bp);
}
function $bq(self, values, $br) {
	return $aG(self, 0, $y(self), values, $br);
}
function $bs(self, $bt) {
	return $aG(self, 0, $y(self), [  ], $bt);
}
function $bu(self) {
	return $y(self) === 0;
}
function $bx(self) {
	return self[0].length;
}
function $by(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($C(self[0], start, taking, inserted));
}
function $bv(self, value2, $bw) {
	return $by(self, $bx(self), 0, [ __clone(value2) ], $bw);
}
function $bz(self, at, $bA) {
	return $by(self, at, 1, [  ], $bA);
}
function $bD(elements) {
	return [ __clone(elements), [  ] ];
}
function $bB(self, body, $bC) {
	let recorder = $bD($j(self[0]));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	$aO(self[0], (list) => {
		__replace(list, __clone(produced));
		for (const op of recorded) {
			$aK(self[1], op);
		}
		return;
	}, $bC);
}
function $bE(target, $bF) {
	$bv(target, "from-fill-1", $bF);
	$bv(target, "from-fill-2", $bF);
}
function $bG(self, value2, $Z) {
	$aO(self[0], (list) => {
		__replace(list, __clone(value2));
		$aK(self[1], [ 2, __clone(value2) ]);
		return;
	}, $Z);
}
function $bH(self, items, $bI) {
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
		const $bJ = __list_get(items, index);
		let $bK = null;
		if ($bJ[0] === 0) {
			const value2 = $bJ[1];
			$bK = arriving.push(value2);
		} else {
			$bK = undefined;
		}
		$bK;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aG(self, prefix, removed, arriving, $bI);
}
function $bL(self, from, count, to, $ab) {
	if (count <= 0 || from === to) {
		return;
	}
	$aO(self[0], (list) => {
		let lifted = [  ];
		let taken = 0;
		while (taken < count) {
			lifted.push(__remove_at(list, from));
			taken = taken + 1;
		}
		let offset = 0;
		while (offset < lifted.length) {
			const $bM = __list_get(lifted, offset);
			let $bN = null;
			if ($bM[0] === 0) {
				const value2 = $bM[1];
				$bN = __insert_at(list, to + offset, value2);
			} else {
				$bN = undefined;
			}
			$bN;
			offset = offset + 1;
		}
		$aK(self[1], [ 3, from, count, to ]);
		return;
	}, $ab);
}
function $bP(body, $bQ) {
	const $bR = $bQ;
	let $bS = null;
	if ($bR[0] === 0) {
		const current = $bR[1];
		$bS = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$bS = result;
	}
	return $bS;
}
function $bV(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $bT(elements, limit) {
	return [ $b(elements), $bV(limit) ];
}
function $bY(self) {
	return self[0].v.length;
}
function $bX(self) {
	return $bY(self[1]);
}
function $ca(self, at, count, $cb) {
	return $by(self, at, count, [  ], $cb);
}
function $cc(self, at, value2, $cd) {
	return $by(self, at, 0, [ __clone(value2) ], $cd);
}
function $cf(self) {
	return $h(self[1]);
}
function $ci(self, cursor) {
	const $cm = $p(self[1], cursor);
	let $cn = null;
	if ($cm[0] === 1) {
		let lost = [  ];
		lost.push([ 2, $j(self[0]) ]);
		$cn = lost;
	} else {
		const recorded = $cm[1];
		$cn = recorded;
	}
	return $cn;
}
function $cr(self, observer) {
	return $ag(self, observer);
}
function $cq(self, observer) {
	return $cr(self[0], observer);
}
function $cv(self, cursor) {
	$az(self[1], cursor);
}
function $ce(source2, g, $e, $f) {
	const cursor = $cf(source2);
	const out = $l($k($aU(source2), g));
	as_derivation();
	register_with_owner($cq(source2, (_published) => {
		for (const op of $ci(source2, cursor)) {
			const $co = op;
			let $cp = null;
			if ($co[0] === 0) {
				const at = $co[1];
				const removed = $co[2];
				const inserted = $co[3];
				$w(out, at, removed.length, $k(inserted, g), $e);
				$cp = undefined;
			} else if ($co[0] === 1) {
				const at2 = $co[1];
				const _was = $co[2];
				const value2 = $co[3];
				$cp = $U(out, at2, g(value2), $e);
			} else if ($co[0] === 2) {
				const items = $co[1];
				$cp = $Y(out, $k(items, g), $e);
			} else {
				const from = $co[1];
				const count = $co[2];
				const to = $co[3];
				$cp = $aa(out, from, count, to, $e);
			}
			$cp;
		}
		return;
	}), $e, $f);
	defer_to_owner(() => {
		return $cv(source2, cursor);
	}, $f);
	return out;
}
function $cA(self, value2, $aE) {
	return $w(self, $y(self), 0, [ __clone(value2) ], $aE);
}
function $cB(self, at, value2, $ba) {
	return $w(self, at, 0, [ __clone(value2) ], $ba);
}
function $cC(self, at, $aW) {
	return $w(self, at, 1, [  ], $aW);
}
function $cD(self, $bh) {
	const size = $y(self);
	let $cE = null;
	if (size > 0) {
		$cE = $w(self, size - 1, 1, [  ], $bh);
	}
	return $cE;
}
function $cF(self, at, count, $bk) {
	return $w(self, at, count, [  ], $bk);
}
function $cG(self, length, $bm) {
	const size = $y(self);
	let $cH = null;
	if (size > length) {
		$cH = $w(self, length, size - length, [  ], $bm);
	}
	return $cH;
}
function $cI(self, items, $bI) {
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
		const $cJ = __list_get(items, index);
		let $cK = null;
		if ($cJ[0] === 0) {
			const value2 = $cJ[1];
			$cK = arriving.push(value2);
		} else {
			$cK = undefined;
		}
		$cK;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$w(self, prefix, removed, arriving, $bI);
}
function $cO(self, value2, $bw) {
	return $by(self, $bx(self), 0, [ __clone(value2) ], $bw);
}
function $cQ(self, body, $bC) {
	let recorder = $bD($j(self[0]));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	$J(self[0], (list) => {
		__replace(list, __clone(produced));
		for (const op of recorded) {
			$F(self[1], op);
		}
		return;
	}, $bC);
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
$aD(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($aU(my_nums), 0));
const source = $a();
const derived = $d(source, counted, [ 1 ], [ 1 ]);
$ae(source, (_list) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aD(source, "aa", [ 1 ]);
$aD(source, "bbb", [ 1 ]);
$aD(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$aV(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$aX(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$aZ(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$bb(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$bd(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bg(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bj(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bl(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bo(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$bq(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$bs(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($bu(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$bB(source, (list) => {
	$bv(list, "kk", [ 1 ]);
	$bv(list, "lll", [ 1 ]);
	$bz(list, 0, [ 1 ]);
	$bv(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($i(source)));
$bB(source, (list) => {
	$bE(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($i(source)));
const before_set = calls.v;
$bG(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$bH(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$bH(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$bL(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($i(source)));
const before_batch = notifications.v;
$bP(($bO) => {
	$aD(source, "r", [ 0, $bO ]);
	$aD(source, "ss", [ 0, $bO ]);
	$aV(source, 0, [ 0, $bO ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $bT([ "seed" ], 3);
const mirror = $d(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$bP(($bW) => {
	let round = 0;
	while (round < 8) {
		$aD(lagging, "row-" + round, [ 0, $bW ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $bX(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $l([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $d(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$bB(clamped, (list) => {
	$ca(list, 1, 99, [ 1 ]);
	$bz(list, 50, [ 1 ]);
	$cc(list, -(3), "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bj(clamped, 1, 99, [ 1 ]);
$aV(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($i(clamped)));
const walk = $l([ 1, 2, 3 ]);
const first = $ce(walk, doubled, [ 1 ], [ 1 ]);
const second = $ce(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$cq(walk, (_list) => {
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
	$bP(($cz) => {
		let made = 0;
		while (made < op_count) {
			const size = $y(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$cA(walk, next_random(100), [ 0, $cz ]);
			} else if (choice < 11) {
				$cB(walk, next_random(size + 1), next_random(100), [ 0, $cz ]);
			} else if (choice < 13) {
				$cC(walk, next_random(size), [ 0, $cz ]);
			} else if (choice < 15) {
				$U(walk, next_random(size), next_random(100), [ 0, $cz ]);
			} else if (choice < 17) {
				const from = next_random(size);
				const count = 1 + next_random(size - from);
				$aa(walk, from, count, next_random(size - count + 1), [ 0, $cz ]);
			} else if (choice < 18) {
				$cD(walk, [ 0, $cz ]);
			} else if (choice < 19 && size > 12) {
				$cF(walk, next_random(size), 1 + next_random(4), [ 0, $cz ]);
			} else if (choice < 20 && size > 20) {
				$cG(walk, next_random(size), [ 0, $cz ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$Y(walk, fresh, [ 0, $cz ]);
			} else if (choice < 22) {
				let edited = $aU(walk);
				__at_put(edited, next_random(size), next_random(100));
				edited.push(next_random(100));
				$cI(walk, edited, [ 0, $cz ]);
			} else {
				const at = next_random(size);
				$cQ(walk, (list) => {
					$cc(list, at, next_random(100), [ 0, $cz ]);
					$bz(list, 0, [ 0, $cz ]);
					$cO(list, next_random(100), [ 0, $cz ]);
					return;
				}, [ 0, $cz ]);
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
	for (const value of $aU(walk)) {
		first_wanted.push(value * 2 + 1);
		second_wanted.push(value * 2 + 8);
		naive = naive + 1;
	}
	same("first", turn, $aU(first), first_wanted);
	same("second", turn, $aU(second), second_wanted);
	turn = turn + 1;
}
if (walk_calls.v + chain_calls.v >= naive) {
	(() => {
		throw "walk: " + walk_calls.v + " + " + chain_calls.v + " calls against a rerun of " + naive;
	})();
}
console.log("walk: turns=300 silent=" + silent + " length=" + $aU(walk).length + " g=" + walk_calls.v + " h=" + chain_calls.v + " rerun=" + naive);
