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
	return $u(self[0].v) && $u(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $ag = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ag = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ag;
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
				while (!($u(turn[1].v)) && budget > 0) {
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
	const $ae = turn;
	let $af = null;
	if ($ae[0] === 0) {
		const ambient = $ae[1];
		$af = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $al = $ai(draining_turns.v);
		let $am = null;
		if ($al[0] === 0) {
			const draining = $al[1];
			$am = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$am = undefined;
		}
		$af = $am;
	}
	return $af;
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
		$bg = $ai(draining_turns.v);
	}
	const ambient = $bg;
	release_under(self, ambient);
}
function detach(handle) {
	const $aq = $ai(releasing_turns.v);
	let $ar = null;
	if ($aq[0] === 0) {
		const at_release = $aq[1];
		$ar = at_release;
	} else {
		$ar = $ai(draining_turns.v);
	}
	const turn = $ar;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $as = [ 0, handle[0] ];
	let $at = null;
	if ($as[0] === 0) {
		const subscribers = $as[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$at = undefined;
	} else {
		$at = undefined;
	}
	$at;
	const $au = ambient;
	let $av = null;
	if ($au[0] === 0) {
		const turn = $au[1];
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
		$av = undefined;
	} else {
		$av = undefined;
	}
	$av;
	const $aw = handle[3].v;
	let $ax = null;
	if ($aw[0] === 0) {
		const release = $aw[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ax = undefined;
	} else {
		$ax = undefined;
	}
	return $ax;
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
	const $z = self[0].v[3];
	let $A = null;
	if ($z[0] === 0) {
		const nursery2 = __clone($z[1]);
		let $B = null;
		if (has_spawned(nursery2)) {
			$B = [ 1 ];
		} else {
			$B = [ 0, nursery2 ];
		}
		$A = $B;
	} else {
		$A = [ 1 ];
	}
	const carried = $A;
	advance([ self[0], self[0].v[0] ], carried);
	if ($E(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $O = null;
	if (is_disposed(self)) {
		$O = [ 1 ];
	} else {
		$O = self[0].v[3];
	}
	return $O;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $N = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $C = held[3];
		let $D = null;
		if ($C[0] === 0) {
			const nursery2 = $C[1];
			if ($E(carried)) {
				nursery2.cancel();
			}
			$D = undefined;
		} else {
			$D = undefined;
		}
		$D;
		let $M = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $G = __guarded(cleanup);
				let $H = null;
				if ($G[0] === 0) {
					const message = $G[1];
					if ($E(failure)) {
						failure = [ 0, message ];
					}
					$H = undefined;
				} else {
					$H = undefined;
				}
				$H;
			}
			const $K = failure;
			let $L = null;
			if ($K[0] === 0) {
				const message2 = $K[1];
				$L = (() => {
					throw message2;
				})();
			} else {
				$L = undefined;
			}
			$M = $L;
		}
		$N = $M;
	}
	return $N;
}
function get_owner($bb) {
	return $bb;
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
	const $s = tracker[0].v[6];
	let $t = null;
	if ($s[0] === 0) {
		const lists = $s[1];
		if (!($u(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$t = undefined;
	} else {
		$t = undefined;
	}
	$t;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $T = tracker[0].v[6];
	let $U = null;
	if ($T[0] === 0) {
		const lists = $T[1];
		$U = lists;
	} else {
		return;
		$U = undefined;
	}
	const lists2 = $U;
	const $V = tracker[0].v[5];
	let $W = null;
	if ($V[0] === 0) {
		const target = __clone($V[1]);
		$W = reconnect(tracker, lists2, target);
	} else {
		if (!($u(lists2.v[0])) || !($u(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$W = undefined;
	}
	$W;
	if (!($u(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($u(lists.v[0]) && $u(lists.v[2])) {
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
		const $ac = reusable(held, kept, dependency[0], position);
		let $ad = null;
		if ($ac[0] === 0) {
			const index = $ac[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$ad = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ad = undefined;
		}
		$ad;
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
	const $Y = identity;
	let $Z = null;
	if ($Y[0] === 0) {
		const wanted = $Y[1];
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
		$Z = [ 1 ];
	} else {
		$Z = [ 1 ];
	}
	return $Z;
}
function same_identity(identity, wanted) {
	const $aa = identity;
	let $ab = null;
	if ($aa[0] === 0) {
		const held = $aa[1];
		$ab = held === wanted;
	} else {
		$ab = false;
	}
	return $ab;
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
	if (!($u(lists2.v[1]))) {
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
		$aQ = undefined;
	}
	return $aQ;
}
function forget_reads(tracker) {
	const $aW = tracker[0].v[6];
	let $aX = null;
	if ($aW[0] === 0) {
		const lists = $aW[1];
		$aX = lists;
	} else {
		return;
		$aX = undefined;
	}
	const lists2 = $aX;
	let $aY = null;
	if (!($u(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aY = undefined;
	}
	$aY;
	if (!($u(lists2.v[1]))) {
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
function $u(self) {
	return self.length === 0;
}
function $E(self) {
	const $F = self;
	return $F[0] === 1;
}
function $y(runs, body) {
	const run = renew(runs);
	const $P = nursery(run);
	let $Q = null;
	if ($P[0] === 0) {
		const nursery2 = $P[1];
		$Q = (($R) => {
			return (($S) => {
				return body($R, $S);
			})(nursery2);
		})(run);
	} else {
		$Q = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $Q;
}
function $ai(self) {
	let $ak = null;
	if ($u(self)) {
		$ak = [ 1 ];
	} else {
		$ak = __list_get(self, self.length - 1);
	}
	return $ak;
}
function $r(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $y(runs, ($v, $w) => {
		return (($x) => {
			return body($v, $x, $w);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $n(runs, tracker, body) {
	let value = $r(runs, tracker, ($o, $p, $q) => {
		return body($o, $p, $q);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $r(runs, tracker, ($ay, $az, $aA) => {
			return body($ay, $az, $aA);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aG(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $aD(signal, observer) {
	const cell = signal[0];
	return $aG(signal, mint_subscriber(() => {
		const $aE = [ 0, cell ];
		let $aF = null;
		if ($aE[0] === 0) {
			const live = $aE[1];
			$aF = observer(live.v);
		} else {
			$aF = undefined;
		}
		return $aF;
	}));
}
function $aH(self) {
	return __clone(self[0].v);
}
function $aC(self, observer, immediately) {
	const subscription = $aD(self, observer);
	if (immediately) {
		observer($aH(self));
	}
	return subscription;
}
function $aB(self, observer) {
	return $aC(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $bc(self, item, $bd) {
	defer(self, () => {
		dispose(item, $bd);
		return;
	});
	return __clone(item);
}
function $g(self, body, $h, $i) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $aB(self, (value2, $j) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$n(runs, tracker, ($k, $l, $m) => {
				return body(value2, $k, $l, $m);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aN = null;
		if (tracker[0].v[2]) {
			const $aI = latest;
			let $aJ = null;
			if ($aI[0] === 0) {
				const value2 = __clone($aI[1]);
				$aJ = $n(runs, tracker, ($aK, $aL, $aM) => {
					return body(value2, $aK, $aL, $aM);
				});
			} else {
				$aJ = undefined;
			}
			$aN = $aJ;
		}
		return $aN;
	}, false);
	attach_tracker(tracker, rerun);
	const $aR = latest;
	let $aS = null;
	if ($aR[0] === 0) {
		const value = __clone($aR[1]);
		$aS = $n(runs, tracker, ($aT, $aU, $aV) => {
			return body(value, $aT, $aU, $aV);
		});
	} else {
		$aS = undefined;
	}
	$aS;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bc(get_owner($i), subscription, $h);
}
function $bk(self, $bl) {
	const $bm = $bl;
	let $bn = null;
	if ($bm[0] === 0) {
		const turn = $bm[1];
		$bn = enqueue(turn, __clone(self[1].v));
	} else {
		const $bo = $ai(draining_turns.v);
		let $bp = null;
		if ($bo[0] === 0) {
			const draining = $bo[1];
			$bp = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bp = undefined;
		}
		$bn = $bp;
	}
	return $bn;
}
function $bi(self, value, $bj) {
	self[0].v = __clone(value);
	$bk(self, $bj);
}
function $bC(owner2, body) {
	return body(owner2);
}
function $bH(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const run_nurseries_allocated_count = __shared_new(0);
const count = $a(1);
const owner = new2();
(($c) => {
	$g(__clone(count), (value, $d, $e, $f) => {
		return console.log("seen " + value);
	}, [ 1 ], $c);
	return;
})(owner);
$bi(count, 2, [ 1 ]);
dispose2(owner);
$bi(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($bq) => {
	(($br) => {
		$g(__clone(count), (value, $bs, $bt, $bu) => {
			return console.log("inner " + value);
		}, [ 1 ], $br);
		return;
	})(inner);
	$g(__clone(count), (value, $bv, $bw, $bx) => {
		return console.log("outer " + value);
	}, [ 1 ], $bq);
	return;
})(outer);
$bi(count, 4, [ 1 ]);
dispose2(inner);
$bi(count, 5, [ 1 ]);
dispose2(outer);
$bi(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$bC(wrapped, ($by) => {
	$g(__clone(count), (value, $bz, $bA, $bB) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $by);
	return;
});
$bi(count, 7, [ 1 ]);
dispose2(wrapped);
$bi(count, 8, [ 1 ]);
console.log("fin");
const $bI = $bH(($bD) => {
	$g(__clone(count), (value, $bE, $bF, $bG) => {
		return console.log("comp " + value);
	}, [ 1 ], $bD);
	return "built";
});
const label = $bI[0];
const scope = $bI[1];
console.log(label);
$bi(count, 9, [ 1 ]);
dispose2(scope);
$bi(count, 10, [ 1 ]);
console.log("post");
