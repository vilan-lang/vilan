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
	let $bc = null;
	if (wrapped < 0) {
		$bc = wrapped + modulus;
	} else {
		$bc = wrapped;
	}
	return $bc;
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
	let $bd = null;
	if (wrapped >= half) {
		$bd = wrapped - modulus;
	} else {
		$bd = wrapped;
	}
	return $bd;
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
	return $V(self[0].v) && $V(self[1].v);
}
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $U = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$U = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber));
		}
		$U;
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
				while (!($V(turn2[1].v)) && budget > 0) {
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
function dispose(self, $az) {
	const $aA = $az;
	let $aB = null;
	if ($aA[0] === 0) {
		const established = $aA[1];
		$aB = [ 0, established ];
	} else {
		$aB = $W(draining_turns.v);
	}
	const ambient = $aB;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aC = [ 0, handle[0] ];
	let $aD = null;
	if ($aC[0] === 0) {
		const subscribers = $aC[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aD = undefined;
	} else {
		$aD = undefined;
	}
	$aD;
	const $aE = ambient;
	let $aF = null;
	if ($aE[0] === 0) {
		const turn2 = $aE[1];
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
		$aF = undefined;
	} else {
		$aF = undefined;
	}
	$aF;
	const $aG = handle[3].v;
	let $aH = null;
	if ($aG[0] === 0) {
		const release = $aG[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aH = undefined;
	} else {
		$aH = undefined;
	}
	return $aH;
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aI = null;
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
		$aI = undefined;
	}
	return $aI;
}
function register_with_owner(subscription, $at, $au) {
	const $av = $au;
	let $aw = null;
	if ($av[0] === 0) {
		const owner = $av[1];
		$aw = $ax(owner, subscription, $at);
	} else {
		$aw = __clone(subscription);
	}
	return $aw;
}
function defer_to_owner(cleanup, $aL) {
	const $aM = $aL;
	let $aN = null;
	if ($aM[0] === 0) {
		const owner = $aM[1];
		$aN = defer(owner, cleanup);
	} else {
		$aN = undefined;
	}
	return $aN;
}
function splice_start(held, at) {
	let $C = null;
	if (at > held) {
		$C = held;
	} else {
		$C = at;
	}
	return $C;
}
function splice_count(held, start, removed) {
	let $D = null;
	if (start + removed > held) {
		$D = held - start;
	} else {
		$D = removed;
	}
	return $D;
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
function $B(self) {
	return self[0].v.length;
}
function $E(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $F = __list_get(inserted, offset);
		let $G = null;
		if ($F[0] === 0) {
			const value2 = $F[1];
			$G = __insert_at(list, start + offset, value2);
		} else {
			$G = undefined;
		}
		$G;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function $I(self) {
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
			const $J = __list_get(held, index);
			let $K = null;
			if ($J[0] === 0) {
				const op = $J[1];
				$K = kept.push(op);
			} else {
				$K = undefined;
			}
			$K;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function $H(self, op) {
	$I(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $N(self) {
	return self[1].v;
}
function $V(self) {
	return self.length === 0;
}
function $W(self) {
	let $Y = null;
	if ($V(self)) {
		$Y = [ 1 ];
	} else {
		$Y = __list_get(self, self.length - 1);
	}
	return $Y;
}
function $Q(self, $R) {
	const $S = $R;
	let $T = null;
	if ($S[0] === 0) {
		const turn2 = $S[1];
		$T = enqueue(turn2, __clone(self[1].v));
	} else {
		const $Z = $W(draining_turns.v);
		let $aa = null;
		if ($Z[0] === 0) {
			const draining = $Z[1];
			$aa = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aa = undefined;
		}
		$T = $aa;
	}
	return $T;
}
function $O(self, value2, $P) {
	self[0].v = __clone(value2);
	$Q(self, $P);
}
function $L(cell, $M) {
	$O(cell[1], $N(cell[2]), $M);
}
function $z(self, at, removed, inserted, $A) {
	const held = $B(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$H(self[2], $E(self[0].v, start, taking, inserted));
	$L(self, $A);
}
function $ab(self, at, value2, $ac) {
	const $ad = __list_get(self[0].v, at);
	let $ae = null;
	if ($ad[0] === 0) {
		const previous = $ad[1];
		__at_put(self[0].v, at, __clone(value2));
		$H(self[2], [ 1, at, previous, __clone(value2) ]);
		$L(self, $ac);
		$ae = undefined;
	} else {
		$ae = undefined;
	}
	return $ae;
}
function $af(self, value2, $ag) {
	self[0].v = __clone(value2);
	$H(self[2], [ 2, __clone(value2) ]);
	$L(self, $ag);
}
function $ah(self, from, count, to, $ai) {
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
		const $aj = __list_get(lifted, offset);
		let $ak = null;
		if ($aj[0] === 0) {
			const value2 = $aj[1];
			$ak = __insert_at(self[0].v, to + offset, value2);
		} else {
			$ak = undefined;
		}
		$ak;
		offset = offset + 1;
	}
	$H(self[2], [ 3, from, count, to ]);
	$L(self, $ai);
}
function $ar(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $ao(signal, observer) {
	const cell = signal[0];
	return $ar(signal, mint_subscriber(() => {
		const $ap = [ 0, cell ];
		let $aq = null;
		if ($ap[0] === 0) {
			const live = $ap[1];
			$aq = observer(live.v);
		} else {
			$aq = undefined;
		}
		return $aq;
	}));
}
function $as(self) {
	return __clone(self[0].v);
}
function $an(self, observer, immediately) {
	const subscription = $ao(self, observer);
	if (immediately) {
		observer($as(self));
	}
	return subscription;
}
function $am(self, observer, immediately) {
	const items = self[0];
	return $an(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function $al(self, observer) {
	return $am(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function $ax(self, item, $ay) {
	defer(self, () => {
		dispose(item, $ay);
		return;
	});
	return __clone(item);
}
function $aK(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $aJ(self, cursor) {
	$aK(self[2], cursor);
}
function $e(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $l($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($al(__clone(source2), (_published, $w) => {
		for (const op of read(cursor)) {
			const $x = op;
			let $y = null;
			if ($x[0] === 0) {
				const at = $x[1];
				const removed = $x[2];
				const inserted = $x[3];
				$z(out, at, removed.length, $k(inserted, g), $f);
				$y = undefined;
			} else if ($x[0] === 1) {
				const at2 = $x[1];
				const _was = $x[2];
				const value2 = $x[3];
				$y = $ab(out, at2, g(value2), $f);
			} else if ($x[0] === 2) {
				const items = $x[1];
				$y = $af(out, $k(items, g), $f);
			} else {
				const from = $x[1];
				const count = $x[2];
				const to = $x[3];
				$y = $ah(out, from, count, to, $f);
			}
			$y;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aJ(source2, cursor);
	}, $g);
	return out;
}
function $aV(self, op) {
	$I(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $aZ(cell, $M) {
	$O(cell[1], $N(cell[2]), $M);
}
function $aR(self, at, removed, inserted, $A) {
	const held = $B(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aV(self[2], $E(self[0].v, start, taking, inserted));
	$aZ(self, $A);
}
function $aO(self, value2, $aP) {
	return $aR(self, $B(self), 0, [ __clone(value2) ], $aP);
}
function $bg(elements) {
	return $n(elements, $b());
}
function $bl(self, at, removed, inserted, $A) {
	const held = $B(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aV(self[2], $E(self[0].v, start, taking, inserted));
	$aZ(self, $A);
}
function $bw(self, at, value2, $ac) {
	const $bx = __list_get(self[0].v, at);
	let $by = null;
	if ($bx[0] === 0) {
		const previous = $bx[1];
		__at_put(self[0].v, at, __clone(value2));
		$aV(self[2], [ 1, at, previous, __clone(value2) ]);
		$aZ(self, $ac);
		$by = undefined;
	} else {
		$by = undefined;
	}
	return $by;
}
function $bz(self, value2, $ag) {
	self[0].v = __clone(value2);
	$aV(self[2], [ 2, __clone(value2) ]);
	$aZ(self, $ag);
}
function $bA(self, from, count, to, $ai) {
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
		const $bB = __list_get(lifted, offset);
		let $bC = null;
		if ($bB[0] === 0) {
			const value2 = $bB[1];
			$bC = __insert_at(self[0].v, to + offset, value2);
		} else {
			$bC = undefined;
		}
		$bC;
		offset = offset + 1;
	}
	$aV(self[2], [ 3, from, count, to ]);
	$aZ(self, $ai);
}
function $be(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $bg($k($j(source2), g));
	as_derivation();
	const read = $o(source2);
	register_with_owner($al(__clone(source2), (_published, $w) => {
		for (const op of read(cursor)) {
			const $bj = op;
			let $bk = null;
			if ($bj[0] === 0) {
				const at = $bj[1];
				const removed = $bj[2];
				const inserted = $bj[3];
				$bl(out, at, removed.length, $k(inserted, g), $f);
				$bk = undefined;
			} else if ($bj[0] === 1) {
				const at2 = $bj[1];
				const _was = $bj[2];
				const value2 = $bj[3];
				$bk = $bw(out, at2, g(value2), $f);
			} else if ($bj[0] === 2) {
				const items = $bj[1];
				$bk = $bz(out, $k(items, g), $f);
			} else {
				const from = $bj[1];
				const count = $bj[2];
				const to = $bj[3];
				$bk = $bA(out, from, count, to, $f);
			}
			$bk;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aJ(source2, cursor);
	}, $g);
	return out;
}
function $bE(self, observer) {
	return $am(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function $bG(self, at, $bH) {
	return $aR(self, at, 1, [  ], $bH);
}
function $bI(self, values, $bJ) {
	return $aR(self, $B(self), 0, values, $bJ);
}
function $bK(self, at, value2, $bL) {
	return $aR(self, at, 0, [ __clone(value2) ], $bL);
}
function $bM(self, value2, $bN) {
	return $aR(self, 0, 0, [ __clone(value2) ], $bN);
}
function $bR(self, $bS) {
	const size = $B(self);
	let $bT = null;
	if (size > 0) {
		$bT = $aR(self, size - 1, 1, [  ], $bS);
	}
	return $bT;
}
function $bU(self, at, count, $bV) {
	return $aR(self, at, count, [  ], $bV);
}
function $bW(self, length, $bX) {
	const size = $B(self);
	let $bY = null;
	if (size > length) {
		$bY = $aR(self, length, size - length, [  ], $bX);
	}
	return $bY;
}
function $bZ(self, at, values, $ca) {
	return $aR(self, at, 0, values, $ca);
}
function $cb(self, values, $cc) {
	return $aR(self, 0, $B(self), values, $cc);
}
function $cd(self, $ce) {
	return $aR(self, 0, $B(self), [  ], $ce);
}
function $cf(self) {
	return $B(self) === 0;
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
	self[1].push($E(self[0], start, taking, inserted));
}
function $cg(self, value2, $ch) {
	return $cj(self, $ci(self), 0, [ __clone(value2) ], $ch);
}
function $ck(self, at, $cl) {
	return $cj(self, at, 1, [  ], $cl);
}
function $co(elements) {
	return [ elements, [  ] ];
}
function $cm(self, body, $cn) {
	let recorder = $co(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aV(self[2], op);
	}
	$aZ(self, $cn);
}
function $cp(target, $cq) {
	$cg(target, "from-fill-1", $cq);
	$cg(target, "from-fill-2", $cq);
}
function $cs(self, items, $ct) {
	const old_length = $B(self);
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
	$aR(self, prefix, removed, arriving, $ct);
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
	return $an(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function $db(self, observer) {
	return $dc(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function $dd(self, cursor) {
	$aK(self[2], cursor);
}
function $cO(source2, g, $f, $g) {
	const cursor = $cP(source2);
	const out = $bg($k($j(source2), g));
	as_derivation();
	const read = $cS(source2);
	register_with_owner($db(__clone(source2), (_published, $w) => {
		for (const op of read(cursor)) {
			const $cZ = op;
			let $da = null;
			if ($cZ[0] === 0) {
				const at = $cZ[1];
				const removed = $cZ[2];
				const inserted = $cZ[3];
				$bl(out, at, removed.length, $k(inserted, g), $f);
				$da = undefined;
			} else if ($cZ[0] === 1) {
				const at2 = $cZ[1];
				const _was = $cZ[2];
				const value2 = $cZ[3];
				$da = $bw(out, at2, g(value2), $f);
			} else if ($cZ[0] === 2) {
				const items = $cZ[1];
				$da = $bz(out, $k(items, g), $f);
			} else {
				const from = $cZ[1];
				const count = $cZ[2];
				const to = $cZ[3];
				$da = $bA(out, from, count, to, $f);
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
function $dg(self, observer) {
	return $dc(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function $di(self, value2, $aP) {
	return $bl(self, $B(self), 0, [ __clone(value2) ], $aP);
}
function $dj(self, at, value2, $bL) {
	return $bl(self, at, 0, [ __clone(value2) ], $bL);
}
function $dk(self, at, $bH) {
	return $bl(self, at, 1, [  ], $bH);
}
function $dl(self, $bS) {
	const size = $B(self);
	let $dm = null;
	if (size > 0) {
		$dm = $bl(self, size - 1, 1, [  ], $bS);
	}
	return $dm;
}
function $dn(self, at, count, $bV) {
	return $bl(self, at, count, [  ], $bV);
}
function $do(self, length, $bX) {
	const size = $B(self);
	let $dp = null;
	if (size > length) {
		$dp = $bl(self, length, size - length, [  ], $bX);
	}
	return $dp;
}
function $dq(self, items, $ct) {
	const old_length = $B(self);
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
		const $dr = __list_get(items, index);
		let $ds = null;
		if ($dr[0] === 0) {
			const value2 = $dr[1];
			$ds = arriving.push(value2);
		} else {
			$ds = undefined;
		}
		$ds;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$bl(self, prefix, removed, arriving, $ct);
}
function $dw(self, value2, $ch) {
	return $cj(self, $ci(self), 0, [ __clone(value2) ], $ch);
}
function $dy(self, body, $cn) {
	let recorder = $co(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aV(self[2], op);
	}
	$aZ(self, $cn);
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
$aO(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($j(my_nums), 0));
const source = $a();
const derived = $be(source, counted, [ 1 ], [ 1 ]);
$bE(__clone(source), (_list, $bD) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aO(source, "aa", [ 1 ]);
$aO(source, "bbb", [ 1 ]);
$aO(source, "c", [ 1 ]);
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
$bw(source, 0, "gggggg", [ 1 ]);
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
$bz(source, [ "n", "oo", "ppp" ], [ 1 ]);
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
$bA(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($j(source)));
const before_batch = notifications.v;
$cA(($cz) => {
	$aO(source, "r", [ 0, $cz ]);
	$aO(source, "ss", [ 0, $cz ]);
	$bG(source, 0, [ 0, $cz ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $cE([ "seed" ], 3);
const mirror = $be(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$cA(($cG) => {
	let round = 0;
	while (round < 8) {
		$aO(lagging, "row-" + round, [ 0, $cG ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $cH(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $cJ([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $be(clamped, counted, [ 1 ], [ 1 ]);
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
const walk = $bg([ 1, 2, 3 ]);
const first = $cO(walk, doubled, [ 1 ], [ 1 ]);
const second = $cO(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$dg(__clone(walk), (_list, $df) => {
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
	$cA(($dh) => {
		let made = 0;
		while (made < op_count) {
			const size = $B(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$di(walk, next_random(100), [ 0, $dh ]);
			} else if (choice < 11) {
				$dj(walk, pick(size + 1), next_random(100), [ 0, $dh ]);
			} else if (choice < 13) {
				$dk(walk, pick(size), [ 0, $dh ]);
			} else if (choice < 15) {
				$bw(walk, pick(size), next_random(100), [ 0, $dh ]);
			} else if (choice < 17) {
				const from = pick(size);
				const count = 1 + pick(size - from);
				$bA(walk, from, count, pick(size - count + 1), [ 0, $dh ]);
			} else if (choice < 18) {
				$dl(walk, [ 0, $dh ]);
			} else if (choice < 19 && size > 12) {
				$dn(walk, pick(size), 1 + pick(4), [ 0, $dh ]);
			} else if (choice < 20 && size > 20) {
				$do(walk, pick(size), [ 0, $dh ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$bz(walk, fresh, [ 0, $dh ]);
			} else if (choice < 22) {
				let edited = $j(walk);
				__at_put(edited, pick(size), next_random(100));
				edited.push(next_random(100));
				$dq(walk, edited, [ 0, $dh ]);
			} else {
				const at = pick(size);
				$dy(walk, (list) => {
					$cM(list, at, next_random(100), [ 0, $dh ]);
					$ck(list, 0, [ 0, $dh ]);
					$dw(list, next_random(100), [ 0, $dh ]);
					return;
				}, [ 0, $dh ]);
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
