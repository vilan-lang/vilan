function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
function __insert_at(list, index, value, location) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __remove_at(list, index, location) {
	if (index >= 0 && index < list.length) return list.splice(index, 1)[0];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
	let $A = null;
	if (wrapped < 0) {
		$A = wrapped + modulus;
	} else {
		$A = wrapped;
	}
	return $A;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $b = null;
	if (truncated > 0) {
		$b = truncated;
	} else {
		$b = 0;
	}
	return $b;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $B = null;
	if (wrapped >= half) {
		$B = wrapped - modulus;
	} else {
		$B = wrapped;
	}
	return $B;
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
function mint_subscriber(notify2) {
	const derived2 = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify2, derived2);
}
function subscriber_of(notify2, derived2) {
	return [ fresh_id(), notify2, __shared_new(true), derived2 ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $s = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$s = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$s;
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
				while (!(is_empty(turn[1].v)) && budget > 0) {
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
		if (ne(__list_get(left, index), __list_get(right, index))) {
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
function drain_mirror(source2, cursor3, held2, label, at_turn) {
	let mirror = __clone(held2);
	for (const op of since2(source2, cursor3)) {
		const $P = op;
		let $Q = null;
		if ($P[0] === 2) {
			const items = $P[1];
			resets_to_mirror.v = resets_to_mirror.v + 1;
			mirror = __clone(items);
			$Q = undefined;
		} else if ($P[0] === 1) {
			const at2 = $P[1];
			const _previous = $P[2];
			const value2 = $P[3];
			ops_to_mirror.v = ops_to_mirror.v + 1;
			__at_put(mirror, at2, value2, "delta-law.vl:324:5");
			$Q = undefined;
		} else if ($P[0] === 0) {
			const at3 = $P[1];
			const removed = $P[2];
			const inserted = $P[3];
			ops_to_mirror.v = ops_to_mirror.v + 1;
			let taken = 0;
			const leaving = removed.length;
			while (taken < leaving) {
				__remove_at(mirror, at3, "delta-law.vl:331:28");
				taken = taken + 1;
			}
			let offset = 0;
			const count = inserted.length;
			while (offset < count) {
				const $R = __list_get(inserted, offset);
				let $S = null;
				if ($R[0] === 0) {
					const value3 = $R[1];
					$S = __insert_at(mirror, at3 + offset, value3, "delta-law.vl:338:33");
				} else {
					$S = undefined;
				}
				$S;
				offset = offset + 1;
			}
			$Q = undefined;
		} else {
			const _from = $P[1];
			const _count = $P[2];
			const _to = $P[3];
			(() => {
				throw __panic("the generator emits no Move", "delta-law.vl:345:5");
			})();
			$Q = undefined;
		}
		$Q;
	}
	checks.v = checks.v + 1;
	if (!(same(mirror, get2(source2)))) {
		(() => {
			throw __panic("turn " + at_turn + ": " + label + "=[" + render(mirror) + "] source=[" + render(get2(source2)) + "]", "delta-law.vl:351:3");
		})();
	}
	return mirror;
}
function new3(value2) {
	let subscribers = [  ];
	return [ __shared_new(value2), __shared_new(subscribers) ];
}
function new4(value2) {
	return new3(value2);
}
function with_limit(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function bounded(initial, limit) {
	return [ new4(__clone(initial)), with_limit(limit) ];
}
function cursor(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function cursor2(self) {
	return cursor(self[1]);
}
function get(self) {
	return __clone(self[0].v);
}
function get2(self) {
	return get(self[0]);
}
function new5() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function new6(initial) {
	return [ new4(__clone(initial)), new5() ];
}
function since(self, cursor3) {
	const at2 = cursor3[1].v;
	const version = self[1].v;
	let ops = [  ];
	if (at2 >= version) {
		return [ 0, ops ];
	}
	cursor3[1].v = version;
	const base = self[2].v;
	if (at2 < base) {
		return [ 1 ];
	}
	let index = as_usize(at2 - base);
	const held2 = __clone(self[0].v);
	const length = held2.length;
	while (index < length) {
		const $c = __list_get(held2, index);
		let $d = null;
		if ($c[0] === 0) {
			const op = $c[1];
			$d = ops.push(op);
		} else {
			$d = undefined;
		}
		$d;
		index = index + 1;
	}
	return [ 0, ops ];
}
function drain2(log, elements, cursor3) {
	const $e = since(log, cursor3);
	let $f = null;
	if ($e[0] === 0) {
		const ops = $e[1];
		$f = ops;
	} else {
		let lost = [  ];
		lost.push([ 2, __clone(elements.v) ]);
		$f = lost;
	}
	return $f;
}
function reader(self) {
	const log = __clone(self[1]);
	const elements = self[0][0];
	return (cursor3) => {
		return drain2(log, elements, cursor3);
	};
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor3 of self[3].v) {
		const at2 = cursor3[1].v;
		if (at2 < lowest) {
			lowest = at2;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index = as_usize(dropped);
		const held2 = __clone(self[0].v);
		const length = held2.length;
		while (index < length) {
			const $m = __list_get(held2, index);
			let $n = null;
			if ($m[0] === 0) {
				const op = $m[1];
				$n = kept.push(op);
			} else {
				$n = undefined;
			}
			$n;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function record(self, op) {
	trim(self);
	const next = self[1].v + 1;
	self[1].v = next;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next;
	} else {
		self[0].v.push(__clone(op));
	}
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $t = null;
	if (is_empty(self)) {
		$t = [ 1 ];
	} else {
		$t = __list_get(self, self.length - 1);
	}
	return $t;
}
function notify(self, $p) {
	const $q = $p;
	let $r = null;
	if ($q[0] === 0) {
		const turn = $q[1];
		$r = enqueue(turn, __clone(self[1].v));
	} else {
		const $u = last(draining_turns.v);
		let $v = null;
		if ($u[0] === 0) {
			const draining = $u[1];
			$v = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$v = undefined;
		}
		$r = $v;
	}
	return $r;
}
function update(self, mutate, $o) {
	mutate(self[0].v);
	notify(self, $o);
}
function splice(self, at2, removed, inserted, $j) {
	update(self[0], (list) => {
		let left = [  ];
		let taken = 0;
		while (taken < removed) {
			left.push(__remove_at(list, at2, "delta-law.vl:185:20"));
			taken = taken + 1;
		}
		let offset = 0;
		const count = inserted.length;
		while (offset < count) {
			const $k = __list_get(inserted, offset);
			let $l = null;
			if ($k[0] === 0) {
				const value2 = $k[1];
				$l = __insert_at(list, at2 + offset, value2, "delta-law.vl:192:30");
			} else {
				$l = undefined;
			}
			$l;
			offset = offset + 1;
		}
		record(self[1], [ 0, at2, left, __clone(inserted) ]);
		return;
	}, $j);
}
function set_at(self, at2, value2, $w) {
	update(self[0], (list) => {
		const previous = __clone(__at(list, at2, "delta-law.vl:82:19"));
		__at_put(list, at2, __clone(value2), "delta-law.vl:83:4");
		record(self[1], [ 1, at2, previous, __clone(value2) ]);
		return;
	}, $w);
}
function set(self, value2, $x) {
	update(self[0], (list) => {
		__replace(list, __clone(value2));
		record(self[1], [ 2, __clone(value2) ]);
		return;
	}, $x);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $y = [ 0, cell ];
		let $z = null;
		if ($y[0] === 0) {
			const live = $y[1];
			$z = observer(live.v);
		} else {
			$z = undefined;
		}
		return $z;
	}));
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function attach_observer2(self, observer, immediately) {
	return attach_observer(self[0], observer, immediately);
}
function on_change(self, observer) {
	return attach_observer2(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function map_each(source2, $a) {
	const cursor3 = cursor2(source2);
	let seeded = [  ];
	for (const value2 of get2(source2)) {
		seeded.push(g(value2));
	}
	const out = new6(seeded);
	const read = reader(source2);
	on_change(__clone(source2), (_list, $g) => {
		notifications.v = notifications.v + 1;
		for (const op of read(cursor3)) {
			const $h = op;
			let $i = null;
			if ($h[0] === 0) {
				const at2 = $h[1];
				const removed = $h[2];
				const inserted = $h[3];
				let mapped = [  ];
				for (const value3 of inserted) {
					incremental_calls.v = incremental_calls.v + 1;
					mapped.push(g(value3));
				}
				splice(out, at2, removed.length, mapped, $a);
				$i = undefined;
			} else if ($h[0] === 1) {
				const at3 = $h[1];
				const _previous = $h[2];
				const value4 = $h[3];
				incremental_calls.v = incremental_calls.v + 1;
				set_at(out, at3, g(value4), $a);
				$i = undefined;
			} else if ($h[0] === 2) {
				const items = $h[1];
				let mapped2 = [  ];
				for (const value5 of items) {
					reset_calls.v = reset_calls.v + 1;
					mapped2.push(g(value5));
				}
				set(out, mapped2, $a);
				$i = undefined;
			} else {
				const _from = $h[1];
				const _count = $h[2];
				const _to = $h[3];
				(() => {
					throw __panic("the generator emits no Move", "delta-law.vl:273:6");
				})();
				$i = undefined;
			}
			$i;
		}
		return;
	});
	return out;
}
function size(self) {
	return get(self[0]).length;
}
function push(self, value2, $D) {
	return splice(self, size(self), 0, [ __clone(value2) ], $D);
}
function insert_at(self, at2, value2, $E) {
	return splice(self, at2, 0, [ __clone(value2) ], $E);
}
function remove_at(self, at2, $F) {
	return splice(self, at2, 1, [  ], $F);
}
function clear(self, $G) {
	return splice(self, 0, size(self), [  ], $G);
}
function batch(body, $H) {
	const $I = $H;
	let $J = null;
	if ($I[0] === 0) {
		const current = $I[1];
		$J = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$J = result;
	}
	return $J;
}
function eq(self, b) {
	const $K = self;
	let $N = null;
	if ($K[0] === 0) {
		const $L = b;
		let $M = null;
		if ($L[0] === 0) {
			$M = $K[1] === $L[1];
		} else {
			$M = false;
		}
		$N = $M;
	} else {
		const $O = b;
		$N = $O[0] === 1;
	}
	return $N;
}
function ne(self, b) {
	return !(eq(self, b));
}
function since2(self, cursor3) {
	return drain2(self[1], self[0][0], cursor3);
}
function held(self) {
	return self[0].v.length;
}
function oldest(self) {
	return self[2].v;
}
function at(self) {
	return self[1].v;
}
function drop_cursor(self, cursor3) {
	let kept = [  ];
	for (const held2 of self[3].v) {
		if (held2[0] !== cursor3[0]) {
			kept.push(__clone(held2));
		}
	}
	self[3].v = kept;
}
function drop_cursor2(self, cursor3) {
	drop_cursor(self[1], cursor3);
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
const source = bounded([ 1, 2, 3 ], 30);
const derived = map_each(source, [ 1 ]);
const near = cursor2(source);
const far = cursor2(source);
let mirror_near = [ 1, 2, 3 ];
let mirror_far = [ 1, 2, 3 ];
let turn_index = 1;
while (turn_index <= 400) {
	const before = notifications.v;
	const op_count = 1 + next_random(4);
	batch(($C) => {
		let made = 0;
		while (made < op_count) {
			const size2 = size(source);
			const choice = next_random(20);
			if (choice < 9 || size2 === 0) {
				push(source, next_random(100), [ 0, $C ]);
			} else if (choice < 13) {
				insert_at(source, pick(size2), next_random(100), [ 0, $C ]);
			} else if (choice < 16) {
				remove_at(source, pick(size2), [ 0, $C ]);
			} else if (choice < 18) {
				set_at(source, pick(size2), next_random(100), [ 0, $C ]);
			} else if (choice < 19 && size2 > 15) {
				let fresh = [  ];
				let fill = 0;
				const length = 1 + next_random(8);
				while (fill < length) {
					fresh.push(next_random(100));
					fill = fill + 1;
				}
				set(source, fresh, [ 0, $C ]);
			} else if (size2 > 25) {
				clear(source, [ 0, $C ]);
			} else {
				push(source, next_random(100), [ 0, $C ]);
			}
			made = made + 1;
		}
		return;
	}, [ 1 ]);
	const waves = notifications.v - before;
	if (waves !== 1) {
		(() => {
			throw __panic("turn " + turn_index + ": notifications=" + waves + " (expected 1)", "delta-law.vl:406:4");
		})();
	}
	let reference = [  ];
	for (const value of get2(source)) {
		naive_calls.v = naive_calls.v + 1;
		reference.push(value * 2 + 1);
	}
	checks.v = checks.v + 1;
	if (!(same(get2(derived), reference))) {
		(() => {
			throw __panic("turn " + turn_index + ": derived=[" + render(get2(derived)) + "] expected=[" + render(reference) + "]", "delta-law.vl:417:4");
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
		throw __panic("no lagging cursor was ever answered with ops", "delta-law.vl:437:3");
	})();
}
if (resets_to_mirror.v === 0) {
	(() => {
		throw __panic("no lagging cursor ever fell past the log\'s base", "delta-law.vl:440:3");
	})();
}
if (calls.v >= naive_calls.v) {
	(() => {
		throw __panic("the derivation made " + calls.v + " calls against the rerun\'s " + naive_calls.v, "delta-law.vl:443:3");
	})();
}
console.log("turns=400 checks=" + checks.v + " failures=0");
console.log("lagging cursors: ops drained=" + ops_to_mirror.v + " resets=" + resets_to_mirror.v);
console.log("g calls: incremental=" + calls.v + " (splice/set_at=" + incremental_calls.v + ", reset=" + reset_calls.v + ") naive-rerun=" + naive_calls.v);
console.log("final length " + get2(source).length + ", log held " + held(source[1]) + " ops, base " + oldest(source[1]) + ", version " + at(source[1]));
drop_cursor2(source, near);
drop_cursor2(source, far);
