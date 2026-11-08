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
function __nursery_has_spawned(n) {
	return n.children.length > 0;
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
function hash(self) {
	return __hash(self);
}
function assert(condition, message, caller) {
	let $bj = null;
	if (!(condition)) {
		$bj = (() => {
			throw __panic(message, caller);
		})();
	}
	return $bj;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $h = null;
	if (wrapped < 0) {
		$h = wrapped + modulus;
	} else {
		$h = wrapped;
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
	const wrapped = fold_unsigned(value, modulus);
	let $i = null;
	if (wrapped >= half) {
		$i = wrapped - modulus;
	} else {
		$i = wrapped;
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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
		const key = hash(subscriber[0]);
		let $au = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				if (tracing.v) {
					trace_hooks.v[3](turn, subscriber[0]);
				}
				turn[1].v.push(__clone(subscriber));
			}
			$au = undefined;
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
		$au;
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
	const $as = turn;
	let $at = null;
	if ($as[0] === 0) {
		const ambient = $as[1];
		$at = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $av = last(draining_turns.v);
		let $aw = null;
		if ($av[0] === 0) {
			const draining = $av[1];
			$aw = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aw = undefined;
		}
		$at = $aw;
	}
	return $at;
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
function retiring(subscriber, release3) {
	return detached(subscriber[0], subscriber[2], release3);
}
function detached(id, live, release3) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release3 ]) ];
}
function dispose(self, $bg) {
	const $bh = $bg;
	let $bi = null;
	if ($bh[0] === 0) {
		const established = $bh[1];
		$bi = [ 0, established ];
	} else {
		$bi = last(draining_turns.v);
	}
	const ambient = $bi;
	release_under(self, ambient);
}
function detach(handle) {
	const $aA = last(releasing_turns.v);
	let $aB = null;
	if ($aA[0] === 0) {
		const at_release = $aA[1];
		$aB = at_release;
	} else {
		$aB = last(draining_turns.v);
	}
	const turn = $aB;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aC = [ 0, handle[0] ];
	let $aD = null;
	if ($aC[0] === 0) {
		const subscribers = $aC[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aD = undefined;
	} else {
		$aD = undefined;
	}
	$aD;
	const $aE = ambient;
	let $aF = null;
	if ($aE[0] === 0) {
		const turn = $aE[1];
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
		$aF = undefined;
	} else {
		$aF = undefined;
	}
	$aF;
	const $aG = handle[3].v;
	let $aH = null;
	if ($aG[0] === 0) {
		const release3 = $aG[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(__clone(ambient));
		__with_finally(release3, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aH = undefined;
	} else {
		$aH = undefined;
	}
	return $aH;
}
function new3() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $o = null;
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
		$o = undefined;
	}
	return $o;
}
function renew(self) {
	const $R = self[0].v[3];
	let $S = null;
	if ($R[0] === 0) {
		const nursery2 = __clone($R[1]);
		let $T = null;
		if (has_spawned(nursery2)) {
			$T = [ 1 ];
		} else {
			$T = [ 0, nursery2 ];
		}
		$S = $T;
	} else {
		$S = [ 1 ];
	}
	const carried = $S;
	advance([ self[0], self[0].v[0] ], carried);
	if (is_none(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $ae = null;
	if (is_disposed(self)) {
		$ae = [ 1 ];
	} else {
		$ae = self[0].v[3];
	}
	return $ae;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $ad = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, __clone(carried) ];
		const $U = held[3];
		let $V = null;
		if ($U[0] === 0) {
			const nursery2 = $U[1];
			if (is_none(carried)) {
				nursery2.cancel();
			}
			$V = undefined;
		} else {
			$V = undefined;
		}
		$V;
		let $ac = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $X = __guarded(cleanup);
				let $Y = null;
				if ($X[0] === 0) {
					const message = $X[1];
					if (is_none(failure)) {
						failure = [ 0, message ];
					}
					$Y = undefined;
				} else {
					$Y = undefined;
				}
				$Y;
			}
			const $aa = failure;
			let $ab = null;
			if ($aa[0] === 0) {
				const message2 = $aa[1];
				$ab = (() => {
					throw __panic(message2, "std/src/reactive.vl:1294:27");
				})();
			} else {
				$ab = undefined;
			}
			$ac = $ab;
		}
		$ad = $ac;
	}
	return $ad;
}
function register_with_owner(subscription, $bb, $bc) {
	const $bd = $bc;
	let $be = null;
	if ($bd[0] === 0) {
		const owner = $bd[1];
		$be = take(owner, subscription, $bb);
	} else {
		$be = __clone(subscription);
	}
	return $be;
}
function defer_to_owner(cleanup, $l) {
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const owner = $m[1];
		$n = defer(owner, cleanup);
	} else {
		$n = undefined;
	}
	return $n;
}
function release_runs(runs2) {
	dispose2([ runs2[0], runs2[0].v[0] ]);
}
function new_tracker(site) {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ], site ]) ];
}
function lists_of(tracker) {
	const $A = tracker[0].v[6];
	let $B = null;
	if ($A[0] === 0) {
		const lists = $A[1];
		$B = lists;
	} else {
		trackers_allocated_count.v = trackers_allocated_count.v + 1;
		const made = __shared_new([ [  ], [  ], [  ] ]);
		tracker[0].v[6] = [ 0, made ];
		$B = made;
	}
	return $B;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $M = tracker[0].v[6];
	let $N = null;
	if ($M[0] === 0) {
		const lists = $M[1];
		if (!(is_empty(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$N = undefined;
	} else {
		$N = undefined;
	}
	$N;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $aj = tracker[0].v[6];
	let $ak = null;
	if ($aj[0] === 0) {
		const lists = $aj[1];
		$ak = lists;
	} else {
		return;
		$ak = undefined;
	}
	const lists2 = $ak;
	const $al = tracker[0].v[5];
	let $am = null;
	if ($al[0] === 0) {
		const target = __clone($al[1]);
		$am = reconnect(tracker, lists2, target);
	} else {
		if (!(is_empty(lists2.v[0])) || !(is_empty(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$am = undefined;
	}
	$am;
	if (!(is_empty(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if (is_empty(lists.v[0]) && is_empty(lists.v[2])) {
		return;
	}
	const held = __clone(lists.v[2]);
	const reading = __clone(lists.v[0]);
	let kept = [  ];
	for (const _edge of held) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $ap = reusable(held, kept, dependency[0], position);
		let $aq = null;
		if ($ap[0] === 0) {
			const index = $ap[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1715:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1716:15")));
			$aq = undefined;
		} else {
			next.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
			$aq = undefined;
		}
		$aq;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1730:7"))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity3, position) {
	const $an = identity3;
	let $ao = null;
	if ($an[0] === 0) {
		const wanted = $an[1];
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1753:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1754:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1759:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1759:38")[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$ao = [ 1 ];
	} else {
		$ao = [ 1 ];
	}
	return $ao;
}
function same_identity(identity3, wanted) {
	const $y = identity3;
	let $z = null;
	if ($y[0] === 0) {
		const held = $y[1];
		$z = held === wanted;
	} else {
		$z = false;
	}
	return $z;
}
function relay_for(tracker, target) {
	const relay = subscriber_of(() => {
		const connecting = tracker[0].v[3];
		tracker[0].v[2] = true;
		if (connecting) {
			tracker[0].v[4] = true;
		} else {
			wake(target);
		}
		return;
	}, true);
	const $ax = tracker[0].v[7];
	let $ay = null;
	if ($ax[0] === 0) {
		const site = __clone($ax[1]);
		$ay = place_subscriber(relay[0], site);
	} else {
		$ay = undefined;
	}
	$ay;
	return relay;
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aL = tracker[0].v[6];
	let $aM = null;
	if ($aL[0] === 0) {
		const lists = $aL[1];
		$aM = lists;
	} else {
		return;
		$aM = undefined;
	}
	const lists2 = $aM;
	let $aN = null;
	if (!(is_empty(lists2.v[1]))) {
		const read = __clone(lists2.v[1]);
		lists2.v[1] = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ __clone(dependency[0]), dependency[1](relay_for(tracker, target)) ]);
		}
		lists2.v[2] = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aN = undefined;
	}
	return $aN;
}
function forget_reads(tracker) {
	const $aO = tracker[0].v[6];
	let $aP = null;
	if ($aO[0] === 0) {
		const lists = $aO[1];
		$aP = lists;
	} else {
		return;
		$aP = undefined;
	}
	const lists2 = $aP;
	let $aQ = null;
	if (!(is_empty(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aQ = undefined;
	}
	$aQ;
	if (!(is_empty(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function admits(scope, identity3) {
	const tracker = __clone(scope[0]);
	if (!(tracker[0].v[1]) || tracker[0].v[0] !== scope[1]) {
		return false;
	}
	const $u = tracker[0].v[6];
	let $v = null;
	if ($u[0] === 0) {
		const lists = $u[1];
		$v = lists;
	} else {
		return true;
		$v = undefined;
	}
	const lists2 = $v;
	const $w = identity3;
	let $x = null;
	if ($w[0] === 0) {
		const wanted = $w[1];
		const count2 = lists2.v[0].length;
		let index = 0;
		while (index < count2) {
			if (same_identity(__at(lists2.v[0], index, "std/src/reactive.vl:1877:22")[0], wanted)) {
				return false;
			}
			index = index + 1;
		}
		$x = true;
	} else {
		$x = true;
	}
	return $x;
}
function place_subscriber(id, site) {
	if (tracing.v) {
		trace_hooks.v[8](id, site);
	}
}
function also_releasing(handle, release3) {
	const previous = __clone(handle[3].v);
	handle[3].v = [ 0, () => {
		release3();
		const $aZ = previous;
		let $ba = null;
		if ($aZ[0] === 0) {
			const earlier = $aZ[1];
			$ba = earlier();
		} else {
			$ba = undefined;
		}
		return $ba;
	} ];
}
function settle_together(body, $bR) {
	const $bS = $bR;
	let $bT = null;
	if ($bS[0] === 0) {
		const current = $bS[1];
		$bT = body(current);
	} else {
		const $bU = last(draining_turns.v);
		let $bV = null;
		if ($bU[0] === 0) {
			const draining = $bU[1];
			$bV = body(draining);
		} else {
			$bV = batch(body, $bR);
		}
		$bT = $bV;
	}
	return $bT;
}
function new4() {
	return [ [  ], [  ], [  ], [  ], [  ], [  ], 0 ];
}
function seat(self, at2) {
	let low = 0;
	let high = self[3].length;
	while (high - low > 1) {
		const middle = Math.trunc((low + high) / 2);
		if (__at(self[3], middle, "std/src/reactive/lens.vl:157:7") <= at2) {
			low = middle;
		} else {
			high = middle;
		}
	}
	return [ low, at2 - __at(self[3], low, "std/src/reactive/lens.vl:163:14") ];
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
function widen(self, at2, count2) {
	if (count2 === 0 || at2 >= self[6]) {
		return;
	}
	const $bu = seat(self, at2);
	const place = $bu[0];
	const index = $bu[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:209:15");
	let $bv = null;
	if (count2 <= 8) {
		let made = 0;
		while (made < count2) {
			__insert_at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:213:5"), index, -(1), "std/src/reactive/lens.vl:213:35");
			made = made + 1;
		}
		$bv = undefined;
	} else {
		const before = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:217:17"));
		let grown = [  ];
		let copied = 0;
		while (copied < index) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:221:16"));
			copied = copied + 1;
		}
		let made2 = 0;
		while (made2 < count2) {
			grown.push(-(1));
			made2 = made2 + 1;
		}
		const held = before.length;
		while (copied < held) {
			grown.push(__at(before, copied, "std/src/reactive/lens.vl:231:16"));
			copied = copied + 1;
		}
		__at_put(self[0], as_usize(block), grown, "std/src/reactive/lens.vl:234:4");
	}
	$bv;
	self[6] = self[6] + count2;
	renumber(self, block, index + count2);
	if (__at(self[0], as_usize(block), "std/src/reactive/lens.vl:238:6").length > 2 * block_size) {
		rebalance(self, place);
	}
	restart(self);
}
function close(self, at2, count2, wakes) {
	let left = count2;
	while (left > 0 && at2 < self[6]) {
		const $bs = seat(self, at2);
		const place = $bs[0];
		const index = $bs[1];
		const block = __at(self[1], place, "std/src/reactive/lens.vl:250:16");
		const held = __at(self[0], as_usize(block), "std/src/reactive/lens.vl:251:15").length;
		let $bt = null;
		if (left < held - index) {
			$bt = left;
		} else {
			$bt = held - index;
		}
		const take2 = $bt;
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
	const all = __clone(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:276:13"));
	let first = [  ];
	let index = 0;
	while (index < block_size) {
		first.push(__at(all, index, "std/src/reactive/lens.vl:280:15"));
		index = index + 1;
	}
	__at_put(self[0], as_usize(block), first, "std/src/reactive/lens.vl:283:3");
	let next = place + 1;
	const held = all.length;
	while (index < held) {
		const fresh = open(self, next);
		let filled = 0;
		while (filled < block_size && index < held) {
			__at(self[0], as_usize(fresh), "std/src/reactive/lens.vl:290:5").push(__at(all, index, "std/src/reactive/lens.vl:290:40"));
			index = index + 1;
			filled = filled + 1;
		}
		renumber(self, fresh, 0);
		next = next + 1;
	}
}
function follow(self, at2) {
	while (self[6] <= at2) {
		const places = self[1].length;
		if (places === 0 || __at(self[0], as_usize(__at(self[1], places - 1, "std/src/reactive/lens.vl:304:34")), "std/src/reactive/lens.vl:304:22").length >= block_size) {
			const fresh = open(self, places);
			__at_put(self[2], as_usize(fresh), places, "std/src/reactive/lens.vl:306:5");
			self[3].push(self[6]);
		}
		const last2 = __at(self[1], self[1].length - 1, "std/src/reactive/lens.vl:309:15");
		__at(self[0], as_usize(last2), "std/src/reactive/lens.vl:310:4").push(-(1));
		self[6] = self[6] + 1;
	}
	const $j = seat(self, at2);
	const place = $j[0];
	const index = $j[1];
	const block = __at(self[1], place, "std/src/reactive/lens.vl:314:15");
	const held = __at(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:315:14"), index, "std/src/reactive/lens.vl:315:14");
	if (held !== -(1)) {
		return held;
	}
	const slot2 = as_i32(self[4].length);
	self[4].push(block);
	self[5].push(index);
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:322:3"), index, slot2, "std/src/reactive/lens.vl:322:3");
	return slot2;
}
function apply(self, change2, wakes) {
	const $bq = change2;
	let $br = null;
	if ($bq[0] === 0) {
		const at2 = $bq[1];
		const removed = $bq[2];
		const inserted = $bq[3];
		close(self, at2, removed, wakes);
		widen(self, at2, inserted);
		$br = undefined;
	} else if ($bq[0] === 1) {
		const at3 = $bq[1];
		const count2 = $bq[2];
		$br = widen(self, at3, count2);
	} else if ($bq[0] === 3) {
		const at4 = $bq[1];
		const count3 = $bq[2];
		$br = close(self, at4, count3, wakes);
	} else if ($bq[0] === 2) {
		const at5 = $bq[1];
		let $bx = null;
		if (at5 < self[6]) {
			const $bw = seat(self, at5);
			const place = $bw[0];
			const index = $bw[1];
			const slot2 = __at(__at(self[0], as_usize(__at(self[1], place, "std/src/reactive/lens.vl:339:29")), "std/src/reactive/lens.vl:339:17"), index, "std/src/reactive/lens.vl:339:17");
			if (slot2 !== -(1)) {
				wakes.push([ 0, slot2 ]);
			}
			$bx = undefined;
		}
		$br = $bx;
	} else if ($bq[0] === 4) {
		const at6 = $bq[1];
		const part2 = $bq[2];
		let $bz = null;
		if (at6 < self[6]) {
			const $by = seat(self, at6);
			const place2 = $by[0];
			const index2 = $by[1];
			const slot3 = __at(__at(self[0], as_usize(__at(self[1], place2, "std/src/reactive/lens.vl:348:29")), "std/src/reactive/lens.vl:348:17"), index2, "std/src/reactive/lens.vl:348:17");
			if (slot3 !== -(1)) {
				wakes.push([ 1, slot3, part2 ]);
			}
			$bz = undefined;
		}
		$br = $bz;
	} else if ($bq[0] === 5) {
		const from = $bq[1];
		const to = $bq[2];
		if (from >= self[6] && to >= self[6]) {
			return;
		}
		let $bA = null;
		if (from > to) {
			$bA = from;
		} else {
			$bA = to;
		}
		const far = $bA;
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
		const $bB = seat(self, from);
		const left = $bB[0];
		const index3 = $bB[1];
		const source = __at(self[1], left, "std/src/reactive/lens.vl:372:18");
		const slot4 = __remove_at(__at(self[0], as_usize(source), "std/src/reactive/lens.vl:373:16"), index3, "std/src/reactive/lens.vl:373:47");
		self[6] = self[6] - 1;
		if (__at(self[0], as_usize(source), "std/src/reactive/lens.vl:375:8").length === 0 && self[1].length > 1) {
			__remove_at(self[1], left, "std/src/reactive/lens.vl:376:17");
		} else {
			renumber(self, source, index3);
		}
		restart(self);
		let $bC = null;
		if (to >= self[6]) {
			const last3 = self[1].length - 1;
			$bC = [ last3, __at(self[0], as_usize(__at(self[1], last3, "std/src/reactive/lens.vl:384:25")), "std/src/reactive/lens.vl:384:13").length ];
		} else {
			$bC = seat(self, to);
		}
		const $bD = $bC;
		const place3 = $bD[0];
		const landing = $bD[1];
		const target = __at(self[1], place3, "std/src/reactive/lens.vl:388:18");
		__insert_at(__at(self[0], as_usize(target), "std/src/reactive/lens.vl:389:5"), landing, slot4, "std/src/reactive/lens.vl:389:36");
		self[6] = self[6] + 1;
		renumber(self, target, landing);
		restart(self);
		$br = undefined;
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
		$br = undefined;
	}
	return $br;
}
function follows_items(self) {
	return true;
}
function release(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:418:15");
	if (block === -(1)) {
		return;
	}
	__at_put(__at(self[0], as_usize(block), "std/src/reactive/lens.vl:422:3"), __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:422:33"), -(1), "std/src/reactive/lens.vl:422:3");
	__at_put(self[4], as_usize(slot2), -(1), "std/src/reactive/lens.vl:423:3");
}
function locate(self, slot2) {
	const block = __at(self[4], as_usize(slot2), "std/src/reactive/lens.vl:427:15");
	if (block === -(1)) {
		return [ 1 ];
	}
	return [ 0, __at(self[3], __at(self[2], as_usize(block), "std/src/reactive/lens.vl:431:20"), "std/src/reactive/lens.vl:431:8") + __at(self[5], as_usize(slot2), "std/src/reactive/lens.vl:431:51") ];
}
function fresh_cells() {
	return [ new5(0, "std/src/reactive/lens.vl:888:33"), __shared_new([  ]), __shared_new(0) ];
}
function wake_all(cells, slot2, $bJ) {
	const held = viewing2(cells, (list2) => {
		return __clone(__at(list2, as_usize(slot2), "std/src/reactive/lens.vl:928:35"));
	});
	notify(held[0], $bJ);
	for (const part2 of __clone(held[1].v)) {
		notify(part2[1], $bJ);
	}
}
function wake_part(cells, slot2, name, $bQ) {
	const held = viewing2(cells, (list2) => {
		return __clone(__at(list2, as_usize(slot2), "std/src/reactive/lens.vl:936:35"));
	});
	notify(held[0], $bQ);
	for (const part2 of __clone(held[1].v)) {
		if (part2[0] === name) {
			notify(part2[1], $bQ);
		}
	}
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function item(self, at2) {
	return __list_get(self, at2);
}
function lend(self, at2, read) {
	if (at2 < self.length) {
		read(__at(self, at2, "std/src/reactive/lens.vl:2008:10"));
	}
}
function put(self, at2, value) {
	__at_put(self, at2, __clone(value), "std/src/reactive/lens.vl:2003:3");
}
function change(self, at2, with2) {
	if (at2 < self.length) {
		with2(__at(self, at2, "std/src/reactive/lens.vl:2014:14"));
	}
}
function new5(value, caller) {
	let subscribers = [  ];
	const cell2 = [ __shared_new(value), __shared_new(subscribers) ];
	if (tracing.v) {
		trace_hooks.v[0](__shared_identity(cell2[0]), caller);
	}
	return cell2;
}
function over(value, handler, access) {
	return [ [ 0, __shared_new(value) ], __shared_new(handler), __clone(access), __shared_new([  ]), new5(0, "std/src/reactive/lens.vl:647:28"), new5(0, "std/src/reactive/lens.vl:648:24"), new5(0, "std/src/reactive/lens.vl:649:24"), __shared_new([  ]), __shared_new(0), __shared_new([ 1 ]) ];
}
function sequence(value, $a) {
	return over(value, new4(), [ (held, at2) => {
		return item(held, at2);
	}, (held, at2, read) => {
		return lend(held, at2, read);
	}, (held, at2, value2) => {
		return put(held, at2, value2, $a);
	}, (held, at2, with2) => {
		return change(held, at2, with2, $a);
	} ]);
}
function lend2(value, read) {
	return read(value);
}
function viewing(cell2, read) {
	return lend2(cell2.v, read);
}
function viewing2(cell2, read) {
	return lend2(cell2.v, read);
}
function peek_storage(storage, read) {
	const $c = storage;
	let $d = null;
	if ($c[0] === 0) {
		const cell2 = $c[1];
		$d = viewing2(cell2, read);
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
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$d = $f;
	}
	return $d;
}
function holds(storage, view, at2) {
	let found = false;
	peek_storage(storage, (held) => {
		return view(held, at2, (_item) => {
			return found = true;
		});
	});
	return found;
}
function follow_id(storage, handler, cells, view, at2) {
	if (viewing(handler, (held) => {
		return follows_items(held);
	}) && !(holds(storage, view, at2))) {
		return -(1);
	}
	const id = follow(handler.v, at2);
	while (cells.v.length <= as_usize(id)) {
		cells.v.push(fresh_cells());
	}
	const slot_cells = viewing2(cells, (held) => {
		return __clone(__at(held, as_usize(id), "std/src/reactive/lens.vl:909:41"));
	});
	const $k = [ slot_cells[2], "v" ];
	slot_cells[2].v = $k[0][$k[1]] + 1;
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
function follow2(self, at2) {
	return slot_of(self, follow_id(self[0], self[1], self[3], self[2][1], at2));
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
		release(handler.v, id);
	}
}
function release2(self) {
	release_id(self[0][1], self[0][3], self[1]);
}
function slot(self, at2, $b) {
	const handle = follow2(self, at2);
	defer_to_owner(() => {
		return release2(handle);
	}, $b);
	return handle;
}
function part(self, name, read) {
	for (const held of self[2][1].v) {
		if (held[0] === name) {
			return [ __clone(self), __clone(held[1]), read ];
		}
	}
	const woke = new5(0, "std/src/reactive/lens.vl:1081:26");
	self[2][1].v.push([ name, __clone(woke) ]);
	return [ __clone(self), woke, read ];
}
function identity(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function identity2(self) {
	return identity(self[1]);
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
	return on_settle(self[1], subscriber);
}
function at(self) {
	if (self[1] < 0) {
		return [ 1 ];
	}
	return viewing2(self[0][1], (handler) => {
		return locate(handler, self[1]);
	});
}
function peek(self, read) {
	return peek_storage(self[0], read);
}
function get(self) {
	const $C = at(self[0]);
	if ($C[0] === 0) {

	} else {
		return [ 1 ];
	}
	let out = [ 1 ];
	const view = self[0][0][2][1];
	const read = self[2];
	peek(self[0][0], (held) => {
		return view(held, $C[1], (item2) => {
			return out = [ 0, read(item2) ];
		});
	});
	return out;
}
function track(self, $t) {
	const scope = $t;
	const identity3 = identity2(self);
	if (admits(scope, identity3)) {
		lists_of(scope[0]).v[0].push([ identity3, (subscriber) => {
			return on_settle2(self, subscriber);
		} ]);
	}
	return get(self);
}
function unwrap_or(self, fallback) {
	const $D = self;
	let $E = null;
	if ($D[0] === 0) {
		const x = $D[1];
		$E = x;
	} else {
		$E = __clone(__force(fallback));
	}
	return $E;
}
function derive(body, caller) {
	return [ body, caller ];
}
function is_empty(self) {
	return self.length === 0;
}
function is_none(self) {
	const $W = self;
	return $W[0] === 1;
}
function run_body(runs2, body) {
	const run = renew(runs2);
	const $af = nursery(run);
	let $ag = null;
	if ($af[0] === 0) {
		const nursery2 = $af[1];
		$ag = (($ah) => {
			return (($ai) => {
				return body($ah, $ai);
			})(nursery2);
		})(run);
	} else {
		$ag = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1405:11");
		})();
	}
	return $ag;
}
function last(self) {
	let $ar = null;
	if (is_empty(self)) {
		$ar = [ 1 ];
	} else {
		$ar = __list_get(self, self.length - 1);
	}
	return $ar;
}
function run_once(runs2, tracker, body) {
	const scope = open_run(tracker);
	const value = run_body(runs2, ($O, $P) => {
		return (($Q) => {
			return body($O, $Q, $P);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function run_tracked(runs2, tracker, body) {
	let value = run_once(runs2, tracker, ($J, $K, $L) => {
		return body($J, $K, $L);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = run_once(runs2, tracker, ($aI, $aJ, $aK) => {
			return body($aI, $aJ, $aK);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function start(self) {
	const body = self[0];
	const runs2 = new3();
	const tracker = new_tracker([ 0, __clone(self[1]) ]);
	return [ () => {
		return run_tracked(runs2, tracker, body);
	}, (subscriber) => {
		attach_tracker(tracker, subscriber);
		return retiring(subscriber, () => {
			return detach_tracker(tracker);
		});
	}, () => {
		detach_tracker(tracker);
		release_runs(runs2);
		return;
	} ];
}
function notify(self, $aS) {
	const traced = tracing.v;
	let $aT = null;
	if (traced) {
		$aT = trace_hooks.v[2](__shared_identity(self[0]));
	} else {
		$aT = "";
	}
	const cause = $aT;
	const $aU = $aS;
	let $aV = null;
	if ($aU[0] === 0) {
		const turn = $aU[1];
		$aV = queue_wakes(turn, __clone(self[1].v), traced, cause);
	} else {
		const $aW = last(draining_turns.v);
		let $aX = null;
		if ($aW[0] === 0) {
			const draining = $aW[1];
			$aX = queue_wakes(draining, __clone(self[1].v), traced, cause);
		} else {
			for (const subscriber of __clone(self[1].v)) {
				let $aY = null;
				if (subscriber[2].v) {
					if (traced) {
						trace_hooks.v[5](subscriber, cause);
					} else {
						subscriber[1]();
					}
					$aY = undefined;
				}
				$aY;
			}
			$aX = undefined;
		}
		$aV = $aX;
	}
	return $aV;
}
function set(self, value, $aR, caller) {
	if (tracing.v) {
		trace_hooks.v[1](caller);
	}
	self[0].v = __clone(value);
	notify(self, $aR);
}
function take(self, item2, $bf) {
	defer(self, () => {
		dispose(item2, $bf);
		return;
	});
	return __clone(item2);
}
function cell(self, $H, $I, caller) {
	const instance = start(self);
	const pull = instance[0];
	const cached = new5(pull(), caller);
	const refresh = subscriber_of(() => {
		set(cached, pull(), $H, "std/src/reactive.vl:2422:11");
		return;
	}, true);
	place_subscriber(refresh[0], caller);
	const refreshed = instance[1](refresh);
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $H, $I);
	return cached;
}
function memo(self, $F, $G, caller) {
	return [ cell(self, $F, $G, caller) ];
}
function get2(self) {
	return __clone(self[0].v);
}
function get3(self) {
	return get2(self[0]);
}
function has(self, at2) {
	return holds(self[0], self[2][1], at2);
}
function coming(self, changes) {
	if (self[7].v.length === 0) {
		return;
	}
	for (const tap of __clone(self[7].v)) {
		for (const change2 of changes) {
			tap[1](change2);
		}
	}
}
function mutate(self, change2) {
	const $bl = self[0];
	let $bm = null;
	if ($bl[0] === 0) {
		const cell2 = $bl[1];
		$bm = change2(cell2.v);
	} else {
		const port = $bl[1];
		$bm = port[1](change2);
	}
	return $bm;
}
function batch(body, $bW) {
	const $bX = $bW;
	let $bY = null;
	if ($bX[0] === 0) {
		const current = $bX[1];
		$bY = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$bY = result;
	}
	return $bY;
}
function landed(self, changes, late, $bn) {
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
		const $bo = change2;
		let $bp = null;
		if ($bo[0] === 0) {
			const _at = $bo[1];
			const removed = $bo[2];
			const inserted = $bo[3];
			shaped = true;
			if (removed !== inserted) {
				resized = true;
			}
			$bp = undefined;
		} else if ($bo[0] === 1) {
			const _at2 = $bo[1];
			const _count = $bo[2];
			$bp = resized = true;
		} else if ($bo[0] === 3) {
			const _at3 = $bo[1];
			const _count2 = $bo[2];
			$bp = resized = true;
		} else if ($bo[0] === 6) {
			$bp = resized = true;
		} else if ($bo[0] === 5) {
			const _from = $bo[1];
			const _to = $bo[2];
			$bp = shaped = true;
		} else {
			$bp = undefined;
		}
		$bp;
		apply(self[1].v, change2, wakes);
		let $bF = null;
		if (tapped) {
			let $bE = null;
			if (!(late)) {
				$bE = [ 0 ];
			} else if (!(several)) {
				$bE = [ 1 ];
			} else if (index === 0) {
				$bE = [ 2 ];
			} else {
				$bE = [ 3 ];
			}
			const stage = $bE;
			for (const tap of __clone(self[7].v)) {
				tap[2](change2, stage);
			}
			$bF = undefined;
		}
		$bF;
		index = index + 1;
	}
	const cells = self[3];
	const published = __clone(self[4]);
	const shape = __clone(self[5]);
	const sized = __clone(self[6]);
	settle_together(($bG) => {
		for (const wake2 of wakes) {
			const $bH = wake2;
			let $bI = null;
			if ($bH[0] === 0) {
				const slot2 = $bH[1];
				$bI = wake_all(cells, slot2, [ 0, $bG ]);
			} else if ($bH[0] === 2) {
				const slot3 = $bH[1];
				$bI = wake_all(cells, slot3, [ 0, $bG ]);
			} else {
				const slot4 = $bH[1];
				const part2 = $bH[2];
				$bI = wake_part(cells, slot4, part2, [ 0, $bG ]);
			}
			$bI;
		}
		if (resized) {
			notify(sized, [ 0, $bG ]);
		}
		if (shaped || resized) {
			notify(shape, [ 0, $bG ]);
		}
		notify(published, [ 0, $bG ]);
		return;
	}, $bn);
}
function edit(self, at2, part2, change2, $bk) {
	if (!(has(self, at2))) {
		return false;
	}
	coming(self, [ [ 4, __clone(at2), part2 ] ]);
	const modify = self[2][3];
	mutate(self, (held) => {
		return modify(held, at2, change2);
	});
	landed(self, [ [ 4, __clone(at2), part2 ] ], false, $bk);
	return true;
}
function count(self) {
	return self.length;
}
function peek_storage2(storage, read) {
	const $cb = storage;
	let $cc = null;
	if ($cb[0] === 0) {
		const cell2 = $cb[1];
		$cc = viewing2(cell2, read);
	} else {
		const port = $cb[1];
		let out = [ 1 ];
		port[0]((held) => {
			return out = [ 0, read(held) ];
		});
		const $cd = out;
		let $ce = null;
		if ($cd[0] === 0) {
			const value = __clone($cd[1]);
			$ce = value;
		} else {
			$ce = (() => {
				throw __panic("lens: a nested lens\'s port lent nothing", "std/src/reactive/lens.vl:632:13");
			})();
		}
		$cc = $ce;
	}
	return $cc;
}
function peek2(self, read) {
	return peek_storage2(self[0], read);
}
function size(self) {
	return peek2(self, (held) => {
		return count(held);
	});
}
function splice(self, at2, removed, inserted) {
	let taken = 0;
	while (taken < removed) {
		__remove_at(self, at2, "std/src/reactive/lens.vl:1992:9");
		taken = taken + 1;
	}
	let offset = 0;
	for (const value of inserted) {
		__insert_at(self, at2 + offset, __clone(value), "std/src/reactive/lens.vl:1997:9");
		offset = offset + 1;
	}
}
function splice2(self, at2, removed, inserted, $ca) {
	const held = size(self);
	let $cf = null;
	if (at2 > held) {
		$cf = held;
	} else {
		$cf = at2;
	}
	const start2 = $cf;
	let $cg = null;
	if (start2 + removed > held) {
		$cg = held - start2;
	} else {
		$cg = removed;
	}
	const taking = $cg;
	const arriving = inserted.length;
	if (taking === 0 && arriving === 0) {
		return;
	}
	const change2 = [ 0, start2, taking, arriving ];
	coming(self, [ __clone(change2) ]);
	mutate(self, (held2) => {
		return splice(held2, start2, taking, inserted);
	});
	landed(self, [ change2 ], false, $ca);
}
function insert_at(self, at2, value, $bZ) {
	return splice2(self, at2, 0, [ __clone(value) ], $bZ);
}
function edit2(self, part2, change2, $ch) {
	const $ci = at(self);
	if ($ci[0] === 0) {

	} else {
		return false;
	}
	return edit(self[0], $ci[1], part2, change2, $ch);
}
function remove_at(self, at2, $cj) {
	return splice2(self, at2, 1, [  ], $cj);
}
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
const run_nurseries_allocated_count = __shared_new(0);
const trackers_allocated_count = __shared_new(0);
const block_size = 128;
const list = sequence([ [ "a", "open" ], [ "b", "open" ], [ "c", "open" ] ], [ 1 ]);
const second = slot(list, 1, [ 1 ]);
const title = part(second, "title", (task) => {
	return task[0];
});
const status = part(second, "status", (task) => {
	return task[1];
});
const runs = __shared_new(0);
const label = memo(derive(($p, $q, $r) => {
	const $s = [ runs, "v" ];
	runs.v = $s[0][$s[1]] + 1;
	return "" + unwrap_or(track(title, $q), __lazy("fallback", () => {
		return "?";
	})) + " [" + unwrap_or(track(status, $q), __lazy("fallback", () => {
		return "?";
	})) + "]";
}, "lens-tracked.vl:24:14"), [ 1 ], [ 1 ], "lens-tracked.vl:27:5");
assert(get3(label) === "b [open]", "the tracked body read both parts", "lens-tracked.vl:28:2");
const seeded = runs.v;
edit(list, 0, "title", (task) => {
	return task[0] = "A";
}, [ 1 ]);
insert_at(list, 0, [ "new", "open" ], [ 1 ]);
assert(get3(label) === "b [open]" && runs.v === seeded, "another row\'s edit and an insert above do not re-run it", "lens-tracked.vl:33:2");
edit2(second, "status", (task) => {
	return task[1] = "done";
}, [ 1 ]);
assert(get3(label) === "b [done]" && runs.v === seeded + 1, "one part\'s edit re-runs it once", "lens-tracked.vl:39:2");
remove_at(list, 2, [ 1 ]);
assert(get3(label) === "? [?]" && runs.v === seeded + 2, "the row\'s deletion re-runs it once", "lens-tracked.vl:45:2");
console.log("tracked: all passed");
//# sourceMappingURL=lens-tracked.mjs.map
