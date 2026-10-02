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
	let $ba = null;
	if (wrapped < 0) {
		$ba = wrapped + modulus;
	} else {
		$ba = wrapped;
	}
	return $ba;
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
	let $bb = null;
	if (wrapped >= half) {
		$bb = wrapped - modulus;
	} else {
		$bb = wrapped;
	}
	return $bb;
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
function dispose(self, $ax) {
	const $ay = $ax;
	let $az = null;
	if ($ay[0] === 0) {
		const established = $ay[1];
		$az = [ 0, established ];
	} else {
		$az = $V(draining_turns.v);
	}
	const ambient = $az;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aA = [ 0, handle[0] ];
	let $aB = null;
	if ($aA[0] === 0) {
		const subscribers = $aA[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aB = undefined;
	} else {
		$aB = undefined;
	}
	$aB;
	const $aC = ambient;
	let $aD = null;
	if ($aC[0] === 0) {
		const turn2 = $aC[1];
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
		$aD = undefined;
	} else {
		$aD = undefined;
	}
	$aD;
	const $aE = handle[3].v;
	let $aF = null;
	if ($aE[0] === 0) {
		const release = $aE[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aF = undefined;
	} else {
		$aF = undefined;
	}
	return $aF;
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aG = null;
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
		$aG = undefined;
	}
	return $aG;
}
function register_with_owner(subscription, $ar, $as) {
	const $at = $as;
	let $au = null;
	if ($at[0] === 0) {
		const owner = $at[1];
		$au = $av(owner, subscription, $ar);
	} else {
		$au = __clone(subscription);
	}
	return $au;
}
function defer_to_owner(cleanup, $aJ) {
	const $aK = $aJ;
	let $aL = null;
	if ($aK[0] === 0) {
		const owner = $aK[1];
		$aL = defer(owner, cleanup);
	} else {
		$aL = undefined;
	}
	return $aL;
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
		$S = enqueue(turn2, __clone(self[1].v));
	} else {
		const $Y = $V(draining_turns.v);
		let $Z = null;
		if ($Y[0] === 0) {
			const draining = $Y[1];
			$Z = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
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
function $aq(self) {
	return __clone(self[0].v);
}
function $al(self, observer, immediately) {
	const subscription = $am(self, observer);
	if (immediately) {
		observer($aq(self));
	}
	return subscription;
}
function $ak(self, observer, immediately) {
	const items = self[0];
	return $al(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function $av(self, item, $aw) {
	defer(self, () => {
		dispose(item, $aw);
		return;
	});
	return __clone(item);
}
function $aI(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $aH(self, cursor) {
	$aI(self[2], cursor);
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
	}, false), $f, $g);
	defer_to_owner(() => {
		return $aH(source2, cursor);
	}, $g);
	return out;
}
function $aT(self, op) {
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
function $aX(cell, $L) {
	$N(cell[1], $M(cell[2]), $L);
}
function $aP(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aT(self[2], $D(self[0].v, start, taking, inserted));
	$aX(self, $z);
}
function $aM(self, value2, $aN) {
	return $aP(self, $A(self), 0, [ __clone(value2) ], $aN);
}
function $be(elements) {
	return $n(elements, $b());
}
function $bj(self, at, removed, inserted, $z) {
	const held = $A(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aT(self[2], $D(self[0].v, start, taking, inserted));
	$aX(self, $z);
}
function $bu(self, at, value2, $ab) {
	const $bv = __list_get(self[0].v, at);
	let $bw = null;
	if ($bv[0] === 0) {
		const previous = $bv[1];
		__at_put(self[0].v, at, __clone(value2));
		$aT(self[2], [ 1, at, previous, __clone(value2) ]);
		$aX(self, $ab);
		$bw = undefined;
	} else {
		$bw = undefined;
	}
	return $bw;
}
function $bx(self, value2, $af) {
	self[0].v = __clone(value2);
	$aT(self[2], [ 2, __clone(value2) ]);
	$aX(self, $af);
}
function $by(self, from, count, to, $ah) {
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
		const $bz = __list_get(lifted, offset);
		let $bA = null;
		if ($bz[0] === 0) {
			const value2 = $bz[1];
			$bA = __insert_at(self[0].v, to + offset, value2);
		} else {
			$bA = undefined;
		}
		$bA;
		offset = offset + 1;
	}
	$aT(self[2], [ 3, from, count, to ]);
	$aX(self, $ah);
}
function $bc(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $be($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($ak(source2, (_published) => {
		for (const op of read(cursor)) {
			const $bh = op;
			let $bi = null;
			if ($bh[0] === 0) {
				const at = $bh[1];
				const removed = $bh[2];
				const inserted = $bh[3];
				$bj(out, at, removed.length, $k(inserted, g), $f);
				$bi = undefined;
			} else if ($bh[0] === 1) {
				const at2 = $bh[1];
				const _was = $bh[2];
				const value2 = $bh[3];
				$bi = $bu(out, at2, g(value2), $f);
			} else if ($bh[0] === 2) {
				const items = $bh[1];
				$bi = $bx(out, $k(items, g), $f);
			} else {
				const from = $bh[1];
				const count = $bh[2];
				const to = $bh[3];
				$bi = $by(out, from, count, to, $f);
			}
			$bi;
		}
		return;
	}, false), $f, $g);
	defer_to_owner(() => {
		return $aH(source2, cursor);
	}, $g);
	return out;
}
function $bC(self, observer) {
	return $ak(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function $bE(self, at, $bF) {
	return $aP(self, at, 1, [  ], $bF);
}
function $bG(self, values, $bH) {
	return $aP(self, $A(self), 0, values, $bH);
}
function $bI(self, at, value2, $bJ) {
	return $aP(self, at, 0, [ __clone(value2) ], $bJ);
}
function $bK(self, value2, $bL) {
	return $aP(self, 0, 0, [ __clone(value2) ], $bL);
}
function $bP(self, $bQ) {
	const size = $A(self);
	let $bR = null;
	if (size > 0) {
		$bR = $aP(self, size - 1, 1, [  ], $bQ);
	}
	return $bR;
}
function $bS(self, at, count, $bT) {
	return $aP(self, at, count, [  ], $bT);
}
function $bU(self, length, $bV) {
	const size = $A(self);
	let $bW = null;
	if (size > length) {
		$bW = $aP(self, length, size - length, [  ], $bV);
	}
	return $bW;
}
function $bX(self, at, values, $bY) {
	return $aP(self, at, 0, values, $bY);
}
function $bZ(self, values, $ca) {
	return $aP(self, 0, $A(self), values, $ca);
}
function $cb(self, $cc) {
	return $aP(self, 0, $A(self), [  ], $cc);
}
function $cd(self) {
	return $A(self) === 0;
}
function $cg(self) {
	return self[0].length;
}
function $ch(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($D(self[0], start, taking, inserted));
}
function $ce(self, value2, $cf) {
	return $ch(self, $cg(self), 0, [ __clone(value2) ], $cf);
}
function $ci(self, at, $cj) {
	return $ch(self, at, 1, [  ], $cj);
}
function $cm(elements) {
	return [ elements, [  ] ];
}
function $ck(self, body, $cl) {
	let recorder = $cm(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aT(self[2], op);
	}
	$aX(self, $cl);
}
function $cn(target, $co) {
	$ce(target, "from-fill-1", $co);
	$ce(target, "from-fill-2", $co);
}
function $cq(self, items, $cr) {
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
		const $cs = __list_get(items, index);
		let $ct = null;
		if ($cs[0] === 0) {
			const value2 = $cs[1];
			$ct = arriving.push(value2);
		} else {
			$ct = undefined;
		}
		$ct;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aP(self, prefix, removed, arriving, $cr);
}
function $cy(body, $cz) {
	const $cA = $cz;
	let $cB = null;
	if ($cA[0] === 0) {
		const current = $cA[1];
		$cB = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$cB = result;
	}
	return $cB;
}
function $cD(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $cC(elements, limit) {
	return $c(elements, $cD(limit));
}
function $cG(self) {
	return self[0].v.length;
}
function $cF(self) {
	return $cG(self[2]);
}
function $cH(elements) {
	return $c(elements, $b());
}
function $cI(self, at, count, $cJ) {
	return $ch(self, at, count, [  ], $cJ);
}
function $cK(self, at, value2, $cL) {
	return $ch(self, at, 0, [ __clone(value2) ], $cL);
}
function $cN(self) {
	return $i(self[2]);
}
function $cR(log, items, cursor) {
	const $cV = $q(log, cursor);
	let $cW = null;
	if ($cV[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$cW = lost;
	} else {
		const recorded = $cV[1];
		$cW = recorded;
	}
	return $cW;
}
function $cQ(self) {
	const log = __clone(self[2]);
	const items = self[0];
	return (cursor) => {
		return $cR(log, items, cursor);
	};
}
function $cZ(self, observer, immediately) {
	const items = self[0];
	return $al(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function $da(self, cursor) {
	$aI(self[2], cursor);
}
function $cM(source2, g, $f, $g) {
	const cursor = $cN(source2);
	const out = $be($k($j(source2), g));
	as_derivation();
	const read = $cQ(source2);
	register_with_owner($cZ(source2, (_published) => {
		for (const op of read(cursor)) {
			const $cX = op;
			let $cY = null;
			if ($cX[0] === 0) {
				const at = $cX[1];
				const removed = $cX[2];
				const inserted = $cX[3];
				$bj(out, at, removed.length, $k(inserted, g), $f);
				$cY = undefined;
			} else if ($cX[0] === 1) {
				const at2 = $cX[1];
				const _was = $cX[2];
				const value2 = $cX[3];
				$cY = $bu(out, at2, g(value2), $f);
			} else if ($cX[0] === 2) {
				const items = $cX[1];
				$cY = $bx(out, $k(items, g), $f);
			} else {
				const from = $cX[1];
				const count = $cX[2];
				const to = $cX[3];
				$cY = $by(out, from, count, to, $f);
			}
			$cY;
		}
		return;
	}, false), $f, $g);
	defer_to_owner(() => {
		return $da(source2, cursor);
	}, $g);
	return out;
}
function $dd(self, observer) {
	return $cZ(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function $df(self, value2, $aN) {
	return $bj(self, $A(self), 0, [ __clone(value2) ], $aN);
}
function $dg(self, at, value2, $bJ) {
	return $bj(self, at, 0, [ __clone(value2) ], $bJ);
}
function $dh(self, at, $bF) {
	return $bj(self, at, 1, [  ], $bF);
}
function $di(self, $bQ) {
	const size = $A(self);
	let $dj = null;
	if (size > 0) {
		$dj = $bj(self, size - 1, 1, [  ], $bQ);
	}
	return $dj;
}
function $dk(self, at, count, $bT) {
	return $bj(self, at, count, [  ], $bT);
}
function $dl(self, length, $bV) {
	const size = $A(self);
	let $dm = null;
	if (size > length) {
		$dm = $bj(self, length, size - length, [  ], $bV);
	}
	return $dm;
}
function $dn(self, items, $cr) {
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
		const $do = __list_get(items, index);
		let $dp = null;
		if ($do[0] === 0) {
			const value2 = $do[1];
			$dp = arriving.push(value2);
		} else {
			$dp = undefined;
		}
		$dp;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$bj(self, prefix, removed, arriving, $cr);
}
function $dt(self, value2, $cf) {
	return $ch(self, $cg(self), 0, [ __clone(value2) ], $cf);
}
function $dv(self, body, $cl) {
	let recorder = $cm(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aT(self[2], op);
	}
	$aX(self, $cl);
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
$aM(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($j(my_nums), 0));
const source = $a();
const derived = $bc(source, counted, [ 1 ], [ 1 ]);
$bC(__clone(source), (_list, $bB) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aM(source, "aa", [ 1 ]);
$aM(source, "bbb", [ 1 ]);
$aM(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$bE(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$bG(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$bI(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$bK(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$bu(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bP(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bS(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bU(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bX(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$bZ(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$cb(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($cd(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$ck(source, (list) => {
	$ce(list, "kk", [ 1 ]);
	$ce(list, "lll", [ 1 ]);
	$ci(list, 0, [ 1 ]);
	$ce(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($j(source)));
$ck(source, (list) => {
	$cn(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($j(source)));
const before_set = calls.v;
$bx(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$cq(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$cq(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$by(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($j(source)));
const before_batch = notifications.v;
$cy(($cx) => {
	$aM(source, "r", [ 0, $cx ]);
	$aM(source, "ss", [ 0, $cx ]);
	$bE(source, 0, [ 0, $cx ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $cC([ "seed" ], 3);
const mirror = $bc(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$cy(($cE) => {
	let round = 0;
	while (round < 8) {
		$aM(lagging, "row-" + round, [ 0, $cE ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $cF(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $cH([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $bc(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$ck(clamped, (list) => {
	$cI(list, 1, 99, [ 1 ]);
	$ci(list, 50, [ 1 ]);
	$cK(list, 0, "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bS(clamped, 1, 99, [ 1 ]);
$bE(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($j(clamped)));
const walk = $be([ 1, 2, 3 ]);
const first = $cM(walk, doubled, [ 1 ], [ 1 ]);
const second = $cM(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$dd(__clone(walk), (_list, $dc) => {
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
	$cy(($de) => {
		let made = 0;
		while (made < op_count) {
			const size = $A(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$df(walk, next_random(100), [ 0, $de ]);
			} else if (choice < 11) {
				$dg(walk, pick(size + 1), next_random(100), [ 0, $de ]);
			} else if (choice < 13) {
				$dh(walk, pick(size), [ 0, $de ]);
			} else if (choice < 15) {
				$bu(walk, pick(size), next_random(100), [ 0, $de ]);
			} else if (choice < 17) {
				const from = pick(size);
				const count = 1 + pick(size - from);
				$by(walk, from, count, pick(size - count + 1), [ 0, $de ]);
			} else if (choice < 18) {
				$di(walk, [ 0, $de ]);
			} else if (choice < 19 && size > 12) {
				$dk(walk, pick(size), 1 + pick(4), [ 0, $de ]);
			} else if (choice < 20 && size > 20) {
				$dl(walk, pick(size), [ 0, $de ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$bx(walk, fresh, [ 0, $de ]);
			} else if (choice < 22) {
				let edited = $j(walk);
				__at_put(edited, pick(size), next_random(100));
				edited.push(next_random(100));
				$dn(walk, edited, [ 0, $de ]);
			} else {
				const at = pick(size);
				$dv(walk, (list) => {
					$cK(list, at, next_random(100), [ 0, $de ]);
					$ci(list, 0, [ 0, $de ]);
					$dt(list, next_random(100), [ 0, $de ]);
					return;
				}, [ 0, $de ]);
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
