function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
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
function __nmap_get(table, hash) {
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0 ? [ 0, __clone(entry[1][1]) ] : [ 1 ];
}
function __nmap_has(table, hash) {
	if (hash !== table[1]) return table[0].has(hash);
	const entry = table[0].get(hash);
	return entry !== undefined && entry[1][0] === 0;
}
function __nmap_insert(table, hash, key, value) {
	__nmap_settle(table);
	const entry = table[0].get(hash);
	if (entry === undefined) table[0].set(hash, [ key, [ 0, value ] ]);
	else {
		entry[0] = key;
		if (entry[1][0] !== 0 || entry[1][1] !== value) entry[1] = [ 0, value ];
	}
}
function __nmap_keys(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[0]);
	return out.map(__clone);
}
function __nmap_len(table) {
	const hash = table[1];
	if (hash !== undefined) {
		const entry = table[0].get(hash);
		if (entry !== undefined && entry[1][0] === 1) return table[0].size - 1;
	}
	return table[0].size;
}
function __nmap_remove(table, hash) {
	__nmap_settle(table);
	table[0].delete(hash);
}
function __nmap_settle(table) {
	const hash = table[1];
	if (hash === undefined) return;
	table[1] = undefined;
	const entry = table[0].get(hash);
	if (entry !== undefined && entry[1][0] === 1) table[0].delete(hash);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
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
function eq(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index = 0;
	for (const item of self) {
		if (!(item === __at(b, index, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index = index + 1;
	}
	return true;
}
function eq2(self, b) {
	return self === b;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function assert(condition, message, caller) {
	let $a = null;
	if (!(condition)) {
		$a = (() => {
			throw __panic(message, caller);
		})();
	}
	return $a;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $j = null;
	if (wrapped < 0) {
		$j = wrapped + modulus;
	} else {
		$j = wrapped;
	}
	return $j;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $l = null;
	if (truncated > 0) {
		$l = truncated;
	} else {
		$l = 0;
	}
	return $l;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $k = null;
	if (wrapped >= half) {
		$k = wrapped - modulus;
	} else {
		$k = wrapped;
	}
	return $k;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
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
function mint_subscriber(notify2) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify2, derived);
}
function subscriber_of(notify2, derived) {
	return [ fresh_id(), notify2, __shared_new(true), derived ];
}
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash2(subscriber[0]);
		let $ac = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ac = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$ac;
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
						if (tracing.v) {
							trace_hooks.v[4](turn, subscriber);
						} else if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
				for (const subscriber2 of wave) {
					if (tracing.v) {
						trace_hooks.v[4](turn, subscriber2);
					} else if (subscriber2[2].v) {
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
function queue_wakes(turn, subscribers, traced, cause) {
	if (traced) {
		waking.v = [ 0, cause ];
		enqueue(turn, subscribers);
		waking.v = [ 1 ];
	} else {
		enqueue(turn, subscribers);
	}
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $t = null;
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
		$t = undefined;
	}
	return $t;
}
function defer_to_owner(cleanup, $q) {
	const $r = $q;
	let $s = null;
	if ($r[0] === 0) {
		const owner = $r[1];
		$s = defer(owner, cleanup);
	} else {
		$s = undefined;
	}
	return $s;
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function settle_together(body, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const current = $ar[1];
		$as = body(current);
	} else {
		const $at = last(draining_turns.v);
		let $au = null;
		if ($at[0] === 0) {
			const draining = $at[1];
			$au = body(draining);
		} else {
			$au = batch(body, $aq);
		}
		$as = $au;
	}
	return $as;
}
function new3() {
	return [ new5(), [  ] ];
}
function contains(self, member) {
	return contains5(self[0], member);
}
function members(self) {
	return __clone(self[0]);
}
function add(self, member) {
	if (!(contains5(self[0], member))) {
		self[0].push(member);
	}
}
function remove(self, member) {
	let kept = [  ];
	for (const held of self[0]) {
		if (held !== member) {
			kept.push(held);
		}
	}
	self[0] = kept;
}
function describe(op) {
	const $aT = op;
	let $aU = null;
	if ($aT[0] === 0) {
		const member = $aT[1];
		$aU = "add " + member;
	} else if ($aT[0] === 1) {
		const member2 = $aT[1];
		$aU = "remove " + member2;
	} else {
		const all2 = $aT[1];
		$aU = "reset " + len(all2);
	}
	return $aU;
}
function described(ops) {
	return map(ops, (op) => {
		return describe(op);
	});
}
function new4() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function insert(self, value) {
	__nmap_insert(self[0], hash(value), __clone(value), undefined);
}
function remove2(self, value) {
	__nmap_remove(self[0], hash(value));
}
function contains2(self, value) {
	return __nmap_has(self[0], hash(value));
}
function len(self) {
	return __nmap_len(self[0]);
}
function new5() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function contains3(self, member) {
	return contains2(self, member);
}
function add2(self, member) {
	insert(self, member);
}
function new6(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new6(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function set(value) {
	return [ over(value, new3(), [ (held, member) => {
		return contains3(held, member);
	}, (held, member) => {
		let $b = null;
		if (contains3(held, member)) {
			$b = [ 0, __clone(member) ];
		} else {
			$b = [ 1 ];
		}
		return $b;
	}, (held, member, read) => {
		if (contains3(held, member)) {
			read([ [ member ], 0 ]);
		}
		return;
	}, (held, member, _value) => {
		return add2(held, member);
	}, (_held, _member, _with) => {
		(() => {
			throw __panic("lens: a set\'s member is its own address and cannot be edited in place", "std/src/reactive/lens.vl:2574:5");
		})();
		return;
	} ]) ];
}
function follows_items(self) {
	return false;
}
function lend(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend(cell.v, read);
}
function viewing2(cell, read) {
	return lend(cell.v, read);
}
function peek_storage(storage, read) {
	const $e = storage;
	let $f = null;
	if ($e[0] === 0) {
		const cell = $e[1];
		$f = viewing2(cell, read);
	} else {
		const port = $e[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $g = out;
		let $h = null;
		if ($g[0] === 0) {
			const value = __clone($g[1]);
			$h = value;
		} else {
			$h = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$f = $h;
	}
	return $f;
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function insert2(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function follow(self, at3) {
	const $i = get(self[0], at3);
	if ($i[0] === 0) {
		return $i[1];
	}
	const slot5 = as_i32(self[1].length);
	self[1].push(__clone(at3));
	insert2(self[0], at3, slot5);
	return slot5;
}
function follow_id(storage, watch, holds, at3) {
	if (viewing(watch, (held) => {
		return follows_items(held[0]);
	}) && !(peek_storage(storage, (held) => {
		return holds(held, at3);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at3);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $m = as_usize(id);
	__at(watch.v[1], $m, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $m, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $o = __clone(self);
	let $n = null;
	if (id < 0) {
		$n = -(1);
	} else {
		$n = id;
	}
	return [ $o, $n ];
}
function follow2(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at3));
}
function remove3(self, key) {
	__nmap_remove(self[0], hash(key));
}
function release(self, slot5) {
	const $p = __list_get(self[1], as_usize(slot5));
	if ($p[0] === 0) {
		remove3(self[0], $p[1]);
	}
}
function release_id(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1068:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1069:2")[2] = left;
	if (left <= 0) {
		release(watch.v[0], id);
	}
}
function release2(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at3, $d) {
	const handle = follow2(self, at3);
	defer_to_owner(() => {
		return release2(handle);
	}, $d);
	return handle;
}
function slot2(self, member, $c) {
	return [ slot(self[0], member, $c) ];
}
function locate(self, slot5) {
	return __list_get(self[1], as_usize(slot5));
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
	});
}
function peek_storage2(storage, read) {
	const $w = storage;
	let $x = null;
	if ($w[0] === 0) {
		const cell = $w[1];
		$x = viewing2(cell, read);
	} else {
		const port = $w[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $y = out;
		let $z = null;
		if ($y[0] === 0) {
			const value = __clone($y[1]);
			$z = value;
		} else {
			$z = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$x = $z;
	}
	return $x;
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get2(self) {
	const $v = at(self);
	if ($v[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held) => {
		return read(held, $v[1]);
	});
}
function get3(self) {
	const $A = get2(self[0]);
	return $A[0] === 0;
}
function slot_cell(watch, slot5) {
	if (slot5 < 0) {
		return new6(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot5);
	const $B = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell = __clone($B[1]);
	if ($B[0] === 0) {
		return cell;
	}
	const cell2 = new6(0, "std/src/reactive/lens.vl:1085:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1086:2")[0] = [ 0, __clone(cell2) ];
	return cell2;
}
function attach(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function on_settle(self, subscriber) {
	return attach(self, subscriber);
}
function on_settle2(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function on_settle3(self, subscriber) {
	return on_settle2(self[0], subscriber);
}
function attach_observer(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get3(self));
	}));
	if (immediately) {
		observer(get3(self));
	}
	return subscription;
}
function on_change(self, observer, caller) {
	return place_subscription(attach_observer(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function peek2(self, read) {
	return peek_storage(self[0].v[0], read);
}
function has(self, at3) {
	const holds = self[0].v[1][0];
	return peek2(self, (held) => {
		return holds(held, at3);
	});
}
function coming_one(self, change) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		tap2[1](change);
	}
}
function mutate(self, change) {
	const $F = self[0].v[0];
	let $G = null;
	if ($F[0] === 0) {
		const cell = $F[1];
		$G = change(cell.v);
	} else {
		const port = __clone($F[1]);
		$G = port[1](change);
	}
	return $G;
}
function apply(self, change, wakes) {
	const $L = change;
	let $M = null;
	if ($L[0] === 0) {
		const key = $L[1];
		const _removed = $L[2];
		const _inserted = $L[3];
		const $N = get(self[0], key);
		if ($N[0] === 0) {
			wakes.push([ 0, $N[1] ]);
		}
		$M = undefined;
	} else if ($L[0] === 1) {
		const key2 = $L[1];
		const _count = $L[2];
		const $O = get(self[0], key2);
		if ($O[0] === 0) {
			wakes.push([ 0, $O[1] ]);
		}
		$M = undefined;
	} else if ($L[0] === 2) {
		const key3 = $L[1];
		const $P = get(self[0], key3);
		if ($P[0] === 0) {
			wakes.push([ 0, $P[1] ]);
		}
		$M = undefined;
	} else if ($L[0] === 3) {
		const key4 = $L[1];
		const _count2 = $L[2];
		const $Q = get(self[0], key4);
		if ($Q[0] === 0) {
			wakes.push([ 0, $Q[1] ]);
		}
		$M = undefined;
	} else if ($L[0] === 4) {
		const key5 = $L[1];
		const part = $L[2];
		const $R = get(self[0], key5);
		if ($R[0] === 0) {
			wakes.push([ 1, $R[1], part ]);
		}
		$M = undefined;
	} else if ($L[0] === 5) {
		const _from = $L[1];
		const _to = $L[2];
		$M = undefined;
	} else {
		let slot5 = 0;
		const held = as_i32(self[1].length);
		while (slot5 < held) {
			wakes.push([ 0, slot5 ]);
			slot5 = slot5 + 1;
		}
		$M = undefined;
	}
	return $M;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $ad = null;
	if (is_empty(self)) {
		$ad = [ 1 ];
	} else {
		$ad = __list_get(self, self.length - 1);
	}
	return $ad;
}
function notify(self, $Y) {
	const traced = tracing.v;
	let $Z = null;
	if (traced) {
		$Z = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$Z = "";
	}
	const cause = $Z;
	const $aa = $Y;
	let $ab = null;
	if ($aa[0] === 0) {
		const turn = $aa[1];
		$ab = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $ae = last(draining_turns.v);
		let $af = null;
		if ($ae[0] === 0) {
			const draining = $ae[1];
			$af = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ag = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ag = undefined;
				}
				$ag;
			}
			$af = undefined;
		}
		$ab = $af;
	}
	return $ab;
}
function wake_all(watch, slot5, $ak) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot5), "std/src/reactive/lens.vl:1117:36"));
	});
	const $al = held[0];
	if ($al[0] === 0) {
		notify($al[1], $ak);
	}
	for (const part of held[1]) {
		notify(part[1], $ak);
	}
}
function wake_part(watch, slot5, name, $am) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot5), "std/src/reactive/lens.vl:1127:36"));
	});
	const $an = held[0];
	if ($an[0] === 0) {
		notify($an[1], $am);
	}
	for (const part of held[1]) {
		if (part[0] === name) {
			notify(part[1], $am);
		}
	}
}
function batch(body, $av) {
	const $aw = $av;
	let $ax = null;
	if ($aw[0] === 0) {
		const current = $aw[1];
		$ax = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$ax = result;
	}
	return $ax;
}
function landed(self, changes, late, $I) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change of changes) {
		const $J = change;
		let $K = null;
		if ($J[0] === 0) {
			const _at = $J[1];
			const removed = $J[2];
			const inserted = $J[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$K = undefined;
		} else if ($J[0] === 1) {
			const _at2 = $J[1];
			const _count = $J[2];
			$K = resized = true;
		} else if ($J[0] === 3) {
			const _at3 = $J[1];
			const _count2 = $J[2];
			$K = resized = true;
		} else if ($J[0] === 6) {
			$K = resized = true;
		} else if ($J[0] === 5) {
			const _from = $J[1];
			const _to = $J[2];
			$K = shaped = true;
		} else {
			$K = undefined;
		}
		$K;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change, wakes);
		}
		let $T = null;
		if (self[0].v[4].length > 0) {
			let $S = null;
			if (!(late)) {
				$S = [ 0 ];
			} else if (!(several)) {
				$S = [ 1 ];
			} else if (index === 0) {
				$S = [ 2 ];
			} else {
				$S = [ 3 ];
			}
			const stage = $S;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change, stage);
			}
			$T = undefined;
		}
		$T;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $U = null;
	if (shaped || resized) {
		$U = watch.v[2];
	} else {
		$U = [ 1 ];
	}
	const shape = __clone($U);
	let $V = null;
	if (resized) {
		$V = watch.v[3];
	} else {
		$V = [ 1 ];
	}
	const sized = __clone($V);
	let $W = undefined;
	let $X = undefined;
	if (wakes.length === 0 && (($W = shape), $W[0] === 1) && (($X = sized), $X[0] === 1)) {
		notify(published, $I);
		return;
	}
	settle_together(($ah) => {
		for (const wake of wakes) {
			const $ai = wake;
			let $aj = null;
			if ($ai[0] === 0) {
				const slot5 = $ai[1];
				$aj = wake_all(watch, slot5, [ 0, $ah ]);
			} else if ($ai[0] === 2) {
				const slot6 = $ai[1];
				$aj = wake_all(watch, slot6, [ 0, $ah ]);
			} else {
				const slot7 = $ai[1];
				const part = $ai[2];
				$aj = wake_part(watch, slot7, part, [ 0, $ah ]);
			}
			$aj;
		}
		const $ao = sized;
		if ($ao[0] === 0) {
			notify($ao[1], [ 0, $ah ]);
		}
		const $ap = shape;
		if ($ap[0] === 0) {
			notify($ap[1], [ 0, $ah ]);
		}
		notify(published, [ 0, $ah ]);
		return;
	}, $I);
}
function landed_one(self, change, $H) {
	landed(self, [ __clone(change) ], false, $H);
}
function add3(self, member, $E) {
	if (has(self[0], member)) {
		return false;
	}
	const change = [ 1, __clone(member), 1 ];
	coming_one(self[0], change);
	mutate(self[0], (held) => {
		return add2(held, member);
	});
	landed_one(self[0], change, $E);
	return true;
}
function remove4(self, member) {
	remove2(self, member);
}
function remove5(self, member, $ay) {
	if (!(has(self[0], member))) {
		return false;
	}
	const change = [ 3, __clone(member), 1 ];
	coming_one(self[0], change);
	mutate(self[0], (held) => {
		return remove4(held, member);
	});
	landed_one(self[0], change, $ay);
	return true;
}
function contains4(self, member) {
	return has(self[0], member);
}
function length(self) {
	return [ __clone(self[0]) ];
}
function count(self) {
	return len(self);
}
function peek3(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get4(self) {
	return peek3(self[0], (held) => {
		return count(held);
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $aF = watch.v[3];
	const cell = __clone($aF[1]);
	if ($aF[0] === 0) {
		return cell;
	}
	const cell2 = new6(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle4(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
}
function attach_observer2(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
	}
	return subscription;
}
function on_change2(self, observer, caller) {
	return place_subscription(attach_observer2(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function new7() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor4 of self[3].v) {
		const at3 = cursor4[1].v;
		if (at3 < lowest) {
			lowest = at3;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index = as_usize(dropped);
		const held = __clone(self[0].v);
		const length2 = held.length;
		while (index < length2) {
			const $aJ = __list_get(held, index);
			let $aK = null;
			if ($aJ[0] === 0) {
				const op = $aJ[1];
				$aK = kept.push(op);
			} else {
				$aK = undefined;
			}
			$aK;
			index = index + 1;
		}
		self[0].v = kept;
		self[2].v = lowest;
	}
}
function record(self, op) {
	trim(self);
	const next2 = self[1].v + 1;
	self[1].v = next2;
	if (self[0].v.length >= self[4]) {
		self[0].v = [  ];
		self[2].v = next2;
	} else {
		self[0].v.push(__clone(op));
	}
}
function values(self) {
	return __nmap_keys(self[0]);
}
function members2(self) {
	return values(self);
}
function to_set(self) {
	let result = new4();
	for (const value of self) {
		insert(result, value);
	}
	return result;
}
function members_of(storage) {
	return to_set(peek_storage2(storage, (held) => {
		return members2(held);
	}));
}
function tap(self, before, after2) {
	const id = self[0].v[5];
	self[0].v[5] = id + 1;
	self[0].v[4].push([ id, before, after2 ]);
	return id;
}
function untap(self, id) {
	let kept = [  ];
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		if (tap2[0] !== id) {
			kept.push(__clone(tap2));
		}
	}
	self[0].v[4] = kept;
}
function release3(self) {
	untap(self[0], self[2]);
}
function members3(self, $aG) {
	const log = new7();
	const storage = __clone(self[0][0].v[0]);
	const tap2 = tap(self[0], (_change) => {
		return;
	}, (change, _stage) => {
		const $aH = change;
		let $aI = null;
		if ($aH[0] === 1) {
			const member = $aH[1];
			const _count = $aH[2];
			$aI = record(log, [ 0, __clone(member) ]);
		} else if ($aH[0] === 3) {
			const member2 = $aH[1];
			const _count2 = $aH[2];
			$aI = record(log, [ 1, __clone(member2) ]);
		} else if ($aH[0] === 0) {
			const member3 = $aH[1];
			const removed = $aH[2];
			const inserted = $aH[3];
			if (inserted > removed) {
				record(log, [ 0, __clone(member3) ]);
			} else if (removed > inserted) {
				record(log, [ 1, __clone(member3) ]);
			}
			$aI = undefined;
		} else if ($aH[0] === 2) {
			const _member = $aH[1];
			$aI = undefined;
		} else if ($aH[0] === 4) {
			const _member2 = $aH[1];
			const _part = $aH[2];
			$aI = undefined;
		} else if ($aH[0] === 5) {
			const _from = $aH[1];
			const _to = $aH[2];
			$aI = undefined;
		} else {
			$aI = record(log, [ 2, members_of(storage) ]);
		}
		return $aI;
	});
	const feed2 = [ __clone(self[0]), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release3(feed2);
	}, $aG);
	return feed2;
}
function cursor(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function cursor2(self) {
	return cursor(self[1]);
}
function since(self, cursor4) {
	const at3 = cursor4[1].v;
	const version = self[1].v;
	let ops = [  ];
	if (at3 >= version) {
		return [ 0, ops ];
	}
	cursor4[1].v = version;
	const base = self[2].v;
	if (at3 < base) {
		return [ 1 ];
	}
	let index = as_usize(at3 - base);
	const held = __clone(self[0].v);
	const length2 = held.length;
	while (index < length2) {
		const $aP = __list_get(held, index);
		let $aQ = null;
		if ($aP[0] === 0) {
			const op = $aP[1];
			$aQ = ops.push(op);
		} else {
			$aQ = undefined;
		}
		$aQ;
		index = index + 1;
	}
	return [ 0, ops ];
}
function since2(self, cursor4) {
	const $aR = since(self[1], cursor4);
	let $aS = null;
	if ($aR[0] === 0) {
		const recorded = $aR[1];
		$aS = recorded;
	} else {
		$aS = [ [ 2, members_of(self[0][0].v[0]) ] ];
	}
	return $aS;
}
function map(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
function audit(self, before, changes) {
	const $bf = self[0].v[6];
	const check = $bf[1];
	if ($bf[0] === 0) {

	} else {
		return;
	}
	const $bg = before;
	if ($bg[0] === 0) {

	} else {
		return;
	}
	const problem = peek(self, (held) => {
		return check($bg[1], held, changes);
	});
	const $bh = problem;
	if ($bh[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $bh[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function update(self, change, $aW) {
	const $aX = self[0].v[6];
	const _check = $aX[1];
	let $bc = null;
	if ($aX[0] === 0) {
		$bc = [ 0, peek3(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$bc = [ 1 ];
	}
	const before = $bc;
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, change(held) ];
	});
	const $bd = out;
	let $be = null;
	if ($bd[0] === 0) {
		const result = __clone($bd[1][0]);
		const changes = __clone($bd[1][1]);
		audit(self, before, changes);
		landed(self, changes, true, $aW);
		$be = result;
	} else {
		$be = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $be;
}
function update2(self, change, $aV) {
	return update(self[0], change, $aV);
}
function get5(self) {
	return members_of(self[0][0].v[0]);
}
function all(self) {
	return to_set(peek3(self[0], (held) => {
		return members2(held);
	}));
}
function eq3(self, other) {
	if (__nmap_len(self[0]) !== __nmap_len(other[0])) {
		return false;
	}
	for (const value of __nmap_keys(self[0])) {
		if (!(__nmap_has(other[0], hash(value)))) {
			return false;
		}
	}
	return true;
}
function reader(self) {
	return (cursor4) => {
		return since2(self, cursor4);
	};
}
function on_settle5(self, subscriber) {
	return on_settle(self[0][0].v[2], subscriber);
}
function drop_cursor(self, cursor4) {
	let kept = [  ];
	for (const held of self[3].v) {
		if (held[0] !== cursor4[0]) {
			kept.push(__clone(held));
		}
	}
	self[3].v = kept;
}
function drop_cursor2(self, cursor4) {
	drop_cursor(self[1], cursor4);
}
function open(self) {
	const cursor4 = cursor2(self);
	const read = reader(self);
	return [ () => {
		return get5(self);
	}, (subscriber) => {
		return on_settle5(self, subscriber);
	}, () => {
		return read(cursor4);
	}, () => {
		return drop_cursor2(self, cursor4);
	} ];
}
function get6(self) {
	return peek3(self, (held) => {
		return __clone(held);
	});
}
function get7(self) {
	return get6(self[0]);
}
function on_settle6(self, subscriber) {
	return on_settle(self[0].v[2], subscriber);
}
function on_settle7(self, subscriber) {
	return on_settle6(self[0], subscriber);
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle7(self, mint_subscriber(() => {
		return observer(get7(self));
	}));
	if (immediately) {
		observer(get7(self));
	}
	return subscription;
}
function on_change3(self, observer, caller) {
	return place_subscription(attach_observer3(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function peek4(self, read) {
	return peek3(self[0], read);
}
function reconcile_set(self, next2, $bk) {
	const lens2 = __clone(self);
	batch(($bl) => {
		for (const member of peek4(lens2, (held) => {
			return members2(held);
		})) {
			if (!(contains2(next2, member))) {
				remove5(lens2, member, [ 0, $bl ]);
			}
		}
		for (const member2 of values(next2)) {
			add3(lens2, member2, [ 0, $bl ]);
		}
		return;
	}, $bk);
}
function lens(self) {
	return __clone(self[0]);
}
function tapped(self) {
	return self[0].v[4].length;
}
function contains5(self, value) {
	for (const item of self) {
		if (eq2(item, value)) {
			return true;
		}
	}
	return false;
}
function lens_over2(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new6(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over2(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function set2(value) {
	return [ over2(value, new3(), [ (held, member) => {
		return contains(held, member);
	}, (held, member) => {
		let $bm = null;
		if (contains(held, member)) {
			$bm = [ 0, __clone(member) ];
		} else {
			$bm = [ 1 ];
		}
		return $bm;
	}, (held, member, read) => {
		if (contains(held, member)) {
			read([ [ member ], 0 ]);
		}
		return;
	}, (held, member, _value) => {
		return add(held, member);
	}, (_held, _member, _with) => {
		(() => {
			throw __panic("lens: a set\'s member is its own address and cannot be edited in place", "std/src/reactive/lens.vl:2574:5");
		})();
		return;
	} ]) ];
}
function over3(value) {
	return set2(value);
}
function follow_id2(storage, watch, holds, at3) {
	if (viewing(watch, (held) => {
		return follows_items(held[0]);
	}) && !(peek_storage2(storage, (held) => {
		return holds(held, at3);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at3);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $br = as_usize(id);
	__at(watch.v[1], $br, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $br, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function follow3(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at3));
}
function release4(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot3(self, at3, $d) {
	const handle = follow3(self, at3);
	defer_to_owner(() => {
		return release4(handle);
	}, $d);
	return handle;
}
function slot4(self, member, $c) {
	return [ slot3(self[0], member, $c) ];
}
function has2(self, at3) {
	const holds = self[0].v[1][0];
	return peek3(self, (held) => {
		return holds(held, at3);
	});
}
function landed2(self, changes, late, $I) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change of changes) {
		const $bw = change;
		let $bx = null;
		if ($bw[0] === 0) {
			const _at = $bw[1];
			const removed = $bw[2];
			const inserted = $bw[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bx = undefined;
		} else if ($bw[0] === 1) {
			const _at2 = $bw[1];
			const _count = $bw[2];
			$bx = resized = true;
		} else if ($bw[0] === 3) {
			const _at3 = $bw[1];
			const _count2 = $bw[2];
			$bx = resized = true;
		} else if ($bw[0] === 6) {
			$bx = resized = true;
		} else if ($bw[0] === 5) {
			const _from = $bw[1];
			const _to = $bw[2];
			$bx = shaped = true;
		} else {
			$bx = undefined;
		}
		$bx;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change, wakes);
		}
		let $bz = null;
		if (self[0].v[4].length > 0) {
			let $by = null;
			if (!(late)) {
				$by = [ 0 ];
			} else if (!(several)) {
				$by = [ 1 ];
			} else if (index === 0) {
				$by = [ 2 ];
			} else {
				$by = [ 3 ];
			}
			const stage = $by;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change, stage);
			}
			$bz = undefined;
		}
		$bz;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bA = null;
	if (shaped || resized) {
		$bA = watch.v[2];
	} else {
		$bA = [ 1 ];
	}
	const shape = __clone($bA);
	let $bB = null;
	if (resized) {
		$bB = watch.v[3];
	} else {
		$bB = [ 1 ];
	}
	const sized = __clone($bB);
	let $bC = undefined;
	let $bD = undefined;
	if (wakes.length === 0 && (($bC = shape), $bC[0] === 1) && (($bD = sized), $bD[0] === 1)) {
		notify(published, $I);
		return;
	}
	settle_together(($ah) => {
		for (const wake of wakes) {
			const $bE = wake;
			let $bF = null;
			if ($bE[0] === 0) {
				const slot5 = $bE[1];
				$bF = wake_all(watch, slot5, [ 0, $ah ]);
			} else if ($bE[0] === 2) {
				const slot6 = $bE[1];
				$bF = wake_all(watch, slot6, [ 0, $ah ]);
			} else {
				const slot7 = $bE[1];
				const part = $bE[2];
				$bF = wake_part(watch, slot7, part, [ 0, $ah ]);
			}
			$bF;
		}
		const $bG = sized;
		if ($bG[0] === 0) {
			notify($bG[1], [ 0, $ah ]);
		}
		const $bH = shape;
		if ($bH[0] === 0) {
			notify($bH[1], [ 0, $ah ]);
		}
		notify(published, [ 0, $ah ]);
		return;
	}, $I);
}
function landed_one2(self, change, $H) {
	landed2(self, [ __clone(change) ], false, $H);
}
function add4(self, member, $E) {
	if (has2(self[0], member)) {
		return false;
	}
	const change = [ 1, __clone(member), 1 ];
	coming_one(self[0], change);
	mutate(self[0], (held) => {
		return add(held, member);
	});
	landed_one2(self[0], change, $E);
	return true;
}
function at2(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
	});
}
function get8(self) {
	const $bI = at2(self);
	if ($bI[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek3(self[0], (held) => {
		return read(held, $bI[1]);
	});
}
function get9(self) {
	const $bN = get8(self[0]);
	return $bN[0] === 0;
}
function count2(self) {
	return members(self).length;
}
function get10(self) {
	return peek3(self[0], (held) => {
		return count2(held);
	});
}
function remove6(self, member, $ay) {
	if (!(has2(self[0], member))) {
		return false;
	}
	const change = [ 3, __clone(member), 1 ];
	coming_one(self[0], change);
	mutate(self[0], (held) => {
		return remove(held, member);
	});
	landed_one2(self[0], change, $ay);
	return true;
}
function reconcile_set2(self, next2, $bk) {
	const lens2 = __clone(self);
	batch(($bl) => {
		for (const member of peek4(lens2, (held) => {
			return members(held);
		})) {
			if (!(contains2(next2, member))) {
				remove6(lens2, member, [ 0, $bl ]);
			}
		}
		for (const member2 of values(next2)) {
			add4(lens2, member2, [ 0, $bl ]);
		}
		return;
	}, $bk);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
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
}, (_id, _site) => {
	return;
} ]);
const waking = __shared_new([ 1 ]);
const owner_lists_allocated_count = __shared_new(0);
const delta_log_limit = 1024;
let plain = new4();
insert(plain, "a");
remove2(plain, "a");
assert(!(contains2(plain, "a")) && len(plain) === 0, "HashSet\'s own members still answer", "lens-set.vl:60:2");
const tags = set(plain);
const urgent = slot2(tags, "urgent", [ 1 ]);
const later = slot2(tags, "later", [ 1 ]);
const urgent_seen = __shared_new([  ]);
const later_runs = __shared_new(0);
on_change(__clone(urgent), (on, $u) => {
	return urgent_seen.v.push(on);
}, "lens-set.vl:68:18");
on_change(__clone(later), (_on, $C) => {
	const $D = [ later_runs, "v" ];
	return later_runs.v = $D[0][$D[1]] + 1;
}, "lens-set.vl:69:17");
assert(!(get3(urgent)), "absent before it joins", "lens-set.vl:70:2");
assert(add3(tags, "urgent", [ 1 ]), "a new member is added", "lens-set.vl:71:2");
assert(!(add3(tags, "urgent", [ 1 ])), "a held member writes nothing", "lens-set.vl:72:2");
assert(add3(tags, "other", [ 1 ]), "another member", "lens-set.vl:73:2");
assert(remove5(tags, "urgent", [ 1 ]), "a held member leaves", "lens-set.vl:74:2");
assert(!(remove5(tags, "urgent", [ 1 ])), "an absent member writes nothing", "lens-set.vl:75:2");
assert(add3(tags, "urgent", [ 1 ]), "it returns", "lens-set.vl:76:2");
assert(eq(urgent_seen.v, [ true, false, true ]), "the slot heard join, leave and return, and no repeat", "lens-set.vl:77:2");
assert(later_runs.v === 0, "another member\'s slot slept through all of it", "lens-set.vl:81:2");
assert(get3(urgent) && contains4(tags, "other") && !(contains4(tags, "later")), "presence reads", "lens-set.vl:82:2");
const lengths = __shared_new(0);
on_change2(length(tags), (_n, $az) => {
	const $aA = [ lengths, "v" ];
	return lengths.v = $aA[0][$aA[1]] + 1;
}, "lens-set.vl:86:25");
add3(tags, "other", [ 1 ]);
assert(lengths.v === 0, "a repeat leaves the length asleep", "lens-set.vl:88:2");
add3(tags, "later", [ 1 ]);
assert(lengths.v === 1 && get4(length(tags)) === 3, "a join wakes it", "lens-set.vl:90:2");
assert(later_runs.v === 1 && get3(later), "and wakes that member\'s slot", "lens-set.vl:91:2");
const feed = members3(tags, [ 1 ]);
const cursor3 = cursor2(feed);
remove5(tags, "other", [ 1 ]);
add3(tags, "new", [ 1 ]);
assert(eq(described(since2(feed, cursor3)), [ "remove other", "add new" ]), "one op per change, in order", "lens-set.vl:98:2");
const after = cursor2(feed);
update2(tags, (held) => {
	add2(held, "x");
	remove2(held, "x");
	add2(held, "y");
	return [ undefined, [ [ 1, "x", 1 ], [ 3, "x", 1 ], [ 1, "y", 1 ] ] ];
}, [ 1 ]);
assert(eq(described(since2(feed, after)), [ "add x", "remove x", "add y" ]), "an update\'s changes, replayed", "lens-set.vl:113:2");
const reset_at = cursor2(feed);
update2(tags, (_held) => {
	return [ undefined, [ [ 6 ] ] ];
}, [ 1 ]);
assert(eq(described(since2(feed, reset_at)), [ "reset 4" ]), "a reset carries the whole set", "lens-set.vl:119:2");
assert(eq3(get5(feed), all(tags)) && len(all(tags)) === 4, "the feed reads the set", "lens-set.vl:120:2");
const flow_feed = members3(tags, [ 1 ]);
const flow = open(__clone(flow_feed));
assert(len(flow[0]()) === 4, "the flow\'s snapshot is the set", "lens-set.vl:125:2");
remove5(tags, "y", [ 1 ]);
assert(eq(described(flow[2]()), [ "remove y" ]), "the flow drains one op", "lens-set.vl:127:2");
flow[3]();
release3(flow_feed);
const whole_runs = __shared_new(0);
on_change3(__clone(tags), (_all, $bi) => {
	const $bj = [ whole_runs, "v" ];
	return whole_runs.v = $bj[0][$bj[1]] + 1;
}, "lens-set.vl:133:16");
const urgent_before = urgent_seen.v.length;
const changed = cursor2(feed);
let next = new4();
insert(next, "urgent");
insert(next, "fresh");
reconcile_set(tags, next, [ 1 ]);
assert(eq3(all(tags), next), "the set became the new set", "lens-set.vl:140:2");
assert(whole_runs.v === 1, "in one turn", "lens-set.vl:141:2");
assert(urgent_seen.v.length === urgent_before, "a member both hold did not wake", "lens-set.vl:142:2");
assert(later_runs.v === 3 && !(get3(later)), "a member that left woke once", "lens-set.vl:144:2");
assert(described(since2(feed, changed)).length === 3, "two left and one joined", "lens-set.vl:145:2");
release3(feed);
assert(tapped(lens(tags)) === 0, "released, the feed let its tap go", "lens-set.vl:147:2");
const custom = over3([ [ "b" ] ]);
const a = slot4(custom, "a", [ 1 ]);
add4(custom, "a", [ 1 ]);
assert(get9(a) && get10(length(custom)) === 2, "a second set shape joins the same way", "lens-set.vl:153:2");
reconcile_set2(custom, to_set([ "c" ]), [ 1 ]);
assert(!(get9(a)) && eq(peek4(custom, (held) => {
	return __clone(held[0]);
}), [ "c" ]), "and reconciles", "lens-set.vl:155:2");
console.log("set: all passed");
//# sourceMappingURL=lens-set.mjs.map
