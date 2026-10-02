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
	return $D(self[0].v) && $D(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $ap = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ap = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ap;
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
				while (!($D(turn[1].v)) && budget > 0) {
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
	const $an = turn;
	let $ao = null;
	if ($an[0] === 0) {
		const ambient = $an[1];
		$ao = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $au = $ar(draining_turns.v);
		let $av = null;
		if ($au[0] === 0) {
			const draining = $au[1];
			$av = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$av = undefined;
		}
		$ao = $av;
	}
	return $ao;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $bh) {
	const $bi = $bh;
	let $bj = null;
	if ($bi[0] === 0) {
		const established = $bi[1];
		$bj = [ 0, established ];
	} else {
		$bj = $ar(draining_turns.v);
	}
	const ambient = $bj;
	release_under(self, ambient);
}
function detach(handle) {
	const $az = $ar(releasing_turns.v);
	let $aA = null;
	if ($az[0] === 0) {
		const at_release = $az[1];
		$aA = at_release;
	} else {
		$aA = $ar(draining_turns.v);
	}
	const turn = $aA;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aB = [ 0, handle[0] ];
	let $aC = null;
	if ($aB[0] === 0) {
		const subscribers = $aB[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aC = undefined;
	} else {
		$aC = undefined;
	}
	$aC;
	const $aD = ambient;
	let $aE = null;
	if ($aD[0] === 0) {
		const turn = $aD[1];
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
		$aE = undefined;
	} else {
		$aE = undefined;
	}
	$aE;
	const $aF = handle[3].v;
	let $aG = null;
	if ($aF[0] === 0) {
		const release = $aF[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aG = undefined;
	} else {
		$aG = undefined;
	}
	return $aG;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $bk = null;
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
		$bk = undefined;
	}
	return $bk;
}
function renew(self) {
	const $I = self[0].v[3];
	let $J = null;
	if ($I[0] === 0) {
		const nursery2 = __clone($I[1]);
		let $K = null;
		if (has_spawned(nursery2)) {
			$K = [ 1 ];
		} else {
			$K = [ 0, nursery2 ];
		}
		$J = $K;
	} else {
		$J = [ 1 ];
	}
	const carried = $J;
	advance([ self[0], self[0].v[0] ], carried);
	if ($N(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $X = null;
	if (is_disposed(self)) {
		$X = [ 1 ];
	} else {
		$X = self[0].v[3];
	}
	return $X;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $W = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $L = held[3];
		let $M = null;
		if ($L[0] === 0) {
			const nursery2 = $L[1];
			if ($N(carried)) {
				nursery2.cancel();
			}
			$M = undefined;
		} else {
			$M = undefined;
		}
		$M;
		let $V = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $P = __guarded(cleanup);
				let $Q = null;
				if ($P[0] === 0) {
					const message = $P[1];
					if ($N(failure)) {
						failure = [ 0, message ];
					}
					$Q = undefined;
				} else {
					$Q = undefined;
				}
				$Q;
			}
			const $T = failure;
			let $U = null;
			if ($T[0] === 0) {
				const message2 = $T[1];
				$U = (() => {
					throw message2;
				})();
			} else {
				$U = undefined;
			}
			$V = $U;
		}
		$W = $V;
	}
	return $W;
}
function get_owner($be) {
	return $be;
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
	const $B = tracker[0].v[6];
	let $C = null;
	if ($B[0] === 0) {
		const lists = $B[1];
		if (!($D(lists.v[0]))) {
			lists.v[0] = [  ];
		}
		$C = undefined;
	} else {
		$C = undefined;
	}
	$C;
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $ac = tracker[0].v[6];
	let $ad = null;
	if ($ac[0] === 0) {
		const lists = $ac[1];
		$ad = lists;
	} else {
		return;
		$ad = undefined;
	}
	const lists2 = $ad;
	const $ae = tracker[0].v[5];
	let $af = null;
	if ($ae[0] === 0) {
		const target = __clone($ae[1]);
		$af = reconnect(tracker, lists2, target);
	} else {
		if (!($D(lists2.v[0])) || !($D(lists2.v[1]))) {
			lists2.v[1] = __clone(lists2.v[0]);
		}
		$af = undefined;
	}
	$af;
	if (!($D(lists2.v[0]))) {
		lists2.v[0] = [  ];
	}
}
function reconnect(tracker, lists, target) {
	if ($D(lists.v[0]) && $D(lists.v[2])) {
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
		const $al = reusable(held, kept, dependency[0], position);
		let $am = null;
		if ($al[0] === 0) {
			const index = $al[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$am = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$am = undefined;
		}
		$am;
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
	const $ah = identity;
	let $ai = null;
	if ($ah[0] === 0) {
		const wanted = $ah[1];
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
		$ai = [ 1 ];
	} else {
		$ai = [ 1 ];
	}
	return $ai;
}
function same_identity(identity, wanted) {
	const $aj = identity;
	let $ak = null;
	if ($aj[0] === 0) {
		const held = $aj[1];
		$ak = held === wanted;
	} else {
		$ak = false;
	}
	return $ak;
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
	const $aR = tracker[0].v[6];
	let $aS = null;
	if ($aR[0] === 0) {
		const lists = $aR[1];
		$aS = lists;
	} else {
		return;
		$aS = undefined;
	}
	const lists2 = $aS;
	let $aT = null;
	if (!($D(lists2.v[1]))) {
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
		$aT = undefined;
	}
	return $aT;
}
function forget_reads(tracker) {
	const $aZ = tracker[0].v[6];
	let $ba = null;
	if ($aZ[0] === 0) {
		const lists = $aZ[1];
		$ba = lists;
	} else {
		return;
		$ba = undefined;
	}
	const lists2 = $ba;
	let $bb = null;
	if (!($D(lists2.v[2]))) {
		const edges = __clone(lists2.v[2]);
		lists2.v[2] = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$bb = undefined;
	}
	$bb;
	if (!($D(lists2.v[1]))) {
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
		const $bc = previous;
		let $bd = null;
		if ($bc[0] === 0) {
			const earlier = $bc[1];
			$bd = earlier();
		} else {
			$bd = undefined;
		}
		return $bd;
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
	return $j(self, observer, false);
}
function $D(self) {
	return self.length === 0;
}
function $N(self) {
	const $O = self;
	return $O[0] === 1;
}
function $H(runs, body) {
	const run = renew(runs);
	const $Y = nursery(run);
	let $Z = null;
	if ($Y[0] === 0) {
		const nursery2 = $Y[1];
		$Z = (($aa) => {
			return (($ab) => {
				return body($aa, $ab);
			})(nursery2);
		})(run);
	} else {
		$Z = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $Z;
}
function $ar(self) {
	let $at = null;
	if ($D(self)) {
		$at = [ 1 ];
	} else {
		$at = __list_get(self, self.length - 1);
	}
	return $at;
}
function $A(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $H(runs, ($E, $F) => {
		return (($G) => {
			return body($E, $G, $F);
		})(scope);
	});
	close_run(tracker);
	return value;
}
function $w(runs, tracker, body) {
	let value = $A(runs, tracker, ($x, $y, $z) => {
		return body($x, $y, $z);
	});
	let rounds = 0;
	while (tracker[0].v[4] && rounds < 100) {
		tracker[0].v[4] = false;
		value = $A(runs, tracker, ($aH, $aI, $aJ) => {
			return body($aH, $aI, $aJ);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aK(self, observer) {
	return $j(self, observer, true);
}
function $bf(self, item, $bg) {
	defer(self, () => {
		dispose(item, $bg);
		return;
	});
	return __clone(item);
}
function $q(self, body, $r, $s) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $aK(self, (value2) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$w(runs, tracker, ($t, $u, $v) => {
				return body(value2, $t, $u, $v);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aQ = null;
		if (tracker[0].v[2]) {
			const $aL = latest;
			let $aM = null;
			if ($aL[0] === 0) {
				const value2 = __clone($aL[1]);
				$aM = $w(runs, tracker, ($aN, $aO, $aP) => {
					return body(value2, $aN, $aO, $aP);
				});
			} else {
				$aM = undefined;
			}
			$aQ = $aM;
		}
		return $aQ;
	}, false);
	attach_tracker(tracker, rerun);
	const $aU = latest;
	let $aV = null;
	if ($aU[0] === 0) {
		const value = __clone($aU[1]);
		$aV = $w(runs, tracker, ($aW, $aX, $aY) => {
			return body(value, $aW, $aX, $aY);
		});
	} else {
		$aV = undefined;
	}
	$aV;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bf(get_owner($s), subscription, $r);
}
const $f = Object.create({get: $g, on_settle: $h, attach_observer: $j, identity: $n, start: $o, on_change: $p, effect: $q});
function $bm(self, observer, immediately) {
	const subscription = on_settle(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function $bn(self) {
	return [ 1 ];
}
function $bo(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bp(self, observer) {
	return $bm(self, observer, false);
}
function $br(self, observer) {
	return $bm(self, observer, true);
}
function $bq(self, body, $r, $s) {
	const runs = new2();
	const tracker = new_tracker();
	let latest = [ 1 ];
	const subscription = $br(self, (value2) => {
		latest = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$w(runs, tracker, ($t, $u, $v) => {
				return body(value2, $t, $u, $v);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $bu = null;
		if (tracker[0].v[2]) {
			const $bs = latest;
			let $bt = null;
			if ($bs[0] === 0) {
				const value2 = __clone($bs[1]);
				$bt = $w(runs, tracker, ($aN, $aO, $aP) => {
					return body(value2, $aN, $aO, $aP);
				});
			} else {
				$bt = undefined;
			}
			$bu = $bt;
		}
		return $bu;
	}, false);
	attach_tracker(tracker, rerun);
	const $bv = latest;
	let $bw = null;
	if ($bv[0] === 0) {
		const value = __clone($bv[1]);
		$bw = $w(runs, tracker, ($aW, $aX, $aY) => {
			return body(value, $aW, $aX, $aY);
		});
	} else {
		$bw = undefined;
	}
	$bw;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$bf(get_owner($s), subscription, $r);
}
const $bl = Object.create({get: get, on_settle: on_settle, attach_observer: $bm, identity: $bn, start: $bo, on_change: $bp, effect: $bq});
function $bA(self, $bB) {
	const $bC = $bB;
	let $bD = null;
	if ($bC[0] === 0) {
		const turn = $bC[1];
		$bD = enqueue(turn, __clone(self[1].v));
	} else {
		const $bE = $ar(draining_turns.v);
		let $bF = null;
		if ($bE[0] === 0) {
			const draining = $bE[1];
			$bF = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bF = undefined;
		}
		$bD = $bF;
	}
	return $bD;
}
function $by(self, value, $bz) {
	self[0].v = __clone(value);
	$bA(self, $bz);
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
console.log(__at(slots, 0));
const root = $e(1);
const sources = [ __clone([ root, $f ]), [ [ __clone([ root, $f ]), 100 ], $bl ] ];
const watch = (($bx) => {
	return $bx[1].on_change($bx[0], (n) => {
		return console.log("offset saw " + n);
	});
})(__clone(__at(sources, 1)));
$by(root, 5, [ 1 ]);
for (const source of sources) {
	console.log(source[1].get(source[0]));
}
dispose(watch, [ 1 ]);
