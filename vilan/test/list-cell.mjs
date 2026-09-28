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
	let $aY = null;
	if (wrapped < 0) {
		$aY = wrapped + modulus;
	} else {
		$aY = wrapped;
	}
	return $aY;
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
	let $aZ = null;
	if (wrapped >= half) {
		$aZ = wrapped - modulus;
	} else {
		$aZ = wrapped;
	}
	return $aZ;
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
function dispose(self, $aw) {
	const $ax = $aw;
	let $ay = null;
	if ($ax[0] === 0) {
		const established = $ax[1];
		$ay = [ 0, established ];
	} else {
		$ay = $V(draining_turns.v);
	}
	const ambient = $ay;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $az = [ 0, handle[0] ];
	let $aA = null;
	if ($az[0] === 0) {
		const subscribers = $az[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aA = undefined;
	} else {
		$aA = undefined;
	}
	$aA;
	const $aB = ambient;
	let $aC = null;
	if ($aB[0] === 0) {
		const turn2 = $aB[1];
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
		$aC = undefined;
	} else {
		$aC = undefined;
	}
	$aC;
	const $aD = handle[3].v;
	let $aE = null;
	if ($aD[0] === 0) {
		const release = $aD[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aE = undefined;
	} else {
		$aE = undefined;
	}
	return $aE;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $aq, $ar) {
	const $as = $ar;
	let $at = null;
	if ($as[0] === 0) {
		const owner = $as[1];
		$at = $au(owner, subscription, $aq);
	} else {
		$at = __clone(subscription);
	}
	return $at;
}
function defer_to_owner(cleanup, $aH) {
	const $aI = $aH;
	let $aJ = null;
	if ($aI[0] === 0) {
		const owner = $aI[1];
		$aJ = defer(owner, cleanup);
	} else {
		$aJ = undefined;
	}
	return $aJ;
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
function $ap(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $am(signal, observer) {
	const cell = signal[0];
	return $ap(signal, mint_subscriber(() => {
		const $an = [ 0, cell ];
		let $ao = null;
		if ($an[0] === 0) {
			const live = $an[1];
			$ao = observer(live.v);
		} else {
			$ao = undefined;
		}
		return $ao;
	}));
}
function $al(self, observer) {
	return $am(self, observer);
}
function $ak(self, observer) {
	const items = self[0];
	return $al(self[1], (_sequence) => {
		return observer(items.v);
	});
}
function $au(self, item, $av) {
	if (self[1].v) {
		dispose(item, $av);
	} else {
		self[0].v.push(() => {
			dispose(item, $av);
			return;
		});
	}
	return __clone(item);
}
function $aG(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $aF(self, cursor) {
	$aG(self[2], cursor);
}
function $e(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $l($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($ak(source2, (_published) => {
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
		return $aF(source2, cursor);
	}, $g);
	return out;
}
function $aR(self, op) {
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
function $aV(cell, $L) {
	$N(cell[1], $M(cell[2]), $L);
}
function $aN(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aR(self[2], $D(self[0].v, start, taking, inserted));
	$aV(self, $z);
}
function $aK(self, value2, $aL) {
	return $aN(self, $A(self), 0, [ __clone(value2) ], $aL);
}
function $bc(elements) {
	return $n(elements, $b());
}
function $bh(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aR(self[2], $D(self[0].v, start, taking, inserted));
	$aV(self, $z);
}
function $bs(self, at, value2, $ab) {
	const $bt = __list_get(self[0].v, at);
	let $bu = null;
	if ($bt[0] === 0) {
		const previous = $bt[1];
		__at_put(self[0].v, at, __clone(value2));
		$aR(self[2], [ 1, at, previous, __clone(value2) ]);
		$aV(self, $ab);
		$bu = undefined;
	} else {
		$bu = undefined;
	}
	return $bu;
}
function $bv(self, value2, $af) {
	self[0].v = __clone(value2);
	$aR(self[2], [ 2, __clone(value2) ]);
	$aV(self, $af);
}
function $bw(self, from, count, to, $ah) {
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
		const $bx = __list_get(lifted, offset);
		let $by = null;
		if ($bx[0] === 0) {
			const value2 = $bx[1];
			$by = __insert_at(self[0].v, to + offset, value2);
		} else {
			$by = undefined;
		}
		$by;
		offset = offset + 1;
	}
	$aR(self[2], [ 3, from, count, to ]);
	$aV(self, $ah);
}
function $ba(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $bc($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($ak(source2, (_published) => {
		for (const op of read(cursor)) {
			const $bf = op;
			let $bg = null;
			if ($bf[0] === 0) {
				const at = $bf[1];
				const removed = $bf[2];
				const inserted = $bf[3];
				$bh(out, at, removed.length, $k(inserted, g), $f);
				$bg = undefined;
			} else if ($bf[0] === 1) {
				const at2 = $bf[1];
				const _was = $bf[2];
				const value2 = $bf[3];
				$bg = $bs(out, at2, g(value2), $f);
			} else if ($bf[0] === 2) {
				const items = $bf[1];
				$bg = $bv(out, $k(items, g), $f);
			} else {
				const from = $bf[1];
				const count = $bf[2];
				const to = $bf[3];
				$bg = $bw(out, from, count, to, $f);
			}
			$bg;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aF(source2, cursor);
	}, $g);
	return out;
}
function $bA(self, at, $bB) {
	return $aN(self, at, 1, [  ], $bB);
}
function $bC(self, values, $bD) {
	return $aN(self, $A(self), 0, values, $bD);
}
function $bE(self, at, value2, $bF) {
	return $aN(self, at, 0, [ __clone(value2) ], $bF);
}
function $bG(self, value2, $bH) {
	return $aN(self, 0, 0, [ __clone(value2) ], $bH);
}
function $bL(self, $bM) {
	const size = $A(self);
	let $bN = null;
	if (size > 0) {
		$bN = $aN(self, size - 1, 1, [  ], $bM);
	}
	return $bN;
}
function $bO(self, at, count, $bP) {
	return $aN(self, at, count, [  ], $bP);
}
function $bQ(self, length, $bR) {
	const size = $A(self);
	let $bS = null;
	if (size > length) {
		$bS = $aN(self, length, size - length, [  ], $bR);
	}
	return $bS;
}
function $bT(self, at, values, $bU) {
	return $aN(self, at, 0, values, $bU);
}
function $bV(self, values, $bW) {
	return $aN(self, 0, $A(self), values, $bW);
}
function $bX(self, $bY) {
	return $aN(self, 0, $A(self), [  ], $bY);
}
function $bZ(self) {
	return $A(self) === 0;
}
function $cc(self) {
	return self[0].length;
}
function $cd(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($D(self[0], start, taking, inserted));
}
function $ca(self, value2, $cb) {
	return $cd(self, $cc(self), 0, [ __clone(value2) ], $cb);
}
function $ce(self, at, $cf) {
	return $cd(self, at, 1, [  ], $cf);
}
function $ci(elements) {
	return [ __clone(elements), [  ] ];
}
function $cg(self, body, $ch) {
	let recorder = $ci(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aR(self[2], op);
	}
	$aV(self, $ch);
}
function $cj(target, $ck) {
	$ca(target, "from-fill-1", $ck);
	$ca(target, "from-fill-2", $ck);
}
function $cm(self, items, $cn) {
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
		const $co = __list_get(items, index);
		let $cp = null;
		if ($co[0] === 0) {
			const value2 = $co[1];
			$cp = arriving.push(value2);
		} else {
			$cp = undefined;
		}
		$cp;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aN(self, prefix, removed, arriving, $cn);
}
function $cu(body, $cv) {
	const $cw = $cv;
	let $cx = null;
	if ($cw[0] === 0) {
		const current = $cw[1];
		$cx = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cx = result;
	}
	return $cx;
}
function $cz(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $cy(elements, limit) {
	return $c(elements, $cz(limit));
}
function $cC(self) {
	return self[0].v.length;
}
function $cB(self) {
	return $cC(self[2]);
}
function $cD(elements) {
	return $c(elements, $b());
}
function $cE(self, at, count, $cF) {
	return $cd(self, at, count, [  ], $cF);
}
function $cG(self, at, value2, $cH) {
	return $cd(self, at, 0, [ __clone(value2) ], $cH);
}
function $cJ(self) {
	return $i(self[2]);
}
function $cN(log, items, cursor) {
	const $cR = $q(log, cursor);
	let $cS = null;
	if ($cR[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$cS = lost;
	} else {
		const recorded = $cR[1];
		$cS = recorded;
	}
	return $cS;
}
function $cM(self) {
	const log = __clone(self[2]);
	const items = self[0];
	return (cursor) => {
		return $cN(log, items, cursor);
	};
}
function $cV(self, observer) {
	const items = self[0];
	return $al(self[1], (_sequence) => {
		return observer(items.v);
	});
}
function $cW(self, cursor) {
	$aG(self[2], cursor);
}
function $cI(source2, g, $f, $g) {
	const cursor = $cJ(source2);
	const out = $bc($k($j(source2), g));
	as_derivation();
	const read = $cM(source2);
	register_with_owner($cV(source2, (_published) => {
		for (const op of read(cursor)) {
			const $cT = op;
			let $cU = null;
			if ($cT[0] === 0) {
				const at = $cT[1];
				const removed = $cT[2];
				const inserted = $cT[3];
				$bh(out, at, removed.length, $k(inserted, g), $f);
				$cU = undefined;
			} else if ($cT[0] === 1) {
				const at2 = $cT[1];
				const _was = $cT[2];
				const value2 = $cT[3];
				$cU = $bs(out, at2, g(value2), $f);
			} else if ($cT[0] === 2) {
				const items = $cT[1];
				$cU = $bv(out, $k(items, g), $f);
			} else {
				const from = $cT[1];
				const count = $cT[2];
				const to = $cT[3];
				$cU = $bw(out, from, count, to, $f);
			}
			$cU;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $cW(source2, cursor);
	}, $g);
	return out;
}
function $cZ(self, value2, $aL) {
	return $bh(self, $A(self), 0, [ __clone(value2) ], $aL);
}
function $da(self, at, value2, $bF) {
	return $bh(self, at, 0, [ __clone(value2) ], $bF);
}
function $db(self, at, $bB) {
	return $bh(self, at, 1, [  ], $bB);
}
function $dc(self, $bM) {
	const size = $A(self);
	let $dd = null;
	if (size > 0) {
		$dd = $bh(self, size - 1, 1, [  ], $bM);
	}
	return $dd;
}
function $de(self, at, count, $bP) {
	return $bh(self, at, count, [  ], $bP);
}
function $df(self, length, $bR) {
	const size = $A(self);
	let $dg = null;
	if (size > length) {
		$dg = $bh(self, length, size - length, [  ], $bR);
	}
	return $dg;
}
function $dh(self, items, $cn) {
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
		const $di = __list_get(items, index);
		let $dj = null;
		if ($di[0] === 0) {
			const value2 = $di[1];
			$dj = arriving.push(value2);
		} else {
			$dj = undefined;
		}
		$dj;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$bh(self, prefix, removed, arriving, $cn);
}
function $dn(self, value2, $cb) {
	return $cd(self, $cc(self), 0, [ __clone(value2) ], $cb);
}
function $dp(self, body, $ch) {
	let recorder = $ci(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aR(self[2], op);
	}
	$aV(self, $ch);
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
$aK(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($j(my_nums), 0));
const source = $a();
const derived = $ba(source, counted, [ 1 ], [ 1 ]);
$ak(source, (_list) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aK(source, "aa", [ 1 ]);
$aK(source, "bbb", [ 1 ]);
$aK(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$bA(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$bC(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$bE(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$bG(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$bs(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bL(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bO(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bQ(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bT(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$bV(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$bX(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($bZ(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$cg(source, (list) => {
	$ca(list, "kk", [ 1 ]);
	$ca(list, "lll", [ 1 ]);
	$ce(list, 0, [ 1 ]);
	$ca(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($j(source)));
$cg(source, (list) => {
	$cj(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($j(source)));
const before_set = calls.v;
$bv(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$cm(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$cm(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$bw(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($j(source)));
const before_batch = notifications.v;
$cu(($ct) => {
	$aK(source, "r", [ 0, $ct ]);
	$aK(source, "ss", [ 0, $ct ]);
	$bA(source, 0, [ 0, $ct ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $cy([ "seed" ], 3);
const mirror = $ba(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$cu(($cA) => {
	let round = 0;
	while (round < 8) {
		$aK(lagging, "row-" + round, [ 0, $cA ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $cB(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $cD([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $ba(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$cg(clamped, (list) => {
	$cE(list, 1, 99, [ 1 ]);
	$ce(list, 50, [ 1 ]);
	$cG(list, 0, "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bO(clamped, 1, 99, [ 1 ]);
$bA(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($j(clamped)));
const walk = $bc([ 1, 2, 3 ]);
const first = $cI(walk, doubled, [ 1 ], [ 1 ]);
const second = $cI(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$cV(walk, (_list) => {
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
	$cu(($cY) => {
		let made = 0;
		while (made < op_count) {
			const size = $A(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$cZ(walk, next_random(100), [ 0, $cY ]);
			} else if (choice < 11) {
				$da(walk, pick(size + 1), next_random(100), [ 0, $cY ]);
			} else if (choice < 13) {
				$db(walk, pick(size), [ 0, $cY ]);
			} else if (choice < 15) {
				$bs(walk, pick(size), next_random(100), [ 0, $cY ]);
			} else if (choice < 17) {
				const from = pick(size);
				const count = 1 + pick(size - from);
				$bw(walk, from, count, pick(size - count + 1), [ 0, $cY ]);
			} else if (choice < 18) {
				$dc(walk, [ 0, $cY ]);
			} else if (choice < 19 && size > 12) {
				$de(walk, pick(size), 1 + pick(4), [ 0, $cY ]);
			} else if (choice < 20 && size > 20) {
				$df(walk, pick(size), [ 0, $cY ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$bv(walk, fresh, [ 0, $cY ]);
			} else if (choice < 22) {
				let edited = $j(walk);
				__at_put(edited, pick(size), next_random(100));
				edited.push(next_random(100));
				$dh(walk, edited, [ 0, $cY ]);
			} else {
				const at = pick(size);
				$dp(walk, (list) => {
					$cG(list, at, next_random(100), [ 0, $cY ]);
					$ce(list, 0, [ 0, $cY ]);
					$dn(list, next_random(100), [ 0, $cY ]);
					return;
				}, [ 0, $cY ]);
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
