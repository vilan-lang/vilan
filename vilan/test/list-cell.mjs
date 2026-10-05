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
	let $af = null;
	if (wrapped < 0) {
		$af = wrapped + modulus;
	} else {
		$af = wrapped;
	}
	return $af;
}
function saturate_unsigned(value2) {
	const truncated = Math.trunc(value2);
	let $c = null;
	if (truncated > 0) {
		$c = truncated;
	} else {
		$c = 0;
	}
	return $c;
}
function fold_signed(value2, modulus, half) {
	const wrapped = fold_unsigned(value2, modulus);
	let $ag = null;
	if (wrapped >= half) {
		$ag = wrapped - modulus;
	} else {
		$ag = wrapped;
	}
	return $ag;
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
function enqueue(turn2, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $v = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				turn2[1].v.push(__clone(subscriber));
			}
			$v = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$v;
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
				while (!(is_empty(turn2[1].v)) && budget > 0) {
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
function dispose(self, $N) {
	const $O = $N;
	let $P = null;
	if ($O[0] === 0) {
		const established = $O[1];
		$P = [ 0, established ];
	} else {
		$P = last(draining_turns.v);
	}
	const ambient = $P;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $Q = [ 0, handle[0] ];
	let $R = null;
	if ($Q[0] === 0) {
		const subscribers = $Q[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$R = undefined;
	} else {
		$R = undefined;
	}
	$R;
	const $S = ambient;
	let $T = null;
	if ($S[0] === 0) {
		const turn2 = $S[1];
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
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	const $U = handle[3].v;
	let $V = null;
	if ($U[0] === 0) {
		const release = $U[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$V = undefined;
	} else {
		$V = undefined;
	}
	return $V;
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $W = null;
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
		$W = undefined;
	}
	return $W;
}
function register_with_owner(subscription, $I, $J) {
	const $K = $J;
	let $L = null;
	if ($K[0] === 0) {
		const owner = $K[1];
		$L = take(owner, subscription, $I);
	} else {
		$L = __clone(subscription);
	}
	return $L;
}
function defer_to_owner(cleanup, $X) {
	const $Y = $X;
	let $Z = null;
	if ($Y[0] === 0) {
		const owner = $Y[1];
		$Z = defer(owner, cleanup);
	} else {
		$Z = undefined;
	}
	return $Z;
}
function splice_start(held2, at2) {
	let $k = null;
	if (at2 > held2) {
		$k = held2;
	} else {
		$k = at2;
	}
	return $k;
}
function splice_count(held2, start, removed) {
	let $l = null;
	if (start + removed > held2) {
		$l = held2 - start;
	} else {
		$l = removed;
	}
	return $l;
}
function counted(text) {
	calls.v = calls.v + 1;
	return as_i322(text.length);
}
function law(label, source2, derived2) {
	let naive2 = [  ];
	for (const value2 of get(source2)) {
		naive2.push(as_i322(value2.length));
	}
	const held2 = get(derived2);
	if (held2.length !== naive2.length) {
		(() => {
			throw __panic("" + label + ": derived holds " + held2.length + " where the rerun holds " + naive2.length, "list-cell.vl:66:3");
		})();
	}
	let index = 0;
	while (index < naive2.length) {
		if (__at(held2, index, "list-cell.vl:70:6") !== __at(naive2, index, "list-cell.vl:70:21")) {
			(() => {
				throw __panic("" + label + ": element " + index + " is " + __at(held2, index, "list-cell.vl:71:41") + ", not " + __at(naive2, index, "list-cell.vl:71:60"), "list-cell.vl:71:4");
			})();
		}
		index = index + 1;
	}
}
function expect(label, seen, wanted) {
	if (seen !== wanted) {
		(() => {
			throw __panic("" + label + ": " + seen + ", expected " + wanted, "list-cell.vl:79:3");
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
function same(label, turn2, held2, wanted) {
	if (held2.length !== wanted.length) {
		(() => {
			throw __panic("turn " + turn2 + ": " + label + " holds " + held2.length + ", the rerun " + wanted.length, "list-cell.vl:131:3");
		})();
	}
	let index = 0;
	while (index < wanted.length) {
		if (__at(held2, index, "list-cell.vl:135:6") !== __at(wanted, index, "list-cell.vl:135:21")) {
			(() => {
				throw __panic("turn " + turn2 + ": " + label + "[" + index + "] is " + __at(held2, index, "list-cell.vl:136:46") + ", not " + __at(wanted, index, "list-cell.vl:136:65"), "list-cell.vl:136:4");
			})();
		}
		index = index + 1;
	}
}
function new3() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function new4(value2) {
	let subscribers = [  ];
	return [ __shared_new(value2), __shared_new(subscribers) ];
}
function list_cell(elements, log) {
	return [ __shared_new(elements), new4(0), __clone(log) ];
}
function new5() {
	return list_cell([  ], new3());
}
function cursor(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function cursor2(self) {
	return cursor(self[2]);
}
function get(self) {
	return __clone(self[0].v);
}
function map(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function list_cell2(elements, log) {
	return [ __shared_new(elements), new4(0), __clone(log) ];
}
function of(elements) {
	return list_cell2(elements, new3());
}
function since(self, cursor4) {
	const at2 = cursor4[1].v;
	const version = self[1].v;
	let ops = [  ];
	if (at2 >= version) {
		return [ 0, ops ];
	}
	cursor4[1].v = version;
	const base = self[2].v;
	if (at2 < base) {
		return [ 1 ];
	}
	let index = as_usize(at2 - base);
	const held2 = __clone(self[0].v);
	const length = held2.length;
	while (index < length) {
		const $d = __list_get(held2, index);
		let $e = null;
		if ($d[0] === 0) {
			const op = $d[1];
			$e = ops.push(op);
		} else {
			$e = undefined;
		}
		$e;
		index = index + 1;
	}
	return [ 0, ops ];
}
function list_cell_since(log, items, cursor4) {
	const $f = since(log, cursor4);
	let $g = null;
	if ($f[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$g = lost;
	} else {
		const recorded = $f[1];
		$g = recorded;
	}
	return $g;
}
function reader(self) {
	const log = __clone(self[2]);
	const items = self[0];
	return (cursor4) => {
		return list_cell_since(log, items, cursor4);
	};
}
function size(self) {
	return self[0].v.length;
}
function splice_in_place(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start, "std/src/reactive/delta.vl:692:18"));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $m = __list_get(inserted, offset);
		let $n = null;
		if ($m[0] === 0) {
			const value2 = $m[1];
			$n = __insert_at(list, start + offset, value2, "std/src/reactive/delta.vl:699:28");
		} else {
			$n = undefined;
		}
		$n;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor4 of self[3].v) {
		const at2 = cursor4[1].v;
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
			const $o = __list_get(held2, index);
			let $p = null;
			if ($o[0] === 0) {
				const op = $o[1];
				$p = kept.push(op);
			} else {
				$p = undefined;
			}
			$p;
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
function at(self) {
	return self[1].v;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $w = null;
	if (is_empty(self)) {
		$w = [ 1 ];
	} else {
		$w = __list_get(self, self.length - 1);
	}
	return $w;
}
function notify(self, $s) {
	const $t = $s;
	let $u = null;
	if ($t[0] === 0) {
		const turn2 = $t[1];
		$u = enqueue(turn2, __clone(self[1].v));
	} else {
		const $x = last(draining_turns.v);
		let $y = null;
		if ($x[0] === 0) {
			const draining = $x[1];
			$y = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$y = undefined;
		}
		$u = $y;
	}
	return $u;
}
function set(self, value2, $r) {
	self[0].v = __clone(value2);
	notify(self, $r);
}
function publish(cell, $q) {
	set(cell[1], at(cell[2]), $q);
}
function splice(self, at2, removed, inserted, $j) {
	const held2 = size(self);
	const start = splice_start(held2, at2);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $j);
}
function set_at(self, at2, value2, $z) {
	const $A = __list_get(self[0].v, at2);
	let $B = null;
	if ($A[0] === 0) {
		const previous = $A[1];
		__at_put(self[0].v, at2, __clone(value2), "std/src/reactive/delta.vl:844:5");
		record(self[2], [ 1, at2, previous, __clone(value2) ]);
		publish(self, $z);
		$B = undefined;
	} else {
		$B = undefined;
	}
	return $B;
}
function set2(self, value2, $C) {
	self[0].v = __clone(value2);
	record(self[2], [ 2, __clone(value2) ]);
	publish(self, $C);
}
function move_range(self, from, count, to, $D) {
	if (count === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:864:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $E = __list_get(lifted, offset);
		let $F = null;
		if ($E[0] === 0) {
			const value2 = $E[1];
			$F = __insert_at(self[0].v, to + offset, value2, "std/src/reactive/delta.vl:870:43");
		} else {
			$F = undefined;
		}
		$F;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count, to ]);
	publish(self, $D);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $G = [ 0, cell ];
		let $H = null;
		if ($G[0] === 0) {
			const live = $G[1];
			$H = observer(live.v);
		} else {
			$H = undefined;
		}
		return $H;
	}));
}
function get2(self) {
	return __clone(self[0].v);
}
function attach_observer(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get2(self));
	}
	return subscription;
}
function attach_observer2(self, observer, immediately) {
	const items = self[0];
	return attach_observer(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function take(self, item, $M) {
	defer(self, () => {
		dispose(item, $M);
		return;
	});
	return __clone(item);
}
function drop_cursor(self, cursor4) {
	let kept = [  ];
	for (const held2 of self[3].v) {
		if (held2[0] !== cursor4[0]) {
			kept.push(__clone(held2));
		}
	}
	self[3].v = kept;
}
function drop_cursor2(self, cursor4) {
	drop_cursor(self[2], cursor4);
}
function map_each(source2, g, $a, $b) {
	const cursor4 = cursor2(source2);
	const out = of(map(get(source2), g));
	as_derivation();
	const read = reader(source2);
	register_with_owner(attach_observer2(source2, (_published) => {
		for (const op of read(cursor4)) {
			const $h = op;
			let $i = null;
			if ($h[0] === 0) {
				const at2 = $h[1];
				const removed = $h[2];
				const inserted = $h[3];
				splice(out, at2, removed.length, map(inserted, g), $a);
				$i = undefined;
			} else if ($h[0] === 1) {
				const at3 = $h[1];
				const _was = $h[2];
				const value2 = $h[3];
				$i = set_at(out, at3, g(value2), $a);
			} else if ($h[0] === 2) {
				const items = $h[1];
				$i = set2(out, map(items, g), $a);
			} else {
				const from = $h[1];
				const count = $h[2];
				const to = $h[3];
				$i = move_range(out, from, count, to, $a);
			}
			$i;
		}
		return;
	}, false), $a, $b);
	defer_to_owner(() => {
		return drop_cursor2(source2, cursor4);
	}, $b);
	return out;
}
function record2(self, op) {
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
function publish2(cell, $q) {
	set(cell[1], at(cell[2]), $q);
}
function splice2(self, at2, removed, inserted, $j) {
	const held2 = size(self);
	const start = splice_start(held2, at2);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record2(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish2(self, $j);
}
function push(self, value2, $aa) {
	return splice2(self, size(self), 0, [ __clone(value2) ], $aa);
}
function of2(elements) {
	return list_cell2(elements, new3());
}
function splice3(self, at2, removed, inserted, $j) {
	const held2 = size(self);
	const start = splice_start(held2, at2);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record2(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish2(self, $j);
}
function set_at2(self, at2, value2, $z) {
	const $an = __list_get(self[0].v, at2);
	let $ao = null;
	if ($an[0] === 0) {
		const previous = $an[1];
		__at_put(self[0].v, at2, __clone(value2), "std/src/reactive/delta.vl:844:5");
		record2(self[2], [ 1, at2, previous, __clone(value2) ]);
		publish2(self, $z);
		$ao = undefined;
	} else {
		$ao = undefined;
	}
	return $ao;
}
function set3(self, value2, $C) {
	self[0].v = __clone(value2);
	record2(self[2], [ 2, __clone(value2) ]);
	publish2(self, $C);
}
function move_range2(self, from, count, to, $D) {
	if (count === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:864:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $ap = __list_get(lifted, offset);
		let $aq = null;
		if ($ap[0] === 0) {
			const value2 = $ap[1];
			$aq = __insert_at(self[0].v, to + offset, value2, "std/src/reactive/delta.vl:870:43");
		} else {
			$aq = undefined;
		}
		$aq;
		offset = offset + 1;
	}
	record2(self[2], [ 3, from, count, to ]);
	publish2(self, $D);
}
function map_each2(source2, g, $a, $b) {
	const cursor4 = cursor2(source2);
	const out = of2(map(get(source2), g));
	as_derivation();
	const read = reader(source2);
	register_with_owner(attach_observer2(source2, (_published) => {
		for (const op of read(cursor4)) {
			const $ah = op;
			let $ai = null;
			if ($ah[0] === 0) {
				const at2 = $ah[1];
				const removed = $ah[2];
				const inserted = $ah[3];
				splice3(out, at2, removed.length, map(inserted, g), $a);
				$ai = undefined;
			} else if ($ah[0] === 1) {
				const at3 = $ah[1];
				const _was = $ah[2];
				const value2 = $ah[3];
				$ai = set_at2(out, at3, g(value2), $a);
			} else if ($ah[0] === 2) {
				const items = $ah[1];
				$ai = set3(out, map(items, g), $a);
			} else {
				const from = $ah[1];
				const count = $ah[2];
				const to = $ah[3];
				$ai = move_range2(out, from, count, to, $a);
			}
			$ai;
		}
		return;
	}, false), $a, $b);
	defer_to_owner(() => {
		return drop_cursor2(source2, cursor4);
	}, $b);
	return out;
}
function on_change(self, observer) {
	return attach_observer2(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function remove_at(self, at2, $as) {
	return splice2(self, at2, 1, [  ], $as);
}
function extend(self, values, $at) {
	return splice2(self, size(self), 0, values, $at);
}
function insert_at(self, at2, value2, $au) {
	return splice2(self, at2, 0, [ __clone(value2) ], $au);
}
function prepend(self, value2, $av) {
	return splice2(self, 0, 0, [ __clone(value2) ], $av);
}
function pop(self, $ay) {
	const size3 = size(self);
	let $az = null;
	if (size3 > 0) {
		$az = splice2(self, size3 - 1, 1, [  ], $ay);
	}
	return $az;
}
function remove_range(self, at2, count, $aA) {
	return splice2(self, at2, count, [  ], $aA);
}
function truncate(self, length, $aB) {
	const size3 = size(self);
	let $aC = null;
	if (size3 > length) {
		$aC = splice2(self, length, size3 - length, [  ], $aB);
	}
	return $aC;
}
function insert_all(self, at2, values, $aD) {
	return splice2(self, at2, 0, values, $aD);
}
function set_all(self, values, $aE) {
	return splice2(self, 0, size(self), values, $aE);
}
function clear(self, $aF) {
	return splice2(self, 0, size(self), [  ], $aF);
}
function is_empty2(self) {
	return size(self) === 0;
}
function size2(self) {
	return self[0].length;
}
function splice4(self, at2, removed, inserted) {
	const held2 = self[0].length;
	const start = splice_start(held2, at2);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	self[1].push(splice_in_place(self[0], start, taking, inserted));
}
function push2(self, value2, $aG) {
	return splice4(self, size2(self), 0, [ __clone(value2) ], $aG);
}
function remove_at2(self, at2, $aH) {
	return splice4(self, at2, 1, [  ], $aH);
}
function new6(elements) {
	return [ elements, [  ] ];
}
function edit(self, body, $aI) {
	let recorder = new6(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		record2(self[2], op);
	}
	publish2(self, $aI);
}
function fill(target, $aJ) {
	push2(target, "from-fill-1", $aJ);
	push2(target, "from-fill-2", $aJ);
}
function reconcile_to(self, items, $aK) {
	const old_length = size(self);
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(self[0].v, prefix, "std/src/reactive/delta.vl:925:7") !== __at(items, prefix, "std/src/reactive/delta.vl:925:36")) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(self[0].v, old_length - 1 - suffix, "std/src/reactive/delta.vl:932:7") !== __at(items, new_length - 1 - suffix, "std/src/reactive/delta.vl:932:53")) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $aL = __list_get(items, index);
		let $aM = null;
		if ($aL[0] === 0) {
			const value2 = $aL[1];
			$aM = arriving.push(value2);
		} else {
			$aM = undefined;
		}
		$aM;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	splice2(self, prefix, removed, arriving, $aK);
}
function batch(body, $aQ) {
	const $aR = $aQ;
	let $aS = null;
	if ($aR[0] === 0) {
		const current = $aR[1];
		$aS = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aS = result;
	}
	return $aS;
}
function with_limit(limit) {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), limit ];
}
function with_limit2(elements, limit) {
	return list_cell(elements, with_limit(limit));
}
function held(self) {
	return self[0].v.length;
}
function logged(self) {
	return held(self[2]);
}
function of3(elements) {
	return list_cell(elements, new3());
}
function remove_range2(self, at2, count, $aU) {
	return splice4(self, at2, count, [  ], $aU);
}
function insert_at2(self, at2, value2, $aV) {
	return splice4(self, at2, 0, [ __clone(value2) ], $aV);
}
function cursor3(self) {
	return cursor(self[2]);
}
function list_cell_since2(log, items, cursor4) {
	const $aY = since(log, cursor4);
	let $aZ = null;
	if ($aY[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$aZ = lost;
	} else {
		const recorded = $aY[1];
		$aZ = recorded;
	}
	return $aZ;
}
function reader2(self) {
	const log = __clone(self[2]);
	const items = self[0];
	return (cursor4) => {
		return list_cell_since2(log, items, cursor4);
	};
}
function attach_observer3(self, observer, immediately) {
	const items = self[0];
	return attach_observer(self[1], (_sequence) => {
		return observer(items.v);
	}, immediately);
}
function drop_cursor3(self, cursor4) {
	drop_cursor(self[2], cursor4);
}
function map_each3(source2, g, $a, $b) {
	const cursor4 = cursor3(source2);
	const out = of2(map(get(source2), g));
	as_derivation();
	const read = reader2(source2);
	register_with_owner(attach_observer3(source2, (_published) => {
		for (const op of read(cursor4)) {
			const $ba = op;
			let $bb = null;
			if ($ba[0] === 0) {
				const at2 = $ba[1];
				const removed = $ba[2];
				const inserted = $ba[3];
				splice3(out, at2, removed.length, map(inserted, g), $a);
				$bb = undefined;
			} else if ($ba[0] === 1) {
				const at3 = $ba[1];
				const _was = $ba[2];
				const value2 = $ba[3];
				$bb = set_at2(out, at3, g(value2), $a);
			} else if ($ba[0] === 2) {
				const items = $ba[1];
				$bb = set3(out, map(items, g), $a);
			} else {
				const from = $ba[1];
				const count = $ba[2];
				const to = $ba[3];
				$bb = move_range2(out, from, count, to, $a);
			}
			$bb;
		}
		return;
	}, false), $a, $b);
	defer_to_owner(() => {
		return drop_cursor3(source2, cursor4);
	}, $b);
	return out;
}
function on_change2(self, observer) {
	return attach_observer3(self, (value2) => {
		return (() => {
			return observer(value2, [ 1 ]);
		})();
	}, false);
}
function push3(self, value2, $aa) {
	return splice3(self, size(self), 0, [ __clone(value2) ], $aa);
}
function insert_at3(self, at2, value2, $au) {
	return splice3(self, at2, 0, [ __clone(value2) ], $au);
}
function remove_at3(self, at2, $as) {
	return splice3(self, at2, 1, [  ], $as);
}
function pop2(self, $ay) {
	const size3 = size(self);
	let $be = null;
	if (size3 > 0) {
		$be = splice3(self, size3 - 1, 1, [  ], $ay);
	}
	return $be;
}
function remove_range3(self, at2, count, $aA) {
	return splice3(self, at2, count, [  ], $aA);
}
function truncate2(self, length, $aB) {
	const size3 = size(self);
	let $bf = null;
	if (size3 > length) {
		$bf = splice3(self, length, size3 - length, [  ], $aB);
	}
	return $bf;
}
function reconcile_to2(self, items, $aK) {
	const old_length = size(self);
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(self[0].v, prefix, "std/src/reactive/delta.vl:925:7") !== __at(items, prefix, "std/src/reactive/delta.vl:925:36")) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(self[0].v, old_length - 1 - suffix, "std/src/reactive/delta.vl:932:7") !== __at(items, new_length - 1 - suffix, "std/src/reactive/delta.vl:932:53")) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $bg = __list_get(items, index);
		let $bh = null;
		if ($bg[0] === 0) {
			const value2 = $bg[1];
			$bh = arriving.push(value2);
		} else {
			$bh = undefined;
		}
		$bh;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	splice3(self, prefix, removed, arriving, $aK);
}
function push4(self, value2, $aG) {
	return splice4(self, size2(self), 0, [ __clone(value2) ], $aG);
}
function edit2(self, body, $aI) {
	let recorder = new6(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		record2(self[2], op);
	}
	publish2(self, $aI);
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
const my_list = new5();
const my_nums = map_each(my_list, (x) => {
	console.log("ran");
	return x.length;
}, [ 1 ], [ 1 ]);
push(my_list, "10.5", [ 1 ]);
console.log("after one push: " + __at(get(my_nums), 0, "list-cell.vl:150:27"));
const source = new5();
const derived = map_each2(source, counted, [ 1 ], [ 1 ]);
on_change(__clone(source), (_list, $ar) => {
	notifications.v = notifications.v + 1;
	return;
});
expect("seeded calls", calls.v, 0);
push(source, "aa", [ 1 ]);
push(source, "bbb", [ 1 ]);
push(source, "c", [ 1 ]);
law("3 pushes", source, derived);
expect("3 pushes", calls.v, 3);
remove_at(source, 1, [ 1 ]);
law("remove_at", source, derived);
expect("a removal runs g", calls.v, 3);
extend(source, [ "dddd", "ee" ], [ 1 ]);
law("extend", source, derived);
expect("extend x2", calls.v, 5);
insert_at(source, 1, "zz", [ 1 ]);
law("insert_at", source, derived);
expect("insert_at", calls.v, 6);
prepend(source, "f", [ 1 ]);
law("prepend", source, derived);
expect("prepend", calls.v, 7);
set_at2(source, 0, "gggggg", [ 1 ]);
law("set_at", source, derived);
expect("set_at", calls.v, 8);
pop(source, [ 1 ]);
law("pop", source, derived);
expect("pop runs g", calls.v, 8);
remove_range(source, 0, 2, [ 1 ]);
law("remove_range", source, derived);
expect("remove_range runs g", calls.v, 8);
truncate(source, 1, [ 1 ]);
law("truncate", source, derived);
expect("truncate runs g", calls.v, 8);
insert_all(source, 0, [ "h", "ii" ], [ 1 ]);
law("insert_all", source, derived);
expect("insert_all x2", calls.v, 10);
set_all(source, [ "jjj" ], [ 1 ]);
law("set_all", source, derived);
expect("set_all x1", calls.v, 11);
clear(source, [ 1 ]);
law("clear", source, derived);
expect("clear runs g", calls.v, 11);
if (!(is_empty2(source))) {
	(() => {
		throw __panic("clear left something behind", "list-cell.vl:210:3");
	})();
}
console.log("twelve defaults: calls=" + calls.v + " notifications=" + notifications.v);
const before_edit = notifications.v;
const before_calls = calls.v;
edit(source, (list) => {
	push2(list, "kk", [ 1 ]);
	push2(list, "lll", [ 1 ]);
	remove_at2(list, 0, [ 1 ]);
	push2(list, "m", [ 1 ]);
	return;
}, [ 1 ]);
expect("one edit, one notification", notifications.v - before_edit, 1);
expect("one edit, three insertions", calls.v - before_calls, 3);
law("edit", source, derived);
console.log("after edit: " + render(get(source)));
edit(source, (list) => {
	fill(list, [ 1 ]);
	return;
}, [ 1 ]);
law("fill through the bound", source, derived);
console.log("after fill: " + render(get(source)));
const before_set = calls.v;
set3(source, [ "n", "oo", "ppp" ], [ 1 ]);
law("set(whole)", source, derived);
expect("set(whole) is the honest N", calls.v - before_set, 3);
const before_reconcile = calls.v;
reconcile_to(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
law("reconcile_to", source, derived);
expect("reconcile_to runs g once", calls.v - before_reconcile, 1);
const quiet = notifications.v;
reconcile_to(source, [ "n", "oo", "qqqq", "ppp" ], [ 1 ]);
expect("an unchanged reconcile_to is silent", notifications.v - quiet, 0);
expect("an unchanged reconcile_to runs g", calls.v - before_reconcile, 1);
const before_move = calls.v;
move_range2(source, 0, 1, 3, [ 1 ]);
law("move_range", source, derived);
expect("a move runs g", calls.v - before_move, 0);
console.log("after move: " + render(get(source)));
const before_batch = notifications.v;
batch(($aP) => {
	push(source, "r", [ 0, $aP ]);
	push(source, "ss", [ 0, $aP ]);
	remove_at(source, 0, [ 0, $aP ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = with_limit2([ "seed" ], 3);
const mirror = map_each2(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
batch(($aT) => {
	let round = 0;
	while (round < 8) {
		push(lagging, "row-" + round, [ 0, $aT ]);
		round = round + 1;
	}
	return;
}, [ 1 ]);
law("past the log limit", lagging, mirror);
console.log("lagged: held=" + logged(lagging) + " calls=" + (calls.v - lag_calls));
console.log("total: calls=" + calls.v + " notifications=" + notifications.v);
const clamped = of3([ "a", "bb", "ccc", "dddd" ]);
const clamped_lengths = map_each2(clamped, counted, [ 1 ], [ 1 ]);
const clamp_calls = calls.v;
edit(clamped, (list) => {
	remove_range2(list, 1, 99, [ 1 ]);
	remove_at2(list, 50, [ 1 ]);
	insert_at2(list, 0, "front", [ 1 ]);
	return;
}, [ 1 ]);
law("clamped edit", clamped, clamped_lengths);
remove_range(clamped, 1, 99, [ 1 ]);
remove_at(clamped, 50, [ 1 ]);
law("clamped cell", clamped, clamped_lengths);
expect("clamping ran g for the one insertion", calls.v - clamp_calls, 1);
console.log("clamped: " + render(get(clamped)));
const walk = of2([ 1, 2, 3 ]);
const first = map_each3(walk, doubled, [ 1 ], [ 1 ]);
const second = map_each3(first, shifted, [ 1 ], [ 1 ]);
const walk_notifications = __shared_new(0);
on_change2(__clone(walk), (_list, $bc) => {
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
	batch(($bd) => {
		let made = 0;
		while (made < op_count) {
			const size3 = size(walk);
			const choice = next_random(24);
			if (choice < 8 || size3 === 0) {
				push3(walk, next_random(100), [ 0, $bd ]);
			} else if (choice < 11) {
				insert_at3(walk, pick(size3 + 1), next_random(100), [ 0, $bd ]);
			} else if (choice < 13) {
				remove_at3(walk, pick(size3), [ 0, $bd ]);
			} else if (choice < 15) {
				set_at2(walk, pick(size3), next_random(100), [ 0, $bd ]);
			} else if (choice < 17) {
				const from = pick(size3);
				const count = 1 + pick(size3 - from);
				move_range2(walk, from, count, pick(size3 - count + 1), [ 0, $bd ]);
			} else if (choice < 18) {
				pop2(walk, [ 0, $bd ]);
			} else if (choice < 19 && size3 > 12) {
				remove_range3(walk, pick(size3), 1 + pick(4), [ 0, $bd ]);
			} else if (choice < 20 && size3 > 20) {
				truncate2(walk, pick(size3), [ 0, $bd ]);
			} else if (choice < 21 && size3 > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				set3(walk, fresh, [ 0, $bd ]);
			} else if (choice < 22) {
				let edited = get(walk);
				__at_put(edited, pick(size3), next_random(100), "list-cell.vl:371:6");
				edited.push(next_random(100));
				reconcile_to2(walk, edited, [ 0, $bd ]);
			} else {
				const at2 = pick(size3);
				edit2(walk, (list) => {
					insert_at2(list, at2, next_random(100), [ 0, $bd ]);
					remove_at2(list, 0, [ 0, $bd ]);
					push4(list, next_random(100), [ 0, $bd ]);
					return;
				}, [ 0, $bd ]);
			}
			made = made + 1;
		}
		return;
	}, [ 1 ]);
	const waves = walk_notifications.v - before;
	if (waves > 1) {
		(() => {
			throw __panic("turn " + turn + ": " + waves + " notifications, expected one per turn", "list-cell.vl:390:4");
		})();
	}
	if (waves === 0) {
		silent = silent + 1;
	}
	let first_wanted = [  ];
	let second_wanted = [  ];
	for (const value of get(walk)) {
		first_wanted.push(value * 2 + 1);
		second_wanted.push(value * 2 + 8);
		naive = naive + 1;
	}
	same("first", turn, get(first), first_wanted);
	same("second", turn, get(second), second_wanted);
	turn = turn + 1;
}
if (walk_calls.v + chain_calls.v >= naive) {
	(() => {
		throw __panic("walk: " + walk_calls.v + " + " + chain_calls.v + " calls against a rerun of " + naive, "list-cell.vl:409:3");
	})();
}
console.log("walk: turns=300 silent=" + silent + " length=" + get(walk).length + " g=" + walk_calls.v + " h=" + chain_calls.v + " rerun=" + naive);
