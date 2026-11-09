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
	let index = 0;
	for (const item2 of self) {
		if (!(item2 === __at(b, index, "std/src/compare.vl:147:17"))) {
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
function new2() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function assert(condition, message, caller) {
	let $g = null;
	if (!(condition)) {
		$g = (() => {
			throw __panic(message, caller);
		})();
	}
	return $g;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $y = null;
	if (wrapped < 0) {
		$y = wrapped + modulus;
	} else {
		$y = wrapped;
	}
	return $y;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $x = null;
	if (truncated > 0) {
		$x = truncated;
	} else {
		$x = 0;
	}
	return $x;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $z = null;
	if (wrapped >= half) {
		$z = wrapped - modulus;
	} else {
		$z = wrapped;
	}
	return $z;
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function eq3(self, b) {
	return self === b;
}
function eq4(self, b) {
	const $bd = self;
	let $bg = null;
	if ($bd[0] === 0) {
		const $be = b;
		let $bf = null;
		if ($be[0] === 0) {
			$bf = $bd[1] === $be[1];
		} else {
			$bf = false;
		}
		$bg = $bf;
	} else {
		const $bh = b;
		$bg = $bh[0] === 1;
	}
	return $bg;
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
		const key = hash2(subscriber[0]);
		let $ay = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$ay = undefined;
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
		$ay;
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
	const $cF = turn;
	let $cG = null;
	if ($cF[0] === 0) {
		const ambient = $cF[1];
		$cG = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $cH = last(draining_turns.v);
		let $cI = null;
		if ($cH[0] === 0) {
			const draining = $cH[1];
			$cI = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$cI = undefined;
		}
		$cG = $cI;
	}
	return $cG;
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
function dispose(self, $dY) {
	const $dZ = $dY;
	let $ea = null;
	if ($dZ[0] === 0) {
		const established = $dZ[1];
		$ea = [ 0, established ];
	} else {
		$ea = last(draining_turns.v);
	}
	const ambient = $ea;
	release_under(self, ambient);
}
function detach(handle) {
	const $cT = last(releasing_turns.v);
	let $cU = null;
	if ($cT[0] === 0) {
		const at_release = $cT[1];
		$cU = at_release;
	} else {
		$cU = last(draining_turns.v);
	}
	const turn = $cU;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $cV = [ 0, handle[0] ];
	let $cW = null;
	if ($cV[0] === 0) {
		const subscribers = $cV[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$cW = undefined;
	} else {
		$cW = undefined;
	}
	$cW;
	const $cX = ambient;
	let $cY = null;
	if ($cX[0] === 0) {
		const turn = $cX[1];
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
		$cY = undefined;
	} else {
		$cY = undefined;
	}
	$cY;
	const $cZ = handle[3].v;
	let $da = null;
	if ($cZ[0] === 0) {
		const release7 = $cZ[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release7, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$da = undefined;
	} else {
		$da = undefined;
	}
	return $da;
}
function new4() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $G = null;
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
		$G = undefined;
	}
	return $G;
}
function stage_nursery(self) {
	const $ct = nursery(self);
	let $cu = null;
	if ($ct[0] === 0) {
		const held = $ct[1];
		$cu = held;
	} else {
		const made = detached_nursery();
		self[0].v[3] = [ 0, __clone(made) ];
		$cu = made;
	}
	return $cu;
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
	let $cs = null;
	if (is_disposed(self)) {
		$cs = [ 1 ];
	} else {
		$cs = self[0].v[3];
	}
	return $cs;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $dm = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $dd = held[3];
		let $de = null;
		if ($dd[0] === 0) {
			const nursery2 = $dd[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$de = undefined;
		} else {
			$de = undefined;
		}
		$de;
		let $dl = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $dg = __guarded(cleanup);
				let $dh = null;
				if ($dg[0] === 0) {
					const message = $dg[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$dh = undefined;
				} else {
					$dh = undefined;
				}
				$dh;
			}
			const $dj = failure;
			let $dk = null;
			if ($dj[0] === 0) {
				const message2 = $dj[1];
				$dk = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$dk = undefined;
			}
			$dl = $dk;
		}
		$dm = $dl;
	}
	return $dm;
}
function register_with_owner(subscription, $dT, $dU) {
	const $dV = $dU;
	let $dW = null;
	if ($dV[0] === 0) {
		const owner = $dV[1];
		$dW = take(owner, subscription, $dT);
	} else {
		$dW = __clone(subscription);
	}
	return $dW;
}
function defer_to_owner(cleanup, $D) {
	const $E = $D;
	let $F = null;
	if ($E[0] === 0) {
		const owner = $E[1];
		$F = defer(owner, cleanup);
	} else {
		$F = undefined;
	}
	return $F;
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
		const $dR = previous;
		let $dS = null;
		if ($dR[0] === 0) {
			const earlier = $dR[1];
			$dS = earlier();
		} else {
			$dS = undefined;
		}
		return $dS;
	} ];
}
function settle_together(body, $aM) {
	const $aN = $aM;
	let $aO = null;
	if ($aN[0] === 0) {
		const current = $aN[1];
		$aO = body(current);
	} else {
		const $aP = last(draining_turns.v);
		let $aQ = null;
		if ($aP[0] === 0) {
			const draining = $aP[1];
			$aQ = body(draining);
		} else {
			$aQ = batch(body, $aM);
		}
		$aO = $aQ;
	}
	return $aO;
}
function splice_start(held, at3) {
	let $cM = null;
	if (at3 > held) {
		$cM = held;
	} else {
		$cM = at3;
	}
	return $cM;
}
function splice_count(held, start, removed) {
	let $cN = null;
	if (start + removed > held) {
		$cN = held - start;
	} else {
		$cN = removed;
	}
	return $cN;
}
function new5() {
	return [ [  ] ];
}
function follow(self, at3) {
	if (self[0].length === 0) {
		self[0].push(new6());
	}
	return follow2(__at(self[0], 0, "std/src/reactive/lens.vl:145:3"), at3);
}
function apply(self, change2, wakes) {
	if (self[0].length > 0) {
		apply2(__at(self[0], 0, "std/src/reactive/lens.vl:150:4"), change2, wakes);
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
	const $ae = seat(self, at3);
	const place = $ae[0];
	const index = $ae[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:265:15");
	let $af = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:269:5"), index, -(1), "std/src/reactive/lens.vl:269:35");
			made = made + 1;
		}
		$af = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:273:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
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
	$af;
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
		const $ac = seat(self, at3);
		const place = $ac[0];
		const index = $ac[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:306:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:307:15").length;
		let $ad = null;
		if (left < held - index) {
			$ad = left;
		} else {
			$ad = held - index;
		}
		const take2 = $ad;
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
	const all2 = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:332:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all2, index, "std/src/reactive/lens.vl:336:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:339:3");
	let next = place + 1;
	const held = all2.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:346:5").push(__at(all2, index, "std/src/reactive/lens.vl:346:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow2(self, at3) {
	while (self[6] <= at3) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:361:34")), "std/src/reactive/lens.vl:361:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:363:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:366:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:367:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $A = seat(self, at3);
	const place = $A[0];
	const index = $A[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:371:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:372:14"), index, "std/src/reactive/lens.vl:372:14");
	if (held !== -(1)) {
		return held;
	}
	const slot2 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:379:3"), index, slot2, "std/src/reactive/lens.vl:379:3");
	return slot2;
}
function apply2(self, change2, wakes) {
	const $aa = change2;
	let $ab = null;
	if ($aa[0] === 0) {
		const at3 = $aa[1];
		const removed = $aa[2];
		const inserted = $aa[3];
		close(self, at3, removed, wakes);
		widen(self, at3, inserted);
		$ab = undefined;
	} else if ($aa[0] === 1) {
		const at4 = $aa[1];
		const count2 = $aa[2];
		$ab = widen(self, at4, count2);
	} else if ($aa[0] === 3) {
		const at5 = $aa[1];
		const count3 = $aa[2];
		$ab = close(self, at5, count3, wakes);
	} else if ($aa[0] === 2) {
		const at6 = $aa[1];
		let $ah = null;
		if (at6 < self[6]) {
			const $ag = seat(self, at6);
			const place = $ag[0];
			const index = $ag[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:396:29")), "std/src/reactive/lens.vl:396:17"), index, "std/src/reactive/lens.vl:396:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$ah = undefined;
		}
		$ab = $ah;
	} else if ($aa[0] === 4) {
		const at7 = $aa[1];
		const part = $aa[2];
		let $aj = null;
		if (at7 < self[6]) {
			const $ai = seat(self, at7);
			const place2 = $ai[0];
			const index2 = $ai[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:405:29")), "std/src/reactive/lens.vl:405:17"), index2, "std/src/reactive/lens.vl:405:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part ]);
			}
			$aj = undefined;
		}
		$ab = $aj;
	} else if ($aa[0] === 5) {
		const from = $aa[1];
		const to = $aa[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $ak = null;
		if (from > to) {
			$ak = from;
		} else {
			$ak = to;
		}
		const far = $ak;
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
		const $al = seat(self, from);
		const left = $al[0];
		const index3 = $al[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:429:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:430:16"), index3, "std/src/reactive/lens.vl:430:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:432:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:433:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $am = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$am = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:441:25")), "std/src/reactive/lens.vl:441:13").length ];
		} else {
			$am = seat(self, to);
		}
		const $an = $am;
		const place3 = $an[0];
		const landing = $an[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:445:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:446:5"), landing, slot4, "std/src/reactive/lens.vl:446:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$ab = undefined;
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
		$ab = undefined;
	}
	return $ab;
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
function extend_run(runs, from, to) {
	const held = runs.v.length;
	let $bU = null;
	if (held > 0) {
		const $bT = __at(runs.v, held - 1, "std/src/reactive/lens.vl:1992:31");
		const start = $bT[0];
		const count2 = $bT[1];
		const first = $bT[2];
		const leftward = first < start && from === start + count2 && to === first + count2;
		const rightward = first > start && from === start && to === first;
		if (leftward || rightward) {
			__at_put(runs.v, held - 1, [ start, count2 + 1, first ], "std/src/reactive/lens.vl:1996:4");
			return;
		}
		$bU = undefined;
	}
	$bU;
	runs.v.push([ from, 1, to ]);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function smaller(left, right) {
	let $a = null;
	if (left < right) {
		$a = left;
	} else {
		$a = right;
	}
	return $a;
}
function new7(cap, lines3) {
	let rope = [ [  ], [  ], cap, 0, 0, 0 ];
	splice(rope, 0, 0, lines3);
	rope[4] = 0;
	return rope;
}
function seat2(self, at3) {
	let low = 0;
	let high = self[1].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[1], middle, "lens-rope.vl:49:7") <= at3) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at3 - __at(self[1], low, "lens-rope.vl:55:14") ];
}
function restart2(self, from) {
	while (self[1].length > from) {
		__list_pop(self[1]);
	}
	let $c = null;
	if (from === 0) {
		$c = 0;
	} else {
		$c = __at(self[1], from - 1, "lens-rope.vl:66:4") + __at(self[0], from - 1, "lens-rope.vl:66:28").length;
	}
	let running = $c;
	let chunk = from;
	while (chunk < self[0].length) {
		self[1].push(running);
		running = running + __at(self[0], chunk, "lens-rope.vl:71:15").length;
		chunk = chunk + 1;
	}
}
function cut(self, lines3) {
	const held = lines3.length;
	if (held === 0) {
		return [  ];
	}
	const pieces = Math.trunc((held + self[2] - 1) / self[2]);
	const base = Math.trunc(held / pieces);
	const longer = held % pieces;
	let out = [  ];
	let piece = [  ];
	let made = 0;
	for (const line of lines3) {
		piece.push(line);
		let $b = null;
		if (made < longer) {
			$b = base + 1;
		} else {
			$b = base;
		}
		const size3 = $b;
		if (piece.length === size3) {
			out.push(__clone(piece));
			piece = [  ];
			made = made + 1;
		}
	}
	return out;
}
function chunk_count(self) {
	return self[0].length;
}
function balanced(self) {
	let running = 0;
	let chunk = 0;
	while (chunk < self[0].length) {
		const size3 = __at(self[0], chunk, "lens-rope.vl:109:15").length;
		if (size3 > self[2] || size3 === 0 || __at(self[1], chunk, "lens-rope.vl:110:39") !== running) {
			return false;
		}
		if (self[0].length > 1 && size3 * 2 < self[2]) {
			return false;
		}
		running = running + size3;
		chunk = chunk + 1;
	}
	return running === self[3] && self[1].length === self[0].length;
}
function lines(self) {
	let out = [  ];
	for (const chunk of self[0]) {
		push_many(out, __clone(chunk));
	}
	return out;
}
function count(self) {
	return self[3];
}
function item(self, at3) {
	if (at3 >= self[3]) {
		return [ 1 ];
	}
	const $k = seat2(self, at3);
	const chunk = $k[0];
	const index = $k[1];
	return __list_get(__at(self[0], chunk, "lens-rope.vl:141:3"), index);
}
function lend(self, at3, read) {
	if (at3 >= self[3]) {
		return;
	}
	const $l = seat2(self, at3);
	const chunk = $l[0];
	const index = $l[1];
	read(__at_view(__at(self[0], chunk, "lens-rope.vl:149:9"), index, "lens-rope.vl:149:9"));
}
function splice(self, at3, removed, inserted) {
	const start = smaller(at3, self[3]);
	const taking = smaller(removed, self[3] - start);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	if (self[0].length === 0) {
		self[0] = cut(self, inserted);
		self[3] = inserted.length;
		restart2(self, 0);
		return;
	}
	let $d = null;
	if (start < self[3]) {
		$d = seat2(self, start);
	} else {
		$d = [ self[0].length - 1, __at(self[0], self[0].length - 1, "lens-rope.vl:173:28").length ];
	}
	const $e = $d;
	const first = $e[0];
	const offset = $e[1];
	let from = first;
	let last2 = first + 1;
	while (last2 < self[0].length && __at(self[1], last2, "lens-rope.vl:177:35") < start + taking) {
		last2 = last2 + 1;
	}
	let fresh = [  ];
	let index = 0;
	while (index < offset) {
		fresh.push(__at(__at(self[0], first, "lens-rope.vl:184:15"), index, "lens-rope.vl:184:15"));
		index = index + 1;
	}
	push_many(fresh, __clone(inserted));
	const tail_from = start + taking;
	const window_end = __at(self[1], last2 - 1, "lens-rope.vl:189:20") + __at(self[0], last2 - 1, "lens-rope.vl:189:44").length;
	let line = tail_from;
	while (line < window_end) {
		const $f = seat2(self, line);
		const chunk = $f[0];
		const inner = $f[1];
		fresh.push(__at(__at(self[0], chunk, "lens-rope.vl:193:15"), inner, "lens-rope.vl:193:15"));
		line = line + 1;
	}
	while (fresh.length * 2 < self[2] && (from > 0 || last2 < self[0].length)) {
		if (from > 0) {
			from = from - 1;
			let joined = __clone(__at(self[0], from, "lens-rope.vl:200:18"));
			push_many(joined, __clone(fresh));
			fresh = joined;
		} else {
			push_many(fresh, __clone(__at(self[0], last2, "lens-rope.vl:204:21")));
			last2 = last2 + 1;
		}
	}
	const pieces = cut(self, fresh);
	const consumed = last2 - from;
	if (pieces.length > consumed) {
		self[4] = self[4] + 1;
	}
	if (pieces.length < consumed) {
		self[5] = self[5] + 1;
	}
	let gone = 0;
	while (gone < consumed) {
		__remove_at(self[0], from, "lens-rope.vl:218:16");
		gone = gone + 1;
	}
	let placed = 0;
	for (const piece of pieces) {
		__insert_at(self[0], from + placed, __clone(piece), "lens-rope.vl:223:16");
		placed = placed + 1;
	}
	self[3] = self[3] - taking + inserted.length;
	restart2(self, from);
}
function put(self, at3, value) {
	if (at3 >= self[3]) {
		return;
	}
	const $m = seat2(self, at3);
	const chunk = $m[0];
	const index = $m[1];
	__at_put(__at(self[0], chunk, "lens-rope.vl:236:3"), index, value, "lens-rope.vl:236:3");
}
function change(self, at3, with2) {
	if (at3 >= self[3]) {
		return;
	}
	const $n = seat2(self, at3);
	const chunk = $n[0];
	const index = $n[1];
	with2(__at_view(__at(self[0], chunk, "lens-rope.vl:244:13"), index, "lens-rope.vl:244:13"));
}
function lines2(prefix, count2) {
	let out = [  ];
	let index = 0;
	while (index < count2) {
		out.push("" + prefix + index);
		index = index + 1;
	}
	return out;
}
function splits_and_merges($h, $i) {
	const text = sequence(new7(8, lines2("l", 20)), $h);
	assert(peek(text, (rope) => {
		return chunk_count(rope);
	}) === 3 && peek2(text, (rope) => {
		return balanced(rope);
	}), "20 lines cut into 3 chunks", "lens-rope.vl:261:2");
	const l5 = slot(text, 5, $i);
	const l17 = slot(text, 17, $i);
	const runs5 = __shared_new(0);
	const runs17 = __shared_new(0);
	on_change(__clone(l5), (_line, $H) => {
		const $I = [ runs5, "v" ];
		return runs5.v = $I[0][$I[1]] + 1;
	}, "lens-rope.vl:269:14");
	on_change(__clone(l17), (_line, $P) => {
		const $Q = [ runs17, "v" ];
		return runs17.v = $Q[0][$Q[1]] + 1;
	}, "lens-rope.vl:270:15");
	splice3(text, 2, 0, lines2("new", 10), $h);
	assert(peek4(text, (rope) => {
		return rope[4];
	}) === 1, "the insert split a chunk", "lens-rope.vl:274:2");
	assert(peek2(text, (rope) => {
		return balanced(rope);
	}), "chunks stay in bounds after the split", "lens-rope.vl:275:2");
	assert(eq5(at(l5), [ 0, 15 ]) && eq4(get(l5), [ 0, "l5" ]), "l5 follows its line through the split", "lens-rope.vl:276:2");
	assert(eq5(at(l17), [ 0, 27 ]) && eq4(get(l17), [ 0, "l17" ]), "l17 follows too", "lens-rope.vl:277:2");
	assert(runs5.v === 0 && runs17.v === 0, "an insert above wakes neither", "lens-rope.vl:278:2");
	splice3(text, 3, 11, [  ], $h);
	assert(peek4(text, (rope) => {
		return rope[5];
	}) >= 1, "the delete merged chunks", "lens-rope.vl:282:2");
	assert(peek2(text, (rope) => {
		return balanced(rope);
	}), "chunks stay in bounds after the merge", "lens-rope.vl:283:2");
	assert(eq5(at(l5), [ 0, 4 ]) && eq4(get(l5), [ 0, "l5" ]), "l5 follows its line through the merge", "lens-rope.vl:284:2");
	assert(runs5.v === 0, "a delete next to l5 does not wake it", "lens-rope.vl:285:2");
	splice3(text, 4, 1, [  ], $h);
	assert(eq4(get(l5), [ 1 ]) && runs5.v === 1, "l5 reads None and woke once", "lens-rope.vl:289:2");
	assert(eq4(get(l17), [ 0, "l17" ]) && runs17.v === 0, "l17 did not wake", "lens-rope.vl:290:2");
	const chunks_before = peek(text, (rope) => {
		return chunk_count(rope);
	});
	set(l17, "L17", $h);
	assert(contains(all(text), "L17") && runs17.v === 1, "a slot write lands and wakes once", "lens-rope.vl:295:2");
	assert(peek(text, (rope) => {
		return chunk_count(rope);
	}) === chunks_before, "a one-line write cut no chunk", "lens-rope.vl:296:2");
	const held = unwrap_or(at(l17), __lazy("fallback", () => {
		return 0;
	}));
	splice3(text, held + 1, size(text), [  ], $h);
	splice3(text, 0, held, [  ], $h);
	assert(eq(all(text), [ "L17" ]) && peek(text, (rope) => {
		return chunk_count(rope);
	}) === 1, "one line, one chunk", "lens-rope.vl:302:2");
	assert(eq5(at(l17), [ 0, 0 ]) && runs17.v === 1, "l17 survived every merge, asleep", "lens-rope.vl:306:2");
	console.log("rope: splits and merges kept every slot");
}
function random_splices() {
	let rope = new7(8, lines2("r", 40));
	let model = lines2("r", 40);
	let seed2 = 11;
	let step = 0;
	while (step < 600) {
		seed2 = (seed2 * 75 + 74) % 65537;
		const at3 = seed2 % (model.length + 1);
		seed2 = (seed2 * 75 + 74) % 65537;
		const removed = seed2 % 13;
		seed2 = (seed2 * 75 + 74) % 65537;
		const arriving = lines2("s" + step + "-", seed2 % 15);
		splice(rope, at3, removed, arriving);
		splice2(model, at3, smaller(removed, model.length - at3), arriving);
		assert(eq(lines(rope), model), "step " + step + ": the rope is the model", "lens-rope.vl:326:3");
		assert(balanced(rope), "step " + step + ": the chunks are in bounds", "lens-rope.vl:327:3");
		step = step + 1;
	}
	assert(rope[4] > 0 && rope[5] > 0, "the walk split and merged", "lens-rope.vl:330:2");
	console.log("rope: 600 random bulk splices kept the rope whole");
}
function the_paste($bt, $bu) {
	const text = sequence(new7(64, lines2("l", 300)), $bt);
	const l250 = slot(text, 250, $bu);
	const runs = __shared_new(0);
	on_change(__clone(l250), (_line, $bv) => {
		const $bw = [ runs, "v" ];
		return runs.v = $bw[0][$bw[1]] + 1;
	}, "lens-rope.vl:338:16");
	const seen = __shared_new([  ]);
	const befores = __shared_new(0);
	tap(text, (_change) => {
		const $bx = [ befores, "v" ];
		return befores.v = $bx[0][$bx[1]] + 1;
	}, (change2, _stage) => {
		return seen.v.push(__clone(change2));
	});
	const whole = __shared_new(0);
	const lengths = __shared_new(0);
	on_change2(__clone(text), (_rope, $by) => {
		const $bz = [ whole, "v" ];
		return whole.v = $bz[0][$bz[1]] + 1;
	}, "lens-rope.vl:346:16");
	on_change3(length(text), (_n, $bE) => {
		const $bF = [ lengths, "v" ];
		return lengths.v = $bF[0][$bF[1]] + 1;
	}, "lens-rope.vl:347:25");
	const rows2 = rows(text, $bu);
	const row_cursor = cursor2(rows2);
	const calls = __shared_new(0);
	const widths = memo(map(items(text, $bu), (line, $ch, $ci) => {
		const $cj = [ calls, "v" ];
		calls.v = $cj[0][$cj[1]] + 1;
		return line.length;
	}), $bt, $bu, "lens-rope.vl:357:4");
	const by_line = index_by(text, (line) => {
		return line[0][line[1]];
	}, $bu);
	assert(calls.v === 300 && get6(rows2).length === 300, "the feeds seeded 300 lines", "lens-rope.vl:359:2");
	splice3(text, 1, 0, lines2("paste", 5000), $bt);
	assert(eq7(seen.v, [ [ 0, 1, 0, 5000 ] ]), "the paste is ONE change", "lens-rope.vl:362:2");
	assert(befores.v === 1, "the tap\'s before ran once", "lens-rope.vl:363:2");
	assert(size(text) === 5300 && peek2(text, (rope) => {
		return balanced(rope);
	}), "5,000 lines arrived, chunks in bounds", "lens-rope.vl:364:2");
	assert(eq5(at(l250), [ 0, 5250 ]) && eq4(get(l250), [ 0, "l250" ]) && runs.v === 0, "l250 moved by 5,000 asleep", "lens-rope.vl:368:2");
	assert(whole.v === 1 && lengths.v === 1, "the whole value and the length woke once each", "lens-rope.vl:372:2");
	const row_ops = since3(rows2, row_cursor);
	let $eH = undefined;
	assert(row_ops.length === 1 && (($eH = __at(row_ops, 0, "lens-rope.vl:379:7")), $eH[0] === 0) && $eH[1] === 1 && $eH[3].length === 5000, "rows(): one op with 5,000 rows", "lens-rope.vl:377:2");
	assert(calls.v === 5300 && get8(widths).length === 5300, "items(): the map ran for the 5,000 arrivals alone", "lens-rope.vl:384:2");
	assert(eq8(map3(find(by_line, "paste4999"), (slot2) => {
		return at(slot2);
	}), [ 0, [ 0, 5000 ] ]), "index_by found the last pasted line", "lens-rope.vl:388:2");
	assert(eq4(map3(find(by_line, "l250"), (slot2) => {
		return eq9(slot2, l250);
	}), [ 0, true ]), "and the index\'s slot is l250\'s own", "lens-rope.vl:392:2");
	splice3(text, 1, 5000, [  ], $bt);
	assert(seen.v.length === 2 && size(text) === 300 && peek2(text, (rope) => {
		return balanced(rope);
	}), "the cut is one change", "lens-rope.vl:400:2");
	assert(get6(rows2).length === 300 && get8(widths).length === 300 && eq10(find(by_line, "paste0"), [ 1 ]), "the feeds followed the cut", "lens-rope.vl:404:2");
	assert(eq5(at(l250), [ 0, 250 ]) && runs.v === 0, "l250 is back, asleep", "lens-rope.vl:408:2");
	release4(rows2);
	release6(by_line);
	console.log("rope: a 5000-line paste is one change");
}
function timings($eY, $eZ) {
	const text = sequence(new7(64, lines2("b", 20000)), $eY);
	const rows2 = rows(text, $eZ);
	const widths = memo(map(items(text, $eZ), (line, $fa, $fb) => {
		return line.length;
	}), $eY, $eZ, "lens-rope.vl:420:51");
	const by_line = index_by(text, (line) => {
		return line[0][line[1]];
	}, $eZ);
	const pasted = lines2("p", 5000);
	const paste_start = Date.now();
	splice3(text, 10000, 0, pasted, $eY);
	const paste_ms = Date.now() - paste_start;
	assert(size(text) === 25000 && get6(rows2).length === 25000 && get8(widths).length === 25000, "the paste landed everywhere", "lens-rope.vl:426:2");
	let model = all(text);
	let seed2 = 7;
	let step = 0;
	const edits_start = Date.now();
	while (step < 2000) {
		seed2 = (seed2 * 75 + 74) % 65537;
		const at3 = seed2 % size(text);
		const kind = step % 3;
		if (kind === 0) {
			insert_at(text, at3, "edit" + step, $eY);
			__insert_at(model, at3, "edit" + step, "lens-rope.vl:441:10");
		} else if (kind === 1) {
			replace(text, at3, "set" + step, $eY);
			__at_put(model, at3, "set" + step, "lens-rope.vl:444:4");
		} else {
			remove_at(text, at3, $eY);
			__remove_at(model, at3, "lens-rope.vl:447:10");
		}
		step = step + 1;
	}
	const edits_ms = Date.now() - edits_start;
	assert(eq(all(text), model), "2,000 edits: the rope is the model", "lens-rope.vl:452:2");
	assert(peek2(text, (rope) => {
		return balanced(rope);
	}), "and its chunks are in bounds", "lens-rope.vl:453:2");
	assert(get6(rows2).length === model.length && get8(widths).length === model.length, "the rows and the map followed every edit", "lens-rope.vl:454:2");
	assert(contains(model, "edit1998"), "the walk kept its next-to-last insert", "lens-rope.vl:458:2");
	assert(eq12(map3(find(by_line, "edit1998"), (slot2) => {
		return get(slot2);
	}), [ 0, [ 0, "edit1998" ] ]), "the index followed", "lens-rope.vl:459:2");
	release4(rows2);
	release6(by_line);
	console.log("rope timing: a 5000-line paste into 20000 lines with rows, a map and an index attached: " + paste_ms + " ms");
	console.log("rope timing: 2000 single-line edits under the same: " + edits_ms + " ms");
}
function push_onto(self, target) {
	for (const item2 of self) {
		target.push(__clone(item2));
	}
}
function push_many(self, items2) {
	push_onto(items2, self);
}
function splice2(self, at3, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at3, "std/src/reactive/lens.vl:2231:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at3 + offset, __clone(value), "std/src/reactive/lens.vl:2236:9");
		offset = offset + 1;
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
function sequence(value, $j) {
	return over(value, new5(), [ (held, at3) => {
		return at3 < count(held);
	}, (held, at3) => {
		return item(held, at3);
	}, (held, at3, read) => {
		return lend(held, at3, read);
	}, (held, at3, value2) => {
		return put(held, at3, value2, $j);
	}, (held, at3, with2) => {
		return change(held, at3, with2, $j);
	} ]);
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage(storage, read) {
	const $o = storage;
	let $p = null;
	if ($o[0] === 0) {
		const cell = $o[1];
		$p = viewing(cell, read);
	} else {
		const port = $o[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $q = out;
		let $r = null;
		if ($q[0] === 0) {
			const value = __clone($q[1]);
			$r = value;
		} else {
			$r = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$p = $r;
	}
	return $p;
}
function peek(self, read) {
	return peek_storage(self[0].v[0], read);
}
function viewing2(cell, read) {
	return lend2(cell.v, read);
}
function peek_storage2(storage, read) {
	const $s = storage;
	let $t = null;
	if ($s[0] === 0) {
		const cell = $s[1];
		$t = viewing2(cell, read);
	} else {
		const port = $s[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $u = out;
		let $v = null;
		if ($u[0] === 0) {
			const value = __clone($u[1]);
			$v = value;
		} else {
			$v = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$t = $v;
	}
	return $t;
}
function peek2(self, read) {
	return peek_storage2(self[0].v[0], read);
}
function follow_id(storage, watch, holds, at3) {
	if (viewing2(watch, (held) => {
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
	__at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1061:2")[2] + 1;
	return id;
}
function slot_of(self, id) {
	const $C = __clone(self);
	let $B = null;
	if (id < 0) {
		$B = -(1);
	} else {
		$B = id;
	}
	return [ $C, $B ];
}
function follow3(self, at3) {
	const core = self[0];
	return slot_of(self, follow_id(core.v[0], core.v[3], core.v[1][0], at3));
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
function slot(self, at3, $w) {
	const handle = follow3(self, at3);
	defer_to_owner(() => {
		return release3(handle);
	}, $w);
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
function peek_storage3(storage, read) {
	const $K = storage;
	let $L = null;
	if ($K[0] === 0) {
		const cell = $K[1];
		$L = viewing2(cell, read);
	} else {
		const port = $K[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $M = out;
		let $N = null;
		if ($M[0] === 0) {
			const value = __clone($M[1]);
			$N = value;
		} else {
			$N = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:713:13");
			})();
		}
		$L = $N;
	}
	return $L;
}
function peek3(self, read) {
	return peek_storage3(self[0].v[0], read);
}
function get(self) {
	const $J = at(self);
	if ($J[0] === 0) {

	} else {
		return [ 1 ];
	}
	const read = self[0][0].v[1][1];
	return peek3(self[0], (held) => {
		return read(held, $J[1]);
	});
}
function slot_cell(watch, slot2) {
	if (slot2 < 0) {
		return new8(0, "std/src/reactive/lens.vl:1082:19");
	}
	const entry = as_usize(slot2);
	const $O = __at(watch.v[1], entry, "std/src/reactive/lens.vl:1085:5")[0];
	const cell = __clone($O[1]);
	if ($O[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1088:25");
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
function size(self) {
	return peek(self, (held) => {
		return count(held);
	});
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
	const $U = self[0].v[0];
	let $V = null;
	if ($U[0] === 0) {
		const cell = $U[1];
		$V = change2(cell.v);
	} else {
		const port = __clone($U[1]);
		$V = port[1](change2);
	}
	return $V;
}
function is_empty(self) {
	return self.length === 0;
}
function last(self) {
	let $az = null;
	if (is_empty(self)) {
		$az = [ 1 ];
	} else {
		$az = __list_get(self, self.length - 1);
	}
	return $az;
}
function notify(self, $au) {
	const traced = tracing.v;
	let $av = null;
	if (traced) {
		$av = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$av = "";
	}
	const cause = $av;
	const $aw = $au;
	let $ax = null;
	if ($aw[0] === 0) {
		const turn = $aw[1];
		$ax = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aA = last(draining_turns.v);
		let $aB = null;
		if ($aA[0] === 0) {
			const draining = $aA[1];
			$aB = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aC = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aC = undefined;
				}
				$aC;
			}
			$aB = undefined;
		}
		$ax = $aB;
	}
	return $ax;
}
function wake_all(watch, slot2, $aG) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1120:36"));
	});
	const $aH = held[0];
	if ($aH[0] === 0) {
		notify($aH[1], $aG);
	}
	for (const part of held[1]) {
		notify(part[1], $aG);
	}
}
function wake_part(watch, slot2, name, $aI) {
	const held = viewing2(watch, (table) => {
		return __clone(__at(table[1], as_usize(slot2), "std/src/reactive/lens.vl:1130:36"));
	});
	const $aJ = held[0];
	if ($aJ[0] === 0) {
		notify($aJ[1], $aI);
	}
	for (const part of held[1]) {
		if (part[0] === name) {
			notify(part[1], $aI);
		}
	}
}
function batch(body, $aR) {
	const $aS = $aR;
	let $aT = null;
	if ($aS[0] === 0) {
		const current = $aS[1];
		$aT = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$aT = result;
	}
	return $aT;
}
function landed(self, changes, late, $X) {
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
		const $Y = change2;
		let $Z = null;
		if ($Y[0] === 0) {
			const _at = $Y[1];
			const removed = $Y[2];
			const inserted = $Y[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$Z = undefined;
		} else if ($Y[0] === 1) {
			const _at2 = $Y[1];
			const _count = $Y[2];
			$Z = resized = true;
		} else if ($Y[0] === 3) {
			const _at3 = $Y[1];
			const _count2 = $Y[2];
			$Z = resized = true;
		} else if ($Y[0] === 6) {
			$Z = resized = true;
		} else if ($Y[0] === 5) {
			const _from = $Y[1];
			const _to = $Y[2];
			$Z = shaped = true;
		} else {
			$Z = undefined;
		}
		$Z;
		if (watch.v[1].length > 0) {
			apply(watch.v[0], change2, wakes);
		}
		let $ap = null;
		if (self[0].v[4].length > 0) {
			let $ao = null;
			if (!(late)) {
				$ao = [ 0 ];
			} else if (!(several)) {
				$ao = [ 1 ];
			} else if (index === 0) {
				$ao = [ 2 ];
			} else {
				$ao = [ 3 ];
			}
			const stage = $ao;
			const taps = __clone(self[0].v[4]);
			for (const tap2 of taps) {
				tap2[2](change2, stage);
			}
			$ap = undefined;
		}
		$ap;
		index = index + 1;
	}
	const published = __clone(self[0].v[2]);
	let $aq = null;
	if (shaped || resized) {
		$aq = watch.v[2];
	} else {
		$aq = [ 1 ];
	}
	const shape = __clone($aq);
	let $ar = null;
	if (resized) {
		$ar = watch.v[3];
	} else {
		$ar = [ 1 ];
	}
	const sized = __clone($ar);
	let $as = undefined;
	let $at = undefined;
	if (wakes.length === 0 && (($as = shape), $as[0] === 1) && (($at = sized), $at[0] === 1)) {
		notify(published, $X);
		return;
	}
	settle_together(($aD) => {
		for (const wake2 of wakes) {
			const $aE = wake2;
			let $aF = null;
			if ($aE[0] === 0) {
				const slot2 = $aE[1];
				$aF = wake_all(watch, slot2, [ 0, $aD ]);
			} else if ($aE[0] === 2) {
				const slot3 = $aE[1];
				$aF = wake_all(watch, slot3, [ 0, $aD ]);
			} else {
				const slot4 = $aE[1];
				const part = $aE[2];
				$aF = wake_part(watch, slot4, part, [ 0, $aD ]);
			}
			$aF;
		}
		const $aK = sized;
		if ($aK[0] === 0) {
			notify($aK[1], [ 0, $aD ]);
		}
		const $aL = shape;
		if ($aL[0] === 0) {
			notify($aL[1], [ 0, $aD ]);
		}
		notify(published, [ 0, $aD ]);
		return;
	}, $X);
}
function landed_one(self, change2, $W) {
	landed(self, [ __clone(change2) ], false, $W);
}
function splice3(self, at3, removed, inserted, $R) {
	const held = size(self);
	let $S = null;
	if (at3 > held) {
		$S = held;
	} else {
		$S = at3;
	}
	const start = $S;
	let $T = null;
	if (start + removed > held) {
		$T = held - start;
	} else {
		$T = removed;
	}
	const taking = $T;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start, taking, arriving ];
	coming_one(self, change2);
	mutate(self, (held2) => {
		return splice(held2, start, taking, inserted);
	});
	landed_one(self, change2, $R);
}
function peek4(self, read) {
	return peek_storage3(self[0].v[0], read);
}
function eq5(self, b) {
	const $aY = self;
	let $bb = null;
	if ($aY[0] === 0) {
		const $aZ = b;
		let $ba = null;
		if ($aZ[0] === 0) {
			$ba = eq3($aY[1], $aZ[1]);
		} else {
			$ba = false;
		}
		$bb = $ba;
	} else {
		const $bc = b;
		$bb = $bc[0] === 1;
	}
	return $bb;
}
function has(self, at3) {
	const holds = self[0].v[1][0];
	return peek2(self, (held) => {
		return holds(held, at3);
	});
}
function replace(self, at3, value, $bk) {
	const watch = self[0].v[3];
	let $bl = null;
	if (has(self, at3)) {
		$bl = [ 2, __clone(at3) ];
	} else if (viewing2(watch, (held) => {
		return follows_items(held[0]);
	})) {
		return false;
	} else {
		$bl = [ 1, __clone(at3), 1 ];
	}
	const change2 = $bl;
	coming_one(self, change2);
	const write = self[0].v[1][3];
	mutate(self, (held) => {
		return write(held, at3, value);
	});
	landed_one(self, change2, $bk);
	return true;
}
function set(self, value, $bi) {
	const $bj = at(self);
	if ($bj[0] === 0) {

	} else {
		return false;
	}
	return replace(self[0], $bj[1], value, $bi);
}
function all_of(storage) {
	return peek_storage3(storage, (held) => {
		let out = [  ];
		let at3 = 0;
		const count2 = count(held);
		while (at3 < count2) {
			const $bm = item(held, at3);
			if ($bm[0] === 0) {
				out.push($bm[1]);
			}
			at3 = at3 + 1;
		}
		return out;
	});
}
function all(self) {
	return all_of(self[0].v[0]);
}
function contains(self, value) {
	for (const item2 of self) {
		if (eq2(item2, value)) {
			return true;
		}
	}
	return false;
}
function unwrap_or(self, fallback) {
	const $br = self;
	let $bs = null;
	if ($br[0] === 0) {
		const x = $br[1];
		$bs = x;
	} else {
		$bs = __clone(__force(fallback));
	}
	return $bs;
}
function tap(self, before, after) {
	const id = self[0].v[5];
	self[0].v[5] = id + 1;
	self[0].v[4].push([ id, before, after ]);
	return id;
}
function get2(self) {
	return peek4(self, (held) => {
		return __clone(held);
	});
}
function on_settle3(self, subscriber) {
	return on_settle(self[0].v[2], subscriber);
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
function length(self) {
	return [ __clone(self) ];
}
function get3(self) {
	return peek(self[0], (held) => {
		return count(held);
	});
}
function sized_cell(self) {
	const watch = self[0].v[3];
	const $bG = watch.v[3];
	const cell = __clone($bG[1]);
	if ($bG[0] === 0) {
		return cell;
	}
	const cell2 = new8(0, "std/src/reactive/lens.vl:1019:26");
	watch.v[3] = [ 0, __clone(cell2) ];
	return cell2;
}
function on_settle4(self, subscriber) {
	return on_settle(sized_cell(self[0]), subscriber);
}
function attach_observer3(self, observer, immediately) {
	const subscription = on_settle4(self, mint_subscriber(() => {
		return observer(get3(self));
	}));
	if (immediately) {
		observer(get3(self));
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
function new9() {
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
			const $bK = __list_get(held, index);
			let $bL = null;
			if ($bK[0] === 0) {
				const op = $bK[1];
				$bL = kept.push(op);
			} else {
				$bL = undefined;
			}
			$bL;
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
	untap(self[0], self[3]);
	for (const id of self[1].v) {
		release_id(self[0][0].v[3], id);
	}
	self[1].v = [  ];
}
function rows(self, $bH) {
	const log = new9();
	const held = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const fill = () => {
		let ids = [  ];
		let at3 = 0;
		const count2 = peek_storage(storage, (list) => {
			return count(list);
		});
		while (at3 < count2) {
			ids.push(follow_id(storage, watch, holds, at3));
			at3 = at3 + 1;
		}
		return ids;
	};
	const leave = (at3, count2) => {
		let gone = [  ];
		let taken = 0;
		while (taken < count2) {
			const id = __remove_at(held.v, at3, "std/src/reactive/lens.vl:1839:27");
			release_id(watch, id);
			gone.push(id);
			taken = taken + 1;
		}
		return gone;
	};
	const arrive = (at3, count2) => {
		let arrived = [  ];
		let index = at3;
		while (index < at3 + count2) {
			const id = follow_id(storage, watch, holds, index);
			__insert_at(held.v, index, id, "std/src/reactive/lens.vl:1851:18");
			arrived.push(id);
			index = index + 1;
		}
		return arrived;
	};
	held.v = fill();
	const tap2 = tap(self, (_change) => {
		return;
	}, (change2, _stage) => {
		const $bI = change2;
		let $bJ = null;
		if ($bI[0] === 0) {
			const at3 = $bI[1];
			const removed = $bI[2];
			const inserted = $bI[3];
			const gone = leave(at3, removed);
			record(log, [ 0, at3, gone, arrive(at3, inserted) ]);
			$bJ = undefined;
		} else if ($bI[0] === 1) {
			const at4 = $bI[1];
			const count2 = $bI[2];
			$bJ = record(log, [ 0, at4, [  ], arrive(at4, count2) ]);
		} else if ($bI[0] === 3) {
			const at5 = $bI[1];
			const count3 = $bI[2];
			$bJ = record(log, [ 0, at5, leave(at5, count3), [  ] ]);
		} else if ($bI[0] === 5) {
			const from = $bI[1];
			const to = $bI[2];
			const id = __remove_at(held.v, from, "std/src/reactive/lens.vl:1867:28");
			__insert_at(held.v, to, id, "std/src/reactive/lens.vl:1868:19");
			record(log, [ 3, from, 1, to ]);
			$bJ = undefined;
		} else if ($bI[0] === 6) {
			for (const id2 of held.v) {
				release_id(watch, id2);
			}
			held.v = fill();
			record(log, [ 2, __clone(held.v) ]);
			$bJ = undefined;
		} else {
			$bJ = undefined;
		}
		return $bJ;
	});
	const feed = [ __clone(self), held, __clone(log), tap2 ];
	defer_to_owner(() => {
		return release4(feed);
	}, $bH);
	return feed;
}
function cursor(self) {
	const minted = [ fresh_id(), __shared_new(self[1].v) ];
	self[3].v.push(__clone(minted));
	return minted;
}
function cursor2(self) {
	return cursor(self[2]);
}
function arrived_at(storage, at3, count2) {
	return peek_storage3(storage, (held) => {
		let arrived = [  ];
		let index = at3;
		while (index < at3 + count2) {
			const $bQ = item(held, index);
			if ($bQ[0] === 0) {
				arrived.push($bQ[1]);
			}
			index = index + 1;
		}
		return arrived;
	});
}
function one(item2) {
	const $bR = item2;
	let $bS = null;
	if ($bR[0] === 0) {
		const value = $bR[1];
		$bS = [ __clone(value) ];
	} else {
		$bS = [  ];
	}
	return $bS;
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
	const before = __list_get(leaving.v, 0);
	const after = peek_storage3(storage, (held) => {
		return item(held, at3);
	});
	const $cc = before;
	let $cd = undefined;
	if ($cc[0] === 0 && (($cd = after), $cd[0] === 0)) {
		record2(log, [ 1, at3, __clone($cc[1]), __clone($cd[1]) ]);
	}
}
function release5(self) {
	untap(self[0], self[2]);
}
function items_into(self, log, $bN) {
	const leaving = __shared_new([  ]);
	const runs = __shared_new([  ]);
	const skipping = __shared_new(0);
	const storage = __clone(self[0].v[0]);
	const tap2 = tap(self, (change2) => {
		const $bO = change2;
		let $bP = null;
		if ($bO[0] === 0) {
			const at3 = $bO[1];
			const removed = $bO[2];
			const _inserted = $bO[3];
			leaving.v = arrived_at(storage, at3, removed);
			$bP = undefined;
		} else if ($bO[0] === 3) {
			const at4 = $bO[1];
			const count2 = $bO[2];
			leaving.v = arrived_at(storage, at4, count2);
			$bP = undefined;
		} else if ($bO[0] === 2) {
			const at5 = $bO[1];
			leaving.v = one(peek_storage3(storage, (held) => {
				return item(held, at5);
			}));
			$bP = undefined;
		} else if ($bO[0] === 4) {
			const at6 = $bO[1];
			const _part = $bO[2];
			leaving.v = one(peek_storage3(storage, (held) => {
				return item(held, at6);
			}));
			$bP = undefined;
		} else if ($bO[0] === 5) {
			const from = $bO[1];
			const to = $bO[2];
			$bP = extend_run(runs, from, to);
		} else {
			$bP = undefined;
		}
		return $bP;
	}, (change2, stage) => {
		const $bV = stage;
		let $bW = null;
		if ($bV[0] === 2) {
			record2(log, [ 2, all_of(storage) ]);
			return;
			$bW = undefined;
		} else if ($bV[0] === 3) {
			return;
		} else {
			$bW = undefined;
		}
		$bW;
		const $bZ = stage;
		const late = $bZ[0] === 1;
		const $ca = change2;
		let $cb = null;
		if ($ca[0] === 0) {
			const at3 = $ca[1];
			const removed = $ca[2];
			const inserted = $ca[3];
			if (late && removed > 0) {
				record2(log, [ 2, all_of(storage) ]);
			} else if (late) {
				record2(log, [ 0, at3, [  ], arrived_at(storage, at3, inserted) ]);
			} else {
				record2(log, [ 0, at3, __clone(leaving.v), arrived_at(storage, at3, inserted) ]);
			}
			$cb = undefined;
		} else if ($ca[0] === 1) {
			const at4 = $ca[1];
			const count2 = $ca[2];
			record2(log, [ 0, at4, [  ], arrived_at(storage, at4, count2) ]);
			$cb = undefined;
		} else if ($ca[0] === 3) {
			const at5 = $ca[1];
			const _count = $ca[2];
			if (late) {
				record2(log, [ 2, all_of(storage) ]);
			} else {
				record2(log, [ 0, at5, __clone(leaving.v), [  ] ]);
			}
			$cb = undefined;
		} else if ($ca[0] === 2) {
			const at6 = $ca[1];
			$cb = record_set(log, storage, leaving, at6, late);
		} else if ($ca[0] === 4) {
			const at7 = $ca[1];
			const _part = $ca[2];
			$cb = record_set(log, storage, leaving, at7, late);
		} else if ($ca[0] === 5) {
			const from = $ca[1];
			const to = $ca[2];
			if (skipping.v > 0) {
				const $ce = [ skipping, "v" ];
				skipping.v = $ce[0][$ce[1]] - 1;
				return;
			}
			if (runs.v.length === 0) {
				record2(log, [ 3, from, 1, to ]);
				return;
			}
			const $cf = __remove_at(runs.v, 0, "std/src/reactive/lens.vl:1679:47");
			const start = $cf[0];
			const count3 = $cf[1];
			const first = $cf[2];
			skipping.v = count3 - 1;
			let $cg = null;
			if (first > start) {
				$cg = first + 1 - count3;
			} else {
				$cg = first;
			}
			const landing = $cg;
			record2(log, [ 3, start, count3, landing ]);
			$cb = undefined;
		} else {
			$cb = record2(log, [ 2, all_of(storage) ]);
		}
		return $cb;
	});
	const feed = [ __clone(self), __clone(log), tap2 ];
	defer_to_owner(() => {
		return release5(feed);
	}, $bN);
	return feed;
}
function items(self, $bM) {
	return items_into(self, new9(), $bM);
}
function map(self, transform) {
	return [ self, transform ];
}
function cursor3(self) {
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
		const $cm = __list_get(held, index);
		let $cn = null;
		if ($cm[0] === 0) {
			const op = $cm[1];
			$cn = ops.push(op);
		} else {
			$cn = undefined;
		}
		$cn;
		index = index + 1;
	}
	return [ 0, ops ];
}
function since2(self, cursor4) {
	const $co = since(self[1], cursor4);
	let $cp = null;
	if ($co[0] === 0) {
		const recorded = $co[1];
		$cp = recorded;
	} else {
		$cp = [ [ 2, all(self[0]) ] ];
	}
	return $cp;
}
function reader(self) {
	return (cursor4) => {
		return since2(self, cursor4);
	};
}
function get4(self) {
	return all(self[0]);
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
function open2(self) {
	const cursor4 = cursor3(self);
	const read = reader(self);
	return [ () => {
		return get4(self);
	}, (subscriber) => {
		return on_settle5(self, subscriber);
	}, () => {
		return read(cursor4);
	}, () => {
		return drop_cursor2(self, cursor4);
	} ];
}
function run_element(stage, body) {
	const nursery2 = stage_nursery(stage);
	const value = (($cv) => {
		return (($cw) => {
			return body($cv, $cw);
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
function admit(core, item2) {
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
function seed(core, items2) {
	for (const item2 of items2) {
		const $cJ = admit(core, item2);
		const id = $cJ[0];
		const value = $cJ[1];
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
function held_pairs(core, at3, count2) {
	let pairs = [  ];
	let index = at3;
	while (index < at3 + count2) {
		pairs.push([ __clone(__at(core[1].v, index, "std/src/reactive/delta.vl:1618:15")), __clone(__at(core[2].v, index, "std/src/reactive/delta.vl:1618:41")) ]);
		index = index + 1;
	}
	return pairs;
}
function is_none(self) {
	const $df = self;
	return $df[0] === 1;
}
function retire(core, id) {
	const $cO = __map_get(core[3].v, hash2(id));
	let $cP = null;
	if ($cO[0] === 0) {
		const hold = $cO[1];
		core[3].v.delete(hash2(id));
		const $cQ = hold[2];
		let $cR = null;
		if ($cQ[0] === 0) {
			const handle = $cQ[1];
			$cR = detach(handle);
		} else {
			$cR = undefined;
		}
		$cR;
		hold[3]();
		const $db = hold[4];
		let $dc = null;
		if ($db[0] === 0) {
			const owner = $db[1];
			$dc = dispose2(owner);
		} else {
			$dc = undefined;
		}
		$cP = $dc;
	} else {
		$cP = undefined;
	}
	return $cP;
}
function all_pairs(core) {
	return held_pairs(core, 0, core[0].v.length);
}
function resolve(core, ops) {
	let events = [  ];
	for (const op of ops) {
		const $cK = op;
		let $cL = null;
		if ($cK[0] === 0) {
			const at3 = $cK[1];
			const removed = $cK[2];
			const inserted = $cK[3];
			const held = core[0].v.length;
			const start = splice_start(held, at3);
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
				const $dn = admit(core, item2);
				const id2 = $dn[0];
				const value = $dn[1];
				__insert_at(core[0].v, start + offset, id2, "std/src/reactive/delta.vl:1652:23");
				__insert_at(core[1].v, start + offset, __clone(item2), "std/src/reactive/delta.vl:1653:25");
				__insert_at(core[2].v, start + offset, __clone(value), "std/src/reactive/delta.vl:1654:26");
				arrived.push([ __clone(item2), value ]);
				offset = offset + 1;
			}
			events.push([ 0, start, left, arrived ]);
			$cL = undefined;
		} else if ($cK[0] === 1) {
			const at4 = $cK[1];
			const _was = $cK[2];
			const item3 = $cK[3];
			const $do = __list_get(core[0].v, at4);
			let $dp = null;
			if ($do[0] === 0) {
				const old_id = $do[1];
				const before = [ __clone(__at(core[1].v, at4, "std/src/reactive/delta.vl:1663:21")), __clone(__at(core[2].v, at4, "std/src/reactive/delta.vl:1663:44")) ];
				retire(core, old_id);
				const $dq = admit(core, item3);
				const id3 = $dq[0];
				const value2 = $dq[1];
				__at_put(core[0].v, at4, id3, "std/src/reactive/delta.vl:1669:7");
				__at_put(core[1].v, at4, __clone(item3), "std/src/reactive/delta.vl:1670:7");
				__at_put(core[2].v, at4, __clone(value2), "std/src/reactive/delta.vl:1671:7");
				events.push([ 1, at4, before, [ __clone(item3), value2 ], true ]);
				$dp = undefined;
			} else {
				$dp = undefined;
			}
			$cL = $dp;
		} else if ($cK[0] === 2) {
			const items2 = $cK[1];
			const left2 = all_pairs(core);
			for (const id4 of __clone(core[0].v)) {
				retire(core, id4);
			}
			core[0].v = [  ];
			core[1].v = [  ];
			core[2].v = [  ];
			seed(core, items2);
			events.push([ 2, left2, all_pairs(core) ]);
			$cL = undefined;
		} else {
			const from = $cK[1];
			const count2 = $cK[2];
			const to = $cK[3];
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
			$cL = undefined;
		}
		$cL;
	}
	const woke = __clone(core[4].v);
	let $dv = null;
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
			const $dr = __map_get(positions, hash2(id6));
			let $ds = null;
			if ($dr[0] === 0) {
				const at5 = $dr[1];
				const $dt = __map_get(core[3].v, hash2(id6));
				let $du = null;
				if ($dt[0] === 0) {
					const hold = $dt[1];
					if (hold[0]) {
						const item4 = __clone(__at(core[1].v, at5, "std/src/reactive/delta.vl:1731:19"));
						const before2 = __clone(__at(core[2].v, at5, "std/src/reactive/delta.vl:1732:21"));
						const now = hold[1]();
						__at_put(core[2].v, at5, __clone(now), "std/src/reactive/delta.vl:1734:8");
						events.push([ 1, at5, [ __clone(item4), before2 ], [ item4, now ], false ]);
					}
					$du = undefined;
				} else {
					$du = undefined;
				}
				$ds = $du;
			} else {
				$ds = undefined;
			}
			$ds;
		}
		$dv = undefined;
	}
	$dv;
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
		const $dw = event;
		let $dx = null;
		if ($dw[0] === 0) {
			const at3 = $dw[1];
			const left = $dw[2];
			const arrived = $dw[3];
			ops.push([ 0, at3, map2(left, (pair) => {
				return __clone(pair[1]);
			}), map2(arrived, (pair) => {
				return __clone(pair[1]);
			}) ]);
			$dx = undefined;
		} else if ($dw[0] === 1) {
			const at4 = $dw[1];
			const before = $dw[2];
			const after = $dw[3];
			const _rerun = $dw[4];
			ops.push([ 1, at4, __clone(before[1]), __clone(after[1]) ]);
			$dx = undefined;
		} else if ($dw[0] === 2) {
			const _left = $dw[1];
			const now = $dw[2];
			$dx = ops.push([ 2, map2(now, (pair) => {
				return __clone(pair[1]);
			}) ]);
		} else {
			const from = $dw[1];
			const count2 = $dw[2];
			const to = $dw[3];
			$dx = ops.push([ 3, from, count2, to ]);
		}
		$dx;
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
		const $cx = run_element(stage, ($cq, $cr) => {
			return transform(item2, $cq, $cr);
		});
		const answer = $cx[0];
		const owner = $cx[1];
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
function of(elements) {
	return list_cell(elements, new9());
}
function size2(self) {
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
		const $dE = __list_get(inserted, offset);
		let $dF = null;
		if ($dE[0] === 0) {
			const value = $dE[1];
			$dF = __insert_at(list, start + offset, value, "std/src/reactive/delta.vl:702:28");
		} else {
			$dF = undefined;
		}
		$dF;
		offset = offset + 1;
	}
	return [ 0, start, left, __clone(inserted) ];
}
function at2(self) {
	return self[1].v;
}
function set2(self, value, $dJ, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $dJ);
}
function publish(cell, $dI) {
	set2(cell[1], at2(cell[2]), $dI, "std/src/reactive/delta.vl:799:17");
}
function splice4(self, at3, removed, inserted, $dD) {
	const held = size2(self);
	const start = splice_start(held, at3);
	const taking = splice_count(held, start, removed);
	if (taking === 0 && inserted.length === 0) {
		return;
	}
	record2(self[2], splice_in_place(self[0].v, start, taking, inserted));
	publish(self, $dD);
}
function set_at(self, at3, value, $dK) {
	const $dL = __list_get(self[0].v, at3);
	let $dM = null;
	if ($dL[0] === 0) {
		const previous = $dL[1];
		__at_put(self[0].v, at3, __clone(value), "std/src/reactive/delta.vl:855:5");
		record2(self[2], [ 1, at3, previous, __clone(value) ]);
		publish(self, $dK);
		$dM = undefined;
	} else {
		$dM = undefined;
	}
	return $dM;
}
function set3(self, value, $dN, caller) {
	self[0].v = __clone(value);
	record2(self[2], [ 2, __clone(value) ]);
	publish(self, $dN);
}
function move_range(self, from, count2, to, $dO) {
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
		const $dP = __list_get(lifted, offset);
		let $dQ = null;
		if ($dP[0] === 0) {
			const value = $dP[1];
			$dQ = __insert_at(self[0].v, to + offset, value, "std/src/reactive/delta.vl:881:43");
		} else {
			$dQ = undefined;
		}
		$dQ;
		offset = offset + 1;
	}
	record2(self[2], [ 3, from, count2, to ]);
	publish(self, $dO);
}
function apply_ops(cell, ops, $dA) {
	for (const op of ops) {
		const $dB = op;
		let $dC = null;
		if ($dB[0] === 0) {
			const at3 = $dB[1];
			const removed = $dB[2];
			const inserted = $dB[3];
			$dC = splice4(cell, at3, removed.length, inserted, $dA);
		} else if ($dB[0] === 1) {
			const at4 = $dB[1];
			const _was = $dB[2];
			const value = $dB[3];
			$dC = set_at(cell, at4, value, $dA);
		} else if ($dB[0] === 2) {
			const items2 = $dB[1];
			$dC = set3(cell, items2, $dA, "std/src/reactive/delta.vl:1325:36");
		} else {
			const from = $dB[1];
			const count2 = $dB[2];
			const to = $dB[3];
			$dC = move_range(cell, from, count2, to, $dA);
		}
		$dC;
	}
}
function take(self, item2, $dX) {
	defer(self, () => {
		dispose(item2, $dX);
		return;
	});
	return __clone(item2);
}
function seal_collection(instance, owned, $dy, $dz, caller) {
	const sealed = of(fresh_list(instance[0]()));
	const drain2 = instance[2];
	if (owned) {
		const refresh = subscriber_of(() => {
			return apply_ops(sealed, drain2(), $dy);
		}, true);
		place_subscriber(refresh[0], caller);
		const refreshed = instance[1](refresh);
		also_releasing(refreshed, instance[3]);
		register_with_owner(refreshed, $dy, $dz);
	} else {
		const cell_global_refresh = () => {
			return apply_ops(sealed, drain2(), $dy);
		};
		const refresh2 = subscriber_of(cell_global_refresh, true);
		place_subscriber(refresh2[0], caller);
		instance[1](refresh2);
	}
	return sealed;
}
function memo(self, $ck, $cl, caller) {
	return [ seal_collection(open3(self), true, $ck, $cl, caller) ];
}
function get5(self, key) {
	return __nmap_get(self[0], hash(key));
}
function insert(self, key, value) {
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
		const $ei = __list_pop(keys);
		let $ej = null;
		if ($ei[0] === 0) {
			const key = $ei[1];
			const $ek = __list_pop(values);
			let $el = null;
			if ($ek[0] === 0) {
				const value = $ek[1];
				$el = backwards.push([ key, value ]);
			} else {
				$el = undefined;
			}
			$ej = $el;
		} else {
			$ej = undefined;
		}
		$ej;
	}
	let result = [  ];
	while (!(is_empty(backwards))) {
		const $em = __list_pop(backwards);
		let $en = null;
		if ($em[0] === 0) {
			const pair = $em[1];
			$en = result.push(pair);
		} else {
			$en = undefined;
		}
		$en;
	}
	return result;
}
function new10() {
	const table = [ new Map(), undefined ];
	return [ table ];
}
function release6(self) {
	untap(self[0], self[2]);
	for (const _ of entries(self[1].v)) {
		const $ey = _;
		const _key = $ey[0];
		const id = $ey[1];
		release_id(self[0][0].v[3], id);
	}
	self[1].v = new10();
}
function index_by(self, key, $eb) {
	const found = __shared_new(new2());
	const leaving = __shared_new([  ]);
	const storage = __clone(self[0].v[0]);
	const watch = self[0].v[3];
	const holds = self[0].v[1][0];
	const view = self[0].v[1][2];
	const key_at = (at3) => {
		let out = [ 1 ];
		peek_storage3(storage, (held) => {
			return view(held, at3, (item2) => {
				return out = [ 0, key(item2) ];
			});
		});
		return out;
	};
	const map4 = (held, at3) => {
		const id = follow_id(storage, watch, holds, at3);
		const before = get5(found.v, held);
		insert(found.v, held, id);
		const $eg = before;
		if ($eg[0] === 0) {
			release_id(watch, $eg[1]);
		}
		return;
	};
	const unmap = (held) => {
		const $eh = get5(found.v, held);
		if ($eh[0] === 0) {
			remove(found.v, held);
			release_id(watch, $eh[1]);
		}
		return;
	};
	const fill = () => {
		for (const _ of entries(found.v)) {
			const $eo = _;
			const _held = $eo[0];
			const old = $eo[1];
			release_id(watch, old);
		}
		found.v = new10();
		let at3 = 0;
		const count2 = peek_storage(storage, (held) => {
			return count(held);
		});
		while (at3 < count2) {
			const $ep = key_at(at3);
			if ($ep[0] === 0) {
				map4($ep[1], at3);
			}
			at3 = at3 + 1;
		}
		return;
	};
	fill();
	const note = (at3, count2) => {
		let gone = [  ];
		let index2 = at3;
		while (index2 < at3 + count2) {
			const $eq = key_at(index2);
			if ($eq[0] === 0) {
				gone.push($eq[1]);
			}
			index2 = index2 + 1;
		}
		leaving.v = gone;
		return;
	};
	const arrive = (at3, count2) => {
		let index2 = at3;
		while (index2 < at3 + count2) {
			const $er = key_at(index2);
			if ($er[0] === 0) {
				map4($er[1], index2);
			}
			index2 = index2 + 1;
		}
		return;
	};
	const tap2 = tap(self, (change2) => {
		const $es = change2;
		let $et = null;
		if ($es[0] === 0) {
			const at3 = $es[1];
			const removed = $es[2];
			const _inserted = $es[3];
			$et = note(at3, removed);
		} else if ($es[0] === 3) {
			const at4 = $es[1];
			const count2 = $es[2];
			$et = note(at4, count2);
		} else if ($es[0] === 2) {
			const at5 = $es[1];
			$et = note(at5, 1);
		} else if ($es[0] === 4) {
			const at6 = $es[1];
			const _part = $es[2];
			$et = note(at6, 1);
		} else {
			$et = undefined;
		}
		return $et;
	}, (change2, stage) => {
		const $eu = stage;
		let $ev = null;
		if ($eu[0] === 0) {
			$ev = undefined;
		} else if ($eu[0] === 3) {
			return;
		} else {
			fill();
			return;
			$ev = undefined;
		}
		$ev;
		const $ew = change2;
		let $ex = null;
		if ($ew[0] === 0) {
			const at3 = $ew[1];
			const _removed = $ew[2];
			const inserted = $ew[3];
			for (const held of leaving.v) {
				unmap(held);
			}
			arrive(at3, inserted);
			$ex = undefined;
		} else if ($ew[0] === 1) {
			const at4 = $ew[1];
			const count2 = $ew[2];
			$ex = arrive(at4, count2);
		} else if ($ew[0] === 3) {
			const _at = $ew[1];
			const _count = $ew[2];
			for (const held2 of leaving.v) {
				unmap(held2);
			}
			$ex = undefined;
		} else if ($ew[0] === 2) {
			const at5 = $ew[1];
			for (const held3 of leaving.v) {
				unmap(held3);
			}
			arrive(at5, 1);
			$ex = undefined;
		} else if ($ew[0] === 4) {
			const at6 = $ew[1];
			const _part = $ew[2];
			for (const held4 of leaving.v) {
				unmap(held4);
			}
			arrive(at6, 1);
			$ex = undefined;
		} else if ($ew[0] === 5) {
			const _from = $ew[1];
			const _to = $ew[2];
			$ex = undefined;
		} else {
			$ex = fill();
		}
		return $ex;
	});
	const index = [ __clone(self), found, tap2 ];
	defer_to_owner(() => {
		return release6(index);
	}, $eb);
	return index;
}
function handles(self, ids) {
	let out = [  ];
	for (const id of ids) {
		out.push(slot_of(self[0], id));
	}
	return out;
}
function get6(self) {
	return handles(self, self[1].v);
}
function eq6(self, other) {
	const $ez = [ self, other ];
	let $eA = null;
	if ($ez[0][0] === 0 && $ez[1][0] === 0) {
		const s0 = $ez[0][1];
		const s1 = $ez[0][2];
		const s2 = $ez[0][3];
		const o0 = $ez[1][1];
		const o1 = $ez[1][2];
		const o2 = $ez[1][3];
		$eA = eq3(s0, o0) && s1 === o1 && s2 === o2;
	} else if ($ez[0][0] === 1 && $ez[1][0] === 1) {
		const s02 = $ez[0][1];
		const s12 = $ez[0][2];
		const o02 = $ez[1][1];
		const o12 = $ez[1][2];
		$eA = eq3(s02, o02) && s12 === o12;
	} else if ($ez[0][0] === 2 && $ez[1][0] === 2) {
		const s03 = $ez[0][1];
		const o03 = $ez[1][1];
		$eA = eq3(s03, o03);
	} else if ($ez[0][0] === 3 && $ez[1][0] === 3) {
		const s04 = $ez[0][1];
		const s13 = $ez[0][2];
		const o04 = $ez[1][1];
		const o13 = $ez[1][2];
		$eA = eq3(s04, o04) && s13 === o13;
	} else if ($ez[0][0] === 4 && $ez[1][0] === 4) {
		const s05 = $ez[0][1];
		const s14 = $ez[0][2];
		const o05 = $ez[1][1];
		const o14 = $ez[1][2];
		$eA = eq3(s05, o05) && s14 === o14;
	} else if ($ez[0][0] === 5 && $ez[1][0] === 5) {
		const s06 = $ez[0][1];
		const s15 = $ez[0][2];
		const o06 = $ez[1][1];
		const o15 = $ez[1][2];
		$eA = eq3(s06, o06) && eq3(s15, o15);
	} else if ($ez[0][0] === 6 && $ez[1][0] === 6) {
		$eA = true;
	} else {
		$eA = false;
	}
	return $eA;
}
function eq7(self, b) {
	if (self.length !== b.length) {
		return false;
	}
	let index = 0;
	for (const item2 of self) {
		if (!(eq6(item2, __at(b, index, "std/src/compare.vl:147:17")))) {
			return false;
		}
		index = index + 1;
	}
	return true;
}
function lift(self, op) {
	const $eF = op;
	let $eG = null;
	if ($eF[0] === 0) {
		const at3 = $eF[1];
		const gone = $eF[2];
		const arrived = $eF[3];
		$eG = [ 0, at3, handles(self, gone), handles(self, arrived) ];
	} else if ($eF[0] === 1) {
		const at4 = $eF[1];
		const old = $eF[2];
		const new11 = $eF[3];
		$eG = [ 1, at4, slot_of(self[0], old), slot_of(self[0], new11) ];
	} else if ($eF[0] === 2) {
		const all2 = $eF[1];
		$eG = [ 2, handles(self, all2) ];
	} else {
		const from = $eF[1];
		const count2 = $eF[2];
		const to = $eF[3];
		$eG = [ 3, from, count2, to ];
	}
	return $eG;
}
function since3(self, cursor4) {
	const $eD = since(self[2], cursor4);
	let $eE = null;
	if ($eD[0] === 0) {
		const recorded = $eD[1];
		let out = [  ];
		for (const op of recorded) {
			out.push(lift(self, op));
		}
		$eE = out;
	} else {
		$eE = [ [ 2, get6(self) ] ];
	}
	return $eE;
}
function get7(self) {
	return __clone(self[0].v);
}
function get8(self) {
	return get7(self[0]);
}
function map3(self, fn) {
	const $eI = self;
	let $eJ = null;
	if ($eI[0] === 0) {
		const x = $eI[1];
		$eJ = [ 0, fn(x) ];
	} else {
		$eJ = [ 1 ];
	}
	return $eJ;
}
function find(self, key) {
	const lens = __clone(self[0]);
	return map3(viewing2(self[1], (held) => {
		return get5(held, key);
	}), (id) => {
		return slot_of(lens, id);
	});
}
function eq8(self, b) {
	const $eM = self;
	let $eP = null;
	if ($eM[0] === 0) {
		const $eN = b;
		let $eO = null;
		if ($eN[0] === 0) {
			$eO = eq5($eM[1], $eN[1]);
		} else {
			$eO = false;
		}
		$eP = $eO;
	} else {
		const $eQ = b;
		$eP = $eQ[0] === 1;
	}
	return $eP;
}
function eq9(self, other) {
	return self[1] === other[1];
}
function eq10(self, b) {
	const $eT = self;
	let $eW = null;
	if ($eT[0] === 0) {
		const $eU = b;
		let $eV = null;
		if ($eU[0] === 0) {
			$eV = eq9($eT[1], $eU[1]);
		} else {
			$eV = false;
		}
		$eW = $eV;
	} else {
		const $eX = b;
		$eW = $eX[0] === 1;
	}
	return $eW;
}
function insert_at(self, at3, value, $fc) {
	return splice3(self, at3, 0, [ __clone(value) ], $fc);
}
function remove_at(self, at3, $fd) {
	return splice3(self, at3, 1, [  ], $fd);
}
function eq11(self, b) {
	const $fi = self;
	let $fl = null;
	if ($fi[0] === 0) {
		const $fj = b;
		let $fk = null;
		if ($fj[0] === 0) {
			$fk = $fi[1] === $fj[1];
		} else {
			$fk = false;
		}
		$fl = $fk;
	} else {
		const $fm = b;
		$fl = $fm[0] === 1;
	}
	return $fl;
}
function eq12(self, b) {
	const $fg = self;
	let $fo = null;
	if ($fg[0] === 0) {
		const $fh = b;
		let $fn = null;
		if ($fh[0] === 0) {
			$fn = eq11($fg[1], $fh[1]);
		} else {
			$fn = false;
		}
		$fo = $fn;
	} else {
		const $fp = b;
		$fo = $fp[0] === 1;
	}
	return $fo;
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
random_splices();
splits_and_merges([ 1 ], [ 1 ]);
the_paste([ 1 ], [ 1 ]);
timings([ 1 ], [ 1 ]);
console.log("rope: all passed");
//# sourceMappingURL=lens-rope.mjs.map
