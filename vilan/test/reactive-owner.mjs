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
		let $ad = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ad = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ad;
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
	const $ab = turn;
	let $ac = null;
	if ($ab[0] === 0) {
		const ambient = $ab[1];
		$ac = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ai = $af(draining_turns.v);
		let $aj = null;
		if ($ai[0] === 0) {
			const draining = $ai[1];
			$aj = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$aj = undefined;
		}
		$ac = $aj;
	}
	return $ac;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aX) {
	const $aY = $aX;
	let $aZ = null;
	if ($aY[0] === 0) {
		const established = $aY[1];
		$aZ = [ 0, established ];
	} else {
		$aZ = $af(draining_turns.v);
	}
	const ambient = $aZ;
	release_under(self, ambient);
}
function detach(handle) {
	const $an = $af(releasing_turns.v);
	let $ao = null;
	if ($an[0] === 0) {
		const at_release = $an[1];
		$ao = at_release;
	} else {
		$ao = $af(draining_turns.v);
	}
	const turn = $ao;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ap = [ 0, handle[0] ];
	let $aq = null;
	if ($ap[0] === 0) {
		const subscribers = $ap[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aq = undefined;
	} else {
		$aq = undefined;
	}
	$aq;
	const $ar = ambient;
	let $as = null;
	if ($ar[0] === 0) {
		const turn = $ar[1];
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
		$as = undefined;
	} else {
		$as = undefined;
	}
	$as;
	const $at = handle[3].v;
	let $au = null;
	if ($at[0] === 0) {
		const release = $at[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$au = undefined;
	} else {
		$au = undefined;
	}
	return $au;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ba = null;
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
		$ba = undefined;
	}
	return $ba;
}
function renew(self) {
	const live = self[0].v;
	const $w = live[3];
	let $x = null;
	if ($w[0] === 0) {
		const nursery2 = $w[1];
		let $y = null;
		if (has_spawned(nursery2)) {
			$y = [ 1 ];
		} else {
			$y = [ 0, __clone(nursery2) ];
		}
		$x = $y;
	} else {
		$x = [ 1 ];
	}
	const carried = $x;
	advance([ self[0], self[0].v[0] ], carried);
	const opened2 = self[0].v;
	const $L = opened2[3];
	let $M = null;
	if ($L[0] === 0) {
		$M = undefined;
	} else {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v = [ opened2[0], opened2[1], opened2[2], [ 0, detached_nursery() ] ];
		$M = undefined;
	}
	$M;
	return [ self[0], opened2[0] ];
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
	let $K = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $z = held[3];
		let $A = null;
		if ($z[0] === 0) {
			const nursery2 = $z[1];
			if ($B(carried)) {
				nursery2.cancel();
			}
			$A = undefined;
		} else {
			$A = undefined;
		}
		$A;
		let $J = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $D = __guarded(cleanup);
				let $E = null;
				if ($D[0] === 0) {
					const message = $D[1];
					if ($B(failure)) {
						failure = [ 0, message ];
					}
					$E = undefined;
				} else {
					$E = undefined;
				}
				$E;
			}
			const $H = failure;
			let $I = null;
			if ($H[0] === 0) {
				const message2 = $H[1];
				$I = (() => {
					throw message2;
				})();
			} else {
				$I = undefined;
			}
			$J = $I;
		}
		$K = $J;
	}
	return $K;
}
function get_owner($aU) {
	return $aU;
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
function has_run(tracker) {
	return tracker[0].v[0] > 0;
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
	const $S = tracker[4].v;
	let $T = null;
	if ($S[0] === 0) {
		const target = $S[1];
		$T = reconnect(tracker, target);
	} else {
		if (!($r(tracker[1].v)) || !($r(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$T = undefined;
	}
	$T;
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
		const $Z = reusable(held, kept, dependency[0], position);
		let $aa = null;
		if ($Z[0] === 0) {
			const index = $Z[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$aa = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$aa = undefined;
		}
		$aa;
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
	const $V = identity;
	let $W = null;
	if ($V[0] === 0) {
		const wanted = $V[1];
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
		$W = [ 1 ];
	} else {
		$W = [ 1 ];
	}
	return $W;
}
function same_identity(identity, wanted) {
	const $X = identity;
	let $Y = null;
	if ($X[0] === 0) {
		const held = $X[1];
		$Y = held === wanted;
	} else {
		$Y = false;
	}
	return $Y;
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
		$aL = undefined;
	}
	return $aL;
}
function forget_reads(tracker) {
	let $aR = null;
	if (!($r(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aR = undefined;
	}
	$aR;
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
		const $aS = previous;
		let $aT = null;
		if ($aS[0] === 0) {
			const earlier = $aS[1];
			$aT = earlier();
		} else {
			$aT = undefined;
		}
		return $aT;
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
function $r(self) {
	return self.length === 0;
}
function $B(self) {
	const $C = self;
	return $C[0] === 1;
}
function $v(runs, body) {
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
function $af(self) {
	let $ah = null;
	if ($r(self)) {
		$ah = [ 1 ];
	} else {
		$ah = __list_get(self, self.length - 1);
	}
	return $ah;
}
function $q(runs, tracker, body) {
	const scope2 = open_run(tracker);
	const value = $v(runs, ($s, $t) => {
		return (($u) => {
			return body($s, $u, $t);
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
		tracker[0].v = answered(tracker[0].v);
		value = $q(runs, tracker, ($av, $aw, $ax) => {
			return body($av, $aw, $ax);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $aD(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $aA(signal, observer) {
	const cell = signal[0];
	return $aD(signal, mint_subscriber(() => {
		const $aB = [ 0, cell ];
		let $aC = null;
		if ($aB[0] === 0) {
			const live = $aB[1];
			$aC = observer(live.v);
		} else {
			$aC = undefined;
		}
		return $aC;
	}));
}
function $aE(self) {
	return __clone(self[0].v);
}
function $az(self, observer, immediately) {
	const subscription = $aA(self, observer);
	if (immediately) {
		observer($aE(self));
	}
	return subscription;
}
function $ay(self, observer) {
	return $az(self, observer, true);
}
function $aV(self, item, $aW) {
	defer(self, () => {
		dispose(item, $aW);
		return;
	});
	return __clone(item);
}
function $g(self, body, $h, $i) {
	const runs = new2();
	const tracker = new_tracker();
	const latest = __shared_new([ 1 ]);
	const subscription = $ay(self, (value2) => {
		latest.v = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$m(runs, tracker, ($j, $k, $l) => {
				return body(value2, $j, $k, $l);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aK = null;
		if (tracker[0].v[2]) {
			const $aF = latest.v;
			let $aG = null;
			if ($aF[0] === 0) {
				const value2 = $aF[1];
				$aG = $m(runs, tracker, ($aH, $aI, $aJ) => {
					return body(value2, $aH, $aI, $aJ);
				});
			} else {
				$aG = undefined;
			}
			$aK = $aG;
		}
		return $aK;
	}, false);
	attach_tracker(tracker, rerun);
	const $aM = latest.v;
	let $aN = null;
	if ($aM[0] === 0) {
		const value = $aM[1];
		$aN = $m(runs, tracker, ($aO, $aP, $aQ) => {
			return body(value, $aO, $aP, $aQ);
		});
	} else {
		$aN = undefined;
	}
	$aN;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$aV(get_owner($i), subscription, $h);
}
function $bd(self, $be) {
	const $bf = $be;
	let $bg = null;
	if ($bf[0] === 0) {
		const turn = $bf[1];
		$bg = enqueue(turn, __clone(self[1].v));
	} else {
		const $bh = $af(draining_turns.v);
		let $bi = null;
		if ($bh[0] === 0) {
			const draining = $bh[1];
			$bi = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bi = undefined;
		}
		$bg = $bi;
	}
	return $bg;
}
function $bb(self, value, $bc) {
	self[0].v = __clone(value);
	$bd(self, $bc);
}
function $bv(owner2, body) {
	return body(owner2);
}
function $bA(body) {
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
$bb(count, 2, [ 1 ]);
dispose2(owner);
$bb(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($bj) => {
	(($bk) => {
		$g(__clone(count), (value, $bl, $bm, $bn) => {
			return console.log("inner " + value);
		}, [ 1 ], $bk);
		return;
	})(inner);
	$g(__clone(count), (value, $bo, $bp, $bq) => {
		return console.log("outer " + value);
	}, [ 1 ], $bj);
	return;
})(outer);
$bb(count, 4, [ 1 ]);
dispose2(inner);
$bb(count, 5, [ 1 ]);
dispose2(outer);
$bb(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$bv(wrapped, ($br) => {
	$g(__clone(count), (value, $bs, $bt, $bu) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $br);
	return;
});
$bb(count, 7, [ 1 ]);
dispose2(wrapped);
$bb(count, 8, [ 1 ]);
console.log("fin");
const $bB = $bA(($bw) => {
	$g(__clone(count), (value, $bx, $by, $bz) => {
		return console.log("comp " + value);
	}, [ 1 ], $bw);
	return "built";
});
const label = $bB[0];
const scope = $bB[1];
console.log(label);
$bb(count, 9, [ 1 ]);
dispose2(scope);
$bb(count, 10, [ 1 ]);
console.log("post");
