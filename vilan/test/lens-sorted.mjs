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
function __force(cell) {
	if (cell.state === 2) return cell.value;
	if (cell.state === 1) throw "lazy initialization cycle: `" + cell.name + "`";
	if (cell.state === 3) throw "lazy `" + cell.name + "` is poisoned: its initializer panicked: " + (cell.value && cell.value.location !== undefined ? cell.value.message : cell.value);
	cell.state = 1;
	try {
		cell.value = cell.thunk();
	} catch (failure) {
		cell.state = 3;
		cell.value = failure;
		throw failure;
	}
	cell.state = 2;
	cell.thunk = null;
	return cell.value;
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
function __lazy(name, thunk) {
	return { name: name, state: 0, value: undefined, thunk: thunk };
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
	let index2 = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b, index2, "std/src/compare.vl:147:17"))) {
			return false;
		}
		index2 = index2 + 1;
	}
	return true;
}
function hash(self) {
	return __hash(self);
}
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function assert(condition, message, caller) {
	let $cH = null;
	if (!(condition)) {
		$cH = (() => {
			throw __panic(message, caller);
		})();
	}
	return $cH;
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
function eq2(self, b) {
	return self === b;
}
function eq3(self, b) {
	const $ey = self;
	let $eB = null;
	if ($ey[0] === 0) {
		const $ez = b;
		let $eA = null;
		if ($ez[0] === 0) {
			$eA = $ey[1] === $ez[1];
		} else {
			$eA = false;
		}
		$eB = $eA;
	} else {
		const $eC = b;
		$eB = $eC[0] === 1;
	}
	return $eB;
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
		const key = hash(subscriber[0]);
		let $aG = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$aG = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			if (tracing.v) {
				trace_hooks.v[3](turn, subscriber[0]);
			}
			let index2 = turn[0].v.length;
			while (index2 > 0 && __at(turn[0].v, index2 - 1, "std/src/reactive.vl:421:21")[0] > subscriber[0]) {
				index2 = index2 - 1;
			}
			__insert_at(turn[0].v, index2, __clone(subscriber), "std/src/reactive.vl:424:25");
		}
		$aG;
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
	const $aE = turn;
	let $aF = null;
	if ($aE[0] === 0) {
		const ambient = $aE[1];
		$aF = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aH = last(draining_turns.v);
		let $aI = null;
		if ($aH[0] === 0) {
			const draining = $aH[1];
			$aI = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aI = undefined;
		}
		$aF = $aI;
	}
	return $aF;
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
function dispose(self, $cd) {
	const $ce = $cd;
	let $cf = null;
	if ($ce[0] === 0) {
		const established = $ce[1];
		$cf = [ 0, established ];
	} else {
		$cf = last(draining_turns.v);
	}
	const ambient = $cf;
	release_under(self, ambient);
}
function detach(handle) {
	const $aT = last2(releasing_turns.v);
	let $aU = null;
	if ($aT[0] === 0) {
		const at_release = $aT[1];
		$aU = at_release;
	} else {
		$aU = last(draining_turns.v);
	}
	const turn = $aU;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aV = [ 0, handle[0] ];
	let $aW = null;
	if ($aV[0] === 0) {
		const subscribers = $aV[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aW = undefined;
	} else {
		$aW = undefined;
	}
	$aW;
	const $aX = ambient;
	let $aY = null;
	if ($aX[0] === 0) {
		const turn = $aX[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== handle[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[2].v.delete(hash(handle[1]));
		let kept_derived = [  ];
		for (const subscriber3 of turn[1].v) {
			if (subscriber3[0] !== handle[1]) {
				kept_derived.push(__clone(subscriber3));
			}
		}
		turn[1].v = kept_derived;
		turn[3].v.delete(hash(handle[1]));
		if (tracing.v) {
			trace_hooks.v[7](turn, handle[1]);
		}
		$aY = undefined;
	} else {
		$aY = undefined;
	}
	$aY;
	const $aZ = handle[3].v;
	let $ba = null;
	if ($aZ[0] === 0) {
		const release7 = $aZ[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release7, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ba = undefined;
	} else {
		$ba = undefined;
	}
	return $ba;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $p = null;
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
		$p = undefined;
	}
	return $p;
}
function stage_nursery(self) {
	const $ar = nursery(self);
	let $as = null;
	if ($ar[0] === 0) {
		const held = $ar[1];
		$as = held;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$as = made;
	}
	return $as;
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
	let $aq = null;
	if (is_disposed(self)) {
		$aq = [ 1 ];
	} else {
		$aq = self[0].v[3];
	}
	return $aq;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $bm = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $bd = held[3];
		let $be = null;
		if ($bd[0] === 0) {
			const nursery2 = $bd[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$be = undefined;
		} else {
			$be = undefined;
		}
		$be;
		let $bl = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $bg = __guarded(cleanup);
				let $bh = null;
				if ($bg[0] === 0) {
					const message = $bg[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$bh = undefined;
				} else {
					$bh = undefined;
				}
				$bh;
			}
			const $bj = failure;
			let $bk = null;
			if ($bj[0] === 0) {
				const message2 = $bj[1];
				$bk = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$bk = undefined;
			}
			$bl = $bk;
		}
		$bm = $bl;
	}
	return $bm;
}
function register_with_owner(subscription, $bY, $bZ) {
	const $ca = $bZ;
	let $cb = null;
	if ($ca[0] === 0) {
		const owner = $ca[1];
		$cb = take(owner, subscription, $bY);
	} else {
		$cb = __clone(subscription);
	}
	return $cb;
}
function defer_to_owner(cleanup, $m) {
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const owner = $n[1];
		$o = defer(owner, cleanup);
	} else {
		$o = undefined;
	}
	return $o;
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
function also_releasing(handle, release7) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release7();
		const $bW = previous;
		let $bX = null;
		if ($bW[0] === 0) {
			const earlier = $bW[1];
			$bX = earlier();
		} else {
			$bX = undefined;
		}
		return $bX;
	} ];
}
function settle_together(body, $dG) {
	const $dH = $dG;
	let $dI = null;
	if ($dH[0] === 0) {
		const current = $dH[1];
		$dI = body(current);
	} else {
		const $dJ = last(draining_turns.v);
		let $dK = null;
		if ($dJ[0] === 0) {
			const draining = $dJ[1];
			$dK = body(draining);
		} else {
			$dK = batch(body, $dG);
		}
		$dI = $dK;
	}
	return $dI;
}
function splice_start(held, at4) {
	let $aM = null;
	if (at4 > held) {
		$aM = held;
	} else {
		$aM = at4;
	}
	return $aM;
}
function splice_count(held, start, removed) {
	let $aN = null;
	if (start + removed > held) {
		$aN = held - start;
	} else {
		$aN = removed;
	}
	return $aN;
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
function apply(self, change, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change, wakes);
	}
}
function locate(self, slot2) {
	if (self[0].length === 0) {
		return [ 1 ];
	}
	return locate2(__at(self[0], 0, "std/src/reactive/lens.vl:158:3"), slot2);
}
function follows_items(self) {
	return true;
}
function release(self, slot2) {
	if (self[0].length > 0) {
		release2(__at(self[0], 0, "std/src/reactive/lens.vl:167:4"), slot2);
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
	let place2 = 0;
	for (const block of self[1]) {
		starts.push(running);
		__at_put(self[2], as_usize(block), place2, "std/src/reactive/lens.vl:229:4");
		running = running + __at(self[0], as_usize(block), "std/src/reactive/lens.vl:230:15").length;
		place2 = place2 + 1;
	}
	self[3] = starts;
}
function renumber(self, block, from) {
	let index2 = from;
	const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:239:14").length;
	while (index2 < held) {
		const slot2 = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:241:15"), index2, "std/src/reactive/lens.vl:241:15");
		if (slot2 !== -(1)) {
			__at_put(self[4], as_usize(slot2), block, "std/src/reactive/lens.vl:243:5");
			__at_put(self[5], as_usize(slot2), index2, "std/src/reactive/lens.vl:244:5");
		}
		index2 = index2 + 1;
	}
}
function open(self, place2) {
	const block = as_i32(self[0].length);
	self[0].push([  ]);
	self[2].push(0);
	__insert_at(self[1], place2, block, "std/src/reactive/lens.vl:255:14");
	return block;
}
function widen(self, at4, count2) {
	if (count2 === 0 || at4 >= self[6]) {
		return;
	}
	const $dh = seat(self, at4);
	const place2 = $dh[0];
	const index2 = $dh[1];
	const block = __at(self[1], place2, "std/src/reactive/lens.vl:265:15");
	let $di = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$di = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index2) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:277:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before.length;
		while (copied < held) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:287:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:290:4");
	}
	$di;
	self[6] = self[6] + count2;
	renumber(self, block, index2 + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:294:6").length > 2 * block_size) {
		rebalance(self, place2);
	}
	restart(self);
}
function close(self, at4, count2, wakes) {
	let left = count2;
	while (left > 0 && at4 < self[6]) {
		const $df = seat(self, at4);
		const place2 = $df[0];
		const index2 = $df[1];
		const block = __at(self[1], place2, "std/src/reactive/lens.vl:306:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $dg = null;
		if (left < held - index2) {
			$dg = left;
		} else {
			$dg = held - index2;
		}
		const take2 = $dg;
		let taken = 0;
		while (taken < take2) {
			const slot2 = __remove_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:311:16"), index2, "std/src/reactive/lens.vl:311:46");
			if (slot2 !== -(1)) {
				__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:313:6");
				wakes.push([ 2, slot2 ]);
			}
			taken = taken + 1;
		}
		self[6] = self[6] - take2;
		left = left - take2;
		if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:320:7").length === 0) {
			__remove_at(self[1], place2, "std/src/reactive/lens.vl:321:16");
		} else {
			renumber(self, block, index2);
		}
		restart(self);
	}
}
function rebalance(self, place2) {
	const block = __at(self[1], place2, "std/src/reactive/lens.vl:331:15");
	const all3 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first = [  ];
	let index2 = 0;
	while (index2 < block_size) {
		first.push(__at(all3, index2, "std/src/reactive/lens.vl:336:15"));
		index2 = index2 + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:339:3");
	let next = place2 + 1;
	const held = all3.length;
	while (index2 < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index2 < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all3, index2, "std/src/reactive/lens.vl:346:40"));
			index2 = index2 + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at4) {
	while (self[6] <= at4) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last3 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last3), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at4);
	const place2 = $j[0];
	const index2 = $j[1];
	const block = __at(self[1], place2, "std/src/reactive/lens.vl:371:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index2, "std/src/reactive/lens.vl:372:14");
	if (held !== -(1)) {
		return held;
	}
	const slot2 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index2);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index2, slot2, "std/src/reactive/lens.vl:379:3");
	return slot2;
}
function apply2(self, change, wakes) {
	const $dd = change;
	let $de = null;
	if ($dd[0] === 0) {
		const at4 = $dd[1];
		const removed = $dd[2];
		const inserted = $dd[3];
		close(self, at4, removed, wakes);
		widen(self, at4, inserted);
		$de = undefined;
	} else if ($dd[0] === 1) {
		const at5 = $dd[1];
		const count2 = $dd[2];
		$de = widen(self, at5, count2);
	} else if ($dd[0] === 3) {
		const at6 = $dd[1];
		const count3 = $dd[2];
		$de = close(self, at6, count3, wakes);
	} else if ($dd[0] === 2) {
		const at7 = $dd[1];
		let $dk = null;
		if (at7 < self[6]) {
			const $dj = seat(self, at7);
			const place2 = $dj[0];
			const index2 = $dj[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$dk = undefined;
		}
		$de = $dk;
	} else if ($dd[0] === 4) {
		const at8 = $dd[1];
		const part2 = $dd[2];
		let $dm = null;
		if (at8 < self[6]) {
			const $dl = seat(self, at8);
			const place3 = $dl[0];
			const index3 = $dl[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place3, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part2 ]);
			}
			$dm = undefined;
		}
		$de = $dm;
	} else if ($dd[0] === 5) {
		const from = $dd[1];
		const to = $dd[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $dn = null;
		if (from > to) {
			$dn = from;
		} else {
			$dn = to;
		}
		const far = $dn;
		while (self[6] <= far) {
			const places = self[1].length;
			if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:419:36")), "std/src/reactive/lens.vl:419:24").length >= block_size) {
				const fresh = open(self, places);
				__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:421:7");
				self[3].push(self[6]);
			}
			const last3 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:424:17");
			__at(self[0], as_usize(last3), "std/src/reactive/lens.vl:425:6").push(-(1));
			self[6] = self[6] + 1;
		}
		const $do = seat(self, from);
		const left = $do[0];
		const index4 = $do[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $dp = null;
		if (to >= self[6]) {
			const last4 = self[1].length - 1;
			$dp = [ last4, __at(self[0], as_usize(__at(self[1], last4, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$dp = seat(self, to);
		}
		const $dq = $dp;
		const place4 = $dq[0];
		const landing = $dq[1];
		const target = __at(self[1], place4, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot4, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$de = undefined;
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
		$de = undefined;
	}
	return $de;
}
function release2(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:471:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:475:3"), __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:475:33"), -(1), "std/src/reactive/lens.vl:475:3");
	__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:476:3");
}
function locate2(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:480:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:484:20"), "std/src/reactive/lens.vl:484:8") + __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:484:51") ];
}
function read_only() {
	(() => {
		throw __panic("lens: this sequence is read-only (`Indexed`): write it through `update` or `plan`", "std/src/reactive/lens.vl:1542:2");
	})();
}
function extend_run(runs, from, to) {
	const held = runs.v.length;
	let $R = null;
	if (held > 0) {
		const $Q = __at(runs.v, held - 1, "std/src/reactive/lens.vl:1992:31");
		const start = $Q[0];
		const count2 = $Q[1];
		const first = $Q[2];
		const leftward = first < start && from === start + count2 && to === first + count2;
		const rightward = first > start && from === start && to === first;
		if (leftward || rightward) {
			__at_put(runs.v, held - 1, [ start, count2 + 1, first ], "std/src/reactive/lens.vl:1996:4");
			return;
		}
		$R = undefined;
	}
	$R;
	runs.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function place(self, value) {
	let low = 0;
	let high = self[0].length;
	while (low < high) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[0], middle, "lens-sorted.vl:26:7") <= value) {
			low = middle + 1;
		} else {
			high = middle;
		}
	}
	return low;
}
function insert(self, value) {
	const at4 = place(self, value);
	__insert_at(self[0], at4, value, "lens-sorted.vl:37:14");
	return [ 1, at4, 1 ];
}
function position(self, value) {
	const at4 = place(self, value);
	if (at4 === 0 || __at(self[0], at4 - 1, "lens-sorted.vl:44:17") !== value) {
		return [ 1 ];
	}
	return [ 0, at4 - 1 ];
}
function remove_at(self, at4) {
	__remove_at(self[0], at4, "lens-sorted.vl:51:14");
}
function reprice(self, at4, value) {
	__remove_at(self[0], at4, "lens-sorted.vl:57:14");
	const to = place(self, value);
	__insert_at(self[0], to, value, "lens-sorted.vl:59:14");
	if (to === at4) {
		return [ [ 2, at4 ] ];
	}
	return [ [ 5, at4, to ], [ 2, to ] ];
}
function count(self) {
	return self[0].length;
}
function item(self, at4) {
	return __list_get(self[0], at4);
}
function ordered(values) {
	let at4 = 1;
	while (at4 < values.length) {
		if (__at(values, at4 - 1, "lens-sorted.vl:80:6") > __at(values, at4, "lens-sorted.vl:80:23")) {
			return false;
		}
		at4 = at4 + 1;
	}
	return true;
}
function lend(self, at4, read) {
	const $a = item(self, at4);
	const item2 = [ $a[1] ];
	if ($a[0] === 0) {
		read([ item2, 0 ]);
	}
}
function new7(value, caller) {
	let subscribers = [  ];
	const cell = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell[0]), caller);
	}
	return cell;
}
function lens_over(storage, handler, access) {
	return [ __shared_new([ __clone(storage), __clone(access), new7(0, "std/src/reactive/lens.vl:729:28"), __shared_new([ handler, [  ], [ 1 ], [ 1 ] ]), [  ], 0, [ 1 ] ]) ];
}
function over(value, handler, access) {
	return lens_over([ 0, __shared_new(value) ], handler, access);
}
function indexed_lens(value) {
	return over(value, new5(), [ (held, at4) => {
		return at4 < count(held);
	}, (held, at4) => {
		return item(held, at4);
	}, (held, at4, read) => {
		return lend(held, at4, read);
	}, (_held, _at, _value) => {
		return read_only();
	}, (_held, _at, _with) => {
		return read_only();
	} ]);
}
function indexed(value) {
	return indexed_lens(value);
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
		port[0]((held) => {
			return out = [ 0, read(held) ];
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
	if (viewing(watch, (held) => {
		return follows_items(held[0]);
	}) && !(peek_storage(storage, (held) => {
		return holds(held, at4);
	}))) {
		return -(1);
	}
	const id = follow(watch.v[0], at4);
	const entry = as_usize(id);
	while (watch.v[1].length <= entry) {
		watch.v[1].push([ [ 1 ], [  ], 0 ]);
	}
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $l = __clone(self);
	let $k = null;
	if (id < 0) {
		$k = -(1);
	} else {
		$k = id;
	}
	return [ $l, $k ];
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
	const left = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1071:13")[2] - 1;
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1072:2")[2] = left;
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
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
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
		port[0]((held) => {
			return out = [ 0, read(held) ];
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
	return peek(self[0], (held) => {
		return read(held, $s[1]);
	});
}
function slot_cell(watch, slot2) {
	if (slot2 < 0) {
		return new7(0, "std/src/reactive/lens.vl:1082:19");
	}
	const entry = as_usize(slot2);
	const $x = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1085:5")[0];
	const cell = __clone($x[1]);
	if ($x[0] === 0) {
		return cell;
	}
	const cell2 = new7(0, "std/src/reactive/lens.vl:1088:25");
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1089:2")[0] = [ 0, __clone(cell2) ];
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
function length(self) {
	return [ __clone(self) ];
}
function peek2(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get2(self) {
	return peek2(self[0], (held) => {
		return count(held);
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $E = watch.v[3];
	const cell = __clone($E[1]);
	if ($E[0] === 0) {
		return cell;
	}
	const cell2 = new7(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle3(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
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
function new8() {
	return [ __shared_new([  ]), __shared_new(0), __shared_new(0), __shared_new([  ]), delta_log_limit ];
}
function arrived_at(storage, at4, count2) {
	return peek_storage2(storage, (held) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count2) {
			const $J = item(held, index2);
			if ($J[0] === 0) {
				arrived.push($J[1]);
			}
			index2 = index2 + 1;
		}
		return arrived;
	});
}
function one(item2) {
	const $O = item2;
	let $P = null;
	if ($O[0] === 0) {
		const value = $O[1];
		$P = [ __clone(value) ];
	} else {
		$P = [  ];
	}
	return $P;
}
function all_of(storage) {
	return peek_storage2(storage, (held) => {
		let out = [  ];
		let at4 = 0;
		const count2 = count(held);
		while (at4 < count2) {
			const $U = item(held, at4);
			if ($U[0] === 0) {
				out.push($U[1]);
			}
			at4 = at4 + 1;
		}
		return out;
	});
}
function trim(self) {
	let lowest = self[1].v;
	for (const cursor4 of self[3].v) {
		const at4 = cursor4[1].v;
		if (at4 < lowest) {
			lowest = at4;
		}
	}
	const base = self[2].v;
	if (lowest > base) {
		const dropped = lowest - base;
		let kept = [  ];
		let index2 = as_usize(dropped);
		const held = __clone(self[0].v);
		const length3 = held.length;
		while (index2 < length3) {
			const $V = __list_get(held, index2);
			let $W = null;
			if ($V[0] === 0) {
				const op = $V[1];
				$W = kept.push(op);
			} else {
				$W = undefined;
			}
			$W;
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
	const after = peek_storage2(storage, (held) => {
		return item(held, at4);
	});
	const $aa = before;
	let $ab = undefined;
	if ($aa[0] === 0 && (($ab = after), $ab[0] === 0)) {
		record(log, [ 1, at4, __clone($aa[1]), __clone($ab[1]) ]);
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
function release4(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $G) {
	const leaving = __shared_new([  ]);
	const runs = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0].v[0]);
	const tap2 = tap(self, (change) => {
		const $H = change;
		let $I = null;
		if ($H[0] === 0) {
			const at4 = $H[1];
			const removed = $H[2];
			const _inserted = $H[3];
			leaving.v = arrived_at(storage, at4, removed);
			$I = undefined;
		} else if ($H[0] === 3) {
			const at5 = $H[1];
			const count2 = $H[2];
			leaving.v = arrived_at(storage, at5, count2);
			$I = undefined;
		} else if ($H[0] === 2) {
			const at6 = $H[1];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at6);
			}));
			$I = undefined;
		} else if ($H[0] === 4) {
			const at7 = $H[1];
			const _part = $H[2];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at7);
			}));
			$I = undefined;
		} else if ($H[0] === 5) {
			const from = $H[1];
			const to = $H[2];
			$I = extend_run(runs, from, to);
		} else {
			$I = undefined;
		}
		return $I;
	}, (change, stage) => {
		const $S = stage;
		let $T = null;
		if ($S[0] === 2) {
			record(log, [ 2, all_of(storage) ]);
			return;
			$T = undefined;
		} else if ($S[0] === 3) {
			return;
		} else {
			$T = undefined;
		}
		$T;
		const $X = stage;
		const late = $X[0] === 1;
		const $Y = change;
		let $Z = null;
		if ($Y[0] === 0) {
			const at4 = $Y[1];
			const removed = $Y[2];
			const inserted = $Y[3];
			if (late && removed > 0) {
				record(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record(log, [ 0, at4, [  ], arrived_at(storage, at4, inserted) ]);
			} else {
				record(log, [ 0, at4, __clone(leaving.v), arrived_at(storage, at4, inserted) ]);
			}
			$Z = undefined;
		} else if ($Y[0] === 1) {
			const at5 = $Y[1];
			const count2 = $Y[2];
			record(log, [ 0, at5, [  ], arrived_at(storage, at5, count2) ]);
			$Z = undefined;
		} else if ($Y[0] === 3) {
			const at6 = $Y[1];
			const _count = $Y[2];
			if (late) {
				record(log, [ 2, all_of(storage) ]);
			} else {
				record(log, [ 0, at6, __clone(leaving.v), [  ] ]);
			}
			$Z = undefined;
		} else if ($Y[0] === 2) {
			const at7 = $Y[1];
			$Z = record_set(log, storage, leaving, at7, late);
		} else if ($Y[0] === 4) {
			const at8 = $Y[1];
			const _part = $Y[2];
			$Z = record_set(log, storage, leaving, at8, late);
		} else if ($Y[0] === 5) {
			const from = $Y[1];
			const to = $Y[2];
			if (skipping.v > 0) {
				const $ac = [ skipping, "v" ];
				skipping.v = $ac[0][$ac[1]] - 1;
				return;
			}
			if (runs.v.length === 0) {
				record(log, [ 3, from, 1, to ]);
				return;
			}
			const $ad = __remove_at(runs.v, 0, "std/src/reactive/lens.vl:1679:47");
			const start = $ad[0];
			const count3 = $ad[1];
			const first = $ad[2];
			skipping.v = count3 - 1;
			let $ae = null;
			if (first > start) {
				$ae = first + 1 - count3;
			} else {
				$ae = first;
			}
			const landing = $ae;
			record(log, [ 3, start, count3, landing ]);
			$Z = undefined;
		} else {
			$Z = record(log, [ 2, all_of(storage) ]);
		}
		return $Z;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $G);
	return feed;
}
function items(self, $F) {
	return items_into(self, new8(), $F);
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
function since(self, cursor4) {
	const at4 = cursor4[1].v;
	const version = self[1].v;
	let ops = [  ];
	if (at4 >= version) {
		return [ 0, ops ];
	}
	cursor4[1].v = version;
	const base = self[2].v;
	if (at4 < base) {
		return [ 1 ];
	}
	let index2 = as_usize(at4 - base);
	const held = __clone(self[0].v);
	const length3 = held.length;
	while (index2 < length3) {
		const $ak = __list_get(held, index2);
		let $al = null;
		if ($ak[0] === 0) {
			const op = $ak[1];
			$al = ops.push(op);
		} else {
			$al = undefined;
		}
		$al;
		index2 = index2 + 1;
	}
	return [ 0, ops ];
}
function all(self) {
	return all_of(self[0].v[0]);
}
function since2(self, cursor4) {
	const $am = since(self[1], cursor4);
	let $an = null;
	if ($am[0] === 0) {
		const recorded = $am[1];
		$an = recorded;
	} else {
		$an = [ [ 2, all(self[0]) ] ];
	}
	return $an;
}
function reader(self) {
	return (cursor4) => {
		return since2(self, cursor4);
	};
}
function get3(self) {
	return all(self[0]);
}
function on_settle4(self, subscriber) {
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
		return get3(self);
	}, (subscriber) => {
		return on_settle4(self, subscriber);
	}, () => {
		return read(cursor4);
	}, () => {
		return drop_cursor2(self, cursor4);
	} ];
}
function run_element(stage, body) {
	const nursery2 = stage_nursery(stage);
	const value = (($at) => {
		return (($au) => {
			return body($at, $au);
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
	core[3].v.set(hash(id), __clone(hold));
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $aD = null;
	if (is_empty(self)) {
		$aD = [ 1 ];
	} else {
		$aD = __list_get(self, self.length - 1);
	}
	return $aD;
}
function admit(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $aw = core[8](item2);
	const answer = $aw[0];
	const owner = $aw[1];
	const $ax = answer;
	let $ay = null;
	if ($ax[0] === 0) {
		const value = $ax[1];
		const $az = owner;
		let $aA = null;
		if ($az[0] === 0) {
			const held = $az[1];
			$aA = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$aA = undefined;
		}
		$aA;
		$ay = [ id, __clone(value) ];
	} else {
		const instance = $ax[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $aB = consumer.v;
			let $aC = null;
			if ($aB[0] === 0) {
				const downstream = $aB[1];
				$aC = wake(downstream);
			} else {
				$aC = undefined;
			}
			return $aC;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$ay = [ id, pull() ];
	}
	return $ay;
}
function seed(core, items2) {
	for (const item2 of items2) {
		const $aJ = admit(core, item2);
		const id = $aJ[0];
		const value = $aJ[1];
		core[0].v.push(id);
		core[1].v.push(__clone(item2));
		core[2].v.push(value);
	}
}
function fresh_list(list) {
	let copy = [  ];
	for (const item2 of list) {
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
function held_pairs(core, at4, count2) {
	let pairs = [  ];
	let index2 = at4;
	while (index2 < at4 + count2) {
		pairs.push([ __clone(__at(core[1].v, index2, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index2, "std/src/reactive/delta.vl:1618:41")) ]);
		index2 = index2 + 1;
	}
	return pairs;
}
function last2(self) {
	let $aS = null;
	if (is_empty(self)) {
		$aS = [ 1 ];
	} else {
		$aS = __list_get(self, self.length - 1);
	}
	return $aS;
}
function is_none(self) {
	const $bf = self;
	return $bf[0] === 1;
}
function retire(core, id) {
	const $aO = __map_get(core[3].v, hash(id));
	let $aP = null;
	if ($aO[0] === 0) {
		const hold = $aO[1];
		core[3].v.delete(hash(id));
		const $aQ = hold[2];
		let $aR = null;
		if ($aQ[0] === 0) {
			const handle = $aQ[1];
			$aR = detach(handle);
		} else {
			$aR = undefined;
		}
		$aR;
		hold[3]();
		const $bb = hold[4];
		let $bc = null;
		if ($bb[0] === 0) {
			const owner = $bb[1];
			$bc = dispose2(owner);
		} else {
			$bc = undefined;
		}
		$aP = $bc;
	} else {
		$aP = undefined;
	}
	return $aP;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops) {
	let events = [  ];
	for (const op of ops) {
		const $aK = op;
		let $aL = null;
		if ($aK[0] === 0) {
			const at4 = $aK[1];
			const removed = $aK[2];
			const inserted = $aK[3];
			const held = core[0].v.length;
			const start = splice_start(held, at4);
			const taking = splice_count(held, start, removed.length);
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
			for (const item2 of inserted) {
				const $bn = admit(core, item2);
				const id2 = $bn[0];
				const value = $bn[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$aL = undefined;
		} else if ($aK[0] === 1) {
			const at5 = $aK[1];
			const _was = $aK[2];
			const item3 = $aK[3];
			const $bo = __list_get(core[0].v, at5);
			let $bp = null;
			if ($bo[0] === 0) {
				const old_id = $bo[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $bq = admit(core, item3);
				const id3 = $bq[0];
				const value2 = $bq[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item3), value2 ], true ]);
				$bp = undefined;
			} else {
				$bp = undefined;
			}
			$aL = $bp;
		} else if ($aK[0] === 2) {
			const items2 = $aK[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$aL = undefined;
		} else {
			const from = $aK[1];
			const count2 = $aK[2];
			const to = $aK[3];
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
			$aL = undefined;
		}
		$aL;
	}
	const woke = __clone(core[4].v);
	let $bv = null;
	if (woke.length > 0) {
		core[4].v = [  ];
		let positions = new Map();
		let index2 = 0;
		for (const id5 of core[0].v) {
			positions.set(hash(id5), index2);
			index2 = index2 + 1;
		}
		let seen = new Map();
		for (const id6 of woke) {
			if (seen.has(hash(id6))) {
				continue;
			}
			seen.set(hash(id6), true);
			const $br = __map_get(positions, hash(id6));
			let $bs = null;
			if ($br[0] === 0) {
				const at6 = $br[1];
				const $bt = __map_get(core[3].v, hash(id6));
				let $bu = null;
				if ($bt[0] === 0) {
					const hold = $bt[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$bu = undefined;
				} else {
					$bu = undefined;
				}
				$bs = $bu;
			} else {
				$bs = undefined;
			}
			$bs;
		}
		$bv = undefined;
	}
	$bv;
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
		const $bw = event;
		let $bx = null;
		if ($bw[0] === 0) {
			const at4 = $bw[1];
			const left = $bw[2];
			const arrived = $bw[3];
			ops.push([ 0, at4, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$bx = undefined;
		} else if ($bw[0] === 1) {
			const at5 = $bw[1];
			const before = $bw[2];
			const after = $bw[3];
			const _rerun = $bw[4];
			ops.push([ 1, at5, __clone(before[1]), __clone(after[1]) ]);
			$bx = undefined;
		} else if ($bw[0] === 2) {
			const _left = $bw[1];
			const now = $bw[2];
			$bx = ops.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $bw[1];
			const count2 = $bw[2];
			const to = $bw[3];
			$bx = ops.push([ 3, from, count2, to ]);
		}
		$bx;
	}
	return ops;
}
function release_core(core) {
	for (const id of __clone(core[0].v)) {
		retire(core, id);
	}
	dispose2(core[6]);
}
function core_release(core, upstream) {
	const release7 = upstream[3];
	return () => {
		release_core(core);
		release7();
		return;
	};
}
function open3(self) {
	const transform = self[1];
	const upstream = open2(__clone(self[0]));
	const stage = new4();
	const core = element_core(stage, (item2) => {
		const $av = run_element(stage, ($ao, $ap) => {
			return transform(item2, $ao, $ap);
		});
		const answer = $av[0];
		const owner = $av[1];
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
	return [ __shared_new(elements), new7(0, "std/src/reactive/delta.vl:792:68"), __clone(log) ];
}
function of(elements) {
	return list_cell(elements, new8());
}
function size(self) {
	return self[0].v.length;
}
function splice_in_place(list, start, taking, inserted) {
	let left = [  ];
	let taken = 0;
	while (taken < taking) {
		left.push(__remove_at(list, start, "std/src/reactive/delta.vl:695:18"));
		taken = taken + 1;
	}
	let offset = 0;
	const arriving = inserted.length;
	while (offset < arriving) {
		const $bE = __list_get(inserted, offset);
		let $bF = null;
		if ($bE[0] === 0) {
			const value = $bE[1];
			$bF = __insert_at(list, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$bF = undefined;
		}
		$bF;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at2(self) {
	return self[1].v;
}
function notify(self, $bI) {
	const traced = tracing.v;
	let $bJ = null;
	if (traced) {
		$bJ = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$bJ = "";
	}
	const cause = $bJ;
	const $bK = $bI;
	let $bL = null;
	if ($bK[0] === 0) {
		const turn = $bK[1];
		$bL = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $bM = last(draining_turns.v);
		let $bN = null;
		if ($bM[0] === 0) {
			const draining = $bM[1];
			$bN = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $bO = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$bO = undefined;
				}
				$bO;
			}
			$bN = undefined;
		}
		$bL = $bN;
	}
	return $bL;
}
function set(self, value, $bH, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $bH);
}
function publish(cell, $bG) {
	set(cell[1], at2(cell[2]), $bG, "std/src/reactive/delta.vl:799:17");
}
function splice(self, at4, removed, inserted, $bD) {
	const held = size(self);
	const start = splice_start(held, at4);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $bD);
}
function set_at(self, at4, value, $bP) {
	const $bQ = __list_get(self[0].v, at4);
	let $bR = null;
	if ($bQ[0] === 0) {
		const previous = $bQ[1];
		__at_put(self[0].v, at4, __clone(value), "std/src/reactive/delta.vl:855:5");
		record(self[2], [ 1, at4, previous, __clone(value) ]);
		publish(self, $bP);
		$bR = undefined;
	} else {
		$bR = undefined;
	}
	return $bR;
}
function set2(self, value, $bS, caller) {
	self[0].v = __clone(value);
	record(self[2], [ 2, __clone(value) ]);
	publish(self, $bS);
}
function move_range(self, from, count2, to, $bT) {
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
		const $bU = __list_get(lifted, offset);
		let $bV = null;
		if ($bU[0] === 0) {
			const value = $bU[1];
			$bV = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$bV = undefined;
		}
		$bV;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count2, to ]);
	publish(self, $bT);
}
function apply_ops(cell, ops, $bA) {
	for (const op of ops) {
		const $bB = op;
		let $bC = null;
		if ($bB[0] === 0) {
			const at4 = $bB[1];
			const removed = $bB[2];
			const inserted = $bB[3];
			$bC = splice(cell, at4, removed.length, inserted, $bA);
		} else if ($bB[0] === 1) {
			const at5 = $bB[1];
			const _was = $bB[2];
			const value = $bB[3];
			$bC = set_at(cell, at5, value, $bA);
		} else if ($bB[0] === 2) {
			const items2 = $bB[1];
			$bC = set2(cell, items2, $bA, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $bB[1];
			const count2 = $bB[2];
			const to = $bB[3];
			$bC = move_range(cell, from, count2, to, $bA);
		}
		$bC;
	}
}
function take(self, item2, $cc) {
	defer(self, () => {
		dispose(item2, $cc);
		return;
	});
	return __clone(item2);
}
function seal_collection(instance, owned, $by, $bz, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $by);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $by, $bz);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $by);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $ai, $aj, caller) {
	return [ seal_collection(open3(self), true, $ai, $aj, caller) ];
}
function release5(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $cg) {
	const log = new8();
	const held = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const fill = () => {
		let ids = [  ];
		let at4 = 0;
		const count2 = peek_storage2(storage, (list) => {
			return count(list);
		});
		while (at4 < count2) {
			ids.push(follow_id(storage, watch, holds, at4));
			at4 = at4 + 1;
		}
		return ids;
	};
	const leave = (at4, count2) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held.v, at4, "std/src/reactive/lens.vl:1839:27");
			release_id(watch, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at4, count2) => {
		let arrived = [  ];
		let index2 = at4;
		while (index2 < at4 + count2) {
			const id = follow_id(storage, watch, holds, index2);
			__insert_at(held.v, index2, id, "std/src/reactive/lens.vl:1851:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change, _stage) => {
		const $ch = change;
		let $ci = null;
		if ($ch[0] === 0) {
			const at4 = $ch[1];
			const removed = $ch[2];
			const inserted = $ch[3];
			const gone = leave(at4, removed);
			record(log, [ 0, at4, gone, arrive(at4, inserted) ]);
			$ci = undefined;
		} else if ($ch[0] === 1) {
			const at5 = $ch[1];
			const count2 = $ch[2];
			$ci = record(log, [ 0, at5, [  ], arrive(at5, count2) ]);
		} else if ($ch[0] === 3) {
			const at6 = $ch[1];
			const count3 = $ch[2];
			$ci = record(log, [ 0, at6, leave(at6, count3), [  ] ]);
		} else if ($ch[0] === 5) {
			const from = $ch[1];
			const to = $ch[2];
			const id = __remove_at(held.v, from, "std/src/reactive/lens.vl:1867:28");
			__insert_at(held.v, to, id, "std/src/reactive/lens.vl:1868:19");
			record(log, [ 3, from, 1, to ]);
			$ci = undefined;
		} else if ($ch[0] === 6) {
			for (const id2 of held.v) {
				release_id(watch, id2);
			}
			held.v = fill();
			record(log, [ 2, __clone(held.v) ]);
			$ci = undefined;
		} else {
			$ci = undefined;
		}
		return $ci;
	});
	const feed = [ __clone(self), held, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed);
	}, $cg);
	return feed;
}
function cursor3(self) {
	return cursor(self[2]);
}
function get4(self, key) {
	return __nmap_get(self[0], hash(key));
}
function insert2(self, key, value) {
	__nmap_insert(self[0], hash(key), __clone(key), __clone(value));
}
function remove(self, key) {
	__nmap_remove(self[0], hash(key));
}
function entries(self) {
	let keys = __nmap_keys(self[0]);
	let values = __nmap_values(self[0]);
	let backwards = [  ];
	while (!(is_empty(keys))) {
		const $cq = __list_pop(keys);
		let $cr = null;
		if ($cq[0] === 0) {
			const key = $cq[1];
			const $cs = __list_pop(values);
			let $ct = null;
			if ($cs[0] === 0) {
				const value = $cs[1];
				$ct = backwards.push([ key, value ]);
			} else {
				$ct = undefined;
			}
			$cr = $ct;
		} else {
			$cr = undefined;
		}
		$cr;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $cu = __list_pop(backwards);
		let $cv = null;
		if ($cu[0] === 0) {
			const pair = $cu[1];
			$cv = result.push(pair);
		} else {
			$cv = undefined;
		}
		$cv;
	}
	return result;
}
function new9() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function release6(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $cG = _;
		const _key = $cG[0];
		const id = $cG[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new9();
}
function index_by(self, key, $cj) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view = self[0].v[1][2];
	const key_at = (at4) => {
		let out = [ 1 ];
		peek_storage2(storage, (held) => {
			return view(held, at4, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map4 = (held, at4) => {
		const id = follow_id(storage, watch, holds, at4);
		const before = get4(found.v, held);
		insert2(found.v, held, id);
		const $co = before;
		if ($co[0] === 0) {
			release_id(watch, $co[1]);
		}
		return;
	};
	const unmap = (held) => {
		const $cp = get4(found.v, held);
		if ($cp[0] === 0) {
			remove(found.v, held);
			release_id(watch, $cp[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $cw = _;
			const _held = $cw[0];
			const old = $cw[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at4 = 0;
		const count2 = peek_storage2(storage, (held) => {
			return count(held);
		});
		while (at4 < count2) {
			const $cx = key_at(at4);
			if ($cx[0] === 0) {
				map4($cx[1], at4);
			}
			at4 = at4 + 1;
		}
		return;
	};
	fill();
	const note = (at4, count2) => {
		let gone = [  ];
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $cy = key_at(index3);
			if ($cy[0] === 0) {
				gone.push($cy[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count2) => {
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $cz = key_at(index3);
			if ($cz[0] === 0) {
				map4($cz[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change) => {
		const $cA = change;
		let $cB = null;
		if ($cA[0] === 0) {
			const at4 = $cA[1];
			const removed = $cA[2];
			const _inserted = $cA[3];
			$cB = note(at4, removed);
		} else if ($cA[0] === 3) {
			const at5 = $cA[1];
			const count2 = $cA[2];
			$cB = note(at5, count2);
		} else if ($cA[0] === 2) {
			const at6 = $cA[1];
			$cB = note(at6, 1);
		} else if ($cA[0] === 4) {
			const at7 = $cA[1];
			const _part = $cA[2];
			$cB = note(at7, 1);
		} else {
			$cB = undefined;
		}
		return $cB;
	}, (change, stage) => {
		const $cC = stage;
		let $cD = null;
		if ($cC[0] === 0) {
			$cD = undefined;
		} else if ($cC[0] === 3) {
			return;
		} else {
			fill();
			return;
			$cD = undefined;
		}
		$cD;
		const $cE = change;
		let $cF = null;
		if ($cE[0] === 0) {
			const at4 = $cE[1];
			const _removed = $cE[2];
			const inserted = $cE[3];
			for (const held of leaving.v) {
				unmap(held);
			}
			arrive(at4, inserted);
			$cF = undefined;
		} else if ($cE[0] === 1) {
			const at5 = $cE[1];
			const count2 = $cE[2];
			$cF = arrive(at5, count2);
		} else if ($cE[0] === 3) {
			const _at = $cE[1];
			const _count = $cE[2];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			$cF = undefined;
		} else if ($cE[0] === 2) {
			const at6 = $cE[1];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at6, 1);
			$cF = undefined;
		} else if ($cE[0] === 4) {
			const at7 = $cE[1];
			const _part = $cE[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at7, 1);
			$cF = undefined;
		} else if ($cE[0] === 5) {
			const _from = $cE[1];
			const _to = $cE[2];
			$cF = undefined;
		} else {
			$cF = fill();
		}
		return $cF;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index2);
	}, $cj);
	return index2;
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get5(self) {
	return handles(self, self[1].v);
}
function mutate(self, change) {
	const $cP = self[0].v[0];
	let $cQ = null;
	if ($cP[0] === 0) {
		const cell = $cP[1];
		$cQ = change(cell.v);
	} else {
		const port = __clone($cP[1]);
		$cQ = port[1](change);
	}
	return $cQ;
}
function audit(self, before, changes) {
	const $cT = self[0].v[6];
	if ($cT[0] === 0) {

	} else {
		return;
	}
	const $cU = before;
	if ($cU[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held) => {
		return $cT[1]($cU[1], held, changes);
	});
	const $cZ = problem;
	if ($cZ[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $cZ[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function wake_all(watch, slot2, $dA) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1120:36"));
	});
	const $dB = held[0];
	if ($dB[0] === 0) {
		notify($dB[1], $dA);
	}
	for (const part2 of held[1]) {
		notify(part2[1], $dA);
	}
}
function wake_part(watch, slot2, name, $dC) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1130:36"));
	});
	const $dD = held[0];
	if ($dD[0] === 0) {
		notify($dD[1], $dC);
	}
	for (const part2 of held[1]) {
		if (part2[0] === name) {
			notify(part2[1], $dC);
		}
	}
}
function batch(body, $dL) {
	const $dM = $dL;
	let $dN = null;
	if ($dM[0] === 0) {
		const current = $dM[1];
		$dN = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$dN = result;
	}
	return $dN;
}
function landed(self, changes, late, $da) {
	if (changes.length === 0) {
		return;
	}
	const watch = self[0].v[3];
	let wakes = [  ];
	let shaped = false;
	let resized = false;
	const several = changes.length > 1;
	let index2 = 0;
	for (const change of changes) {
		const $db = change;
		let $dc = null;
		if ($db[0] === 0) {
			const _at = $db[1];
			const removed = $db[2];
			const inserted = $db[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$dc = undefined;
		} else if ($db[0] === 1) {
			const _at2 = $db[1];
			const _count = $db[2];
			$dc = resized = true;
		} else if ($db[0] === 3) {
			const _at3 = $db[1];
			const _count2 = $db[2];
			$dc = resized = true;
		} else if ($db[0] === 6) {
			$dc = resized = true;
		} else if ($db[0] === 5) {
			const _from = $db[1];
			const _to = $db[2];
			$dc = shaped = true;
		} else {
			$dc = undefined;
		}
		$dc;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change, wakes);
		}
		let $ds = null;
		if (self[0].v[4].length > 0) {
			let $dr = null;
			if (!(late)) {
				$dr = [ 0 ];
			} else if (!(several)) {
				$dr = [ 1 ];
			} else if (index2 === 0) {
				$dr = [ 2 ];
			} else {
				$dr = [ 3 ];
			}
			const stage = $dr;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change, stage);
			}
			$ds = undefined;
		}
		$ds;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $dt = null;
	if (shaped || resized) {
		$dt = watch.v[2];
	} else {
		$dt = [ 1 ];
	}
	const shape = __clone($dt);
	let $du = null;
	if (resized) {
		$du = watch.v[3];
	} else {
		$du = [ 1 ];
	}
	const sized = __clone($du);
	let $dv = undefined;
	let $dw = undefined;
	if (wakes.length === 0 && (($dv = shape), $dv[0] === 1) && (($dw = sized), $dw[0] === 1)) {
		notify(published, $da);
		return;
	}
	settle_together(($dx) => {
		for (const wake2 of wakes) {
			const $dy = wake2;
			let $dz = null;
			if ($dy[0] === 0) {
				const slot2 = $dy[1];
				$dz = wake_all(watch, slot2, [ 0, $dx ]);
			} else if ($dy[0] === 2) {
				const slot3 = $dy[1];
				$dz = wake_all(watch, slot3, [ 0, $dx ]);
			} else {
				const slot4 = $dy[1];
				const part2 = $dy[2];
				$dz = wake_part(watch, slot4, part2, [ 0, $dx ]);
			}
			$dz;
		}
		const $dE = sized;
		if ($dE[0] === 0) {
			notify($dE[1], [ 0, $dx ]);
		}
		const $dF = shape;
		if ($dF[0] === 0) {
			notify($dF[1], [ 0, $dx ]);
		}
		notify(published, [ 0, $dx ]);
		return;
	}, $da);
}
function update(self, change, $cI) {
	const $cJ = self[0].v[6];
	let $cO = null;
	if ($cJ[0] === 0) {
		$cO = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$cO = [ 1 ];
	}
	const before = $cO;
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, change(held) ];
	});
	const $cR = out;
	let $cS = null;
	if ($cR[0] === 0) {
		const result = __clone($cR[1][0]);
		const changes = __clone($cR[1][1]);
		audit(self, before, changes);
		landed(self, changes, true, $cI);
		$cS = result;
	} else {
		$cS = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $cS;
}
function eq4(self, b) {
	const $dO = self;
	let $dR = null;
	if ($dO[0] === 0) {
		const $dP = b;
		let $dQ = null;
		if ($dP[0] === 0) {
			$dQ = eq2($dO[1], $dP[1]);
		} else {
			$dQ = false;
		}
		$dR = $dQ;
	} else {
		const $dS = b;
		$dR = $dS[0] === 1;
	}
	return $dR;
}
function get6(self) {
	return __clone(self[0].v);
}
function get7(self) {
	return get6(self[0]);
}
function lift(self, op) {
	const $dV = op;
	let $dW = null;
	if ($dV[0] === 0) {
		const at4 = $dV[1];
		const gone = $dV[2];
		const arrived = $dV[3];
		$dW = [ 0, at4, handles(self, gone), handles(self, arrived) ];
	} else if ($dV[0] === 1) {
		const at5 = $dV[1];
		const old = $dV[2];
		const new10 = $dV[3];
		$dW = [ 1, at5, slot_of(self[0], old), slot_of(self[0], new10) ];
	} else if ($dV[0] === 2) {
		const all3 = $dV[1];
		$dW = [ 2, handles(self, all3) ];
	} else {
		const from = $dV[1];
		const count2 = $dV[2];
		const to = $dV[3];
		$dW = [ 3, from, count2, to ];
	}
	return $dW;
}
function since3(self, cursor4) {
	const $dT = since(self[2], cursor4);
	let $dU = null;
	if ($dT[0] === 0) {
		const recorded = $dT[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$dU = out;
	} else {
		$dU = [ [ 2, get5(self) ] ];
	}
	return $dU;
}
function map3(self, fn) {
	const $dY = self;
	let $dZ = null;
	if ($dY[0] === 0) {
		const x = $dY[1];
		$dZ = [ 0, fn(x) ];
	} else {
		$dZ = [ 1 ];
	}
	return $dZ;
}
function find(self, key) {
	const lens = __clone(self[0]);
	return map3(viewing2(self[1], (held) => {
		return get4(held, key);
	}), (id) => {
		return slot_of(lens, id);
	});
}
function eq5(self, b) {
	const $ec = self;
	let $ef = null;
	if ($ec[0] === 0) {
		const $ed = b;
		let $ee = null;
		if ($ed[0] === 0) {
			$ee = eq4($ec[1], $ed[1]);
		} else {
			$ee = false;
		}
		$ef = $ee;
	} else {
		const $eg = b;
		$ef = $eg[0] === 1;
	}
	return $ef;
}
function unwrap_or(self, fallback) {
	const $el = self;
	let $em = null;
	if ($el[0] === 0) {
		const x = $el[1];
		$em = x;
	} else {
		$em = __clone(__force(fallback));
	}
	return $em;
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
function landed_one(self, change, $eq) {
	landed(self, [ __clone(change) ], false, $eq);
}
function plan(self, change, write, $en) {
	const $eo = self[0].v[6];
	let $ep = null;
	if ($eo[0] === 0) {
		$ep = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$ep = [ 1 ];
	}
	const before = $ep;
	coming_one(self, change);
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, write(held) ];
	});
	audit(self, before, [ __clone(change) ]);
	landed_one(self, change, $en);
	const $er = out;
	let $es = null;
	if ($er[0] === 0) {
		const result = __clone($er[1]);
		$es = result;
	} else {
		$es = (() => {
			throw __panic("lens: a planned write never ran", "std/src/reactive/lens.vl:796:12");
		})();
	}
	return $es;
}
function eq6(self, other) {
	return self[1] === other[1];
}
function eq7(self, b) {
	const $et = self;
	let $ew = null;
	if ($et[0] === 0) {
		const $eu = b;
		let $ev = null;
		if ($eu[0] === 0) {
			$ev = eq6($et[1], $eu[1]);
		} else {
			$ev = false;
		}
		$ew = $ev;
	} else {
		const $ex = b;
		$ew = $ex[0] === 1;
	}
	return $ew;
}
function part_cell(watch, slot2, name) {
	if (slot2 < 0) {
		return new7(0, "std/src/reactive/lens.vl:1097:19");
	}
	const found = viewing2(watch, (held) => {
		let out = [ 1 ];
		for (const part2 of __at(held[1], as_usize(slot2), "std/src/reactive/lens.vl:1101:15")[1]) {
			if (part2[0] === name) {
				out = [ 0, __clone(part2[1]) ];
			}
		}
		return out;
	});
	const $eD = found;
	if ($eD[0] === 0) {
		return $eD[1];
	}
	const woke = new7(0, "std/src/reactive/lens.vl:1111:25");
	const entry = as_usize(slot2);
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1113:2")[1].push([ name, __clone(woke) ]);
	return woke;
}
function part(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function get8(self) {
	const $eE = at(self[0]);
	if ($eE[0] === 0) {

	} else {
		return [ 1 ];
	}
	const view = self[0][0][0].v[1][2];
	const read = self[2];
	let out = [ 1 ];
	peek2(self[0][0], (held) => {
		return view(held, $eE[1], (item2) => {
			return out = [ 0, read(item2) ];
		});
	});
	return out;
}
function indexed2(value) {
	return [ indexed_lens(value) ];
}
function length2(self) {
	return length(self[0]);
}
function all2(self) {
	return all(self[0]);
}
function rows2(self, $eF) {
	return rows(self[0], $eF);
}
function update2(self, change, $eG) {
	return update(self[0], change, $eG);
}
function tapped(self) {
	return self[0].v[4].length;
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
const index = indexed([ [ 10, 20, 30, 40 ] ]);
const thirty = slot(index, 2, [ 1 ]);
const thirty_runs = __shared_new(0);
on_change(__clone(thirty), (_v, $q) => {
	const $r = [ thirty_runs, "v" ];
	return thirty_runs.v = $r[0][$r[1]] + 1;
}, "lens-sorted.vl:92:18");
const lengths = __shared_new(0);
on_change2(length(index), (_n, $y) => {
	const $z = [ lengths, "v" ];
	return lengths.v = $z[0][$z[1]] + 1;
}, "lens-sorted.vl:94:26");
const calls = __shared_new(0);
const doubled = memo(map(items(index, [ 1 ]), (value, $af, $ag) => {
	const $ah = [ calls, "v" ];
	calls.v = $ah[0][$ah[1]] + 1;
	return value * 2;
}), [ 1 ], [ 1 ], "lens-sorted.vl:102:4");
const rows3 = rows(index, [ 1 ]);
const row_cursor = cursor3(rows3);
const by_value = index_by(index, (value) => {
	return value[0][value[1]];
}, [ 1 ]);
assert(calls.v === 4 && get5(rows3).length === 4, "the feeds seeded four", "lens-sorted.vl:106:2");
update(index, (held) => {
	return [ undefined, [ insert(held, 25) ] ];
}, [ 1 ]);
assert(eq(all(index), [ 10, 20, 25, 30, 40 ]), "25 landed in order", "lens-sorted.vl:111:2");
assert(eq4(at(thirty), [ 0, 3 ]) && thirty_runs.v === 0, "the slot on 30 moved to 3 silently", "lens-sorted.vl:112:2");
assert(lengths.v === 1 && get2(length(index)) === 5, "the length woke once", "lens-sorted.vl:113:2");
assert(calls.v === 5 && eq(get7(doubled), [ 20, 40, 50, 60, 80 ]), "the map ran for 25 alone", "lens-sorted.vl:114:2");
const row_ops = since3(rows3, row_cursor);
let $dX = undefined;
assert(row_ops.length === 1 && (($dX = __at(row_ops, 0, "lens-sorted.vl:118:7")), $dX[0] === 0) && $dX[1] === 2 && $dX[3].length === 1, "one row arrived at 2", "lens-sorted.vl:116:2");
assert(eq5(map3(find(by_value, 25), (slot2) => {
	return at(slot2);
}), [ 0, [ 0, 2 ] ]), "the index found 25 at 2", "lens-sorted.vl:123:2");
assert(eq5(map3(find(by_value, 30), (slot2) => {
	return at(slot2);
}), [ 0, [ 0, 3 ] ]), "and 30 at 3", "lens-sorted.vl:124:2");
const before_delete = calls.v;
const twenty = unwrap_or(peek2(index, (held) => {
	return position(held, 20);
}), __lazy("fallback", () => {
	return 99;
}));
plan(index, [ 3, twenty, 1 ], (held) => {
	return remove_at(held, twenty);
}, [ 1 ]);
assert(eq(all(index), [ 10, 25, 30, 40 ]), "20 left", "lens-sorted.vl:131:2");
assert(calls.v === before_delete && eq(get7(doubled), [ 20, 50, 60, 80 ]), "the map dropped 40 without a run", "lens-sorted.vl:132:2");
assert(eq7(find(by_value, 20), [ 1 ]) && eq5(map3(find(by_value, 40), (slot2) => {
	return at(slot2);
}), [ 0, [ 0, 3 ] ]), "the index let 20 go", "lens-sorted.vl:136:2");
const at3 = unwrap_or(at(thirty), __lazy("fallback", () => {
	return 99;
}));
update(index, (held) => {
	return [ undefined, reprice(held, at3, 5) ];
}, [ 1 ]);
assert(eq(all(index), [ 5, 10, 25, 40 ]) && ordered(all(index)), "repriced and still in order", "lens-sorted.vl:144:2");
assert(eq4(at(thirty), [ 0, 0 ]) && eq3(get(thirty), [ 0, 5 ]) && thirty_runs.v === 1, "the slot followed its item and woke once", "lens-sorted.vl:145:2");
assert(eq(get7(doubled), [ 10, 20, 50, 80 ]), "the map is right (two changes in one update reach it as one reset)", "lens-sorted.vl:149:2");
assert(eq6(__at(get5(rows3), 0, "lens-sorted.vl:153:9"), thirty), "the row followed its item to the front", "lens-sorted.vl:153:2");
assert(eq5(map3(find(by_value, 5), (slot2) => {
	return at(slot2);
}), [ 0, [ 0, 0 ] ]) && eq7(find(by_value, 30), [ 1 ]), "the index re-keyed it", "lens-sorted.vl:154:2");
const first_digit = part(slot(index, 3, [ 1 ]), "tens", (value) => {
	return Math.trunc(value[0][value[1]] / 10);
});
assert(eq3(get8(first_digit), [ 0, 4 ]), "a part reads through the read-only shape", "lens-sorted.vl:161:2");
const wrapped = indexed2([ [ 1, 3 ] ]);
assert(get2(length2(wrapped)) === 2 && eq(all2(wrapped), [ 1, 3 ]), "the wrapper reads", "lens-sorted.vl:165:2");
const wrapped_rows = rows2(wrapped, [ 1 ]);
update2(wrapped, (held) => {
	return [ undefined, [ insert(held, 2) ] ];
}, [ 1 ]);
assert(get5(wrapped_rows).length === 3, "the wrapper\'s rows follow an update", "lens-sorted.vl:168:2");
release5(wrapped_rows);
release5(rows3);
release6(by_value);
assert(tapped(index) === 1, "released, the rows and the index let their taps go; the value feed keeps its own", "lens-sorted.vl:172:2");
console.log("sorted: all passed");
//# sourceMappingURL=lens-sorted.mjs.map
