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
function mint_subscriber(notify) {
	const derived = minting_derivation.v;
	minting_derivation.v = false;
	return subscriber_of(notify, derived);
}
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $r(self[0].v) && $r(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $q = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$q = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$q;
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
				while (!($r(turn[1].v)) && budget > 0) {
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
	const $bn = turn;
	let $bo = null;
	if ($bn[0] === 0) {
		const ambient = $bn[1];
		$bo = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bp = $s(draining_turns.v);
		let $bq = null;
		if ($bp[0] === 0) {
			const draining = $bp[1];
			$bq = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bq = undefined;
		}
		$bo = $bq;
	}
	return $bo;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const established = $y[1];
		$z = [ 0, established ];
	} else {
		$z = $s(draining_turns.v);
	}
	const ambient = $z;
	release_under(self, ambient);
}
function detach(handle) {
	const $bu = $s(releasing_turns.v);
	let $bv = null;
	if ($bu[0] === 0) {
		const at_release = $bu[1];
		$bv = at_release;
	} else {
		$bv = $s(draining_turns.v);
	}
	const turn = $bv;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $A = [ 0, handle[0] ];
	let $B = null;
	if ($A[0] === 0) {
		const subscribers = $A[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$B = undefined;
	} else {
		$B = undefined;
	}
	$B;
	const $C = ambient;
	let $D = null;
	if ($C[0] === 0) {
		const turn = $C[1];
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
		$D = undefined;
	} else {
		$D = undefined;
	}
	$D;
	const $E = handle[3].v;
	let $F = null;
	if ($E[0] === 0) {
		const release = $E[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$F = undefined;
	} else {
		$F = undefined;
	}
	return $F;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ap = null;
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
		$ap = undefined;
	}
	return $ap;
}
function renew(self) {
	const $P = self[0].v[3];
	let $Q = null;
	if ($P[0] === 0) {
		const nursery2 = __clone($P[1]);
		let $R = null;
		if (has_spawned(nursery2)) {
			$R = [ 1 ];
		} else {
			$R = [ 0, nursery2 ];
		}
		$Q = $R;
	} else {
		$Q = [ 1 ];
	}
	const carried = $Q;
	advance([ self[0], self[0].v[0] ], carried);
	if ($U(self[0].v[3])) {
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
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $S = held[3];
		let $T = null;
		if ($S[0] === 0) {
			const nursery2 = $S[1];
			if ($U(carried)) {
				nursery2.cancel();
			}
			$T = undefined;
		} else {
			$T = undefined;
		}
		$T;
		let $ac = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $W = __guarded(cleanup);
				let $X = null;
				if ($W[0] === 0) {
					const message = $W[1];
					if ($U(failure)) {
						failure = [ 0, message ];
					}
					$X = undefined;
				} else {
					$X = undefined;
				}
				$X;
			}
			const $aa = failure;
			let $ab = null;
			if ($aa[0] === 0) {
				const message2 = $aa[1];
				$ab = (() => {
					throw message2;
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
function get_owner($am) {
	return $am;
}
function register_with_owner(subscription, $bM, $bN) {
	const $bO = $bN;
	let $bP = null;
	if ($bO[0] === 0) {
		const owner = $bO[1];
		$bP = $an(owner, subscription, $bM);
	} else {
		$bP = __clone(subscription);
	}
	return $bP;
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
	const $aT = tracker[0].v[6];
	let $aU = null;
	if ($aT[0] === 0) {
		const lists = $aT[1];
		if (!($r(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$aU = undefined;
	} else {
		$aU = undefined;
	}
	$aU;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $bc = tracker[0].v[6];
	let $bd = null;
	if ($bc[0] === 0) {
		const lists = $bc[1];
		$bd = lists;
	} else {
		return;
		$bd = undefined;
	}
	const lists2 = $bd;
	const $be = tracker[0].v[5];
	let $bf = null;
	if ($be[0] === 0) {
		const target = __clone($be[1]);
		$bf = reconnect(tracker, lists2, target);
	} else {
		if (!($r(lists2.v[0])) || !($r(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$bf = undefined;
	}
	$bf;
	if (!($r(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($r(lists.v[0]) && $r(lists.v[2])) {
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
		const $bl = reusable(held, kept, dependency[0], position);
		let $bm = null;
		if ($bl[0] === 0) {
			const index = $bl[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$bm = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$bm = undefined;
		}
		$bm;
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
	const $bh = identity;
	let $bi = null;
	if ($bh[0] === 0) {
		const wanted = $bh[1];
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
		$bi = [ 1 ];
	} else {
		$bi = [ 1 ];
	}
	return $bi;
}
function same_identity(identity, wanted) {
	const $bj = identity;
	let $bk = null;
	if ($bj[0] === 0) {
		const held = $bj[1];
		$bk = held === wanted;
	} else {
		$bk = false;
	}
	return $bk;
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
	const $bz = tracker[0].v[6];
	let $bA = null;
	if ($bz[0] === 0) {
		const lists = $bz[1];
		$bA = lists;
	} else {
		return;
		$bA = undefined;
	}
	const lists2 = $bA;
	let $bB = null;
	if (!($r(lists2.v[1]))) {
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
		$bB = undefined;
	}
	return $bB;
}
function forget_reads(tracker) {
	const $bC = tracker[0].v[6];
	let $bD = null;
	if ($bC[0] === 0) {
		const lists = $bC[1];
		$bD = lists;
	} else {
		return;
		$bD = undefined;
	}
	const lists2 = $bD;
	let $bE = null;
	if (!($r(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$bE = undefined;
	}
	$bE;
	if (!($r(lists2.v[1]))) {
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
		const $ak = previous;
		let $al = null;
		if ($ak[0] === 0) {
			const earlier = $ak[1];
			$al = earlier();
		} else {
			$al = undefined;
		}
		return $al;
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
	return [ __shared_new(__clone(value)), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $h(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $e(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $f = [ 0, cell ];
		let $g = null;
		if ($f[0] === 0) {
			const live = $f[1];
			$g = observer(live.v);
		} else {
			$g = undefined;
		}
		return $g;
	}));
}
function $i(self) {
	return __clone(self[0].v);
}
function $d(self, observer, immediately) {
	const subscription = $e(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $c(self, observer) {
	return $d(self, observer, true);
}
function $j(self, observer) {
	return $d(self, observer, false);
}
function $r(self) {
	return self.length === 0;
}
function $s(self) {
	let $u = null;
	if ($r(self)) {
		$u = [ 1 ];
	} else {
		$u = __list_get(self, self.length - 1);
	}
	return $u;
}
function $m(self, $n) {
	const $o = $n;
	let $p = null;
	if ($o[0] === 0) {
		const turn = $o[1];
		$p = enqueue(turn, __clone(self[1].v));
	} else {
		const $v = $s(draining_turns.v);
		let $w = null;
		if ($v[0] === 0) {
			const draining = $v[1];
			$w = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$w = undefined;
		}
		$p = $w;
	}
	return $p;
}
function $k(self, value, $l) {
	self[0].v = __clone(value);
	$m(self, $l);
}
function $U(self) {
	const $V = self;
	return $V[0] === 1;
}
function $O(runs, body) {
	const run = renew(runs);
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
			throw "a renewed run carries its nursery";
		})();
	}
	return $ag;
}
function $an(self, item, $ao) {
	defer(self, () => {
		dispose(item, $ao);
		return;
	});
	return __clone(item);
}
function $J(self, body, $K, $L) {
	const runs = new2();
	const subscription = $j(self, (value) => {
		return $O(runs, ($M, $N) => {
			return body(value, $M, $N);
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	$an(get_owner($L), subscription, $K);
}
function $aq(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $au(self) {
	return $i(self[0]);
}
function $aw(self, subscriber) {
	return $h(self, subscriber);
}
function $av(self, subscriber) {
	return $aw(self[0], subscriber);
}
function $at(self, observer, immediately) {
	const subscription = $av(self, mint_subscriber(() => {
		return observer($au(self));
	}));
	if (immediately) {
		observer($au(self));
	}
	return subscription;
}
function $as(self, observer) {
	return $at(self, observer, true);
}
function $ax(self, observer) {
	return $at(self, observer, false);
}
function $aC(self, transform) {
	return [ self, transform ];
}
function $aK(self) {
	return [ () => {
		return $au(self);
	}, (subscriber) => {
		return $av(self, subscriber);
	}, () => {
		return;
	} ];
}
function $aZ(runs, body) {
	const run = renew(runs);
	const $ba = nursery(run);
	let $bb = null;
	if ($ba[0] === 0) {
		const nursery2 = $ba[1];
		$bb = (($ah) => {
			return (($ai) => {
				return body($ah, $ai);
			})(nursery2);
		})(run);
	} else {
		$bb = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $bb;
}
function $aS(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $aZ(runs, ($aW, $aX) => {
		return (($aY) => {
			return body($aW, $aY, $aX);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $aO(runs, tracker, body) {
	let value = $aS(runs, tracker, ($aP, $aQ, $aR) => {
		return body($aP, $aQ, $aR);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $aS(runs, tracker, ($bw, $bx, $by) => {
			return body($bw, $bx, $by);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aJ(self) {
	const transform = self[1];
	const upstream = $aK(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $aO(runs, tracker, ($aL, $aM, $aN) => {
			return transform(value, $aL, $aM, $aN);
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
function $bH(self, $n) {
	const $bI = $n;
	let $bJ = null;
	if ($bI[0] === 0) {
		const turn = $bI[1];
		$bJ = enqueue(turn, __clone(self[1].v));
	} else {
		const $bK = $s(draining_turns.v);
		let $bL = null;
		if ($bK[0] === 0) {
			const draining = $bK[1];
			$bL = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bL = undefined;
		}
		$bJ = $bL;
	}
	return $bJ;
}
function $bG(self, value, $l) {
	self[0].v = __clone(value);
	$bH(self, $l);
}
function $aG(self, $aH, $aI) {
	const instance = $aJ(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$bG(cached, pull(), $aH);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aH, $aI);
	return cached;
}
function $aD(self, $aE, $aF) {
	return [ $aG(self, $aE, $aF) ];
}
function $bS(self) {
	return $i(self[0]);
}
function $bX(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $bY = [ 0, cell ];
		let $bZ = null;
		if ($bY[0] === 0) {
			const live = $bY[1];
			$bZ = observer(live.v);
		} else {
			$bZ = undefined;
		}
		return $bZ;
	}));
}
function $bW(self, observer, immediately) {
	const subscription = $bX(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $bV(self, observer, immediately) {
	return $bW(self[0], observer, immediately);
}
function $bU(self, observer) {
	return $bV(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const count = $a(1);
const eager = $c(__clone(count), (value) => {
	return console.log("sub " + value);
});
const quiet = $j(__clone(count), (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$k(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
$k(count, 3, [ 1 ]);
const $ar = $aq(($G) => {
	$J(__clone(count), (value, $H, $I) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $G);
	return;
});
const _built = $ar[0];
const scope = $ar[1];
$k(count, 4, [ 1 ]);
dispose2(scope);
$k(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $as(__clone(stored), (value) => {
	return console.log("eager " + value);
});
$k(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $ax(__clone(stored), (value) => {
	return console.log("stored " + value);
});
$k(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const $bR = $aq(($ay) => {
	return $aD($aC(__clone(stored), (value, $az, $aA, $aB) => {
		return "n=" + value;
	}), [ 1 ], [ 0, $ay ]);
});
const labelled = $bR[0];
const sealed = $bR[1];
console.log($bS(labelled));
const shown = $bU(labelled, (value) => {
	return console.log("label " + value);
});
$k(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
dispose2(sealed);
