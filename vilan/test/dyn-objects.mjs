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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $E(self[0].v) && $E(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $aq = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$aq = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$aq;
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
				while (!($E(turn[1].v)) && budget > 0) {
					const derivations = turn[1].v;
					turn[1].v = [  ];
					turn[3].v = new Map();
					for (const subscriber of derivations) {
						if (subscriber[2].v) {
							subscriber[1]();
						}
						budget = budget - 1;
					}
				}
				const wave = turn[0].v;
				turn[0].v = [  ];
				turn[2].v = new Map();
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
			turn[4].v = false;
			return;
		});
	}
}
function defer_subscriber(turn, subscriber) {
	const $ao = turn;
	let $ap = null;
	if ($ao[0] === 0) {
		const ambient = $ao[1];
		$ap = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $av = $as(draining_turns.v);
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
		$ap = $aw;
	}
	return $ap;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $bi) {
	const $bj = $bi;
	let $bk = null;
	if ($bj[0] === 0) {
		const established = $bj[1];
		$bk = [ 0, established ];
	} else {
		$bk = $as(draining_turns.v);
	}
	const ambient = $bk;
	release_under(self, ambient);
}
function detach(handle) {
	const $aA = $as(releasing_turns.v);
	let $aB = null;
	if ($aA[0] === 0) {
		const at_release = $aA[1];
		$aB = at_release;
	} else {
		$aB = $as(draining_turns.v);
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
		$aF = undefined;
	} else {
		$aF = undefined;
	}
	$aF;
	const $aG = handle[3].v;
	let $aH = null;
	if ($aG[0] === 0) {
		const release = $aG[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aH = undefined;
	} else {
		$aH = undefined;
	}
	return $aH;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bl = null;
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
		$bl = undefined;
	}
	return $bl;
}
function renew(self) {
	const $J = self[0].v[3];
	let $K = null;
	if ($J[0] === 0) {
		const nursery2 = __clone($J[1]);
		let $L = null;
		if (has_spawned(nursery2)) {
			$L = [ 1 ];
		} else {
			$L = [ 0, nursery2 ];
		}
		$K = $L;
	} else {
		$K = [ 1 ];
	}
	const carried = $K;
	advance([ self[0], self[0].v[0] ], carried);
	if ($O(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $Y = null;
	if (is_disposed(self)) {
		$Y = [ 1 ];
	} else {
		$Y = self[0].v[3];
	}
	return $Y;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $X = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $M = held[3];
		let $N = null;
		if ($M[0] === 0) {
			const nursery2 = $M[1];
			if ($O(carried)) {
				nursery2.cancel();
			}
			$N = undefined;
		} else {
			$N = undefined;
		}
		$N;
		let $W = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $Q = __guarded(cleanup);
				let $R = null;
				if ($Q[0] === 0) {
					const message = $Q[1];
					if ($O(failure)) {
						failure = [ 0, message ];
					}
					$R = undefined;
				} else {
					$R = undefined;
				}
				$R;
			}
			const $U = failure;
			let $V = null;
			if ($U[0] === 0) {
				const message2 = $U[1];
				$V = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$V = undefined;
			}
			$W = $V;
		}
		$X = $W;
	}
	return $X;
}
function get_owner($bf) {
	return $bf;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ] ]) ];
}
function has_run(tracker) {
	return tracker[0].v[0] > 0;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $C = tracker[0].v[6];
	let $D = null;
	if ($C[0] === 0) {
		const lists = $C[1];
		if (!($E(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$D = undefined;
	} else {
		$D = undefined;
	}
	$D;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $ad = tracker[0].v[6];
	let $ae = null;
	if ($ad[0] === 0) {
		const lists = $ad[1];
		$ae = lists;
	} else {
		return;
		$ae = undefined;
	}
	const lists2 = $ae;
	const $af = tracker[0].v[5];
	let $ag = null;
	if ($af[0] === 0) {
		const target = __clone($af[1]);
		$ag = reconnect(tracker, lists2, target);
	} else {
		if (!($E(lists2.v[0])) || !($E(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$ag = undefined;
	}
	$ag;
	if (!($E(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($E(lists.v[0]) && $E(lists.v[2])) {
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
		const $am = reusable(held, kept, dependency[0], position);
		let $an = null;
		if ($am[0] === 0) {
			const index = $am[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1624:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1625:15")));
			$an = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$an = undefined;
		}
		$an;
		position = position + 1;
	}
	lists.v[2] = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2, "std/src/reactive.vl:1639:7"))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $ai = identity;
	let $aj = null;
	if ($ai[0] === 0) {
		const wanted = $ai[1];
		if (position < held.length && !(__at(kept, position, "std/src/reactive.vl:1662:9")) && same_identity(__at(held, position, "std/src/reactive.vl:1663:22")[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index, "std/src/reactive.vl:1668:9")) && same_identity(__at(held, index, "std/src/reactive.vl:1668:38")[0], wanted)) {
				return [ 0, index ];
			}
			index = index + 1;
		}
		$aj = [ 1 ];
	} else {
		$aj = [ 1 ];
	}
	return $aj;
}
function same_identity(identity, wanted) {
	const $ak = identity;
	let $al = null;
	if ($ak[0] === 0) {
		const held = $ak[1];
		$al = held === wanted;
	} else {
		$al = false;
	}
	return $al;
}
function relay_for(tracker, target) {
	return subscriber_of(() => {
		const connecting = tracker[0].v[3];
		tracker[0].v[2] = true;
		if (connecting) {
			tracker[0].v[4] = true;
		} else {
			wake(target);
		}
		return;
	}, true);
}
function attach_tracker(tracker, target) {
	tracker[0].v[5] = [ 0, __clone(target) ];
	const $aS = tracker[0].v[6];
	let $aT = null;
	if ($aS[0] === 0) {
		const lists = $aS[1];
		$aT = lists;
	} else {
		return;
		$aT = undefined;
	}
	const lists2 = $aT;
	let $aU = null;
	if (!($E(lists2.v[1]))) {
		const read = __clone(lists2.v[1]);
		lists2.v[1] = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		lists2.v[2] = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aU = undefined;
	}
	return $aU;
}
function forget_reads(tracker) {
	const $ba = tracker[0].v[6];
	let $bb = null;
	if ($ba[0] === 0) {
		const lists = $ba[1];
		$bb = lists;
	} else {
		return;
		$bb = undefined;
	}
	const lists2 = $bb;
	let $bc = null;
	if (!($E(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$bc = undefined;
	}
	$bc;
	if (!($E(lists2.v[1]))) {
		lists2.v[1] = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[0].v[5] = [ 1 ];
	forget_reads(tracker);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $bd = previous;
		let $be = null;
		if ($bd[0] === 0) {
			const earlier = $bd[1];
			$be = earlier();
		} else {
			$be = undefined;
		}
		return $be;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function name(self) {
	return "square";
}
function area(self) {
	return self[0] * self[0];
}
function describe(self, prefix) {
	return "" + prefix + name(self) + " " + self[0];
}
function name2(self) {
	return "rect";
}
function area2(self) {
	return self[0] * self[1];
}
function describe2(self, prefix) {
	return "" + prefix + name2(self) + " " + self[0] + "x" + self[1];
}
function get(self) {
	return self[0][1].get(self[0][0]) + self[1];
}
function on_settle(self, subscriber) {
	return self[0][1].on_settle(self[0][0], subscriber);
}
function total(shapes2) {
	let sum = 0;
	for (const shape of shapes2) {
		sum = sum + shape[1].area(shape[0]);
	}
	return sum;
}
const $a = Object.create({area: area, describe: describe, name: name});
const $b = Object.create({area: area2, describe: describe2, name: name2});
function $c(self) {
	return self[1].area(self[0]) * 2;
}
function $d(shapes2) {
	let best = 0;
	for (const shape of shapes2) {
		if (shape[1].area(shape[0]) > best) {
			best = shape[1].area(shape[0]);
		}
	}
	return best;
}
function $e(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $g(self) {
	return __clone(self[0].v);
}
function $i(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $h(self, subscriber) {
	return $i(self, subscriber);
}
function $k(signal, observer) {
	const cell = signal[0];
	return $i(signal, mint_subscriber(() => {
		const $l = [ 0, cell ];
		let $m = null;
		if ($l[0] === 0) {
			const live = $l[1];
			$m = observer(live.v);
		} else {
			$m = undefined;
		}
		return $m;
	}));
}
function $j(self, observer, immediately) {
	const subscription = $k(self, observer);
	if (immediately) {
		observer($g(self));
	}
	return subscription;
}
function $n(self) {
	return [ 0, __shared_identity(self[0]) ];
}
function $o(self) {
	return [ () => {
		return $g(self);
	}, (subscriber) => {
		return $h(self, subscriber);
	}, () => {
		return;
	} ];
}
function $p(self, observer) {
	return $j(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $E(self) {
	return self.length === 0;
}
function $O(self) {
	const $P = self;
	return $P[0] === 1;
}
function $I(runs, body) {
	const run = renew(runs);
	const $Z = nursery(run);
	let $aa = null;
	if ($Z[0] === 0) {
		const nursery2 = $Z[1];
		$aa = (($ab) => {
			return (($ac) => {
				return body($ab, $ac);
			})(nursery2);
		})(run);
	} else {
		$aa = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $aa;
}
function $as(self) {
	let $au = null;
	if ($E(self)) {
		$au = [ 1 ];
	} else {
		$au = __list_get(self, self.length - 1);
	}
	return $au;
}
function $B(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $I(runs, ($F, $G) => {
		return (($H) => {
			return body($F, $H, $G);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $x(runs, tracker, body) {
	let value = $B(runs, tracker, ($y, $z, $A) => {
		return body($y, $z, $A);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $B(runs, tracker, ($aI, $aJ, $aK) => {
			return body($aI, $aJ, $aK);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aL(self, observer) {
	return $j(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $bg(self, item, $bh) {
	defer(self, () => {
		dispose(item, $bh);
		return;
	});
	return __clone(item);
}
function $q(self, body, $r, $s) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $aL(self, (value2, $t) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$x(runs, tracker, ($u, $v, $w) => {
				return body(value2, $u, $v, $w);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aR = null;
		if (tracker[0].v[2]) {
			const $aM = latest;
			let $aN = null;
			if ($aM[0] === 0) {
				const value2 = __clone($aM[1]);
				$aN = $x(runs, tracker, ($aO, $aP, $aQ) => {
					return body(value2, $aO, $aP, $aQ);
				});
			} else {
				$aN = undefined;
			}
			$aR = $aN;
		}
		return $aR;
	}, false);
	attach_tracker(tracker, rerun);
	const $aV = latest;
	let $aW = null;
	if ($aV[0] === 0) {
		const value = __clone($aV[1]);
		$aW = $x(runs, tracker, ($aX, $aY, $aZ) => {
			return body(value, $aX, $aY, $aZ);
		});
	} else {
		$aW = undefined;
	}
	$aW;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bg(get_owner($s), subscription, $r);
}
const $f = Object.create({get: $g, on_settle: $h, attach_observer: $j, identity: $n, start: $o, on_change: $p, effect: $q});
function $bn(self, observer, immediately) {
	const subscription = on_settle(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function $bo(self) {
	return [ 1 ];
}
function $bp(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bq(self, observer) {
	return $bn(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $bs(self, observer) {
	return $bn(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $br(self, body, $r, $s) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $bs(self, (value2, $t) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$x(runs, tracker, ($u, $v, $w) => {
				return body(value2, $u, $v, $w);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $bv = null;
		if (tracker[0].v[2]) {
			const $bt = latest;
			let $bu = null;
			if ($bt[0] === 0) {
				const value2 = __clone($bt[1]);
				$bu = $x(runs, tracker, ($aO, $aP, $aQ) => {
					return body(value2, $aO, $aP, $aQ);
				});
			} else {
				$bu = undefined;
			}
			$bv = $bu;
		}
		return $bv;
	}, false);
	attach_tracker(tracker, rerun);
	const $bw = latest;
	let $bx = null;
	if ($bw[0] === 0) {
		const value = __clone($bw[1]);
		$bx = $x(runs, tracker, ($aX, $aY, $aZ) => {
			return body(value, $aX, $aY, $aZ);
		});
	} else {
		$bx = undefined;
	}
	$bx;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bg(get_owner($s), subscription, $r);
}
const $bm = Object.create({get: get, on_settle: on_settle, attach_observer: $bn, identity: $bo, start: $bp, on_change: $bq, effect: $br});
function $bC(self, $bD) {
	const $bE = $bD;
	let $bF = null;
	if ($bE[0] === 0) {
		const turn = $bE[1];
		$bF = enqueue(turn, __clone(self[1].v));
	} else {
		const $bG = $as(draining_turns.v);
		let $bH = null;
		if ($bG[0] === 0) {
			const draining = $bG[1];
			$bH = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bH = undefined;
		}
		$bF = $bH;
	}
	return $bF;
}
function $bA(self, value, $bB) {
	self[0].v = __clone(value);
	$bC(self, $bB);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const run_nurseries_allocated_count = __shared_new(0);
const slots = [ [ [ [ 3 ], $a ] ], [ [ [ 2, 5 ], $b ] ] ];
for (const slot of slots) {
	console.log(slot[0][1].describe(slot[0][0], "- "));
	console.log("" + slot[0][1].name(slot[0][0]) + " " + slot[0][1].area(slot[0][0]) + " " + $c(slot[0]));
}
const shapes = [ [ [ 4, 4 ], $b ], [ [ 2 ], $a ] ];
console.log(total(shapes));
console.log($d(shapes));
console.log(__at(slots, 0, "dyn-objects.vl:111:8"));
const root = $e(1);
const sources = [ __clone([ root, $f ]), [ [ __clone([ root, $f ]), 100 ], $bm ] ];
const watch = (($bz) => {
	return $bz[1].on_change($bz[0], (n, $by) => {
		return console.log("offset saw " + n);
	});
})(__clone(__at(sources, 1, "dyn-objects.vl:115:14")));
$bA(root, 5, [ 1 ]);
for (const source of sources) {
	console.log(source[1].get(source[0]));
}
dispose(watch, [ 1 ]);
