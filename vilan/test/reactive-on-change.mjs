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
	const $be = turn;
	let $bf = null;
	if ($be[0] === 0) {
		const ambient = $be[1];
		$bf = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $bg = $s(draining_turns.v);
		let $bh = null;
		if ($bg[0] === 0) {
			const draining = $bg[1];
			$bh = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$bh = undefined;
		}
		$bf = $bh;
	}
	return $bf;
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
	const $bl = $s(releasing_turns.v);
	let $bm = null;
	if ($bl[0] === 0) {
		const at_release = $bl[1];
		$bm = at_release;
	} else {
		$bm = $s(draining_turns.v);
	}
	const turn = $bm;
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
	let $ak = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const held = __clone(self[0].v);
		if (held[2]) {
			held[1].v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v = [ held[0], __shared_new([ cleanup ]), true, held[3] ];
		}
		$ak = undefined;
	}
	return $ak;
}
function renew(self, with_nursery) {
	const live = [ self[0], self[0].v[0] ];
	dispose2(live);
	const next = self[0].v[0];
	if (with_nursery) {
		const held = self[0].v;
		self[0].v = [ held[0], held[1], held[2], [ 0, detached_nursery() ] ];
	}
	return [ self[0], next ];
}
function nursery(self) {
	let $Z = null;
	if (is_disposed(self)) {
		$Z = [ 1 ];
	} else {
		$Z = self[0].v[3];
	}
	return $Z;
}
function dispose2(self) {
	let $Y = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $P = held[3];
		let $Q = null;
		if ($P[0] === 0) {
			const nursery2 = $P[1];
			$Q = nursery2.cancel();
		} else {
			$Q = undefined;
		}
		$Q;
		let $X = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $R = __guarded(cleanup);
				let $S = null;
				if ($R[0] === 0) {
					const message = $R[1];
					if ($T(failure)) {
						failure = [ 0, message ];
					}
					$S = undefined;
				} else {
					$S = undefined;
				}
				$S;
			}
			const $V = failure;
			let $W = null;
			if ($V[0] === 0) {
				const message2 = $V[1];
				$W = (() => {
					throw message2;
				})();
			} else {
				$W = undefined;
			}
			$X = $W;
		}
		$Y = $X;
	}
	return $Y;
}
function get_owner($ah) {
	return $ah;
}
function register_with_owner(subscription, $bz, $bA) {
	const $bB = $bA;
	let $bC = null;
	if ($bB[0] === 0) {
		const owner = $bB[1];
		$bC = $ai(owner, subscription, $bz);
	} else {
		$bC = __clone(subscription);
	}
	return $bC;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false ]), __shared_new([  ]), __shared_new([  ]), __shared_new([  ]), __shared_new([ 1 ]) ];
}
function opened(held) {
	return [ held[0] + 1, true, false, held[3], held[4] ];
}
function closed(held) {
	return [ held[0], false, held[2], held[3], held[4] ];
}
function dirtied(held) {
	return [ held[0], held[1], true, held[3], held[4] || held[3] ];
}
function connecting(held, now) {
	return [ held[0], held[1], held[2], now, held[4] ];
}
function answered(held) {
	return [ held[0], held[1], held[2], held[3], false ];
}
function open_run(tracker) {
	const next = opened(tracker[0].v);
	const epoch = next[0];
	tracker[0].v = next;
	if (!($r(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v = closed(tracker[0].v);
	const $aV = tracker[4].v;
	let $aW = null;
	if ($aV[0] === 0) {
		const target = $aV[1];
		$aW = reconnect(tracker, target);
	} else {
		if (!($r(tracker[1].v)) || !($r(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$aW = undefined;
	}
	$aW;
	if (!($r(tracker[1].v))) {
		tracker[1].v = [  ];
	}
}
function reconnect(tracker, target) {
	if ($r(tracker[1].v) && $r(tracker[3].v)) {
		return;
	}
	const held = __clone(tracker[3].v);
	const reading = __clone(tracker[1].v);
	let kept = [  ];
	for (const _edge of held) {
		kept.push(false);
	}
	let next = [  ];
	tracker[0].v = connecting(tracker[0].v, true);
	let position = 0;
	for (const dependency of reading) {
		const $bc = reusable(held, kept, dependency[0], position);
		let $bd = null;
		if ($bc[0] === 0) {
			const index = $bc[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$bd = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$bd = undefined;
		}
		$bd;
		position = position + 1;
	}
	tracker[3].v = next;
	let index2 = 0;
	for (const edge of held) {
		if (!(__at(kept, index2))) {
			detach(edge[1]);
		}
		index2 = index2 + 1;
	}
	tracker[0].v = connecting(tracker[0].v, false);
}
function reusable(held, kept, identity, position) {
	const $aY = identity;
	let $aZ = null;
	if ($aY[0] === 0) {
		const wanted = $aY[1];
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
		$aZ = [ 1 ];
	} else {
		$aZ = [ 1 ];
	}
	return $aZ;
}
function same_identity(identity, wanted) {
	const $ba = identity;
	let $bb = null;
	if ($ba[0] === 0) {
		const held = $ba[1];
		$bb = held === wanted;
	} else {
		$bb = false;
	}
	return $bb;
}
function relay_for(tracker, target) {
	return subscriber_of(() => {
		const held = tracker[0].v;
		tracker[0].v = dirtied(held);
		if (!(held[3])) {
			wake(target);
		}
		return;
	}, true);
}
function attach_tracker(tracker, target) {
	tracker[4].v = [ 0, __clone(target) ];
	let $bq = null;
	if (!($r(tracker[2].v))) {
		const read = tracker[2].v;
		tracker[2].v = [  ];
		tracker[0].v = connecting(tracker[0].v, true);
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		tracker[3].v = edges;
		tracker[0].v = connecting(tracker[0].v, false);
		if (tracker[0].v[4]) {
			tracker[0].v = answered(tracker[0].v);
			wake(target);
		}
		$bq = undefined;
	}
	return $bq;
}
function forget_reads(tracker) {
	let $br = null;
	if (!($r(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$br = undefined;
	}
	$br;
	if (!($r(tracker[2].v))) {
		tracker[2].v = [  ];
	}
}
function detach_tracker(tracker) {
	tracker[4].v = [ 1 ];
	forget_reads(tracker);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $af = previous;
		let $ag = null;
		if ($af[0] === 0) {
			const earlier = $af[1];
			$ag = earlier();
		} else {
			$ag = undefined;
		}
		return $ag;
	} ];
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
function $T(self) {
	const $U = self;
	return $U[0] === 1;
}
function $O(runs, body) {
	const run = renew(runs, true);
	const $aa = nursery(run);
	let $ab = null;
	if ($aa[0] === 0) {
		const nursery2 = $aa[1];
		$ab = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$ab = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $ab;
}
function $ai(self, item, $aj) {
	defer(self, () => {
		dispose(item, $aj);
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
	$ai(get_owner($L), subscription, $K);
}
function $al(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $ap(self) {
	return $i(self[0]);
}
function $ar(self, subscriber) {
	return $h(self, subscriber);
}
function $aq(self, subscriber) {
	return $ar(self[0], subscriber);
}
function $ao(self, observer, immediately) {
	const subscription = $aq(self, mint_subscriber(() => {
		return observer($ap(self));
	}));
	if (immediately) {
		observer($ap(self));
	}
	return subscription;
}
function $an(self, observer) {
	return $ao(self, observer, true);
}
function $as(self, observer) {
	return $ao(self, observer, false);
}
function $ax(self, transform) {
	return [ __clone(self), transform ];
}
function $aF(self) {
	return [ () => {
		return $ap(self);
	}, (subscriber) => {
		return $aq(self, subscriber);
	}, () => {
		return;
	} ];
}
function $aS(runs, body) {
	const run = renew(runs, true);
	const $aT = nursery(run);
	let $aU = null;
	if ($aT[0] === 0) {
		const nursery2 = $aT[1];
		$aU = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$aU = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $aU;
}
function $aN(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $aS(runs, ($aP, $aQ) => {
		return (($aR) => {
			return body($aP, $aR, $aQ);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $aJ(runs, tracker, body) {
	let value = $aN(runs, tracker, ($aK, $aL, $aM) => {
		return body($aK, $aL, $aM);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v = answered(tracker[0].v);
		value = $aN(runs, tracker, ($bn, $bo, $bp) => {
			return body($bn, $bo, $bp);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $aE(self) {
	const transform = self[1];
	const upstream = $aF(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $aJ(runs, tracker, ($aG, $aH, $aI) => {
			return transform(value, $aG, $aH, $aI);
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
function $bu(self, $n) {
	const $bv = $n;
	let $bw = null;
	if ($bv[0] === 0) {
		const turn = $bv[1];
		$bw = enqueue(turn, __clone(self[1].v));
	} else {
		const $bx = $s(draining_turns.v);
		let $by = null;
		if ($bx[0] === 0) {
			const draining = $bx[1];
			$by = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$by = undefined;
		}
		$bw = $by;
	}
	return $bw;
}
function $bt(self, value, $l) {
	self[0].v = __clone(value);
	$bu(self, $l);
}
function $aB(self, $aC, $aD) {
	const instance = $aE(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$bt(cached, pull(), $aC);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aC, $aD);
	return cached;
}
function $ay(self, $az, $aA) {
	return [ $aB(self, $az, $aA) ];
}
function $bF(self) {
	return $i(self[0]);
}
function $bK(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $bL = [ 0, cell ];
		let $bM = null;
		if ($bL[0] === 0) {
			const live = $bL[1];
			$bM = observer(live.v);
		} else {
			$bM = undefined;
		}
		return $bM;
	}));
}
function $bJ(self, observer, immediately) {
	const subscription = $bK(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $bI(self, observer, immediately) {
	return $bJ(self[0], observer, immediately);
}
function $bH(self, observer) {
	return $bI(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
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
const $am = $al(($G) => {
	$J(__clone(count), (value, $H, $I) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $G);
	return;
});
const _built = $am[0];
const scope = $am[1];
$k(count, 4, [ 1 ]);
dispose2(scope);
$k(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $an(__clone(stored), (value) => {
	return console.log("eager " + value);
});
$k(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $as(__clone(stored), (value) => {
	return console.log("stored " + value);
});
$k(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const $bE = $al(($at) => {
	return $ay($ax(__clone(stored), (value, $au, $av, $aw) => {
		return "n=" + value;
	}), [ 1 ], [ 0, $at ]);
});
const labelled = $bE[0];
const sealed = $bE[1];
console.log($bF(labelled));
const shown = $bH(labelled, (value) => {
	return console.log("label " + value);
});
$k(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
dispose2(sealed);
