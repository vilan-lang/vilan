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
	return $t(self[0].v) && $t(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $s = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$s = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$s;
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
				while (!($t(turn[1].v)) && budget > 0) {
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
	const $bs = turn;
	let $bt = null;
	if ($bs[0] === 0) {
		const ambient = $bs[1];
		$bt = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bu = $u(draining_turns.v);
		let $bv = null;
		if ($bu[0] === 0) {
			const draining = $bu[1];
			$bv = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bv = undefined;
		}
		$bt = $bv;
	}
	return $bt;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $z) {
	const $A = $z;
	let $B = null;
	if ($A[0] === 0) {
		const established = $A[1];
		$B = [ 0, established ];
	} else {
		$B = $u(draining_turns.v);
	}
	const ambient = $B;
	release_under(self, ambient);
}
function detach(handle) {
	const $bz = $u(releasing_turns.v);
	let $bA = null;
	if ($bz[0] === 0) {
		const at_release = $bz[1];
		$bA = at_release;
	} else {
		$bA = $u(draining_turns.v);
	}
	const turn = $bA;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $C = [ 0, handle[0] ];
	let $D = null;
	if ($C[0] === 0) {
		const subscribers = $C[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$D = undefined;
	} else {
		$D = undefined;
	}
	$D;
	const $E = ambient;
	let $F = null;
	if ($E[0] === 0) {
		const turn = $E[1];
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
		$F = undefined;
	} else {
		$F = undefined;
	}
	$F;
	const $G = handle[3].v;
	let $H = null;
	if ($G[0] === 0) {
		const release = $G[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$H = undefined;
	} else {
		$H = undefined;
	}
	return $H;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $as = null;
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
		$as = undefined;
	}
	return $as;
}
function renew(self) {
	const $T = self[0].v[3];
	let $U = null;
	if ($T[0] === 0) {
		const nursery2 = __clone($T[1]);
		let $V = null;
		if (has_spawned(nursery2)) {
			$V = [ 1 ];
		} else {
			$V = [ 0, nursery2 ];
		}
		$U = $V;
	} else {
		$U = [ 1 ];
	}
	const carried = $U;
	advance([ self[0], self[0].v[0] ], carried);
	if ($Y(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $ai = null;
	if (is_disposed(self)) {
		$ai = [ 1 ];
	} else {
		$ai = self[0].v[3];
	}
	return $ai;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $ah = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $W = held[3];
		let $X = null;
		if ($W[0] === 0) {
			const nursery2 = $W[1];
			if ($Y(carried)) {
				nursery2.cancel();
			}
			$X = undefined;
		} else {
			$X = undefined;
		}
		$X;
		let $ag = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $aa = __guarded(cleanup);
				let $ab = null;
				if ($aa[0] === 0) {
					const message = $aa[1];
					if ($Y(failure)) {
						failure = [ 0, message ];
					}
					$ab = undefined;
				} else {
					$ab = undefined;
				}
				$ab;
			}
			const $ae = failure;
			let $af = null;
			if ($ae[0] === 0) {
				const message2 = $ae[1];
				$af = (() => {
					throw __panic(message2, "std/src/reactive.vl:1207:27");
				})();
			} else {
				$af = undefined;
			}
			$ag = $af;
		}
		$ah = $ag;
	}
	return $ah;
}
function get_owner($ap) {
	return $ap;
}
function register_with_owner(subscription, $bR, $bS) {
	const $bT = $bS;
	let $bU = null;
	if ($bT[0] === 0) {
		const owner = $bT[1];
		$bU = $aq(owner, subscription, $bR);
	} else {
		$bU = __clone(subscription);
	}
	return $bU;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false, [ 1 ], [ 1 ] ]) ];
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	const $aY = tracker[0].v[6];
	let $aZ = null;
	if ($aY[0] === 0) {
		const lists = $aY[1];
		if (!($t(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$aZ = undefined;
	} else {
		$aZ = undefined;
	}
	$aZ;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $bh = tracker[0].v[6];
	let $bi = null;
	if ($bh[0] === 0) {
		const lists = $bh[1];
		$bi = lists;
	} else {
		return;
		$bi = undefined;
	}
	const lists2 = $bi;
	const $bj = tracker[0].v[5];
	let $bk = null;
	if ($bj[0] === 0) {
		const target = __clone($bj[1]);
		$bk = reconnect(tracker, lists2, target);
	} else {
		if (!($t(lists2.v[0])) || !($t(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$bk = undefined;
	}
	$bk;
	if (!($t(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($t(lists.v[0]) && $t(lists.v[2])) {
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
		const $bq = reusable(held, kept, dependency[0], position);
		let $br = null;
		if ($bq[0] === 0) {
			const index = $bq[1];
			__at_put(kept, index, true, "std/src/reactive.vl:1624:5");
			next.push(__clone(__at(held, index, "std/src/reactive.vl:1625:15")));
			$br = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$br = undefined;
		}
		$br;
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
	const $bm = identity;
	let $bn = null;
	if ($bm[0] === 0) {
		const wanted = $bm[1];
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
		$bn = [ 1 ];
	} else {
		$bn = [ 1 ];
	}
	return $bn;
}
function same_identity(identity, wanted) {
	const $bo = identity;
	let $bp = null;
	if ($bo[0] === 0) {
		const held = $bo[1];
		$bp = held === wanted;
	} else {
		$bp = false;
	}
	return $bp;
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
	const $bE = tracker[0].v[6];
	let $bF = null;
	if ($bE[0] === 0) {
		const lists = $bE[1];
		$bF = lists;
	} else {
		return;
		$bF = undefined;
	}
	const lists2 = $bF;
	let $bG = null;
	if (!($t(lists2.v[1]))) {
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
		$bG = undefined;
	}
	return $bG;
}
function forget_reads(tracker) {
	const $bH = tracker[0].v[6];
	let $bI = null;
	if ($bH[0] === 0) {
		const lists = $bH[1];
		$bI = lists;
	} else {
		return;
		$bI = undefined;
	}
	const lists2 = $bI;
	let $bJ = null;
	if (!($t(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$bJ = undefined;
	}
	$bJ;
	if (!($t(lists2.v[1]))) {
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
		const $an = previous;
		let $ao = null;
		if ($an[0] === 0) {
			const earlier = $an[1];
			$ao = earlier();
		} else {
			$ao = undefined;
		}
		return $ao;
	} ];
}
function has_spawned(self) {
	return __nursery_has_spawned(self);
}
function detached_nursery() {
	return __nursery_new_detached();
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $i(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $f(signal, observer) {
	const cell = signal[0];
	return $i(signal, mint_subscriber(() => {
		const $g = [ 0, cell ];
		let $h = null;
		if ($g[0] === 0) {
			const live = $g[1];
			$h = observer(live.v);
		} else {
			$h = undefined;
		}
		return $h;
	}));
}
function $j(self) {
	return __clone(self[0].v);
}
function $e(self, observer, immediately) {
	const subscription = $f(self, observer);
	if (immediately) {
		observer($j(self));
	}
	return subscription;
}
function $d(self, observer) {
	return $e(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $l(self, observer) {
	return $e(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $t(self) {
	return self.length === 0;
}
function $u(self) {
	let $w = null;
	if ($t(self)) {
		$w = [ 1 ];
	} else {
		$w = __list_get(self, self.length - 1);
	}
	return $w;
}
function $o(self, $p) {
	const $q = $p;
	let $r = null;
	if ($q[0] === 0) {
		const turn = $q[1];
		$r = enqueue(turn, __clone(self[1].v));
	} else {
		const $x = $u(draining_turns.v);
		let $y = null;
		if ($x[0] === 0) {
			const draining = $x[1];
			$y = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$y = undefined;
		}
		$r = $y;
	}
	return $r;
}
function $m(self, value, $n) {
	self[0].v = __clone(value);
	$o(self, $n);
}
function $Y(self) {
	const $Z = self;
	return $Z[0] === 1;
}
function $S(runs, body) {
	const run = renew(runs);
	const $aj = nursery(run);
	let $ak = null;
	if ($aj[0] === 0) {
		const nursery2 = $aj[1];
		$ak = (($al) => {
			return (($am) => {
				return body($al, $am);
			})(nursery2);
		})(run);
	} else {
		$ak = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $ak;
}
function $aq(self, item, $ar) {
	defer(self, () => {
		dispose(item, $ar);
		return;
	});
	return __clone(item);
}
function $M(self, body, $N, $O) {
	const runs = new2();
	const subscription = $l(self, (value, $P) => {
		return $S(runs, ($Q, $R) => {
			return (() => {
				return body(value, $Q, [ 1 ], $R);
			})();
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	$aq(get_owner($O), subscription, $N);
}
function $at(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $ay(self) {
	return $j(self[0]);
}
function $aA(self, subscriber) {
	return $i(self, subscriber);
}
function $az(self, subscriber) {
	return $aA(self[0], subscriber);
}
function $ax(self, observer, immediately) {
	const subscription = $az(self, mint_subscriber(() => {
		return observer($ay(self));
	}));
	if (immediately) {
		observer($ay(self));
	}
	return subscription;
}
function $aw(self, observer) {
	return $ax(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $aC(self, observer) {
	return $ax(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, false);
}
function $aH(self, transform) {
	return [ self, transform ];
}
function $aP(self) {
	return [ () => {
		return $ay(self);
	}, (subscriber) => {
		return $az(self, subscriber);
	}, () => {
		return;
	} ];
}
function $be(runs, body) {
	const run = renew(runs);
	const $bf = nursery(run);
	let $bg = null;
	if ($bf[0] === 0) {
		const nursery2 = $bf[1];
		$bg = (($al) => {
			return (($am) => {
				return body($al, $am);
			})(nursery2);
		})(run);
	} else {
		$bg = (() => {
			throw __panic("a renewed run carries its nursery", "std/src/reactive.vl:1318:11");
		})();
	}
	return $bg;
}
function $aX(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $be(runs, ($bb, $bc) => {
		return (($bd) => {
			return body($bb, $bd, $bc);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $aT(runs, tracker, body) {
	let value = $aX(runs, tracker, ($aU, $aV, $aW) => {
		return body($aU, $aV, $aW);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $aX(runs, tracker, ($bB, $bC, $bD) => {
			return body($bB, $bC, $bD);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aO(self) {
	const transform = self[1];
	const upstream = $aP(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $aT(runs, tracker, ($aQ, $aR, $aS) => {
			return transform(value, $aQ, $aR, $aS);
		});
	}, (subscriber) => {
		const handle = upstream_attach(subscriber);
		attach_tracker(tracker, subscriber);
		return handle;
	}, () => {
		detach_tracker(tracker);
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $bM(self, $p) {
	const $bN = $p;
	let $bO = null;
	if ($bN[0] === 0) {
		const turn = $bN[1];
		$bO = enqueue(turn, __clone(self[1].v));
	} else {
		const $bP = $u(draining_turns.v);
		let $bQ = null;
		if ($bP[0] === 0) {
			const draining = $bP[1];
			$bQ = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bQ = undefined;
		}
		$bO = $bQ;
	}
	return $bO;
}
function $bL(self, value, $n) {
	self[0].v = __clone(value);
	$bM(self, $n);
}
function $aL(self, $aM, $aN) {
	const instance = $aO(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$bL(cached, pull(), $aM);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aM, $aN);
	return cached;
}
function $aI(self, $aJ, $aK) {
	return [ $aL(self, $aJ, $aK) ];
}
function $bX(self) {
	return $j(self[0]);
}
function $cd(signal, observer) {
	const cell = signal[0];
	return $i(signal, mint_subscriber(() => {
		const $ce = [ 0, cell ];
		let $cf = null;
		if ($ce[0] === 0) {
			const live = $ce[1];
			$cf = observer(live.v);
		} else {
			$cf = undefined;
		}
		return $cf;
	}));
}
function $cc(self, observer, immediately) {
	const subscription = $cd(self, observer);
	if (immediately) {
		observer($j(self));
	}
	return subscription;
}
function $cb(self, observer, immediately) {
	return $cc(self[0], observer, immediately);
}
function $ca(self, observer) {
	return $cb(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const count = $a(1);
const eager = $d(__clone(count), (value, $c) => {
	return console.log("sub " + value);
});
const quiet = $l(__clone(count), (value, $k) => {
	return console.log("on_change " + value);
});
console.log("attached");
$m(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
$m(count, 3, [ 1 ]);
const $au = $at(($I) => {
	$M(__clone(count), (value, $J, $K, $L) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $I);
	return;
});
const _built = $au[0];
const scope = $au[1];
$m(count, 4, [ 1 ]);
dispose2(scope);
$m(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $aw(__clone(stored), (value, $av) => {
	return console.log("eager " + value);
});
$m(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $aC(__clone(stored), (value, $aB) => {
	return console.log("stored " + value);
});
$m(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const $bW = $at(($aD) => {
	return $aI($aH(__clone(stored), (value, $aE, $aF, $aG) => {
		return "n=" + value;
	}), [ 1 ], [ 0, $aD ]);
});
const labelled = $bW[0];
const sealed = $bW[1];
console.log($bX(labelled));
const shown = $ca(labelled, (value, $bZ) => {
	return console.log("label " + value);
});
$m(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
dispose2(sealed);
