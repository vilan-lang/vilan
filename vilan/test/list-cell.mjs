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
	let $bd = null;
	if (wrapped < 0) {
		$bd = wrapped + modulus;
	} else {
		$bd = wrapped;
	}
	return $bd;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $r = null;
	if (truncated > 0) {
		$r = truncated;
	} else {
		$r = 0;
	}
	return $r;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $be = null;
	if (wrapped >= half) {
		$be = wrapped - modulus;
	} else {
		$be = wrapped;
	}
	return $be;
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
	return $U(self[0].v) && $U(self[1].v);
}
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $T = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$T = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber));
		}
		$T;
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
				while (!($U(turn2[1].v)) && budget > 0) {
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
function dispose(self, $ay) {
	const $az = $ay;
	let $aA = null;
	if ($az[0] === 0) {
		const established = $az[1];
		$aA = [ 0, established ];
	} else {
		$aA = $V(draining_turns.v);
	}
	const ambient = $aA;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aB = [ 0, handle[0] ];
	let $aC = null;
	if ($aB[0] === 0) {
		const subscribers = $aB[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aC = undefined;
	} else {
		$aC = undefined;
	}
	$aC;
	const $aD = ambient;
	let $aE = null;
	if ($aD[0] === 0) {
		const turn2 = $aD[1];
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
		$aE = undefined;
	} else {
		$aE = undefined;
	}
	$aE;
	const $aF = handle[3].v;
	let $aG = null;
	if ($aF[0] === 0) {
		const release = $aF[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aG = undefined;
	} else {
		$aG = undefined;
	}
	return $aG;
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aJ = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const $aH = self[0].v[1];
		let $aI = null;
		if ($aH[0] === 0) {
			const list = $aH[1];
			$aI = list.v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = [ 0, __shared_new([ cleanup ]) ];
			$aI = undefined;
		}
		$aJ = $aI;
	}
	return $aJ;
}
function register_with_owner(subscription, $as, $at) {
	const $au = $at;
	let $av = null;
	if ($au[0] === 0) {
		const owner = $au[1];
		$av = $aw(owner, subscription, $as);
	} else {
		$av = __clone(subscription);
	}
	return $av;
}
function defer_to_owner(cleanup, $aM) {
	const $aN = $aM;
	let $aO = null;
	if ($aN[0] === 0) {
		const owner = $aN[1];
		$aO = defer(owner, cleanup);
	} else {
		$aO = undefined;
	}
	return $aO;
}
function splice_start(held, at) {
	let $B = null;
	if (at > held) {
		$B = held;
	} else {
		$B = at;
	}
	return $B;
}
function splice_count(held, start, removed) {
	let $C = null;
	if (start + removed > held) {
		$C = held - start;
	} else {
		$C = removed;
	}
	return $C;
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
function $q(self, cursor) {
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
		const $s = __list_get(held, index);
		let $t = null;
		if ($s[0] === 0) {
			const op = $s[1];
			$t = ops.push(op);
		} else {
			$t = undefined;
		}
		$t;
		index = index + 1;
	}
	return [ 0, ops ];
}
function $p(log, items, cursor) {
	const $u = $q(log, cursor);
	let $v = null;
	if ($u[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$v = lost;
	} else {
		const recorded = $u[1];
		$v = recorded;
	}
	return $v;
}
function $o(self) {
	const log = __clone(self[2]);
	const items = self[0];
	return (cursor) => {
		return $p(log, items, cursor);
	};
}
function $A(self) {
	return self[0].v.length;
}
function $D(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $E = __list_get(inserted, offset);
		let $F = null;
		if ($E[0] === 0) {
			const value2 = $E[1];
			$F = __insert_at(list, start + offset, value2);
		} else {
			$F = undefined;
		}
		$F;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function $H(self) {
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
			const $I = __list_get(held, index);
			let $J = null;
			if ($I[0] === 0) {
				const op = $I[1];
				$J = kept.push(op);
			} else {
				$J = undefined;
			}
			$J;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function $G(self, op) {
	$H(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $M(self) {
	return self[1].v;
}
function $U(self) {
	return self.length === 0;
}
function $V(self) {
	let $X = null;
	if ($U(self)) {
		$X = [ 1 ];
	} else {
		$X = __list_get(self, self.length - 1);
	}
	return $X;
}
function $P(self, $Q) {
	const $R = $Q;
	let $S = null;
	if ($R[0] === 0) {
		const turn2 = $R[1];
		$S = enqueue(turn2, self[1].v);
	} else {
		const $Y = $V(draining_turns.v);
		let $Z = null;
		if ($Y[0] === 0) {
			const draining = $Y[1];
			$Z = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$Z = undefined;
		}
		$S = $Z;
	}
	return $S;
}
function $N(self, value2, $O) {
	self[0].v = __clone(value2);
	$P(self, $O);
}
function $K(cell, $L) {
	$N(cell[1], $M(cell[2]), $L);
}
function $y(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$G(self[2], $D(self[0].v, start, taking, inserted));
	$K(self, $z);
}
function $aa(self, at, value2, $ab) {
	const $ac = __list_get(self[0].v, at);
	let $ad = null;
	if ($ac[0] === 0) {
		const previous = $ac[1];
		__at_put(self[0].v, at, __clone(value2));
		$G(self[2], [ 1, at, previous, __clone(value2) ]);
		$K(self, $ab);
		$ad = undefined;
	} else {
		$ad = undefined;
	}
	return $ad;
}
function $ae(self, value2, $af) {
	self[0].v = __clone(value2);
	$G(self[2], [ 2, __clone(value2) ]);
	$K(self, $af);
}
function $ag(self, from, count, to, $ah) {
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
		const $ai = __list_get(lifted, offset);
		let $aj = null;
		if ($ai[0] === 0) {
			const value2 = $ai[1];
			$aj = __insert_at(self[0].v, to + offset, value2);
		} else {
			$aj = undefined;
		}
		$aj;
		offset = offset + 1;
	}
	$G(self[2], [ 3, from, count, to ]);
	$K(self, $ah);
}
function $aq(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $an(signal, observer) {
	const cell = signal[0];
	return $aq(signal, mint_subscriber(() => {
		const $ao = [ 0, cell ];
		let $ap = null;
		if ($ao[0] === 0) {
			const live = $ao[1];
			$ap = observer(live.v);
		} else {
			$ap = undefined;
		}
		return $ap;
	}));
}
function $ar(self) {
	return __clone(self[0].v);
}
function $am(self, observer, immediately) {
	const subscription = $an(self, observer);
	if (immediately) {
		observer($ar(self));
	}
	return subscription;
}
function $al(self, observer, immediately) {
	const items = self[0];
	return $am(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function $ak(self, observer) {
	return $al(self, observer, false);
}
function $aw(self, item, $ax) {
	defer(self, () => {
		dispose(item, $ax);
		return;
	});
	return __clone(item);
}
function $aL(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $aK(self, cursor) {
	$aL(self[2], cursor);
}
function $e(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $l($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($ak(__clone(source2), (_published) => {
		for (const op of read(cursor)) {
			const $w = op;
			let $x = null;
			if ($w[0] === 0) {
				const at = $w[1];
				const removed = $w[2];
				const inserted = $w[3];
				$y(out, at, removed.length, $k(inserted, g), $f);
				$x = undefined;
			} else if ($w[0] === 1) {
				const at2 = $w[1];
				const _was = $w[2];
				const value2 = $w[3];
				$x = $aa(out, at2, g(value2), $f);
			} else if ($w[0] === 2) {
				const items = $w[1];
				$x = $ae(out, $k(items, g), $f);
			} else {
				const from = $w[1];
				const count = $w[2];
				const to = $w[3];
				$x = $ag(out, from, count, to, $f);
			}
			$x;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aK(source2, cursor);
	}, $g);
	return out;
}
function $aW(self, op) {
	$H(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $ba(cell, $L) {
	$N(cell[1], $M(cell[2]), $L);
}
function $aS(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aW(self[2], $D(self[0].v, start, taking, inserted));
	$ba(self, $z);
}
function $aP(self, value2, $aQ) {
	return $aS(self, $A(self), 0, [ __clone(value2) ], $aQ);
}
function $bh(elements) {
	return $n(elements, $b());
}
function $bm(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aW(self[2], $D(self[0].v, start, taking, inserted));
	$ba(self, $z);
}
function $bx(self, at, value2, $ab) {
	const $by = __list_get(self[0].v, at);
	let $bz = null;
	if ($by[0] === 0) {
		const previous = $by[1];
		__at_put(self[0].v, at, __clone(value2));
		$aW(self[2], [ 1, at, previous, __clone(value2) ]);
		$ba(self, $ab);
		$bz = undefined;
	} else {
		$bz = undefined;
	}
	return $bz;
}
function $bA(self, value2, $af) {
	self[0].v = __clone(value2);
	$aW(self[2], [ 2, __clone(value2) ]);
	$ba(self, $af);
}
function $bB(self, from, count, to, $ah) {
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
		const $bC = __list_get(lifted, offset);
		let $bD = null;
		if ($bC[0] === 0) {
			const value2 = $bC[1];
			$bD = __insert_at(self[0].v, to + offset, value2);
		} else {
			$bD = undefined;
		}
		$bD;
		offset = offset + 1;
	}
	$aW(self[2], [ 3, from, count, to ]);
	$ba(self, $ah);
}
function $bf(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $bh($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($ak(__clone(source2), (_published) => {
		for (const op of read(cursor)) {
			const $bk = op;
			let $bl = null;
			if ($bk[0] === 0) {
				const at = $bk[1];
				const removed = $bk[2];
				const inserted = $bk[3];
				$bm(out, at, removed.length, $k(inserted, g), $f);
				$bl = undefined;
			} else if ($bk[0] === 1) {
				const at2 = $bk[1];
				const _was = $bk[2];
				const value2 = $bk[3];
				$bl = $bx(out, at2, g(value2), $f);
			} else if ($bk[0] === 2) {
				const items = $bk[1];
				$bl = $bA(out, $k(items, g), $f);
			} else {
				const from = $bk[1];
				const count = $bk[2];
				const to = $bk[3];
				$bl = $bB(out, from, count, to, $f);
			}
			$bl;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aK(source2, cursor);
	}, $g);
	return out;
}
function $bE(self, observer) {
	return $al(self, observer, false);
}
function $bG(self, at, $bH) {
	return $aS(self, at, 1, [  ], $bH);
}
function $bI(self, values, $bJ) {
	return $aS(self, $A(self), 0, values, $bJ);
}
function $bK(self, at, value2, $bL) {
	return $aS(self, at, 0, [ __clone(value2) ], $bL);
}
function $bM(self, value2, $bN) {
	return $aS(self, 0, 0, [ __clone(value2) ], $bN);
}
function $bR(self, $bS) {
	const size = $A(self);
	let $bT = null;
	if (size > 0) {
		$bT = $aS(self, size - 1, 1, [  ], $bS);
	}
	return $bT;
}
function $bU(self, at, count, $bV) {
	return $aS(self, at, count, [  ], $bV);
}
function $bW(self, length, $bX) {
	const size = $A(self);
	let $bY = null;
	if (size > length) {
		$bY = $aS(self, length, size - length, [  ], $bX);
	}
	return $bY;
}
function $bZ(self, at, values, $ca) {
	return $aS(self, at, 0, values, $ca);
}
function $cb(self, values, $cc) {
	return $aS(self, 0, $A(self), values, $cc);
}
function $cd(self, $ce) {
	return $aS(self, 0, $A(self), [  ], $ce);
}
function $cf(self) {
	return $A(self) === 0;
}
function $ci(self) {
	return self[0].length;
}
function $cj(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($D(self[0], start, taking, inserted));
}
function $cg(self, value2, $ch) {
	return $cj(self, $ci(self), 0, [ __clone(value2) ], $ch);
}
function $ck(self, at, $cl) {
	return $cj(self, at, 1, [  ], $cl);
}
function $co(elements) {
	return [ __clone(elements), [  ] ];
}
function $cm(self, body, $cn) {
	let recorder = $co(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aW(self[2], op);
	}
	$ba(self, $cn);
}
function $cp(target, $cq) {
	$cg(target, "from-fill-1", $cq);
	$cg(target, "from-fill-2", $cq);
}
function $cs(self, items, $ct) {
	const old_length = $A(self);
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
		const $cu = __list_get(items, index);
		let $cv = null;
		if ($cu[0] === 0) {
			const value2 = $cu[1];
			$cv = arriving.push(value2);
		} else {
			$cv = undefined;
		}
		$cv;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aS(self, prefix, removed, arriving, $ct);
}
function $cA(body, $cB) {
	const $cC = $cB;
	let $cD = null;
	if ($cC[0] === 0) {
		const current = $cC[1];
		$cD = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cD = result;
	}
	return $cD;
}
function $cF(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $cE(elements, limit) {
	return $c(elements, $cF(limit));
}
function $cI(self) {
	return self[0].v.length;
}
function $cH(self) {
	return $cI(self[2]);
}
function $cJ(elements) {
	return $c(elements, $b());
}
function $cK(self, at, count, $cL) {
	return $cj(self, at, count, [  ], $cL);
}
function $cM(self, at, value2, $cN) {
	return $cj(self, at, 0, [ __clone(value2) ], $cN);
}
function $cP(self) {
	return $i(self[2]);
}
function $cT(log, items, cursor) {
	const $cX = $q(log, cursor);
	let $cY = null;
	if ($cX[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$cY = lost;
	} else {
		const recorded = $cX[1];
		$cY = recorded;
	}
	return $cY;
}
function $cS(self) {
	const log = __clone(self[2]);
	const items = self[0];
	return (cursor) => {
		return $cT(log, items, cursor);
	};
}
function $dc(self, observer, immediately) {
	const items = self[0];
	return $am(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function $db(self, observer) {
	return $dc(self, observer, false);
}
function $dd(self, cursor) {
	$aL(self[2], cursor);
}
function $cO(source2, g, $f, $g) {
	const cursor = $cP(source2);
	const out = $bh($k($j(source2), g));
	as_derivation();
	const read = $cS(source2);
	register_with_owner($db(__clone(source2), (_published) => {
		for (const op of read(cursor)) {
			const $cZ = op;
			let $da = null;
			if ($cZ[0] === 0) {
				const at = $cZ[1];
				const removed = $cZ[2];
				const inserted = $cZ[3];
				$bm(out, at, removed.length, $k(inserted, g), $f);
				$da = undefined;
			} else if ($cZ[0] === 1) {
				const at2 = $cZ[1];
				const _was = $cZ[2];
				const value2 = $cZ[3];
				$da = $bx(out, at2, g(value2), $f);
			} else if ($cZ[0] === 2) {
				const items = $cZ[1];
				$da = $bA(out, $k(items, g), $f);
			} else {
				const from = $cZ[1];
				const count = $cZ[2];
				const to = $cZ[3];
				$da = $bB(out, from, count, to, $f);
			}
			$da;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $dd(source2, cursor);
	}, $g);
	return out;
}
function $df(self, observer) {
	return $dc(self, observer, false);
}
function $dh(self, value2, $aQ) {
	return $bm(self, $A(self), 0, [ __clone(value2) ], $aQ);
}
function $di(self, at, value2, $bL) {
	return $bm(self, at, 0, [ __clone(value2) ], $bL);
}
function $dj(self, at, $bH) {
	return $bm(self, at, 1, [  ], $bH);
}
function $dk(self, $bS) {
	const size = $A(self);
	let $dl = null;
	if (size > 0) {
		$dl = $bm(self, size - 1, 1, [  ], $bS);
	}
	return $dl;
}
function $dm(self, at, count, $bV) {
	return $bm(self, at, count, [  ], $bV);
}
function $dn(self, length, $bX) {
	const size = $A(self);
	let $do = null;
	if (size > length) {
		$do = $bm(self, length, size - length, [  ], $bX);
	}
	return $do;
}
function $dp(self, items, $ct) {
	const old_length = $A(self);
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
		const $dq = __list_get(items, index);
		let $dr = null;
		if ($dq[0] === 0) {
			const value2 = $dq[1];
			$dr = arriving.push(value2);
		} else {
			$dr = undefined;
		}
		$dr;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$bm(self, prefix, removed, arriving, $ct);
}
function $dv(self, value2, $ch) {
	return $cj(self, $ci(self), 0, [ __clone(value2) ], $ch);
}
function $dx(self, body, $cn) {
	let recorder = $co(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aW(self[2], op);
	}
	$ba(self, $cn);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
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
$aP(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($j(my_nums), 0));
const source = $a();
const derived = $bf(source, counted, [ 1 ], [ 1 ]);
$bE(__clone(source), (_list) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aP(source, "aa", [ 1 ]);
$aP(source, "bbb", [ 1 ]);
$aP(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$bG(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$bI(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$bK(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$bM(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$bx(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bR(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bU(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bW(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bZ(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$cb(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$cd(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($cf(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$cm(source, (list) => {
	$cg(list, "kk", [ 1 ]);
	$cg(list, "lll", [ 1 ]);
	$ck(list, 0, [ 1 ]);
	$cg(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($j(source)));
$cm(source, (list) => {
	$cp(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($j(source)));
const before_set = calls.v;
$bA(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$cs(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$cs(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$bB(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($j(source)));
const before_batch = notifications.v;
$cA(($cz) => {
	$aP(source, "r", [ 0, $cz ]);
	$aP(source, "ss", [ 0, $cz ]);
	$bG(source, 0, [ 0, $cz ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $cE([ "seed" ], 3);
const mirror = $bf(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$cA(($cG) => {
	let round = 0;
	while (round < 8) {
		$aP(lagging, "row-" + round, [ 0, $cG ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $cH(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $cJ([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $bf(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$cm(clamped, (list) => {
	$cK(list, 1, 99, [ 1 ]);
	$ck(list, 50, [ 1 ]);
	$cM(list, 0, "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bU(clamped, 1, 99, [ 1 ]);
$bG(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($j(clamped)));
const walk = $bh([ 1, 2, 3 ]);
const first = $cO(walk, doubled, [ 1 ], [ 1 ]);
const second = $cO(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$df(__clone(walk), (_list) => {
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
	$cA(($dg) => {
		let made = 0;
		while (made < op_count) {
			const size = $A(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$dh(walk, next_random(100), [ 0, $dg ]);
			} else if (choice < 11) {
				$di(walk, pick(size + 1), next_random(100), [ 0, $dg ]);
			} else if (choice < 13) {
				$dj(walk, pick(size), [ 0, $dg ]);
			} else if (choice < 15) {
				$bx(walk, pick(size), next_random(100), [ 0, $dg ]);
			} else if (choice < 17) {
				const from = pick(size);
				const count = 1 + pick(size - from);
				$bB(walk, from, count, pick(size - count + 1), [ 0, $dg ]);
			} else if (choice < 18) {
				$dk(walk, [ 0, $dg ]);
			} else if (choice < 19 && size > 12) {
				$dm(walk, pick(size), 1 + pick(4), [ 0, $dg ]);
			} else if (choice < 20 && size > 20) {
				$dn(walk, pick(size), [ 0, $dg ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$bA(walk, fresh, [ 0, $dg ]);
			} else if (choice < 22) {
				let edited = $j(walk);
				__at_put(edited, pick(size), next_random(100));
				edited.push(next_random(100));
				$dp(walk, edited, [ 0, $dg ]);
			} else {
				const at = pick(size);
				$dx(walk, (list) => {
					$cM(list, at, next_random(100), [ 0, $dg ]);
					$ck(list, 0, [ 0, $dg ]);
					$dv(list, next_random(100), [ 0, $dg ]);
					return;
				}, [ 0, $dg ]);
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
