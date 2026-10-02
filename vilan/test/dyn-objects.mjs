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
	return $B(self[0].v) && $B(self[1].v);
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
function dispose(self, $aU) {
	const $aV = $aU;
	let $aW = null;
	if ($aV[0] === 0) {
		const established = $aV[1];
		$aW = [ 0, established ];
	} else {
		$aW = $ai(draining_turns.v);
	}
	const ambient = $aW;
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
	let $aX = null;
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
		$aX = undefined;
	}
	return $aX;
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
	let $Q = null;
	if (is_disposed(self)) {
		$Q = [ 1 ];
	} else {
		$Q = self[0].v[3];
	}
	return $Q;
}
function dispose2(self) {
	let $P = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $G = held[3];
		let $H = null;
		if ($G[0] === 0) {
			const nursery2 = $G[1];
			$H = nursery2.cancel();
		} else {
			$H = undefined;
		}
		$H;
		let $O = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $I = __guarded(cleanup);
				let $J = null;
				if ($I[0] === 0) {
					const message = $I[1];
					if ($K(failure)) {
						failure = [ 0, message ];
					}
					$J = undefined;
				} else {
					$J = undefined;
				}
				$J;
			}
			const $M = failure;
			let $N = null;
			if ($M[0] === 0) {
				const message2 = $M[1];
				$N = (() => {
					throw message2;
				})();
			} else {
				$N = undefined;
			}
			$O = $N;
		}
		$P = $O;
	}
	return $P;
}
function get_owner($aR) {
	return $aR;
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
	if (!($B(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v = closed(tracker[0].v);
	const $V = tracker[4].v;
	let $W = null;
	if ($V[0] === 0) {
		const target = $V[1];
		$W = reconnect(tracker, target);
	} else {
		if (!($B(tracker[1].v)) || !($B(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$W = undefined;
	}
	$W;
	if (!($B(tracker[1].v))) {
		tracker[1].v = [  ];
	}
}
function reconnect(tracker, target) {
	if ($B(tracker[1].v) && $B(tracker[3].v)) {
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
	let $aI = null;
	if (!($B(tracker[2].v))) {
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
		$aI = undefined;
	}
	return $aI;
}
function forget_reads(tracker) {
	let $aO = null;
	if (!($B(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aO = undefined;
	}
	$aO;
	if (!($B(tracker[2].v))) {
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
		const $aP = previous;
		let $aQ = null;
		if ($aP[0] === 0) {
			const earlier = $aP[1];
			$aQ = earlier();
		} else {
			$aQ = undefined;
		}
		return $aQ;
	} ];
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
	return [ __shared_new(__clone(value)), __shared_new(subscribers) ];
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
function $B(self) {
	return self.length === 0;
}
function $K(self) {
	const $L = self;
	return $L[0] === 1;
}
function $F(runs, body) {
	const run = renew(runs, true);
	const $R = nursery(run);
	let $S = null;
	if ($R[0] === 0) {
		const nursery2 = $R[1];
		$S = (($T) => {
			return (($U) => {
				return body($T, $U);
			})(nursery2);
		})(run);
	} else {
		$S = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $S;
}
function $ai(self) {
	let $ak = null;
	if ($B(self)) {
		$ak = [ 1 ];
	} else {
		$ak = __list_get(self, self.length - 1);
	}
	return $ak;
}
function $A(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $F(runs, ($C, $D) => {
		return (($E) => {
			return body($C, $E, $D);
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
		tracker[0].v = answered(tracker[0].v);
		value = $A(runs, tracker, ($ay, $az, $aA) => {
			return body($ay, $az, $aA);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $aB(self, observer) {
	return $j(self, observer, true);
}
function $aS(self, item, $aT) {
	defer(self, () => {
		dispose(item, $aT);
		return;
	});
	return __clone(item);
}
function $q(self, body, $r, $s) {
	const runs = new2();
	const tracker = new_tracker();
	const latest = __shared_new([ 1 ]);
	const subscription = $aB(self, (value2) => {
		latest.v = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$w(runs, tracker, ($t, $u, $v) => {
				return body(value2, $t, $u, $v);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aH = null;
		if (tracker[0].v[2]) {
			const $aC = latest.v;
			let $aD = null;
			if ($aC[0] === 0) {
				const value2 = $aC[1];
				$aD = $w(runs, tracker, ($aE, $aF, $aG) => {
					return body(value2, $aE, $aF, $aG);
				});
			} else {
				$aD = undefined;
			}
			$aH = $aD;
		}
		return $aH;
	}, false);
	attach_tracker(tracker, rerun);
	const $aJ = latest.v;
	let $aK = null;
	if ($aJ[0] === 0) {
		const value = $aJ[1];
		$aK = $w(runs, tracker, ($aL, $aM, $aN) => {
			return body(value, $aL, $aM, $aN);
		});
	} else {
		$aK = undefined;
	}
	$aK;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$aS(get_owner($s), subscription, $r);
}
const $f = Object.create({get: $g, on_settle: $h, attach_observer: $j, identity: $n, start: $o, on_change: $p, effect: $q});
function $aZ(self, observer, immediately) {
	const subscription = on_settle(self, mint_subscriber(() => {
		return observer(get(self));
	}));
	if (immediately) {
		observer(get(self));
	}
	return subscription;
}
function $ba(self) {
	return [ 1 ];
}
function $bb(self) {
	return [ () => {
		return get(self);
	}, (subscriber) => {
		return on_settle(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bc(self, observer) {
	return $aZ(self, observer, false);
}
function $be(self, observer) {
	return $aZ(self, observer, true);
}
function $bd(self, body, $r, $s) {
	const runs = new2();
	const tracker = new_tracker();
	const latest = __shared_new([ 1 ]);
	const subscription = $be(self, (value2) => {
		latest.v = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$w(runs, tracker, ($t, $u, $v) => {
				return body(value2, $t, $u, $v);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $bh = null;
		if (tracker[0].v[2]) {
			const $bf = latest.v;
			let $bg = null;
			if ($bf[0] === 0) {
				const value2 = $bf[1];
				$bg = $w(runs, tracker, ($aE, $aF, $aG) => {
					return body(value2, $aE, $aF, $aG);
				});
			} else {
				$bg = undefined;
			}
			$bh = $bg;
		}
		return $bh;
	}, false);
	attach_tracker(tracker, rerun);
	const $bi = latest.v;
	let $bj = null;
	if ($bi[0] === 0) {
		const value = $bi[1];
		$bj = $w(runs, tracker, ($aL, $aM, $aN) => {
			return body(value, $aL, $aM, $aN);
		});
	} else {
		$bj = undefined;
	}
	$bj;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$aS(get_owner($s), subscription, $r);
}
const $aY = Object.create({get: get, on_settle: on_settle, attach_observer: $aZ, identity: $ba, start: $bb, on_change: $bc, effect: $bd});
function $bn(self, $bo) {
	const $bp = $bo;
	let $bq = null;
	if ($bp[0] === 0) {
		const turn = $bp[1];
		$bq = enqueue(turn, __clone(self[1].v));
	} else {
		const $br = $ai(draining_turns.v);
		let $bs = null;
		if ($br[0] === 0) {
			const draining = $br[1];
			$bs = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bs = undefined;
		}
		$bq = $bs;
	}
	return $bq;
}
function $bl(self, value, $bm) {
	self[0].v = __clone(value);
	$bn(self, $bm);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
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
const sources = [ __clone([ root, $f ]), [ [ __clone([ root, $f ]), 100 ], $aY ] ];
const watch = (($bk) => {
	return $bk[1].on_change($bk[0], (n) => {
		return console.log("offset saw " + n);
	});
})(__clone(__at(sources, 1)));
$bl(root, 5, [ 1 ]);
for (const source of sources) {
	console.log(source[1].get(source[0]));
}
dispose(watch, [ 1 ]);
