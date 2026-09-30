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
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $C(self[0].v) && $C(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $ah = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ah = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ah;
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
				while (!($C(turn[1].v)) && budget > 0) {
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
	const $af = turn;
	let $ag = null;
	if ($af[0] === 0) {
		const ambient = $af[1];
		$ag = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $am = $aj(draining_turns.v);
		let $an = null;
		if ($am[0] === 0) {
			const draining = $am[1];
			$an = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$an = undefined;
		}
		$ag = $an;
	}
	return $ag;
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
function dispose(self, $bd) {
	const $be = $bd;
	let $bf = null;
	if ($be[0] === 0) {
		const established = $be[1];
		$bf = [ 0, established ];
	} else {
		$bf = $aj(draining_turns.v);
	}
	const ambient = $bf;
	release_under(self, ambient);
}
function detach(handle) {
	const $ar = $aj(releasing_turns.v);
	let $as = null;
	if ($ar[0] === 0) {
		const at_release = $ar[1];
		$as = at_release;
	} else {
		$as = $aj(draining_turns.v);
	}
	const turn = $as;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $at = [ 0, handle[0] ];
	let $au = null;
	if ($at[0] === 0) {
		const subscribers = $at[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$au = undefined;
	} else {
		$au = undefined;
	}
	$au;
	const $av = ambient;
	let $aw = null;
	if ($av[0] === 0) {
		const turn = $av[1];
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
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	$aw;
	const $ax = handle[3].v;
	let $ay = null;
	if ($ax[0] === 0) {
		const release = $ax[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	return $ay;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bg = null;
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
		$bg = undefined;
	}
	return $bg;
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
	let $R = null;
	if (is_disposed(self)) {
		$R = [ 1 ];
	} else {
		$R = self[0].v[3];
	}
	return $R;
}
function dispose2(self) {
	let $Q = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $H = held[3];
		let $I = null;
		if ($H[0] === 0) {
			const nursery2 = $H[1];
			$I = nursery2.cancel();
		} else {
			$I = undefined;
		}
		$I;
		let $P = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $J = __guarded(cleanup);
				let $K = null;
				if ($J[0] === 0) {
					const message = $J[1];
					if ($L(failure)) {
						failure = [ 0, message ];
					}
					$K = undefined;
				} else {
					$K = undefined;
				}
				$K;
			}
			const $N = failure;
			let $O = null;
			if ($N[0] === 0) {
				const message2 = $N[1];
				$O = (() => {
					throw message2;
				})();
			} else {
				$O = undefined;
			}
			$P = $O;
		}
		$Q = $P;
	}
	return $Q;
}
function register_with_owner(subscription, $aX, $aY) {
	const $aZ = $aY;
	let $ba = null;
	if ($aZ[0] === 0) {
		const owner = $aZ[1];
		$ba = $bb(owner, subscription, $aX);
	} else {
		$ba = __clone(subscription);
	}
	return $ba;
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
	if (!($C(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v = closed(tracker[0].v);
	const $W = tracker[4].v;
	let $X = null;
	if ($W[0] === 0) {
		const target = $W[1];
		$X = reconnect(tracker, target);
	} else {
		if (!($C(tracker[1].v)) || !($C(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$X = undefined;
	}
	$X;
	if (!($C(tracker[1].v))) {
		tracker[1].v = [  ];
	}
}
function reconnect(tracker, target) {
	if ($C(tracker[1].v) && $C(tracker[3].v)) {
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
		const $ad = reusable(held, kept, dependency[0], position);
		let $ae = null;
		if ($ad[0] === 0) {
			const index = $ad[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$ae = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ae = undefined;
		}
		$ae;
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
	const $Z = identity;
	let $aa = null;
	if ($Z[0] === 0) {
		const wanted = $Z[1];
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
		$aa = [ 1 ];
	} else {
		$aa = [ 1 ];
	}
	return $aa;
}
function same_identity(identity, wanted) {
	const $ab = identity;
	let $ac = null;
	if ($ab[0] === 0) {
		const held = $ab[1];
		$ac = held === wanted;
	} else {
		$ac = false;
	}
	return $ac;
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
	let $aL = null;
	if (!($C(tracker[2].v))) {
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
		$aL = undefined;
	}
	return $aL;
}
function forget_reads(tracker) {
	let $aM = null;
	if (!($C(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aM = undefined;
	}
	$aM;
	if (!($C(tracker[2].v))) {
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
		const $aV = previous;
		let $aW = null;
		if ($aV[0] === 0) {
			const earlier = $aV[1];
			$aW = earlier();
		} else {
			$aW = undefined;
		}
		return $aW;
	} ];
}
function relay_to(subscriber) {
	return subscriber_of(() => {
		return wake(subscriber);
	}, true);
}
function detach_held(handle) {
	const $aG = handle.v;
	let $aH = null;
	if ($aG[0] === 0) {
		const held = $aG[1];
		$aH = detach(held);
	} else {
		$aH = undefined;
	}
	$aH;
	handle.v = [ 1 ];
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
	return [ __clone(self), select ];
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
function $C(self) {
	return self.length === 0;
}
function $L(self) {
	const $M = self;
	return $M[0] === 1;
}
function $G(runs, body) {
	const run = renew(runs, true);
	const $S = nursery(run);
	let $T = null;
	if ($S[0] === 0) {
		const nursery2 = $S[1];
		$T = (($U) => {
			return (($V) => {
				return body($U, $V);
			})(nursery2);
		})(run);
	} else {
		$T = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $T;
}
function $aj(self) {
	let $al = null;
	if ($C(self)) {
		$al = [ 1 ];
	} else {
		$al = __list_get(self, self.length - 1);
	}
	return $al;
}
function $B(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $G(runs, ($D, $E) => {
		return (($F) => {
			return body($D, $F, $E);
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
		tracker[0].v = answered(tracker[0].v);
		value = $B(runs, tracker, ($az, $aA, $aB) => {
			return body($az, $aA, $aB);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $aE(self, subscriber) {
	return $t(self, subscriber);
}
function $aC(self) {
	return [ () => {
		return $r(self);
	}, (subscriber) => {
		return $aE(self, subscriber);
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
	const followed = __shared_new($aC($x(runs, tracker, ($u, $v, $w) => {
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
			const next = $aC($x(runs, tracker, ($aI, $aJ, $aK) => {
				return select(outer_pull(), $aI, $aJ, $aK);
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
function $aP(self, $aQ) {
	const $aR = $aQ;
	let $aS = null;
	if ($aR[0] === 0) {
		const turn = $aR[1];
		$aS = enqueue(turn, __clone(self[1].v));
	} else {
		const $aT = $aj(draining_turns.v);
		let $aU = null;
		if ($aT[0] === 0) {
			const draining = $aT[1];
			$aU = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aU = undefined;
		}
		$aS = $aU;
	}
	return $aS;
}
function $aN(self, value, $aO) {
	self[0].v = __clone(value);
	$aP(self, $aO);
}
function $bb(self, item, $bc) {
	defer(self, () => {
		dispose(item, $bc);
		return;
	});
	return __clone(item);
}
function $m(self, $n, $o) {
	const instance = $p(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aN(cached, pull(), $n);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $n, $o);
	return cached;
}
function $j(self, $k, $l) {
	return [ $m(self, $k, $l) ];
}
function $bh(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $bj(self) {
	return $r(self[0]);
}
function $bk(self, value, $aO) {
	self[0].v = __clone(value);
	$aP(self, $aO);
}
function $bu(self, transform) {
	return [ __clone(self), transform ];
}
function $bz(self, subscriber) {
	return $t(self[0], subscriber);
}
function $by(self) {
	return [ () => {
		return $bj(self);
	}, (subscriber) => {
		return $bz(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bF(runs, body) {
	const run = renew(runs, true);
	const $bG = nursery(run);
	let $bH = null;
	if ($bG[0] === 0) {
		const nursery2 = $bG[1];
		$bH = (($U) => {
			return (($V) => {
				return body($U, $V);
			})(nursery2);
		})(run);
	} else {
		$bH = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $bH;
}
function $bE(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $bF(runs, ($D, $E) => {
		return (($F) => {
			return body($D, $F, $E);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $bD(runs, tracker, body) {
	let value = $bE(runs, tracker, ($y, $z, $A) => {
		return body($y, $z, $A);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v = answered(tracker[0].v);
		value = $bE(runs, tracker, ($az, $aA, $aB) => {
			return body($az, $aA, $aB);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $bx(self) {
	const transform = self[1];
	const upstream = $by(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $bD(runs, tracker, ($bA, $bB, $bC) => {
			return transform(value, $bA, $bB, $bC);
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
function $bw(self, $n, $o) {
	const instance = $bx(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aN(cached, pull(), $n);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $n, $o);
	return cached;
}
function $bv(self, $k, $l) {
	return [ $bw(self, $k, $l) ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const first = $a(1);
const second = $a(10);
const outer = $c(first);
const $bi = $bh(($e) => {
	return $j($i(__clone(outer), (inner, $f, $g, $h) => {
		return __clone(inner);
	}), [ 1 ], [ 0, $e ]);
});
const joined = $bi[0];
const scope = $bi[1];
console.log($bj(joined));
$aN(first, 2, [ 1 ]);
console.log($bj(joined));
$bk(outer, second, [ 1 ]);
console.log($bj(joined));
$aN(first, 99, [ 1 ]);
console.log($bj(joined));
$aN(second, 11, [ 1 ]);
console.log($bj(joined));
const $bI = $bh(($bq) => {
	return $bv($bu(__clone(joined), (value, $br, $bs, $bt) => {
		return value * 2;
	}), [ 1 ], [ 0, $bq ]);
});
const doubled = $bI[0];
const stacked = $bI[1];
$aN(second, 21, [ 1 ]);
console.log($bj(doubled));
dispose2(stacked);
dispose2(scope);
