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
	let $aX = null;
	if (wrapped < 0) {
		$aX = wrapped + modulus;
	} else {
		$aX = wrapped;
	}
	return $aX;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $q = null;
	if (truncated > 0) {
		$q = truncated;
	} else {
		$q = 0;
	}
	return $q;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $aY = null;
	if (wrapped >= half) {
		$aY = wrapped - modulus;
	} else {
		$aY = wrapped;
	}
	return $aY;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function as_i322(self) {
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
	return $T(self[0].v) && $T(self[1].v);
}
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $S = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$S = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber));
		}
		$S;
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
				while (!($T(turn2[1].v)) && budget > 0) {
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
function dispose(self, $av) {
	const $aw = $av;
	let $ax = null;
	if ($aw[0] === 0) {
		const established = $aw[1];
		$ax = [ 0, established ];
	} else {
		$ax = $U(draining_turns.v);
	}
	const ambient = $ax;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ay = [ 0, handle[0] ];
	let $az = null;
	if ($ay[0] === 0) {
		const subscribers = $ay[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$az = undefined;
	} else {
		$az = undefined;
	}
	$az;
	const $aA = ambient;
	let $aB = null;
	if ($aA[0] === 0) {
		const turn2 = $aA[1];
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
		$aB = undefined;
	} else {
		$aB = undefined;
	}
	$aB;
	const $aC = handle[3].v;
	let $aD = null;
	if ($aC[0] === 0) {
		const release = $aC[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aD = undefined;
	} else {
		$aD = undefined;
	}
	return $aD;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $ap, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const owner = $ar[1];
		$as = $at(owner, subscription, $ap);
	} else {
		$as = __clone(subscription);
	}
	return $as;
}
function defer_to_owner(cleanup, $aG) {
	const $aH = $aG;
	let $aI = null;
	if ($aH[0] === 0) {
		const owner = $aH[1];
		$aI = defer(owner, cleanup);
	} else {
		$aI = undefined;
	}
	return $aI;
}
function splice_start(held, at) {
	let $A = null;
	if (at > held) {
		$A = held;
	} else {
		$A = at;
	}
	return $A;
}
function splice_count(held, start, removed) {
	let $B = null;
	if (start + removed > held) {
		$B = held - start;
	} else {
		$B = removed;
	}
	return $B;
}
function counted(text) {
	calls.v = calls.v + 1;
	return as_i322(text.length);
}
function law(label, source2, derived2) {
	let naive2 = [  ];
	for (const value2 of $j(source2)) {
		naive2.push(as_i322(value2.length));
	}
	const held = $j(derived2);
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
function pick(bound) {
	return as_usize(next_random(as_i322(bound)));
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
function $b() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function $d(value2) {
	let subscribers = [  ];
	return [ __shared_new(value2), __shared_new(subscribers) ];
}
function $c(elements, log) {
	return [ __shared_new(elements), $d(0), __clone(log) ];
}
function $a() {
	return $c([  ], $b());
}
function $i(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function $h(self) {
	return $i(self[2]);
}
function $j(self) {
	return __clone(self[0].v);
}
function $k(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function $n(elements, log) {
	return [ __shared_new(elements), $d(0), __clone(log) ];
}
function $l(elements) {
	return $n(elements, $b());
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
	let index = as_usize(at - base);
	const held = __clone(self[0].v);
	const length = held.length;
	while (index < length) {
		const $r = __list_get(held, index);
		let $s = null;
		if ($r[0] === 0) {
			const op = $r[1];
			$s = ops.push(op);
		} else {
			$s = undefined;
		}
		$s;
		index = index + 1;
	}
	return [ 0, ops ];
}
function $o(self, cursor) {
	const $t = $p(self[2], cursor);
	let $u = null;
	if ($t[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(self[0].v) ]);
		$u = lost;
	} else {
		const recorded = $t[1];
		$u = recorded;
	}
	return $u;
}
function $z(self) {
	return self[0].v.length;
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
		let index = as_usize(dropped);
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
function $L(self) {
	return self[1].v;
}
function $T(self) {
	return self.length === 0;
}
function $U(self) {
	let $W = null;
	if ($T(self)) {
		$W = [ 1 ];
	} else {
		$W = __list_get(self, self.length - 1);
	}
	return $W;
}
function $O(self, $P) {
	const $Q = $P;
	let $R = null;
	if ($Q[0] === 0) {
		const turn2 = $Q[1];
		$R = enqueue(turn2, self[1].v);
	} else {
		const $X = $U(draining_turns.v);
		let $Y = null;
		if ($X[0] === 0) {
			const draining = $X[1];
			$Y = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$Y = undefined;
		}
		$R = $Y;
	}
	return $R;
}
function $M(self, value2, $N) {
	self[0].v = __clone(value2);
	$O(self, $N);
}
function $J(cell, $K) {
	$M(cell[1], $L(cell[2]), $K);
}
function $x(self, at, removed, inserted, $y) {
	const held = $z(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$F(self[2], $C(self[0].v, start, taking, inserted));
	$J(self, $y);
}
function $Z(self, at, value2, $aa) {
	const $ab = __list_get(self[0].v, at);
	let $ac = null;
	if ($ab[0] === 0) {
		const previous = $ab[1];
		__at_put(self[0].v, at, __clone(value2));
		$F(self[2], [ 1, at, previous, __clone(value2) ]);
		$J(self, $aa);
		$ac = undefined;
	} else {
		$ac = undefined;
	}
	return $ac;
}
function $ad(self, value2, $ae) {
	self[0].v = __clone(value2);
	$F(self[2], [ 2, __clone(value2) ]);
	$J(self, $ae);
}
function $af(self, from, count, to, $ag) {
	if (count === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count) {
		lifted.push(__remove_at(self[0].v, from));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $ah = __list_get(lifted, offset);
		let $ai = null;
		if ($ah[0] === 0) {
			const value2 = $ah[1];
			$ai = __insert_at(self[0].v, to + offset, value2);
		} else {
			$ai = undefined;
		}
		$ai;
		offset = offset + 1;
	}
	$F(self[2], [ 3, from, count, to ]);
	$J(self, $ag);
}
function $ao(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $al(signal, observer) {
	const cell = signal[0];
	return $ao(signal, mint_subscriber(() => {
		const $am = [ 0, cell ];
		let $an = null;
		if ($am[0] === 0) {
			const live = $am[1];
			$an = observer(live.v);
		} else {
			$an = undefined;
		}
		return $an;
	}));
}
function $ak(self, observer) {
	return $al(self, observer);
}
function $aj(self, observer) {
	return $ak(self[1], (_sequence) => {
		return observer(self[0].v);
	});
}
function $at(self, item, $au) {
	if (self[1].v) {
		dispose(item, $au);
	} else {
		self[0].v.push(() => {
			dispose(item, $au);
			return;
		});
	}
	return __clone(item);
}
function $aF(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $aE(self, cursor) {
	$aF(self[2], cursor);
}
function $e(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $l($k($j(source2), g));
	as_derivation();
	register_with_owner($aj(source2, (_published) => {
		for (const op of $o(source2, cursor)) {
			const $v = op;
			let $w = null;
			if ($v[0] === 0) {
				const at = $v[1];
				const removed = $v[2];
				const inserted = $v[3];
				$x(out, at, removed.length, $k(inserted, g), $f);
				$w = undefined;
			} else if ($v[0] === 1) {
				const at2 = $v[1];
				const _was = $v[2];
				const value2 = $v[3];
				$w = $Z(out, at2, g(value2), $f);
			} else if ($v[0] === 2) {
				const items = $v[1];
				$w = $ad(out, $k(items, g), $f);
			} else {
				const from = $v[1];
				const count = $v[2];
				const to = $v[3];
				$w = $af(out, from, count, to, $f);
			}
			$w;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aE(source2, cursor);
	}, $g);
	return out;
}
function $aQ(self, op) {
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
function $aU(cell, $K) {
	$M(cell[1], $L(cell[2]), $K);
}
function $aM(self, at, removed, inserted, $y) {
	const held = $z(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aQ(self[2], $C(self[0].v, start, taking, inserted));
	$aU(self, $y);
}
function $aJ(self, value2, $aK) {
	return $aM(self, $z(self), 0, [ __clone(value2) ], $aK);
}
function $bb(elements) {
	return $n(elements, $b());
}
function $bg(self, at, removed, inserted, $y) {
	const held = $z(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aQ(self[2], $C(self[0].v, start, taking, inserted));
	$aU(self, $y);
}
function $br(self, at, value2, $aa) {
	const $bs = __list_get(self[0].v, at);
	let $bt = null;
	if ($bs[0] === 0) {
		const previous = $bs[1];
		__at_put(self[0].v, at, __clone(value2));
		$aQ(self[2], [ 1, at, previous, __clone(value2) ]);
		$aU(self, $aa);
		$bt = undefined;
	} else {
		$bt = undefined;
	}
	return $bt;
}
function $bu(self, value2, $ae) {
	self[0].v = __clone(value2);
	$aQ(self[2], [ 2, __clone(value2) ]);
	$aU(self, $ae);
}
function $bv(self, from, count, to, $ag) {
	if (count === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count) {
		lifted.push(__remove_at(self[0].v, from));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $bw = __list_get(lifted, offset);
		let $bx = null;
		if ($bw[0] === 0) {
			const value2 = $bw[1];
			$bx = __insert_at(self[0].v, to + offset, value2);
		} else {
			$bx = undefined;
		}
		$bx;
		offset = offset + 1;
	}
	$aQ(self[2], [ 3, from, count, to ]);
	$aU(self, $ag);
}
function $aZ(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $bb($k($j(source2), g));
	as_derivation();
	register_with_owner($aj(source2, (_published) => {
		for (const op of $o(source2, cursor)) {
			const $be = op;
			let $bf = null;
			if ($be[0] === 0) {
				const at = $be[1];
				const removed = $be[2];
				const inserted = $be[3];
				$bg(out, at, removed.length, $k(inserted, g), $f);
				$bf = undefined;
			} else if ($be[0] === 1) {
				const at2 = $be[1];
				const _was = $be[2];
				const value2 = $be[3];
				$bf = $br(out, at2, g(value2), $f);
			} else if ($be[0] === 2) {
				const items = $be[1];
				$bf = $bu(out, $k(items, g), $f);
			} else {
				const from = $be[1];
				const count = $be[2];
				const to = $be[3];
				$bf = $bv(out, from, count, to, $f);
			}
			$bf;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aE(source2, cursor);
	}, $g);
	return out;
}
function $bz(self, at, $bA) {
	return $aM(self, at, 1, [  ], $bA);
}
function $bB(self, values, $bC) {
	return $aM(self, $z(self), 0, values, $bC);
}
function $bD(self, at, value2, $bE) {
	return $aM(self, at, 0, [ __clone(value2) ], $bE);
}
function $bF(self, value2, $bG) {
	return $aM(self, 0, 0, [ __clone(value2) ], $bG);
}
function $bK(self, $bL) {
	const size = $z(self);
	let $bM = null;
	if (size > 0) {
		$bM = $aM(self, size - 1, 1, [  ], $bL);
	}
	return $bM;
}
function $bN(self, at, count, $bO) {
	return $aM(self, at, count, [  ], $bO);
}
function $bP(self, length, $bQ) {
	const size = $z(self);
	let $bR = null;
	if (size > length) {
		$bR = $aM(self, length, size - length, [  ], $bQ);
	}
	return $bR;
}
function $bS(self, at, values, $bT) {
	return $aM(self, at, 0, values, $bT);
}
function $bU(self, values, $bV) {
	return $aM(self, 0, $z(self), values, $bV);
}
function $bW(self, $bX) {
	return $aM(self, 0, $z(self), [  ], $bX);
}
function $bY(self) {
	return $z(self) === 0;
}
function $cb(self) {
	return self[0].length;
}
function $cc(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($C(self[0], start, taking, inserted));
}
function $bZ(self, value2, $ca) {
	return $cc(self, $cb(self), 0, [ __clone(value2) ], $ca);
}
function $cd(self, at, $ce) {
	return $cc(self, at, 1, [  ], $ce);
}
function $ch(elements) {
	return [ __clone(elements), [  ] ];
}
function $cf(self, body, $cg) {
	let recorder = $ch(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aQ(self[2], op);
	}
	$aU(self, $cg);
}
function $ci(target, $cj) {
	$bZ(target, "from-fill-1", $cj);
	$bZ(target, "from-fill-2", $cj);
}
function $cl(self, items, $cm) {
	const old_length = $z(self);
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(self[0].v, prefix) !== __at(items, prefix)) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(self[0].v, old_length - 1 - suffix) !== __at(items, new_length - 1 - suffix)) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $cn = __list_get(items, index);
		let $co = null;
		if ($cn[0] === 0) {
			const value2 = $cn[1];
			$co = arriving.push(value2);
		} else {
			$co = undefined;
		}
		$co;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aM(self, prefix, removed, arriving, $cm);
}
function $ct(body, $cu) {
	const $cv = $cu;
	let $cw = null;
	if ($cv[0] === 0) {
		const current = $cv[1];
		$cw = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cw = result;
	}
	return $cw;
}
function $cy(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $cx(elements, limit) {
	return $c(elements, $cy(limit));
}
function $cB(self) {
	return self[0].v.length;
}
function $cA(self) {
	return $cB(self[2]);
}
function $cC(elements) {
	return $c(elements, $b());
}
function $cD(self, at, count, $cE) {
	return $cc(self, at, count, [  ], $cE);
}
function $cF(self, at, value2, $cG) {
	return $cc(self, at, 0, [ __clone(value2) ], $cG);
}
function $cI(self) {
	return $i(self[2]);
}
function $cL(self, cursor) {
	const $cP = $p(self[2], cursor);
	let $cQ = null;
	if ($cP[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(self[0].v) ]);
		$cQ = lost;
	} else {
		const recorded = $cP[1];
		$cQ = recorded;
	}
	return $cQ;
}
function $cT(self, observer) {
	return $ak(self[1], (_sequence) => {
		return observer(self[0].v);
	});
}
function $cU(self, cursor) {
	$aF(self[2], cursor);
}
function $cH(source2, g, $f, $g) {
	const cursor = $cI(source2);
	const out = $bb($k($j(source2), g));
	as_derivation();
	register_with_owner($cT(source2, (_published) => {
		for (const op of $cL(source2, cursor)) {
			const $cR = op;
			let $cS = null;
			if ($cR[0] === 0) {
				const at = $cR[1];
				const removed = $cR[2];
				const inserted = $cR[3];
				$bg(out, at, removed.length, $k(inserted, g), $f);
				$cS = undefined;
			} else if ($cR[0] === 1) {
				const at2 = $cR[1];
				const _was = $cR[2];
				const value2 = $cR[3];
				$cS = $br(out, at2, g(value2), $f);
			} else if ($cR[0] === 2) {
				const items = $cR[1];
				$cS = $bu(out, $k(items, g), $f);
			} else {
				const from = $cR[1];
				const count = $cR[2];
				const to = $cR[3];
				$cS = $bv(out, from, count, to, $f);
			}
			$cS;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $cU(source2, cursor);
	}, $g);
	return out;
}
function $cX(self, value2, $aK) {
	return $bg(self, $z(self), 0, [ __clone(value2) ], $aK);
}
function $cY(self, at, value2, $bE) {
	return $bg(self, at, 0, [ __clone(value2) ], $bE);
}
function $cZ(self, at, $bA) {
	return $bg(self, at, 1, [  ], $bA);
}
function $da(self, $bL) {
	const size = $z(self);
	let $db = null;
	if (size > 0) {
		$db = $bg(self, size - 1, 1, [  ], $bL);
	}
	return $db;
}
function $dc(self, at, count, $bO) {
	return $bg(self, at, count, [  ], $bO);
}
function $dd(self, length, $bQ) {
	const size = $z(self);
	let $de = null;
	if (size > length) {
		$de = $bg(self, length, size - length, [  ], $bQ);
	}
	return $de;
}
function $df(self, items, $cm) {
	const old_length = $z(self);
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(self[0].v, prefix) !== __at(items, prefix)) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(self[0].v, old_length - 1 - suffix) !== __at(items, new_length - 1 - suffix)) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $dg = __list_get(items, index);
		let $dh = null;
		if ($dg[0] === 0) {
			const value2 = $dg[1];
			$dh = arriving.push(value2);
		} else {
			$dh = undefined;
		}
		$dh;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$bg(self, prefix, removed, arriving, $cm);
}
function $dl(self, value2, $ca) {
	return $cc(self, $cb(self), 0, [ __clone(value2) ], $ca);
}
function $dn(self, body, $cg) {
	let recorder = $ch(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aQ(self[2], op);
	}
	$aU(self, $cg);
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
const my_nums = $e(my_list, (x) => {
	console.log("ran");
	return x.length;
}, [ 1 ], [ 1 ]);
$aJ(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($j(my_nums), 0));
const source = $a();
const derived = $aZ(source, counted, [ 1 ], [ 1 ]);
$aj(source, (_list) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aJ(source, "aa", [ 1 ]);
$aJ(source, "bbb", [ 1 ]);
$aJ(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$bz(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$bB(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$bD(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$bF(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$br(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bK(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bN(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bP(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bS(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$bU(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$bW(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($bY(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$cf(source, (list) => {
	$bZ(list, "kk", [ 1 ]);
	$bZ(list, "lll", [ 1 ]);
	$cd(list, 0, [ 1 ]);
	$bZ(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($j(source)));
$cf(source, (list) => {
	$ci(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($j(source)));
const before_set = calls.v;
$bu(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$cl(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$cl(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$bv(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($j(source)));
const before_batch = notifications.v;
$ct(($cs) => {
	$aJ(source, "r", [ 0, $cs ]);
	$aJ(source, "ss", [ 0, $cs ]);
	$bz(source, 0, [ 0, $cs ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $cx([ "seed" ], 3);
const mirror = $aZ(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$ct(($cz) => {
	let round = 0;
	while (round < 8) {
		$aJ(lagging, "row-" + round, [ 0, $cz ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $cA(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $cC([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $aZ(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$cf(clamped, (list) => {
	$cD(list, 1, 99, [ 1 ]);
	$cd(list, 50, [ 1 ]);
	$cF(list, 0, "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bN(clamped, 1, 99, [ 1 ]);
$bz(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($j(clamped)));
const walk = $bb([ 1, 2, 3 ]);
const first = $cH(walk, doubled, [ 1 ], [ 1 ]);
const second = $cH(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$cT(walk, (_list) => {
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
	$ct(($cW) => {
		let made = 0;
		while (made < op_count) {
			const size = $z(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$cX(walk, next_random(100), [ 0, $cW ]);
			} else if (choice < 11) {
				$cY(walk, pick(size + 1), next_random(100), [ 0, $cW ]);
			} else if (choice < 13) {
				$cZ(walk, pick(size), [ 0, $cW ]);
			} else if (choice < 15) {
				$br(walk, pick(size), next_random(100), [ 0, $cW ]);
			} else if (choice < 17) {
				const from = pick(size);
				const count = 1 + pick(size - from);
				$bv(walk, from, count, pick(size - count + 1), [ 0, $cW ]);
			} else if (choice < 18) {
				$da(walk, [ 0, $cW ]);
			} else if (choice < 19 && size > 12) {
				$dc(walk, pick(size), 1 + pick(4), [ 0, $cW ]);
			} else if (choice < 20 && size > 20) {
				$dd(walk, pick(size), [ 0, $cW ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$bu(walk, fresh, [ 0, $cW ]);
			} else if (choice < 22) {
				let edited = $j(walk);
				__at_put(edited, pick(size), next_random(100));
				edited.push(next_random(100));
				$df(walk, edited, [ 0, $cW ]);
			} else {
				const at = pick(size);
				$dn(walk, (list) => {
					$cF(list, at, next_random(100), [ 0, $cW ]);
					$cd(list, 0, [ 0, $cW ]);
					$dl(list, next_random(100), [ 0, $cW ]);
					return;
				}, [ 0, $cW ]);
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
	for (const value of $j(walk)) {
		first_wanted.push(value * 2 + 1);
		second_wanted.push(value * 2 + 8);
		naive = naive + 1;
	}
	same("first", turn, $j(first), first_wanted);
	same("second", turn, $j(second), second_wanted);
	turn = turn + 1;
}
if (walk_calls.v + chain_calls.v >= naive) {
	(() => {
		throw "walk: " + walk_calls.v + " + " + chain_calls.v + " calls against a rerun of " + naive;
	})();
}
console.log("walk: turns=300 silent=" + silent + " length=" + $j(walk).length + " g=" + walk_calls.v + " h=" + chain_calls.v + " rerun=" + naive);
