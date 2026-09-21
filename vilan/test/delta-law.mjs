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
	let $V = null;
	if (wrapped < 0) {
		$V = wrapped + modulus;
	} else {
		$V = wrapped;
	}
	return $V;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $W = null;
	if (wrapped >= half) {
		$W = wrapped - modulus;
	} else {
		$W = wrapped;
	}
	return $W;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		if (!(turn[1].v.has(key))) {
			turn[1].v.set(key, true);
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[3].v && !(turn[4].v) && !(turn[2].v)) {
		turn[4].v = true;
		queueMicrotask(() => {
			turn[4].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[2].v)) {
		turn[2].v = true;
		draining_turns.v.push(__clone(turn));
		__with_finally(() => {
			let budget = 100000;
			while (!($I(turn[0].v)) && budget > 0) {
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[1].v = new Map();
				for (const subscriber of wave) {
					if (subscriber[2].v) {
						subscriber[1]();
					}
					budget = budget - 1;
				}
			}
			return;
		}, () => {
			__list_pop(draining_turns.v);
			turn[2].v = false;
			return;
		});
	}
}
function next_random(bound) {
	const state = seed.v * 16807 % 2147483647;
	seed.v = state;
	return as_i32(state % as_i53(bound));
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
		if ($al(__list_get(left, index), __list_get(right, index))) {
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
	for (const op of $m(source2, cursor)) {
		const $as = op;
		let $at = null;
		if ($as[0] === 2) {
			const items = $as[1];
			resets_to_mirror.v = resets_to_mirror.v + 1;
			mirror = __clone(items);
			$at = undefined;
		} else if ($as[0] === 1) {
			const at = $as[1];
			const _previous = $as[2];
			const value2 = $as[3];
			ops_to_mirror.v = ops_to_mirror.v + 1;
			__at_put(mirror, at, value2);
			$at = undefined;
		} else if ($as[0] === 0) {
			const at2 = $as[1];
			const removed = $as[2];
			const inserted = $as[3];
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
				const $au = __list_get(inserted, offset);
				let $av = null;
				if ($au[0] === 0) {
					const value3 = $au[1];
					$av = __insert_at(mirror, at2 + offset, value3);
				} else {
					$av = undefined;
				}
				$av;
				offset = offset + 1;
			}
			$at = undefined;
		} else {
			const _from = $as[1];
			const _count = $as[2];
			const _to = $as[3];
			(() => {
				throw "the generator emits no Move";
			})();
			$at = undefined;
		}
		$at;
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
function $n(self, cursor) {
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
		const $o = __list_get(held, index);
		let $p = null;
		if ($o[0] === 0) {
			const op = $o[1];
			$p = ops.push(op);
		} else {
			$p = undefined;
		}
		$p;
		index = index + 1;
	}
	return [ 0, ops ];
}
function $m(self, cursor) {
	const $q = $n(self[1], cursor);
	let $r = null;
	if ($q[0] === 0) {
		const ops = $q[1];
		$r = ops;
	} else {
		let lost = [  ];
		lost.push([ 2, $j(self[0]) ]);
		$r = lost;
	}
	return $r;
}
function $z(self) {
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
			const $A = __list_get(held, index);
			let $B = null;
			if ($A[0] === 0) {
				const op = $A[1];
				$B = kept.push(op);
			} else {
				$B = undefined;
			}
			$B;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function $y(self, op) {
	$z(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function $I(self) {
	return self.length === 0;
}
function $J(self) {
	return __list_get(self, self.length - 1);
}
function $E(self, $F) {
	const $G = $F;
	let $H = null;
	if ($G[0] === 0) {
		const turn = $G[1];
		$H = enqueue(turn, self[1].v);
	} else {
		const $K = $J(draining_turns.v);
		let $L = null;
		if ($K[0] === 0) {
			const draining = $K[1];
			$L = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$L = undefined;
		}
		$H = $L;
	}
	return $H;
}
function $C(self, mutate, $D) {
	mutate(self[0].v);
	$E(self, $D);
}
function $u(self, at, removed, inserted, $v) {
	$C(self[0], (list) => {
		let left = [  ];
		let taken = 0;
		while (taken < removed) {
			left.push(__remove_at(list, at));
			taken = taken + 1;
		}
		let offset = 0;
		const count = inserted.length;
		while (offset < count) {
			const $w = __list_get(inserted, offset);
			let $x = null;
			if ($w[0] === 0) {
				const value2 = $w[1];
				$x = __insert_at(list, at + offset, value2);
			} else {
				$x = undefined;
			}
			$x;
			offset = offset + 1;
		}
		$y(self[1], [ 0, at, left, __clone(inserted) ]);
		return;
	}, $v);
}
function $M(self, at, value2, $N) {
	$C(self[0], (list) => {
		const previous = __clone(__at(list, at));
		__at_put(list, at, __clone(value2));
		$y(self[1], [ 1, at, previous, __clone(value2) ]);
		return;
	}, $N);
}
function $O(self, value2, $P) {
	$C(self[0], (list) => {
		__replace(list, __clone(value2));
		$y(self[1], [ 2, __clone(value2) ]);
		return;
	}, $P);
}
function $S(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	const live = __shared_new(true);
	signal[1].v.push([ id, () => {
		const $T = [ 0, cell ];
		let $U = null;
		if ($T[0] === 0) {
			const live2 = $T[1];
			$U = observer(live2.v);
		} else {
			$U = undefined;
		}
		return $U;
	}, live ]);
	return [ signal[1], id, live, __shared_new([ 1 ]) ];
}
function $R(self, observer) {
	return $S(self, observer);
}
function $Q(self, observer) {
	return $R(self[0], observer);
}
function $e(source2, $f) {
	const cursor = $g(source2);
	let seeded = [  ];
	for (const value2 of $i(source2)) {
		seeded.push(g(value2));
	}
	const out = $k(seeded);
	$Q(source2, (_list) => {
		notifications.v = notifications.v + 1;
		for (const op of $m(source2, cursor)) {
			const $s = op;
			let $t = null;
			if ($s[0] === 0) {
				const at = $s[1];
				const removed = $s[2];
				const inserted = $s[3];
				let mapped = [  ];
				for (const value3 of inserted) {
					incremental_calls.v = incremental_calls.v + 1;
					mapped.push(g(value3));
				}
				$u(out, at, removed.length, mapped, $f);
				$t = undefined;
			} else if ($s[0] === 1) {
				const at2 = $s[1];
				const _previous = $s[2];
				const value4 = $s[3];
				incremental_calls.v = incremental_calls.v + 1;
				$M(out, at2, g(value4), $f);
				$t = undefined;
			} else if ($s[0] === 2) {
				const items = $s[1];
				let mapped2 = [  ];
				for (const value5 of items) {
					reset_calls.v = reset_calls.v + 1;
					mapped2.push(g(value5));
				}
				$O(out, mapped2, $f);
				$t = undefined;
			} else {
				const _from = $s[1];
				const _count = $s[2];
				const _to = $s[3];
				(() => {
					throw "the generator emits no Move";
				})();
				$t = undefined;
			}
			$t;
		}
		return;
	});
	return out;
}
function $Y(self) {
	return $j(self[0]).length;
}
function $Z(self, value2, $aa) {
	return $u(self, $Y(self), 0, [ __clone(value2) ], $aa);
}
function $ab(self, at, value2, $ac) {
	return $u(self, at, 0, [ __clone(value2) ], $ac);
}
function $ad(self, at, $ae) {
	return $u(self, at, 1, [  ], $ae);
}
function $af(self, $ag) {
	return $u(self, 0, $Y(self), [  ], $ag);
}
function $ah(body, $ai) {
	const $aj = $ai;
	let $ak = null;
	if ($aj[0] === 0) {
		const current = $aj[1];
		$ak = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[3].v = true;
		$ak = result;
	}
	return $ak;
}
function $am(self, b) {
	const $an = self;
	let $aq = null;
	if ($an[0] === 0) {
		const $ao = b;
		let $ap = null;
		if ($ao[0] === 0) {
			$ap = $an[1] === $ao[1];
		} else {
			$ap = false;
		}
		$aq = $ap;
	} else {
		const $ar = b;
		$aq = $ar[0] === 1;
	}
	return $aq;
}
function $al(self, b) {
	return !($am(self, b));
}
function $aw(self) {
	return self[0].v.length;
}
function $ax(self) {
	return self[2].v;
}
function $ay(self) {
	return self[1].v;
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
	$ah(($X) => {
		let made = 0;
		while (made < op_count) {
			const size = $Y(source);
			const choice = next_random(20);
			if (choice < 9 || size === 0) {
				$Z(source, next_random(100), [ 0, $X ]);
			} else if (choice < 13) {
				$ab(source, next_random(size), next_random(100), [ 0, $X ]);
			} else if (choice < 16) {
				$ad(source, next_random(size), [ 0, $X ]);
			} else if (choice < 18) {
				$M(source, next_random(size), next_random(100), [ 0, $X ]);
			} else if (choice < 19 && size > 15) {
				let fresh = [  ];
				let fill = 0;
				const length = 1 + next_random(8);
				while (fill < length) {
					fresh.push(next_random(100));
					fill = fill + 1;
				}
				$O(source, fresh, [ 0, $X ]);
			} else if (size > 25) {
				$af(source, [ 0, $X ]);
			} else {
				$Z(source, next_random(100), [ 0, $X ]);
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
console.log("final length " + $i(source).length + ", log held " + $aw(source[1]) + " ops, base " + $ax(source[1]) + ", version " + $ay(source[1]));
$az(source, near);
$az(source, far);
