function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_view(list, index, location) {
	if (index >= 0 && index < list.length) return [ list, index ];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __guarded(body) {
	try {
		body();
		return [ 1 ];
	} catch (error) {
		return [ 0, error && error.message ? error.message : String(error) ];
	}
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
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function __map_values(map) {
	return [ ...map.values() ].map(__clone);
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
function __nmap_values(table) {
	const out = [];
	for (const entry of table[0].values()) if (entry[1][0] === 0) out.push(entry[1][1]);
	return out.map(__clone);
}
class __Nursery {
	constructor(parent) {
		this.children = [];
		this.failedTask = undefined;
		this.failWake = undefined;
		this.controller = new AbortController();
		if (parent) {
			const signal = parent.controller.signal;
			if (signal.aborted) this.controller.abort(signal.reason);
			else signal.addEventListener("abort", () => this.controller.abort(signal.reason), { once: true });
		}
	}
	cancel() {
		this.controller.abort();
	}
	__fail(task) {
		if (this.failedTask === undefined) {
			this.failedTask = task;
			this.controller.abort();
			if (this.failWake) this.failWake();
		}
	}
	is_cancelled() {
		return this.controller.signal.aborted;
	}
	signal_of() {
		return this.controller.signal;
	}
}
function __nursery_new(parent) {
	return new __Nursery(parent && parent[0] === 0 ? parent[1] : undefined);
}
function __nursery_new_detached() {
	const n = __nursery_new(undefined);
	n.detached = true;
	n.__fail = function (task) {
		if (!task.observed) {
			globalThis.setTimeout(() => {
				if (!task.observed) console.error("unhandled task error (spawned in " + task.origin + "): " + String(task.error));
			}, 0);
		}
	};
	return n;
}
function __nursery_is_cancel(error) {
	return !!error && error.name === "AbortError";
}
async function __nursery_run(n, body) {
	let result;
	let bodyError;
	let bodyFailed = false;
	try {
		result = await body();
	} catch (error) {
		bodyFailed = true;
		bodyError = error;
	}
	if (bodyFailed) n.controller.abort();
	const failed = new Promise((resolve) => {
		n.failWake = resolve;
		if (n.failedTask !== undefined) resolve();
	});
	let index = 0;
	while (!bodyFailed && n.failedTask === undefined && index < n.children.length) {
		try {
			await Promise.race([n.children[index], failed]);
		} catch (error) {}
		if (n.failedTask === undefined) index += 1;
	}
	if (!bodyFailed && n.failedTask === undefined) return result;
	for (const task of n.children) task.then(null, () => {});
	if (bodyFailed) throw bodyError;
	const winner = n.failedTask;
	const failure = winner.error;
	if (typeof failure === "string") throw failure + " (in task spawned in " + winner.origin + ")";
	if (failure && failure.location !== undefined) throw __panic(failure.message + " (in task spawned in " + winner.origin + ")", failure.location);
	throw failure;
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
function eq(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b, index, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index = index + 1;
	}
	return true;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function assert(condition, message, caller) {
	let $dg = null;
	if (!(condition)) {
		$dg = (() => {
			throw __panic(message, caller);
		})();
	}
	return $dg;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $v = null;
	if (wrapped < 0) {
		$v = wrapped + modulus;
	} else {
		$v = wrapped;
	}
	return $v;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $D = null;
	if (truncated > 0) {
		$D = truncated;
	} else {
		$D = 0;
	}
	return $D;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $w = null;
	if (wrapped >= half) {
		$w = wrapped - modulus;
	} else {
		$w = wrapped;
	}
	return $w;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function eq2(self, b) {
	const $db = self;
	let $de = null;
	if ($db[0] === 0) {
		const $dc = b;
		let $dd = null;
		if ($dc[0] === 0) {
			$dd = $db[1] === $dc[1];
		} else {
			$dd = false;
		}
		$de = $dd;
	} else {
		const $df = b;
		$de = $df[0] === 1;
	}
	return $de;
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
		let $I = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$I = undefined;
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
		$I;
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
function defer_subscriber(turn, subscriber) {
	const $aZ = turn;
	let $ba = null;
	if ($aZ[0] === 0) {
		const ambient = $aZ[1];
		$ba = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bb = last(draining_turns.v);
		let $bc = null;
		if ($bb[0] === 0) {
			const draining = $bb[1];
			$bc = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bc = undefined;
		}
		$ba = $bc;
	}
	return $ba;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	if (tracing.v && trace_hooks.v[6](subscriber, last(draining_turns.v))) {
		return;
	}
	defer_subscriber([ 1 ], subscriber);
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
function dispose(self, $cq) {
	const $cr = $cq;
	let $cs = null;
	if ($cr[0] === 0) {
		const established = $cr[1];
		$cs = [ 0, established ];
	} else {
		$cs = last(draining_turns.v);
	}
	const ambient = $cs;
	release_under(self, ambient);
}
function detach(handle) {
	const $bn = last(releasing_turns.v);
	let $bo = null;
	if ($bn[0] === 0) {
		const at_release = $bn[1];
		$bo = at_release;
	} else {
		$bo = last(draining_turns.v);
	}
	const turn = $bo;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $bp = [ 0, handle[0] ];
	let $bq = null;
	if ($bp[0] === 0) {
		const subscribers = $bp[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$bq = undefined;
	} else {
		$bq = undefined;
	}
	$bq;
	const $br = ambient;
	let $bs = null;
	if ($br[0] === 0) {
		const turn = $br[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash2(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash2(handle[1]));
		if (tracing.v) {
			trace_hooks.v[7](turn, handle[1]);
		}
		$bs = undefined;
	} else {
		$bs = undefined;
	}
	$bs;
	const $bt = handle[3].v;
	let $bu = null;
	if ($bt[0] === 0) {
		const release5 = $bt[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release5, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$bu = undefined;
	} else {
		$bu = undefined;
	}
	return $bu;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $av = null;
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
		$av = undefined;
	}
	return $av;
}
function stage_nursery(self) {
	const $aH = nursery(self);
	let $aI = null;
	if ($aH[0] === 0) {
		const held = $aH[1];
		$aI = held;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$aI = made;
	}
	return $aI;
}
function split_registrations(self) {
	if (!(self[0].v[2]) || is_disposed(self)) {
		return [ 1 ];
	}
	const cleanups = self[0].v[1];
	self[0].v[1] = no_cleanups;
	self[0].v[2] = false;
	return [ 0, [ __shared_new([ 0, cleanups, true, [ 1 ] ]), 0 ] ];
}
function nursery(self) {
	let $aG = null;
	if (is_disposed(self)) {
		$aG = [ 1 ];
	} else {
		$aG = self[0].v[3];
	}
	return $aG;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $bG = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $bx = held[3];
		let $by = null;
		if ($bx[0] === 0) {
			const nursery2 = $bx[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$by = undefined;
		} else {
			$by = undefined;
		}
		$by;
		let $bF = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $bA = __guarded(cleanup);
				let $bB = null;
				if ($bA[0] === 0) {
					const message = $bA[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$bB = undefined;
				} else {
					$bB = undefined;
				}
				$bB;
			}
			const $bD = failure;
			let $bE = null;
			if ($bD[0] === 0) {
				const message2 = $bD[1];
				$bE = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$bE = undefined;
			}
			$bF = $bE;
		}
		$bG = $bF;
	}
	return $bG;
}
function register_with_owner(subscription, $cl, $cm) {
	const $cn = $cm;
	let $co = null;
	if ($cn[0] === 0) {
		const owner = $cn[1];
		$co = take(owner, subscription, $cl);
	} else {
		$co = __clone(subscription);
	}
	return $co;
}
function defer_to_owner(cleanup, $as) {
	const $at = $as;
	let $au = null;
	if ($at[0] === 0) {
		const owner = $at[1];
		$au = defer(owner, cleanup);
	} else {
		$au = undefined;
	}
	return $au;
}
function place_subscriber(id, site) {
	if (tracing.v) {
		trace_hooks.v[8](id, site);
	}
}
function place_subscription(subscription, site) {
	if (tracing.v) {
		trace_hooks.v[8](subscription[1], site);
	}
	return subscription;
}
function also_releasing(handle, release5) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release5();
		const $cj = previous;
		let $ck = null;
		if ($cj[0] === 0) {
			const earlier = $cj[1];
			$ck = earlier();
		} else {
			$ck = undefined;
		}
		return $ck;
	} ];
}
function settle_together(body, $O) {
	const $P = $O;
	let $Q = null;
	if ($P[0] === 0) {
		const current = $P[1];
		$Q = body(current);
	} else {
		const $R = last(draining_turns.v);
		let $S = null;
		if ($R[0] === 0) {
			const draining = $R[1];
			$S = body(draining);
		} else {
			$S = batch(body, $O);
		}
		$Q = $S;
	}
	return $Q;
}
function splice_start(held, at3) {
	let $eP = null;
	if (at3 > held) {
		$eP = held;
	} else {
		$eP = at3;
	}
	return $eP;
}
function splice_count(held, start, removed) {
	let $eQ = null;
	if (start + removed > held) {
		$eQ = held - start;
	} else {
		$eQ = removed;
	}
	return $eQ;
}
function new4() {
	const table = new Map();
	const identities = new Map();
	return [ __shared_new(table), __shared_new(identities) ];
}
function wake2(self, hash3, $ce) {
	const $cf = __map_get(self[0].v, hash3);
	let $cg = null;
	if ($cf[0] === 0) {
		const slot2 = $cf[1];
		$cg = notify(slot2[0], $ce);
	} else {
		$cg = undefined;
	}
	return $cg;
}
function wake_all(self, $cd) {
	for (const slot2 of __map_values(self[0].v)) {
		notify(slot2[0], $cd);
	}
}
function new5() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at3) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:157:7") <= at3) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at3 - __at(self[3], low, "std/src/reactive/lens.vl:163:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:173:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:174:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:183:14").length;
	while (index < held) {
		const slot2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:185:15"), index, "std/src/reactive/lens.vl:185:15");
		if (slot2 !== -(1)) {
			__at_put(self[4], as_usize(slot2), block, "std/src/reactive/lens.vl:187:5");
			__at_put(self[5], as_usize(slot2), index, "std/src/reactive/lens.vl:188:5");
		}
		index = index + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:199:14");
	return block;
}
function widen(self, at3, count2) {
	if (count2 === 0 || at3 >= self[6]) {
		return;
	}
	const $fM = seat(self, at3);
	const place = $fM[0];
	const index = $fM[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:209:15");
	let $fN = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:213:5"), index, -(1), "std/src/reactive/lens.vl:213:35");
			made = made + 1;
		}
		$fN = undefined;
	} else {
		const before2 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:217:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:221:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before2.length;
		while (copied < held) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:231:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:234:4");
	}
	$fN;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at3, count2, wakes) {
	let left = count2;
	while (left > 0 && at3 < self[6]) {
		const $fK = seat(self, at3);
		const place = $fK[0];
		const index = $fK[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:250:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:251:15").length;
		let $fL = null;
		if (left < held - index) {
			$fL = left;
		} else {
			$fL = held - index;
		}
		const take2 = $fL;
		let taken = 0;
		while (taken < take2) {
			const slot2 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:255:16"), index, "std/src/reactive/lens.vl:255:46");
			if (slot2 !== -(1)) {
				__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:257:6");
				wakes.push([ 2, slot2 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:264:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:265:16");
		} else {
			renumber(self, block, index);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:275:15");
	const all3 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:276:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all3, index, "std/src/reactive/lens.vl:280:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:283:3");
	let next = place + 1;
	const held = all3.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:290:5").push(__at(all3, index, "std/src/reactive/lens.vl:290:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function apply(self, change2, wakes) {
	const $fI = change2;
	let $fJ = null;
	if ($fI[0] === 0) {
		const at3 = $fI[1];
		const removed = $fI[2];
		const inserted = $fI[3];
		close(self, at3, removed, wakes);
		widen(self, at3, inserted);
		$fJ = undefined;
	} else if ($fI[0] === 1) {
		const at4 = $fI[1];
		const count2 = $fI[2];
		$fJ = widen(self, at4, count2);
	} else if ($fI[0] === 3) {
		const at5 = $fI[1];
		const count3 = $fI[2];
		$fJ = close(self, at5, count3, wakes);
	} else if ($fI[0] === 2) {
		const at6 = $fI[1];
		let $fP = null;
		if (at6 < self[6]) {
			const $fO = seat(self, at6);
			const place = $fO[0];
			const index = $fO[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:339:29")), "std/src/reactive/lens.vl:339:17"), index, "std/src/reactive/lens.vl:339:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$fP = undefined;
		}
		$fJ = $fP;
	} else if ($fI[0] === 4) {
		const at7 = $fI[1];
		const part = $fI[2];
		let $fR = null;
		if (at7 < self[6]) {
			const $fQ = seat(self, at7);
			const place2 = $fQ[0];
			const index2 = $fQ[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:348:29")), "std/src/reactive/lens.vl:348:17"), index2, "std/src/reactive/lens.vl:348:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part ]);
			}
			$fR = undefined;
		}
		$fJ = $fR;
	} else if ($fI[0] === 5) {
		const from = $fI[1];
		const to = $fI[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $fS = null;
		if (from > to) {
			$fS = from;
		} else {
			$fS = to;
		}
		const far = $fS;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:362:36")), "std/src/reactive/lens.vl:362:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:364:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:367:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:368:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $fT = seat(self, from);
		const left = $fT[0];
		const index3 = $fT[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:372:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:373:16"), index3, "std/src/reactive/lens.vl:373:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:375:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:376:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $fU = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$fU = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:384:25")), "std/src/reactive/lens.vl:384:13").length ];
		} else {
			$fU = seat(self, to);
		}
		const $fV = $fU;
		const place3 = $fV[0];
		const landing = $fV[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:388:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:389:5"), landing, slot4, "std/src/reactive/lens.vl:389:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$fJ = undefined;
	} else {
		let slot5 = 0;
		const slots = as_i32(self[4].length);
		while (slot5 < slots) {
			if (__at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:398:9") !== -(1)) {
				__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:399:7");
				wakes.push([ 2, slot5 ]);
			}
			slot5 = slot5 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$fJ = undefined;
	}
	return $fJ;
}
function new6() {
	return [ new7(), [  ] ];
}
function fresh_cells() {
	return [ new8(0, "std/src/reactive/lens.vl:888:33"), __shared_new([  ]), __shared_new(0) ];
}
function wake_all2(cells, slot2, $C) {
	const held = viewing2(cells, (list2) => {
		return __clone(__at(list2, as_usize(slot2), "std/src/reactive/lens.vl:928:35"));
	});
	notify(held[0], $C);
	for (const part of __clone(held[1].v)) {
		notify(part[1], $C);
	}
}
function wake_part(cells, slot2, name, $N) {
	const held = viewing2(cells, (list2) => {
		return __clone(__at(list2, as_usize(slot2), "std/src/reactive/lens.vl:936:35"));
	});
	notify(held[0], $N);
	for (const part of __clone(held[1].v)) {
		if (part[0] === name) {
			notify(part[1], $N);
		}
	}
}
function extend_run(runs2, from, to) {
	const held = runs2.v.length;
	let $eg = null;
	if (held > 0) {
		const $ef = __at(runs2.v, held - 1, "std/src/reactive/lens.vl:1761:31");
		const start = $ef[0];
		const count2 = $ef[1];
		const first = $ef[2];
		const leftward = first < start && from === start + count2 && to === first + count2;
		const rightward = first > start && from === start && to === first;
		if (leftward || rightward) {
			__at_put(runs2.v, held - 1, [ start, count2 + 1, first ], "std/src/reactive/lens.vl:1765:4");
			return;
		}
		$eg = undefined;
	}
	$eg;
	runs2.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function new7() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function get(self, key) {
	return __nmap_get(self[0], hash(key));
}
function find(self, key) {
	return get(self, key);
}
function insert(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function store(self, key, value) {
	insert(self, key, value);
}
function new8(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function over(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new8(0, "std/src/reactive/lens.vl:647:28"), new8(0, "std/src/reactive/lens.vl:648:24"), new8(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function keyed(value) {
	return over(value, new6(), [ (held, key) => {
		return find(held, key);
	}, (held, key, read) => {
		const $a = find(held, key);
		const item2 = [ $a[1] ];
		if ($a[0] === 0) {
			read([ item2, 0 ]);
		}
		return;
	}, (held, key, value2) => {
		return store(held, key, value2);
	}, (held, key, with2) => {
		const $b = find(held, key);
		if ($b[0] === 0) {

		} else {
			return;
		}
		let item2 = [ $b[1] ];
		with2([ item2, 0 ]);
		store(held, key, item2[0]);
		return;
	} ]);
}
function lend(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend(cell.v, read);
}
function peek_storage(storage, read) {
	const $e = storage;
	let $f = null;
	if ($e[0] === 0) {
		const cell = $e[1];
		$f = viewing(cell, read);
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
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$f = $h;
	}
	return $f;
}
function peek(self, read) {
	return peek_storage(self[0], read);
}
function coming(self, changes) {
	if (self[7].v.length === 0) {
		return;
	}
	for (const tap2 of __clone(self[7].v)) {
		for (const change2 of changes) {
			tap2[1](change2);
		}
	}
}
function mutate(self, change2) {
	const $j = self[0];
	let $k = null;
	if ($j[0] === 0) {
		const cell = $j[1];
		$k = change2(cell.v);
	} else {
		const port = $j[1];
		$k = port[1](change2);
	}
	return $k;
}
function apply2(self, change2, wakes) {
	const $o = change2;
	let $p = null;
	if ($o[0] === 0) {
		const key = $o[1];
		const _removed = $o[2];
		const _inserted = $o[3];
		const $q = get(self[0], key);
		if ($q[0] === 0) {
			wakes.push([ 0, $q[1] ]);
		}
		$p = undefined;
	} else if ($o[0] === 1) {
		const key2 = $o[1];
		const _count = $o[2];
		const $r = get(self[0], key2);
		if ($r[0] === 0) {
			wakes.push([ 0, $r[1] ]);
		}
		$p = undefined;
	} else if ($o[0] === 2) {
		const key3 = $o[1];
		const $s = get(self[0], key3);
		if ($s[0] === 0) {
			wakes.push([ 0, $s[1] ]);
		}
		$p = undefined;
	} else if ($o[0] === 3) {
		const key4 = $o[1];
		const _count2 = $o[2];
		const $t = get(self[0], key4);
		if ($t[0] === 0) {
			wakes.push([ 0, $t[1] ]);
		}
		$p = undefined;
	} else if ($o[0] === 4) {
		const key5 = $o[1];
		const part = $o[2];
		const $u = get(self[0], key5);
		if ($u[0] === 0) {
			wakes.push([ 1, $u[1], part ]);
		}
		$p = undefined;
	} else if ($o[0] === 5) {
		const _from = $o[1];
		const _to = $o[2];
		$p = undefined;
	} else {
		let slot2 = 0;
		const held = as_i32(self[1].length);
		while (slot2 < held) {
			wakes.push([ 0, slot2 ]);
			slot2 = slot2 + 1;
		}
		$p = undefined;
	}
	return $p;
}
function viewing2(cell, read) {
	return lend(cell.v, read);
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $J = null;
	if (is_empty(self)) {
		$J = [ 1 ];
	} else {
		$J = __list_get(self, self.length - 1);
	}
	return $J;
}
function notify(self, $E) {
	const traced = tracing.v;
	let $F = null;
	if (traced) {
		$F = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$F = "";
	}
	const cause = $F;
	const $G = $E;
	let $H = null;
	if ($G[0] === 0) {
		const turn = $G[1];
		$H = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $K = last(draining_turns.v);
		let $L = null;
		if ($K[0] === 0) {
			const draining = $K[1];
			$L = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $M = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$M = undefined;
				}
				$M;
			}
			$L = undefined;
		}
		$H = $L;
	}
	return $H;
}
function batch(body, $T) {
	const $U = $T;
	let $V = null;
	if ($U[0] === 0) {
		const current = $U[1];
		$V = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$V = result;
	}
	return $V;
}
function landed(self, changes, late, $l) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index = 0;
	for (const change2 of changes) {
		const $m = change2;
		let $n = null;
		if ($m[0] === 0) {
			const _at = $m[1];
			const removed = $m[2];
			const inserted = $m[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$n = undefined;
		} else if ($m[0] === 1) {
			const _at2 = $m[1];
			const _count = $m[2];
			$n = resized = true;
		} else if ($m[0] === 3) {
			const _at3 = $m[1];
			const _count2 = $m[2];
			$n = resized = true;
		} else if ($m[0] === 6) {
			$n = resized = true;
		} else if ($m[0] === 5) {
			const _from = $m[1];
			const _to = $m[2];
			$n = shaped = true;
		} else {
			$n = undefined;
		}
		$n;
		apply2(self[1].v, change2, wakes);
		let $y = null;
		if (tapped) {
			let $x = null;
			if (!(late)) {
				$x = [ 0 ];
			} else if (!(several)) {
				$x = [ 1 ];
			} else if (index === 0) {
				$x = [ 2 ];
			} else {
				$x = [ 3 ];
			}
			const stage = $x;
			for (const tap2 of __clone(self[7].v)) {
				tap2[2](change2, stage);
			}
			$y = undefined;
		}
		$y;
		index = index + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($z) => {
		for (const wake3 of wakes) {
			const $A = wake3;
			let $B = null;
			if ($A[0] === 0) {
				const slot2 = $A[1];
				$B = wake_all2(cells, slot2, [ 0, $z ]);
			} else if ($A[0] === 2) {
				const slot3 = $A[1];
				$B = wake_all2(cells, slot3, [ 0, $z ]);
			} else {
				const slot4 = $A[1];
				const part = $A[2];
				$B = wake_part(cells, slot4, part, [ 0, $z ]);
			}
			$B;
		}
		if (resized) {
			notify(sized, [ 0, $z ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $z ]);
		}
		notify(published, [ 0, $z ]);
		return;
	}, $l);
}
function put(self, key, value, $c) {
	const had = peek(self, (held) => {
		const $d = find(held, key);
		return $d[0] === 0;
	});
	let $i = null;
	if (had) {
		$i = [ 2, __clone(key) ];
	} else {
		$i = [ 1, __clone(key), 1 ];
	}
	const change2 = $i;
	coming(self, [ __clone(change2) ]);
	mutate(self, (held) => {
		return store(held, key, value);
	});
	landed(self, [ change2 ], false, $c);
}
function new9() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function peek_storage2(storage, read) {
	const $X = storage;
	let $Y = null;
	if ($X[0] === 0) {
		const cell = $X[1];
		$Y = viewing2(cell, read);
	} else {
		const port = $X[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $Z = out;
		let $aa = null;
		if ($Z[0] === 0) {
			const value = __clone($Z[1]);
			$aa = value;
		} else {
			$aa = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$Y = $aa;
	}
	return $Y;
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
			const $ah = __list_get(held, index);
			let $ai = null;
			if ($ah[0] === 0) {
				const op = $ah[1];
				$ai = kept.push(op);
			} else {
				$ai = undefined;
			}
			$ai;
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
function record_put(log, key, now, leaving, late) {
	const $aj = now;
	if ($aj[0] === 0) {
		const $al = __clone(key);
		let $ak = null;
		if (late) {
			$ak = [ 1 ];
		} else {
			$ak = leaving.v;
		}
		record(log, [ 0, $al, __clone($ak), __clone($aj[1]) ]);
	}
}
function keys(self) {
	return __nmap_keys(self[0]);
}
function held_keys(self) {
	return keys(self);
}
function entries_of(storage) {
	return peek_storage2(storage, (held) => {
		let out = new7();
		for (const key of held_keys(held)) {
			const $am = find(held, key);
			if ($am[0] === 0) {
				insert(out, key, $am[1]);
			}
		}
		return out;
	});
}
function tap(self, before2, after) {
	const id = self[8].v;
	self[8].v = id + 1;
	self[7].v.push([ id, before2, after ]);
	return id;
}
function untap(self, id) {
	let kept = [  ];
	for (const tap2 of self[7].v) {
		if (tap2[0] !== id) {
			kept.push(__clone(tap2));
		}
	}
	self[7].v = kept;
}
function release(self) {
	untap(self[0], self[2]);
}
function entries(self, $W) {
	const log = new9();
	const leaving = __shared_new([ 1 ]);
	const storage = __clone(self[0]);
	const find2 = (key) => {
		return peek_storage2(storage, (held) => {
			return find(held, key);
		});
	};
	const tap2 = tap(self, (change2) => {
		const $ab = change2;
		let $ac = null;
		if ($ab[0] === 0) {
			const key = $ab[1];
			const _removed = $ab[2];
			const _inserted = $ab[3];
			$ac = leaving.v = find2(key);
		} else if ($ab[0] === 3) {
			const key2 = $ab[1];
			const _count = $ab[2];
			$ac = leaving.v = find2(key2);
		} else if ($ab[0] === 2) {
			const key3 = $ab[1];
			$ac = leaving.v = find2(key3);
		} else if ($ab[0] === 4) {
			const key4 = $ab[1];
			const _part = $ab[2];
			$ac = leaving.v = find2(key4);
		} else {
			$ac = undefined;
		}
		return $ac;
	}, (change2, stage) => {
		const $ad = stage;
		const late = !($ad[0] === 0);
		const $ae = change2;
		let $af = null;
		if ($ae[0] === 1) {
			const key = $ae[1];
			const _count = $ae[2];
			const $ag = find2(key);
			if ($ag[0] === 0) {
				record(log, [ 0, __clone(key), [ 1 ], $ag[1] ]);
			}
			$af = undefined;
		} else if ($ae[0] === 0) {
			const key2 = $ae[1];
			const _removed = $ae[2];
			const _inserted = $ae[3];
			$af = record_put(log, key2, find2(key2), leaving, late);
		} else if ($ae[0] === 2) {
			const key3 = $ae[1];
			$af = record_put(log, key3, find2(key3), leaving, late);
		} else if ($ae[0] === 4) {
			const key4 = $ae[1];
			const _part = $ae[2];
			$af = record_put(log, key4, find2(key4), leaving, late);
		} else if ($ae[0] === 3) {
			const key5 = $ae[1];
			const _count2 = $ae[2];
			if (late) {
				record(log, [ 2, entries_of(storage) ]);
			} else {
				const $ar = leaving.v;
				if ($ar[0] === 0) {
					record(log, [ 1, __clone(key5), $ar[1] ]);
				}
			}
			$af = undefined;
		} else if ($ae[0] === 5) {
			const _from = $ae[1];
			const _to = $ae[2];
			$af = undefined;
		} else {
			$af = record(log, [ 2, entries_of(storage) ]);
		}
		return $af;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release(feed);
	}, $W);
	return feed;
}
function filter(self, keep) {
	return [ self, keep ];
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
		const $aA = __list_get(held, index);
		let $aB = null;
		if ($aA[0] === 0) {
			const op = $aA[1];
			$aB = ops.push(op);
		} else {
			$aB = undefined;
		}
		$aB;
		index = index + 1;
	}
	return [ 0, ops ];
}
function all(self) {
	return entries_of(self[0]);
}
function since2(self, cursor4) {
	const $aC = since(self[1], cursor4);
	let $aD = null;
	if ($aC[0] === 0) {
		const recorded = $aC[1];
		$aD = recorded;
	} else {
		$aD = [ [ 2, all(self[0]) ] ];
	}
	return $aD;
}
function reader(self) {
	return (cursor4) => {
		return since2(self, cursor4);
	};
}
function get2(self) {
	return all(self[0]);
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
	return on_settle(self[0][4], subscriber);
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
function open2(self) {
	const cursor4 = cursor2(self);
	const read = reader(self);
	return [ () => {
		return get2(self);
	}, (subscriber) => {
		return on_settle2(self, subscriber);
	}, () => {
		return read(cursor4);
	}, () => {
		return drop_cursor2(self, cursor4);
	} ];
}
function run_element(stage, body) {
	const nursery2 = stage_nursery(stage);
	const value = (($aJ) => {
		return (($aK) => {
			return body($aJ, $aK);
		})(nursery2);
	})(stage);
	return [ value, split_registrations(stage) ];
}
function start_element(self) {
	return [ 0, self ];
}
function element_core(stage, run) {
	const ids = [  ];
	const items2 = [  ];
	const values = [  ];
	const holds2 = new Map();
	const dirty = [  ];
	const consumer = [ 1 ];
	return [ __shared_new(ids), __shared_new(items2), __shared_new(values), __shared_new(holds2), __shared_new(dirty), __shared_new(0), __clone(stage), __shared_new(consumer), run ];
}
function keyed_core(stage, run) {
	const rows = new Map();
	const key_of_slot = new Map();
	return [ element_core(stage, run), __shared_new(rows), __shared_new(key_of_slot) ];
}
function entries2(self) {
	let keys2 = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys2))) {
		const $aM = __list_pop(keys2);
		let $aN = null;
		if ($aM[0] === 0) {
			const key = $aM[1];
			const $aO = __list_pop(values);
			let $aP = null;
			if ($aO[0] === 0) {
				const value = $aO[1];
				$aP = backwards.push([ key, value ]);
			} else {
				$aP = undefined;
			}
			$aN = $aP;
		} else {
			$aN = undefined;
		}
		$aN;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $aQ = __list_pop(backwards);
		let $aR = null;
		if ($aQ[0] === 0) {
			const pair = $aQ[1];
			$aR = result.push(pair);
		} else {
			$aR = undefined;
		}
		$aR;
	}
	return result;
}
function keep_hold(core, id, hold) {
	element_holds_allocated_count.v = element_holds_allocated_count.v + 1;
	core[3].v.set(hash2(id), __clone(hold));
}
function admit(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $aS = core[8](item2);
	const answer = $aS[0];
	const owner = $aS[1];
	const $aT = answer;
	let $aU = null;
	if ($aT[0] === 0) {
		const value = $aT[1];
		const $aV = owner;
		let $aW = null;
		if ($aV[0] === 0) {
			const held = $aV[1];
			$aW = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$aW = undefined;
		}
		$aW;
		$aU = [ id, __clone(value) ];
	} else {
		const instance = $aT[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $aX = consumer.v;
			let $aY = null;
			if ($aX[0] === 0) {
				const downstream = $aX[1];
				$aY = wake(downstream);
			} else {
				$aY = undefined;
			}
			return $aY;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$aU = [ id, pull() ];
	}
	return $aU;
}
function keyed_admit(keyed2, key, input) {
	const $bd = admit(keyed2[0], [ __clone(key), __clone(input) ]);
	const id = $bd[0];
	const value = $bd[1];
	const row = [ __clone(key), __clone(input), value, id ];
	const hash3 = hash(key);
	keyed2[1].v.set(hash3, __clone(row));
	keyed2[2].v.set(hash2(id), hash3);
	return row;
}
function keyed_seed(keyed2, map3) {
	for (const entry of entries2(map3)) {
		keyed_admit(keyed2, entry[0], entry[1]);
	}
}
function keyed_rows(keyed2) {
	return __map_values(keyed2[1].v);
}
function kept_map(rows) {
	let out = new7();
	for (const row of rows) {
		if (row[2]) {
			insert(out, row[0], row[1]);
		}
	}
	return out;
}
function keyed_attach(keyed2, attach2) {
	const consumer = keyed2[0][7];
	return (subscriber) => {
		consumer.v = [ 0, __clone(subscriber) ];
		return attach2(subscriber);
	};
}
function is_none(self) {
	const $bz = self;
	return $bz[0] === 1;
}
function retire(core, id) {
	const $bi = __map_get(core[3].v, hash2(id));
	let $bj = null;
	if ($bi[0] === 0) {
		const hold = $bi[1];
		core[3].v.delete(hash2(id));
		const $bk = hold[2];
		let $bl = null;
		if ($bk[0] === 0) {
			const handle = $bk[1];
			$bl = detach(handle);
		} else {
			$bl = undefined;
		}
		$bl;
		hold[3]();
		const $bv = hold[4];
		let $bw = null;
		if ($bv[0] === 0) {
			const owner = $bv[1];
			$bw = dispose2(owner);
		} else {
			$bw = undefined;
		}
		$bj = $bw;
	} else {
		$bj = undefined;
	}
	return $bj;
}
function keyed_retire(keyed2, row) {
	keyed2[2].v.delete(hash2(row[3]));
	retire(keyed2[0], row[3]);
}
function keyed_resolve(keyed2, ops) {
	let events = [  ];
	for (const op of ops) {
		const $be = op;
		let $bf = null;
		if ($be[0] === 0) {
			const key = $be[1];
			const _was = $be[2];
			const input = $be[3];
			const $bg = __map_get(keyed2[1].v, hash(key));
			let $bh = null;
			if ($bg[0] === 0) {
				const row = $bg[1];
				keyed_retire(keyed2, row);
				const fresh = keyed_admit(keyed2, key, input);
				events.push([ 1, __clone(key), [ __clone(row[1]), __clone(row[2]) ], [ __clone(fresh[1]), __clone(fresh[2]) ], true ]);
				$bh = undefined;
			} else {
				const fresh2 = keyed_admit(keyed2, key, input);
				events.push([ 0, __clone(key), __clone(fresh2[1]), __clone(fresh2[2]) ]);
				$bh = undefined;
			}
			$bf = $bh;
		} else if ($be[0] === 1) {
			const key2 = $be[1];
			const _gone = $be[2];
			const hash3 = hash(key2);
			const $bH = __map_get(keyed2[1].v, hash3);
			let $bI = null;
			if ($bH[0] === 0) {
				const row2 = $bH[1];
				keyed_retire(keyed2, row2);
				keyed2[1].v.delete(hash3);
				events.push([ 2, __clone(key2), __clone(row2[1]), __clone(row2[2]) ]);
				$bI = undefined;
			} else {
				$bI = undefined;
			}
			$bf = $bI;
		} else {
			const map3 = $be[1];
			for (const row3 of keyed_rows(keyed2)) {
				keyed_retire(keyed2, row3);
			}
			const emptied = new Map();
			keyed2[1].v = emptied;
			keyed_seed(keyed2, map3);
			events.push([ 3, keyed_rows(keyed2) ]);
			$bf = undefined;
		}
		$bf;
	}
	const woke = __clone(keyed2[0][4].v);
	let $bP = null;
	if (woke.length > 0) {
		keyed2[0][4].v = [  ];
		let seen = new Map();
		for (const id of woke) {
			if (seen.has(hash2(id))) {
				continue;
			}
			seen.set(hash2(id), true);
			const $bJ = __map_get(keyed2[2].v, hash2(id));
			let $bK = null;
			if ($bJ[0] === 0) {
				const hash4 = $bJ[1];
				const $bL = __map_get(keyed2[1].v, hash4);
				let $bM = null;
				if ($bL[0] === 0) {
					const row4 = $bL[1];
					const $bN = __map_get(keyed2[0][3].v, hash2(id));
					let $bO = null;
					if ($bN[0] === 0) {
						const hold = $bN[1];
						if (hold[0]) {
							const now = hold[1]();
							keyed2[1].v.set(hash4, [ __clone(row4[0]), __clone(row4[1]), __clone(now), row4[3] ]);
							events.push([ 1, __clone(row4[0]), [ __clone(row4[1]), __clone(row4[2]) ], [ __clone(row4[1]), now ], false ]);
						}
						$bO = undefined;
					} else {
						$bO = undefined;
					}
					$bM = $bO;
				} else {
					$bM = undefined;
				}
				$bK = $bM;
			} else {
				$bK = undefined;
			}
			$bK;
		}
		$bP = undefined;
	}
	$bP;
	return events;
}
function filtered_map_ops(events) {
	let produced = [  ];
	for (const event of events) {
		const $bQ = event;
		let $bR = null;
		if ($bQ[0] === 0) {
			const key = $bQ[1];
			const input = $bQ[2];
			const kept = $bQ[3];
			if (kept) {
				produced.push([ 0, __clone(key), [ 1 ], __clone(input) ]);
			}
			$bR = undefined;
		} else if ($bQ[0] === 1) {
			const key2 = $bQ[1];
			const before2 = $bQ[2];
			const after = $bQ[3];
			const rerun = $bQ[4];
			let $bS = null;
			if (before2[1] && after[1]) {
				if (rerun) {
					produced.push([ 0, __clone(key2), [ 0, __clone(before2[0]) ], __clone(after[0]) ]);
				}
				$bS = undefined;
			} else if (before2[1]) {
				produced.push([ 1, __clone(key2), __clone(before2[0]) ]);
			} else if (after[1]) {
				produced.push([ 0, __clone(key2), [ 1 ], __clone(after[0]) ]);
			}
			$bR = $bS;
		} else if ($bQ[0] === 2) {
			const key3 = $bQ[1];
			const input2 = $bQ[2];
			const kept2 = $bQ[3];
			if (kept2) {
				produced.push([ 1, __clone(key3), __clone(input2) ]);
			}
			$bR = undefined;
		} else {
			const rows = $bQ[1];
			$bR = produced.push([ 2, kept_map(rows) ]);
		}
		$bR;
	}
	return produced;
}
function keyed_release(keyed2, release5) {
	return () => {
		for (const row of keyed_rows(keyed2)) {
			keyed_retire(keyed2, row);
		}
		dispose2(keyed2[0][6]);
		release5();
		return;
	};
}
function open3(self) {
	const keep = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new3();
	const keyed2 = keyed_core(stage, (entry) => {
		const $aL = run_element(stage, ($aE, $aF) => {
			return keep(entry[0], entry[1], $aE, $aF);
		});
		const answer = $aL[0];
		const owner = $aL[1];
		return [ start_element(answer), owner ];
	});
	keyed_seed(keyed2, upstream[0]());
	const drain2 = upstream[2];
	return [ () => {
		return kept_map(keyed_rows(keyed2));
	}, keyed_attach(keyed2, upstream[1]), () => {
		return filtered_map_ops(keyed_resolve(keyed2, drain2()));
	}, keyed_release(keyed2, upstream[3]) ];
}
function hash_map_cell(map3, log) {
	return [ __shared_new(map3), new8(0, "std/src/reactive/hash_map_cell.vl:244:27"), __clone(log), new4() ];
}
function of(map3) {
	return hash_map_cell(map3, new9());
}
function remove(self, key) {
	__nmap_remove(self[0], hash(key));
}
function wake_for(slots, ops, $ca) {
	let named = new Map();
	let order = [  ];
	for (const op of ops) {
		const $cb = op;
		let $cc = null;
		if ($cb[0] === 0) {
			const key = $cb[1];
			const _was = $cb[2];
			const _now = $cb[3];
			$cc = hash(key);
		} else if ($cb[0] === 1) {
			const key2 = $cb[1];
			const _gone = $cb[2];
			$cc = hash(key2);
		} else {
			const _map = $cb[1];
			wake_all(slots, $ca);
			return;
			$cc = undefined;
		}
		const hash3 = $cc;
		if (!(named.has(hash3))) {
			named.set(hash3, true);
			order.push(hash3);
		}
	}
	for (const hash4 of order) {
		wake2(slots, hash4, $ca);
	}
}
function at(self) {
	return self[1].v;
}
function set(self, value, $ci, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $ci);
}
function publish_map(cell, $ch) {
	set(cell[1], at(cell[2]), $ch, "std/src/reactive/hash_map_cell.vl:252:17");
}
function apply_map_ops(cell, ops, $bV) {
	let recorded = [  ];
	for (const op of ops) {
		const $bW = op;
		let $bX = null;
		if ($bW[0] === 0) {
			const key = $bW[1];
			const _claimed = $bW[2];
			const now = $bW[3];
			const previous = get(cell[0].v, key);
			insert(cell[0].v, key, now);
			recorded.push([ 0, __clone(key), previous, __clone(now) ]);
			$bX = undefined;
		} else if ($bW[0] === 1) {
			const key2 = $bW[1];
			const _claimed2 = $bW[2];
			const $bY = get(cell[0].v, key2);
			let $bZ = null;
			if ($bY[0] === 0) {
				const gone = $bY[1];
				remove(cell[0].v, key2);
				recorded.push([ 1, __clone(key2), gone ]);
				$bZ = undefined;
			} else {
				$bZ = undefined;
			}
			$bX = $bZ;
		} else {
			const map3 = $bW[1];
			cell[0].v = __clone(map3);
			recorded.push([ 2, __clone(map3) ]);
			$bX = undefined;
		}
		$bX;
	}
	if (recorded.length === 0) {
		return;
	}
	for (const op2 of recorded) {
		record(cell[2], op2);
	}
	wake_for(cell[3], recorded, $bV);
	publish_map(cell, $bV);
}
function take(self, item2, $cp) {
	defer(self, () => {
		dispose(item2, $cp);
		return;
	});
	return __clone(item2);
}
function seal_map(instance, owned, $bT, $bU, caller) {
	const sealed = of(instance[0]());
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_map_ops(sealed, drain2(), $bT);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $bT, $bU);
	} else {
		const cell_global_refresh = () => {
			return apply_map_ops(sealed, drain2(), $bT);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $ay, $az, caller) {
	return [ seal_map(open3(self), true, $ay, $az, caller) ];
}
function map_values(self, transform) {
	return [ self, transform ];
}
function start_element_value(self) {
	return [ 0, self ];
}
function keyed_core2(stage, run) {
	const rows = new Map();
	const key_of_slot = new Map();
	return [ element_core(stage, run), __shared_new(rows), __shared_new(key_of_slot) ];
}
function admit2(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $cy = core[8](item2);
	const answer = $cy[0];
	const owner = $cy[1];
	const $cz = answer;
	let $cA = null;
	if ($cz[0] === 0) {
		const value = $cz[1];
		const $cB = owner;
		let $cC = null;
		if ($cB[0] === 0) {
			const held = $cB[1];
			$cC = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$cC = undefined;
		}
		$cC;
		$cA = [ id, __clone(value) ];
	} else {
		const instance = $cz[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $cD = consumer.v;
			let $cE = null;
			if ($cD[0] === 0) {
				const downstream = $cD[1];
				$cE = wake(downstream);
			} else {
				$cE = undefined;
			}
			return $cE;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$cA = [ id, pull() ];
	}
	return $cA;
}
function keyed_admit2(keyed2, key, input) {
	const $cF = admit2(keyed2[0], [ __clone(key), __clone(input) ]);
	const id = $cF[0];
	const value = $cF[1];
	const row = [ __clone(key), __clone(input), value, id ];
	const hash3 = hash(key);
	keyed2[1].v.set(hash3, __clone(row));
	keyed2[2].v.set(hash2(id), hash3);
	return row;
}
function keyed_seed2(keyed2, map3) {
	for (const entry of entries2(map3)) {
		keyed_admit2(keyed2, entry[0], entry[1]);
	}
}
function retire2(core, id) {
	const $cK = __map_get(core[3].v, hash2(id));
	let $cL = null;
	if ($cK[0] === 0) {
		const hold = $cK[1];
		core[3].v.delete(hash2(id));
		const $cM = hold[2];
		let $cN = null;
		if ($cM[0] === 0) {
			const handle = $cM[1];
			$cN = detach(handle);
		} else {
			$cN = undefined;
		}
		$cN;
		hold[3]();
		const $cO = hold[4];
		let $cP = null;
		if ($cO[0] === 0) {
			const owner = $cO[1];
			$cP = dispose2(owner);
		} else {
			$cP = undefined;
		}
		$cL = $cP;
	} else {
		$cL = undefined;
	}
	return $cL;
}
function keyed_retire2(keyed2, row) {
	keyed2[2].v.delete(hash2(row[3]));
	retire2(keyed2[0], row[3]);
}
function keyed_resolve2(keyed2, ops) {
	let events = [  ];
	for (const op of ops) {
		const $cG = op;
		let $cH = null;
		if ($cG[0] === 0) {
			const key = $cG[1];
			const _was = $cG[2];
			const input = $cG[3];
			const $cI = __map_get(keyed2[1].v, hash(key));
			let $cJ = null;
			if ($cI[0] === 0) {
				const row = $cI[1];
				keyed_retire2(keyed2, row);
				const fresh = keyed_admit2(keyed2, key, input);
				events.push([ 1, __clone(key), [ __clone(row[1]), __clone(row[2]) ], [ __clone(fresh[1]), __clone(fresh[2]) ], true ]);
				$cJ = undefined;
			} else {
				const fresh2 = keyed_admit2(keyed2, key, input);
				events.push([ 0, __clone(key), __clone(fresh2[1]), __clone(fresh2[2]) ]);
				$cJ = undefined;
			}
			$cH = $cJ;
		} else if ($cG[0] === 1) {
			const key2 = $cG[1];
			const _gone = $cG[2];
			const hash3 = hash(key2);
			const $cQ = __map_get(keyed2[1].v, hash3);
			let $cR = null;
			if ($cQ[0] === 0) {
				const row2 = $cQ[1];
				keyed_retire2(keyed2, row2);
				keyed2[1].v.delete(hash3);
				events.push([ 2, __clone(key2), __clone(row2[1]), __clone(row2[2]) ]);
				$cR = undefined;
			} else {
				$cR = undefined;
			}
			$cH = $cR;
		} else {
			const map3 = $cG[1];
			for (const row3 of keyed_rows(keyed2)) {
				keyed_retire2(keyed2, row3);
			}
			const emptied = new Map();
			keyed2[1].v = emptied;
			keyed_seed2(keyed2, map3);
			events.push([ 3, keyed_rows(keyed2) ]);
			$cH = undefined;
		}
		$cH;
	}
	const woke = __clone(keyed2[0][4].v);
	let $cY = null;
	if (woke.length > 0) {
		keyed2[0][4].v = [  ];
		let seen = new Map();
		for (const id of woke) {
			if (seen.has(hash2(id))) {
				continue;
			}
			seen.set(hash2(id), true);
			const $cS = __map_get(keyed2[2].v, hash2(id));
			let $cT = null;
			if ($cS[0] === 0) {
				const hash4 = $cS[1];
				const $cU = __map_get(keyed2[1].v, hash4);
				let $cV = null;
				if ($cU[0] === 0) {
					const row4 = $cU[1];
					const $cW = __map_get(keyed2[0][3].v, hash2(id));
					let $cX = null;
					if ($cW[0] === 0) {
						const hold = $cW[1];
						if (hold[0]) {
							const now = hold[1]();
							keyed2[1].v.set(hash4, [ __clone(row4[0]), __clone(row4[1]), __clone(now), row4[3] ]);
							events.push([ 1, __clone(row4[0]), [ __clone(row4[1]), __clone(row4[2]) ], [ __clone(row4[1]), now ], false ]);
						}
						$cX = undefined;
					} else {
						$cX = undefined;
					}
					$cV = $cX;
				} else {
					$cV = undefined;
				}
				$cT = $cV;
			} else {
				$cT = undefined;
			}
			$cT;
		}
		$cY = undefined;
	}
	$cY;
	return events;
}
function mapped_map_ops(events) {
	let produced = [  ];
	for (const event of events) {
		const $cZ = event;
		let $da = null;
		if ($cZ[0] === 0) {
			const key = $cZ[1];
			const _input = $cZ[2];
			const value = $cZ[3];
			$da = produced.push([ 0, __clone(key), [ 1 ], __clone(value) ]);
		} else if ($cZ[0] === 1) {
			const key2 = $cZ[1];
			const before2 = $cZ[2];
			const after = $cZ[3];
			const _rerun = $cZ[4];
			produced.push([ 0, __clone(key2), [ 0, __clone(before2[1]) ], __clone(after[1]) ]);
			$da = undefined;
		} else if ($cZ[0] === 2) {
			const key3 = $cZ[1];
			const _input2 = $cZ[2];
			const value2 = $cZ[3];
			$da = produced.push([ 1, __clone(key3), __clone(value2) ]);
		} else {
			const rows = $cZ[1];
			let map3 = new7();
			for (const row of rows) {
				insert(map3, row[0], row[2]);
			}
			produced.push([ 2, map3 ]);
			$da = undefined;
		}
		$da;
	}
	return produced;
}
function keyed_release2(keyed2, release5) {
	return () => {
		for (const row of keyed_rows(keyed2)) {
			keyed_retire2(keyed2, row);
		}
		dispose2(keyed2[0][6]);
		release5();
		return;
	};
}
function open4(self) {
	const transform = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new3();
	const keyed2 = keyed_core2(stage, (entry) => {
		const $cx = run_element(stage, ($cv, $cw) => {
			return transform(entry[1], $cv, $cw);
		});
		const answer = $cx[0];
		const owner = $cx[1];
		return [ start_element_value(answer), owner ];
	});
	keyed_seed2(keyed2, upstream[0]());
	const drain2 = upstream[2];
	return [ () => {
		let out = new7();
		for (const row of keyed_rows(keyed2)) {
			insert(out, row[0], row[2]);
		}
		return out;
	}, keyed_attach(keyed2, upstream[1]), () => {
		return mapped_map_ops(keyed_resolve2(keyed2, drain2()));
	}, keyed_release2(keyed2, upstream[3]) ];
}
function memo2(self, $ay, $az, caller) {
	return [ seal_map(open4(self), true, $ay, $az, caller) ];
}
function len(self) {
	return __nmap_len(self[0]);
}
function len2(self) {
	return len(self[0].v);
}
function len3(self) {
	return len2(self[0]);
}
function get3(self, key) {
	return get(self[0].v, key);
}
function get4(self, key) {
	return get3(self[0], key);
}
function drop(self, key) {
	remove(self, key);
}
function remove2(self, key, $dh) {
	const had = peek(self, (held) => {
		const $di = find(held, key);
		return $di[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming(self, [ [ 3, __clone(key), 1 ] ]);
	mutate(self, (held) => {
		return drop(held, key);
	});
	landed(self, [ [ 3, __clone(key), 1 ] ], false, $dh);
}
function length(self) {
	return [ __clone(self) ];
}
function peek2(self, read) {
	return peek_storage2(self[0], read);
}
function get5(self) {
	return peek2(self[0], (held) => {
		return held_keys(held).length;
	});
}
function on_settle3(self, subscriber) {
	return on_settle(self[0][6], subscriber);
}
function attach_observer(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get5(self));
	}));
	if (immediately) {
		observer(get5(self));
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
function follows_items(self) {
	return false;
}
function holds(storage, view, at3) {
	let found = false;
	peek_storage2(storage, (held) => {
		return view(held, at3, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow(self, at3) {
	const $du = get(self[0], at3);
	if ($du[0] === 0) {
		return $du[1];
	}
	const slot2 = as_i32(self[1].length);
	self[1].push(__clone(at3));
	insert(self[0], at3, slot2);
	return slot2;
}
function follow_id(storage, handler, cells, view, at3) {
	if (viewing2(handler, (held) => {
		return follows_items(held);
	}) && !(holds(storage, view, at3))) {
		return -(1);
	}
	const id = follow(handler.v, at3);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $dv = [ slot_cells[2], "v" ];
	slot_cells[2].v = $dv[0][$dv[1]] + 1;
	return id;
}
function slot_of(self, id) {
	if (id < 0) {
		return [ __clone(self), -(1), fresh_cells() ];
	}
	return [ __clone(self), id, viewing2(self[3], (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:827:66"));
	}) ];
}
function follow2(self, at3) {
	return slot_of(self, follow_id(self[0], self[1], self[3], self[2][1], at3));
}
function release2(self, slot2) {
	const $dw = __list_get(self[1], as_usize(slot2));
	if ($dw[0] === 0) {
		remove(self[0], $dw[1]);
	}
}
function release_id(handler, cells, id) {
	if (id < 0) {
		return;
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:919:41"));
	});
	const left = slot_cells[2].v - 1;
	slot_cells[2].v = left;
	if (left <= 0) {
		release2(handler.v, id);
	}
}
function release3(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot(self, at3, $dp) {
	const handle = follow2(self, at3);
	defer_to_owner(() => {
		return release3(handle);
	}, $dp);
	return handle;
}
function locate(self, slot2) {
	return __list_get(self[1], as_usize(slot2));
}
function at2(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][1], (handler) => {
		return locate(handler, self[1]);
	});
}
function get6(self) {
	const $dz = at2(self);
	if ($dz[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][2][0];
	return peek2(self[0], (held) => {
		return read(held, $dz[1]);
	});
}
function on_settle4(self, subscriber) {
	return on_settle(self[2][0], subscriber);
}
function attach_observer2(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get6(self));
	}));
	if (immediately) {
		observer(get6(self));
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
function contains_key(self, key) {
	return __nmap_has(self[0], hash(key));
}
function reconcile_keyed(self, next, $dA) {
	const lens = __clone(self);
	batch(($dB) => {
		for (const key of peek2(lens, (held2) => {
			return held_keys(held2);
		})) {
			if (!(contains_key(next, key))) {
				remove2(lens, key, [ 0, $dB ]);
			}
		}
		for (const _ of entries2(next)) {
			const $dG = _;
			const key2 = $dG[0];
			const value = $dG[1];
			const $dH = peek2(lens, (held2) => {
				return find(held2, key2);
			});
			let $dI = null;
			if ($dH[0] === 0) {
				const held = $dH[1];
				if (held !== value) {
					put(lens, key2, value, [ 0, $dB ]);
				}
				$dI = undefined;
			} else {
				$dI = put(lens, key2, value, [ 0, $dB ]);
			}
			$dI;
		}
		return;
	}, $dA);
}
function item(self, at3) {
	return __list_get(self, at3);
}
function lend2(self, at3, read) {
	if (at3 < self.length) {
		read(__at_view(self, at3, "std/src/reactive/lens.vl:2008:10"));
	}
}
function put2(self, at3, value) {
	__at_put(self, at3, __clone(value), "std/src/reactive/lens.vl:2003:3");
}
function change(self, at3, with2) {
	if (at3 < self.length) {
		with2(__at_view(self, at3, "std/src/reactive/lens.vl:2014:14"));
	}
}
function over2(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new8(0, "std/src/reactive/lens.vl:647:28"), new8(0, "std/src/reactive/lens.vl:648:24"), new8(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function sequence(value, $dK) {
	return over2(value, new5(), [ (held, at3) => {
		return item(held, at3);
	}, (held, at3, read) => {
		return lend2(held, at3, read);
	}, (held, at3, value2) => {
		return put2(held, at3, value2, $dK);
	}, (held, at3, with2) => {
		return change(held, at3, with2, $dK);
	} ]);
}
function count(self) {
	return self.length;
}
function explain_sequence(before2, after, changes) {
	const old_count = count(before2);
	const new_count = count(after);
	let low = old_count;
	let high = 0;
	let grew = 0;
	for (const change2 of changes) {
		const $dL = change2;
		let $dM = null;
		if ($dL[0] === 0) {
			const at3 = $dL[1];
			const removed = $dL[2];
			const inserted = $dL[3];
			if (at3 < low) {
				low = at3;
			}
			if (at3 + removed > high) {
				high = at3 + removed;
			}
			grew = grew + (as_i32(inserted) - as_i32(removed));
			$dM = undefined;
		} else if ($dL[0] === 1) {
			const at4 = $dL[1];
			const count2 = $dL[2];
			if (at4 < low) {
				low = at4;
			}
			if (at4 > high) {
				high = at4;
			}
			grew = grew + as_i32(count2);
			$dM = undefined;
		} else if ($dL[0] === 3) {
			const at5 = $dL[1];
			const count3 = $dL[2];
			if (at5 < low) {
				low = at5;
			}
			if (at5 + count3 > high) {
				high = at5 + count3;
			}
			grew = grew - as_i32(count3);
			$dM = undefined;
		} else if ($dL[0] === 2) {
			const at6 = $dL[1];
			if (at6 < low) {
				low = at6;
			}
			if (at6 + 1 > high) {
				high = at6 + 1;
			}
			$dM = undefined;
		} else if ($dL[0] === 4) {
			const at7 = $dL[1];
			const _part = $dL[2];
			if (at7 < low) {
				low = at7;
			}
			if (at7 + 1 > high) {
				high = at7 + 1;
			}
			$dM = undefined;
		} else if ($dL[0] === 5) {
			const from = $dL[1];
			const to = $dL[2];
			let $dN = null;
			if (from < to) {
				$dN = from;
			} else {
				$dN = to;
			}
			const first = $dN;
			let $dO = null;
			if (from < to) {
				$dO = to;
			} else {
				$dO = from;
			}
			const last2 = $dO;
			if (first < low) {
				low = first;
			}
			if (last2 + 1 > high) {
				high = last2 + 1;
			}
			$dM = undefined;
		} else {
			return [ 1 ];
		}
		$dM;
	}
	if (as_i32(new_count) !== as_i32(old_count) + grew) {
		return [ 0, "the changes add up to " + (as_i32(old_count) + grew) + " items, the write left " + new_count ];
	}
	if (changes.length === 0) {
		low = 0;
		high = 0;
	}
	let at8 = 0;
	let $dP = null;
	if (low < old_count) {
		$dP = low;
	} else {
		$dP = old_count;
	}
	const head = $dP;
	while (at8 < head) {
		if (!(eq2(item(before2, at8), item(after, at8)))) {
			return [ 0, "item " + at8 + " changed, and no change names it" ];
		}
		at8 = at8 + 1;
	}
	if (high > old_count) {
		high = old_count;
	}
	let tail = high;
	while (tail < old_count) {
		const moved = as_usize(as_i32(tail) + grew);
		if (!(eq2(item(before2, tail), item(after, moved)))) {
			return [ 0, "item " + tail + " (now " + moved + ") changed, and no change names it" ];
		}
		tail = tail + 1;
	}
	return [ 1 ];
}
function sequence_checked(value, $dJ) {
	const lens = sequence(value, $dJ);
	lens[9].v = [ 0, (before2, after, changes) => {
		return explain_sequence(before2, after, changes);
	} ];
	return lens;
}
function arrived_at(storage, at3, count2) {
	return peek_storage2(storage, (held) => {
		let arrived = [  ];
		let index = at3;
		while (index < at3 + count2) {
			const $dU = item(held, index);
			if ($dU[0] === 0) {
				arrived.push($dU[1]);
			}
			index = index + 1;
		}
		return arrived;
	});
}
function one(item2) {
	const $ed = item2;
	let $ee = null;
	if ($ed[0] === 0) {
		const value = $ed[1];
		$ee = [ __clone(value) ];
	} else {
		$ee = [  ];
	}
	return $ee;
}
function all_of(storage) {
	return peek_storage2(storage, (held) => {
		let out = [  ];
		let at3 = 0;
		const count2 = count(held);
		while (at3 < count2) {
			const $ej = item(held, at3);
			if ($ej[0] === 0) {
				out.push($ej[1]);
			}
			at3 = at3 + 1;
		}
		return out;
	});
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
function record_set(log, storage, leaving, at3, late) {
	if (late) {
		record2(log, [ 2, all_of(storage) ]);
		return;
	}
	const before2 = __list_get(leaving.v, 0);
	const after = peek_storage2(storage, (held) => {
		return item(held, at3);
	});
	const $ep = before2;
	let $eq = undefined;
	if ($ep[0] === 0 && (($eq = after), $eq[0] === 0)) {
		record2(log, [ 1, at3, __clone($ep[1]), __clone($eq[1]) ]);
	}
}
function release4(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $dR) {
	const leaving = __shared_new([  ]);
	const runs2 = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0]);
	const tap2 = tap(self, (change2) => {
		const $dS = change2;
		let $dT = null;
		if ($dS[0] === 0) {
			const at3 = $dS[1];
			const removed = $dS[2];
			const _inserted = $dS[3];
			leaving.v = arrived_at(storage, at3, removed);
			$dT = undefined;
		} else if ($dS[0] === 3) {
			const at4 = $dS[1];
			const count2 = $dS[2];
			leaving.v = arrived_at(storage, at4, count2);
			$dT = undefined;
		} else if ($dS[0] === 2) {
			const at5 = $dS[1];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at5);
			}));
			$dT = undefined;
		} else if ($dS[0] === 4) {
			const at6 = $dS[1];
			const _part = $dS[2];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at6);
			}));
			$dT = undefined;
		} else if ($dS[0] === 5) {
			const from = $dS[1];
			const to = $dS[2];
			$dT = extend_run(runs2, from, to);
		} else {
			$dT = undefined;
		}
		return $dT;
	}, (change2, stage) => {
		const $eh = stage;
		let $ei = null;
		if ($eh[0] === 2) {
			record2(log, [ 2, all_of(storage) ]);
			return;
			$ei = undefined;
		} else if ($eh[0] === 3) {
			return;
		} else {
			$ei = undefined;
		}
		$ei;
		const $em = stage;
		const late = $em[0] === 1;
		const $en = change2;
		let $eo = null;
		if ($en[0] === 0) {
			const at3 = $en[1];
			const removed = $en[2];
			const inserted = $en[3];
			if (late && removed > 0) {
				record2(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record2(log, [ 0, at3, [  ], arrived_at(storage, at3, inserted) ]);
			} else {
				record2(log, [ 0, at3, __clone(leaving.v), arrived_at(storage, at3, inserted) ]);
			}
			$eo = undefined;
		} else if ($en[0] === 1) {
			const at4 = $en[1];
			const count2 = $en[2];
			record2(log, [ 0, at4, [  ], arrived_at(storage, at4, count2) ]);
			$eo = undefined;
		} else if ($en[0] === 3) {
			const at5 = $en[1];
			const _count = $en[2];
			if (late) {
				record2(log, [ 2, all_of(storage) ]);
			} else {
				record2(log, [ 0, at5, __clone(leaving.v), [  ] ]);
			}
			$eo = undefined;
		} else if ($en[0] === 2) {
			const at6 = $en[1];
			$eo = record_set(log, storage, leaving, at6, late);
		} else if ($en[0] === 4) {
			const at7 = $en[1];
			const _part = $en[2];
			$eo = record_set(log, storage, leaving, at7, late);
		} else if ($en[0] === 5) {
			const from = $en[1];
			const to = $en[2];
			if (skipping.v > 0) {
				const $er = [ skipping, "v" ];
				skipping.v = $er[0][$er[1]] - 1;
				return;
			}
			if (runs2.v.length === 0) {
				record2(log, [ 3, from, 1, to ]);
				return;
			}
			const $es = __remove_at(runs2.v, 0, "std/src/reactive/lens.vl:1447:47");
			const start = $es[0];
			const count3 = $es[1];
			const first = $es[2];
			skipping.v = count3 - 1;
			let $et = null;
			if (first > start) {
				$et = first + 1 - count3;
			} else {
				$et = first;
			}
			const landing = $et;
			record2(log, [ 3, start, count3, landing ]);
			$eo = undefined;
		} else {
			$eo = record2(log, [ 2, all_of(storage) ]);
		}
		return $eo;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $dR);
	return feed;
}
function items(self, $dQ) {
	return items_into(self, new9(), $dQ);
}
function map(self, transform) {
	return [ self, transform ];
}
function cursor3(self) {
	return cursor(self[1]);
}
function all2(self) {
	return all_of(self[0]);
}
function since3(self, cursor4) {
	const $eA = since(self[1], cursor4);
	let $eB = null;
	if ($eA[0] === 0) {
		const recorded = $eA[1];
		$eB = recorded;
	} else {
		$eB = [ [ 2, all2(self[0]) ] ];
	}
	return $eB;
}
function reader2(self) {
	return (cursor4) => {
		return since3(self, cursor4);
	};
}
function get7(self) {
	return all2(self[0]);
}
function on_settle5(self, subscriber) {
	return on_settle(self[0][4], subscriber);
}
function drop_cursor3(self, cursor4) {
	drop_cursor(self[1], cursor4);
}
function open5(self) {
	const cursor4 = cursor3(self);
	const read = reader2(self);
	return [ () => {
		return get7(self);
	}, (subscriber) => {
		return on_settle5(self, subscriber);
	}, () => {
		return read(cursor4);
	}, () => {
		return drop_cursor3(self, cursor4);
	} ];
}
function seed(core, items2) {
	for (const item2 of items2) {
		const $eM = admit2(core, item2);
		const id = $eM[0];
		const value = $eM[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item2));
		core[2].v.push(value);
	}
}
function fresh_list(list2) {
	let copy = [  ];
	for (const item2 of list2) {
		copy.push(__clone(item2));
	}
	return copy;
}
function core_attach(core, upstream) {
	const consumer = core[7];
	const attach2 = upstream[1];
	return (subscriber) => {
		consumer.v = [ 0, __clone(subscriber) ];
		return attach2(subscriber);
	};
}
function held_pairs(core, at3, count2) {
	let pairs = [  ];
	let index = at3;
	while (index < at3 + count2) {
		pairs.push([ __clone(__at(core[1].v, index, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index, "std/src/reactive/delta.vl:1618:41")) ]);
		index = index + 1;
	}
	return pairs;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops) {
	let events = [  ];
	for (const op of ops) {
		const $eN = op;
		let $eO = null;
		if ($eN[0] === 0) {
			const at3 = $eN[1];
			const removed = $eN[2];
			const inserted = $eN[3];
			const held = core[0].v.length;
			const start = splice_start(held, at3);
			const taking = splice_count(held, start, removed.length);
			const left = held_pairs(core, start, taking);
			let taken = 0;
			while (taken < taking) {
				const id = __remove_at(core[0].v, start, "std/src/reactive/delta.vl:1642:32");
				retire2(core, id);
				__remove_at(core[1].v, start, "std/src/reactive/delta.vl:1644:37");
				__remove_at(core[2].v, start, "std/src/reactive/delta.vl:1645:39");
				taken = taken + 1;
			}
			let arrived = [  ];
			let offset = 0;
			for (const item2 of inserted) {
				const $eX = admit2(core, item2);
				const id2 = $eX[0];
				const value = $eX[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$eO = undefined;
		} else if ($eN[0] === 1) {
			const at4 = $eN[1];
			const _was = $eN[2];
			const item3 = $eN[3];
			const $eY = __list_get(core[0].v, at4);
			let $eZ = null;
			if ($eY[0] === 0) {
				const old_id = $eY[1];
				const before2 = [ __clone(__at(core[1].v, at4, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at4, "std/src/reactive/delta.vl:1663:44")) ];
				retire2(core, old_id);
				const $fa = admit2(core, item3);
				const id3 = $fa[0];
				const value2 = $fa[1];
				__at_put(core[0].v, at4, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at4, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at4, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at4, before2, [ __clone(item3), value2 ], true ]);
				$eZ = undefined;
			} else {
				$eZ = undefined;
			}
			$eO = $eZ;
		} else if ($eN[0] === 2) {
			const items2 = $eN[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire2(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$eO = undefined;
		} else {
			const from = $eN[1];
			const count2 = $eN[2];
			const to = $eN[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count2) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count2) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count2, to ]);
			$eO = undefined;
		}
		$eO;
	}
	const woke = __clone(core[4].v);
	let $ff = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index = 0;
		for (const id5 of core[0].v) {
			positions.set(hash2(id5), index);
			index = index + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash2(id6))) {
				continue;
			}
			seen.set(hash2(id6), true);
			const $fb = __map_get(positions, hash2(id6));
			let $fc = null;
			if ($fb[0] === 0) {
				const at5 = $fb[1];
				const $fd = __map_get(core[3].v, hash2(id6));
				let $fe = null;
				if ($fd[0] === 0) {
					const hold = $fd[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1731:19"));
						const before3 = __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at5, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at5, [ __clone(item4), before3 ], [ item4, now ], false ]);
					}
					$fe = undefined;
				} else {
					$fe = undefined;
				}
				$fc = $fe;
			} else {
				$fc = undefined;
			}
			$fc;
		}
		$ff = undefined;
	}
	$ff;
	return events;
}
function map2(self, fn) {
	let result = [  ];
	for (const item2 of self) {
		result.push(fn(item2));
	}
	return result;
}
function mapped_ops(events) {
	let ops = [  ];
	for (const event of events) {
		const $fg = event;
		let $fh = null;
		if ($fg[0] === 0) {
			const at3 = $fg[1];
			const left = $fg[2];
			const arrived = $fg[3];
			ops.push([ 0, at3, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$fh = undefined;
		} else if ($fg[0] === 1) {
			const at4 = $fg[1];
			const before2 = $fg[2];
			const after = $fg[3];
			const _rerun = $fg[4];
			ops.push([ 1, at4, __clone(before2[1]), __clone(after[1]) ]);
			$fh = undefined;
		} else if ($fg[0] === 2) {
			const _left = $fg[1];
			const now = $fg[2];
			$fh = ops.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $fg[1];
			const count2 = $fg[2];
			const to = $fg[3];
			$fh = ops.push([ 3, from, count2, to ]);
		}
		$fh;
	}
	return ops;
}
function release_core(core) {
	for (const id of __clone(core[0].v)) {
		retire2(core, id);
	}
	dispose2(core[6]);
}
function core_release(core, upstream) {
	const release5 = upstream[3];
	return () => {
		release_core(core);
		release5();
		return;
	};
}
function open6(self) {
	const transform = self[1];
	const upstream = open5(__clone(self[0]));
	const stage = new3();
	const core = element_core(stage, (item2) => {
		const $eE = run_element(stage, ($eC, $eD) => {
			return transform(item2, $eC, $eD);
		});
		const answer = $eE[0];
		const owner = $eE[1];
		return [ start_element_value(answer), owner ];
	});
	seed(core, upstream[0]());
	const drain2 = upstream[2];
	return [ () => {
		return fresh_list(core[2].v);
	}, core_attach(core, upstream), () => {
		return mapped_ops(resolve(core, drain2()));
	}, core_release(core, upstream) ];
}
function list_cell(elements, log) {
	return [ __shared_new(elements), new8(0, "std/src/reactive/delta.vl:792:68"), __clone(log) ];
}
function of2(elements) {
	return list_cell(elements, new9());
}
function size(self) {
	return self[0].v.length;
}
function splice_in_place(list2, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list2, start, "std/src/reactive/delta.vl:695:18"));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $fo = __list_get(inserted, offset);
		let $fp = null;
		if ($fo[0] === 0) {
			const value = $fo[1];
			$fp = __insert_at(list2, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$fp = undefined;
		}
		$fp;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function publish(cell, $fq) {
	set(cell[1], at(cell[2]), $fq, "std/src/reactive/delta.vl:799:17");
}
function splice(self, at3, removed, inserted, $fn) {
	const held = size(self);
	const start = splice_start(held, at3);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record2(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $fn);
}
function set_at(self, at3, value, $fr) {
	const $fs = __list_get(self[0].v, at3);
	let $ft = null;
	if ($fs[0] === 0) {
		const previous = $fs[1];
		__at_put(self[0].v, at3, __clone(value), "std/src/reactive/delta.vl:855:5");
		record2(self[2], [ 1, at3, previous, __clone(value) ]);
		publish(self, $fr);
		$ft = undefined;
	} else {
		$ft = undefined;
	}
	return $ft;
}
function set2(self, value, $fu, caller) {
	self[0].v = __clone(value);
	record2(self[2], [ 2, __clone(value) ]);
	publish(self, $fu);
}
function move_range(self, from, count2, to, $fv) {
	if (count2 === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count2) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:875:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $fw = __list_get(lifted, offset);
		let $fx = null;
		if ($fw[0] === 0) {
			const value = $fw[1];
			$fx = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$fx = undefined;
		}
		$fx;
		offset = offset + 1;
	}
	record2(self[2], [ 3, from, count2, to ]);
	publish(self, $fv);
}
function apply_ops(cell, ops, $fk) {
	for (const op of ops) {
		const $fl = op;
		let $fm = null;
		if ($fl[0] === 0) {
			const at3 = $fl[1];
			const removed = $fl[2];
			const inserted = $fl[3];
			$fm = splice(cell, at3, removed.length, inserted, $fk);
		} else if ($fl[0] === 1) {
			const at4 = $fl[1];
			const _was = $fl[2];
			const value = $fl[3];
			$fm = set_at(cell, at4, value, $fk);
		} else if ($fl[0] === 2) {
			const items2 = $fl[1];
			$fm = set2(cell, items2, $fk, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $fl[1];
			const count2 = $fl[2];
			const to = $fl[3];
			$fm = move_range(cell, from, count2, to, $fk);
		}
		$fm;
	}
}
function seal_collection(instance, owned, $fi, $fj, caller) {
	const sealed = of2(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $fi);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $fi, $fj);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $fi);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo3(self, $ew, $ex, caller) {
	return [ seal_collection(open6(self), true, $ew, $ex, caller) ];
}
function splice2(self, at3, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at3, "std/src/reactive/lens.vl:1992:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at3 + offset, __clone(value), "std/src/reactive/lens.vl:1997:9");
		offset = offset + 1;
	}
}
function audit(self, before2, changes) {
	const $fD = self[9].v;
	if ($fD[0] === 0) {

	} else {
		return;
	}
	const $fE = before2;
	if ($fE[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held) => {
		return $fD[1]($fE[1], held, changes);
	});
	const $fF = problem;
	if ($fF[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $fF[1], "std/src/reactive/lens.vl:713:4");
		})();
	}
}
function landed2(self, changes, late, $l) {
	if (changes.length === 0) {
		return;
	}
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	const tapped = self[7].v.length > 0;
	let index = 0;
	for (const change2 of changes) {
		const $fG = change2;
		let $fH = null;
		if ($fG[0] === 0) {
			const _at = $fG[1];
			const removed = $fG[2];
			const inserted = $fG[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$fH = undefined;
		} else if ($fG[0] === 1) {
			const _at2 = $fG[1];
			const _count = $fG[2];
			$fH = resized = true;
		} else if ($fG[0] === 3) {
			const _at3 = $fG[1];
			const _count2 = $fG[2];
			$fH = resized = true;
		} else if ($fG[0] === 6) {
			$fH = resized = true;
		} else if ($fG[0] === 5) {
			const _from = $fG[1];
			const _to = $fG[2];
			$fH = shaped = true;
		} else {
			$fH = undefined;
		}
		$fH;
		apply(self[1].v, change2, wakes);
		let $fX = null;
		if (tapped) {
			let $fW = null;
			if (!(late)) {
				$fW = [ 0 ];
			} else if (!(several)) {
				$fW = [ 1 ];
			} else if (index === 0) {
				$fW = [ 2 ];
			} else {
				$fW = [ 3 ];
			}
			const stage = $fW;
			for (const tap2 of __clone(self[7].v)) {
				tap2[2](change2, stage);
			}
			$fX = undefined;
		}
		$fX;
		index = index + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($z) => {
		for (const wake3 of wakes) {
			const $fY = wake3;
			let $fZ = null;
			if ($fY[0] === 0) {
				const slot2 = $fY[1];
				$fZ = wake_all2(cells, slot2, [ 0, $z ]);
			} else if ($fY[0] === 2) {
				const slot3 = $fY[1];
				$fZ = wake_all2(cells, slot3, [ 0, $z ]);
			} else {
				const slot4 = $fY[1];
				const part = $fY[2];
				$fZ = wake_part(cells, slot4, part, [ 0, $z ]);
			}
			$fZ;
		}
		if (resized) {
			notify(sized, [ 0, $z ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $z ]);
		}
		notify(published, [ 0, $z ]);
		return;
	}, $l);
}
function plan(self, change2, write, $fy) {
	const $fz = self[9].v;
	let $fA = null;
	if ($fz[0] === 0) {
		const _check = $fz[1];
		$fA = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$fA = [ 1 ];
	}
	const before2 = $fA;
	coming(self, [ __clone(change2) ]);
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, write(held) ];
	});
	audit(self, before2, [ __clone(change2) ]);
	landed2(self, [ __clone(change2) ], false, $fy);
	const $ga = out;
	let $gb = null;
	if ($ga[0] === 0) {
		const result = __clone($ga[1]);
		$gb = result;
	} else {
		$gb = (() => {
			throw __panic("lens: a planned write never ran", "std/src/reactive/lens.vl:703:12");
		})();
	}
	return $gb;
}
function get8(self) {
	return __clone(self[0].v);
}
function get9(self) {
	return get8(self[0]);
}
function get10(self) {
	return peek2(self, (held) => {
		return __clone(held);
	});
}
function on_settle6(self, subscriber) {
	return on_settle(self[4], subscriber);
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle6(self, mint_subscriber(() => {
		return observer(get10(self));
	}));
	if (immediately) {
		observer(get10(self));
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
function update(self, change2, $ge) {
	const $gf = self[9].v;
	let $gg = null;
	if ($gf[0] === 0) {
		const _check = $gf[1];
		$gg = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$gg = [ 1 ];
	}
	const before2 = $gg;
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, change2(held) ];
	});
	const $gh = out;
	let $gi = null;
	if ($gh[0] === 0) {
		const result = __clone($gh[1][0]);
		const changes = __clone($gh[1][1]);
		audit(self, before2, changes);
		landed2(self, changes, true, $ge);
		$gi = result;
	} else {
		$gi = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:684:12");
		})();
	}
	return $gi;
}
function plan2(self, change2, write, $fy) {
	const $go = self[9].v;
	let $gp = null;
	if ($go[0] === 0) {
		const _check = $go[1];
		$gp = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$gp = [ 1 ];
	}
	const before2 = $gp;
	coming(self, [ __clone(change2) ]);
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, write(held) ];
	});
	audit(self, before2, [ __clone(change2) ]);
	landed2(self, [ __clone(change2) ], false, $fy);
	const $gq = out;
	let $gr = null;
	if ($gq[0] === 0) {
		const result = __clone($gq[1]);
		$gr = result;
	} else {
		$gr = (() => {
			throw __panic("lens: a planned write never ran", "std/src/reactive/lens.vl:703:12");
		})();
	}
	return $gr;
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
}, (_id, _site) => {
	return;
} ]);
const waking = __shared_new([ 1 ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const delta_log_limit = 1024;
const element_holds_allocated_count = __shared_new(0);
const block_size = 128;
const ages = new7();
const people = keyed(ages);
put(people, "ada", 36, [ 1 ]);
put(people, "bob", 41, [ 1 ]);
const adults = memo(filter(entries(people, [ 1 ]), (_name, age, $aw, $ax) => {
	return age >= 40;
}), [ 1 ], [ 1 ], "lens-keyed.vl:16:63");
const next_year = memo2(map_values(entries(people, [ 1 ]), (age, $ct, $cu) => {
	return age + 1;
}), [ 1 ], [ 1 ], "lens-keyed.vl:17:66");
assert(len3(adults) === 1 && eq2(get4(adults, "bob"), [ 0, 41 ]), "filter seeded", "lens-keyed.vl:18:2");
assert(eq2(get4(next_year, "ada"), [ 0, 37 ]), "map seeded", "lens-keyed.vl:19:2");
put(people, "cy", 52, [ 1 ]);
put(people, "ada", 40, [ 1 ]);
remove2(people, "bob", [ 1 ]);
assert(len3(adults) === 2 && eq2(get4(adults, "ada"), [ 0, 40 ]) && eq2(get4(adults, "bob"), [ 1 ]), "filter followed put, replace and remove", "lens-keyed.vl:23:2");
assert(eq2(get4(next_year, "cy"), [ 0, 53 ]) && eq2(get4(next_year, "bob"), [ 1 ]), "map followed", "lens-keyed.vl:27:2");
const lengths = __shared_new(0);
on_change(length(people), (_n, $dj) => {
	const $dk = [ lengths, "v" ];
	return lengths.v = $dk[0][$dk[1]] + 1;
}, "lens-keyed.vl:31:27");
put(people, "ada", 41, [ 1 ]);
assert(lengths.v === 0 && get5(length(people)) === 2, "a replace leaves the length asleep", "lens-keyed.vl:33:2");
put(people, "dee", 29, [ 1 ]);
assert(lengths.v === 1 && get5(length(people)) === 3, "an arrival wakes it", "lens-keyed.vl:35:2");
const ada = slot(people, "ada", [ 1 ]);
const ada_runs = __shared_new(0);
on_change2(ada, (_v, $dx) => {
	const $dy = [ ada_runs, "v" ];
	return ada_runs.v = $dy[0][$dy[1]] + 1;
}, "lens-keyed.vl:40:15");
let wanted = new7();
insert(wanted, "ada", 41);
insert(wanted, "eve", 33);
reconcile_keyed(people, wanted, [ 1 ]);
assert(len(all(people)) === 2 && eq2(get(all(people), "eve"), [ 0, 33 ]) && eq2(get(all(people), "cy"), [ 1 ]), "the map became the new map", "lens-keyed.vl:45:2");
assert(ada_runs.v === 0, "an unchanged key did not wake", "lens-keyed.vl:54:2");
const list = sequence_checked([ "a", "b", "c", "d" ], [ 1 ]);
const mirror = memo3(map(items(list, [ 1 ]), (v, $eu, $ev) => {
	return v;
}), [ 1 ], [ 1 ], "lens-keyed.vl:58:39");
const took = plan(list, [ 3, 1, 2 ], (held) => {
	const gone = [ __at(held, 1, "lens-keyed.vl:60:15"), __at(held, 2, "lens-keyed.vl:60:24") ];
	splice2(held, 1, 2, [  ]);
	return gone;
}, [ 1 ]);
assert(eq(took, [ "b", "c" ]) && eq(all2(list), [ "a", "d" ]), "the planned write ran and answered", "lens-keyed.vl:64:2");
assert(eq(get9(mirror), [ "a", "d" ]), "the mirror followed it by one splice", "lens-keyed.vl:65:2");
const runs = __shared_new(0);
on_change3(__clone(list), (_all, $gc) => {
	const $gd = [ runs, "v" ];
	return runs.v = $gd[0][$gd[1]] + 1;
}, "lens-keyed.vl:69:20");
update(list, (held) => {
	__insert_at(held, 0, "z", "lens-keyed.vl:71:8");
	return [ undefined, [ [ 1, 0, 1 ] ] ];
}, [ 1 ]);
assert(eq(all2(list), [ "z", "a", "d" ]) && runs.v === 1, "an honest update lands", "lens-keyed.vl:74:2");
const before = [ "z", "a", "d" ];
const $gj = explain_sequence(before, [ "y", "z", "a", "d" ], [ [ 1, 2, 1 ] ]);
assert($gj[0] === 0, "a wrong position is caught", "lens-keyed.vl:78:2");
const $gk = explain_sequence(before, [ "y", "z", "a", "d" ], [ [ 1, 0, 2 ] ]);
assert($gk[0] === 0, "a wrong count is caught", "lens-keyed.vl:82:2");
const $gl = explain_sequence(before, [ "z", "A", "d" ], [ [ 4, 2, "x" ] ]);
assert($gl[0] === 0, "an edit at the wrong row is caught", "lens-keyed.vl:86:2");
assert(eq2(explain_sequence(before, [ "y", "z", "a", "d" ], [ [ 1, 0, 1 ] ]), [ 1 ]), "the honest change passes", "lens-keyed.vl:90:2");
assert(eq2(explain_sequence(before, [ "z", "p", "q", "d" ], [ [ 0, 1, 1, 2 ] ]), [ 1 ]), "an honest splice passes", "lens-keyed.vl:98:2");
const $gm = explain_sequence(before, [ "z", "p", "q", "d" ], [ [ 0, 1, 1, 1 ] ]);
assert($gm[0] === 0, "a splice with the wrong count is caught", "lens-keyed.vl:105:2");
const $gn = explain_sequence(before, [ "z", "p", "q", "d" ], [ [ 0, 2, 1, 2 ] ]);
assert($gn[0] === 0, "a splice at the wrong address is caught", "lens-keyed.vl:109:2");
plan2(list, [ 0, 1, 1, 2 ], (held) => {
	splice2(held, 1, 1, [ "p", "q" ]);
	return;
}, [ 1 ]);
assert(eq(all2(list), [ "z", "p", "q", "d" ]) && eq(get9(mirror), [ "z", "p", "q", "d" ]), "a planned splice is one exact op", "lens-keyed.vl:117:2");
console.log("keyed: all passed");
//# sourceMappingURL=lens-keyed.mjs.map
