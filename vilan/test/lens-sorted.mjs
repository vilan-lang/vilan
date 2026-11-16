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
	let $cI = null;
	if (!(condition)) {
		$cI = (() => {
			throw __panic(message, caller);
		})();
	}
	return $cI;
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
	const $ez = self;
	let $eC = null;
	if ($ez[0] === 0) {
		const $eA = b;
		let $eB = null;
		if ($eA[0] === 0) {
			$eB = $ez[1] === $eA[1];
		} else {
			$eB = false;
		}
		$eC = $eB;
	} else {
		const $eD = b;
		$eC = $eD[0] === 1;
	}
	return $eC;
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
		let $aH = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$aH = undefined;
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
		$aH;
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
	const $aF = turn;
	let $aG = null;
	if ($aF[0] === 0) {
		const ambient = $aF[1];
		$aG = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aI = last(draining_turns.v);
		let $aJ = null;
		if ($aI[0] === 0) {
			const draining = $aI[1];
			$aJ = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aJ = undefined;
		}
		$aG = $aJ;
	}
	return $aG;
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
function dispose(self, $ce) {
	const $cf = $ce;
	let $cg = null;
	if ($cf[0] === 0) {
		const established = $cf[1];
		$cg = [ 0, established ];
	} else {
		$cg = last(draining_turns.v);
	}
	const ambient = $cg;
	release_under(self, ambient);
}
function detach(handle) {
	const $aU = last2(releasing_turns.v);
	let $aV = null;
	if ($aU[0] === 0) {
		const at_release = $aU[1];
		$aV = at_release;
	} else {
		$aV = last(draining_turns.v);
	}
	const turn = $aV;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aW = [ 0, handle[0] ];
	let $aX = null;
	if ($aW[0] === 0) {
		const subscribers = $aW[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aX = undefined;
	} else {
		$aX = undefined;
	}
	$aX;
	const $aY = ambient;
	let $aZ = null;
	if ($aY[0] === 0) {
		const turn = $aY[1];
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
		$aZ = undefined;
	} else {
		$aZ = undefined;
	}
	$aZ;
	const $ba = handle[3].v;
	let $bb = null;
	if ($ba[0] === 0) {
		const release7 = $ba[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release7, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$bb = undefined;
	} else {
		$bb = undefined;
	}
	return $bb;
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
	const $as = nursery(self);
	let $at = null;
	if ($as[0] === 0) {
		const held = $as[1];
		$at = held;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$at = made;
	}
	return $at;
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
	let $ar = null;
	if (is_disposed(self)) {
		$ar = [ 1 ];
	} else {
		$ar = self[0].v[3];
	}
	return $ar;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $bn = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $be = held[3];
		let $bf = null;
		if ($be[0] === 0) {
			const nursery2 = $be[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$bf = undefined;
		} else {
			$bf = undefined;
		}
		$bf;
		let $bm = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $bh = __guarded(cleanup);
				let $bi = null;
				if ($bh[0] === 0) {
					const message = $bh[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$bi = undefined;
				} else {
					$bi = undefined;
				}
				$bi;
			}
			const $bk = failure;
			let $bl = null;
			if ($bk[0] === 0) {
				const message2 = $bk[1];
				$bl = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$bl = undefined;
			}
			$bm = $bl;
		}
		$bn = $bm;
	}
	return $bn;
}
function register_with_owner(subscription, $bZ, $ca) {
	const $cb = $ca;
	let $cc = null;
	if ($cb[0] === 0) {
		const owner = $cb[1];
		$cc = take(owner, subscription, $bZ);
	} else {
		$cc = __clone(subscription);
	}
	return $cc;
}
function defer_to_owner(cleanup, $n) {
	const $o = $n;
	let $p = null;
	if ($o[0] === 0) {
		const owner = $o[1];
		$p = defer(owner, cleanup);
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
function also_releasing(handle, release7) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release7();
		const $bX = previous;
		let $bY = null;
		if ($bX[0] === 0) {
			const earlier = $bX[1];
			$bY = earlier();
		} else {
			$bY = undefined;
		}
		return $bY;
	} ];
}
function settle_together(body, $dH) {
	const $dI = $dH;
	let $dJ = null;
	if ($dI[0] === 0) {
		const current = $dI[1];
		$dJ = body(current);
	} else {
		const $dK = last(draining_turns.v);
		let $dL = null;
		if ($dK[0] === 0) {
			const draining = $dK[1];
			$dL = body(draining);
		} else {
			$dL = batch(body, $dH);
		}
		$dJ = $dL;
	}
	return $dJ;
}
function splice_start(held, at4) {
	let $aN = null;
	if (at4 > held) {
		$aN = held;
	} else {
		$aN = at4;
	}
	return $aN;
}
function splice_count(held, start, removed) {
	let $aO = null;
	if (start + removed > held) {
		$aO = held - start;
	} else {
		$aO = removed;
	}
	return $aO;
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
	const $di = seat(self, at4);
	const place2 = $di[0];
	const index2 = $di[1];
	const block = __at(self[1], place2, "std/src/reactive/lens.vl:265:15");
	let $dj = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index2, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$dj = undefined;
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
	$dj;
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
		const $dg = seat(self, at4);
		const place2 = $dg[0];
		const index2 = $dg[1];
		const block = __at(self[1], place2, "std/src/reactive/lens.vl:306:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $dh = null;
		if (left < held - index2) {
			$dh = left;
		} else {
			$dh = held - index2;
		}
		const take2 = $dh;
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
	const $de = change;
	let $df = null;
	if ($de[0] === 0) {
		const at4 = $de[1];
		const removed = $de[2];
		const inserted = $de[3];
		close(self, at4, removed, wakes);
		widen(self, at4, inserted);
		$df = undefined;
	} else if ($de[0] === 1) {
		const at5 = $de[1];
		const count2 = $de[2];
		$df = widen(self, at5, count2);
	} else if ($de[0] === 3) {
		const at6 = $de[1];
		const count3 = $de[2];
		$df = close(self, at6, count3, wakes);
	} else if ($de[0] === 2) {
		const at7 = $de[1];
		let $dl = null;
		if (at7 < self[6]) {
			const $dk = seat(self, at7);
			const place2 = $dk[0];
			const index2 = $dk[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index2, "std/src/reactive/lens.vl:396:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$dl = undefined;
		}
		$df = $dl;
	} else if ($de[0] === 4) {
		const at8 = $de[1];
		const part2 = $de[2];
		let $dn = null;
		if (at8 < self[6]) {
			const $dm = seat(self, at8);
			const place3 = $dm[0];
			const index3 = $dm[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place3, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index3, "std/src/reactive/lens.vl:405:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part2 ]);
			}
			$dn = undefined;
		}
		$df = $dn;
	} else if ($de[0] === 5) {
		const from = $de[1];
		const to = $de[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $do = null;
		if (from > to) {
			$do = from;
		} else {
			$do = to;
		}
		const far = $do;
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
		const $dp = seat(self, from);
		const left = $dp[0];
		const index4 = $dp[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index4, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index4);
		}
		restart(self);
		let $dq = null;
		if (to >= self[6]) {
			const last4 = self[1].length - 1;
			$dq = [ last4, __at(self[0], as_usize(__at(self[1], last4, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$dq = seat(self, to);
		}
		const $dr = $dq;
		const place4 = $dr[0];
		const landing = $dr[1];
		const target = __at(self[1], place4, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot4, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$df = undefined;
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
		$df = undefined;
	}
	return $df;
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
		throw __panic("lens: this sequence is read-only (`Indexed`): write it through `update` or `plan`", "std/src/reactive/lens.vl:1539:2");
	})();
}
function extend_run(runs, from, to) {
	const held = runs.v.length;
	let $S = null;
	if (held > 0) {
		const $R = __at(runs.v, held - 1, "std/src/reactive/lens.vl:1989:31");
		const start = $R[0];
		const count2 = $R[1];
		const first = $R[2];
		const leftward = first < start && from === start + count2 && to === first + count2;
		const rightward = first > start && from === start && to === first;
		if (leftward || rightward) {
			__at_put(runs.v, held - 1, [ start, count2 + 1, first ], "std/src/reactive/lens.vl:1993:4");
			return;
		}
		$S = undefined;
	}
	$S;
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
	return viewing2(self[0][0].v[3], (held) => {
		return locate(held[0], self[1]);
	});
}
function peek_storage2(storage, read) {
	const $u = storage;
	let $v = null;
	if ($u[0] === 0) {
		const cell = $u[1];
		$v = viewing2(cell, read);
	} else {
		const port = $u[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $w = out;
		let $x = null;
		if ($w[0] === 0) {
			const value = __clone($w[1]);
			$x = value;
		} else {
			$x = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$v = $x;
	}
	return $v;
}
function peek(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function get(self) {
	const $t = at(self);
	if ($t[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek(self[0], (held) => {
		return read(held, $t[1]);
	});
}
function slot_cell(watch, slot2) {
	if (slot2 < 0) {
		return new7(0, "std/src/reactive/lens.vl:1079:19");
	}
	const entry = as_usize(slot2);
	const $y = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1082:5")[0];
	const cell = __clone($y[1]);
	if ($y[0] === 0) {
		return cell;
	}
	const cell2 = new7(0, "std/src/reactive/lens.vl:1085:25");
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
	const $F = watch.v[3];
	const cell = __clone($F[1]);
	if ($F[0] === 0) {
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
			const $K = item(held, index2);
			if ($K[0] === 0) {
				arrived.push($K[1]);
			}
			index2 = index2 + 1;
		}
		return arrived;
	});
}
function one(item2) {
	const $P = item2;
	let $Q = null;
	if ($P[0] === 0) {
		const value = $P[1];
		$Q = [ __clone(value) ];
	} else {
		$Q = [  ];
	}
	return $Q;
}
function all_of(storage) {
	return peek_storage2(storage, (held) => {
		let out = [  ];
		let at4 = 0;
		const count2 = count(held);
		while (at4 < count2) {
			const $V = item(held, at4);
			if ($V[0] === 0) {
				out.push($V[1]);
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
			const $W = __list_get(held, index2);
			let $X = null;
			if ($W[0] === 0) {
				const op = $W[1];
				$X = kept.push(op);
			} else {
				$X = undefined;
			}
			$X;
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
	const $ab = before;
	let $ac = undefined;
	if ($ab[0] === 0 && (($ac = after), $ac[0] === 0)) {
		record(log, [ 1, at4, __clone($ab[1]), __clone($ac[1]) ]);
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
function items_into(self, log, $H) {
	const leaving = __shared_new([  ]);
	const runs = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0].v[0]);
	const tap2 = tap(self, (change) => {
		const $I = change;
		let $J = null;
		if ($I[0] === 0) {
			const at4 = $I[1];
			const removed = $I[2];
			const _inserted = $I[3];
			leaving.v = arrived_at(storage, at4, removed);
			$J = undefined;
		} else if ($I[0] === 3) {
			const at5 = $I[1];
			const count2 = $I[2];
			leaving.v = arrived_at(storage, at5, count2);
			$J = undefined;
		} else if ($I[0] === 2) {
			const at6 = $I[1];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at6);
			}));
			$J = undefined;
		} else if ($I[0] === 4) {
			const at7 = $I[1];
			const _part = $I[2];
			leaving.v = one(peek_storage2(storage, (held) => {
				return item(held, at7);
			}));
			$J = undefined;
		} else if ($I[0] === 5) {
			const from = $I[1];
			const to = $I[2];
			$J = extend_run(runs, from, to);
		} else {
			$J = undefined;
		}
		return $J;
	}, (change, stage) => {
		const $T = stage;
		let $U = null;
		if ($T[0] === 2) {
			record(log, [ 2, all_of(storage) ]);
			return;
			$U = undefined;
		} else if ($T[0] === 3) {
			return;
		} else {
			$U = undefined;
		}
		$U;
		const $Y = stage;
		const late = $Y[0] === 1;
		const $Z = change;
		let $aa = null;
		if ($Z[0] === 0) {
			const at4 = $Z[1];
			const removed = $Z[2];
			const inserted = $Z[3];
			if (late && removed > 0) {
				record(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record(log, [ 0, at4, [  ], arrived_at(storage, at4, inserted) ]);
			} else {
				record(log, [ 0, at4, __clone(leaving.v), arrived_at(storage, at4, inserted) ]);
			}
			$aa = undefined;
		} else if ($Z[0] === 1) {
			const at5 = $Z[1];
			const count2 = $Z[2];
			record(log, [ 0, at5, [  ], arrived_at(storage, at5, count2) ]);
			$aa = undefined;
		} else if ($Z[0] === 3) {
			const at6 = $Z[1];
			const _count = $Z[2];
			if (late) {
				record(log, [ 2, all_of(storage) ]);
			} else {
				record(log, [ 0, at6, __clone(leaving.v), [  ] ]);
			}
			$aa = undefined;
		} else if ($Z[0] === 2) {
			const at7 = $Z[1];
			$aa = record_set(log, storage, leaving, at7, late);
		} else if ($Z[0] === 4) {
			const at8 = $Z[1];
			const _part = $Z[2];
			$aa = record_set(log, storage, leaving, at8, late);
		} else if ($Z[0] === 5) {
			const from = $Z[1];
			const to = $Z[2];
			if (skipping.v > 0) {
				const $ad = [ skipping, "v" ];
				skipping.v = $ad[0][$ad[1]] - 1;
				return;
			}
			if (runs.v.length === 0) {
				record(log, [ 3, from, 1, to ]);
				return;
			}
			const $ae = __remove_at(runs.v, 0, "std/src/reactive/lens.vl:1676:47");
			const start = $ae[0];
			const count3 = $ae[1];
			const first = $ae[2];
			skipping.v = count3 - 1;
			let $af = null;
			if (first > start) {
				$af = first + 1 - count3;
			} else {
				$af = first;
			}
			const landing = $af;
			record(log, [ 3, start, count3, landing ]);
			$aa = undefined;
		} else {
			$aa = record(log, [ 2, all_of(storage) ]);
		}
		return $aa;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $H);
	return feed;
}
function items(self, $G) {
	return items_into(self, new8(), $G);
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
		const $al = __list_get(held, index2);
		let $am = null;
		if ($al[0] === 0) {
			const op = $al[1];
			$am = ops.push(op);
		} else {
			$am = undefined;
		}
		$am;
		index2 = index2 + 1;
	}
	return [ 0, ops ];
}
function all(self) {
	return all_of(self[0].v[0]);
}
function since2(self, cursor4) {
	const $an = since(self[1], cursor4);
	let $ao = null;
	if ($an[0] === 0) {
		const recorded = $an[1];
		$ao = recorded;
	} else {
		$ao = [ [ 2, all(self[0]) ] ];
	}
	return $ao;
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
	const value = (($au) => {
		return (($av) => {
			return body($au, $av);
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
	let $aE = null;
	if (is_empty(self)) {
		$aE = [ 1 ];
	} else {
		$aE = __list_get(self, self.length - 1);
	}
	return $aE;
}
function admit(core, item2) {
	const id = core[5].v;
	core[5].v = id + 1;
	const $ax = core[8](item2);
	const answer = $ax[0];
	const owner = $ax[1];
	const $ay = answer;
	let $az = null;
	if ($ay[0] === 0) {
		const value = $ay[1];
		const $aA = owner;
		let $aB = null;
		if ($aA[0] === 0) {
			const held = $aA[1];
			$aB = keep_hold(core, id, [ false, () => {
				return __clone(value);
			}, [ 1 ], () => {
				return;
			}, [ 0, __clone(held) ] ]);
		} else {
			$aB = undefined;
		}
		$aB;
		$az = [ id, __clone(value) ];
	} else {
		const instance = $ay[1];
		const pull = instance[0];
		const dirty = core[4];
		const consumer = core[7];
		const handle = instance[1](subscriber_of(() => {
			dirty.v.push(id);
			const $aC = consumer.v;
			let $aD = null;
			if ($aC[0] === 0) {
				const downstream = $aC[1];
				$aD = wake(downstream);
			} else {
				$aD = undefined;
			}
			return $aD;
		}, true));
		keep_hold(core, id, [ true, pull, [ 0, handle ], instance[2], owner ]);
		$az = [ id, pull() ];
	}
	return $az;
}
function seed(core, items2) {
	for (const item2 of items2) {
		const $aK = admit(core, item2);
		const id = $aK[0];
		const value = $aK[1];
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
	let $aT = null;
	if (is_empty(self)) {
		$aT = [ 1 ];
	} else {
		$aT = __list_get(self, self.length - 1);
	}
	return $aT;
}
function is_none(self) {
	const $bg = self;
	return $bg[0] === 1;
}
function retire(core, id) {
	const $aP = __map_get(core[3].v, hash(id));
	let $aQ = null;
	if ($aP[0] === 0) {
		const hold = $aP[1];
		core[3].v.delete(hash(id));
		const $aR = hold[2];
		let $aS = null;
		if ($aR[0] === 0) {
			const handle = $aR[1];
			$aS = detach(handle);
		} else {
			$aS = undefined;
		}
		$aS;
		hold[3]();
		const $bc = hold[4];
		let $bd = null;
		if ($bc[0] === 0) {
			const owner = $bc[1];
			$bd = dispose2(owner);
		} else {
			$bd = undefined;
		}
		$aQ = $bd;
	} else {
		$aQ = undefined;
	}
	return $aQ;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops) {
	let events = [  ];
	for (const op of ops) {
		const $aL = op;
		let $aM = null;
		if ($aL[0] === 0) {
			const at4 = $aL[1];
			const removed = $aL[2];
			const inserted = $aL[3];
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
				const $bo = admit(core, item2);
				const id2 = $bo[0];
				const value = $bo[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$aM = undefined;
		} else if ($aL[0] === 1) {
			const at5 = $aL[1];
			const _was = $aL[2];
			const item3 = $aL[3];
			const $bp = __list_get(core[0].v, at5);
			let $bq = null;
			if ($bp[0] === 0) {
				const old_id = $bp[1];
				const before = [ __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $br = admit(core, item3);
				const id3 = $br[0];
				const value2 = $br[1];
				__at_put(core[0].v, at5, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at5, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at5, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at5, before, [ __clone(item3), value2 ], true ]);
				$bq = undefined;
			} else {
				$bq = undefined;
			}
			$aM = $bq;
		} else if ($aL[0] === 2) {
			const items2 = $aL[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$aM = undefined;
		} else {
			const from = $aL[1];
			const count2 = $aL[2];
			const to = $aL[3];
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
			$aM = undefined;
		}
		$aM;
	}
	const woke = __clone(core[4].v);
	let $bw = null;
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
			const $bs = __map_get(positions, hash(id6));
			let $bt = null;
			if ($bs[0] === 0) {
				const at6 = $bs[1];
				const $bu = __map_get(core[3].v, hash(id6));
				let $bv = null;
				if ($bu[0] === 0) {
					const hold = $bu[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at6, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at6, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at6, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at6, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$bv = undefined;
				} else {
					$bv = undefined;
				}
				$bt = $bv;
			} else {
				$bt = undefined;
			}
			$bt;
		}
		$bw = undefined;
	}
	$bw;
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
		const $bx = event;
		let $by = null;
		if ($bx[0] === 0) {
			const at4 = $bx[1];
			const left = $bx[2];
			const arrived = $bx[3];
			ops.push([ 0, at4, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$by = undefined;
		} else if ($bx[0] === 1) {
			const at5 = $bx[1];
			const before = $bx[2];
			const after = $bx[3];
			const _rerun = $bx[4];
			ops.push([ 1, at5, __clone(before[1]), __clone(after[1]) ]);
			$by = undefined;
		} else if ($bx[0] === 2) {
			const _left = $bx[1];
			const now = $bx[2];
			$by = ops.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $bx[1];
			const count2 = $bx[2];
			const to = $bx[3];
			$by = ops.push([ 3, from, count2, to ]);
		}
		$by;
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
		const $aw = run_element(stage, ($ap, $aq) => {
			return transform(item2, $ap, $aq);
		});
		const answer = $aw[0];
		const owner = $aw[1];
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
		const $bF = __list_get(inserted, offset);
		let $bG = null;
		if ($bF[0] === 0) {
			const value = $bF[1];
			$bG = __insert_at(list, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$bG = undefined;
		}
		$bG;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at2(self) {
	return self[1].v;
}
function notify(self, $bJ) {
	const traced = tracing.v;
	let $bK = null;
	if (traced) {
		$bK = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$bK = "";
	}
	const cause = $bK;
	const $bL = $bJ;
	let $bM = null;
	if ($bL[0] === 0) {
		const turn = $bL[1];
		$bM = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $bN = last(draining_turns.v);
		let $bO = null;
		if ($bN[0] === 0) {
			const draining = $bN[1];
			$bO = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $bP = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$bP = undefined;
				}
				$bP;
			}
			$bO = undefined;
		}
		$bM = $bO;
	}
	return $bM;
}
function set(self, value, $bI, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $bI);
}
function publish(cell, $bH) {
	set(cell[1], at2(cell[2]), $bH, "std/src/reactive/delta.vl:799:17");
}
function splice(self, at4, removed, inserted, $bE) {
	const held = size(self);
	const start = splice_start(held, at4);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $bE);
}
function set_at(self, at4, value, $bQ) {
	const $bR = __list_get(self[0].v, at4);
	let $bS = null;
	if ($bR[0] === 0) {
		const previous = $bR[1];
		__at_put(self[0].v, at4, __clone(value), "std/src/reactive/delta.vl:855:5");
		record(self[2], [ 1, at4, previous, __clone(value) ]);
		publish(self, $bQ);
		$bS = undefined;
	} else {
		$bS = undefined;
	}
	return $bS;
}
function set2(self, value, $bT, caller) {
	self[0].v = __clone(value);
	record(self[2], [ 2, __clone(value) ]);
	publish(self, $bT);
}
function move_range(self, from, count2, to, $bU) {
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
		const $bV = __list_get(lifted, offset);
		let $bW = null;
		if ($bV[0] === 0) {
			const value = $bV[1];
			$bW = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$bW = undefined;
		}
		$bW;
		offset = offset + 1;
	}
	record(self[2], [ 3, from, count2, to ]);
	publish(self, $bU);
}
function apply_ops(cell, ops, $bB) {
	for (const op of ops) {
		const $bC = op;
		let $bD = null;
		if ($bC[0] === 0) {
			const at4 = $bC[1];
			const removed = $bC[2];
			const inserted = $bC[3];
			$bD = splice(cell, at4, removed.length, inserted, $bB);
		} else if ($bC[0] === 1) {
			const at5 = $bC[1];
			const _was = $bC[2];
			const value = $bC[3];
			$bD = set_at(cell, at5, value, $bB);
		} else if ($bC[0] === 2) {
			const items2 = $bC[1];
			$bD = set2(cell, items2, $bB, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $bC[1];
			const count2 = $bC[2];
			const to = $bC[3];
			$bD = move_range(cell, from, count2, to, $bB);
		}
		$bD;
	}
}
function take(self, item2, $cd) {
	defer(self, () => {
		dispose(item2, $cd);
		return;
	});
	return __clone(item2);
}
function seal_collection(instance, owned, $bz, $bA, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $bz);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $bz, $bA);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $bz);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $aj, $ak, caller) {
	return [ seal_collection(open3(self), true, $aj, $ak, caller) ];
}
function release5(self) {
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $ch) {
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
			const id = __remove_at(held.v, at4, "std/src/reactive/lens.vl:1836:27");
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
			__insert_at(held.v, index2, id, "std/src/reactive/lens.vl:1848:18");
			arrived.push(id);
			index2 = index2 + 1;
		}
		return arrived;
	};
	held.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change, _stage) => {
		const $ci = change;
		let $cj = null;
		if ($ci[0] === 0) {
			const at4 = $ci[1];
			const removed = $ci[2];
			const inserted = $ci[3];
			const gone = leave(at4, removed);
			record(log, [ 0, at4, gone, arrive(at4, inserted) ]);
			$cj = undefined;
		} else if ($ci[0] === 1) {
			const at5 = $ci[1];
			const count2 = $ci[2];
			$cj = record(log, [ 0, at5, [  ], arrive(at5, count2) ]);
		} else if ($ci[0] === 3) {
			const at6 = $ci[1];
			const count3 = $ci[2];
			$cj = record(log, [ 0, at6, leave(at6, count3), [  ] ]);
		} else if ($ci[0] === 5) {
			const from = $ci[1];
			const to = $ci[2];
			const id = __remove_at(held.v, from, "std/src/reactive/lens.vl:1864:28");
			__insert_at(held.v, to, id, "std/src/reactive/lens.vl:1865:19");
			record(log, [ 3, from, 1, to ]);
			$cj = undefined;
		} else if ($ci[0] === 6) {
			for (const id2 of held.v) {
				release_id(watch, id2);
			}
			held.v = fill();
			record(log, [ 2, __clone(held.v) ]);
			$cj = undefined;
		} else {
			$cj = undefined;
		}
		return $cj;
	});
	const feed = [ __clone(self), held, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed);
	}, $ch);
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
		const $cr = __list_pop(keys);
		let $cs = null;
		if ($cr[0] === 0) {
			const key = $cr[1];
			const $ct = __list_pop(values);
			let $cu = null;
			if ($ct[0] === 0) {
				const value = $ct[1];
				$cu = backwards.push([ key, value ]);
			} else {
				$cu = undefined;
			}
			$cs = $cu;
		} else {
			$cs = undefined;
		}
		$cs;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $cv = __list_pop(backwards);
		let $cw = null;
		if ($cv[0] === 0) {
			const pair = $cv[1];
			$cw = result.push(pair);
		} else {
			$cw = undefined;
		}
		$cw;
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
		const $cH = _;
		const _key = $cH[0];
		const id = $cH[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new9();
}
function index_by(self, key, $ck) {
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
		const $cp = before;
		if ($cp[0] === 0) {
			release_id(watch, $cp[1]);
		}
		return;
	};
	const unmap = (held) => {
		const $cq = get4(found.v, held);
		if ($cq[0] === 0) {
			remove(found.v, held);
			release_id(watch, $cq[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $cx = _;
			const _held = $cx[0];
			const old = $cx[1];
			release_id(watch, old);
		}
		found.v = new9();
		let at4 = 0;
		const count2 = peek_storage2(storage, (held) => {
			return count(held);
		});
		while (at4 < count2) {
			const $cy = key_at(at4);
			if ($cy[0] === 0) {
				map4($cy[1], at4);
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
			const $cz = key_at(index3);
			if ($cz[0] === 0) {
				gone.push($cz[1]);
			}
			index3 = index3 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at4, count2) => {
		let index3 = at4;
		while (index3 < at4 + count2) {
			const $cA = key_at(index3);
			if ($cA[0] === 0) {
				map4($cA[1], index3);
			}
			index3 = index3 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change) => {
		const $cB = change;
		let $cC = null;
		if ($cB[0] === 0) {
			const at4 = $cB[1];
			const removed = $cB[2];
			const _inserted = $cB[3];
			$cC = note(at4, removed);
		} else if ($cB[0] === 3) {
			const at5 = $cB[1];
			const count2 = $cB[2];
			$cC = note(at5, count2);
		} else if ($cB[0] === 2) {
			const at6 = $cB[1];
			$cC = note(at6, 1);
		} else if ($cB[0] === 4) {
			const at7 = $cB[1];
			const _part = $cB[2];
			$cC = note(at7, 1);
		} else {
			$cC = undefined;
		}
		return $cC;
	}, (change, stage) => {
		const $cD = stage;
		let $cE = null;
		if ($cD[0] === 0) {
			$cE = undefined;
		} else if ($cD[0] === 3) {
			return;
		} else {
			fill();
			return;
			$cE = undefined;
		}
		$cE;
		const $cF = change;
		let $cG = null;
		if ($cF[0] === 0) {
			const at4 = $cF[1];
			const _removed = $cF[2];
			const inserted = $cF[3];
			for (const held of leaving.v) {
				unmap(held);
			}
			arrive(at4, inserted);
			$cG = undefined;
		} else if ($cF[0] === 1) {
			const at5 = $cF[1];
			const count2 = $cF[2];
			$cG = arrive(at5, count2);
		} else if ($cF[0] === 3) {
			const _at = $cF[1];
			const _count = $cF[2];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			$cG = undefined;
		} else if ($cF[0] === 2) {
			const at6 = $cF[1];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at6, 1);
			$cG = undefined;
		} else if ($cF[0] === 4) {
			const at7 = $cF[1];
			const _part = $cF[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at7, 1);
			$cG = undefined;
		} else if ($cF[0] === 5) {
			const _from = $cF[1];
			const _to = $cF[2];
			$cG = undefined;
		} else {
			$cG = fill();
		}
		return $cG;
	});
	const index2 = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index2);
	}, $ck);
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
	const $cQ = self[0].v[0];
	let $cR = null;
	if ($cQ[0] === 0) {
		const cell = $cQ[1];
		$cR = change(cell.v);
	} else {
		const port = __clone($cQ[1]);
		$cR = port[1](change);
	}
	return $cR;
}
function audit(self, before, changes) {
	const $cU = self[0].v[6];
	const check = $cU[1];
	if ($cU[0] === 0) {

	} else {
		return;
	}
	const $cV = before;
	if ($cV[0] === 0) {

	} else {
		return;
	}
	const problem = peek2(self, (held) => {
		return check($cV[1], held, changes);
	});
	const $da = problem;
	if ($da[0] === 0) {
		(() => {
			throw __panic("lens: the changes do not match the write: " + $da[1], "std/src/reactive/lens.vl:806:4");
		})();
	}
}
function wake_all(watch, slot2, $dB) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1117:36"));
	});
	const $dC = held[0];
	if ($dC[0] === 0) {
		notify($dC[1], $dB);
	}
	for (const part2 of held[1]) {
		notify(part2[1], $dB);
	}
}
function wake_part(watch, slot2, name, $dD) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1127:36"));
	});
	const $dE = held[0];
	if ($dE[0] === 0) {
		notify($dE[1], $dD);
	}
	for (const part2 of held[1]) {
		if (part2[0] === name) {
			notify(part2[1], $dD);
		}
	}
}
function batch(body, $dM) {
	const $dN = $dM;
	let $dO = null;
	if ($dN[0] === 0) {
		const current = $dN[1];
		$dO = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$dO = result;
	}
	return $dO;
}
function landed(self, changes, late, $db) {
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
		const $dc = change;
		let $dd = null;
		if ($dc[0] === 0) {
			const _at = $dc[1];
			const removed = $dc[2];
			const inserted = $dc[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$dd = undefined;
		} else if ($dc[0] === 1) {
			const _at2 = $dc[1];
			const _count = $dc[2];
			$dd = resized = true;
		} else if ($dc[0] === 3) {
			const _at3 = $dc[1];
			const _count2 = $dc[2];
			$dd = resized = true;
		} else if ($dc[0] === 6) {
			$dd = resized = true;
		} else if ($dc[0] === 5) {
			const _from = $dc[1];
			const _to = $dc[2];
			$dd = shaped = true;
		} else {
			$dd = undefined;
		}
		$dd;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change, wakes);
		}
		let $dt = null;
		if (self[0].v[4].length > 0) {
			let $ds = null;
			if (!(late)) {
				$ds = [ 0 ];
			} else if (!(several)) {
				$ds = [ 1 ];
			} else if (index2 === 0) {
				$ds = [ 2 ];
			} else {
				$ds = [ 3 ];
			}
			const stage = $ds;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change, stage);
			}
			$dt = undefined;
		}
		$dt;
		index2 = index2 + 1;
	}
	const published = __clone(self[0].v[2]);
	let $du = null;
	if (shaped || resized) {
		$du = watch.v[2];
	} else {
		$du = [ 1 ];
	}
	const shape = __clone($du);
	let $dv = null;
	if (resized) {
		$dv = watch.v[3];
	} else {
		$dv = [ 1 ];
	}
	const sized = __clone($dv);
	let $dw = undefined;
	let $dx = undefined;
	if (wakes.length === 0 && (($dw = shape), $dw[0] === 1) && (($dx = sized), $dx[0] === 1)) {
		notify(published, $db);
		return;
	}
	settle_together(($dy) => {
		for (const wake2 of wakes) {
			const $dz = wake2;
			let $dA = null;
			if ($dz[0] === 0) {
				const slot2 = $dz[1];
				$dA = wake_all(watch, slot2, [ 0, $dy ]);
			} else if ($dz[0] === 2) {
				const slot3 = $dz[1];
				$dA = wake_all(watch, slot3, [ 0, $dy ]);
			} else {
				const slot4 = $dz[1];
				const part2 = $dz[2];
				$dA = wake_part(watch, slot4, part2, [ 0, $dy ]);
			}
			$dA;
		}
		const $dF = sized;
		if ($dF[0] === 0) {
			notify($dF[1], [ 0, $dy ]);
		}
		const $dG = shape;
		if ($dG[0] === 0) {
			notify($dG[1], [ 0, $dy ]);
		}
		notify(published, [ 0, $dy ]);
		return;
	}, $db);
}
function update(self, change, $cJ) {
	const $cK = self[0].v[6];
	const _check = $cK[1];
	let $cP = null;
	if ($cK[0] === 0) {
		$cP = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$cP = [ 1 ];
	}
	const before = $cP;
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, change(held) ];
	});
	const $cS = out;
	let $cT = null;
	if ($cS[0] === 0) {
		const result = __clone($cS[1][0]);
		const changes = __clone($cS[1][1]);
		audit(self, before, changes);
		landed(self, changes, true, $cJ);
		$cT = result;
	} else {
		$cT = (() => {
			throw __panic("lens: an update\'s write never ran", "std/src/reactive/lens.vl:776:12");
		})();
	}
	return $cT;
}
function eq4(self, b) {
	const $dP = self;
	let $dS = null;
	if ($dP[0] === 0) {
		const $dQ = b;
		let $dR = null;
		if ($dQ[0] === 0) {
			$dR = eq2($dP[1], $dQ[1]);
		} else {
			$dR = false;
		}
		$dS = $dR;
	} else {
		const $dT = b;
		$dS = $dT[0] === 1;
	}
	return $dS;
}
function get6(self) {
	return __clone(self[0].v);
}
function get7(self) {
	return get6(self[0]);
}
function lift(self, op) {
	const $dW = op;
	let $dX = null;
	if ($dW[0] === 0) {
		const at4 = $dW[1];
		const gone = $dW[2];
		const arrived = $dW[3];
		$dX = [ 0, at4, handles(self, gone), handles(self, arrived) ];
	} else if ($dW[0] === 1) {
		const at5 = $dW[1];
		const old = $dW[2];
		const new10 = $dW[3];
		$dX = [ 1, at5, slot_of(self[0], old), slot_of(self[0], new10) ];
	} else if ($dW[0] === 2) {
		const all3 = $dW[1];
		$dX = [ 2, handles(self, all3) ];
	} else {
		const from = $dW[1];
		const count2 = $dW[2];
		const to = $dW[3];
		$dX = [ 3, from, count2, to ];
	}
	return $dX;
}
function since3(self, cursor4) {
	const $dU = since(self[2], cursor4);
	let $dV = null;
	if ($dU[0] === 0) {
		const recorded = $dU[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$dV = out;
	} else {
		$dV = [ [ 2, get5(self) ] ];
	}
	return $dV;
}
function map3(self, fn) {
	const $dZ = self;
	let $ea = null;
	if ($dZ[0] === 0) {
		const x = $dZ[1];
		$ea = [ 0, fn(x) ];
	} else {
		$ea = [ 1 ];
	}
	return $ea;
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
	const $ed = self;
	let $eg = null;
	if ($ed[0] === 0) {
		const $ee = b;
		let $ef = null;
		if ($ee[0] === 0) {
			$ef = eq4($ed[1], $ee[1]);
		} else {
			$ef = false;
		}
		$eg = $ef;
	} else {
		const $eh = b;
		$eg = $eh[0] === 1;
	}
	return $eg;
}
function unwrap_or(self, fallback) {
	const $em = self;
	let $en = null;
	if ($em[0] === 0) {
		const x = $em[1];
		$en = x;
	} else {
		$en = __clone(__force(fallback));
	}
	return $en;
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
function landed_one(self, change, $er) {
	landed(self, [ __clone(change) ], false, $er);
}
function plan(self, change, write, $eo) {
	const $ep = self[0].v[6];
	const _check = $ep[1];
	let $eq = null;
	if ($ep[0] === 0) {
		$eq = [ 0, peek2(self, (held) => {
			return __clone(held);
		}) ];
	} else {
		$eq = [ 1 ];
	}
	const before = $eq;
	coming_one(self, change);
	let out = [ 1 ];
	mutate(self, (held) => {
		return out = [ 0, write(held) ];
	});
	audit(self, before, [ __clone(change) ]);
	landed_one(self, change, $eo);
	const $es = out;
	let $et = null;
	if ($es[0] === 0) {
		const result = __clone($es[1]);
		$et = result;
	} else {
		$et = (() => {
			throw __panic("lens: a planned write never ran", "std/src/reactive/lens.vl:796:12");
		})();
	}
	return $et;
}
function eq6(self, other) {
	return self[1] === other[1];
}
function eq7(self, b) {
	const $eu = self;
	let $ex = null;
	if ($eu[0] === 0) {
		const $ev = b;
		let $ew = null;
		if ($ev[0] === 0) {
			$ew = eq6($eu[1], $ev[1]);
		} else {
			$ew = false;
		}
		$ex = $ew;
	} else {
		const $ey = b;
		$ex = $ey[0] === 1;
	}
	return $ex;
}
function part_cell(watch, slot2, name) {
	if (slot2 < 0) {
		return new7(0, "std/src/reactive/lens.vl:1094:19");
	}
	const found = viewing2(watch, (held) => {
		let out = [ 1 ];
		for (const part2 of __at(held[1], as_usize(slot2), "std/src/reactive/lens.vl:1098:15")[1]) {
			if (part2[0] === name) {
				out = [ 0, __clone(part2[1]) ];
			}
		}
		return out;
	});
	const $eE = found;
	if ($eE[0] === 0) {
		return $eE[1];
	}
	const woke = new7(0, "std/src/reactive/lens.vl:1108:25");
	const entry = as_usize(slot2);
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1110:2")[1].push([ name, __clone(woke) ]);
	return woke;
}
function part(self, name, read) {
	return [ __clone(self), part_cell(self[0][0].v[3], self[1], name), read ];
}
function get8(self) {
	const $eF = at(self[0]);
	if ($eF[0] === 0) {

	} else {
		return [ 1 ];
	}
	const view = self[0][0][0].v[1][2];
	const read = self[2];
	let out = [ 1 ];
	peek2(self[0][0], (held) => {
		return view(held, $eF[1], (item2) => {
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
function rows2(self, $eG) {
	return rows(self[0], $eG);
}
function update2(self, change, $eH) {
	return update(self[0], change, $eH);
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
on_change(__clone(thirty), (_v, $r) => {
	const $s = [ thirty_runs, "v" ];
	return thirty_runs.v = $s[0][$s[1]] + 1;
}, "lens-sorted.vl:92:18");
const lengths = __shared_new(0);
on_change2(length(index), (_n, $z) => {
	const $A = [ lengths, "v" ];
	return lengths.v = $A[0][$A[1]] + 1;
}, "lens-sorted.vl:94:26");
const calls = __shared_new(0);
const doubled = memo(map(items(index, [ 1 ]), (value, $ag, $ah) => {
	const $ai = [ calls, "v" ];
	calls.v = $ai[0][$ai[1]] + 1;
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
let $dY = undefined;
assert(row_ops.length === 1 && (($dY = __at(row_ops, 0, "lens-sorted.vl:118:7")), $dY[0] === 0) && $dY[1] === 2 && $dY[3].length === 1, "one row arrived at 2", "lens-sorted.vl:116:2");
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
