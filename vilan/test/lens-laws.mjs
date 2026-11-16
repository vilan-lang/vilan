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
function __replace(target, value) {
	if (Array.isArray(target) && Array.isArray(value)) target.length = value.length;
	return Object.assign(target, value);
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
function eq(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(item3 === __at(b2, index2, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function hash(self) {
	return __hash(self);
}
function hash2(self) {
	return __hash(self);
}
function new2() {
	const table2 = [ new Map(), undefined ];
	return [ table2 ];
}
function assert(condition, message, caller) {
	let $aL = null;
	if (!(condition)) {
		$aL = (() => {
			throw __panic(message, caller);
		})();
	}
	return $aL;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped2 = truncated % modulus;
	let $h = null;
	if (wrapped2 < 0) {
		$h = wrapped2 + modulus;
	} else {
		$h = wrapped2;
	}
	return $h;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $g = null;
	if (truncated > 0) {
		$g = truncated;
	} else {
		$g = 0;
	}
	return $g;
}
function fold_signed(value, modulus, half) {
	const wrapped2 = fold_unsigned(value, modulus);
	let $i = null;
	if (wrapped2 >= half) {
		$i = wrapped2 - modulus;
	} else {
		$i = wrapped2;
	}
	return $i;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function eq2(self, b2) {
	return self === b2;
}
function eq3(self, b2) {
	const $aG = self;
	let $aJ = null;
	if ($aG[0] === 0) {
		const $aH = b2;
		let $aI = null;
		if ($aH[0] === 0) {
			$aI = $aG[1] === $aH[1];
		} else {
			$aI = false;
		}
		$aJ = $aI;
	} else {
		const $aK = b2;
		$aJ = $aK[0] === 1;
	}
	return $aJ;
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
function new3() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return is_empty(self[0].v) && is_empty(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key2 = hash2(subscriber[0]);
		let $ak = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key2))) {
				turn[3].v.set(key2, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ak = undefined;
		} else if (!(turn[2].v.has(key2))) {
			turn[2].v.set(key2, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index2 = turn[0].v.length;
			while (index2 > 0 && __at(turn[0].v, index2 - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index2 = index2 - 1;
			}
			__insert_at(turn[0].v, index2, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$ak;
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
	const $dW = turn;
	let $dX = null;
	if ($dW[0] === 0) {
		const ambient = $dW[1];
		$dX = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $dY = last(draining_turns.v);
		let $dZ = null;
		if ($dY[0] === 0) {
			const draining = $dY[1];
			$dZ = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$dZ = undefined;
		}
		$dX = $dZ;
	}
	return $dX;
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
function dispose(self, $fn) {
	const $fo = $fn;
	let $fp = null;
	if ($fo[0] === 0) {
		const established = $fo[1];
		$fp = [ 0, established ];
	} else {
		$fp = last(draining_turns.v);
	}
	const ambient = $fp;
	release_under(self, ambient);
}
function detach(handle) {
	const $ek = last(releasing_turns.v);
	let $el = null;
	if ($ek[0] === 0) {
		const at_release = $ek[1];
		$el = at_release;
	} else {
		$el = last(draining_turns.v);
	}
	const turn = $el;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $em = [ 0, handle[0] ];
	let $en = null;
	if ($em[0] === 0) {
		const subscribers = $em[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$en = undefined;
	} else {
		$en = undefined;
	}
	$en;
	const $eo = ambient;
	let $ep = null;
	if ($eo[0] === 0) {
		const turn = $eo[1];
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
		$ep = undefined;
	} else {
		$ep = undefined;
	}
	$ep;
	const $eq = handle[3].v;
	let $er = null;
	if ($eq[0] === 0) {
		const release11 = $eq[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release11, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$er = undefined;
	} else {
		$er = undefined;
	}
	return $er;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $q = null;
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
		$q = undefined;
	}
	return $q;
}
function stage_nursery(self) {
	const $dK = nursery(self);
	let $dL = null;
	if ($dK[0] === 0) {
		const held2 = $dK[1];
		$dL = held2;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$dL = made;
	}
	return $dL;
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
	let $dJ = null;
	if (is_disposed(self)) {
		$dJ = [ 1 ];
	} else {
		$dJ = self[0].v[3];
	}
	return $dJ;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $eD = null;
	if (!(is_disposed(self))) {
		const held2 = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $eu = held2[3];
		let $ev = null;
		if ($eu[0] === 0) {
			const nursery2 = $eu[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$ev = undefined;
		} else {
			$ev = undefined;
		}
		$ev;
		let $eC = null;
		if (held2[2]) {
			let failure = [ 1 ];
			for (const cleanup of held2[1].v) {
				const $ex = __guarded(cleanup);
				let $ey = null;
				if ($ex[0] === 0) {
					const message = $ex[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$ey = undefined;
				} else {
					$ey = undefined;
				}
				$ey;
			}
			const $eA = failure;
			let $eB = null;
			if ($eA[0] === 0) {
				const message2 = $eA[1];
				$eB = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$eB = undefined;
			}
			$eC = $eB;
		}
		$eD = $eC;
	}
	return $eD;
}
function register_with_owner(subscription, $fi, $fj) {
	const $fk = $fj;
	let $fl = null;
	if ($fk[0] === 0) {
		const owner2 = $fk[1];
		$fl = take(owner2, subscription, $fi);
	} else {
		$fl = __clone(subscription);
	}
	return $fl;
}
function defer_to_owner(cleanup, $n) {
	const $o = $n;
	let $p = null;
	if ($o[0] === 0) {
		const owner2 = $o[1];
		$p = defer(owner2, cleanup);
	} else {
		$p = undefined;
	}
	return $p;
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
function also_releasing(handle, release11) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release11();
		const $fg = previous;
		let $fh = null;
		if ($fg[0] === 0) {
			const earlier = $fg[1];
			$fh = earlier();
		} else {
			$fh = undefined;
		}
		return $fh;
	} ];
}
function settle_together(body, $ay) {
	const $az = $ay;
	let $aA = null;
	if ($az[0] === 0) {
		const current = $az[1];
		$aA = body(current);
	} else {
		const $aB = last(draining_turns.v);
		let $aC = null;
		if ($aB[0] === 0) {
			const draining = $aB[1];
			$aC = body(draining);
		} else {
			$aC = batch(body, $ay);
		}
		$aA = $aC;
	}
	return $aA;
}
function splice_start(held2, at4) {
	let $ed = null;
	if (at4 > held2) {
		$ed = held2;
	} else {
		$ed = at4;
	}
	return $ed;
}
function splice_count(held2, start, removed) {
	let $ee = null;
	if (start + removed > held2) {
		$ee = held2 - start;
	} else {
		$ee = removed;
	}
	return $ee;
}
function kept_index(flags) {
	const index2 = [ __shared_new([  ]) ];
	rebuild_kept(index2, flags);
	return index2;
}
function rebuild_kept(index2, flags) {
	let tree = [ 0 ];
	for (const flag of flags) {
		const $fE = tree;
		let $fD = null;
		if (flag) {
			$fD = 1;
		} else {
			$fD = 0;
		}
		$fE.push($fD);
	}
	const size6 = as_i32(flags.length);
	let at4 = 1;
	while (at4 <= size6) {
		const parent = at4 + (at4 & 0 - at4);
		if (parent <= size6) {
			__at_put(tree, as_usize(parent), __at(tree, as_usize(parent), "std/src/reactive/delta.vl:1810:30") + __at(tree, as_usize(at4), "std/src/reactive/delta.vl:1810:56"), "std/src/reactive/delta.vl:1810:4");
		}
		at4 = at4 + 1;
	}
	index2[0].v = tree;
}
function kept_before(index2, position) {
	let total = 0;
	let at4 = as_i32(position);
	while (at4 > 0) {
		total = total + __at(index2[0].v, as_usize(at4), "std/src/reactive/delta.vl:1822:12");
		at4 = at4 - (at4 & 0 - at4);
	}
	return as_usize(total);
}
function kept_add(index2, position, delta) {
	const size6 = as_i32(index2[0].v.length) - 1;
	let at4 = as_i32(position) + 1;
	while (at4 <= size6) {
		__at_put(index2[0].v, as_usize(at4), __at(index2[0].v, as_usize(at4), "std/src/reactive/delta.vl:1833:39") + delta, "std/src/reactive/delta.vl:1833:3");
		at4 = at4 + (at4 & 0 - at4);
	}
}
function new5() {
	return [ [  ] ];
}
function follow(self, at4) {
	if (self[0].length === 0) {
		self[0].push(new6());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:145:3"), at4);
}
function apply(self, change4, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change4, wakes);
	}
}
function locate(self, slot7) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:158:3"), slot7);
}
function follows_items(self) {
	return true;
}
function release(self, slot7) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:167:4"), slot7);
	}
}
function new6() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at4) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:213:7") <= at4) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at4 - __at(self[3], low, "std/src/reactive/lens.vl:219:14") ];
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
	let index2 = from;
	const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:239:14").length;
	while (index2 < held2) {
		const slot7 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index2, "std/src/reactive/lens.vl:241:15");
		if (slot7 !== -(1)) {
			__at_put(self[4], as_usize(slot7), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot7), index2, "std/src/reactive/lens.vl:244:5");
		}
		index2 = index2 + 1;
	}
}
function open(self, place) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place, block, "std/src/reactive/lens.vl:255:14");
	return block;
}
function widen(self, at4, count3) {
	if (count3 === 0 || at4 >= self[6]) {
		return;
	}
	const $Q = seat(self, at4);
	const place = $Q[0];
	const index2 = $Q[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $R = null;
	if (count3 <= 8) {
		let made = 0;
		while (made < count3) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$R = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown2 = [  ];
		let copied2 = 0;
		while (copied2 < index2) {
			grown2.push(__at(before, copied2, "std/src/reactive/lens.vl:277:16"));
			copied2 = copied2 + 1;
		}
		let made2 = 0;
		while (made2 < count3) {
			grown2.push(-(1));
			made2 = made2 + 1;
		}
		const held2 = before.length;
		while (copied2 < held2) {
			grown2.push(__at(before, copied2, "std/src/reactive/lens.vl:287:16"));
			copied2 = copied2 + 1;
		}
		__at_put(self[0], as_usize(block), grown2, "std/src/reactive/lens.vl:290:4");
	}
	$R;
	self[6] = self[6] + count3;
	renumber(self, block, index2 + count3);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at4, count3, wakes) {
	let left = count3;
	while (left > 0 && at4 < self[6]) {
		const $O = seat(self, at4);
		const place = $O[0];
		const index2 = $O[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held2 = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $P = null;
		if (left < held2 - index2) {
			$P = left;
		} else {
			$P = held2 - index2;
		}
		const take2 = $P;
		let taken = 0;
		while (taken < take2) {
			const slot7 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index2, "std/src/reactive/lens.vl:311:46");
			if (slot7 !== -(1)) {
				__at_put(self[4], as_usize(slot7), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot7 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:320:7").length === 0) {
			__remove_at(self[1], place, "std/src/reactive/lens.vl:321:16");
		} else {
			renumber(self, block, index2);
		}
		restart(self);
	}
}
function rebalance(self, place) {
	const block = __at(self[1], place, "std/src/reactive/lens.vl:331:15");
	const all5 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first2 = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first2.push(__at(all5, index2, "std/src/reactive/lens.vl:336:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first2, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held2 = all5.length;
	while (index2 < held2) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held2) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all5, index2, "std/src/reactive/lens.vl:346:40"));
			index2 = index2 + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at4) {
	while (self[6] <= at4) {
		const places2 = self[1].length;
		if (places2 === 0 || __at(self[0], as_usize(__at(self[1], places2 - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh = open(self, places2);
			__at_put(self[2], as_usize(fresh), places2, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at4);
	const place = $j[0];
	const index2 = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index2, "std/src/reactive/lens.vl:372:14");
	if (held2 !== -(1)) {
		return held2;
	}
	const slot7 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index2, slot7, "std/src/reactive/lens.vl:379:3");
	return slot7;
}
function apply2(self, change4, wakes) {
	const $M = change4;
	let $N = null;
	if ($M[0] === 0) {
		const at4 = $M[1];
		const removed = $M[2];
		const inserted = $M[3];
		close(self, at4, removed, wakes);
		widen(self, at4, inserted);
		$N = undefined;
	} else if ($M[0] === 1) {
		const at5 = $M[1];
		const count3 = $M[2];
		$N = widen(self, at5, count3);
	} else if ($M[0] === 3) {
		const at6 = $M[1];
		const count4 = $M[2];
		$N = close(self, at6, count4, wakes);
	} else if ($M[0] === 2) {
		const at7 = $M[1];
		let $T = null;
		if (at7 < self[6]) {
			const $S = seat(self, at7);
			const place = $S[0];
			const index2 = $S[1];
			const slot7 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot7 !== -(1)) {
				wakes.push([ 0, slot7 ]);
			}
			$T = undefined;
		}
		$N = $T;
	} else if ($M[0] === 4) {
		const at8 = $M[1];
		const part3 = $M[2];
		let $V = null;
		if (at8 < self[6]) {
			const $U = seat(self, at8);
			const place2 = $U[0];
			const index3 = $U[1];
			const slot8 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot8 !== -(1)) {
				wakes.push([ 1, slot8, part3 ]);
			}
			$V = undefined;
		}
		$N = $V;
	} else if ($M[0] === 5) {
		const from = $M[1];
		const to = $M[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $W = null;
		if (from > to) {
			$W = from;
		} else {
			$W = to;
		}
		const far = $W;
		while (self[6] <= far) {
			const places2 = self[1].length;
			if (places2 === 0 || __at(self[0], as_usize(__at(self[1], places2 - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh = open(self, places2);
				__at_put(self[2], as_usize(fresh), places2, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $X = seat(self, from);
		const left = $X[0];
		const index4 = $X[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot9 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $Y = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$Y = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$Y = seat(self, to);
		}
		const $Z = $Y;
		const place3 = $Z[0];
		const landing = $Z[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot9, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$N = undefined;
	} else {
		let slot10 = 0;
		const slots = as_i32(self[4].length);
		while (slot10 < slots) {
			if (__at(self[4], as_usize(slot10), "std/src/reactive/lens.vl:455:9") !== -(1)) {
				__at_put(self[4], as_usize(slot10), -(1), "std/src/reactive/lens.vl:456:7");
				wakes.push([ 2, slot10 ]);
			}
			slot10 = slot10 + 1;
		}
		self[0] = [  ];
		self[1] = [  ];
		self[2] = [  ];
		self[3] = [  ];
		self[6] = 0;
		$N = undefined;
	}
	return $N;
}
function release2(self, slot7) {
	const block = __at(self[4], as_usize(slot7), "std/src/reactive/lens.vl:471:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:475:3"), __at(self[5], as_usize(slot7), "std/src/reactive/lens.vl:475:33"), -(1), "std/src/reactive/lens.vl:475:3");
	__at_put(self[4], as_usize(slot7), -(1), "std/src/reactive/lens.vl:476:3");
}
function locate2(self, slot7) {
	const block = __at(self[4], as_usize(slot7), "std/src/reactive/lens.vl:480:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:484:20"), "std/src/reactive/lens.vl:484:8") + __at(self[5], as_usize(slot7), "std/src/reactive/lens.vl:484:51") ];
}
function new7() {
	return [ new9(), [  ] ];
}
function extend_run(runs2, from, to) {
	const held2 = runs2.v.length;
	let $dl = null;
	if (held2 > 0) {
		const $dk = __at(runs2.v, held2 - 1, "std/src/reactive/lens.vl:1989:31");
		const start = $dk[0];
		const count3 = $dk[1];
		const first2 = $dk[2];
		const leftward = first2 < start && from === start + count3 && to === first2 + count3;
		const rightward = first2 > start && from === start && to === first2;
		if (leftward || rightward) {
			__at_put(runs2.v, held2 - 1, [ start, count3 + 1, first2 ], "std/src/reactive/lens.vl:1993:4");
			return;
		}
		$dl = undefined;
	}
	$dl;
	runs2.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function tasks() {
	return [ [ "a", "open" ], [ "b", "open" ], [ "c", "open" ] ];
}
function ages_of() {
	let ages2 = new9();
	insert(ages2, "bo", 4);
	return ages2;
}
function count(self) {
	return self[0].length;
}
function item(self, at4) {
	return __list_get(self[0], at4);
}
function splice(self, at4, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self[0], at4, "lens-laws.vl:65:14");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self[0], at4 + offset, value, "lens-laws.vl:70:14");
		offset = offset + 1;
	}
}
function eq4(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function count2(self) {
	return self.length;
}
function item2(self, at4) {
	return __list_get(self, at4);
}
function lend(self, at4, read) {
	if (at4 < self.length) {
		read(__at_view(self, at4, "std/src/reactive/lens.vl:2219:10"));
	}
}
function put(self, at4, value) {
	__at_put(self, at4, __clone(value), "std/src/reactive/lens.vl:2239:3");
}
function change(self, at4, with2) {
	if (at4 < self.length) {
		with2(__at_view(self, at4, "std/src/reactive/lens.vl:2244:14"));
	}
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
function sequence(value, $a) {
	return over(value, new5(), [ (held2, at4) => {
		return at4 < count2(held2);
	}, (held2, at4) => {
		return item2(held2, at4);
	}, (held2, at4, read) => {
		return lend(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change(held2, at4, with2, $a);
	} ]);
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend2(cell.v, read);
}
function viewing2(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage(storage, read) {
	const $c = storage;
	let $d = null;
	if ($c[0] === 0) {
		const cell = $c[1];
		$d = viewing2(cell, read);
	} else {
		const port = $c[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $e = out;
		let $f = null;
		if ($e[0] === 0) {
			const value = __clone($e[1]);
			$f = value;
		} else {
			$f = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function follow_id(storage, watch, holds, at4) {
	if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	}) && !(peek_storage(storage, (held2) => {
		return holds(held2, at4);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $k = as_usize(id);
	__at(watch.v[1], $k, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $k, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $m = __clone(self);
	let $l = null;
	if (id < 0) {
		$l = -(1);
	} else {
		$l = id;
	}
	return [ $m, $l ];
}
function follow3(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at4));
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
function release3(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot(self, at4, $b) {
	const handle = follow3(self, at4);
	defer_to_owner(() => {
		return release3(handle);
	}, $b);
	return handle;
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held2) => {
		return locate(held2[0], self[1]);
	});
}
function peek_storage2(storage, read) {
	const $t = storage;
	let $u = null;
	if ($t[0] === 0) {
		const cell = $t[1];
		$u = viewing2(cell, read);
	} else {
		const port = $t[1];
		let out = [ 1 ];
		port[0]((held2) => {
			return out = [ 0, read(held2) ];
		});
		const $v = out;
		let $w = null;
		if ($v[0] === 0) {
			const value = __clone($v[1]);
			$w = value;
		} else {
			$w = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$u = $w;
	}
	return $u;
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get(self) {
	const $s = at(self);
	if ($s[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held2) => {
		return read(held2, $s[1]);
	});
}
function slot_cell(watch, slot7) {
	if (slot7 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot7);
	const $x = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell = __clone($x[1]);
	if ($x[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1085:25");
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
function attach_observer(self, observer, immediately) {
	const subscription = on_settle2(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
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
	return peek_storage2(self[0].v[0], read);
}
function size(self) {
	return peek2(self, (held2) => {
		return count2(held2);
	});
}
function coming_one(self, change4) {
	if (self[0].v[4].length === 0) {
		return;
	}
	const taps = __clone(self[0].v[4]);
	for (const tap2 of taps) {
		tap2[1](change4);
	}
}
function splice2(self, at4, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at4, "std/src/reactive/lens.vl:2228:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at4 + offset, __clone(value), "std/src/reactive/lens.vl:2233:9");
		offset = offset + 1;
	}
}
function mutate(self, change4) {
	const $G = self[0].v[0];
	let $H = null;
	if ($G[0] === 0) {
		const cell = $G[1];
		$H = change4(cell.v);
	} else {
		const port = __clone($G[1]);
		$H = port[1](change4);
	}
	return $H;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $al = null;
	if (is_empty(self)) {
		$al = [ 1 ];
	} else {
		$al = __list_get(self, self.length - 1);
	}
	return $al;
}
function notify(self, $ag) {
	const traced = tracing.v;
	let $ah = null;
	if (traced) {
		$ah = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$ah = "";
	}
	const cause = $ah;
	const $ai = $ag;
	let $aj = null;
	if ($ai[0] === 0) {
		const turn = $ai[1];
		$aj = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $am = last(draining_turns.v);
		let $an = null;
		if ($am[0] === 0) {
			const draining = $am[1];
			$an = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $ao = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$ao = undefined;
				}
				$ao;
			}
			$an = undefined;
		}
		$aj = $an;
	}
	return $aj;
}
function wake_all(watch, slot7, $as) {
	const held2 = viewing2(watch, (table2) => {
		return __clone(__at(table2[1], as_usize(slot7), "std/src/reactive/lens.vl:1117:36"));
	});
	const $at = held2[0];
	if ($at[0] === 0) {
		notify($at[1], $as);
	}
	for (const part3 of held2[1]) {
		notify(part3[1], $as);
	}
}
function wake_part(watch, slot7, name, $au) {
	const held2 = viewing2(watch, (table2) => {
		return __clone(__at(table2[1], as_usize(slot7), "std/src/reactive/lens.vl:1127:36"));
	});
	const $av = held2[0];
	if ($av[0] === 0) {
		notify($av[1], $au);
	}
	for (const part3 of held2[1]) {
		if (part3[0] === name) {
			notify(part3[1], $au);
		}
	}
}
function batch(body, $aD) {
	const $aE = $aD;
	let $aF = null;
	if ($aE[0] === 0) {
		const current = $aE[1];
		$aF = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aF = result;
	}
	return $aF;
}
function landed(self, changes, late, $J) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change4 of changes) {
		const $K = change4;
		let $L = null;
		if ($K[0] === 0) {
			const _at = $K[1];
			const removed = $K[2];
			const inserted = $K[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$L = undefined;
		} else if ($K[0] === 1) {
			const _at2 = $K[1];
			const _count = $K[2];
			$L = resized = true;
		} else if ($K[0] === 3) {
			const _at3 = $K[1];
			const _count2 = $K[2];
			$L = resized = true;
		} else if ($K[0] === 6) {
			$L = resized = true;
		} else if ($K[0] === 5) {
			const _from = $K[1];
			const _to = $K[2];
			$L = shaped = true;
		} else {
			$L = undefined;
		}
		$L;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change4, wakes);
		}
		let $ab = null;
		if (self[0].v[4].length > 0) {
			let $aa = null;
			if (!(late)) {
				$aa = [ 0 ];
			} else if (!(several)) {
				$aa = [ 1 ];
			} else if (index2 === 0) {
				$aa = [ 2 ];
			} else {
				$aa = [ 3 ];
			}
			const stage = $aa;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change4, stage);
			}
			$ab = undefined;
		}
		$ab;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $ac = null;
	if (shaped || resized) {
		$ac = watch.v[2];
	} else {
		$ac = [ 1 ];
	}
	const shape = __clone($ac);
	let $ad = null;
	if (resized) {
		$ad = watch.v[3];
	} else {
		$ad = [ 1 ];
	}
	const sized = __clone($ad);
	let $ae = undefined;
	let $af = undefined;
	if (wakes.length === 0 && (($ae = shape), $ae[0] === 1) && (($af = sized), $af[0] === 1)) {
		notify(published, $J);
		return;
	}
	settle_together(($ap) => {
		for (const wake2 of wakes) {
			const $aq = wake2;
			let $ar = null;
			if ($aq[0] === 0) {
				const slot7 = $aq[1];
				$ar = wake_all(watch, slot7, [ 0, $ap ]);
			} else if ($aq[0] === 2) {
				const slot8 = $aq[1];
				$ar = wake_all(watch, slot8, [ 0, $ap ]);
			} else {
				const slot9 = $aq[1];
				const part3 = $aq[2];
				$ar = wake_part(watch, slot9, part3, [ 0, $ap ]);
			}
			$ar;
		}
		const $aw = sized;
		if ($aw[0] === 0) {
			notify($aw[1], [ 0, $ap ]);
		}
		const $ax = shape;
		if ($ax[0] === 0) {
			notify($ax[1], [ 0, $ap ]);
		}
		notify(published, [ 0, $ap ]);
		return;
	}, $J);
}
function landed_one(self, change4, $I) {
	landed(self, [ __clone(change4) ], false, $I);
}
function splice3(self, at4, removed, inserted, $z) {
	const held2 = size(self);
	let $E = null;
	if (at4 > held2) {
		$E = held2;
	} else {
		$E = at4;
	}
	const start = $E;
	let $F = null;
	if (start + removed > held2) {
		$F = held2 - start;
	} else {
		$F = removed;
	}
	const taking = $F;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change4 = [ 0, start, taking, arriving ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return splice2(held3, start, taking, inserted);
	});
	landed_one(self, change4, $z);
}
function insert_at(self, at4, value, $y) {
	return splice3(self, at4, 0, [ __clone(value) ], $y);
}
function eq5(self, b2) {
	const $aM = self;
	let $aP = null;
	if ($aM[0] === 0) {
		const $aN = b2;
		let $aO = null;
		if ($aN[0] === 0) {
			$aO = eq2($aM[1], $aN[1]);
		} else {
			$aO = false;
		}
		$aP = $aO;
	} else {
		const $aQ = b2;
		$aP = $aQ[0] === 1;
	}
	return $aP;
}
function peek3(self, read) {
	return peek_storage(self[0].v[0], read);
}
function has(self, at4) {
	const holds = self[0].v[1][0];
	return peek3(self, (held2) => {
		return holds(held2, at4);
	});
}
function replace(self, at4, value, $ba) {
	const watch = self[0].v[3];
	let $bb = null;
	if (has(self, at4)) {
		$bb = [ 2, __clone(at4) ];
	} else if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$bb = [ 1, __clone(at4), 1 ];
	}
	const change4 = $bb;
	coming_one(self, change4);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed_one(self, change4, $ba);
	return true;
}
function set(self, value, $aY) {
	const $aZ = at(self);
	if ($aZ[0] === 0) {

	} else {
		return false;
	}
	return replace(self[0], $aZ[1], value, $aY);
}
function all_of(storage) {
	return peek_storage2(storage, (held2) => {
		let out = [  ];
		let at4 = 0;
		const count3 = count2(held2);
		while (at4 < count3) {
			const $bc = item2(held2, at4);
			if ($bc[0] === 0) {
				out.push($bc[1]);
			}
			at4 = at4 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0].v[0]);
}
function remove_at(self, at4, $bh) {
	return splice3(self, at4, 1, [  ], $bh);
}
function push(self, value, $bi) {
	return splice3(self, size(self), 0, [ __clone(value) ], $bi);
}
function lend3(self, at4, read) {
	if (at4 < self.length) {
		read(__at(self, at4, "std/src/reactive/lens.vl:2219:10"));
	}
}
function change2(self, at4, with2) {
	if (at4 < self.length) {
		with2(__at(self, at4, "std/src/reactive/lens.vl:2244:14"));
	}
}
function lens_over2(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new8(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over2(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function sequence2(value, $a) {
	return over2(value, new5(), [ (held2, at4) => {
		return at4 < count2(held2);
	}, (held2, at4) => {
		return item2(held2, at4);
	}, (held2, at4, read) => {
		return lend3(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change2(held2, at4, with2, $a);
	} ]);
}
function follow_id2(storage, watch, holds, at4) {
	if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	}) && !(peek_storage2(storage, (held2) => {
		return holds(held2, at4);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $bn = as_usize(id);
	__at(watch.v[1], $bn, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $bn, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function follow4(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at4));
}
function release4(self) {
	release_id(self[0][0].v[3], self[1]);
}
function slot2(self, at4, $b) {
	const handle = follow4(self, at4);
	defer_to_owner(() => {
		return release4(handle);
	}, $b);
	return handle;
}
function part_cell(watch, slot7, name) {
	if (slot7 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1094:19");
	}
	const found = viewing2(watch, (held2) => {
		let out = [ 1 ];
		for (const part3 of __at(held2[1], as_usize(slot7), "std/src/reactive/lens.vl:1098:15")[1]) {
			if (part3[0] === name) {
				out = [ 0, __clone(part3[1]) ];
			}
		}
		return out;
	});
	const $bq = found;
	if ($bq[0] === 0) {
		return $bq[1];
	}
	const woke = new8(0, "std/src/reactive/lens.vl:1108:25");
	const entry = as_usize(slot7);
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1110:2")[1].push([ name, __clone(woke) ]);
	return woke;
}
function part(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function get2(self) {
	const $bt = at(self[0]);
	if ($bt[0] === 0) {

	} else {
		return [ 1 ];
	}
	const view = self[0][0][0].v[1][2];
	const read = self[2];
	let out = [ 1 ];
	peek2(self[0][0], (held2) => {
		return view(held2, $bt[1], (item3) => {
			return out = [ 0, read(item3) ];
		});
	});
	return out;
}
function on_settle3(self, subscriber) {
	return on_settle(self[1], subscriber);
}
function attach_observer2(self, observer, immediately) {
	const subscription = on_settle3(self, mint_subscriber(() => {
		return observer(get2(self));
	}));
	if (immediately) {
		observer(get2(self));
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
function has2(self, at4) {
	const holds = self[0].v[1][0];
	return peek2(self, (held2) => {
		return holds(held2, at4);
	});
}
function landed2(self, changes, late, $J) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change4 of changes) {
		const $bH = change4;
		let $bI = null;
		if ($bH[0] === 0) {
			const _at = $bH[1];
			const removed = $bH[2];
			const inserted = $bH[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bI = undefined;
		} else if ($bH[0] === 1) {
			const _at2 = $bH[1];
			const _count = $bH[2];
			$bI = resized = true;
		} else if ($bH[0] === 3) {
			const _at3 = $bH[1];
			const _count2 = $bH[2];
			$bI = resized = true;
		} else if ($bH[0] === 6) {
			$bI = resized = true;
		} else if ($bH[0] === 5) {
			const _from = $bH[1];
			const _to = $bH[2];
			$bI = shaped = true;
		} else {
			$bI = undefined;
		}
		$bI;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change4, wakes);
		}
		let $bK = null;
		if (self[0].v[4].length > 0) {
			let $bJ = null;
			if (!(late)) {
				$bJ = [ 0 ];
			} else if (!(several)) {
				$bJ = [ 1 ];
			} else if (index2 === 0) {
				$bJ = [ 2 ];
			} else {
				$bJ = [ 3 ];
			}
			const stage = $bJ;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change4, stage);
			}
			$bK = undefined;
		}
		$bK;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $bL = null;
	if (shaped || resized) {
		$bL = watch.v[2];
	} else {
		$bL = [ 1 ];
	}
	const shape = __clone($bL);
	let $bM = null;
	if (resized) {
		$bM = watch.v[3];
	} else {
		$bM = [ 1 ];
	}
	const sized = __clone($bM);
	let $bN = undefined;
	let $bO = undefined;
	if (wakes.length === 0 && (($bN = shape), $bN[0] === 1) && (($bO = sized), $bO[0] === 1)) {
		notify(published, $J);
		return;
	}
	settle_together(($ap) => {
		for (const wake2 of wakes) {
			const $bP = wake2;
			let $bQ = null;
			if ($bP[0] === 0) {
				const slot7 = $bP[1];
				$bQ = wake_all(watch, slot7, [ 0, $ap ]);
			} else if ($bP[0] === 2) {
				const slot8 = $bP[1];
				$bQ = wake_all(watch, slot8, [ 0, $ap ]);
			} else {
				const slot9 = $bP[1];
				const part3 = $bP[2];
				$bQ = wake_part(watch, slot9, part3, [ 0, $ap ]);
			}
			$bQ;
		}
		const $bR = sized;
		if ($bR[0] === 0) {
			notify($bR[1], [ 0, $ap ]);
		}
		const $bS = shape;
		if ($bS[0] === 0) {
			notify($bS[1], [ 0, $ap ]);
		}
		notify(published, [ 0, $ap ]);
		return;
	}, $J);
}
function landed_one2(self, change4, $I) {
	landed2(self, [ __clone(change4) ], false, $I);
}
function edit(self, at4, part3, change4, $bE) {
	if (!(has2(self, at4))) {
		return false;
	}
	coming_one(self, [ 4, __clone(at4), part3 ]);
	const modify = self[0].v[1][4];
	mutate(self, (held2) => {
		return modify(held2, at4, change4);
	});
	landed_one2(self, [ 4, __clone(at4), part3 ], $bE);
	return true;
}
function edit2(self, part3, change4, $bC) {
	const $bD = at(self);
	if ($bD[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $bD[1], part3, change4, $bC);
}
function size2(self) {
	return peek2(self, (held2) => {
		return count2(held2);
	});
}
function splice4(self, at4, removed, inserted, $z) {
	const held2 = size2(self);
	let $bX = null;
	if (at4 > held2) {
		$bX = held2;
	} else {
		$bX = at4;
	}
	const start = $bX;
	let $bY = null;
	if (start + removed > held2) {
		$bY = held2 - start;
	} else {
		$bY = removed;
	}
	const taking = $bY;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change4 = [ 0, start, taking, arriving ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return splice2(held3, start, taking, inserted);
	});
	landed_one2(self, change4, $z);
}
function insert_at2(self, at4, value, $y) {
	return splice4(self, at4, 0, [ __clone(value) ], $y);
}
function audit(self, before, changes) {
	const $cm = self[0].v[6];
	const check = $cm[1];
	if ($cm[0] === 0) {

	} else {
		return;
	}
	const $cn = before;
	if ($cn[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held2) => {
		return check($cn[1], held2, changes);
	});
	const $cs = problem;
	if ($cs[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $cs[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function update(self, change4, $cd) {
	const $ce = self[0].v[6];
	const _check = $ce[1];
	let $cj = null;
	if ($ce[0] === 0) {
		$cj = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$cj = [ 1 ];
	}
	const before = $cj;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change4(held2) ];
	});
	const $ck = out;
	let $cl = null;
	if ($ck[0] === 0) {
		const result = __clone($ck[1][0]);
		const changes = __clone($ck[1][1]);
		audit(self, before, changes);
		landed2(self, changes, true, $cd);
		$cl = result;
	} else {
		$cl = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $cl;
}
function get3(self) {
	const $ct = at(self);
	if ($ct[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek2(self[0], (held2) => {
		return read(held2, $ct[1]);
	});
}
function eq6(self, b2) {
	const $cy = self;
	let $cB = null;
	if ($cy[0] === 0) {
		const $cz = b2;
		let $cA = null;
		if ($cz[0] === 0) {
			$cA = eq4($cy[1], $cz[1]);
		} else {
			$cA = false;
		}
		$cB = $cA;
	} else {
		const $cC = b2;
		$cB = $cC[0] === 1;
	}
	return $cB;
}
function get4(self) {
	return peek2(self, (held2) => {
		return __clone(held2);
	});
}
function on_settle4(self, subscriber) {
	return on_settle(self[0].v[2], subscriber);
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
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
function length(self) {
	return [ __clone(self) ];
}
function get5(self) {
	return peek2(self[0], (held2) => {
		return count2(held2);
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $cH = watch.v[3];
	const cell = __clone($cH[1]);
	if ($cH[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle5(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
}
function attach_observer4(self, observer, immediately) {
	const subscription = on_settle5(self, mint_subscriber(() => {
		return observer(get5(self));
	}));
	if (immediately) {
		observer(get5(self));
	}
	return subscription;
}
function on_change4(self, observer, caller) {
	return place_subscription(attach_observer4(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function replace2(self, at4, value, $ba) {
	const watch = self[0].v[3];
	let $cI = null;
	if (has2(self, at4)) {
		$cI = [ 2, __clone(at4) ];
	} else if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$cI = [ 1, __clone(at4), 1 ];
	}
	const change4 = $cI;
	coming_one(self, change4);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed_one2(self, change4, $ba);
	return true;
}
function push2(self, value, $bi) {
	return splice4(self, size2(self), 0, [ __clone(value) ], $bi);
}
function shift(self, from, to) {
	const item3 = __remove_at(self, from, "std/src/reactive/lens.vl:2249:19");
	__insert_at(self, to, item3, "std/src/reactive/lens.vl:2250:8");
}
function move_item(self, from, to, $cL) {
	const held2 = size(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $cM = null;
	if (to >= held2) {
		$cM = held2 - 1;
	} else {
		$cM = to;
	}
	const landing = $cM;
	const change4 = [ 5, from, landing ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return shift(held3, from, landing, $cL);
	});
	landed_one(self, change4, $cL);
}
function new9() {
	const table2 = [ new Map(), undefined ];
	return [ table2 ];
}
function insert(self, key2, value) {
	__nmap_insert(self[0], hash(key2), __clone(key2), __clone(value));
}
function contains_key(self, key2) {
	return __nmap_has(self[0], hash(key2));
}
function reconcile_by_key(self, next, key2, $cN) {
	const lens2 = __clone(self);
	batch(($cO) => {
		let wanted = new9();
		for (const $cP of next) {
			const item3 = [ $cP ];
			insert(wanted, key2([ item3, 0 ]), true);
		}
		let keys2 = [  ];
		for (const $cQ of all(lens2)) {
			const item4 = [ $cQ ];
			keys2.push(key2([ item4, 0 ]));
		}
		let at4 = keys2.length;
		while (at4 > 0) {
			at4 = at4 - 1;
			if (!(contains_key(wanted, __at(keys2, at4, "std/src/reactive/lens.vl:1907:29")))) {
				remove_at(lens2, at4, [ 0, $cO ]);
				__remove_at(keys2, at4, "std/src/reactive/lens.vl:1909:11");
			}
		}
		let index2 = 0;
		for (const $cR of next) {
			const item5 = [ $cR ];
			const want = key2([ item5, 0 ]);
			let $cS = null;
			if (index2 < keys2.length && __at(keys2, index2, "std/src/reactive/lens.vl:1915:30") === want) {
				if (!(eq3(peek(lens2, (list2) => {
					return item2(list2, index2);
				}), [ 0, __clone(item5[0]) ]))) {
					replace(lens2, index2, item5[0], [ 0, $cO ]);
				}
				$cS = undefined;
			} else {
				let found = [ 1 ];
				let search = index2 + 1;
				while (search < keys2.length) {
					if (__at(keys2, search, "std/src/reactive/lens.vl:1923:10") === want) {
						found = [ 0, search ];
						search = keys2.length;
					}
					search = search + 1;
				}
				const $cT = found;
				let $cU = null;
				if ($cT[0] === 0) {
					const from = $cT[1];
					move_item(lens2, from, index2, [ 0, $cO ]);
					const moved = __remove_at(keys2, from, "std/src/reactive/lens.vl:1932:25");
					__insert_at(keys2, index2, moved, "std/src/reactive/lens.vl:1933:13");
					if (!(eq3(peek(lens2, (list2) => {
						return item2(list2, index2);
					}), [ 0, __clone(item5[0]) ]))) {
						replace(lens2, index2, item5[0], [ 0, $cO ]);
					}
					$cU = undefined;
				} else {
					insert_at(lens2, index2, item5[0], [ 0, $cO ]);
					__insert_at(keys2, index2, want, "std/src/reactive/lens.vl:1940:13");
					$cU = undefined;
				}
				$cS = $cU;
			}
			$cS;
			index2 = index2 + 1;
		}
		return;
	}, $cN);
}
function over3(value, handler, access) {
	return lens_over2([ 0, __shared_new(value) ], handler, access);
}
function sequence3(value, $a) {
	return over3(value, new5(), [ (held2, at4) => {
		return at4 < count2(held2);
	}, (held2, at4) => {
		return item2(held2, at4);
	}, (held2, at4, read) => {
		return lend(held2, at4, read);
	}, (held2, at4, value2) => {
		return put(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change(held2, at4, with2, $a);
	} ]);
}
function new10() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function arrived_at(storage, at4, count3) {
	return peek_storage2(storage, (held2) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count3) {
			const $cZ = item2(held2, index2);
			if ($cZ[0] === 0) {
				arrived.push($cZ[1]);
			}
			index2 = index2 + 1;
		}
		return arrived;
	});
}
function one(item3) {
	const $di = item3;
	let $dj = null;
	if ($di[0] === 0) {
		const value = $di[1];
		$dj = [ __clone(value) ];
	} else {
		$dj = [  ];
	}
	return $dj;
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor6 of self[3].v) {
		const at4 = cursor6[1].v;
		if (at4 < lowest) {
			lowest = at4;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index2 = as_usize(dropped);
		const held2 = __clone(self[0].v);
		const length5 = held2.length;
		while (index2 < length5) {
			const $dp = __list_get(held2, index2);
			let $dq = null;
			if ($dp[0] === 0) {
				const op = $dp[1];
				$dq = kept.push(op);
			} else {
				$dq = undefined;
			}
			$dq;
			index2 = index2 + 1;
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
function record_set(log, storage, leaving, at4, late) {
	if (late) {
		record(log, [ 2, all_of(storage) ]);
		return;
	}
	const before = __list_get(leaving.v, 0);
	const after = peek_storage2(storage, (held2) => {
		return item2(held2, at4);
	});
	const $du = before;
	let $dv = undefined;
	if ($du[0] === 0 && (($dv = after), $dv[0] === 0)) {
		record(log, [ 1, at4, __clone($du[1]), __clone($dv[1]) ]);
	}
}
function tap(self, before, after) {
	const id = self[0].v[5];
	self[0].v[5] = id + 1;
	self[0].v[4].push([ id, before, after ]);
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
function release5(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $cW) {
	const leaving = __shared_new([  ]);
	const runs2 = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0].v[0]);
	const tap2 = tap(self, (change4) => {
		const $cX = change4;
		let $cY = null;
		if ($cX[0] === 0) {
			const at4 = $cX[1];
			const removed = $cX[2];
			const _inserted = $cX[3];
			leaving.v = arrived_at(storage, at4, removed);
			$cY = undefined;
		} else if ($cX[0] === 3) {
			const at5 = $cX[1];
			const count3 = $cX[2];
			leaving.v = arrived_at(storage, at5, count3);
			$cY = undefined;
		} else if ($cX[0] === 2) {
			const at6 = $cX[1];
			leaving.v = one(peek_storage2(storage, (held2) => {
				return item2(held2, at6);
			}));
			$cY = undefined;
		} else if ($cX[0] === 4) {
			const at7 = $cX[1];
			const _part = $cX[2];
			leaving.v = one(peek_storage2(storage, (held2) => {
				return item2(held2, at7);
			}));
			$cY = undefined;
		} else if ($cX[0] === 5) {
			const from = $cX[1];
			const to = $cX[2];
			$cY = extend_run(runs2, from, to);
		} else {
			$cY = undefined;
		}
		return $cY;
	}, (change4, stage) => {
		const $dm = stage;
		let $dn = null;
		if ($dm[0] === 2) {
			record(log, [ 2, all_of(storage) ]);
			return;
			$dn = undefined;
		} else if ($dm[0] === 3) {
			return;
		} else {
			$dn = undefined;
		}
		$dn;
		const $dr = stage;
		const late = $dr[0] === 1;
		const $ds = change4;
		let $dt = null;
		if ($ds[0] === 0) {
			const at4 = $ds[1];
			const removed = $ds[2];
			const inserted = $ds[3];
			if (late && removed > 0) {
				record(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record(log, [ 0, at4, [  ], arrived_at(storage, at4, inserted) ]);
			} else {
				record(log, [ 0, at4, __clone(leaving.v), arrived_at(storage, at4, inserted) ]);
			}
			$dt = undefined;
		} else if ($ds[0] === 1) {
			const at5 = $ds[1];
			const count3 = $ds[2];
			record(log, [ 0, at5, [  ], arrived_at(storage, at5, count3) ]);
			$dt = undefined;
		} else if ($ds[0] === 3) {
			const at6 = $ds[1];
			const _count = $ds[2];
			if (late) {
				record(log, [ 2, all_of(storage) ]);
			} else {
				record(log, [ 0, at6, __clone(leaving.v), [  ] ]);
			}
			$dt = undefined;
		} else if ($ds[0] === 2) {
			const at7 = $ds[1];
			$dt = record_set(log, storage, leaving, at7, late);
		} else if ($ds[0] === 4) {
			const at8 = $ds[1];
			const _part = $ds[2];
			$dt = record_set(log, storage, leaving, at8, late);
		} else if ($ds[0] === 5) {
			const from = $ds[1];
			const to = $ds[2];
			if (skipping.v > 0) {
				const $dw = [ skipping, "v" ];
				skipping.v = $dw[0][$dw[1]] - 1;
				return;
			}
			if (runs2.v.length === 0) {
				record(log, [ 3, from, 1, to ]);
				return;
			}
			const $dx = __remove_at(runs2.v, 0, "std/src/reactive/lens.vl:1676:47");
			const start = $dx[0];
			const count4 = $dx[1];
			const first2 = $dx[2];
			skipping.v = count4 - 1;
			let $dy = null;
			if (first2 > start) {
				$dy = first2 + 1 - count4;
			} else {
				$dy = first2;
			}
			const landing = $dy;
			record(log, [ 3, start, count4, landing ]);
			$dt = undefined;
		} else {
			$dt = record(log, [ 2, all_of(storage) ]);
		}
		return $dt;
	});
	const feed4 = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed4);
	}, $cW);
	return feed4;
}
function items(self, $cV) {
	return items_into(self, new10(), $cV);
}
function map(self, transform) {
	return [ self, transform ];
}
function cursor(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function cursor2(self) {
	return cursor(self[1]);
}
function since(self, cursor6) {
	const at4 = cursor6[1].v;
	const version = self[1].v;
	let ops2 = [  ];
	if (at4 >= version) {
		return [ 0, ops2 ];
	}
	cursor6[1].v = version;
	const base = self[2].v;
	if (at4 < base) {
		return [ 1 ];
	}
	let index2 = as_usize(at4 - base);
	const held2 = __clone(self[0].v);
	const length5 = held2.length;
	while (index2 < length5) {
		const $dD = __list_get(held2, index2);
		let $dE = null;
		if ($dD[0] === 0) {
			const op = $dD[1];
			$dE = ops2.push(op);
		} else {
			$dE = undefined;
		}
		$dE;
		index2 = index2 + 1;
	}
	return [ 0, ops2 ];
}
function all2(self) {
	return all_of(self[0].v[0]);
}
function since2(self, cursor6) {
	const $dF = since(self[1], cursor6);
	let $dG = null;
	if ($dF[0] === 0) {
		const recorded = $dF[1];
		$dG = recorded;
	} else {
		$dG = [ [ 2, all2(self[0]) ] ];
	}
	return $dG;
}
function reader(self) {
	return (cursor6) => {
		return since2(self, cursor6);
	};
}
function get6(self) {
	return all2(self[0]);
}
function on_settle6(self, subscriber) {
	return on_settle(self[0][0].v[2], subscriber);
}
function drop_cursor(self, cursor6) {
	let kept = [  ];
	for (const held2 of self[3].v) {
		if (held2[0] !== cursor6[0]) {
			kept.push(__clone(held2));
		}
	}
	self[3].v = kept;
}
function drop_cursor2(self, cursor6) {
	drop_cursor(self[1], cursor6);
}
function open2(self) {
	const cursor6 = cursor2(self);
	const read = reader(self);
	return [ () => {
		return get6(self);
	}, (subscriber) => {
		return on_settle6(self, subscriber);
	}, () => {
		return read(cursor6);
	}, () => {
		return drop_cursor2(self, cursor6);
	} ];
}
function run_element(stage, body) {
	const nursery2 = stage_nursery(stage);
	const value = (($dM) => {
		return (($dN) => {
			return body($dM, $dN);
		})(nursery2);
	})(stage);
	return [ value, split_registrations(stage) ];
}
function start_element_value(self) {
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
function keep_hold(core, id, hold) {
	element_holds_allocated_count.v = element_holds_allocated_count.v + 1;
	core[3].v.set(hash2(id), __clone(hold));
}
function admit(core, item3) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $dP = core[8](item3);
	const answer = $dP[0];
	const owner2 = $dP[1];
	const $dQ = answer;
	let $dR = null;
	if ($dQ[0] === 0) {
		const value = $dQ[1];
		const $dS = owner2;
		let $dT = null;
		if ($dS[0] === 0) {
			const held2 = $dS[1];
			$dT = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held2) ] ]);
		} else {
			$dT = undefined;
		}
		$dT;
		$dR = [ id, __clone(value) ];
	} else {
		const instance = $dQ[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $dU = consumer.v;
			let $dV = null;
			if ($dU[0] === 0) {
				const downstream = $dU[1];
				$dV = wake(downstream);
			} else {
				$dV = undefined;
			}
			return $dV;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner2 ]);
		$dR = [ id, pull() ];
	}
	return $dR;
}
function seed(core, items2) {
	for (const item3 of items2) {
		const $ea = admit(core, item3);
		const id = $ea[0];
		const value = $ea[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item3));
		core[2].v.push(value);
	}
}
function fresh_list(list2) {
	let copy = [  ];
	for (const item3 of list2) {
		copy.push(__clone(item3));
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
function held_pairs(core, at4, count3) {
	let pairs = [  ];
	let index2 = at4;
	while (index2 < at4 + count3) {
		pairs.push([ __clone(__at(core[1].v, index2, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index2, "std/src/reactive/delta.vl:1618:41")) ]);
		index2 = index2 + 1;
	}
	return pairs;
}
function is_none(self) {
	const $ew = self;
	return $ew[0] === 1;
}
function retire(core, id) {
	const $ef = __map_get(core[3].v, hash2(id));
	let $eg = null;
	if ($ef[0] === 0) {
		const hold = $ef[1];
		core[3].v.delete(hash2(id));
		const $eh = hold[2];
		let $ei = null;
		if ($eh[0] === 0) {
			const handle = $eh[1];
			$ei = detach(handle);
		} else {
			$ei = undefined;
		}
		$ei;
		hold[3]();
		const $es = hold[4];
		let $et = null;
		if ($es[0] === 0) {
			const owner2 = $es[1];
			$et = dispose2(owner2);
		} else {
			$et = undefined;
		}
		$eg = $et;
	} else {
		$eg = undefined;
	}
	return $eg;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops2) {
	let events = [  ];
	for (const op of ops2) {
		const $eb = op;
		let $ec = null;
		if ($eb[0] === 0) {
			const at4 = $eb[1];
			const removed = $eb[2];
			const inserted = $eb[3];
			const held2 = core[0].v.length;
			const start = splice_start(held2, at4);
			const taking = splice_count(held2, start, removed.length);
			const left = held_pairs(core, start, taking);
			let taken = 0;
			while (taken < taking) {
				const id = __remove_at(core[0].v, start, "std/src/reactive/delta.vl:1642:32");
				retire(core, id);
				__remove_at(core[1].v, start, "std/src/reactive/delta.vl:1644:37");
				__remove_at(core[2].v, start, "std/src/reactive/delta.vl:1645:39");
				taken = taken + 1;
			}
			let arrived = [  ];
			let offset = 0;
			for (const item3 of inserted) {
				const $eE = admit(core, item3);
				const id2 = $eE[0];
				const value = $eE[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item3), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item3), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$ec = undefined;
		} else if ($eb[0] === 1) {
			const at5 = $eb[1];
			const _was = $eb[2];
			const item4 = $eb[3];
			const $eF = __list_get(core[0].v, at5);
			let $eG = null;
			if ($eF[0] === 0) {
				const old_id = $eF[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $eH = admit(core, item4);
				const id3 = $eH[0];
				const value2 = $eH[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item4), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item4), value2 ], true ]);
				$eG = undefined;
			} else {
				$eG = undefined;
			}
			$ec = $eG;
		} else if ($eb[0] === 2) {
			const items2 = $eb[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$ec = undefined;
		} else {
			const from = $eb[1];
			const count3 = $eb[2];
			const to = $eb[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count3) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count3) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count3, to ]);
			$ec = undefined;
		}
		$ec;
	}
	const woke = __clone(core[4].v);
	let $eM = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index2 = 0;
		for (const id5 of core[0].v) {
			positions.set(hash2(id5), index2);
			index2 = index2 + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash2(id6))) {
				continue;
			}
			seen.set(hash2(id6), true);
			const $eI = __map_get(positions, hash2(id6));
			let $eJ = null;
			if ($eI[0] === 0) {
				const at6 = $eI[1];
				const $eK = __map_get(core[3].v, hash2(id6));
				let $eL = null;
				if ($eK[0] === 0) {
					const hold = $eK[1];
					if (hold[0]) {
						const item5 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item5), before2 ], [ item5, now ], false ]);
					}
					$eL = undefined;
				} else {
					$eL = undefined;
				}
				$eJ = $eL;
			} else {
				$eJ = undefined;
			}
			$eJ;
		}
		$eM = undefined;
	}
	$eM;
	return events;
}
function map2(self, fn) {
	let result = [  ];
	for (const item3 of self) {
		result.push(fn(item3));
	}
	return result;
}
function mapped_ops(events) {
	let ops2 = [  ];
	for (const event of events) {
		const $eN = event;
		let $eO = null;
		if ($eN[0] === 0) {
			const at4 = $eN[1];
			const left = $eN[2];
			const arrived = $eN[3];
			ops2.push([ 0, at4, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$eO = undefined;
		} else if ($eN[0] === 1) {
			const at5 = $eN[1];
			const before = $eN[2];
			const after = $eN[3];
			const _rerun = $eN[4];
			ops2.push([ 1, at5, __clone(before[1]), __clone(after[1]) ]);
			$eO = undefined;
		} else if ($eN[0] === 2) {
			const _left = $eN[1];
			const now = $eN[2];
			$eO = ops2.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $eN[1];
			const count3 = $eN[2];
			const to = $eN[3];
			$eO = ops2.push([ 3, from, count3, to ]);
		}
		$eO;
	}
	return ops2;
}
function release_core(core) {
	for (const id of __clone(core[0].v)) {
		retire(core, id);
	}
	dispose2(core[6]);
}
function core_release(core, upstream) {
	const release11 = upstream[3];
	return () => {
		release_core(core);
		release11();
		return;
	};
}
function open3(self) {
	const transform = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item3) => {
		const $dO = run_element(stage, ($dH, $dI) => {
			return transform(item3, $dH, $dI);
		});
		const answer = $dO[0];
		const owner2 = $dO[1];
		return [ start_element_value(answer), owner2 ];
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
function of(elements) {
	return list_cell(elements, new10());
}
function size3(self) {
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
		const $eV = __list_get(inserted, offset);
		let $eW = null;
		if ($eV[0] === 0) {
			const value = $eV[1];
			$eW = __insert_at(list2, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$eW = undefined;
		}
		$eW;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at2(self) {
	return self[1].v;
}
function set2(self, value, $eY, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $eY);
}
function publish(cell, $eX) {
	set2(cell[1], at2(cell[2]), $eX, "std/src/reactive/delta.vl:799:17");
}
function splice5(self, at4, removed, inserted, $eU) {
	const held2 = size3(self);
	const start = splice_start(held2, at4);
	const taking = splice_count(held2, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $eU);
}
function set_at(self, at4, value, $eZ) {
	const $fa = __list_get(self[0].v, at4);
	let $fb = null;
	if ($fa[0] === 0) {
		const previous = $fa[1];
		__at_put(self[0].v, at4, __clone(value), "std/src/reactive/delta.vl:855:5");
		record(self[2], [ 1, at4, previous, __clone(value) ]);
		publish(self, $eZ);
		$fb = undefined;
	} else {
		$fb = undefined;
	}
	return $fb;
}
function set3(self, value, $fc, caller) {
	self[0].v = __clone(value);
	record(self[2], [ 2, __clone(value) ]);
	publish(self, $fc);
}
function move_range(self, from, count3, to, $fd) {
	if (count3 === 0 || from === to) {
		return;
	}
	let lifted = [  ];
	let taken = 0;
	while (taken < count3) {
		lifted.push(__remove_at(self[0].v, from, "std/src/reactive/delta.vl:875:35"));
		taken = taken + 1;
	}
	let offset = 0;
	while (offset < lifted.length) {
		const $fe = __list_get(lifted, offset);
		let $ff = null;
		if ($fe[0] === 0) {
			const value = $fe[1];
			$ff = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$ff = undefined;
		}
		$ff;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count3, to ]);
	publish(self, $fd);
}
function apply_ops(cell, ops2, $eR) {
	for (const op of ops2) {
		const $eS = op;
		let $eT = null;
		if ($eS[0] === 0) {
			const at4 = $eS[1];
			const removed = $eS[2];
			const inserted = $eS[3];
			$eT = splice5(cell, at4, removed.length, inserted, $eR);
		} else if ($eS[0] === 1) {
			const at5 = $eS[1];
			const _was = $eS[2];
			const value = $eS[3];
			$eT = set_at(cell, at5, value, $eR);
		} else if ($eS[0] === 2) {
			const items2 = $eS[1];
			$eT = set3(cell, items2, $eR, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $eS[1];
			const count3 = $eS[2];
			const to = $eS[3];
			$eT = move_range(cell, from, count3, to, $eR);
		}
		$eT;
	}
}
function take(self, item3, $fm) {
	defer(self, () => {
		dispose(item3, $fm);
		return;
	});
	return __clone(item3);
}
function seal_collection(instance, owned, $eP, $eQ, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $eP);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $eP, $eQ);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $eP);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $dB, $dC, caller) {
	return [ seal_collection(open3(self), true, $dB, $dC, caller) ];
}
function filter(self, keep) {
	return [ self, keep ];
}
function start_element(self) {
	return [ 0, self ];
}
function admit2(core, item3) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $fv = core[8](item3);
	const answer = $fv[0];
	const owner2 = $fv[1];
	const $fw = answer;
	let $fx = null;
	if ($fw[0] === 0) {
		const value = $fw[1];
		const $fy = owner2;
		let $fz = null;
		if ($fy[0] === 0) {
			const held2 = $fy[1];
			$fz = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held2) ] ]);
		} else {
			$fz = undefined;
		}
		$fz;
		$fx = [ id, __clone(value) ];
	} else {
		const instance = $fw[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $fA = consumer.v;
			let $fB = null;
			if ($fA[0] === 0) {
				const downstream = $fA[1];
				$fB = wake(downstream);
			} else {
				$fB = undefined;
			}
			return $fB;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner2 ]);
		$fx = [ id, pull() ];
	}
	return $fx;
}
function seed2(core, items2) {
	for (const item3 of items2) {
		const $fC = admit2(core, item3);
		const id = $fC[0];
		const value = $fC[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item3));
		core[2].v.push(value);
	}
}
function retire2(core, id) {
	const $fH = __map_get(core[3].v, hash2(id));
	let $fI = null;
	if ($fH[0] === 0) {
		const hold = $fH[1];
		core[3].v.delete(hash2(id));
		const $fJ = hold[2];
		let $fK = null;
		if ($fJ[0] === 0) {
			const handle = $fJ[1];
			$fK = detach(handle);
		} else {
			$fK = undefined;
		}
		$fK;
		hold[3]();
		const $fL = hold[4];
		let $fM = null;
		if ($fL[0] === 0) {
			const owner2 = $fL[1];
			$fM = dispose2(owner2);
		} else {
			$fM = undefined;
		}
		$fI = $fM;
	} else {
		$fI = undefined;
	}
	return $fI;
}
function resolve2(core, ops2) {
	let events = [  ];
	for (const op of ops2) {
		const $fF = op;
		let $fG = null;
		if ($fF[0] === 0) {
			const at4 = $fF[1];
			const removed = $fF[2];
			const inserted = $fF[3];
			const held2 = core[0].v.length;
			const start = splice_start(held2, at4);
			const taking = splice_count(held2, start, removed.length);
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
			for (const item3 of inserted) {
				const $fN = admit2(core, item3);
				const id2 = $fN[0];
				const value = $fN[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item3), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item3), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$fG = undefined;
		} else if ($fF[0] === 1) {
			const at5 = $fF[1];
			const _was = $fF[2];
			const item4 = $fF[3];
			const $fO = __list_get(core[0].v, at5);
			let $fP = null;
			if ($fO[0] === 0) {
				const old_id = $fO[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire2(core, old_id);
				const $fQ = admit2(core, item4);
				const id3 = $fQ[0];
				const value2 = $fQ[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item4), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item4), value2 ], true ]);
				$fP = undefined;
			} else {
				$fP = undefined;
			}
			$fG = $fP;
		} else if ($fF[0] === 2) {
			const items2 = $fF[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire2(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed2(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$fG = undefined;
		} else {
			const from = $fF[1];
			const count3 = $fF[2];
			const to = $fF[3];
			let lifted_ids = [  ];
			let lifted_items = [  ];
			let lifted_values = [  ];
			let taken2 = 0;
			while (taken2 < count3) {
				lifted_ids.push(__remove_at(core[0].v, from, "std/src/reactive/delta.vl:1694:39"));
				lifted_items.push(__remove_at(core[1].v, from, "std/src/reactive/delta.vl:1695:43"));
				lifted_values.push(__remove_at(core[2].v, from, "std/src/reactive/delta.vl:1696:45"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			while (offset2 < count3) {
				__insert_at(core[0].v, to + offset2, __at(lifted_ids, offset2, "std/src/reactive/delta.vl:1701:43"), "std/src/reactive/delta.vl:1701:23");
				__insert_at(core[1].v, to + offset2, __clone(__at(lifted_items, offset2, "std/src/reactive/delta.vl:1702:45")), "std/src/reactive/delta.vl:1702:25");
				__insert_at(core[2].v, to + offset2, __clone(__at(lifted_values, offset2, "std/src/reactive/delta.vl:1703:46")), "std/src/reactive/delta.vl:1703:26");
				offset2 = offset2 + 1;
			}
			events.push([ 3, from, count3, to ]);
			$fG = undefined;
		}
		$fG;
	}
	const woke = __clone(core[4].v);
	let $fV = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index2 = 0;
		for (const id5 of core[0].v) {
			positions.set(hash2(id5), index2);
			index2 = index2 + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash2(id6))) {
				continue;
			}
			seen.set(hash2(id6), true);
			const $fR = __map_get(positions, hash2(id6));
			let $fS = null;
			if ($fR[0] === 0) {
				const at6 = $fR[1];
				const $fT = __map_get(core[3].v, hash2(id6));
				let $fU = null;
				if ($fT[0] === 0) {
					const hold = $fT[1];
					if (hold[0]) {
						const item5 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item5), before2 ], [ item5, now ], false ]);
					}
					$fU = undefined;
				} else {
					$fU = undefined;
				}
				$fS = $fU;
			} else {
				$fS = undefined;
			}
			$fS;
		}
		$fV = undefined;
	}
	$fV;
	return events;
}
function kept_ops(kept, index2, events, out, payload_follows) {
	let ops2 = [  ];
	for (const event of events) {
		const $fX = event;
		let $fY = null;
		if ($fX[0] === 0) {
			const at4 = $fX[1];
			const left = $fX[2];
			const arrived = $fX[3];
			const position = kept_before(index2, at4);
			let removed = [  ];
			for (const pair of left) {
				const $fZ = out(pair);
				let $ga = null;
				if ($fZ[0] === 0) {
					const value = $fZ[1];
					$ga = removed.push(value);
				} else {
					$ga = undefined;
				}
				$ga;
			}
			let inserted = [  ];
			let flags = [  ];
			for (const pair2 of arrived) {
				const $gb = out(pair2);
				let $gc = null;
				if ($gb[0] === 0) {
					const value2 = $gb[1];
					inserted.push(value2);
					flags.push(true);
					$gc = undefined;
				} else {
					$gc = flags.push(false);
				}
				$gc;
			}
			let taken = 0;
			while (taken < left.length) {
				__remove_at(kept.v, at4, "std/src/reactive/delta.vl:2153:31");
				taken = taken + 1;
			}
			let offset = 0;
			for (const flag of flags) {
				__insert_at(kept.v, at4 + offset, flag, "std/src/reactive/delta.vl:2158:19");
				offset = offset + 1;
			}
			rebuild_kept(index2, kept.v);
			if (removed.length > 0 || inserted.length > 0) {
				ops2.push([ 0, position, removed, inserted ]);
			}
			$fY = undefined;
		} else if ($fX[0] === 1) {
			const at5 = $fX[1];
			const before = $fX[2];
			const after = $fX[3];
			const rerun = $fX[4];
			const position2 = kept_before(index2, at5);
			const $gd = [ out(before), out(after) ];
			let $ge = null;
			if ($gd[0][0] === 0 && $gd[1][0] === 0) {
				const was = $gd[0][1];
				const now = $gd[1][1];
				if (rerun || payload_follows) {
					ops2.push([ 1, position2, was, now ]);
				}
				$ge = undefined;
			} else if ($gd[0][0] === 0 && $gd[1][0] === 1) {
				const was2 = $gd[0][1];
				__at_put(kept.v, at5, false, "std/src/reactive/delta.vl:2179:7");
				kept_add(index2, at5, -(1));
				ops2.push([ 0, position2, [ was2 ], [  ] ]);
				$ge = undefined;
			} else if ($gd[0][0] === 1 && $gd[1][0] === 0) {
				const now2 = $gd[1][1];
				__at_put(kept.v, at5, true, "std/src/reactive/delta.vl:2184:7");
				kept_add(index2, at5, 1);
				ops2.push([ 0, position2, [  ], [ now2 ] ]);
				$ge = undefined;
			} else {
				$ge = undefined;
			}
			$fY = $ge;
		} else if ($fX[0] === 2) {
			const _left = $fX[1];
			const now3 = $fX[2];
			let flags2 = [  ];
			let values = [  ];
			for (const pair3 of now3) {
				const $gf = out(pair3);
				let $gg = null;
				if ($gf[0] === 0) {
					const value3 = $gf[1];
					values.push(value3);
					flags2.push(true);
					$gg = undefined;
				} else {
					$gg = flags2.push(false);
				}
				$gg;
			}
			kept.v = flags2;
			rebuild_kept(index2, kept.v);
			ops2.push([ 2, values ]);
			$fY = undefined;
		} else {
			const from = $fX[1];
			const count3 = $fX[2];
			const to = $fX[3];
			const out_from = kept_before(index2, from);
			const moved = kept_before(index2, from + count3) - out_from;
			let lifted = [  ];
			let taken2 = 0;
			while (taken2 < count3) {
				lifted.push(__remove_at(kept.v, from, "std/src/reactive/delta.vl:2213:31"));
				taken2 = taken2 + 1;
			}
			let offset2 = 0;
			for (const flag2 of lifted) {
				__insert_at(kept.v, to + offset2, flag2, "std/src/reactive/delta.vl:2218:19");
				offset2 = offset2 + 1;
			}
			rebuild_kept(index2, kept.v);
			const out_to = kept_before(index2, to);
			if (moved > 0 && out_from !== out_to) {
				ops2.push([ 3, out_from, moved, out_to ]);
			}
			$fY = undefined;
		}
		$fY;
	}
	return ops2;
}
function release_core2(core) {
	for (const id of __clone(core[0].v)) {
		retire2(core, id);
	}
	dispose2(core[6]);
}
function core_release2(core, upstream) {
	const release11 = upstream[3];
	return () => {
		release_core2(core);
		release11();
		return;
	};
}
function open4(self) {
	const keep = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item3) => {
		const $fu = run_element(stage, ($fs, $ft) => {
			return keep(item3, $fs, $ft);
		});
		const answer = $fu[0];
		const owner2 = $fu[1];
		return [ start_element(answer), owner2 ];
	});
	seed2(core, upstream[0]());
	const kept = __shared_new(fresh_list(core[2].v));
	const index2 = kept_index(kept.v);
	const drain2 = upstream[2];
	return [ () => {
		let out = [  ];
		let at4 = 0;
		for (const item3 of core[1].v) {
			if (__at(core[2].v, at4, "std/src/reactive/delta.vl:2040:9")) {
				out.push(__clone(item3));
			}
			at4 = at4 + 1;
		}
		return out;
	}, core_attach(core, upstream), () => {
		return kept_ops(kept, index2, resolve2(core, drain2()), (pair) => {
			let $fW = null;
			if (pair[1]) {
				$fW = [ 0, __clone(pair[0]) ];
			} else {
				$fW = [ 1 ];
			}
			return $fW;
		}, false);
	}, core_release2(core, upstream) ];
}
function memo2(self, $dB, $dC, caller) {
	return [ seal_collection(open4(self), true, $dB, $dC, caller) ];
}
function get7(self) {
	return __clone(self[0].v);
}
function get8(self) {
	return get7(self[0]);
}
function landed_one3(self, change4, $I) {
	landed2(self, [ __clone(change4) ], false, $I);
}
function splice6(self, at4, removed, inserted, $z) {
	const held2 = size2(self);
	let $gl = null;
	if (at4 > held2) {
		$gl = held2;
	} else {
		$gl = at4;
	}
	const start = $gl;
	let $gm = null;
	if (start + removed > held2) {
		$gm = held2 - start;
	} else {
		$gm = removed;
	}
	const taking = $gm;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change4 = [ 0, start, taking, arriving ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return splice2(held3, start, taking, inserted);
	});
	landed_one3(self, change4, $z);
}
function insert_at3(self, at4, value, $y) {
	return splice6(self, at4, 0, [ __clone(value) ], $y);
}
function remove_at2(self, at4, $bh) {
	return splice6(self, at4, 1, [  ], $bh);
}
function replace3(self, at4, value, $ba) {
	const watch = self[0].v[3];
	let $gF = null;
	if (has2(self, at4)) {
		$gF = [ 2, __clone(at4) ];
	} else if (viewing(watch, (held2) => {
		return follows_items(held2[0]);
	})) {
		return false;
	} else {
		$gF = [ 1, __clone(at4), 1 ];
	}
	const change4 = $gF;
	coming_one(self, change4);
	const write = self[0].v[1][3];
	mutate(self, (held2) => {
		return write(held2, at4, value);
	});
	landed_one3(self, change4, $ba);
	return true;
}
function get9(self, key2) {
	return __nmap_get(self[0], hash(key2));
}
function remove(self, key2) {
	__nmap_remove(self[0], hash(key2));
}
function entries(self) {
	let keys2 = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys2))) {
		const $gJ = __list_pop(keys2);
		let $gK = null;
		if ($gJ[0] === 0) {
			const key2 = $gJ[1];
			const $gL = __list_pop(values);
			let $gM = null;
			if ($gL[0] === 0) {
				const value = $gL[1];
				$gM = backwards.push([ key2, value ]);
			} else {
				$gM = undefined;
			}
			$gK = $gM;
		} else {
			$gK = undefined;
		}
		$gK;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $gN = __list_pop(backwards);
		let $gO = null;
		if ($gN[0] === 0) {
			const pair = $gN[1];
			$gO = result.push(pair);
		} else {
			$gO = undefined;
		}
		$gO;
	}
	return result;
}
function release6(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $gZ = _;
		const _key = $gZ[0];
		const id = $gZ[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new9();
}
function index_by(self, key2, $gG) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view = self[0].v[1][2];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage2(storage, (held2) => {
			return view(held2, at4, (item3) => {
				return out = [ 0, key2(item3) ];
			});
		});
		return out;
	};
	const map4 = (held2, at4) => {
		const id = follow_id2(storage, watch, holds, at4);
		const before = get9(found.v, held2);
		insert(found.v, held2, id);
		const $gH = before;
		if ($gH[0] === 0) {
			release_id(watch, $gH[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $gI = get9(found.v, held2);
		if ($gI[0] === 0) {
			remove(found.v, held2);
			release_id(watch, $gI[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $gP = _;
			const _held = $gP[0];
			const old = $gP[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at4 = 0;
		const count3 = peek_storage2(storage, (held2) => {
			return count2(held2);
		});
		while (at4 < count3) {
			const $gQ = key_at(at4);
			if ($gQ[0] === 0) {
				map4($gQ[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count3) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $gR = key_at(index3);
			if ($gR[0] === 0) {
				gone.push($gR[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count3) => {
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $gS = key_at(index3);
			if ($gS[0] === 0) {
				map4($gS[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change4) => {
		const $gT = change4;
		let $gU = null;
		if ($gT[0] === 0) {
			const at4 = $gT[1];
			const removed = $gT[2];
			const _inserted = $gT[3];
			$gU = note(at4, removed);
		} else if ($gT[0] === 3) {
			const at5 = $gT[1];
			const count3 = $gT[2];
			$gU = note(at5, count3);
		} else if ($gT[0] === 2) {
			const at6 = $gT[1];
			$gU = note(at6, 1);
		} else if ($gT[0] === 4) {
			const at7 = $gT[1];
			const _part = $gT[2];
			$gU = note(at7, 1);
		} else {
			$gU = undefined;
		}
		return $gU;
	}, (change4, stage) => {
		const $gV = stage;
		let $gW = null;
		if ($gV[0] === 0) {
			$gW = undefined;
		} else if ($gV[0] === 3) {
			return;
		} else {
			fill();
			return;
			$gW = undefined;
		}
		$gW;
		const $gX = change4;
		let $gY = null;
		if ($gX[0] === 0) {
			const at4 = $gX[1];
			const _removed = $gX[2];
			const inserted = $gX[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at4, inserted);
			$gY = undefined;
		} else if ($gX[0] === 1) {
			const at5 = $gX[1];
			const count3 = $gX[2];
			$gY = arrive(at5, count3);
		} else if ($gX[0] === 3) {
			const _at = $gX[1];
			const _count = $gX[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$gY = undefined;
		} else if ($gX[0] === 2) {
			const at6 = $gX[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$gY = undefined;
		} else if ($gX[0] === 4) {
			const at7 = $gX[1];
			const _part = $gX[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at7, 1);
			$gY = undefined;
		} else if ($gX[0] === 5) {
			const _from = $gX[1];
			const _to = $gX[2];
			$gY = undefined;
		} else {
			$gY = fill();
		}
		return $gY;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index2);
	}, $gG);
	return index2;
}
function map3(self, fn) {
	const $ha = self;
	let $hb = null;
	if ($ha[0] === 0) {
		const x = $ha[1];
		$hb = [ 0, fn(x) ];
	} else {
		$hb = [ 1 ];
	}
	return $hb;
}
function find(self, key2) {
	const lens2 = __clone(self[0]);
	return map3(viewing2(self[1], (held2) => {
		return get9(held2, key2);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function eq7(self, b2) {
	const $he = self;
	let $hh = null;
	if ($he[0] === 0) {
		const $hf = b2;
		let $hg = null;
		if ($hf[0] === 0) {
			$hg = eq5($he[1], $hf[1]);
		} else {
			$hg = false;
		}
		$hh = $hg;
	} else {
		const $hi = b2;
		$hh = $hi[0] === 1;
	}
	return $hh;
}
function remove_at3(self, at4, $bh) {
	return splice4(self, at4, 1, [  ], $bh);
}
function index(self) {
	return [ __clone(self) ];
}
function get10(self) {
	return at(self[0]);
}
function shape_cell(self) {
	const watch = self[0].v[3];
	const $hk = watch.v[2];
	const cell = __clone($hk[1]);
	if ($hk[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1031:26");
	watch.v[2] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle7(self, subscriber) {
	return on_settle(shape_cell(self[0][0]), subscriber);
}
function attach_observer5(self, observer, immediately) {
	const subscription = on_settle7(self, mint_subscriber(() => {
		return observer(get10(self));
	}));
	if (immediately) {
		observer(get10(self));
	}
	return subscription;
}
function on_change5(self, observer, caller) {
	return place_subscription(attach_observer5(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function release7(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $hl) {
	const log = new10();
	const held2 = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const fill = () => {
		let ids = [  ];
		let at4 = 0;
		const count3 = peek_storage2(storage, (list2) => {
			return count2(list2);
		});
		while (at4 < count3) {
			ids.push(follow_id(storage, watch, holds, at4));
			at4 = at4 + 1;
		}
		return ids;
	};
	const leave = (at4, count3) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count3) {
			const id = __remove_at(held2.v, at4, "std/src/reactive/lens.vl:1836:27");
			release_id(watch, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at4, count3) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count3) {
			const id = follow_id(storage, watch, holds, index2);
			__insert_at(held2.v, index2, id, "std/src/reactive/lens.vl:1848:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held2.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change4, _stage) => {
		const $hm = change4;
		let $hn = null;
		if ($hm[0] === 0) {
			const at4 = $hm[1];
			const removed = $hm[2];
			const inserted = $hm[3];
			const gone = leave(at4, removed);
			record(log, [ 0, at4, gone, arrive(at4, inserted) ]);
			$hn = undefined;
		} else if ($hm[0] === 1) {
			const at5 = $hm[1];
			const count3 = $hm[2];
			$hn = record(log, [ 0, at5, [  ], arrive(at5, count3) ]);
		} else if ($hm[0] === 3) {
			const at6 = $hm[1];
			const count4 = $hm[2];
			$hn = record(log, [ 0, at6, leave(at6, count4), [  ] ]);
		} else if ($hm[0] === 5) {
			const from = $hm[1];
			const to = $hm[2];
			const id = __remove_at(held2.v, from, "std/src/reactive/lens.vl:1864:28");
			__insert_at(held2.v, to, id, "std/src/reactive/lens.vl:1865:19");
			record(log, [ 3, from, 1, to ]);
			$hn = undefined;
		} else if ($hm[0] === 6) {
			for (const id2 of held2.v) {
				release_id(watch, id2);
			}
			held2.v = fill();
			record(log, [ 2, __clone(held2.v) ]);
			$hn = undefined;
		} else {
			$hn = undefined;
		}
		return $hn;
	});
	const feed4 = [ __clone(self), held2, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release7(feed4);
	}, $hl);
	return feed4;
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get11(self) {
	return handles(self, self[1].v);
}
function tapped(self) {
	return self[0].v[4].length;
}
function comp(body) {
	const scope = new4();
	const result = body(scope);
	return [ ...result, scope ];
}
function find2(self, key2) {
	return get9(self, key2);
}
function store(self, key2, value) {
	insert(self, key2, value);
}
function keyed(value) {
	return over3(value, new7(), [ (held2, key2) => {
		const $hs = find2(held2, key2);
		return $hs[0] === 0;
	}, (held2, key2) => {
		return find2(held2, key2);
	}, (held2, key2, read) => {
		const $ht = find2(held2, key2);
		const item3 = [ $ht[1] ];
		if ($ht[0] === 0) {
			read([ item3, 0 ]);
		}
		return;
	}, (held2, key2, value2) => {
		return store(held2, key2, value2);
	}, (held2, key2, with2) => {
		const $hu = find2(held2, key2);
		if ($hu[0] === 0) {

		} else {
			return;
		}
		let item3 = [ $hu[1] ];
		with2([ item3, 0 ]);
		store(held2, key2, item3[0]);
		return;
	} ]);
}
function follows_items2(self) {
	return false;
}
function follow5(self, at4) {
	const $hz = get9(self[0], at4);
	if ($hz[0] === 0) {
		return $hz[1];
	}
	const slot7 = as_i32(self[1].length);
	self[1].push(__clone(at4));
	insert(self[0], at4, slot7);
	return slot7;
}
function follow_id3(storage, watch, holds, at4) {
	if (viewing2(watch, (held2) => {
		return follows_items2(held2[0]);
	}) && !(peek_storage2(storage, (held2) => {
		return holds(held2, at4);
	}))) {
		return -(1);
	}
	const id = follow5(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	const $hA = as_usize(id);
	__at(watch.v[1], $hA, "std/src/reactive/lens.vl:1058:2")[2] = __at(watch.v[1], $hA, "std/src/reactive/lens.vl:1058:2")[2] + 1;
	return id;
}
function follow6(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id3(core.v[0], core.v[3], core.v[1][0], at4));
}
function release8(self, slot7) {
	const $hD = __list_get(self[1], as_usize(slot7));
	if ($hD[0] === 0) {
		remove(self[0], $hD[1]);
	}
}
function release_id2(watch, id) {
	if (id < 0) {
		return;
	}
	const entry = as_usize(id);
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1068:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1069:2")[2] = left;
	if (left <= 0) {
		release8(watch.v[0], id);
	}
}
function release9(self) {
	release_id2(self[0][0].v[3], self[1]);
}
function slot3(self, at4, $b) {
	const handle = follow6(self, at4);
	defer_to_owner(() => {
		return release9(handle);
	}, $b);
	return handle;
}
function locate3(self, slot7) {
	return __list_get(self[1], as_usize(slot7));
}
function at3(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][0].v[3], (held2) => {
		return locate3(held2[0], self[1]);
	});
}
function get12(self) {
	const $hG = at3(self);
	if ($hG[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek2(self[0], (held2) => {
		return read(held2, $hG[1]);
	});
}
function on_settle8(self, subscriber) {
	return on_settle(slot_cell(self[0][0].v[3], self[1]), subscriber);
}
function attach_observer6(self, observer, immediately) {
	const subscription = on_settle8(self, mint_subscriber(() => {
		return observer(get12(self));
	}));
	if (immediately) {
		observer(get12(self));
	}
	return subscription;
}
function on_change6(self, observer, caller) {
	return place_subscription(attach_observer6(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function apply3(self, change4, wakes) {
	const $hV = change4;
	let $hW = null;
	if ($hV[0] === 0) {
		const key2 = $hV[1];
		const _removed = $hV[2];
		const _inserted = $hV[3];
		const $hX = get9(self[0], key2);
		if ($hX[0] === 0) {
			wakes.push([ 0, $hX[1] ]);
		}
		$hW = undefined;
	} else if ($hV[0] === 1) {
		const key3 = $hV[1];
		const _count = $hV[2];
		const $hY = get9(self[0], key3);
		if ($hY[0] === 0) {
			wakes.push([ 0, $hY[1] ]);
		}
		$hW = undefined;
	} else if ($hV[0] === 2) {
		const key4 = $hV[1];
		const $hZ = get9(self[0], key4);
		if ($hZ[0] === 0) {
			wakes.push([ 0, $hZ[1] ]);
		}
		$hW = undefined;
	} else if ($hV[0] === 3) {
		const key5 = $hV[1];
		const _count2 = $hV[2];
		const $ia = get9(self[0], key5);
		if ($ia[0] === 0) {
			wakes.push([ 0, $ia[1] ]);
		}
		$hW = undefined;
	} else if ($hV[0] === 4) {
		const key6 = $hV[1];
		const part3 = $hV[2];
		const $ib = get9(self[0], key6);
		if ($ib[0] === 0) {
			wakes.push([ 1, $ib[1], part3 ]);
		}
		$hW = undefined;
	} else if ($hV[0] === 5) {
		const _from = $hV[1];
		const _to = $hV[2];
		$hW = undefined;
	} else {
		let slot7 = 0;
		const held2 = as_i32(self[1].length);
		while (slot7 < held2) {
			wakes.push([ 0, slot7 ]);
			slot7 = slot7 + 1;
		}
		$hW = undefined;
	}
	return $hW;
}
function landed3(self, changes, late, $J) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change4 of changes) {
		const $hT = change4;
		let $hU = null;
		if ($hT[0] === 0) {
			const _at = $hT[1];
			const removed = $hT[2];
			const inserted = $hT[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$hU = undefined;
		} else if ($hT[0] === 1) {
			const _at2 = $hT[1];
			const _count = $hT[2];
			$hU = resized = true;
		} else if ($hT[0] === 3) {
			const _at3 = $hT[1];
			const _count2 = $hT[2];
			$hU = resized = true;
		} else if ($hT[0] === 6) {
			$hU = resized = true;
		} else if ($hT[0] === 5) {
			const _from = $hT[1];
			const _to = $hT[2];
			$hU = shaped = true;
		} else {
			$hU = undefined;
		}
		$hU;
		if (watch.v[1].length > 0) {
			apply3(watch.v[0], change4, wakes);
		}
		let $id = null;
		if (self[0].v[4].length > 0) {
			let $ic = null;
			if (!(late)) {
				$ic = [ 0 ];
			} else if (!(several)) {
				$ic = [ 1 ];
			} else if (index2 === 0) {
				$ic = [ 2 ];
			} else {
				$ic = [ 3 ];
			}
			const stage = $ic;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change4, stage);
			}
			$id = undefined;
		}
		$id;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $ie = null;
	if (shaped || resized) {
		$ie = watch.v[2];
	} else {
		$ie = [ 1 ];
	}
	const shape = __clone($ie);
	let $if = null;
	if (resized) {
		$if = watch.v[3];
	} else {
		$if = [ 1 ];
	}
	const sized = __clone($if);
	let $ig = undefined;
	let $ih = undefined;
	if (wakes.length === 0 && (($ig = shape), $ig[0] === 1) && (($ih = sized), $ih[0] === 1)) {
		notify(published, $J);
		return;
	}
	settle_together(($ap) => {
		for (const wake2 of wakes) {
			const $ii = wake2;
			let $ij = null;
			if ($ii[0] === 0) {
				const slot7 = $ii[1];
				$ij = wake_all(watch, slot7, [ 0, $ap ]);
			} else if ($ii[0] === 2) {
				const slot8 = $ii[1];
				$ij = wake_all(watch, slot8, [ 0, $ap ]);
			} else {
				const slot9 = $ii[1];
				const part3 = $ii[2];
				$ij = wake_part(watch, slot9, part3, [ 0, $ap ]);
			}
			$ij;
		}
		const $im = sized;
		if ($im[0] === 0) {
			notify($im[1], [ 0, $ap ]);
		}
		const $in = shape;
		if ($in[0] === 0) {
			notify($in[1], [ 0, $ap ]);
		}
		notify(published, [ 0, $ap ]);
		return;
	}, $J);
}
function landed_one4(self, change4, $I) {
	landed3(self, [ __clone(change4) ], false, $I);
}
function put2(self, key2, value, $hO) {
	const had = peek2(self, (held2) => {
		const $hP = find2(held2, key2);
		return $hP[0] === 0;
	});
	let $hQ = null;
	if (had) {
		$hQ = [ 2, __clone(key2) ];
	} else {
		$hQ = [ 1, __clone(key2), 1 ];
	}
	const change4 = $hQ;
	coming_one(self, change4);
	mutate(self, (held2) => {
		return store(held2, key2, value);
	});
	landed_one4(self, change4, $hO);
}
function drop(self, key2) {
	remove(self, key2);
}
function remove2(self, key2, $io) {
	const had = peek2(self, (held2) => {
		const $ip = find2(held2, key2);
		return $ip[0] === 0;
	});
	if (!(had)) {
		return;
	}
	coming_one(self, [ 3, __clone(key2), 1 ]);
	mutate(self, (held2) => {
		return drop(held2, key2);
	});
	landed_one4(self, [ 3, __clone(key2), 1 ], $io);
}
function push3(self, value, $bi) {
	return splice6(self, size2(self), 0, [ __clone(value) ], $bi);
}
function eq8(self, other) {
	const $is = [ self, other ];
	let $it = null;
	if ($is[0][0] === 0 && $is[1][0] === 0) {
		const s0 = $is[0][1];
		const s1 = $is[0][2];
		const s2 = $is[0][3];
		const o0 = $is[1][1];
		const o1 = $is[1][2];
		const o2 = $is[1][3];
		$it = eq2(s0, o0) && s1 === o1 && s2 === o2;
	} else if ($is[0][0] === 1 && $is[1][0] === 1) {
		const s02 = $is[0][1];
		const s12 = $is[0][2];
		const o02 = $is[1][1];
		const o12 = $is[1][2];
		$it = eq2(s02, o02) && s12 === o12;
	} else if ($is[0][0] === 2 && $is[1][0] === 2) {
		const s03 = $is[0][1];
		const o03 = $is[1][1];
		$it = eq2(s03, o03);
	} else if ($is[0][0] === 3 && $is[1][0] === 3) {
		const s04 = $is[0][1];
		const s13 = $is[0][2];
		const o04 = $is[1][1];
		const o13 = $is[1][2];
		$it = eq2(s04, o04) && s13 === o13;
	} else if ($is[0][0] === 4 && $is[1][0] === 4) {
		const s05 = $is[0][1];
		const s14 = $is[0][2];
		const o05 = $is[1][1];
		const o14 = $is[1][2];
		$it = eq2(s05, o05) && s14 === o14;
	} else if ($is[0][0] === 5 && $is[1][0] === 5) {
		const s06 = $is[0][1];
		const s15 = $is[0][2];
		const o06 = $is[1][1];
		const o15 = $is[1][2];
		$it = eq2(s06, o06) && eq2(s15, o15);
	} else if ($is[0][0] === 6 && $is[1][0] === 6) {
		$it = true;
	} else {
		$it = false;
	}
	return $it;
}
function eq9(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(eq8(item3, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function on_settle9(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
}
function attach_observer7(self, observer, immediately) {
	const subscription = on_settle9(self, mint_subscriber(() => {
		return observer(get5(self));
	}));
	if (immediately) {
		observer(get5(self));
	}
	return subscription;
}
function on_change7(self, observer, caller) {
	return place_subscription(attach_observer7(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function on_settle10(self, subscriber) {
	return on_settle(shape_cell(self[0][0]), subscriber);
}
function attach_observer8(self, observer, immediately) {
	const subscription = on_settle10(self, mint_subscriber(() => {
		return observer(get10(self));
	}));
	if (immediately) {
		observer(get10(self));
	}
	return subscription;
}
function on_change8(self, observer, caller) {
	return place_subscription(attach_observer8(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function cursor3(self) {
	return cursor(self[2]);
}
function lift(self, op) {
	const $iC = op;
	let $iD = null;
	if ($iC[0] === 0) {
		const at4 = $iC[1];
		const gone = $iC[2];
		const arrived = $iC[3];
		$iD = [ 0, at4, handles(self, gone), handles(self, arrived) ];
	} else if ($iC[0] === 1) {
		const at5 = $iC[1];
		const old = $iC[2];
		const new11 = $iC[3];
		$iD = [ 1, at5, slot_of(self[0], old), slot_of(self[0], new11) ];
	} else if ($iC[0] === 2) {
		const all5 = $iC[1];
		$iD = [ 2, handles(self, all5) ];
	} else {
		const from = $iC[1];
		const count3 = $iC[2];
		const to = $iC[3];
		$iD = [ 3, from, count3, to ];
	}
	return $iD;
}
function since3(self, cursor6) {
	const $iA = since(self[2], cursor6);
	let $iB = null;
	if ($iA[0] === 0) {
		const recorded = $iA[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$iB = out;
	} else {
		$iB = [ [ 2, get11(self) ] ];
	}
	return $iB;
}
function eq10(self, other) {
	return self[1] === other[1];
}
function eq11(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(eq10(item3, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function row_arrived_reads(op) {
	const $iF = op;
	let $iG = null;
	if ($iF[0] === 0) {
		const _at = $iF[1];
		const _gone = $iF[2];
		const arrived = $iF[3];
		$iG = map2(arrived, (slot7) => {
			return get(slot7);
		});
	} else {
		$iG = [  ];
	}
	return $iG;
}
function eq12(self, b2) {
	const $iH = self;
	let $iK = null;
	if ($iH[0] === 0) {
		const $iI = b2;
		let $iJ = null;
		if ($iI[0] === 0) {
			$iJ = $iH[1] === $iI[1];
		} else {
			$iJ = false;
		}
		$iK = $iJ;
	} else {
		const $iL = b2;
		$iK = $iL[0] === 1;
	}
	return $iK;
}
function eq13(self, b2) {
	if (self.length !== b2.length) {
		return false;
	}
	let index2 = 0;
	for (const item3 of self) {
		if (!(eq12(item3, __at(b2, index2, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function index_by2(self, key2, $gG) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view = self[0].v[1][2];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage2(storage, (held2) => {
			return view(held2, at4, (item3) => {
				return out = [ 0, key2(item3) ];
			});
		});
		return out;
	};
	const map4 = (held2, at4) => {
		const id = follow_id(storage, watch, holds, at4);
		const before = get9(found.v, held2);
		insert(found.v, held2, id);
		const $iQ = before;
		if ($iQ[0] === 0) {
			release_id(watch, $iQ[1]);
		}
		return;
	};
	const unmap = (held2) => {
		const $iR = get9(found.v, held2);
		if ($iR[0] === 0) {
			remove(found.v, held2);
			release_id(watch, $iR[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $iS = _;
			const _held = $iS[0];
			const old = $iS[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at4 = 0;
		const count3 = peek_storage2(storage, (held2) => {
			return count2(held2);
		});
		while (at4 < count3) {
			const $iT = key_at(at4);
			if ($iT[0] === 0) {
				map4($iT[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count3) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $iU = key_at(index3);
			if ($iU[0] === 0) {
				gone.push($iU[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count3) => {
		let index3 = at4;
		while (index3 < at4 + count3) {
			const $iV = key_at(index3);
			if ($iV[0] === 0) {
				map4($iV[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change4) => {
		const $iW = change4;
		let $iX = null;
		if ($iW[0] === 0) {
			const at4 = $iW[1];
			const removed = $iW[2];
			const _inserted = $iW[3];
			$iX = note(at4, removed);
		} else if ($iW[0] === 3) {
			const at5 = $iW[1];
			const count3 = $iW[2];
			$iX = note(at5, count3);
		} else if ($iW[0] === 2) {
			const at6 = $iW[1];
			$iX = note(at6, 1);
		} else if ($iW[0] === 4) {
			const at7 = $iW[1];
			const _part = $iW[2];
			$iX = note(at7, 1);
		} else {
			$iX = undefined;
		}
		return $iX;
	}, (change4, stage) => {
		const $iY = stage;
		let $iZ = null;
		if ($iY[0] === 0) {
			$iZ = undefined;
		} else if ($iY[0] === 3) {
			return;
		} else {
			fill();
			return;
			$iZ = undefined;
		}
		$iZ;
		const $ja = change4;
		let $jb = null;
		if ($ja[0] === 0) {
			const at4 = $ja[1];
			const _removed = $ja[2];
			const inserted = $ja[3];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			arrive(at4, inserted);
			$jb = undefined;
		} else if ($ja[0] === 1) {
			const at5 = $ja[1];
			const count3 = $ja[2];
			$jb = arrive(at5, count3);
		} else if ($ja[0] === 3) {
			const _at = $ja[1];
			const _count = $ja[2];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			$jb = undefined;
		} else if ($ja[0] === 2) {
			const at6 = $ja[1];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$jb = undefined;
		} else if ($ja[0] === 4) {
			const at7 = $ja[1];
			const _part = $ja[2];
			for (const held5 of leaving.v) {
				unmap(held5);
			}
			arrive(at7, 1);
			$jb = undefined;
		} else if ($ja[0] === 5) {
			const _from = $ja[1];
			const _to = $ja[2];
			$jb = undefined;
		} else {
			$jb = fill();
		}
		return $jb;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index2);
	}, $gG);
	return index2;
}
function find3(self, key2) {
	const lens2 = __clone(self[0]);
	return map3(viewing2(self[1], (held2) => {
		return get9(held2, key2);
	}), (id) => {
		return slot_of(lens2, id);
	});
}
function eq14(self, b2) {
	const $jh = self;
	let $jk = null;
	if ($jh[0] === 0) {
		const $ji = b2;
		let $jj = null;
		if ($ji[0] === 0) {
			$jj = eq12($jh[1], $ji[1]);
		} else {
			$jj = false;
		}
		$jk = $jj;
	} else {
		const $jl = b2;
		$jk = $jl[0] === 1;
	}
	return $jk;
}
function attach_observer9(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get4(self));
	}));
	if (immediately) {
		observer(get4(self));
	}
	return subscription;
}
function on_change9(self, observer, caller) {
	return place_subscription(attach_observer9(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
}
function observe(signal, observer) {
	const cell = signal[0];
	return attach(signal, mint_subscriber(() => {
		const $jp = [ 0, cell ];
		let $jq = null;
		if ($jp[0] === 0) {
			const live = $jp[1];
			$jq = observer(live.v);
		} else {
			$jq = undefined;
		}
		return $jq;
	}));
}
function get13(self) {
	return __clone(self[0].v);
}
function attach_observer10(self, observer, immediately) {
	const subscription = observe(self, observer);
	if (immediately) {
		observer(get13(self));
	}
	return subscription;
}
function on_change10(self, observer, caller) {
	return place_subscription(attach_observer10(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false), caller);
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
function record_put(log, key2, now, leaving, late) {
	const $jD = now;
	if ($jD[0] === 0) {
		const $jF = __clone(key2);
		let $jE = null;
		if (late) {
			$jE = [ 1 ];
		} else {
			$jE = leaving.v;
		}
		record2(log, [ 0, $jF, __clone($jE), __clone($jD[1]) ]);
	}
}
function keys(self) {
	return __nmap_keys(self[0]);
}
function held_keys(self) {
	return keys(self);
}
function entries_of(storage) {
	return peek_storage2(storage, (held2) => {
		let out = new9();
		for (const key2 of held_keys(held2)) {
			const $jG = find2(held2, key2);
			if ($jG[0] === 0) {
				insert(out, key2, $jG[1]);
			}
		}
		return out;
	});
}
function release10(self) {
	untap(self[0], self[2]);
}
function entries2(self, $ju) {
	const log = new10();
	const leaving = __shared_new([ 1 ]);
	const storage = __clone(self[0].v[0]);
	const find4 = (key2) => {
		return peek_storage2(storage, (held2) => {
			return find2(held2, key2);
		});
	};
	const tap2 = tap(self, (change4) => {
		const $jv = change4;
		let $jw = null;
		if ($jv[0] === 0) {
			const key2 = $jv[1];
			const _removed = $jv[2];
			const _inserted = $jv[3];
			$jw = leaving.v = find4(key2);
		} else if ($jv[0] === 3) {
			const key3 = $jv[1];
			const _count = $jv[2];
			$jw = leaving.v = find4(key3);
		} else if ($jv[0] === 2) {
			const key4 = $jv[1];
			$jw = leaving.v = find4(key4);
		} else if ($jv[0] === 4) {
			const key5 = $jv[1];
			const _part = $jv[2];
			$jw = leaving.v = find4(key5);
		} else {
			$jw = undefined;
		}
		return $jw;
	}, (change4, stage) => {
		const $jx = stage;
		const late = !($jx[0] === 0);
		const $jy = change4;
		let $jz = null;
		if ($jy[0] === 1) {
			const key2 = $jy[1];
			const _count = $jy[2];
			const $jA = find4(key2);
			if ($jA[0] === 0) {
				record2(log, [ 0, __clone(key2), [ 1 ], $jA[1] ]);
			}
			$jz = undefined;
		} else if ($jy[0] === 0) {
			const key3 = $jy[1];
			const _removed = $jy[2];
			const _inserted = $jy[3];
			$jz = record_put(log, key3, find4(key3), leaving, late);
		} else if ($jy[0] === 2) {
			const key4 = $jy[1];
			$jz = record_put(log, key4, find4(key4), leaving, late);
		} else if ($jy[0] === 4) {
			const key5 = $jy[1];
			const _part = $jy[2];
			$jz = record_put(log, key5, find4(key5), leaving, late);
		} else if ($jy[0] === 3) {
			const key6 = $jy[1];
			const _count2 = $jy[2];
			if (late) {
				record2(log, [ 2, entries_of(storage) ]);
			} else {
				const $jL = leaving.v;
				const value = __clone($jL[1]);
				if ($jL[0] === 0) {
					record2(log, [ 1, __clone(key6), value ]);
				}
			}
			$jz = undefined;
		} else if ($jy[0] === 5) {
			const _from = $jy[1];
			const _to = $jy[2];
			$jz = undefined;
		} else {
			$jz = record2(log, [ 2, entries_of(storage) ]);
		}
		return $jz;
	});
	const feed4 = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release10(feed4);
	}, $ju);
	return feed4;
}
function cursor4(self) {
	return cursor(self[1]);
}
function update2(self, change4, $cd) {
	const $jM = self[0].v[6];
	const _check = $jM[1];
	let $jN = null;
	if ($jM[0] === 0) {
		$jN = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$jN = [ 1 ];
	}
	const before = $jN;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change4(held2) ];
	});
	const $jO = out;
	let $jP = null;
	if ($jO[0] === 0) {
		const result = __clone($jO[1][0]);
		const changes = __clone($jO[1][1]);
		audit(self, before, changes);
		landed3(self, changes, true, $cd);
		$jP = result;
	} else {
		$jP = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $jP;
}
function all3(self) {
	return entries_of(self[0].v[0]);
}
function since4(self, cursor6) {
	const $jZ = since(self[1], cursor6);
	let $ka = null;
	if ($jZ[0] === 0) {
		const recorded = $jZ[1];
		$ka = recorded;
	} else {
		$ka = [ [ 2, all3(self[0]) ] ];
	}
	return $ka;
}
function over4(value, $kb) {
	return [ sequence(value, $kb) ];
}
function over5(value) {
	return [ keyed(value) ];
}
function slot4(self, at4, $kc) {
	return slot(self[0], at4, $kc);
}
function splice7(self, at4, removed, inserted, $kd) {
	splice3(self[0], at4, removed, inserted, $kd);
}
function insert_at4(self, at4, value, $y) {
	return splice7(self, at4, 0, [ __clone(value) ], $y);
}
function size4(self) {
	return size(self[0]);
}
function put3(self, key2, value, $ke) {
	return put2(self[0], key2, value, $ke);
}
function slot5(self, key2, $kf) {
	return slot3(self[0], key2, $kf);
}
function length2(self) {
	return [ __clone(self) ];
}
function length3(self) {
	return length2(self[0]);
}
function get14(self) {
	return peek2(self[0], (held2) => {
		return held_keys(held2).length;
	});
}
function lens(self) {
	return __clone(self[0]);
}
function identity(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function identity2(self) {
	return identity(slot_cell(self[0][0].v[3], self[1]));
}
function push4(self, value, $bi) {
	return splice7(self, size4(self), 0, [ __clone(value) ], $bi);
}
function length4(self) {
	return length(self[0]);
}
function audit2(self, before, changes) {
	const $ku = self[0].v[6];
	const check = $ku[1];
	if ($ku[0] === 0) {

	} else {
		return;
	}
	const $kv = before;
	if ($kv[0] === 0) {

	} else {
		return;
	}
	const problem = peek(self, (held2) => {
		return check($kv[1], held2, changes);
	});
	const $kw = problem;
	if ($kw[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $kw[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function update3(self, change4, $cd) {
	const $kq = self[0].v[6];
	const _check = $kq[1];
	let $kr = null;
	if ($kq[0] === 0) {
		$kr = [ 0, peek2(self, (held2) => {
			return __clone(held2);
		}) ];
	} else {
		$kr = [ 1 ];
	}
	const before = $kr;
	let out = [ 1 ];
	mutate(self, (held2) => {
		return out = [ 0, change4(held2) ];
	});
	const $ks = out;
	let $kt = null;
	if ($ks[0] === 0) {
		const result = __clone($ks[1][0]);
		const changes = __clone($ks[1][1]);
		audit2(self, before, changes);
		landed(self, changes, true, $cd);
		$kt = result;
	} else {
		$kt = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $kt;
}
function key(self) {
	return self[1];
}
function lend4(self, at4, read) {
	const $kx = item(self, at4);
	const item3 = [ $kx[1] ];
	if ($kx[0] === 0) {
		read([ item3, 0 ]);
	}
}
function put4(self, at4, value, $ky) {
	splice(self, at4, 1, [ __clone(value) ], $ky);
}
function change3(self, at4, with2, $kz) {
	const $kA = item(self, at4);
	if ($kA[0] === 0) {

	} else {
		return;
	}
	let item3 = [ $kA[1] ];
	with2([ item3, 0 ]);
	put4(self, at4, item3[0], $kz);
}
function sequence4(value, $a) {
	return over3(value, new5(), [ (held2, at4) => {
		return at4 < count(held2);
	}, (held2, at4) => {
		return item(held2, at4);
	}, (held2, at4, read) => {
		return lend4(held2, at4, read);
	}, (held2, at4, value2) => {
		return put4(held2, at4, value2, $a);
	}, (held2, at4, with2) => {
		return change3(held2, at4, with2, $a);
	} ]);
}
function follow7(self, at4) {
	const core = self[0];
	return slot_of(self, follow_id2(core.v[0], core.v[3], core.v[1][0], at4));
}
function slot6(self, at4, $b) {
	const handle = follow7(self, at4);
	defer_to_owner(() => {
		return release4(handle);
	}, $b);
	return handle;
}
function size5(self) {
	return peek2(self, (held2) => {
		return count(held2);
	});
}
function shift2(self, from, to, $kN) {
	const $kO = item(self, from);
	if ($kO[0] === 0) {

	} else {
		return;
	}
	splice(self, from, 1, [  ], $kN);
	splice(self, to, 0, [ $kO[1] ], $kN);
}
function move_item2(self, from, to, $cL) {
	const held2 = size5(self);
	if (from >= held2 || from === to) {
		return;
	}
	let $kM = null;
	if (to >= held2) {
		$kM = held2 - 1;
	} else {
		$kM = to;
	}
	const landing = $kM;
	const change4 = [ 5, from, landing ];
	coming_one(self, change4);
	mutate(self, (held3) => {
		return shift2(held3, from, landing, $cL);
	});
	landed_one3(self, change4, $cL);
}
function all_of2(storage) {
	return peek_storage2(storage, (held2) => {
		let out = [  ];
		let at4 = 0;
		const count3 = count(held2);
		while (at4 < count3) {
			const $ld = item(held2, at4);
			if ($ld[0] === 0) {
				out.push($ld[1]);
			}
			at4 = at4 + 1;
		}
		return out;
	});
}
function all4(self) {
	return all_of2(self[0].v[0]);
}
function part2(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function edit3(self, at4, part3, change4, $bE) {
	if (!(has2(self, at4))) {
		return false;
	}
	coming_one(self, [ 4, __clone(at4), part3 ]);
	const modify = self[0].v[1][4];
	mutate(self, (held2) => {
		return modify(held2, at4, change4);
	});
	landed_one3(self, [ 4, __clone(at4), part3 ], $bE);
	return true;
}
function edit4(self, part3, change4, $bC) {
	const $ls = at(self);
	if ($ls[0] === 0) {

	} else {
		return false;
	}
	return edit3(self[0], $ls[1], part3, change4, $bC);
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
const list = sequence([ "foo", "bar", "baz" ], [ 1 ]);
const bar = slot(list, 1, [ 1 ]);
const heard = __shared_new([  ]);
on_change(__clone(bar), (value, $r) => {
	return heard.v.push(__clone(value));
}, "lens-laws.vl:88:19");
insert_at(list, 0, "qux", [ 1 ]);
assert(eq3(get(bar), [ 0, "bar" ]), "L1: the slot still reads its item", "lens-laws.vl:90:2");
assert(eq5(at(bar), [ 0, 2 ]), "L1: and knows where it went", "lens-laws.vl:91:2");
assert(peek2(list, (held2) => {
	return __at(held2, 1, "lens-laws.vl:92:26");
}) === "foo", "position 1 is now foo", "lens-laws.vl:92:2");
assert(heard.v.length === 0, "L4: an insert above a slot does not wake it", "lens-laws.vl:93:2");
const rows2 = sequence([ "a", "b", "c" ], [ 1 ]);
const c = slot(rows2, 2, [ 1 ]);
const runs = __shared_new(0);
on_change(__clone(c), (_value, $aV) => {
	const $aW = [ runs, "v" ];
	return runs.v = $aW[0][$aW[1]] + 1;
}, "lens-laws.vl:100:17");
batch(($aX) => {
	insert_at(rows2, 0, "z", [ 0, $aX ]);
	set(c, "C!", [ 0, $aX ]);
	set(c, "C!!", [ 0, $aX ]);
	assert(runs.v === 0, "L3: nothing runs inside the turn", "lens-laws.vl:105:3");
	return;
}, [ 1 ]);
assert(eq(all(rows2), [ "z", "a", "b", "C!!" ]), "L2: the writes landed on c, not on b", "lens-laws.vl:107:2");
assert(runs.v === 1, "L3: one run for the turn", "lens-laws.vl:108:2");
remove_at(rows2, 3, [ 1 ]);
assert(eq3(get(c), [ 1 ]), "L6: the slot reads None", "lens-laws.vl:112:2");
assert(runs.v === 2, "L6: and woke once to say so", "lens-laws.vl:113:2");
push(rows2, "d", [ 1 ]);
assert(eq3(get(c), [ 1 ]), "L1: a new item at the old position is not the slot\'s item", "lens-laws.vl:115:2");
const board = sequence2(tasks(), [ 1 ]);
const first = slot2(board, 0, [ 1 ]);
const second = slot2(board, 1, [ 1 ]);
const titles = __shared_new(0);
const statuses = __shared_new(0);
const others = __shared_new(0);
const title = part(second, "title", (task) => {
	return task[0];
});
const status = part(second, "status", (task) => {
	return task[1];
});
on_change2(__clone(title), (_text, $br) => {
	const $bs = [ titles, "v" ];
	return titles.v = $bs[0][$bs[1]] + 1;
}, "lens-laws.vl:127:17");
on_change2(status, (_text, $by) => {
	const $bz = [ statuses, "v" ];
	return statuses.v = $bz[0][$bz[1]] + 1;
}, "lens-laws.vl:128:18");
on_change2(part(first, "title", (task) => {
	return task[0];
}), (_text, $bA) => {
	const $bB = [ others, "v" ];
	return others.v = $bB[0][$bB[1]] + 1;
}, "lens-laws.vl:129:50");
edit2(second, "title", (task) => {
	return task[0] = "B";
}, [ 1 ]);
assert(eq3(get2(title), [ 0, "B" ]), "the part reads the edit", "lens-laws.vl:131:2");
assert(titles.v === 1 && statuses.v === 0 && others.v === 0, "L4: only the edited part woke", "lens-laws.vl:132:2");
insert_at2(board, 0, [ "new", "open" ], [ 1 ]);
edit2(second, "status", (task) => {
	return task[1] = "done";
}, [ 1 ]);
assert(peek2(board, (held2) => {
	return __at(held2, 2, "lens-laws.vl:138:27")[1];
}) === "done", "L2: the edit followed the row", "lens-laws.vl:138:2");
assert(titles.v === 1 && statuses.v === 1, "L4: only the status woke", "lens-laws.vl:139:2");
update(board, (held2) => {
	__replace(held2, tasks());
	return [ undefined, [ [ 6 ] ] ];
}, [ 1 ]);
assert(eq6(get3(second), [ 1 ]), "L7: a positional slot dies at a reset", "lens-laws.vl:146:2");
const whole = __shared_new(0);
const lengths = __shared_new(0);
on_change3(__clone(board), (_all, $cD) => {
	const $cE = [ whole, "v" ];
	return whole.v = $cE[0][$cE[1]] + 1;
}, "lens-laws.vl:151:17");
on_change4(length(board), (_n, $cF) => {
	const $cG = [ lengths, "v" ];
	return lengths.v = $cG[0][$cG[1]] + 1;
}, "lens-laws.vl:152:26");
replace2(board, 0, [ "x", "open" ], [ 1 ]);
assert(whole.v === 1 && lengths.v === 0, "a replace wakes the whole value, not the length", "lens-laws.vl:154:2");
push2(board, [ "y", "open" ], [ 1 ]);
assert(whole.v === 2 && lengths.v === 1, "a push wakes both", "lens-laws.vl:159:2");
const order = sequence([ "a", "b", "c", "d" ], [ 1 ]);
const d = slot(order, 3, [ 1 ]);
const d_runs = __shared_new(0);
on_change(__clone(d), (_value, $cJ) => {
	const $cK = [ d_runs, "v" ];
	return d_runs.v = $cK[0][$cK[1]] + 1;
}, "lens-laws.vl:165:13");
move_item(order, 3, 0, [ 1 ]);
assert(eq(all(order), [ "d", "a", "b", "c" ]), "the move happened", "lens-laws.vl:167:2");
assert(eq5(at(d), [ 0, 0 ]) && eq3(get(d), [ 0, "d" ]) && d_runs.v === 0, "the slot went with its item, silently", "lens-laws.vl:168:2");
move_item(order, 0, 9, [ 1 ]);
assert(eq(all(order), [ "a", "b", "c", "d" ]) && eq5(at(d), [ 0, 3 ]), "a move past the end lands at the end", "lens-laws.vl:173:2");
const people2 = sequence([ "ann", "bob", "cy", "dee" ], [ 1 ]);
const bob2 = slot(people2, 1, [ 1 ]);
const dee2 = slot(people2, 3, [ 1 ]);
const cy2 = slot(people2, 2, [ 1 ]);
reconcile_by_key(people2, [ "dee", "zed", "bob", "ann" ], (name) => {
	return name[0][name[1]];
}, [ 1 ]);
assert(eq(all(people2), [ "dee", "zed", "bob", "ann" ]), "the list became the new list", "lens-laws.vl:184:2");
assert(eq5(at(bob2), [ 0, 2 ]) && eq5(at(dee2), [ 0, 0 ]), "kept items keep their slots", "lens-laws.vl:185:2");
assert(eq3(get(cy2), [ 1 ]), "an item that left ends its slot", "lens-laws.vl:186:2");
const numbers = sequence3([ 1, 2, 3, 4 ], [ 1 ]);
const doubled = memo(map(items(numbers, [ 1 ]), (n, $dz, $dA) => {
	return n * 2;
}), [ 1 ], [ 1 ], "lens-laws.vl:190:47");
const evens = memo2(filter(items(numbers, [ 1 ]), (n, $fq, $fr) => {
	return n % 2 === 0;
}), [ 1 ], [ 1 ], "lens-laws.vl:191:53");
assert(eq(get8(doubled), [ 2, 4, 6, 8 ]) && eq(get8(evens), [ 2, 4 ]), "the operators seed from the lens", "lens-laws.vl:192:2");
insert_at3(numbers, 1, 10, [ 1 ]);
remove_at2(numbers, 0, [ 1 ]);
replace3(numbers, 3, 5, [ 1 ]);
assert(eq(all2(numbers), [ 10, 2, 3, 5 ]), "the writes landed", "lens-laws.vl:199:2");
assert(eq(get8(doubled), [ 20, 4, 6, 10 ]), "map follows the lens", "lens-laws.vl:200:2");
assert(eq(get8(evens), [ 10, 2 ]), "filter follows the lens", "lens-laws.vl:201:2");
const tabs = sequence2(tasks(), [ 1 ]);
const by_title = index_by(tabs, (task) => {
	return task[0];
}, [ 1 ]);
const b = find(by_title, "b");
assert(eq7(map3(b, (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 1 ] ]), "the index finds the slot of the item under a key", "lens-laws.vl:207:2");
insert_at2(tabs, 0, [ "first", "open" ], [ 1 ]);
remove_at3(tabs, 3, [ 1 ]);
assert(eq7(map3(find(by_title, "first"), (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 0 ] ]), "an inserted item is indexed", "lens-laws.vl:213:2");
assert(eq7(map3(find(by_title, "c"), (slot7) => {
	return at(slot7);
}), [ 1 ]), "a deleted item leaves the index", "lens-laws.vl:217:2");
assert(eq7(map3(find(by_title, "b"), (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 2 ] ]), "and the others are found where they are now", "lens-laws.vl:218:2");
edit(tabs, 2, "title", (task) => {
	return task[0] = "B";
}, [ 1 ]);
assert(eq7(map3(find(by_title, "b"), (slot7) => {
	return at(slot7);
}), [ 1 ]) && eq7(map3(find(by_title, "B"), (slot7) => {
	return at(slot7);
}), [ 0, [ 0, 2 ] ]), "an edit that changes the key re-indexes the item", "lens-laws.vl:223:2");
const places = __shared_new([  ]);
const first_tab = slot2(tabs, 0, [ 1 ]);
on_change5(index(first_tab), (place, $hj) => {
	return places.v.push(__clone(place));
}, "lens-laws.vl:235:29");
insert_at2(tabs, 0, [ "zero", "open" ], [ 1 ]);
assert(places.v.length === 1 && eq5(at(first_tab), [ 0, 1 ]), "a slot\'s position is a source that follows the shifts", "lens-laws.vl:237:2");
const held = sequence([ "p", "q", "r" ], [ 1 ]);
const one2 = slot(held, 1, [ 1 ]);
const two = slot(held, 1, [ 1 ]);
assert(one2[1] === two[1], "two asks for one item are one slot", "lens-laws.vl:246:2");
release3(one2);
assert(eq3(get(two), [ 0, "q" ]) && eq5(at(two), [ 0, 1 ]), "the slot lives while a handle holds it", "lens-laws.vl:248:2");
release3(two);
assert(eq3(get(two), [ 1 ]) && eq5(at(two), [ 1 ]), "the last release ends it", "lens-laws.vl:250:2");
insert_at(held, 0, "o", [ 1 ]);
assert(eq3(get(slot(held, 2, [ 1 ])), [ 0, "q" ]) && slot(held, 2, [ 1 ])[1] !== one2[1], "a new ask makes a new slot; the old entry is free", "lens-laws.vl:252:2");
const feed = rows(held, [ 1 ]);
assert(get11(feed).length === 4, "rows() holds a slot per item", "lens-laws.vl:257:2");
release7(feed);
assert(tapped(held) === 0, "releasing the feed lets its tap and its slots go", "lens-laws.vl:259:2");
const scoped = sequence([ "m", "n" ], [ 1 ]);
const $hp = comp(($ho) => {
	return [ slot(scoped, 1, [ 0, $ho ]), rows(scoped, [ 0, $ho ]) ];
});
const inner = $hp[0];
const feed2 = $hp[1];
const owner = $hp[2];
assert(eq3(get(inner), [ 0, "n" ]) && tapped(scoped) === 1, "alive while the owner is", "lens-laws.vl:264:2");
dispose2(owner);
assert(eq3(get(inner), [ 1 ]) && tapped(scoped) === 0, "released with the owner", "lens-laws.vl:266:2");
assert(get11(feed2).length === 0, "the feed let its slots go", "lens-laws.vl:267:2");
const one_write = sequence3([ 1, 2, 3 ], [ 1 ]);
const feed3 = items(one_write, [ 1 ]);
const cursor5 = cursor2(feed3);
splice6(one_write, 0, 1, [ 9 ], [ 1 ]);
const ops = since2(feed3, cursor5);
assert(ops.length === 1, "a replacing splice is one op", "lens-laws.vl:277:2");
const $hq = __at(ops, 0, "lens-laws.vl:279:3");
assert($hq[0] === 0 && $hq[1] === 0 && eq($hq[2], [ 1 ]) && eq($hq[3], [ 9 ]), "and it carries what left and what arrived", "lens-laws.vl:278:2");
splice6(one_write, 1, 2, [ 7, 8, 6 ], [ 1 ]);
const again = since2(feed3, cursor5);
let $hr = undefined;
assert(again.length === 1 && (($hr = __at(again, 0, "lens-laws.vl:289:7")), $hr[0] === 0) && $hr[1] === 1 && eq($hr[2], [ 2, 3 ]) && eq($hr[3], [ 7, 8, 6 ]), "so is a splice that takes and inserts different counts", "lens-laws.vl:287:2");
const ages = new9();
const people = keyed(ages);
const ada = slot3(people, "ada", [ 1 ]);
const bob = slot3(people, "bob", [ 1 ]);
const ada_runs = __shared_new(0);
const bob_runs = __shared_new(0);
on_change6(__clone(ada), (_age, $hE) => {
	const $hF = [ ada_runs, "v" ];
	return ada_runs.v = $hF[0][$hF[1]] + 1;
}, "lens-laws.vl:303:15");
on_change6(bob, (_age, $hM) => {
	const $hN = [ bob_runs, "v" ];
	return bob_runs.v = $hN[0][$hN[1]] + 1;
}, "lens-laws.vl:304:15");
assert(eq3(get12(ada), [ 1 ]), "a key\'s slot exists before the key", "lens-laws.vl:305:2");
put2(people, "ada", 36, [ 1 ]);
assert(eq3(get12(ada), [ 0, 36 ]) && ada_runs.v === 1 && bob_runs.v === 0, "only ada\'s slot woke", "lens-laws.vl:307:2");
remove2(people, "ada", [ 1 ]);
assert(eq3(get12(ada), [ 1 ]) && ada_runs.v === 2, "the slot reads the removal", "lens-laws.vl:312:2");
put2(people, "ada", 37, [ 1 ]);
assert(eq3(get12(ada), [ 0, 37 ]) && ada_runs.v === 3, "and follows the key when it returns", "lens-laws.vl:314:2");
const spliced = sequence3([ 1, 2, 3, 4 ], [ 1 ]);
const heard_changes = __shared_new([  ]);
const before_changes = __shared_new(0);
tap(spliced, (_change) => {
	const $iq = [ before_changes, "v" ];
	return before_changes.v = $iq[0][$iq[1]] + 1;
}, (change4, stage) => {
	const $ir = stage;
	assert($ir[0] === 0, "a shape write is exact", "lens-laws.vl:321:3");
	heard_changes.v.push(__clone(change4));
	return;
});
splice6(spliced, 1, 2, [ 7, 8, 9 ], [ 1 ]);
push3(spliced, 5, [ 1 ]);
remove_at2(spliced, 0, [ 1 ]);
const one_each = [ [ 0, 1, 2, 3 ], [ 0, 5, 0, 1 ], [ 0, 0, 1, 0 ] ];
assert(eq9(heard_changes.v, one_each), "R16: each splice is one Splice change", "lens-laws.vl:334:2");
assert(before_changes.v === 3, "and `before` heard each once, ahead of the write", "lens-laws.vl:335:2");
const table = sequence([ "a", "b", "c", "d", "e" ], [ 1 ]);
const b_slot = slot(table, 1, [ 1 ]);
const e_slot = slot(table, 4, [ 1 ]);
splice3(table, 1, 2, [ "x", "y", "z" ], [ 1 ]);
assert(eq3(get(b_slot), [ 1 ]) && eq3(get(e_slot), [ 0, "e" ]) && eq5(at(e_slot), [ 0, 5 ]), "the splice moved the slots in one step", "lens-laws.vl:343:2");
assert(eq3(get(slot(table, 1, [ 1 ])), [ 0, "x" ]), "an arrival gets a slot of its own", "lens-laws.vl:347:2");
const sizes = __shared_new(0);
const shapes = __shared_new(0);
on_change7(length(table), (_n, $iu) => {
	const $iv = [ sizes, "v" ];
	return sizes.v = $iv[0][$iv[1]] + 1;
}, "lens-laws.vl:352:29");
on_change8(index(slot(table, 0, [ 1 ])), (_at, $ix) => {
	const $iy = [ shapes, "v" ];
	return shapes.v = $iy[0][$iy[1]] + 1;
}, "lens-laws.vl:353:37");
splice3(table, 0, 1, [ "A" ], [ 1 ]);
assert(sizes.v === 0 && shapes.v === 1, "an equal-count splice leaves the length asleep and moves the shape", "lens-laws.vl:355:2");
splice3(table, 0, 1, [ "A", "A2" ], [ 1 ]);
assert(sizes.v === 1 && shapes.v === 2, "a growing splice wakes both", "lens-laws.vl:360:2");
const rowed = sequence([ "p", "q", "r" ], [ 1 ]);
const rows3 = rows(rowed, [ 1 ]);
const row_cursor = cursor3(rows3);
const q_row = __clone(__at(get11(rows3), 1, "lens-laws.vl:367:14"));
splice3(rowed, 1, 1, [ "Q", "Q2" ], [ 1 ]);
const row_ops = since3(rows3, row_cursor);
assert(row_ops.length === 1, "a replacing splice is one row op", "lens-laws.vl:370:2");
const $iE = __at(row_ops, 0, "lens-laws.vl:372:3");
assert($iE[0] === 0 && $iE[1] === 1 && eq11($iE[2], [ q_row ]) && $iE[3].length === 2, "with the slot that left and the two that arrived", "lens-laws.vl:371:2");
assert(eq13(row_arrived_reads(__at(row_ops, 0, "lens-laws.vl:379:21")), [ [ 0, "Q" ], [ 0, "Q2" ] ]), "the arrived slots read the arrivals", "lens-laws.vl:378:2");
release7(rows3);
const indexed = sequence([ "k", "l" ], [ 1 ]);
const by_name = index_by2(indexed, (name) => {
	return name[0][name[1]];
}, [ 1 ]);
const l_found = find3(by_name, "l");
assert(eq14(map3(__clone(l_found), (slot7) => {
	return get(slot7);
}), [ 0, [ 0, "l" ] ]), "the index holds l\'s slot", "lens-laws.vl:388:2");
release6(by_name);
assert(eq14(map3(l_found, (slot7) => {
	return get(slot7);
}), [ 0, [ 1 ] ]) && tapped(indexed) === 0, "released, the index let its tap and its slots go", "lens-laws.vl:390:2");
const trigger = new8(0, "lens-laws.vl:397:28");
const echo = sequence3([ 0 ], [ 1 ]);
const echo_runs = __shared_new(0);
on_change9(__clone(echo), (_all, $jm) => {
	const $jn = [ echo_runs, "v" ];
	return echo_runs.v = $jn[0][$jn[1]] + 1;
}, "lens-laws.vl:400:19");
on_change10(__clone(trigger), (value, $jo) => {
	replace3(echo, 0, value, [ 1 ]);
	push3(echo, value, [ 1 ]);
	return;
}, "lens-laws.vl:401:23");
batch(($jr) => {
	return set2(trigger, 1, [ 0, $jr ], "lens-laws.vl:405:19");
}, [ 1 ]);
assert(eq(all2(echo), [ 1, 1 ]), "the observer\'s writes landed", "lens-laws.vl:406:2");
assert(echo_runs.v === 1, "and settled with the drain they ran in: one run for two writes", "lens-laws.vl:407:2");
const scores = new9();
const board2 = keyed(scores);
put2(board2, "ada", 1, [ 1 ]);
const ada_score = slot3(board2, "ada", [ 1 ]);
const ada_wakes = __shared_new(0);
on_change6(__clone(ada_score), (_score, $js) => {
	const $jt = [ ada_wakes, "v" ];
	return ada_wakes.v = $jt[0][$jt[1]] + 1;
}, "lens-laws.vl:415:23");
const score_feed = entries2(board2, [ 1 ]);
const score_cursor = cursor4(score_feed);
update2(board2, (held2) => {
	insert(held2, "ada", 2);
	return [ undefined, [ [ 0, "ada", 1, 1 ] ] ];
}, [ 1 ]);
assert(eq3(get12(ada_score), [ 0, 2 ]) && ada_wakes.v === 1, "a keyed splice wakes the key\'s slot", "lens-laws.vl:422:2");
assert(since4(score_feed, score_cursor).length === 1, "and is one map op", "lens-laws.vl:426:2");
const wrapped = [ over4([ "x", "y" ], [ 1 ]), over5(ages_of()) ];
const y_slot = slot4(wrapped[0], 1, [ 1 ]);
insert_at4(wrapped[0], 0, "w", [ 1 ]);
assert(eq3(get(y_slot), [ 0, "y" ]) && eq5(at(y_slot), [ 0, 2 ]) && size4(wrapped[0]) === 3, "a SequenceLens is the sequence lens", "lens-laws.vl:435:2");
put3(wrapped[1], "cy", 9, [ 1 ]);
assert(eq3(get12(slot5(wrapped[1], "cy", [ 1 ])), [ 0, 9 ]) && get14(length3(wrapped[1])) === 2, "a KeyedLens is the keyed lens", "lens-laws.vl:440:2");
assert(eq(all(lens(wrapped[0])), [ "w", "x", "y" ]), "lens() hands out the full lens", "lens-laws.vl:444:2");
const lazy = sequence([ "a", "b" ], [ 1 ]);
const first_handle = slot(lazy, 1, [ 1 ]);
const second_handle = slot(lazy, 1, [ 1 ]);
const lazy_runs = __shared_new(0);
on_change(__clone(first_handle), (_value, $kk) => {
	const $kl = [ lazy_runs, "v" ];
	return lazy_runs.v = $kl[0][$kl[1]] + 1;
}, "lens-laws.vl:453:25");
on_change(__clone(second_handle), (_value, $km) => {
	const $kn = [ lazy_runs, "v" ];
	return lazy_runs.v = $kn[0][$kn[1]] + 1;
}, "lens-laws.vl:454:26");
assert(eq3(identity2(first_handle), identity2(second_handle)), "S3: one slot, one cell, made on the first listen", "lens-laws.vl:455:2");
replace(lazy, 1, "B", [ 1 ]);
assert(lazy_runs.v === 2, "S3: both listeners ride the slot\'s one cell", "lens-laws.vl:460:2");
const counted = over4([ "a" ], [ 1 ]);
const copied = __clone(counted);
push4(counted, "b", [ 1 ]);
const counts = __shared_new(0);
on_change7(length4(copied), (_n, $ko) => {
	const $kp = [ counts, "v" ];
	return counts.v = $kp[0][$kp[1]] + 1;
}, "lens-laws.vl:468:32");
push4(counted, "c", [ 1 ]);
assert(counts.v === 1 && get5(length4(copied)) === 3, "S3: a length asked through one handle hears a write through another", "lens-laws.vl:470:2");
const empty = [  ];
const grown = sequence(empty, [ 1 ]);
const grown_rows = rows(grown, [ 1 ]);
update3(grown, (held2) => {
	held2.push("x");
	held2.push("y");
	return [ undefined, [ [ 1, 0, 1 ], [ 1, 0, 1 ] ] ];
}, [ 1 ]);
const grown_slots = get11(grown_rows);
assert(grown_slots.length === 2 && key(__at(grown_slots, 0, "lens-laws.vl:487:29")) !== key(__at(grown_slots, 1, "lens-laws.vl:487:53")), "S3: the second change moved the slot the first change\'s tap made", "lens-laws.vl:486:2");
assert(eq3(get(__at(grown_slots, 0, "lens-laws.vl:491:3")), [ 0, "x" ]) && eq3(get(__at(grown_slots, 1, "lens-laws.vl:491:40")), [ 0, "y" ]), "S3: and each row reads its own item", "lens-laws.vl:490:2");
release7(grown_rows);
const bag = sequence4([ [ "p", "q", "r", "s" ] ], [ 1 ]);
const p_slot = slot6(bag, 0, [ 1 ]);
move_item2(bag, 0, 2, [ 1 ]);
assert(eq(all4(bag), [ "q", "r", "p", "s" ]), "S3: the default shift moved the item", "lens-laws.vl:501:2");
assert(eq5(at(p_slot), [ 0, 2 ]) && eq3(get3(p_slot), [ 0, "p" ]), "S3: and its slot went with it", "lens-laws.vl:502:2");
move_item2(bag, 3, 0, [ 1 ]);
assert(eq(all4(bag), [ "s", "q", "r", "p" ]), "S3: leftward too", "lens-laws.vl:504:2");
const p_width = part2(slot6(bag, 3, [ 1 ]), "width", (line) => {
	return line[0][line[1]].length;
});
assert(eq5(get2(p_width), [ 0, 1 ]), "S2: a part reads through a supertrait\'s default lend", "lens-laws.vl:511:2");
edit4(slot6(bag, 3, [ 1 ]), "width", (line) => {
	return line[0][line[1]] = "pp";
}, [ 1 ]);
assert(eq5(get2(p_width), [ 0, 2 ]) && eq(all4(bag), [ "s", "q", "r", "pp" ]), "S2: and an edit writes through change\'s default", "lens-laws.vl:513:2");
console.log("laws: all passed");
//# sourceMappingURL=lens-laws.mjs.map
