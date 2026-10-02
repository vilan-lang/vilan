function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
function __insert_at(list, index, value) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
	throw typeof winner.error === "string" ? winner.error + " (in task spawned in " + winner.origin + ")" : winner.error;
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
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
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
function retiring(subscriber, release) {
	return detached(subscriber[0], subscriber[2], release);
}
function detached(id, live, release) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release ]) ];
}
function dispose(self, $bq) {
	const $br = $bq;
	let $bs = null;
	if ($br[0] === 0) {
		const established = $br[1];
		$bs = [ 0, established ];
	} else {
		$bs = $as(draining_turns.v);
	}
	const ambient = $bs;
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
	let $bt = null;
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
		$bt = undefined;
	}
	return $bt;
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
					throw message2;
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
function register_with_owner(subscription, $bk, $bl) {
	const $bm = $bl;
	let $bn = null;
	if ($bm[0] === 0) {
		const owner = $bm[1];
		$bn = $bo(owner, subscription, $bk);
	} else {
		$bn = __clone(subscription);
	}
	return $bn;
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
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
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
		if (!(__at(kept, index2))) {
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
		if (position < held.length && !(__at(kept, position)) && same_identity(__at(held, position)[0], wanted)) {
			return [ 0, position ];
		}
		let index = 0;
		while (index < held.length) {
			if (!(__at(kept, index)) && same_identity(__at(held, index)[0], wanted)) {
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
	const $aU = tracker[0].v[6];
	let $aV = null;
	if ($aU[0] === 0) {
		const lists = $aU[1];
		$aV = lists;
	} else {
		return;
		$aV = undefined;
	}
	const lists2 = $aV;
	let $aW = null;
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
		$aW = undefined;
	}
	return $aW;
}
function forget_reads(tracker) {
	const $aX = tracker[0].v[6];
	let $aY = null;
	if ($aX[0] === 0) {
		const lists = $aX[1];
		$aY = lists;
	} else {
		return;
		$aY = undefined;
	}
	const lists2 = $aY;
	let $aZ = null;
	if (!($E(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aZ = undefined;
	}
	$aZ;
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
		const $bi = previous;
		let $bj = null;
		if ($bi[0] === 0) {
			const earlier = $bi[1];
			$bj = earlier();
		} else {
			$bj = undefined;
		}
		return $bj;
	} ];
}
function relay_to(subscriber) {
	return subscriber_of(() => {
		return wake(subscriber);
	}, true);
}
function detach_held(handle) {
	const $aP = handle.v;
	let $aQ = null;
	if ($aP[0] === 0) {
		const held = $aP[1];
		$aQ = detach(held);
	} else {
		$aQ = undefined;
	}
	$aQ;
	handle.v = [ 1 ];
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
function $c(value) {
	return $b(value);
}
function $i(self, select) {
	return [ self, select ];
}
function $r(self) {
	return __clone(self[0].v);
}
function $t(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $s(self, subscriber) {
	return $t(self, subscriber);
}
function $q(self) {
	return [ () => {
		return $r(self);
	}, (subscriber) => {
		return $s(self, subscriber);
	}, () => {
		return;
	} ];
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
			throw "a renewed run carries its nursery";
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
	const scope2 = open_run(tracker);
	const value = $I(runs, ($F, $G) => {
		return (($H) => {
			return body($F, $H, $G);
		})(scope2);
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
function $aN(self, subscriber) {
	return $t(self, subscriber);
}
function $aL(self) {
	return [ () => {
		return $r(self);
	}, (subscriber) => {
		return $aN(self, subscriber);
	}, () => {
		return;
	} ];
}
function $p(self) {
	const select = self[1];
	const runs = new2();
	const tracker = new_tracker();
	const outer2 = $q(__clone(self[0]));
	const outer_pull = outer2[0];
	const followed = __shared_new($aL($x(runs, tracker, ($u, $v, $w) => {
		return select(outer_pull(), $u, $v, $w);
	})));
	const followed_handle = __shared_new([ 1 ]);
	return [ () => {
		return followed.v[0]();
	}, (subscriber) => {
		followed_handle.v = [ 0, followed.v[1](relay_to(subscriber)) ];
		const reselect = subscriber_of(() => {
			detach_held(followed_handle);
			followed.v[2]();
			const next = $aL($x(runs, tracker, ($aR, $aS, $aT) => {
				return select(outer_pull(), $aR, $aS, $aT);
			}));
			followed_handle.v = [ 0, next[1](relay_to(subscriber)) ];
			followed.v = next;
			wake(subscriber);
			return;
		}, true);
		const outer_handle = outer2[1](reselect);
		attach_tracker(tracker, reselect);
		return retiring(subscriber, () => {
			detach_held(followed_handle);
			detach(outer_handle);
			detach_tracker(tracker);
			return;
		});
	}, () => {
		detach_tracker(tracker);
		release_runs(runs);
		followed.v[2]();
		outer2[2]();
		return;
	} ];
}
function $bc(self, $bd) {
	const $be = $bd;
	let $bf = null;
	if ($be[0] === 0) {
		const turn = $be[1];
		$bf = enqueue(turn, __clone(self[1].v));
	} else {
		const $bg = $as(draining_turns.v);
		let $bh = null;
		if ($bg[0] === 0) {
			const draining = $bg[1];
			$bh = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bh = undefined;
		}
		$bf = $bh;
	}
	return $bf;
}
function $ba(self, value, $bb) {
	self[0].v = __clone(value);
	$bc(self, $bb);
}
function $bo(self, item, $bp) {
	defer(self, () => {
		dispose(item, $bp);
		return;
	});
	return __clone(item);
}
function $m(self, $n, $o) {
	const instance = $p(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$ba(cached, pull(), $n);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $n, $o);
	return cached;
}
function $j(self, $k, $l) {
	return [ $m(self, $k, $l) ];
}
function $bu(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $bw(self) {
	return $r(self[0]);
}
function $bx(self, value, $bb) {
	self[0].v = __clone(value);
	$bc(self, $bb);
}
function $bH(self, transform) {
	return [ self, transform ];
}
function $bM(self, subscriber) {
	return $t(self[0], subscriber);
}
function $bL(self) {
	return [ () => {
		return $bw(self);
	}, (subscriber) => {
		return $bM(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bS(runs, body) {
	const run = renew(runs);
	const $bT = nursery(run);
	let $bU = null;
	if ($bT[0] === 0) {
		const nursery2 = $bT[1];
		$bU = (($ab) => {
			return (($ac) => {
				return body($ab, $ac);
			})(nursery2);
		})(run);
	} else {
		$bU = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $bU;
}
function $bR(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $bS(runs, ($F, $G) => {
		return (($H) => {
			return body($F, $H, $G);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $bQ(runs, tracker, body) {
	let value = $bR(runs, tracker, ($y, $z, $A) => {
		return body($y, $z, $A);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $bR(runs, tracker, ($aI, $aJ, $aK) => {
			return body($aI, $aJ, $aK);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $bK(self) {
	const transform = self[1];
	const upstream = $bL(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $bQ(runs, tracker, ($bN, $bO, $bP) => {
			return transform(value, $bN, $bO, $bP);
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
function $bJ(self, $n, $o) {
	const instance = $bK(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$ba(cached, pull(), $n);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $n, $o);
	return cached;
}
function $bI(self, $k, $l) {
	return [ $bJ(self, $k, $l) ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const first = $a(1);
const second = $a(10);
const outer = $c(__clone(first));
const $bv = $bu(($e) => {
	return $j($i(__clone(outer), (inner, $f, $g, $h) => {
		return __clone(inner);
	}), [ 1 ], [ 0, $e ]);
});
const joined = $bv[0];
const scope = $bv[1];
console.log($bw(joined));
$ba(first, 2, [ 1 ]);
console.log($bw(joined));
$bx(outer, second, [ 1 ]);
console.log($bw(joined));
$ba(first, 99, [ 1 ]);
console.log($bw(joined));
$ba(second, 11, [ 1 ]);
console.log($bw(joined));
const $bV = $bu(($bD) => {
	return $bI($bH(__clone(joined), (value, $bE, $bF, $bG) => {
		return value * 2;
	}), [ 1 ], [ 0, $bD ]);
});
const doubled = $bV[0];
const stacked = $bV[1];
$ba(second, 21, [ 1 ]);
console.log($bw(doubled));
dispose2(stacked);
dispose2(scope);
