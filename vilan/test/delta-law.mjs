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
	let $aa = null;
	if (wrapped < 0) {
		$aa = wrapped + modulus;
	} else {
		$aa = wrapped;
	}
	return $aa;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $p = null;
	if (truncated > 0) {
		$p = truncated;
	} else {
		$p = 0;
	}
	return $p;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $ab = null;
	if (wrapped >= half) {
		$ab = wrapped - modulus;
	} else {
		$ab = wrapped;
	}
	return $ab;
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
	return $L(self[0].v) && $L(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $K = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$K = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$K;
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
				while (!($L(turn[1].v)) && budget > 0) {
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
function next_random(bound) {
	const state = seed.v * 16807 % 2147483647;
	seed.v = state;
	return as_i32(state % as_i53(bound));
}
function pick(bound) {
	return as_usize(next_random(as_i322(bound)));
}
function g(value2) {
	calls.v = calls.v + 1;
	return value2 * 2 + 1;
}
function same(left, right) {
	if (left.length !== right.length) {
		return false;
	}
	let index = 0;
	let equal = true;
	while (index < left.length) {
		if ($aq(__list_get(left, index), __list_get(right, index))) {
			equal = false;
		}
		index = index + 1;
	}
	return equal;
}
function render(list) {
	let out = "";
	for (const value2 of list) {
		out = out + ("" + value2 + " ");
	}
	return out;
}
function drain_mirror(source2, cursor, held, label, at_turn) {
	let mirror = __clone(held);
	for (const op of $ax(source2, cursor)) {
		const $ay = op;
		let $az = null;
		if ($ay[0] === 2) {
			const items = $ay[1];
			resets_to_mirror.v = resets_to_mirror.v + 1;
			mirror = __clone(items);
			$az = undefined;
		} else if ($ay[0] === 1) {
			const at = $ay[1];
			const _previous = $ay[2];
			const value2 = $ay[3];
			ops_to_mirror.v = ops_to_mirror.v + 1;
			__at_put(mirror, at, value2);
			$az = undefined;
		} else if ($ay[0] === 0) {
			const at2 = $ay[1];
			const removed = $ay[2];
			const inserted = $ay[3];
			ops_to_mirror.v = ops_to_mirror.v + 1;
			let taken = 0;
			const leaving = removed.length;
			while (taken < leaving) {
				__remove_at(mirror, at2);
				taken = taken + 1;
			}
			let offset = 0;
			const count = inserted.length;
			while (offset < count) {
				const $aA = __list_get(inserted, offset);
				let $aB = null;
				if ($aA[0] === 0) {
					const value3 = $aA[1];
					$aB = __insert_at(mirror, at2 + offset, value3);
				} else {
					$aB = undefined;
				}
				$aB;
				offset = offset + 1;
			}
			$az = undefined;
		} else {
			const _from = $ay[1];
			const _count = $ay[2];
			const _to = $ay[3];
			(() => {
				throw "the generator emits no Move";
			})();
			$az = undefined;
		}
		$az;
	}
	checks.v = checks.v + 1;
	if (!(same(mirror, $i(source2)))) {
		(() => {
			throw "turn " + at_turn + ": " + label + "=[" + render(mirror) + "] source=[" + render($i(source2)) + "]";
		})();
	}
	return mirror;
}
function $c(value2) {
	let subscribers = [  ];
	return [ __shared_new(value2), __shared_new(subscribers) ];
}
function $b(value2) {
	return $c(value2);
}
function $d(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function $a(initial, limit) {
	return [ $b(initial), $d(limit) ];
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
function $l() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function $k(initial) {
	return [ $b(initial), $l() ];
}
function $o(self, cursor) {
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
function $n(log, elements, cursor) {
	const $s = $o(log, cursor);
	let $t = null;
	if ($s[0] === 0) {
		const ops = $s[1];
		$t = ops;
	} else {
		let lost = [  ];
		lost.push([ 2, __clone(elements.v) ]);
		$t = lost;
	}
	return $t;
}
function $m(self) {
	const log = __clone(self[1]);
	const elements = self[0][0];
	return (cursor) => {
		return $n(log, elements, cursor);
	};
}
function $B(self) {
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
			const $C = __list_get(held, index);
			let $D = null;
			if ($C[0] === 0) {
				const op = $C[1];
				$D = kept.push(op);
			} else {
				$D = undefined;
			}
			$D;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function $A(self, op) {
	$B(self);
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
	return self.length === 0;
}
function $M(self) {
	let $O = null;
	if ($L(self)) {
		$O = [ 1 ];
	} else {
		$O = __list_get(self, self.length - 1);
	}
	return $O;
}
function $G(self, $H) {
	const $I = $H;
	let $J = null;
	if ($I[0] === 0) {
		const turn = $I[1];
		$J = enqueue(turn, self[1].v);
	} else {
		const $P = $M(draining_turns.v);
		let $Q = null;
		if ($P[0] === 0) {
			const draining = $P[1];
			$Q = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$Q = undefined;
		}
		$J = $Q;
	}
	return $J;
}
function $E(self, mutate, $F) {
	mutate(self[0].v);
	$G(self, $F);
}
function $w(self, at, removed, inserted, $x) {
	$E(self[0], (list) => {
		let left = [  ];
		let taken = 0;
		while (taken < removed) {
			left.push(__remove_at(list, at));
			taken = taken + 1;
		}
		let offset = 0;
		const count = inserted.length;
		while (offset < count) {
			const $y = __list_get(inserted, offset);
			let $z = null;
			if ($y[0] === 0) {
				const value2 = $y[1];
				$z = __insert_at(list, at + offset, value2);
			} else {
				$z = undefined;
			}
			$z;
			offset = offset + 1;
		}
		$A(self[1], [ 0, at, left, __clone(inserted) ]);
		return;
	}, $x);
}
function $R(self, at, value2, $S) {
	$E(self[0], (list) => {
		const previous = __clone(__at(list, at));
		__at_put(list, at, __clone(value2));
		$A(self[1], [ 1, at, previous, __clone(value2) ]);
		return;
	}, $S);
}
function $T(self, value2, $U) {
	$E(self[0], (list) => {
		__replace(list, __clone(value2));
		$A(self[1], [ 2, __clone(value2) ]);
		return;
	}, $U);
}
function $Z(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $Y(self, subscriber) {
	return $Z(self, subscriber);
}
function $X(self, subscriber) {
	return $Y(self[0], subscriber);
}
function $W(self, observer, immediately) {
	const subscription = $X(self, mint_subscriber(() => {
		return observer($i(self));
	}));
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $V(self, observer) {
	return $W(self, observer, false);
}
function $e(source2, $f) {
	const cursor = $g(source2);
	let seeded = [  ];
	for (const value2 of $i(source2)) {
		seeded.push(g(value2));
	}
	const out = $k(seeded);
	const read = $m(source2);
	$V(__clone(source2), (_list) => {
		notifications.v = notifications.v + 1;
		for (const op of read(cursor)) {
			const $u = op;
			let $v = null;
			if ($u[0] === 0) {
				const at = $u[1];
				const removed = $u[2];
				const inserted = $u[3];
				let mapped = [  ];
				for (const value3 of inserted) {
					incremental_calls.v = incremental_calls.v + 1;
					mapped.push(g(value3));
				}
				$w(out, at, removed.length, mapped, $f);
				$v = undefined;
			} else if ($u[0] === 1) {
				const at2 = $u[1];
				const _previous = $u[2];
				const value4 = $u[3];
				incremental_calls.v = incremental_calls.v + 1;
				$R(out, at2, g(value4), $f);
				$v = undefined;
			} else if ($u[0] === 2) {
				const items = $u[1];
				let mapped2 = [  ];
				for (const value5 of items) {
					reset_calls.v = reset_calls.v + 1;
					mapped2.push(g(value5));
				}
				$T(out, mapped2, $f);
				$v = undefined;
			} else {
				const _from = $u[1];
				const _count = $u[2];
				const _to = $u[3];
				(() => {
					throw "the generator emits no Move";
				})();
				$v = undefined;
			}
			$v;
		}
		return;
	});
	return out;
}
function $ad(self) {
	return $j(self[0]).length;
}
function $ae(self, value2, $af) {
	return $w(self, $ad(self), 0, [ __clone(value2) ], $af);
}
function $ag(self, at, value2, $ah) {
	return $w(self, at, 0, [ __clone(value2) ], $ah);
}
function $ai(self, at, $aj) {
	return $w(self, at, 1, [  ], $aj);
}
function $ak(self, $al) {
	return $w(self, 0, $ad(self), [  ], $al);
}
function $am(body, $an) {
	const $ao = $an;
	let $ap = null;
	if ($ao[0] === 0) {
		const current = $ao[1];
		$ap = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$ap = result;
	}
	return $ap;
}
function $ar(self, b) {
	const $as = self;
	let $av = null;
	if ($as[0] === 0) {
		const $at = b;
		let $au = null;
		if ($at[0] === 0) {
			$au = $as[1] === $at[1];
		} else {
			$au = false;
		}
		$av = $au;
	} else {
		const $aw = b;
		$av = $aw[0] === 1;
	}
	return $av;
}
function $aq(self, b) {
	return !($ar(self, b));
}
function $ax(self, cursor) {
	return $n(self[1], self[0][0], cursor);
}
function $aC(self) {
	return self[0].v.length;
}
function $aD(self) {
	return self[2].v;
}
function $aE(self) {
	return self[1].v;
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
	$aG(self[1], cursor);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const delta_log_limit = 1024;
const seed = __shared_new(1);
const calls = __shared_new(0);
const naive_calls = __shared_new(0);
const notifications = __shared_new(0);
const resets_to_mirror = __shared_new(0);
const ops_to_mirror = __shared_new(0);
const incremental_calls = __shared_new(0);
const reset_calls = __shared_new(0);
const checks = __shared_new(0);
const source = $a([ 1, 2, 3 ], 30);
const derived = $e(source, [ 1 ]);
const near = $g(source);
const far = $g(source);
let mirror_near = [ 1, 2, 3 ];
let mirror_far = [ 1, 2, 3 ];
let turn_index = 1;
while (turn_index <= 400) {
	const before = notifications.v;
	const op_count = 1 + next_random(4);
	$am(($ac) => {
		let made = 0;
		while (made < op_count) {
			const size = $ad(source);
			const choice = next_random(20);
			if (choice < 9 || size === 0) {
				$ae(source, next_random(100), [ 0, $ac ]);
			} else if (choice < 13) {
				$ag(source, pick(size), next_random(100), [ 0, $ac ]);
			} else if (choice < 16) {
				$ai(source, pick(size), [ 0, $ac ]);
			} else if (choice < 18) {
				$R(source, pick(size), next_random(100), [ 0, $ac ]);
			} else if (choice < 19 && size > 15) {
				let fresh = [  ];
				let fill = 0;
				const length = 1 + next_random(8);
				while (fill < length) {
					fresh.push(next_random(100));
					fill = fill + 1;
				}
				$T(source, fresh, [ 0, $ac ]);
			} else if (size > 25) {
				$ak(source, [ 0, $ac ]);
			} else {
				$ae(source, next_random(100), [ 0, $ac ]);
			}
			made = made + 1;
		}
		return;
	}, [ 1 ]);
	const waves = notifications.v - before;
	if (waves !== 1) {
		(() => {
			throw "turn " + turn_index + ": notifications=" + waves + " (expected 1)";
		})();
	}
	let reference = [  ];
	for (const value of $i(source)) {
		naive_calls.v = naive_calls.v + 1;
		reference.push(value * 2 + 1);
	}
	checks.v = checks.v + 1;
	if (!(same($i(derived), reference))) {
		(() => {
			throw "turn " + turn_index + ": derived=[" + render($i(derived)) + "] expected=[" + render(reference) + "]";
		})();
	}
	if (turn_index % 3 === 0) {
		mirror_near = drain_mirror(source, near, mirror_near, "mirror_near", turn_index);
	}
	if (turn_index % 20 === 0) {
		mirror_far = drain_mirror(source, far, mirror_far, "mirror_far", turn_index);
	}
	turn_index = turn_index + 1;
}
drain_mirror(source, near, mirror_near, "mirror_near", 0);
drain_mirror(source, far, mirror_far, "mirror_far", 0);
if (ops_to_mirror.v === 0) {
	(() => {
		throw "no lagging cursor was ever answered with ops";
	})();
}
if (resets_to_mirror.v === 0) {
	(() => {
		throw "no lagging cursor ever fell past the log\'s base";
	})();
}
if (calls.v >= naive_calls.v) {
	(() => {
		throw "the derivation made " + calls.v + " calls against the rerun\'s " + naive_calls.v;
	})();
}
console.log("turns=400 checks=" + checks.v + " failures=0");
console.log("lagging cursors: ops drained=" + ops_to_mirror.v + " resets=" + resets_to_mirror.v);
console.log("g calls: incremental=" + calls.v + " (splice/set_at=" + incremental_calls.v + ", reset=" + reset_calls.v + ") naive-rerun=" + naive_calls.v);
console.log("final length " + $i(source).length + ", log held " + $aC(source[1]) + " ops, base " + $aD(source[1]) + ", version " + $aE(source[1]));
$aF(source, near);
$aF(source, far);
