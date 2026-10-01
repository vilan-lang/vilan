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
	return $t(self[0].v) && $t(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $af = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$af = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$af;
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
	const $ad = turn;
	let $ae = null;
	if ($ad[0] === 0) {
		const ambient = $ad[1];
		$ae = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ak = $ah(draining_turns.v);
		let $al = null;
		if ($ak[0] === 0) {
			const draining = $ak[1];
			$al = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$al = undefined;
		}
		$ae = $al;
	}
	return $ae;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $bd) {
	const $be = $bd;
	let $bf = null;
	if ($be[0] === 0) {
		const established = $be[1];
		$bf = [ 0, established ];
	} else {
		$bf = $ah(draining_turns.v);
	}
	const ambient = $bf;
	release_under(self, ambient);
}
function detach(handle) {
	const $ap = $ah(releasing_turns.v);
	let $aq = null;
	if ($ap[0] === 0) {
		const at_release = $ap[1];
		$aq = at_release;
	} else {
		$aq = $ah(draining_turns.v);
	}
	const turn = $aq;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ar = [ 0, handle[0] ];
	let $as = null;
	if ($ar[0] === 0) {
		const subscribers = $ar[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$as = undefined;
	} else {
		$as = undefined;
	}
	$as;
	const $at = ambient;
	let $au = null;
	if ($at[0] === 0) {
		const turn = $at[1];
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
		$au = undefined;
	} else {
		$au = undefined;
	}
	$au;
	const $av = handle[3].v;
	let $aw = null;
	if ($av[0] === 0) {
		const release = $av[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	return $aw;
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
		if (self[0].v[2]) {
			self[0].v[1].v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = __shared_new([ cleanup ]);
			self[0].v[2] = true;
		}
		$bg = undefined;
	}
	return $bg;
}
function renew(self) {
	const $y = self[0].v[3];
	let $z = null;
	if ($y[0] === 0) {
		const nursery2 = __clone($y[1]);
		let $A = null;
		if (has_spawned(nursery2)) {
			$A = [ 1 ];
		} else {
			$A = [ 0, nursery2 ];
		}
		$z = $A;
	} else {
		$z = [ 1 ];
	}
	const carried = $z;
	advance([ self[0], self[0].v[0] ], carried);
	if ($D(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $N = null;
	if (is_disposed(self)) {
		$N = [ 1 ];
	} else {
		$N = self[0].v[3];
	}
	return $N;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $M = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $B = held[3];
		let $C = null;
		if ($B[0] === 0) {
			const nursery2 = $B[1];
			if ($D(carried)) {
				nursery2.cancel();
			}
			$C = undefined;
		} else {
			$C = undefined;
		}
		$C;
		let $L = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $F = __guarded(cleanup);
				let $G = null;
				if ($F[0] === 0) {
					const message = $F[1];
					if ($D(failure)) {
						failure = [ 0, message ];
					}
					$G = undefined;
				} else {
					$G = undefined;
				}
				$G;
			}
			const $J = failure;
			let $K = null;
			if ($J[0] === 0) {
				const message2 = $J[1];
				$K = (() => {
					throw message2;
				})();
			} else {
				$K = undefined;
			}
			$L = $K;
		}
		$M = $L;
	}
	return $M;
}
function get_owner($ba) {
	return $ba;
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
	const $r = tracker[0].v[6];
	let $s = null;
	if ($r[0] === 0) {
		const lists = $r[1];
		if (!($t(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$s = undefined;
	} else {
		$s = undefined;
	}
	$s;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $S = tracker[0].v[6];
	let $T = null;
	if ($S[0] === 0) {
		const lists = $S[1];
		$T = lists;
	} else {
		return;
		$T = undefined;
	}
	const lists2 = $T;
	const $U = tracker[0].v[5];
	let $V = null;
	if ($U[0] === 0) {
		const target = __clone($U[1]);
		$V = reconnect(tracker, lists2, target);
	} else {
		if (!($t(lists2.v[0])) || !($t(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$V = undefined;
	}
	$V;
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
		const $ab = reusable(held, kept, dependency[0], position);
		let $ac = null;
		if ($ab[0] === 0) {
			const index = $ab[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$ac = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ac = undefined;
		}
		$ac;
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
	const $X = identity;
	let $Y = null;
	if ($X[0] === 0) {
		const wanted = $X[1];
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
		$Y = [ 1 ];
	} else {
		$Y = [ 1 ];
	}
	return $Y;
}
function same_identity(identity, wanted) {
	const $Z = identity;
	let $aa = null;
	if ($Z[0] === 0) {
		const held = $Z[1];
		$aa = held === wanted;
	} else {
		$aa = false;
	}
	return $aa;
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
	const $aN = tracker[0].v[6];
	let $aO = null;
	if ($aN[0] === 0) {
		const lists = $aN[1];
		$aO = lists;
	} else {
		return;
		$aO = undefined;
	}
	const lists2 = $aO;
	let $aP = null;
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
		$aP = undefined;
	}
	return $aP;
}
function forget_reads(tracker) {
	const $aV = tracker[0].v[6];
	let $aW = null;
	if ($aV[0] === 0) {
		const lists = $aV[1];
		$aW = lists;
	} else {
		return;
		$aW = undefined;
	}
	const lists2 = $aW;
	let $aX = null;
	if (!($t(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aX = undefined;
	}
	$aX;
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
		const $aY = previous;
		let $aZ = null;
		if ($aY[0] === 0) {
			const earlier = $aY[1];
			$aZ = earlier();
		} else {
			$aZ = undefined;
		}
		return $aZ;
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
function $t(self) {
	return self.length === 0;
}
function $D(self) {
	const $E = self;
	return $E[0] === 1;
}
function $x(runs, body) {
	const run = renew(runs);
	const $O = nursery(run);
	let $P = null;
	if ($O[0] === 0) {
		const nursery2 = $O[1];
		$P = (($Q) => {
			return (($R) => {
				return body($Q, $R);
			})(nursery2);
		})(run);
	} else {
		$P = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $P;
}
function $ah(self) {
	let $aj = null;
	if ($t(self)) {
		$aj = [ 1 ];
	} else {
		$aj = __list_get(self, self.length - 1);
	}
	return $aj;
}
function $q(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $x(runs, ($u, $v) => {
		return (($w) => {
			return body($u, $w, $v);
		})(scope2);
	});
	close_run(tracker);
	return value;
}
function $m(runs, tracker, body) {
	let value = $q(runs, tracker, ($n, $o, $p) => {
		return body($n, $o, $p);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $q(runs, tracker, ($ax, $ay, $az) => {
			return body($ax, $ay, $az);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aF(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $aC(signal, observer) {
	const cell = signal[0];
	return $aF(signal, mint_subscriber(() => {
		const $aD = [ 0, cell ];
		let $aE = null;
		if ($aD[0] === 0) {
			const live = $aD[1];
			$aE = observer(live.v);
		} else {
			$aE = undefined;
		}
		return $aE;
	}));
}
function $aG(self) {
	return __clone(self[0].v);
}
function $aB(self, observer, immediately) {
	const subscription = $aC(self, observer);
	if (immediately) {
		observer($aG(self));
	}
	return subscription;
}
function $aA(self, observer) {
	return $aB(self, observer, true);
}
function $bb(self, item, $bc) {
	defer(self, () => {
		dispose(item, $bc);
		return;
	});
	return __clone(item);
}
function $g(self, body, $h, $i) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $aA(self, (value2) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$m(runs, tracker, ($j, $k, $l) => {
				return body(value2, $j, $k, $l);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aM = null;
		if (tracker[0].v[2]) {
			const $aH = latest;
			let $aI = null;
			if ($aH[0] === 0) {
				const value2 = __clone($aH[1]);
				$aI = $m(runs, tracker, ($aJ, $aK, $aL) => {
					return body(value2, $aJ, $aK, $aL);
				});
			} else {
				$aI = undefined;
			}
			$aM = $aI;
		}
		return $aM;
	}, false);
	attach_tracker(tracker, rerun);
	const $aQ = latest;
	let $aR = null;
	if ($aQ[0] === 0) {
		const value = __clone($aQ[1]);
		$aR = $m(runs, tracker, ($aS, $aT, $aU) => {
			return body(value, $aS, $aT, $aU);
		});
	} else {
		$aR = undefined;
	}
	$aR;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bb(get_owner($i), subscription, $h);
}
function $bj(self, $bk) {
	const $bl = $bk;
	let $bm = null;
	if ($bl[0] === 0) {
		const turn = $bl[1];
		$bm = enqueue(turn, __clone(self[1].v));
	} else {
		const $bn = $ah(draining_turns.v);
		let $bo = null;
		if ($bn[0] === 0) {
			const draining = $bn[1];
			$bo = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bo = undefined;
		}
		$bm = $bo;
	}
	return $bm;
}
function $bh(self, value, $bi) {
	self[0].v = __clone(value);
	$bj(self, $bi);
}
function $bB(owner2, body) {
	return body(owner2);
}
function $bG(body) {
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
$bh(count, 2, [ 1 ]);
dispose2(owner);
$bh(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($bp) => {
	(($bq) => {
		$g(__clone(count), (value, $br, $bs, $bt) => {
			return console.log("inner " + value);
		}, [ 1 ], $bq);
		return;
	})(inner);
	$g(__clone(count), (value, $bu, $bv, $bw) => {
		return console.log("outer " + value);
	}, [ 1 ], $bp);
	return;
})(outer);
$bh(count, 4, [ 1 ]);
dispose2(inner);
$bh(count, 5, [ 1 ]);
dispose2(outer);
$bh(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$bB(wrapped, ($bx) => {
	$g(__clone(count), (value, $by, $bz, $bA) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $bx);
	return;
});
$bh(count, 7, [ 1 ]);
dispose2(wrapped);
$bh(count, 8, [ 1 ]);
console.log("fin");
const $bH = $bG(($bC) => {
	$g(__clone(count), (value, $bD, $bE, $bF) => {
		return console.log("comp " + value);
	}, [ 1 ], $bC);
	return "built";
});
const label = $bH[0];
const scope = $bH[1];
console.log(label);
$bh(count, 9, [ 1 ]);
dispose2(scope);
$bh(count, 10, [ 1 ]);
console.log("post");
