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
	let $dq = null;
	if (!(condition)) {
		$dq = (() => {
			throw __panic(message, caller);
		})();
	}
	return $dq;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $x = null;
	if (wrapped < 0) {
		$x = wrapped + modulus;
	} else {
		$x = wrapped;
	}
	return $x;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $S = null;
	if (truncated > 0) {
		$S = truncated;
	} else {
		$S = 0;
	}
	return $S;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $y = null;
	if (wrapped >= half) {
		$y = wrapped - modulus;
	} else {
		$y = wrapped;
	}
	return $y;
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
	const $dl = self;
	let $do = null;
	if ($dl[0] === 0) {
		const $dm = b;
		let $dn = null;
		if ($dm[0] === 0) {
			$dn = $dl[1] === $dm[1];
		} else {
			$dn = false;
		}
		$do = $dn;
	} else {
		const $dp = b;
		$do = $dp[0] === 1;
	}
	return $do;
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
		let $J = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$J = undefined;
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
		$J;
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
	const $bj = turn;
	let $bk = null;
	if ($bj[0] === 0) {
		const ambient = $bj[1];
		$bk = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bl = last(draining_turns.v);
		let $bm = null;
		if ($bl[0] === 0) {
			const draining = $bl[1];
			$bm = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bm = undefined;
		}
		$bk = $bm;
	}
	return $bk;
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
function dispose(self, $cA) {
	const $cB = $cA;
	let $cC = null;
	if ($cB[0] === 0) {
		const established = $cB[1];
		$cC = [ 0, established ];
	} else {
		$cC = last(draining_turns.v);
	}
	const ambient = $cC;
	release_under(self, ambient);
}
function detach(handle) {
	const $bx = last(releasing_turns.v);
	let $by = null;
	if ($bx[0] === 0) {
		const at_release = $bx[1];
		$by = at_release;
	} else {
		$by = last(draining_turns.v);
	}
	const turn = $by;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $bz = [ 0, handle[0] ];
	let $bA = null;
	if ($bz[0] === 0) {
		const subscribers = $bz[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$bA = undefined;
	} else {
		$bA = undefined;
	}
	$bA;
	const $bB = ambient;
	let $bC = null;
	if ($bB[0] === 0) {
		const turn = $bB[1];
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
		$bC = undefined;
	} else {
		$bC = undefined;
	}
	$bC;
	const $bD = handle[3].v;
	let $bE = null;
	if ($bD[0] === 0) {
		const release5 = $bD[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release5, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$bE = undefined;
	} else {
		$bE = undefined;
	}
	return $bE;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aF = null;
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
		$aF = undefined;
	}
	return $aF;
}
function stage_nursery(self) {
	const $aR = nursery(self);
	let $aS = null;
	if ($aR[0] === 0) {
		const held = $aR[1];
		$aS = held;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$aS = made;
	}
	return $aS;
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
	let $aQ = null;
	if (is_disposed(self)) {
		$aQ = [ 1 ];
	} else {
		$aQ = self[0].v[3];
	}
	return $aQ;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $bQ = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $bH = held[3];
		let $bI = null;
		if ($bH[0] === 0) {
			const nursery2 = $bH[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$bI = undefined;
		} else {
			$bI = undefined;
		}
		$bI;
		let $bP = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $bK = __guarded(cleanup);
				let $bL = null;
				if ($bK[0] === 0) {
					const message = $bK[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$bL = undefined;
				} else {
					$bL = undefined;
				}
				$bL;
			}
			const $bN = failure;
			let $bO = null;
			if ($bN[0] === 0) {
				const message2 = $bN[1];
				$bO = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$bO = undefined;
			}
			$bP = $bO;
		}
		$bQ = $bP;
	}
	return $bQ;
}
function register_with_owner(subscription, $cv, $cw) {
	const $cx = $cw;
	let $cy = null;
	if ($cx[0] === 0) {
		const owner = $cx[1];
		$cy = take(owner, subscription, $cv);
	} else {
		$cy = __clone(subscription);
	}
	return $cy;
}
function defer_to_owner(cleanup, $aC) {
	const $aD = $aC;
	let $aE = null;
	if ($aD[0] === 0) {
		const owner = $aD[1];
		$aE = defer(owner, cleanup);
	} else {
		$aE = undefined;
	}
	return $aE;
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
		const $ct = previous;
		let $cu = null;
		if ($ct[0] === 0) {
			const earlier = $ct[1];
			$cu = earlier();
		} else {
			$cu = undefined;
		}
		return $cu;
	} ];
}
function settle_together(body, $Y) {
	const $Z = $Y;
	let $aa = null;
	if ($Z[0] === 0) {
		const current = $Z[1];
		$aa = body(current);
	} else {
		const $ab = last(draining_turns.v);
		let $ac = null;
		if ($ab[0] === 0) {
			const draining = $ab[1];
			$ac = body(draining);
		} else {
			$ac = batch(body, $Y);
		}
		$aa = $ac;
	}
	return $aa;
}
function splice_start(held, at3) {
	let $eZ = null;
	if (at3 > held) {
		$eZ = held;
	} else {
		$eZ = at3;
	}
	return $eZ;
}
function splice_count(held, start, removed) {
	let $fa = null;
	if (start + removed > held) {
		$fa = held - start;
	} else {
		$fa = removed;
	}
	return $fa;
}
function new4() {
	const table = new Map();
	const identities = new Map();
	return [ __shared_new(table), __shared_new(identities) ];
}
function wake2(self, hash3, $co) {
	const $cp = __map_get(self[0].v, hash3);
	let $cq = null;
	if ($cp[0] === 0) {
		const slot2 = $cp[1];
		$cq = notify(slot2[0], $co);
	} else {
		$cq = undefined;
	}
	return $cq;
}
function wake_all(self, $cn) {
	for (const slot2 of __map_values(self[0].v)) {
		notify(slot2[0], $cn);
	}
}
function new5() {
	return [ [  ] ];
}
function apply(self, change2, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change2, wakes);
	}
}
function seat(self, at3) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:213:7") <= at3) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at3 - __at(self[3], low, "std/src/reactive/lens.vl:219:14") ];
}
function restart(self) {
	let starts = [  ];
	let running = 0;
	let place = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place, "std/src/reactive/lens.vl:229:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:230:15").length;
		place = place + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:239:14").length;
	while (index < held) {
		const slot2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index, "std/src/reactive/lens.vl:241:15");
		if (slot2 !== -(1)) {
			__at_put(self[4], as_usize(slot2), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot2), index, "std/src/reactive/lens.vl:244:5");
		}
		index = index + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:255:14");
	return block;
}
function widen(self, at3, count2) {
	if (count2 === 0 || at3 >= self[6]) {
		return;
	}
	const $fW = seat(self, at3);
	const place = $fW[0];
	const index = $fW[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $fX = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$fX = undefined;
	} else {
		const before2 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:277:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before2.length;
		while (copied < held) {
			grown.push(__at(before2, copied, "std/src/reactive/lens.vl:287:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:290:4");
	}
	$fX;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at3, count2, wakes) {
	let left = count2;
	while (left > 0 && at3 < self[6]) {
		const $fU = seat(self, at3);
		const place = $fU[0];
		const index = $fU[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $fV = null;
		if (left < held - index) {
			$fV = left;
		} else {
			$fV = held - index;
		}
		const take2 = $fV;
		let taken = 0;
		while (taken < take2) {
			const slot2 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index, "std/src/reactive/lens.vl:311:46");
			if (slot2 !== -(1)) {
				__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot2 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:320:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:321:16");
		} else {
			renumber(self, block, index);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:331:15");
	const all3 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all3, index, "std/src/reactive/lens.vl:336:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held = all3.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all3, index, "std/src/reactive/lens.vl:346:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function apply2(self, change2, wakes) {
	const $fS = change2;
	let $fT = null;
	if ($fS[0] === 0) {
		const at3 = $fS[1];
		const removed = $fS[2];
		const inserted = $fS[3];
		close(self, at3, removed, wakes);
		widen(self, at3, inserted);
		$fT = undefined;
	} else if ($fS[0] === 1) {
		const at4 = $fS[1];
		const count2 = $fS[2];
		$fT = widen(self, at4, count2);
	} else if ($fS[0] === 3) {
		const at5 = $fS[1];
		const count3 = $fS[2];
		$fT = close(self, at5, count3, wakes);
	} else if ($fS[0] === 2) {
		const at6 = $fS[1];
		let $fZ = null;
		if (at6 < self[6]) {
			const $fY = seat(self, at6);
			const place = $fY[0];
			const index = $fY[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index, "std/src/reactive/lens.vl:396:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$fZ = undefined;
		}
		$fT = $fZ;
	} else if ($fS[0] === 4) {
		const at7 = $fS[1];
		const part = $fS[2];
		let $gb = null;
		if (at7 < self[6]) {
			const $ga = seat(self, at7);
			const place2 = $ga[0];
			const index2 = $ga[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index2, "std/src/reactive/lens.vl:405:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part ]);
			}
			$gb = undefined;
		}
		$fT = $gb;
	} else if ($fS[0] === 5) {
		const from = $fS[1];
		const to = $fS[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $gc = null;
		if (from > to) {
			$gc = from;
		} else {
			$gc = to;
		}
		const far = $gc;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $gd = seat(self, from);
		const left = $gd[0];
		const index3 = $gd[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index3, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $ge = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$ge = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$ge = seat(self, to);
		}
		const $gf = $ge;
		const place3 = $gf[0];
		const landing = $gf[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot4, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$fT = undefined;
	} else {
		let slot5 = 0;
		const slots = as_i32(self[4].length);
		while (slot5 < slots) {
			if (__at(self[4], as_usize(slot5), "std/src/reactive/lens.vl:455:9") !== -(1)) {
				__at_put(self[4], as_usize(slot5), -(1), "std/src/reactive/lens.vl:456:7");
				wakes.push([ 2, slot5 ]);
			}
			slot5 = slot5 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$fT = undefined;
	}
	return $fT;
}
function new6() {
	return [ new7(), [  ] ];
}
function extend_run(runs2, from, to) {
	const held = runs2.v.length;
	let $eq = null;
	if (held > 0) {
		const $ep = __at(runs2.v, held - 1, "std/src/reactive/lens.vl:1989:31");
		const start = $ep[0];
		const count2 = $ep[1];
		const first = $ep[2];
		const leftward = first < start && from === start + count2 && to === first + count2;
		const rightward = first > start && from === start && to === first;
		if (leftward || rightward) {
			__at_put(runs2.v, held - 1, [ start, count2 + 1, first ], "std/src/reactive/lens.vl:1993:4");
			return;
		}
		$eq = undefined;
	}
	$eq;
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
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function keyed(value) {
	return over(value, new6(), [ (held, key) => {
		const $a = find(held, key);
		return $a[0] === 0;
	}, (held, key) => {
		return find(held, key);
	}, (held, key, read) => {
		const $b = find(held, key);
		const item2 = [ $b[1] ];
		if ($b[0] === 0) {
			read([ item2, 0 ]);
		}
		return;
	}, (held, key, value2) => {
		return store(held, key, value2);
	}, (held, key, with2) => {
		const $c = find(held, key);
		if ($c[0] === 0) {

		} else {
			return;
		}
		let item2 = [ $c[1] ];
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
	const $f = storage;
	let $g = null;
	if ($f[0] === 0) {
		const cell = $f[1];
		$g = viewing(cell, read);
	} else {
		const port = $f[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $h = out;
		let $i = null;
		if ($h[0] === 0) {
			const value = __clone($h[1]);
			$i = value;
		} else {
			$i = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$g = $i;
	}
	return $g;
}
function peek(self, read) {
	return peek_storage(self[0].v[0], read);
}
function coming_one(self, change2) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		tap2[1](change2);
	}
}
function mutate(self, change2) {
	const $k = self[0].v[0];
	let $l = null;
	if ($k[0] === 0) {
		const cell = $k[1];
		$l = change2(cell.v);
	} else {
		const port = __clone($k[1]);
		$l = port[1](change2);
	}
	return $l;
}
function apply3(self, change2, wakes) {
	const $q = change2;
	let $r = null;
	if ($q[0] === 0) {
		const key = $q[1];
		const _removed = $q[2];
		const _inserted = $q[3];
		const $s = get(self[0], key);
		if ($s[0] === 0) {
			wakes.push([ 0, $s[1] ]);
		}
		$r = undefined;
	} else if ($q[0] === 1) {
		const key2 = $q[1];
		const _count = $q[2];
		const $t = get(self[0], key2);
		if ($t[0] === 0) {
			wakes.push([ 0, $t[1] ]);
		}
		$r = undefined;
	} else if ($q[0] === 2) {
		const key3 = $q[1];
		const $u = get(self[0], key3);
		if ($u[0] === 0) {
			wakes.push([ 0, $u[1] ]);
		}
		$r = undefined;
	} else if ($q[0] === 3) {
		const key4 = $q[1];
		const _count2 = $q[2];
		const $v = get(self[0], key4);
		if ($v[0] === 0) {
			wakes.push([ 0, $v[1] ]);
		}
		$r = undefined;
	} else if ($q[0] === 4) {
		const key5 = $q[1];
		const part = $q[2];
		const $w = get(self[0], key5);
		if ($w[0] === 0) {
			wakes.push([ 1, $w[1], part ]);
		}
		$r = undefined;
	} else if ($q[0] === 5) {
		const _from = $q[1];
		const _to = $q[2];
		$r = undefined;
	} else {
		let slot2 = 0;
		const held = as_i32(self[1].length);
		while (slot2 < held) {
			wakes.push([ 0, slot2 ]);
			slot2 = slot2 + 1;
		}
		$r = undefined;
	}
	return $r;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $K = null;
	if (is_empty(self)) {
		$K = [ 1 ];
	} else {
		$K = __list_get(self, self.length - 1);
	}
	return $K;
}
function notify(self, $F) {
	const traced = tracing.v;
	let $G = null;
	if (traced) {
		$G = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$G = "";
	}
	const cause = $G;
	const $H = $F;
	let $I = null;
	if ($H[0] === 0) {
		const turn = $H[1];
		$I = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $L = last(draining_turns.v);
		let $M = null;
		if ($L[0] === 0) {
			const draining = $L[1];
			$M = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $N = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$N = undefined;
				}
				$N;
			}
			$M = undefined;
		}
		$I = $M;
	}
	return $I;
}
function viewing2(cell, read) {
	return lend(cell.v, read);
}
function wake_all2(watch, slot2, $R) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1117:36"));
	});
	const $T = held[0];
	if ($T[0] === 0) {
		notify($T[1], $R);
	}
	for (const part of held[1]) {
		notify(part[1], $R);
	}
}
function wake_part(watch, slot2, name, $U) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1127:36"));
	});
	const $V = held[0];
	if ($V[0] === 0) {
		notify($V[1], $U);
	}
	for (const part of held[1]) {
		if (part[0] === name) {
			notify(part[1], $U);
		}
	}
}
function batch(body, $ad) {
	const $ae = $ad;
	let $af = null;
	if ($ae[0] === 0) {
		const current = $ae[1];
		$af = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$af = result;
	}
	return $af;
}
function landed(self, changes, late, $n) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change2 of changes) {
		const $o = change2;
		let $p = null;
		if ($o[0] === 0) {
			const _at = $o[1];
			const removed = $o[2];
			const inserted = $o[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$p = undefined;
		} else if ($o[0] === 1) {
			const _at2 = $o[1];
			const _count = $o[2];
			$p = resized = true;
		} else if ($o[0] === 3) {
			const _at3 = $o[1];
			const _count2 = $o[2];
			$p = resized = true;
		} else if ($o[0] === 6) {
			$p = resized = true;
		} else if ($o[0] === 5) {
			const _from = $o[1];
			const _to = $o[2];
			$p = shaped = true;
		} else {
			$p = undefined;
		}
		$p;
		if (watch.v[1].length > 0) {
			apply3(watch.v[0], change2, wakes);
		}
		let $A = null;
		if (self[0].v[4].length > 0) {
			let $z = null;
			if (!(late)) {
				$z = [ 0 ];
			} else if (!(several)) {
				$z = [ 1 ];
			} else if (index === 0) {
				$z = [ 2 ];
			} else {
				$z = [ 3 ];
			}
			const stage = $z;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$A = undefined;
		}
		$A;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $B = null;
	if (shaped || resized) {
		$B = watch.v[2];
	} else {
		$B = [ 1 ];
	}
	const shape = __clone($B);
	let $C = null;
	if (resized) {
		$C = watch.v[3];
	} else {
		$C = [ 1 ];
	}
	const sized = __clone($C);
	let $D = undefined;
	let $E = undefined;
	if (wakes.length === 0 && (($D = shape), $D[0] === 1) && (($E = sized), $E[0] === 1)) {
		notify(published, $n);
		return;
	}
	settle_together(($O) => {
		for (const wake3 of wakes) {
			const $P = wake3;
			let $Q = null;
			if ($P[0] === 0) {
				const slot2 = $P[1];
				$Q = wake_all2(watch, slot2, [ 0, $O ]);
			} else if ($P[0] === 2) {
				const slot3 = $P[1];
				$Q = wake_all2(watch, slot3, [ 0, $O ]);
			} else {
				const slot4 = $P[1];
				const part = $P[2];
				$Q = wake_part(watch, slot4, part, [ 0, $O ]);
			}
			$Q;
		}
		const $W = sized;
		if ($W[0] === 0) {
			notify($W[1], [ 0, $O ]);
		}
		const $X = shape;
		if ($X[0] === 0) {
			notify($X[1], [ 0, $O ]);
		}
		notify(published, [ 0, $O ]);
		return;
	}, $n);
}
function landed_one(self, change2, $m) {
	landed(self, [ __clone(change2) ], false, $m);
}
function put(self, key, value, $d) {
	const had = peek(self, (held) => {
		const $e = find(held, key);
		return $e[0] === 0;
	});
	let $j = null;
	if (had) {
		$j = [ 2, __clone(key) ];
	} else {
		$j = [ 1, __clone(key), 1 ];
	}
	const change2 = $j;
	coming_one(self, change2);
	mutate(self, (held) => {
		return store(held, key, value);
	});
	landed_one(self, change2, $d);
}
function new9() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function peek_storage2(storage, read) {
	const $ah = storage;
	let $ai = null;
	if ($ah[0] === 0) {
		const cell = $ah[1];
		$ai = viewing2(cell, read);
	} else {
		const port = $ah[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $aj = out;
		let $ak = null;
		if ($aj[0] === 0) {
			const value = __clone($aj[1]);
			$ak = value;
		} else {
			$ak = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$ai = $ak;
	}
	return $ai;
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
			const $ar = __list_get(held, index);
			let $as = null;
			if ($ar[0] === 0) {
				const op = $ar[1];
				$as = kept.push(op);
			} else {
				$as = undefined;
			}
			$as;
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
	const $at = now;
	if ($at[0] === 0) {
		const $av = __clone(key);
		let $au = null;
		if (late) {
			$au = [ 1 ];
		} else {
			$au = leaving.v;
		}
		record(log, [ 0, $av, __clone($au), __clone($at[1]) ]);
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
			const $aw = find(held, key);
			if ($aw[0] === 0) {
				insert(out, key, $aw[1]);
			}
		}
		return out;
	});
}
function tap(self, before2, after) {
	const id = self[0].v[5];
	self[0].v[5] = id + 1;
	self[0].v[4].push([ id, before2, after ]);
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
function release(self) {
	untap(self[0], self[2]);
}
function entries(self, $ag) {
	const log = new9();
	const leaving = __shared_new([ 1 ]);
	const storage = __clone(self[0].v[0]);
	const find2 = (key) => {
		return peek_storage2(storage, (held) => {
			return find(held, key);
		});
	};
	const tap2 = tap(self, (change2) => {
		const $al = change2;
		let $am = null;
		if ($al[0] === 0) {
			const key = $al[1];
			const _removed = $al[2];
			const _inserted = $al[3];
			$am = leaving.v = find2(key);
		} else if ($al[0] === 3) {
			const key2 = $al[1];
			const _count = $al[2];
			$am = leaving.v = find2(key2);
		} else if ($al[0] === 2) {
			const key3 = $al[1];
			$am = leaving.v = find2(key3);
		} else if ($al[0] === 4) {
			const key4 = $al[1];
			const _part = $al[2];
			$am = leaving.v = find2(key4);
		} else {
			$am = undefined;
		}
		return $am;
	}, (change2, stage) => {
		const $an = stage;
		const late = !($an[0] === 0);
		const $ao = change2;
		let $ap = null;
		if ($ao[0] === 1) {
			const key = $ao[1];
			const _count = $ao[2];
			const $aq = find2(key);
			if ($aq[0] === 0) {
				record(log, [ 0, __clone(key), [ 1 ], $aq[1] ]);
			}
			$ap = undefined;
		} else if ($ao[0] === 0) {
			const key2 = $ao[1];
			const _removed = $ao[2];
			const _inserted = $ao[3];
			$ap = record_put(log, key2, find2(key2), leaving, late);
		} else if ($ao[0] === 2) {
			const key3 = $ao[1];
			$ap = record_put(log, key3, find2(key3), leaving, late);
		} else if ($ao[0] === 4) {
			const key4 = $ao[1];
			const _part = $ao[2];
			$ap = record_put(log, key4, find2(key4), leaving, late);
		} else if ($ao[0] === 3) {
			const key5 = $ao[1];
			const _count2 = $ao[2];
			if (late) {
				record(log, [ 2, entries_of(storage) ]);
			} else {
				const $aB = leaving.v;
				if ($aB[0] === 0) {
					record(log, [ 1, __clone(key5), $aB[1] ]);
				}
			}
			$ap = undefined;
		} else if ($ao[0] === 5) {
			const _from = $ao[1];
			const _to = $ao[2];
			$ap = undefined;
		} else {
			$ap = record(log, [ 2, entries_of(storage) ]);
		}
		return $ap;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release(feed);
	}, $ag);
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
		const $aK = __list_get(held, index);
		let $aL = null;
		if ($aK[0] === 0) {
			const op = $aK[1];
			$aL = ops.push(op);
		} else {
			$aL = undefined;
		}
		$aL;
		index = index + 1;
	}
	return [ 0, ops ];
}
function all(self) {
	return entries_of(self[0].v[0]);
}
function since2(self, cursor4) {
	const $aM = since(self[1], cursor4);
	let $aN = null;
	if ($aM[0] === 0) {
		const recorded = $aM[1];
		$aN = recorded;
	} else {
		$aN = [ [ 2, all(self[0]) ] ];
	}
	return $aN;
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
	const value = (($aT) => {
		return (($aU) => {
			return body($aT, $aU);
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
	const holds = new Map();
	const dirty = [  ];
	const consumer = [ 1 ];
	return [ __shared_new(ids), __shared_new(items2), __shared_new(values), __shared_new(holds), __shared_new(dirty), __shared_new(0), __clone(stage), __shared_new(consumer), run ];
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
		const $aW = __list_pop(keys2);
		let $aX = null;
		if ($aW[0] === 0) {
			const key = $aW[1];
			const $aY = __list_pop(values);
			let $aZ = null;
			if ($aY[0] === 0) {
				const value = $aY[1];
				$aZ = backwards.push([ key, value ]);
			} else {
				$aZ = undefined;
			}
			$aX = $aZ;
		} else {
			$aX = undefined;
		}
		$aX;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $ba = __list_pop(backwards);
		let $bb = null;
		if ($ba[0] === 0) {
			const pair = $ba[1];
			$bb = result.push(pair);
		} else {
			$bb = undefined;
		}
		$bb;
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
	const $bc = core[8](item2);
	const answer = $bc[0];
	const owner = $bc[1];
	const $bd = answer;
	let $be = null;
	if ($bd[0] === 0) {
		const value = $bd[1];
		const $bf = owner;
		let $bg = null;
		if ($bf[0] === 0) {
			const held = $bf[1];
			$bg = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$bg = undefined;
		}
		$bg;
		$be = [ id, __clone(value) ];
	} else {
		const instance = $bd[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $bh = consumer.v;
			let $bi = null;
			if ($bh[0] === 0) {
				const downstream = $bh[1];
				$bi = wake(downstream);
			} else {
				$bi = undefined;
			}
			return $bi;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$be = [ id, pull() ];
	}
	return $be;
}
function keyed_admit(keyed2, key, input) {
	const $bn = admit(keyed2[0], [ __clone(key), __clone(input) ]);
	const id = $bn[0];
	const value = $bn[1];
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
	const $bJ = self;
	return $bJ[0] === 1;
}
function retire(core, id) {
	const $bs = __map_get(core[3].v, hash2(id));
	let $bt = null;
	if ($bs[0] === 0) {
		const hold = $bs[1];
		core[3].v.delete(hash2(id));
		const $bu = hold[2];
		let $bv = null;
		if ($bu[0] === 0) {
			const handle = $bu[1];
			$bv = detach(handle);
		} else {
			$bv = undefined;
		}
		$bv;
		hold[3]();
		const $bF = hold[4];
		let $bG = null;
		if ($bF[0] === 0) {
			const owner = $bF[1];
			$bG = dispose2(owner);
		} else {
			$bG = undefined;
		}
		$bt = $bG;
	} else {
		$bt = undefined;
	}
	return $bt;
}
function keyed_retire(keyed2, row) {
	keyed2[2].v.delete(hash2(row[3]));
	retire(keyed2[0], row[3]);
}
function keyed_resolve(keyed2, ops) {
	let events = [  ];
	for (const op of ops) {
		const $bo = op;
		let $bp = null;
		if ($bo[0] === 0) {
			const key = $bo[1];
			const _was = $bo[2];
			const input = $bo[3];
			const $bq = __map_get(keyed2[1].v, hash(key));
			let $br = null;
			if ($bq[0] === 0) {
				const row = $bq[1];
				keyed_retire(keyed2, row);
				const fresh = keyed_admit(keyed2, key, input);
				events.push([ 1, __clone(key), [ __clone(row[1]), __clone(row[2]) ], [ __clone(fresh[1]), __clone(fresh[2]) ], true ]);
				$br = undefined;
			} else {
				const fresh2 = keyed_admit(keyed2, key, input);
				events.push([ 0, __clone(key), __clone(fresh2[1]), __clone(fresh2[2]) ]);
				$br = undefined;
			}
			$bp = $br;
		} else if ($bo[0] === 1) {
			const key2 = $bo[1];
			const _gone = $bo[2];
			const hash3 = hash(key2);
			const $bR = __map_get(keyed2[1].v, hash3);
			let $bS = null;
			if ($bR[0] === 0) {
				const row2 = $bR[1];
				keyed_retire(keyed2, row2);
				keyed2[1].v.delete(hash3);
				events.push([ 2, __clone(key2), __clone(row2[1]), __clone(row2[2]) ]);
				$bS = undefined;
			} else {
				$bS = undefined;
			}
			$bp = $bS;
		} else {
			const map3 = $bo[1];
			for (const row3 of keyed_rows(keyed2)) {
				keyed_retire(keyed2, row3);
			}
			const emptied = new Map();
			keyed2[1].v = emptied;
			keyed_seed(keyed2, map3);
			events.push([ 3, keyed_rows(keyed2) ]);
			$bp = undefined;
		}
		$bp;
	}
	const woke = __clone(keyed2[0][4].v);
	let $bZ = null;
	if (woke.length > 0) {
		keyed2[0][4].v = [  ];
		let seen = new Map();
		for (const id of woke) {
			if (seen.has(hash2(id))) {
				continue;
			}
			seen.set(hash2(id), true);
			const $bT = __map_get(keyed2[2].v, hash2(id));
			let $bU = null;
			if ($bT[0] === 0) {
				const hash4 = $bT[1];
				const $bV = __map_get(keyed2[1].v, hash4);
				let $bW = null;
				if ($bV[0] === 0) {
					const row4 = $bV[1];
					const $bX = __map_get(keyed2[0][3].v, hash2(id));
					let $bY = null;
					if ($bX[0] === 0) {
						const hold = $bX[1];
						if (hold[0]) {
							const now = hold[1]();
							keyed2[1].v.set(hash4, [ __clone(row4[0]), __clone(row4[1]), __clone(now), row4[3] ]);
							events.push([ 1, __clone(row4[0]), [ __clone(row4[1]), __clone(row4[2]) ], [ __clone(row4[1]), now ], false ]);
						}
						$bY = undefined;
					} else {
						$bY = undefined;
					}
					$bW = $bY;
				} else {
					$bW = undefined;
				}
				$bU = $bW;
			} else {
				$bU = undefined;
			}
			$bU;
		}
		$bZ = undefined;
	}
	$bZ;
	return events;
}
function filtered_map_ops(events) {
	let produced = [  ];
	for (const event of events) {
		const $ca = event;
		let $cb = null;
		if ($ca[0] === 0) {
			const key = $ca[1];
			const input = $ca[2];
			const kept = $ca[3];
			if (kept) {
				produced.push([ 0, __clone(key), [ 1 ], __clone(input) ]);
			}
			$cb = undefined;
		} else if ($ca[0] === 1) {
			const key2 = $ca[1];
			const before2 = $ca[2];
			const after = $ca[3];
			const rerun = $ca[4];
			let $cc = null;
			if (before2[1] && after[1]) {
				if (rerun) {
					produced.push([ 0, __clone(key2), [ 0, __clone(before2[0]) ], __clone(after[0]) ]);
				}
				$cc = undefined;
			} else if (before2[1]) {
				produced.push([ 1, __clone(key2), __clone(before2[0]) ]);
			} else if (after[1]) {
				produced.push([ 0, __clone(key2), [ 1 ], __clone(after[0]) ]);
			}
			$cb = $cc;
		} else if ($ca[0] === 2) {
			const key3 = $ca[1];
			const input2 = $ca[2];
			const kept2 = $ca[3];
			if (kept2) {
				produced.push([ 1, __clone(key3), __clone(input2) ]);
			}
			$cb = undefined;
		} else {
			const rows = $ca[1];
			$cb = produced.push([ 2, kept_map(rows) ]);
		}
		$cb;
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
		const $aV = run_element(stage, ($aO, $aP) => {
			return keep(entry[0], entry[1], $aO, $aP);
		});
		const answer = $aV[0];
		const owner = $aV[1];
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
function wake_for(slots, ops, $ck) {
	let named = new Map();
	let order = [  ];
	for (const op of ops) {
		const $cl = op;
		let $cm = null;
		if ($cl[0] === 0) {
			const key = $cl[1];
			const _was = $cl[2];
			const _now = $cl[3];
			$cm = hash(key);
		} else if ($cl[0] === 1) {
			const key2 = $cl[1];
			const _gone = $cl[2];
			$cm = hash(key2);
		} else {
			const _map = $cl[1];
			wake_all(slots, $ck);
			return;
			$cm = undefined;
		}
		const hash3 = $cm;
		if (!(named.has(hash3))) {
			named.set(hash3, true);
			order.push(hash3);
		}
	}
	for (const hash4 of order) {
		wake2(slots, hash4, $ck);
	}
}
function at(self) {
	return self[1].v;
}
function set(self, value, $cs, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $cs);
}
function publish_map(cell, $cr) {
	set(cell[1], at(cell[2]), $cr, "std/src/reactive/hash_map_cell.vl:252:17");
}
function apply_map_ops(cell, ops, $cf) {
	let recorded = [  ];
	for (const op of ops) {
		const $cg = op;
		let $ch = null;
		if ($cg[0] === 0) {
			const key = $cg[1];
			const _claimed = $cg[2];
			const now = $cg[3];
			const previous = get(cell[0].v, key);
			insert(cell[0].v, key, now);
			recorded.push([ 0, __clone(key), previous, __clone(now) ]);
			$ch = undefined;
		} else if ($cg[0] === 1) {
			const key2 = $cg[1];
			const _claimed2 = $cg[2];
			const $ci = get(cell[0].v, key2);
			let $cj = null;
			if ($ci[0] === 0) {
				const gone = $ci[1];
				remove(cell[0].v, key2);
				recorded.push([ 1, __clone(key2), gone ]);
				$cj = undefined;
			} else {
				$cj = undefined;
			}
			$ch = $cj;
		} else {
			const map3 = $cg[1];
			cell[0].v = __clone(map3);
			recorded.push([ 2, __clone(map3) ]);
			$ch = undefined;
		}
		$ch;
	}
	if (recorded.length === 0) {
		return;
	}
	for (const op2 of recorded) {
		record(cell[2], op2);
	}
	wake_for(cell[3], recorded, $cf);
	publish_map(cell, $cf);
}
function take(self, item2, $cz) {
	defer(self, () => {
		dispose(item2, $cz);
		return;
	});
	return __clone(item2);
}
function seal_map(instance, owned, $cd, $ce, caller) {
	const sealed = of(instance[0]());
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_map_ops(sealed, drain2(), $cd);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $cd, $ce);
	} else {
		const cell_global_refresh = () => {
			return apply_map_ops(sealed, drain2(), $cd);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $aI, $aJ, caller) {
	return [ seal_map(open3(self), true, $aI, $aJ, caller) ];
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
	const $cI = core[8](item2);
	const answer = $cI[0];
	const owner = $cI[1];
	const $cJ = answer;
	let $cK = null;
	if ($cJ[0] === 0) {
		const value = $cJ[1];
		const $cL = owner;
		let $cM = null;
		if ($cL[0] === 0) {
			const held = $cL[1];
			$cM = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$cM = undefined;
		}
		$cM;
		$cK = [ id, __clone(value) ];
	} else {
		const instance = $cJ[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $cN = consumer.v;
			let $cO = null;
			if ($cN[0] === 0) {
				const downstream = $cN[1];
				$cO = wake(downstream);
			} else {
				$cO = undefined;
			}
			return $cO;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$cK = [ id, pull() ];
	}
	return $cK;
}
function keyed_admit2(keyed2, key, input) {
	const $cP = admit2(keyed2[0], [ __clone(key), __clone(input) ]);
	const id = $cP[0];
	const value = $cP[1];
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
	const $cU = __map_get(core[3].v, hash2(id));
	let $cV = null;
	if ($cU[0] === 0) {
		const hold = $cU[1];
		core[3].v.delete(hash2(id));
		const $cW = hold[2];
		let $cX = null;
		if ($cW[0] === 0) {
			const handle = $cW[1];
			$cX = detach(handle);
		} else {
			$cX = undefined;
		}
		$cX;
		hold[3]();
		const $cY = hold[4];
		let $cZ = null;
		if ($cY[0] === 0) {
			const owner = $cY[1];
			$cZ = dispose2(owner);
		} else {
			$cZ = undefined;
		}
		$cV = $cZ;
	} else {
		$cV = undefined;
	}
	return $cV;
}
function keyed_retire2(keyed2, row) {
	keyed2[2].v.delete(hash2(row[3]));
	retire2(keyed2[0], row[3]);
}
function keyed_resolve2(keyed2, ops) {
	let events = [  ];
	for (const op of ops) {
		const $cQ = op;
		let $cR = null;
		if ($cQ[0] === 0) {
			const key = $cQ[1];
			const _was = $cQ[2];
			const input = $cQ[3];
			const $cS = __map_get(keyed2[1].v, hash(key));
			let $cT = null;
			if ($cS[0] === 0) {
				const row = $cS[1];
				keyed_retire2(keyed2, row);
				const fresh = keyed_admit2(keyed2, key, input);
				events.push([ 1, __clone(key), [ __clone(row[1]), __clone(row[2]) ], [ __clone(fresh[1]), __clone(fresh[2]) ], true ]);
				$cT = undefined;
			} else {
				const fresh2 = keyed_admit2(keyed2, key, input);
				events.push([ 0, __clone(key), __clone(fresh2[1]), __clone(fresh2[2]) ]);
				$cT = undefined;
			}
			$cR = $cT;
		} else if ($cQ[0] === 1) {
			const key2 = $cQ[1];
			const _gone = $cQ[2];
			const hash3 = hash(key2);
			const $da = __map_get(keyed2[1].v, hash3);
			let $db = null;
			if ($da[0] === 0) {
				const row2 = $da[1];
				keyed_retire2(keyed2, row2);
				keyed2[1].v.delete(hash3);
				events.push([ 2, __clone(key2), __clone(row2[1]), __clone(row2[2]) ]);
				$db = undefined;
			} else {
				$db = undefined;
			}
			$cR = $db;
		} else {
			const map3 = $cQ[1];
			for (const row3 of keyed_rows(keyed2)) {
				keyed_retire2(keyed2, row3);
			}
			const emptied = new Map();
			keyed2[1].v = emptied;
			keyed_seed2(keyed2, map3);
			events.push([ 3, keyed_rows(keyed2) ]);
			$cR = undefined;
		}
		$cR;
	}
	const woke = __clone(keyed2[0][4].v);
	let $di = null;
	if (woke.length > 0) {
		keyed2[0][4].v = [  ];
		let seen = new Map();
		for (const id of woke) {
			if (seen.has(hash2(id))) {
				continue;
			}
			seen.set(hash2(id), true);
			const $dc = __map_get(keyed2[2].v, hash2(id));
			let $dd = null;
			if ($dc[0] === 0) {
				const hash4 = $dc[1];
				const $de = __map_get(keyed2[1].v, hash4);
				let $df = null;
				if ($de[0] === 0) {
					const row4 = $de[1];
					const $dg = __map_get(keyed2[0][3].v, hash2(id));
					let $dh = null;
					if ($dg[0] === 0) {
						const hold = $dg[1];
						if (hold[0]) {
							const now = hold[1]();
							keyed2[1].v.set(hash4, [ __clone(row4[0]), __clone(row4[1]), __clone(now), row4[3] ]);
							events.push([ 1, __clone(row4[0]), [ __clone(row4[1]), __clone(row4[2]) ], [ __clone(row4[1]), now ], false ]);
						}
						$dh = undefined;
					} else {
						$dh = undefined;
					}
					$df = $dh;
				} else {
					$df = undefined;
				}
				$dd = $df;
			} else {
				$dd = undefined;
			}
			$dd;
		}
		$di = undefined;
	}
	$di;
	return events;
}
function mapped_map_ops(events) {
	let produced = [  ];
	for (const event of events) {
		const $dj = event;
		let $dk = null;
		if ($dj[0] === 0) {
			const key = $dj[1];
			const _input = $dj[2];
			const value = $dj[3];
			$dk = produced.push([ 0, __clone(key), [ 1 ], __clone(value) ]);
		} else if ($dj[0] === 1) {
			const key2 = $dj[1];
			const before2 = $dj[2];
			const after = $dj[3];
			const _rerun = $dj[4];
			produced.push([ 0, __clone(key2), [ 0, __clone(before2[1]) ], __clone(after[1]) ]);
			$dk = undefined;
		} else if ($dj[0] === 2) {
			const key3 = $dj[1];
			const _input2 = $dj[2];
			const value2 = $dj[3];
			$dk = produced.push([ 1, __clone(key3), __clone(value2) ]);
		} else {
			const rows = $dj[1];
			let map3 = new7();
			for (const row of rows) {
				insert(map3, row[0], row[2]);
			}
			produced.push([ 2, map3 ]);
			$dk = undefined;
		}
		$dk;
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
		const $cH = run_element(stage, ($cF, $cG) => {
			return transform(entry[1], $cF, $cG);
		});
		const answer = $cH[0];
		const owner = $cH[1];
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
function memo2(self, $aI, $aJ, caller) {
	return [ seal_map(open4(self), true, $aI, $aJ, caller) ];
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
function remove2(self, key, $dr) {
	const had = peek(self, (held) => {
		const $ds = find(held, key);
		return $ds[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming_one(self, [ 3, __clone(key), 1 ]);
	mutate(self, (held) => {
		return drop(held, key);
	});
	landed_one(self, [ 3, __clone(key), 1 ], $dr);
}
function length(self) {
	return [ __clone(self) ];
}
function peek2(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get5(self) {
	return peek2(self[0], (held) => {
		return held_keys(held).length;
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $dz = watch.v[3];
	const cell = __clone($dz[1]);
	if ($dz[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle3(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
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
function follow(self, at3) {
	const $dB = get(self[0], at3);
	if ($dB[0] === 0) {
		return $dB[1];
	}
	const slot2 = as_i32(self[1].length);
	self[1].push(__clone(at3));
	insert(self[0], at3, slot2);
	return slot2;
}
function follow_id(storage, watch, holds, at3) {
	if (viewing2(watch, (held) => {
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
	const $dC = as_usize(id);
	__at(watch.v[1], $dC, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $dC, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $dE = __clone(self);
	let $dD = null;
	if (id < 0) {
		$dD = -(1);
	} else {
		$dD = id;
	}
	return [ $dE, $dD ];
}
function follow2(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at3));
}
function release2(self, slot2) {
	const $dF = __list_get(self[1], as_usize(slot2));
	if ($dF[0] === 0) {
		remove(self[0], $dF[1]);
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
		release2(watch.v[0], id);
	}
}
function release3(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at3, $dA) {
	const handle = follow2(self, at3);
	defer_to_owner(() => {
		return release3(handle);
	}, $dA);
	return handle;
}
function locate(self, slot2) {
	return __list_get(self[1], as_usize(slot2));
}
function at2(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
	});
}
function get6(self) {
	const $dI = at2(self);
	if ($dI[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek2(self[0], (held) => {
		return read(held, $dI[1]);
	});
}
function slot_cell(watch, slot2) {
	if (slot2 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot2);
	const $dJ = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell = __clone($dJ[1]);
	if ($dJ[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1085:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1086:2")[0] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle4(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
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
function reconcile_keyed(self, next, $dK) {
	const lens = __clone(self);
	batch(($dL) => {
		for (const key of peek2(lens, (held2) => {
			return held_keys(held2);
		})) {
			if (!(contains_key(next, key))) {
				remove2(lens, key, [ 0, $dL ]);
			}
		}
		for (const _ of entries2(next)) {
			const $dQ = _;
			const key2 = $dQ[0];
			const value = $dQ[1];
			const $dR = peek2(lens, (held2) => {
				return find(held2, key2);
			});
			let $dS = null;
			if ($dR[0] === 0) {
				const held = $dR[1];
				if (held !== value) {
					put(lens, key2, value, [ 0, $dL ]);
				}
				$dS = undefined;
			} else {
				$dS = put(lens, key2, value, [ 0, $dL ]);
			}
			$dS;
		}
		return;
	}, $dK);
}
function count(self) {
	return self.length;
}
function item(self, at3) {
	return __list_get(self, at3);
}
function lend2(self, at3, read) {
	if (at3 < self.length) {
		read(__at_view(self, at3, "std/src/reactive/lens.vl:2219:10"));
	}
}
function put2(self, at3, value) {
	__at_put(self, at3, __clone(value), "std/src/reactive/lens.vl:2239:3");
}
function change(self, at3, with2) {
	if (at3 < self.length) {
		with2(__at_view(self, at3, "std/src/reactive/lens.vl:2244:14"));
	}
}
function lens_over2(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over2(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function sequence(value, $dU) {
	return over2(value, new5(), [ (held, at3) => {
		return at3 < count(held);
	}, (held, at3) => {
		return item(held, at3);
	}, (held, at3, read) => {
		return lend2(held, at3, read);
	}, (held, at3, value2) => {
		return put2(held, at3, value2, $dU);
	}, (held, at3, with2) => {
		return change(held, at3, with2, $dU);
	} ]);
}
function explain_sequence(before2, after, changes) {
	const old_count = count(before2);
	const new_count = count(after);
	let low = old_count;
	let high = 0;
	let grew = 0;
	for (const change2 of changes) {
		const $dV = change2;
		let $dW = null;
		if ($dV[0] === 0) {
			const at3 = $dV[1];
			const removed = $dV[2];
			const inserted = $dV[3];
			if (at3 < low) {
				low = at3;
			}
			if (at3 + removed > high) {
				high = at3 + removed;
			}
			grew = grew + (as_i32(inserted) - as_i32(removed));
			$dW = undefined;
		} else if ($dV[0] === 1) {
			const at4 = $dV[1];
			const count2 = $dV[2];
			if (at4 < low) {
				low = at4;
			}
			if (at4 > high) {
				high = at4;
			}
			grew = grew + as_i32(count2);
			$dW = undefined;
		} else if ($dV[0] === 3) {
			const at5 = $dV[1];
			const count3 = $dV[2];
			if (at5 < low) {
				low = at5;
			}
			if (at5 + count3 > high) {
				high = at5 + count3;
			}
			grew = grew - as_i32(count3);
			$dW = undefined;
		} else if ($dV[0] === 2) {
			const at6 = $dV[1];
			if (at6 < low) {
				low = at6;
			}
			if (at6 + 1 > high) {
				high = at6 + 1;
			}
			$dW = undefined;
		} else if ($dV[0] === 4) {
			const at7 = $dV[1];
			const _part = $dV[2];
			if (at7 < low) {
				low = at7;
			}
			if (at7 + 1 > high) {
				high = at7 + 1;
			}
			$dW = undefined;
		} else if ($dV[0] === 5) {
			const from = $dV[1];
			const to = $dV[2];
			let $dX = null;
			if (from < to) {
				$dX = from;
			} else {
				$dX = to;
			}
			const first = $dX;
			let $dY = null;
			if (from < to) {
				$dY = to;
			} else {
				$dY = from;
			}
			const last2 = $dY;
			if (first < low) {
				low = first;
			}
			if (last2 + 1 > high) {
				high = last2 + 1;
			}
			$dW = undefined;
		} else {
			return [ 1 ];
		}
		$dW;
	}
	if (as_i32(new_count) !== as_i32(old_count) + grew) {
		return [ 0, "the changes add up to " + (as_i32(old_count) + grew) + " items, the write left " + new_count ];
	}
	if (changes.length === 0) {
		low = 0;
		high = 0;
	}
	let at8 = 0;
	let $dZ = null;
	if (low < old_count) {
		$dZ = low;
	} else {
		$dZ = old_count;
	}
	const head = $dZ;
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
function sequence_checked(value, $dT) {
	const lens = sequence(value, $dT);
	lens[0].v[6] = [ 0, (before2, after, changes) => {
		return explain_sequence(before2, after, changes);
	} ];
	return lens;
}
function arrived_at(storage, at3, count2) {
	return peek_storage2(storage, (held) => {
		let arrived = [  ];
		let index = at3;
		while (index < at3 + count2) {
			const $ee = item(held, index);
			if ($ee[0] === 0) {
				arrived.push($ee[1]);
			}
			index = index + 1;
		}
		return arrived;
	});
}
function one(item2) {
	const $en = item2;
	let $eo = null;
	if ($en[0] === 0) {
		const value = $en[1];
		$eo = [ __clone(value) ];
	} else {
		$eo = [  ];
	}
	return $eo;
}
function all_of(storage) {
	return peek_storage2(storage, (held) => {
		let out = [  ];
		let at3 = 0;
		const count2 = count(held);
		while (at3 < count2) {
			const $et = item(held, at3);
			if ($et[0] === 0) {
				out.push($et[1]);
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
	const $ez = before2;
	let $eA = undefined;
	if ($ez[0] === 0 && (($eA = after), $eA[0] === 0)) {
		record2(log, [ 1, at3, __clone($ez[1]), __clone($eA[1]) ]);
	}
}
function release4(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $eb) {
	const leaving = __shared_new([  ]);
	const runs2 = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0].v[0]);
	const tap2 = tap(self, (change2) => {
		const $ec = change2;
		let $ed = null;
		if ($ec[0] === 0) {
			const at3 = $ec[1];
			const removed = $ec[2];
			const _inserted = $ec[3];
			leaving.v = arrived_at(storage, at3, removed);
			$ed = undefined;
		} else if ($ec[0] === 3) {
			const at4 = $ec[1];
			const count2 = $ec[2];
			leaving.v = arrived_at(storage, at4, count2);
			$ed = undefined;
		} else if ($ec[0] === 2) {
			const at5 = $ec[1];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at5);
			}));
			$ed = undefined;
		} else if ($ec[0] === 4) {
			const at6 = $ec[1];
			const _part = $ec[2];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at6);
			}));
			$ed = undefined;
		} else if ($ec[0] === 5) {
			const from = $ec[1];
			const to = $ec[2];
			$ed = extend_run(runs2, from, to);
		} else {
			$ed = undefined;
		}
		return $ed;
	}, (change2, stage) => {
		const $er = stage;
		let $es = null;
		if ($er[0] === 2) {
			record2(log, [ 2, all_of(storage) ]);
			return;
			$es = undefined;
		} else if ($er[0] === 3) {
			return;
		} else {
			$es = undefined;
		}
		$es;
		const $ew = stage;
		const late = $ew[0] === 1;
		const $ex = change2;
		let $ey = null;
		if ($ex[0] === 0) {
			const at3 = $ex[1];
			const removed = $ex[2];
			const inserted = $ex[3];
			if (late && removed > 0) {
				record2(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record2(log, [ 0, at3, [  ], arrived_at(storage, at3, inserted) ]);
			} else {
				record2(log, [ 0, at3, __clone(leaving.v), arrived_at(storage, at3, inserted) ]);
			}
			$ey = undefined;
		} else if ($ex[0] === 1) {
			const at4 = $ex[1];
			const count2 = $ex[2];
			record2(log, [ 0, at4, [  ], arrived_at(storage, at4, count2) ]);
			$ey = undefined;
		} else if ($ex[0] === 3) {
			const at5 = $ex[1];
			const _count = $ex[2];
			if (late) {
				record2(log, [ 2, all_of(storage) ]);
			} else {
				record2(log, [ 0, at5, __clone(leaving.v), [  ] ]);
			}
			$ey = undefined;
		} else if ($ex[0] === 2) {
			const at6 = $ex[1];
			$ey = record_set(log, storage, leaving, at6, late);
		} else if ($ex[0] === 4) {
			const at7 = $ex[1];
			const _part = $ex[2];
			$ey = record_set(log, storage, leaving, at7, late);
		} else if ($ex[0] === 5) {
			const from = $ex[1];
			const to = $ex[2];
			if (skipping.v > 0) {
				const $eB = [ skipping, "v" ];
				skipping.v = $eB[0][$eB[1]] - 1;
				return;
			}
			if (runs2.v.length === 0) {
				record2(log, [ 3, from, 1, to ]);
				return;
			}
			const $eC = __remove_at(runs2.v, 0, "std/src/reactive/lens.vl:1676:47");
			const start = $eC[0];
			const count3 = $eC[1];
			const first = $eC[2];
			skipping.v = count3 - 1;
			let $eD = null;
			if (first > start) {
				$eD = first + 1 - count3;
			} else {
				$eD = first;
			}
			const landing = $eD;
			record2(log, [ 3, start, count3, landing ]);
			$ey = undefined;
		} else {
			$ey = record2(log, [ 2, all_of(storage) ]);
		}
		return $ey;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $eb);
	return feed;
}
function items(self, $ea) {
	return items_into(self, new9(), $ea);
}
function map(self, transform) {
	return [ self, transform ];
}
function cursor3(self) {
	return cursor(self[1]);
}
function all2(self) {
	return all_of(self[0].v[0]);
}
function since3(self, cursor4) {
	const $eK = since(self[1], cursor4);
	let $eL = null;
	if ($eK[0] === 0) {
		const recorded = $eK[1];
		$eL = recorded;
	} else {
		$eL = [ [ 2, all2(self[0]) ] ];
	}
	return $eL;
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
	return on_settle(self[0][0].v[2], subscriber);
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
		const $eW = admit2(core, item2);
		const id = $eW[0];
		const value = $eW[1];
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
		const $eX = op;
		let $eY = null;
		if ($eX[0] === 0) {
			const at3 = $eX[1];
			const removed = $eX[2];
			const inserted = $eX[3];
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
				const $fh = admit2(core, item2);
				const id2 = $fh[0];
				const value = $fh[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$eY = undefined;
		} else if ($eX[0] === 1) {
			const at4 = $eX[1];
			const _was = $eX[2];
			const item3 = $eX[3];
			const $fi = __list_get(core[0].v, at4);
			let $fj = null;
			if ($fi[0] === 0) {
				const old_id = $fi[1];
				const before2 = [ __clone(__at(core[1].v, at4, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at4, "std/src/reactive/delta.vl:1663:44")) ];
				retire2(core, old_id);
				const $fk = admit2(core, item3);
				const id3 = $fk[0];
				const value2 = $fk[1];
				__at_put(core[0].v, at4, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at4, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at4, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at4, before2, [ __clone(item3), value2 ], true ]);
				$fj = undefined;
			} else {
				$fj = undefined;
			}
			$eY = $fj;
		} else if ($eX[0] === 2) {
			const items2 = $eX[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire2(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$eY = undefined;
		} else {
			const from = $eX[1];
			const count2 = $eX[2];
			const to = $eX[3];
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
			$eY = undefined;
		}
		$eY;
	}
	const woke = __clone(core[4].v);
	let $fp = null;
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
			const $fl = __map_get(positions, hash2(id6));
			let $fm = null;
			if ($fl[0] === 0) {
				const at5 = $fl[1];
				const $fn = __map_get(core[3].v, hash2(id6));
				let $fo = null;
				if ($fn[0] === 0) {
					const hold = $fn[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1731:19"));
						const before3 = __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at5, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at5, [ __clone(item4), before3 ], [ item4, now ], false ]);
					}
					$fo = undefined;
				} else {
					$fo = undefined;
				}
				$fm = $fo;
			} else {
				$fm = undefined;
			}
			$fm;
		}
		$fp = undefined;
	}
	$fp;
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
		const $fq = event;
		let $fr = null;
		if ($fq[0] === 0) {
			const at3 = $fq[1];
			const left = $fq[2];
			const arrived = $fq[3];
			ops.push([ 0, at3, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$fr = undefined;
		} else if ($fq[0] === 1) {
			const at4 = $fq[1];
			const before2 = $fq[2];
			const after = $fq[3];
			const _rerun = $fq[4];
			ops.push([ 1, at4, __clone(before2[1]), __clone(after[1]) ]);
			$fr = undefined;
		} else if ($fq[0] === 2) {
			const _left = $fq[1];
			const now = $fq[2];
			$fr = ops.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $fq[1];
			const count2 = $fq[2];
			const to = $fq[3];
			$fr = ops.push([ 3, from, count2, to ]);
		}
		$fr;
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
		const $eO = run_element(stage, ($eM, $eN) => {
			return transform(item2, $eM, $eN);
		});
		const answer = $eO[0];
		const owner = $eO[1];
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
		const $fy = __list_get(inserted, offset);
		let $fz = null;
		if ($fy[0] === 0) {
			const value = $fy[1];
			$fz = __insert_at(list2, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$fz = undefined;
		}
		$fz;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function publish(cell, $fA) {
	set(cell[1], at(cell[2]), $fA, "std/src/reactive/delta.vl:799:17");
}
function splice(self, at3, removed, inserted, $fx) {
	const held = size(self);
	const start = splice_start(held, at3);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record2(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $fx);
}
function set_at(self, at3, value, $fB) {
	const $fC = __list_get(self[0].v, at3);
	let $fD = null;
	if ($fC[0] === 0) {
		const previous = $fC[1];
		__at_put(self[0].v, at3, __clone(value), "std/src/reactive/delta.vl:855:5");
		record2(self[2], [ 1, at3, previous, __clone(value) ]);
		publish(self, $fB);
		$fD = undefined;
	} else {
		$fD = undefined;
	}
	return $fD;
}
function set2(self, value, $fE, caller) {
	self[0].v = __clone(value);
	record2(self[2], [ 2, __clone(value) ]);
	publish(self, $fE);
}
function move_range(self, from, count2, to, $fF) {
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
		const $fG = __list_get(lifted, offset);
		let $fH = null;
		if ($fG[0] === 0) {
			const value = $fG[1];
			$fH = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$fH = undefined;
		}
		$fH;
		offset = offset + 1;
	}
	record2(self[2], [ 3, from, count2, to ]);
	publish(self, $fF);
}
function apply_ops(cell, ops, $fu) {
	for (const op of ops) {
		const $fv = op;
		let $fw = null;
		if ($fv[0] === 0) {
			const at3 = $fv[1];
			const removed = $fv[2];
			const inserted = $fv[3];
			$fw = splice(cell, at3, removed.length, inserted, $fu);
		} else if ($fv[0] === 1) {
			const at4 = $fv[1];
			const _was = $fv[2];
			const value = $fv[3];
			$fw = set_at(cell, at4, value, $fu);
		} else if ($fv[0] === 2) {
			const items2 = $fv[1];
			$fw = set2(cell, items2, $fu, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $fv[1];
			const count2 = $fv[2];
			const to = $fv[3];
			$fw = move_range(cell, from, count2, to, $fu);
		}
		$fw;
	}
}
function seal_collection(instance, owned, $fs, $ft, caller) {
	const sealed = of2(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $fs);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $fs, $ft);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $fs);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo3(self, $eG, $eH, caller) {
	return [ seal_collection(open6(self), true, $eG, $eH, caller) ];
}
function splice2(self, at3, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at3, "std/src/reactive/lens.vl:2228:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at3 + offset, __clone(value), "std/src/reactive/lens.vl:2233:9");
		offset = offset + 1;
	}
}
function audit(self, before2, changes) {
	const $fN = self[0].v[6];
	if ($fN[0] === 0) {

	} else {
		return;
	}
	const $fO = before2;
	if ($fO[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held) => {
		return $fN[1]($fO[1], held, changes);
	});
	const $fP = problem;
	if ($fP[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $fP[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function wake_all3(watch, slot2, $R) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1117:36"));
	});
	const $go = held[0];
	if ($go[0] === 0) {
		notify($go[1], $R);
	}
	for (const part of held[1]) {
		notify(part[1], $R);
	}
}
function landed2(self, changes, late, $n) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index = 0;
	for (const change2 of changes) {
		const $fQ = change2;
		let $fR = null;
		if ($fQ[0] === 0) {
			const _at = $fQ[1];
			const removed = $fQ[2];
			const inserted = $fQ[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$fR = undefined;
		} else if ($fQ[0] === 1) {
			const _at2 = $fQ[1];
			const _count = $fQ[2];
			$fR = resized = true;
		} else if ($fQ[0] === 3) {
			const _at3 = $fQ[1];
			const _count2 = $fQ[2];
			$fR = resized = true;
		} else if ($fQ[0] === 6) {
			$fR = resized = true;
		} else if ($fQ[0] === 5) {
			const _from = $fQ[1];
			const _to = $fQ[2];
			$fR = shaped = true;
		} else {
			$fR = undefined;
		}
		$fR;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $gh = null;
		if (self[0].v[4].length > 0) {
			let $gg = null;
			if (!(late)) {
				$gg = [ 0 ];
			} else if (!(several)) {
				$gg = [ 1 ];
			} else if (index === 0) {
				$gg = [ 2 ];
			} else {
				$gg = [ 3 ];
			}
			const stage = $gg;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$gh = undefined;
		}
		$gh;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $gi = null;
	if (shaped || resized) {
		$gi = watch.v[2];
	} else {
		$gi = [ 1 ];
	}
	const shape = __clone($gi);
	let $gj = null;
	if (resized) {
		$gj = watch.v[3];
	} else {
		$gj = [ 1 ];
	}
	const sized = __clone($gj);
	let $gk = undefined;
	let $gl = undefined;
	if (wakes.length === 0 && (($gk = shape), $gk[0] === 1) && (($gl = sized), $gl[0] === 1)) {
		notify(published, $n);
		return;
	}
	settle_together(($O) => {
		for (const wake3 of wakes) {
			const $gm = wake3;
			let $gn = null;
			if ($gm[0] === 0) {
				const slot2 = $gm[1];
				$gn = wake_all3(watch, slot2, [ 0, $O ]);
			} else if ($gm[0] === 2) {
				const slot3 = $gm[1];
				$gn = wake_all3(watch, slot3, [ 0, $O ]);
			} else {
				const slot4 = $gm[1];
				const part = $gm[2];
				$gn = wake_part(watch, slot4, part, [ 0, $O ]);
			}
			$gn;
		}
		const $gq = sized;
		if ($gq[0] === 0) {
			notify($gq[1], [ 0, $O ]);
		}
		const $gr = shape;
		if ($gr[0] === 0) {
			notify($gr[1], [ 0, $O ]);
		}
		notify(published, [ 0, $O ]);
		return;
	}, $n);
}
function landed_one2(self, change2, $m) {
	landed2(self, [ __clone(change2) ], false, $m);
}
function plan(self, change2, write, $fI) {
	const $fJ = self[0].v[6];
	let $fK = null;
	if ($fJ[0] === 0) {
		$fK = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$fK = [ 1 ];
	}
	const before2 = $fK;
	coming_one(self, change2);
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, write(held) ];
	});
	audit(self, before2, [ __clone(change2) ]);
	landed_one2(self, change2, $fI);
	const $gs = out;
	let $gt = null;
	if ($gs[0] === 0) {
		const result = __clone($gs[1]);
		$gt = result;
	} else {
		$gt = (() => {
			throw __panic("lens: a planned write never ran", "std/src/reactive/lens.vl:796:12");
		})();
	}
	return $gt;
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
	return on_settle(self[0].v[2], subscriber);
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
function update(self, change2, $gw) {
	const $gx = self[0].v[6];
	let $gy = null;
	if ($gx[0] === 0) {
		$gy = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$gy = [ 1 ];
	}
	const before2 = $gy;
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, change2(held) ];
	});
	const $gz = out;
	let $gA = null;
	if ($gz[0] === 0) {
		const result = __clone($gz[1][0]);
		const changes = __clone($gz[1][1]);
		audit(self, before2, changes);
		landed2(self, changes, true, $gw);
		$gA = result;
	} else {
		$gA = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $gA;
}
function plan2(self, change2, write, $fI) {
	const $gG = self[0].v[6];
	let $gH = null;
	if ($gG[0] === 0) {
		$gH = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$gH = [ 1 ];
	}
	const before2 = $gH;
	coming_one(self, change2);
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, write(held) ];
	});
	audit(self, before2, [ __clone(change2) ]);
	landed_one2(self, change2, $fI);
	const $gI = out;
	let $gJ = null;
	if ($gI[0] === 0) {
		const result = __clone($gI[1]);
		$gJ = result;
	} else {
		$gJ = (() => {
			throw __panic("lens: a planned write never ran", "std/src/reactive/lens.vl:796:12");
		})();
	}
	return $gJ;
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
const adults = memo(filter(entries(people, [ 1 ]), (_name, age, $aG, $aH) => {
	return age >= 40;
}), [ 1 ], [ 1 ], "lens-keyed.vl:16:63");
const next_year = memo2(map_values(entries(people, [ 1 ]), (age, $cD, $cE) => {
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
on_change(length(people), (_n, $dt) => {
	const $du = [ lengths, "v" ];
	return lengths.v = $du[0][$du[1]] + 1;
}, "lens-keyed.vl:31:27");
put(people, "ada", 41, [ 1 ]);
assert(lengths.v === 0 && get5(length(people)) === 2, "a replace leaves the length asleep", "lens-keyed.vl:33:2");
put(people, "dee", 29, [ 1 ]);
assert(lengths.v === 1 && get5(length(people)) === 3, "an arrival wakes it", "lens-keyed.vl:35:2");
const ada = slot(people, "ada", [ 1 ]);
const ada_runs = __shared_new(0);
on_change2(ada, (_v, $dG) => {
	const $dH = [ ada_runs, "v" ];
	return ada_runs.v = $dH[0][$dH[1]] + 1;
}, "lens-keyed.vl:40:15");
let wanted = new7();
insert(wanted, "ada", 41);
insert(wanted, "eve", 33);
reconcile_keyed(people, wanted, [ 1 ]);
assert(len(all(people)) === 2 && eq2(get(all(people), "eve"), [ 0, 33 ]) && eq2(get(all(people), "cy"), [ 1 ]), "the map became the new map", "lens-keyed.vl:45:2");
assert(ada_runs.v === 0, "an unchanged key did not wake", "lens-keyed.vl:54:2");
const list = sequence_checked([ "a", "b", "c", "d" ], [ 1 ]);
const mirror = memo3(map(items(list, [ 1 ]), (v, $eE, $eF) => {
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
on_change3(__clone(list), (_all, $gu) => {
	const $gv = [ runs, "v" ];
	return runs.v = $gv[0][$gv[1]] + 1;
}, "lens-keyed.vl:69:20");
update(list, (held) => {
	__insert_at(held, 0, "z", "lens-keyed.vl:71:8");
	return [ undefined, [ [ 1, 0, 1 ] ] ];
}, [ 1 ]);
assert(eq(all2(list), [ "z", "a", "d" ]) && runs.v === 1, "an honest update lands", "lens-keyed.vl:74:2");
const before = [ "z", "a", "d" ];
const $gB = explain_sequence(before, [ "y", "z", "a", "d" ], [ [ 1, 2, 1 ] ]);
assert($gB[0] === 0, "a wrong position is caught", "lens-keyed.vl:78:2");
const $gC = explain_sequence(before, [ "y", "z", "a", "d" ], [ [ 1, 0, 2 ] ]);
assert($gC[0] === 0, "a wrong count is caught", "lens-keyed.vl:82:2");
const $gD = explain_sequence(before, [ "z", "A", "d" ], [ [ 4, 2, "x" ] ]);
assert($gD[0] === 0, "an edit at the wrong row is caught", "lens-keyed.vl:86:2");
assert(eq2(explain_sequence(before, [ "y", "z", "a", "d" ], [ [ 1, 0, 1 ] ]), [ 1 ]), "the honest change passes", "lens-keyed.vl:90:2");
assert(eq2(explain_sequence(before, [ "z", "p", "q", "d" ], [ [ 0, 1, 1, 2 ] ]), [ 1 ]), "an honest splice passes", "lens-keyed.vl:98:2");
const $gE = explain_sequence(before, [ "z", "p", "q", "d" ], [ [ 0, 1, 1, 1 ] ]);
assert($gE[0] === 0, "a splice with the wrong count is caught", "lens-keyed.vl:105:2");
const $gF = explain_sequence(before, [ "z", "p", "q", "d" ], [ [ 0, 2, 1, 2 ] ]);
assert($gF[0] === 0, "a splice at the wrong address is caught", "lens-keyed.vl:109:2");
plan2(list, [ 0, 1, 1, 2 ], (held) => {
	splice2(held, 1, 1, [ "p", "q" ]);
	return;
}, [ 1 ]);
assert(eq(all2(list), [ "z", "p", "q", "d" ]) && eq(get9(mirror), [ "z", "p", "q", "d" ]), "a planned splice is one exact op", "lens-keyed.vl:117:2");
console.log("keyed: all passed");
//# sourceMappingURL=lens-keyed.mjs.map
