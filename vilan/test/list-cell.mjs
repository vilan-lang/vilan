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
	let $cr = null;
	if (wrapped < 0) {
		$cr = wrapped + modulus;
	} else {
		$cr = wrapped;
	}
	return $cr;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $cs = null;
	if (wrapped >= half) {
		$cs = wrapped - modulus;
	} else {
		$cs = wrapped;
	}
	return $cs;
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
	return $S(self[0].v) && $S(self[1].v);
}
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $R = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$R = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber));
		}
		$R;
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
				while (!($S(turn2[1].v)) && budget > 0) {
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
function dispose(self, $as) {
	const $at = $as;
	let $au = null;
	if ($at[0] === 0) {
		const established = $at[1];
		$au = [ 0, established ];
	} else {
		$au = $T(draining_turns.v);
	}
	const ambient = $au;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $av = [ 0, handle[0] ];
	let $aw = null;
	if ($av[0] === 0) {
		const subscribers = $av[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	$aw;
	const $ax = ambient;
	let $ay = null;
	if ($ax[0] === 0) {
		const turn2 = $ax[1];
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
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	$ay;
	const $az = handle[3].v;
	let $aA = null;
	if ($az[0] === 0) {
		const release = $az[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aA = undefined;
	} else {
		$aA = undefined;
	}
	return $aA;
}
function defer(self, cleanup) {
	if (self[1].v) {
		cleanup();
	} else {
		self[0].v.push(cleanup);
	}
}
function register_with_owner(subscription, $am, $an) {
	const $ao = $an;
	let $ap = null;
	if ($ao[0] === 0) {
		const owner = $ao[1];
		$ap = $aq(owner, subscription, $am);
	} else {
		$ap = __clone(subscription);
	}
	return $ap;
}
function defer_to_owner(cleanup, $aD) {
	const $aE = $aD;
	let $aF = null;
	if ($aE[0] === 0) {
		const owner = $aE[1];
		$aF = defer(owner, cleanup);
	} else {
		$aF = undefined;
	}
	return $aF;
}
function splice_start(held, at) {
	let $z = null;
	if (at < 0) {
		$z = 0;
	} else if (at > held) {
		$z = held;
	} else {
		$z = at;
	}
	return $z;
}
function splice_count(held, start, removed) {
	let $A = null;
	if (removed < 0) {
		$A = 0;
	} else if (start + removed > held) {
		$A = held - start;
	} else {
		$A = removed;
	}
	return $A;
}
function counted(text) {
	calls.v = calls.v + 1;
	return text.length;
}
function law(label, source2, derived2) {
	let naive2 = [  ];
	for (const value2 of $j(source2)) {
		naive2.push(value2.length);
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
	const $s = $p(self[2], cursor);
	let $t = null;
	if ($s[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(self[0].v) ]);
		$t = lost;
	} else {
		const recorded = $s[1];
		$t = recorded;
	}
	return $t;
}
function $y(self) {
	return self[0].v.length;
}
function $B(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $C = __list_get(inserted, offset);
		let $D = null;
		if ($C[0] === 0) {
			const value2 = $C[1];
			$D = __insert_at(list, start + offset, value2);
		} else {
			$D = undefined;
		}
		$D;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function $F(self) {
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
			const $G = __list_get(held, index);
			let $H = null;
			if ($G[0] === 0) {
				const op = $G[1];
				$H = kept.push(op);
			} else {
				$H = undefined;
			}
			$H;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function $E(self, op) {
	$F(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $K(self) {
	return self[1].v;
}
function $S(self) {
	return self.length === 0;
}
function $T(self) {
	return __list_get(self, self.length - 1);
}
function $N(self, $O) {
	const $P = $O;
	let $Q = null;
	if ($P[0] === 0) {
		const turn2 = $P[1];
		$Q = enqueue(turn2, self[1].v);
	} else {
		const $U = $T(draining_turns.v);
		let $V = null;
		if ($U[0] === 0) {
			const draining = $U[1];
			$V = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$V = undefined;
		}
		$Q = $V;
	}
	return $Q;
}
function $L(self, value2, $M) {
	self[0].v = __clone(value2);
	$N(self, $M);
}
function $I(cell, $J) {
	$L(cell[1], $K(cell[2]), $J);
}
function $w(self, at, removed, inserted, $x) {
	const held = $y(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$E(self[2], $B(self[0].v, start, taking, inserted));
	$I(self, $x);
}
function $W(self, at, value2, $X) {
	const $Y = __list_get(self[0].v, at);
	let $Z = null;
	if ($Y[0] === 0) {
		const previous = $Y[1];
		__at_put(self[0].v, at, __clone(value2));
		$E(self[2], [ 1, at, previous, __clone(value2) ]);
		$I(self, $X);
		$Z = undefined;
	} else {
		$Z = undefined;
	}
	return $Z;
}
function $aa(self, value2, $ab) {
	self[0].v = __clone(value2);
	$E(self[2], [ 2, __clone(value2) ]);
	$I(self, $ab);
}
function $ac(self, from, count, to, $ad) {
	if (count <= 0 || from === to) {
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
		const $ae = __list_get(lifted, offset);
		let $af = null;
		if ($ae[0] === 0) {
			const value2 = $ae[1];
			$af = __insert_at(self[0].v, to + offset, value2);
		} else {
			$af = undefined;
		}
		$af;
		offset = offset + 1;
	}
	$E(self[2], [ 3, from, count, to ]);
	$I(self, $ad);
}
function $al(signal, subscriber) {
	signal[1].v.push(reissued(subscriber));
	return [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
}
function $ai(signal, observer) {
	const cell = signal[0];
	return $al(signal, mint_subscriber(() => {
		const $aj = [ 0, cell ];
		let $ak = null;
		if ($aj[0] === 0) {
			const live = $aj[1];
			$ak = observer(live.v);
		} else {
			$ak = undefined;
		}
		return $ak;
	}));
}
function $ah(self, observer) {
	return $ai(self, observer);
}
function $ag(self, observer) {
	return $ah(self[1], (_sequence) => {
		return observer(self[0].v);
	});
}
function $aq(self, item, $ar) {
	if (self[1].v) {
		dispose(item, $ar);
	} else {
		self[0].v.push(() => {
			dispose(item, $ar);
			return;
		});
	}
	return __clone(item);
}
function $aC(self, cursor) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function $aB(self, cursor) {
	$aC(self[2], cursor);
}
function $e(source2, g, $f, $g) {
	const cursor = $h(source2);
	const out = $l($k($j(source2), g));
	as_derivation();
	register_with_owner($ag(source2, (_published) => {
		for (const op of $o(source2, cursor)) {
			const $u = op;
			let $v = null;
			if ($u[0] === 0) {
				const at = $u[1];
				const removed = $u[2];
				const inserted = $u[3];
				$w(out, at, removed.length, $k(inserted, g), $f);
				$v = undefined;
			} else if ($u[0] === 1) {
				const at2 = $u[1];
				const _was = $u[2];
				const value2 = $u[3];
				$v = $W(out, at2, g(value2), $f);
			} else if ($u[0] === 2) {
				const items = $u[1];
				$v = $aa(out, $k(items, g), $f);
			} else {
				const from = $u[1];
				const count = $u[2];
				const to = $u[3];
				$v = $ac(out, from, count, to, $f);
			}
			$v;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $aB(source2, cursor);
	}, $g);
	return out;
}
function $aN(self, op) {
	$F(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $aR(cell, $J) {
	$L(cell[1], $K(cell[2]), $J);
}
function $aJ(self, at, removed, inserted, $x) {
	const held = $y(self);
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	$aN(self[2], $B(self[0].v, start, taking, inserted));
	$aR(self, $x);
}
function $aG(self, value2, $aH) {
	return $aJ(self, $y(self), 0, [ __clone(value2) ], $aH);
}
function $aU(self, at, $aV) {
	return $aJ(self, at, 1, [  ], $aV);
}
function $aW(self, values, $aX) {
	return $aJ(self, $y(self), 0, values, $aX);
}
function $aY(self, at, value2, $aZ) {
	return $aJ(self, at, 0, [ __clone(value2) ], $aZ);
}
function $ba(self, value2, $bb) {
	return $aJ(self, 0, 0, [ __clone(value2) ], $bb);
}
function $bc(self, at, value2, $X) {
	const $bd = __list_get(self[0].v, at);
	let $be = null;
	if ($bd[0] === 0) {
		const previous = $bd[1];
		__at_put(self[0].v, at, __clone(value2));
		$aN(self[2], [ 1, at, previous, __clone(value2) ]);
		$aR(self, $X);
		$be = undefined;
	} else {
		$be = undefined;
	}
	return $be;
}
function $bf(self, $bg) {
	const size = $y(self);
	let $bh = null;
	if (size > 0) {
		$bh = $aJ(self, size - 1, 1, [  ], $bg);
	}
	return $bh;
}
function $bi(self, at, count, $bj) {
	return $aJ(self, at, count, [  ], $bj);
}
function $bk(self, length, $bl) {
	const size = $y(self);
	let $bm = null;
	if (size > length) {
		$bm = $aJ(self, length, size - length, [  ], $bl);
	}
	return $bm;
}
function $bn(self, at, values, $bo) {
	return $aJ(self, at, 0, values, $bo);
}
function $bp(self, values, $bq) {
	return $aJ(self, 0, $y(self), values, $bq);
}
function $br(self, $bs) {
	return $aJ(self, 0, $y(self), [  ], $bs);
}
function $bt(self) {
	return $y(self) === 0;
}
function $bw(self) {
	return self[0].length;
}
function $bx(self, at, removed, inserted) {
	const held = self[0].length;
	const start = splice_start(held, at);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push($B(self[0], start, taking, inserted));
}
function $bu(self, value2, $bv) {
	return $bx(self, $bw(self), 0, [ __clone(value2) ], $bv);
}
function $by(self, at, $bz) {
	return $bx(self, at, 1, [  ], $bz);
}
function $bC(elements) {
	return [ __clone(elements), [  ] ];
}
function $bA(self, body, $bB) {
	let recorder = $bC(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$aN(self[2], op);
	}
	$aR(self, $bB);
}
function $bD(target, $bE) {
	$bu(target, "from-fill-1", $bE);
	$bu(target, "from-fill-2", $bE);
}
function $bF(self, value2, $ab) {
	self[0].v = __clone(value2);
	$aN(self[2], [ 2, __clone(value2) ]);
	$aR(self, $ab);
}
function $bG(self, items, $bH) {
	const old_length = $y(self);
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
		const $bI = __list_get(items, index);
		let $bJ = null;
		if ($bI[0] === 0) {
			const value2 = $bI[1];
			$bJ = arriving.push(value2);
		} else {
			$bJ = undefined;
		}
		$bJ;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$aJ(self, prefix, removed, arriving, $bH);
}
function $bK(self, from, count, to, $ad) {
	if (count <= 0 || from === to) {
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
		const $bL = __list_get(lifted, offset);
		let $bM = null;
		if ($bL[0] === 0) {
			const value2 = $bL[1];
			$bM = __insert_at(self[0].v, to + offset, value2);
		} else {
			$bM = undefined;
		}
		$bM;
		offset = offset + 1;
	}
	$aN(self[2], [ 3, from, count, to ]);
	$aR(self, $ad);
}
function $bO(body, $bP) {
	const $bQ = $bP;
	let $bR = null;
	if ($bQ[0] === 0) {
		const current = $bQ[1];
		$bR = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$bR = result;
	}
	return $bR;
}
function $bT(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $bS(elements, limit) {
	return $c(elements, $bT(limit));
}
function $bW(self) {
	return self[0].v.length;
}
function $bV(self) {
	return $bW(self[2]);
}
function $bX(elements) {
	return $c(elements, $b());
}
function $bY(self, at, count, $bZ) {
	return $bx(self, at, count, [  ], $bZ);
}
function $ca(self, at, value2, $cb) {
	return $bx(self, at, 0, [ __clone(value2) ], $cb);
}
function $cd(self) {
	return $i(self[2]);
}
function $cg(self, cursor) {
	const $ck = $p(self[2], cursor);
	let $cl = null;
	if ($ck[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(self[0].v) ]);
		$cl = lost;
	} else {
		const recorded = $ck[1];
		$cl = recorded;
	}
	return $cl;
}
function $co(self, observer) {
	return $ah(self[1], (_sequence) => {
		return observer(self[0].v);
	});
}
function $cp(self, cursor) {
	$aC(self[2], cursor);
}
function $cc(source2, g, $f, $g) {
	const cursor = $cd(source2);
	const out = $l($k($j(source2), g));
	as_derivation();
	register_with_owner($co(source2, (_published) => {
		for (const op of $cg(source2, cursor)) {
			const $cm = op;
			let $cn = null;
			if ($cm[0] === 0) {
				const at = $cm[1];
				const removed = $cm[2];
				const inserted = $cm[3];
				$w(out, at, removed.length, $k(inserted, g), $f);
				$cn = undefined;
			} else if ($cm[0] === 1) {
				const at2 = $cm[1];
				const _was = $cm[2];
				const value2 = $cm[3];
				$cn = $W(out, at2, g(value2), $f);
			} else if ($cm[0] === 2) {
				const items = $cm[1];
				$cn = $aa(out, $k(items, g), $f);
			} else {
				const from = $cm[1];
				const count = $cm[2];
				const to = $cm[3];
				$cn = $ac(out, from, count, to, $f);
			}
			$cn;
		}
		return;
	}), $f, $g);
	defer_to_owner(() => {
		return $cp(source2, cursor);
	}, $g);
	return out;
}
function $cu(self, value2, $aH) {
	return $w(self, $y(self), 0, [ __clone(value2) ], $aH);
}
function $cv(self, at, value2, $aZ) {
	return $w(self, at, 0, [ __clone(value2) ], $aZ);
}
function $cw(self, at, $aV) {
	return $w(self, at, 1, [  ], $aV);
}
function $cx(self, $bg) {
	const size = $y(self);
	let $cy = null;
	if (size > 0) {
		$cy = $w(self, size - 1, 1, [  ], $bg);
	}
	return $cy;
}
function $cz(self, at, count, $bj) {
	return $w(self, at, count, [  ], $bj);
}
function $cA(self, length, $bl) {
	const size = $y(self);
	let $cB = null;
	if (size > length) {
		$cB = $w(self, length, size - length, [  ], $bl);
	}
	return $cB;
}
function $cC(self, items, $bH) {
	const old_length = $y(self);
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
		const $cD = __list_get(items, index);
		let $cE = null;
		if ($cD[0] === 0) {
			const value2 = $cD[1];
			$cE = arriving.push(value2);
		} else {
			$cE = undefined;
		}
		$cE;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	$w(self, prefix, removed, arriving, $bH);
}
function $cI(self, value2, $bv) {
	return $bx(self, $bw(self), 0, [ __clone(value2) ], $bv);
}
function $cK(self, body, $bB) {
	let recorder = $bC(self[0].v);
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		$E(self[2], op);
	}
	$I(self, $bB);
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
$aG(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at($j(my_nums), 0));
const source = $a();
const derived = $e(source, counted, [ 1 ], [ 1 ]);
$ag(source, (_list) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
$aG(source, "aa", [ 1 ]);
$aG(source, "bbb", [ 1 ]);
$aG(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
$aU(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
$aW(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
$aY(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
$ba(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
$bc(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
$bf(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
$bi(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
$bk(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
$bn(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
$bp(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
$br(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!($bt(source))) {
	(() => {
		throw "clear left something behind";
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
$bA(source, (list) => {
	$bu(list, "kk", [ 1 ]);
	$bu(list, "lll", [ 1 ]);
	$by(list, 0, [ 1 ]);
	$bu(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render($j(source)));
$bA(source, (list) => {
	$bD(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render($j(source)));
const before_set = calls.v;
$bF(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
$bG(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
$bG(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
$bK(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render($j(source)));
const before_batch = notifications.v;
$bO(($bN) => {
	$aG(source, "r", [ 0, $bN ]);
	$aG(source, "ss", [ 0, $bN ]);
	$aU(source, 0, [ 0, $bN ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = $bS([ "seed" ], 3);
const mirror = $e(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
$bO(($bU) => {
	let round = 0;
	while (round < 8) {
		$aG(lagging, "row-" + round, [ 0, $bU ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + $bV(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = $bX([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = $e(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
$bA(clamped, (list) => {
	$bY(list, 1, 99, [ 1 ]);
	$by(list, 50, [ 1 ]);
	$ca(list, -(3), "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
$bi(clamped, 1, 99, [ 1 ]);
$aU(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render($j(clamped)));
const walk = $l([ 1, 2, 3 ]);
const first = $cc(walk, doubled, [ 1 ], [ 1 ]);
const second = $cc(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
$co(walk, (_list) => {
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
	$bO(($ct) => {
		let made = 0;
		while (made < op_count) {
			const size = $y(walk);
			const choice = next_random(24);
			if (choice < 8 || size === 0) {
				$cu(walk, next_random(100), [ 0, $ct ]);
			} else if (choice < 11) {
				$cv(walk, next_random(size + 1), next_random(100), [ 0, $ct ]);
			} else if (choice < 13) {
				$cw(walk, next_random(size), [ 0, $ct ]);
			} else if (choice < 15) {
				$W(walk, next_random(size), next_random(100), [ 0, $ct ]);
			} else if (choice < 17) {
				const from = next_random(size);
				const count = 1 + next_random(size - from);
				$ac(walk, from, count, next_random(size - count + 1), [ 0, $ct ]);
			} else if (choice < 18) {
				$cx(walk, [ 0, $ct ]);
			} else if (choice < 19 && size > 12) {
				$cz(walk, next_random(size), 1 + next_random(4), [ 0, $ct ]);
			} else if (choice < 20 && size > 20) {
				$cA(walk, next_random(size), [ 0, $ct ]);
			} else if (choice < 21 && size > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				$aa(walk, fresh, [ 0, $ct ]);
			} else if (choice < 22) {
				let edited = $j(walk);
				__at_put(edited, next_random(size), next_random(100));
				edited.push(next_random(100));
				$cC(walk, edited, [ 0, $ct ]);
			} else {
				const at = next_random(size);
				$cK(walk, (list) => {
					$ca(list, at, next_random(100), [ 0, $ct ]);
					$by(list, 0, [ 0, $ct ]);
					$cI(list, next_random(100), [ 0, $ct ]);
					return;
				}, [ 0, $ct ]);
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
