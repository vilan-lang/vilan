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
	return $B(self[0].v) && $B(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $an = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$an = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$an;
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
				while (!($B(turn[1].v)) && budget > 0) {
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
	const $al = turn;
	let $am = null;
	if ($al[0] === 0) {
		const ambient = $al[1];
		$am = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $as = $ap(draining_turns.v);
		let $at = null;
		if ($as[0] === 0) {
			const draining = $as[1];
			$at = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$at = undefined;
		}
		$am = $at;
	}
	return $am;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $be) {
	const $bf = $be;
	let $bg = null;
	if ($bf[0] === 0) {
		const established = $bf[1];
		$bg = [ 0, established ];
	} else {
		$bg = $ap(draining_turns.v);
	}
	const ambient = $bg;
	release_under(self, ambient);
}
function detach(handle) {
	const $ax = $ap(releasing_turns.v);
	let $ay = null;
	if ($ax[0] === 0) {
		const at_release = $ax[1];
		$ay = at_release;
	} else {
		$ay = $ap(draining_turns.v);
	}
	const turn = $ay;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $az = [ 0, handle[0] ];
	let $aA = null;
	if ($az[0] === 0) {
		const subscribers = $az[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aA = undefined;
	} else {
		$aA = undefined;
	}
	$aA;
	const $aB = ambient;
	let $aC = null;
	if ($aB[0] === 0) {
		const turn = $aB[1];
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
		$aC = undefined;
	} else {
		$aC = undefined;
	}
	$aC;
	const $aD = handle[3].v;
	let $aE = null;
	if ($aD[0] === 0) {
		const release = $aD[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aE = undefined;
	} else {
		$aE = undefined;
	}
	return $aE;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bh = null;
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
		$bh = undefined;
	}
	return $bh;
}
function renew(self) {
	const $G = self[0].v[3];
	let $H = null;
	if ($G[0] === 0) {
		const nursery2 = __clone($G[1]);
		let $I = null;
		if (has_spawned(nursery2)) {
			$I = [ 1 ];
		} else {
			$I = [ 0, nursery2 ];
		}
		$H = $I;
	} else {
		$H = [ 1 ];
	}
	const carried = $H;
	advance([ self[0], self[0].v[0] ], carried);
	if ($L(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $V = null;
	if (is_disposed(self)) {
		$V = [ 1 ];
	} else {
		$V = self[0].v[3];
	}
	return $V;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $U = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $J = held[3];
		let $K = null;
		if ($J[0] === 0) {
			const nursery2 = $J[1];
			if ($L(carried)) {
				nursery2.cancel();
			}
			$K = undefined;
		} else {
			$K = undefined;
		}
		$K;
		let $T = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $N = __guarded(cleanup);
				let $O = null;
				if ($N[0] === 0) {
					const message = $N[1];
					if ($L(failure)) {
						failure = [ 0, message ];
					}
					$O = undefined;
				} else {
					$O = undefined;
				}
				$O;
			}
			const $R = failure;
			let $S = null;
			if ($R[0] === 0) {
				const message2 = $R[1];
				$S = (() => {
					throw message2;
				})();
			} else {
				$S = undefined;
			}
			$T = $S;
		}
		$U = $T;
	}
	return $U;
}
function register_with_owner(subscription, $aY, $aZ) {
	const $ba = $aZ;
	let $bb = null;
	if ($ba[0] === 0) {
		const owner2 = $ba[1];
		$bb = $bc(owner2, subscription, $aY);
	} else {
		$bb = __clone(subscription);
	}
	return $bb;
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
	const $z = tracker[0].v[6];
	let $A = null;
	if ($z[0] === 0) {
		const lists = $z[1];
		if (!($B(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$A = undefined;
	} else {
		$A = undefined;
	}
	$A;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $aa = tracker[0].v[6];
	let $ab = null;
	if ($aa[0] === 0) {
		const lists = $aa[1];
		$ab = lists;
	} else {
		return;
		$ab = undefined;
	}
	const lists2 = $ab;
	const $ac = tracker[0].v[5];
	let $ad = null;
	if ($ac[0] === 0) {
		const target = __clone($ac[1]);
		$ad = reconnect(tracker, lists2, target);
	} else {
		if (!($B(lists2.v[0])) || !($B(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$ad = undefined;
	}
	$ad;
	if (!($B(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($B(lists.v[0]) && $B(lists.v[2])) {
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
		const $aj = reusable(held, kept, dependency[0], position);
		let $ak = null;
		if ($aj[0] === 0) {
			const index = $aj[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$ak = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ak = undefined;
		}
		$ak;
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
	const $af = identity;
	let $ag = null;
	if ($af[0] === 0) {
		const wanted = $af[1];
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
		$ag = [ 1 ];
	} else {
		$ag = [ 1 ];
	}
	return $ag;
}
function same_identity(identity, wanted) {
	const $ah = identity;
	let $ai = null;
	if ($ah[0] === 0) {
		const held = $ah[1];
		$ai = held === wanted;
	} else {
		$ai = false;
	}
	return $ai;
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
	const $aI = tracker[0].v[6];
	let $aJ = null;
	if ($aI[0] === 0) {
		const lists = $aI[1];
		$aJ = lists;
	} else {
		return;
		$aJ = undefined;
	}
	const lists2 = $aJ;
	let $aK = null;
	if (!($B(lists2.v[1]))) {
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
		$aK = undefined;
	}
	return $aK;
}
function forget_reads(tracker) {
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
	if (!($B(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aN = undefined;
	}
	$aN;
	if (!($B(lists2.v[1]))) {
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
		const $aW = previous;
		let $aX = null;
		if ($aW[0] === 0) {
			const earlier = $aW[1];
			$aX = earlier();
		} else {
			$aX = undefined;
		}
		return $aX;
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
function $f(self, transform) {
	return [ self, transform ];
}
function $o(self) {
	return __clone(self[0].v);
}
function $q(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $p(self, subscriber) {
	return $q(self, subscriber);
}
function $n(self) {
	return [ () => {
		return $o(self);
	}, (subscriber) => {
		return $p(self, subscriber);
	}, () => {
		return;
	} ];
}
function $B(self) {
	return self.length === 0;
}
function $L(self) {
	const $M = self;
	return $M[0] === 1;
}
function $F(runs, body) {
	const run = renew(runs);
	const $W = nursery(run);
	let $X = null;
	if ($W[0] === 0) {
		const nursery2 = $W[1];
		$X = (($Y) => {
			return (($Z) => {
				return body($Y, $Z);
			})(nursery2);
		})(run);
	} else {
		$X = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $X;
}
function $ap(self) {
	let $ar = null;
	if ($B(self)) {
		$ar = [ 1 ];
	} else {
		$ar = __list_get(self, self.length - 1);
	}
	return $ar;
}
function $y(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $F(runs, ($C, $D) => {
		return (($E) => {
			return body($C, $E, $D);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $u(runs, tracker, body) {
	let value = $y(runs, tracker, ($v, $w, $x) => {
		return body($v, $w, $x);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $y(runs, tracker, ($aF, $aG, $aH) => {
			return body($aF, $aG, $aH);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $m(self) {
	const transform = self[1];
	const upstream = $n(__clone(self[0]));
	const pull = upstream[0];
	const upstream_attach = upstream[1];
	const runs = new2();
	const tracker = new_tracker();
	return [ () => {
		const value = pull();
		return $u(runs, tracker, ($r, $s, $t) => {
			return transform(value, $r, $s, $t);
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
function $aQ(self, $aR) {
	const $aS = $aR;
	let $aT = null;
	if ($aS[0] === 0) {
		const turn = $aS[1];
		$aT = enqueue(turn, __clone(self[1].v));
	} else {
		const $aU = $ap(draining_turns.v);
		let $aV = null;
		if ($aU[0] === 0) {
			const draining = $aU[1];
			$aV = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aV = undefined;
		}
		$aT = $aV;
	}
	return $aT;
}
function $aO(self, value, $aP) {
	self[0].v = __clone(value);
	$aQ(self, $aP);
}
function $bc(self, item, $bd) {
	defer(self, () => {
		dispose(item, $bd);
		return;
	});
	return __clone(item);
}
function $j(self, $k, $l) {
	const instance = $m(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aO(cached, pull(), $k);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $k, $l);
	return cached;
}
function $g(self, $h, $i) {
	return [ $j(self, $h, $i) ];
}
function $bm(signal, observer) {
	const cell = signal[0];
	return $q(signal, mint_subscriber(() => {
		const $bn = [ 0, cell ];
		let $bo = null;
		if ($bn[0] === 0) {
			const live = $bn[1];
			$bo = observer(live.v);
		} else {
			$bo = undefined;
		}
		return $bo;
	}));
}
function $bl(self, observer, immediately) {
	const subscription = $bm(self, observer);
	if (immediately) {
		observer($o(self));
	}
	return subscription;
}
function $bk(self, observer, immediately) {
	return $bl(self[0], observer, immediately);
}
function $bj(self, observer) {
	return $bk(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $bp(self, transform, $bq) {
	$aO(self, transform($o(self)), $bq);
}
function $br(self) {
	return $o(self[0]);
}
function $bt(self, observer) {
	return $bl(self, (value) => {
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
const owner = new2();
const count = $a(0);
const doubled = $g($f(__clone(count), (n, $c, $d, $e) => {
	return n * 2;
}), [ 1 ], [ 1 ]);
$bc(owner, $bj(__clone(doubled), (n, $bi) => {
	return console.log(n);
}), [ 1 ]);
$aO(count, 1, [ 1 ]);
$bp(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($br(doubled));
$bc(owner, $bt(__clone(count), (n, $bs) => {
	return console.log(n);
}), [ 1 ]);
$aO(count, 20, [ 1 ]);
