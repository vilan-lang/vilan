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
let __shared_identity_next = 1;
function __shared_identity(cell) {
	return cell.__id ??= __shared_identity_next++;
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
	let $ah = null;
	if (wrapped < 0) {
		$ah = wrapped + modulus;
	} else {
		$ah = wrapped;
	}
	return $ah;
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
	let $ai = null;
	if (wrapped >= half) {
		$ai = wrapped - modulus;
	} else {
		$ai = wrapped;
	}
	return $ai;
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
		let $w = null;
		if (subscriber[3]) {
			if (!(turn2[3].v.has(key))) {
				turn2[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn2, subscriber[0]);
				}
				turn2[1].v.push(__clone(subscriber));
			}
			$w = undefined;
		} else if (!(turn2[2].v.has(key))) {
			turn2[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn2, subscriber[0]);
			}
			let index = turn2[0].v.length;
			while (index > 0 && __at(turn2[0].v, index - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn2[0].v, index, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$w;
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
						if (tracing.v) {
							trace_hooks.v[4](turn2, subscriber);
						} else if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn2[0].v;
				turn2[0].v = [  ];
				turn2[2].v = new Map();
				for (const subscriber2 of wave) {
					if (tracing.v) {
						trace_hooks.v[4](turn2, subscriber2);
					} else if (subscriber2[2].v) {
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
function queue_wakes(turn2, subscribers, traced, cause) {
	if (traced) {
		waking.v = [ 0, cause ];
		enqueue(turn2, subscribers);
		waking.v = [ 1 ];
	} else {
		enqueue(turn2, subscribers);
	}
}
function dispose(self, $P) {
	const $Q = $P;
	let $R = null;
	if ($Q[0] === 0) {
		const established = $Q[1];
		$R = [ 0, established ];
	} else {
		$R = last(draining_turns.v);
	}
	const ambient = $R;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $S = [ 0, handle[0] ];
	let $T = null;
	if ($S[0] === 0) {
		const subscribers = $S[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	const $U = ambient;
	let $V = null;
	if ($U[0] === 0) {
		const turn2 = $U[1];
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
		if (tracing.v) {
			trace_hooks.v[7](turn2, handle[1]);
		}
		$V = undefined;
	} else {
		$V = undefined;
	}
	$V;
	const $W = handle[3].v;
	let $X = null;
	if ($W[0] === 0) {
		const release = $W[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$X = undefined;
	} else {
		$X = undefined;
	}
	return $X;
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $Y = null;
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
		$Y = undefined;
	}
	return $Y;
}
function register_with_owner(subscription, $K, $L) {
	const $M = $L;
	let $N = null;
	if ($M[0] === 0) {
		const owner = $M[1];
		$N = take(owner, subscription, $K);
	} else {
		$N = __clone(subscription);
	}
	return $N;
}
function defer_to_owner(cleanup, $Z) {
	const $aa = $Z;
	let $ab = null;
	if ($aa[0] === 0) {
		const owner = $aa[1];
		$ab = defer(owner, cleanup);
	} else {
		$ab = undefined;
	}
	return $ab;
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
function new4(value2, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value2), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function list_cell(elements, log) {
	return [ __shared_new(elements), new4(0, "std/src/reactive/delta.vl:790:68"), __clone(log) ];
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
	return [ __shared_new(elements), new4(0, "std/src/reactive/delta.vl:790:68"), __clone(log) ];
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
		left.push(__remove_at(list, start, "std/src/reactive/delta.vl:693:18"));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $m = __list_get(inserted, offset);
		let $n = null;
		if ($m[0] === 0) {
			const value2 = $m[1];
			$n = __insert_at(list, start + offset, value2, "std/src/reactive/delta.vl:700:28");
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
	let $x = null;
	if (is_empty(self)) {
		$x = [ 1 ];
	} else {
		$x = __list_get(self, self.length - 1);
	}
	return $x;
}
function notify(self, $s) {
	const traced = tracing.v;
	let $t = null;
	if (traced) {
		$t = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$t = "";
	}
	const cause = $t;
	const $u = $s;
	let $v = null;
	if ($u[0] === 0) {
		const turn2 = $u[1];
		$v = queue_wakes(turn2, __clone(self[1].v), traced, cause);
	} else {
		const $y = last(draining_turns.v);
		let $z = null;
		if ($y[0] === 0) {
			const draining = $y[1];
			$z = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $A = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$A = undefined;
				}
				$A;
			}
			$z = undefined;
		}
		$v = $z;
	}
	return $v;
}
function set(self, value2, $r, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value2);
	notify(self, $r);
}
function publish(cell, $q) {
	set(cell[1], at(cell[2]), $q, "std/src/reactive/delta.vl:797:17");
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
function set_at(self, at2, value2, $B) {
	const $C = __list_get(self[0].v, at2);
	let $D = null;
	if ($C[0] === 0) {
		const previous = $C[1];
		__at_put(self[0].v, at2, __clone(value2), "std/src/reactive/delta.vl:853:5");
		record(self[2], [ 1, at2, previous, __clone(value2) ]);
		publish(self, $B);
		$D = undefined;
	} else {
		$D = undefined;
	}
	return $D;
}
function set2(self, value2, $E) {
	self[0].v = __clone(value2);
	record(self[2], [ 2, __clone(value2) ]);
	publish(self, $E);
}
function move_range(self, from, count, to, $F) {
	if (count === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:873:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $G = __list_get(lifted, offset);
		let $H = null;
		if ($G[0] === 0) {
			const value2 = $G[1];
			$H = __insert_at(self[0].v, to + offset, value2, "std/src/reactive/delta.vl:879:43");
		} else {
			$H = undefined;
		}
		$H;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count, to ]);
	publish(self, $F);
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $I = [ 0, cell ];
		let $J = null;
		if ($I[0] === 0) {
			const live = $I[1];
			$J = observer(live.v);
		} else {
			$J = undefined;
		}
		return $J;
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
function take(self, item, $O) {
	defer(self, () => {
		dispose(item, $O);
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
	set(cell[1], at(cell[2]), $q, "std/src/reactive/delta.vl:797:17");
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
function push(self, value2, $ac) {
	return splice2(self, size(self), 0, [ __clone(value2) ], $ac);
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
function set_at2(self, at2, value2, $B) {
	const $ap = __list_get(self[0].v, at2);
	let $aq = null;
	if ($ap[0] === 0) {
		const previous = $ap[1];
		__at_put(self[0].v, at2, __clone(value2), "std/src/reactive/delta.vl:853:5");
		record2(self[2], [ 1, at2, previous, __clone(value2) ]);
		publish2(self, $B);
		$aq = undefined;
	} else {
		$aq = undefined;
	}
	return $aq;
}
function set3(self, value2, $E) {
	self[0].v = __clone(value2);
	record2(self[2], [ 2, __clone(value2) ]);
	publish2(self, $E);
}
function move_range2(self, from, count, to, $F) {
	if (count === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:873:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $ar = __list_get(lifted, offset);
		let $as = null;
		if ($ar[0] === 0) {
			const value2 = $ar[1];
			$as = __insert_at(self[0].v, to + offset, value2, "std/src/reactive/delta.vl:879:43");
		} else {
			$as = undefined;
		}
		$as;
		offset = offset + 1;
	}
	record2(self[2], [ 3, from, count, to ]);
	publish2(self, $F);
}
function map_each2(source2, g, $a, $b) {
	const cursor4 = cursor2(source2);
	const out = of2(map(get(source2), g));
	as_derivation();
	const read = reader(source2);
	register_with_owner(attach_observer2(source2, (_published) => {
		for (const op of read(cursor4)) {
			const $aj = op;
			let $ak = null;
			if ($aj[0] === 0) {
				const at2 = $aj[1];
				const removed = $aj[2];
				const inserted = $aj[3];
				splice3(out, at2, removed.length, map(inserted, g), $a);
				$ak = undefined;
			} else if ($aj[0] === 1) {
				const at3 = $aj[1];
				const _was = $aj[2];
				const value2 = $aj[3];
				$ak = set_at2(out, at3, g(value2), $a);
			} else if ($aj[0] === 2) {
				const items = $aj[1];
				$ak = set3(out, map(items, g), $a);
			} else {
				const from = $aj[1];
				const count = $aj[2];
				const to = $aj[3];
				$ak = move_range2(out, from, count, to, $a);
			}
			$ak;
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
function remove_at(self, at2, $au) {
	return splice2(self, at2, 1, [  ], $au);
}
function extend(self, values, $av) {
	return splice2(self, size(self), 0, values, $av);
}
function insert_at(self, at2, value2, $aw) {
	return splice2(self, at2, 0, [ __clone(value2) ], $aw);
}
function prepend(self, value2, $ax) {
	return splice2(self, 0, 0, [ __clone(value2) ], $ax);
}
function pop(self, $aA) {
	const size3 = size(self);
	let $aB = null;
	if (size3 > 0) {
		$aB = splice2(self, size3 - 1, 1, [  ], $aA);
	}
	return $aB;
}
function remove_range(self, at2, count, $aC) {
	return splice2(self, at2, count, [  ], $aC);
}
function truncate(self, length, $aD) {
	const size3 = size(self);
	let $aE = null;
	if (size3 > length) {
		$aE = splice2(self, length, size3 - length, [  ], $aD);
	}
	return $aE;
}
function insert_all(self, at2, values, $aF) {
	return splice2(self, at2, 0, values, $aF);
}
function set_all(self, values, $aG) {
	return splice2(self, 0, size(self), values, $aG);
}
function clear(self, $aH) {
	return splice2(self, 0, size(self), [  ], $aH);
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
function push2(self, value2, $aI) {
	return splice4(self, size2(self), 0, [ __clone(value2) ], $aI);
}
function remove_at2(self, at2, $aJ) {
	return splice4(self, at2, 1, [  ], $aJ);
}
function new6(elements) {
	return [ elements, [  ] ];
}
function edit(self, body, $aK) {
	let recorder = new6(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		record2(self[2], op);
	}
	publish2(self, $aK);
}
function fill(target, $aL) {
	push2(target, "from-fill-1", $aL);
	push2(target, "from-fill-2", $aL);
}
function reconcile_to(self, items, $aM) {
	const old_length = size(self);
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(self[0].v, prefix, "std/src/reactive/delta.vl:945:7") !== __at(items, prefix, "std/src/reactive/delta.vl:945:36")) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(self[0].v, old_length - 1 - suffix, "std/src/reactive/delta.vl:952:7") !== __at(items, new_length - 1 - suffix, "std/src/reactive/delta.vl:952:53")) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $aN = __list_get(items, index);
		let $aO = null;
		if ($aN[0] === 0) {
			const value2 = $aN[1];
			$aO = arriving.push(value2);
		} else {
			$aO = undefined;
		}
		$aO;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	splice2(self, prefix, removed, arriving, $aM);
}
function batch(body, $aS) {
	const $aT = $aS;
	let $aU = null;
	if ($aT[0] === 0) {
		const current = $aT[1];
		$aU = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aU = result;
	}
	return $aU;
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
function remove_range2(self, at2, count, $aW) {
	return splice4(self, at2, count, [  ], $aW);
}
function insert_at2(self, at2, value2, $aX) {
	return splice4(self, at2, 0, [ __clone(value2) ], $aX);
}
function cursor3(self) {
	return cursor(self[2]);
}
function list_cell_since2(log, items, cursor4) {
	const $ba = since(log, cursor4);
	let $bb = null;
	if ($ba[0] === 1) {
		let lost = [  ];
		lost.push([ 2, __clone(items.v) ]);
		$bb = lost;
	} else {
		const recorded = $ba[1];
		$bb = recorded;
	}
	return $bb;
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
			const $bc = op;
			let $bd = null;
			if ($bc[0] === 0) {
				const at2 = $bc[1];
				const removed = $bc[2];
				const inserted = $bc[3];
				splice3(out, at2, removed.length, map(inserted, g), $a);
				$bd = undefined;
			} else if ($bc[0] === 1) {
				const at3 = $bc[1];
				const _was = $bc[2];
				const value2 = $bc[3];
				$bd = set_at2(out, at3, g(value2), $a);
			} else if ($bc[0] === 2) {
				const items = $bc[1];
				$bd = set3(out, map(items, g), $a);
			} else {
				const from = $bc[1];
				const count = $bc[2];
				const to = $bc[3];
				$bd = move_range2(out, from, count, to, $a);
			}
			$bd;
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
function push3(self, value2, $ac) {
	return splice3(self, size(self), 0, [ __clone(value2) ], $ac);
}
function insert_at3(self, at2, value2, $aw) {
	return splice3(self, at2, 0, [ __clone(value2) ], $aw);
}
function remove_at3(self, at2, $au) {
	return splice3(self, at2, 1, [  ], $au);
}
function pop2(self, $aA) {
	const size3 = size(self);
	let $bg = null;
	if (size3 > 0) {
		$bg = splice3(self, size3 - 1, 1, [  ], $aA);
	}
	return $bg;
}
function remove_range3(self, at2, count, $aC) {
	return splice3(self, at2, count, [  ], $aC);
}
function truncate2(self, length, $aD) {
	const size3 = size(self);
	let $bh = null;
	if (size3 > length) {
		$bh = splice3(self, length, size3 - length, [  ], $aD);
	}
	return $bh;
}
function reconcile_to2(self, items, $aM) {
	const old_length = size(self);
	const new_length = items.length;
	let prefix = 0;
	while (prefix < old_length && prefix < new_length) {
		if (__at(self[0].v, prefix, "std/src/reactive/delta.vl:945:7") !== __at(items, prefix, "std/src/reactive/delta.vl:945:36")) {
			break;
		}
		prefix = prefix + 1;
	}
	let suffix = 0;
	while (prefix + suffix < old_length && prefix + suffix < new_length) {
		if (__at(self[0].v, old_length - 1 - suffix, "std/src/reactive/delta.vl:952:7") !== __at(items, new_length - 1 - suffix, "std/src/reactive/delta.vl:952:53")) {
			break;
		}
		suffix = suffix + 1;
	}
	const removed = old_length - prefix - suffix;
	let arriving = [  ];
	let index = prefix;
	while (index < new_length - suffix) {
		const $bi = __list_get(items, index);
		let $bj = null;
		if ($bi[0] === 0) {
			const value2 = $bi[1];
			$bj = arriving.push(value2);
		} else {
			$bj = undefined;
		}
		$bj;
		index = index + 1;
	}
	if (removed === 0 && arriving.length === 0) {
		return;
	}
	splice3(self, prefix, removed, arriving, $aM);
}
function push4(self, value2, $aI) {
	return splice4(self, size2(self), 0, [ __clone(value2) ], $aI);
}
function edit2(self, body, $aK) {
	let recorder = new6(__clone(self[0].v));
	body(recorder);
	const produced = __clone(recorder[0]);
	const recorded = __clone(recorder[1]);
	self[0].v = produced;
	for (const op of recorded) {
		record2(self[2], op);
	}
	publish2(self, $aK);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const tracing = __shared_new(false);
const trace_hooks = __shared_new([ (_identity, _site) => {
	return;
}, (_site) => {
	return;
}, (_identity) => {
	return "";
}, (_turn, _id) => {
	return;
}, (_turn, _subscriber) => {
	return;
}, (_subscriber, _cause) => {
	return;
}, (_subscriber, _draining) => {
	return false;
}, (_turn, _id) => {
	return;
} ]);
const waking = __shared_new([ 1 ]);
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
on_change(__clone(source), (_list, $at) => {
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
batch(($aR) => {
	push(source, "r", [ 0, $aR ]);
	push(source, "ss", [ 0, $aR ]);
	remove_at(source, 0, [ 0, $aR ]);
	return;
}, [ 1 ]);
expect("one batch, one notification", notifications.v - before_batch, 1);
law("batch", source, derived);
const lagging = with_limit2([ "seed" ], 3);
const mirror = map_each2(lagging, counted, [ 1 ], [ 1 ]);
const lag_calls = calls.v;
batch(($aV) => {
	let round = 0;
	while (round < 8) {
		push(lagging, "row-" + round, [ 0, $aV ]);
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
on_change2(__clone(walk), (_list, $be) => {
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
	batch(($bf) => {
		let made = 0;
		while (made < op_count) {
			const size3 = size(walk);
			const choice = next_random(24);
			if (choice < 8 || size3 === 0) {
				push3(walk, next_random(100), [ 0, $bf ]);
			} else if (choice < 11) {
				insert_at3(walk, pick(size3 + 1), next_random(100), [ 0, $bf ]);
			} else if (choice < 13) {
				remove_at3(walk, pick(size3), [ 0, $bf ]);
			} else if (choice < 15) {
				set_at2(walk, pick(size3), next_random(100), [ 0, $bf ]);
			} else if (choice < 17) {
				const from = pick(size3);
				const count = 1 + pick(size3 - from);
				move_range2(walk, from, count, pick(size3 - count + 1), [ 0, $bf ]);
			} else if (choice < 18) {
				pop2(walk, [ 0, $bf ]);
			} else if (choice < 19 && size3 > 12) {
				remove_range3(walk, pick(size3), 1 + pick(4), [ 0, $bf ]);
			} else if (choice < 20 && size3 > 20) {
				truncate2(walk, pick(size3), [ 0, $bf ]);
			} else if (choice < 21 && size3 > 16) {
				let fresh = [  ];
				let fill_index = 0;
				while (fill_index < 1 + next_random(6)) {
					fresh.push(next_random(100));
					fill_index = fill_index + 1;
				}
				set3(walk, fresh, [ 0, $bf ]);
			} else if (choice < 22) {
				let edited = get(walk);
				__at_put(edited, pick(size3), next_random(100), "list-cell.vl:371:6");
				edited.push(next_random(100));
				reconcile_to2(walk, edited, [ 0, $bf ]);
			} else {
				const at2 = pick(size3);
				edit2(walk, (list) => {
					insert_at2(list, at2, next_random(100), [ 0, $bf ]);
					remove_at2(list, 0, [ 0, $bf ]);
					push4(list, next_random(100), [ 0, $bf ]);
					return;
				}, [ 0, $bf ]);
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
//# sourceMappingURL=list-cell.mjs.map
