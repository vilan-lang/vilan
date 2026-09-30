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
	return $z(self[0].v) && $z(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $ae = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ae = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ae;
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
				while (!($z(turn[1].v)) && budget > 0) {
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
	const $ac = turn;
	let $ad = null;
	if ($ac[0] === 0) {
		const ambient = $ac[1];
		$ad = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $aj = $ag(draining_turns.v);
		let $ak = null;
		if ($aj[0] === 0) {
			const draining = $aj[1];
			$ak = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ak = undefined;
		}
		$ad = $ak;
	}
	return $ad;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aR) {
	const $aS = $aR;
	let $aT = null;
	if ($aS[0] === 0) {
		const established = $aS[1];
		$aT = [ 0, established ];
	} else {
		$aT = $ag(draining_turns.v);
	}
	const ambient = $aT;
	release_under(self, ambient);
}
function detach(handle) {
	const $ao = $ag(releasing_turns.v);
	let $ap = null;
	if ($ao[0] === 0) {
		const at_release = $ao[1];
		$ap = at_release;
	} else {
		$ap = $ag(draining_turns.v);
	}
	const turn = $ap;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $aq = [ 0, handle[0] ];
	let $ar = null;
	if ($aq[0] === 0) {
		const subscribers = $aq[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ar = undefined;
	} else {
		$ar = undefined;
	}
	$ar;
	const $as = ambient;
	let $at = null;
	if ($as[0] === 0) {
		const turn = $as[1];
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
		$at = undefined;
	} else {
		$at = undefined;
	}
	$at;
	const $au = handle[3].v;
	let $av = null;
	if ($au[0] === 0) {
		const release = $au[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$av = undefined;
	} else {
		$av = undefined;
	}
	return $av;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aU = null;
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
		$aU = undefined;
	}
	return $aU;
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
	let $O = null;
	if (is_disposed(self)) {
		$O = [ 1 ];
	} else {
		$O = self[0].v[3];
	}
	return $O;
}
function dispose2(self) {
	let $N = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $E = held[3];
		let $F = null;
		if ($E[0] === 0) {
			const nursery2 = $E[1];
			$F = nursery2.cancel();
		} else {
			$F = undefined;
		}
		$F;
		let $M = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $G = __guarded(cleanup);
				let $H = null;
				if ($G[0] === 0) {
					const message = $G[1];
					if ($I(failure)) {
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
function register_with_owner(subscription, $aL, $aM) {
	const $aN = $aM;
	let $aO = null;
	if ($aN[0] === 0) {
		const owner2 = $aN[1];
		$aO = $aP(owner2, subscription, $aL);
	} else {
		$aO = __clone(subscription);
	}
	return $aO;
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
	if (!($z(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v = closed(tracker[0].v);
	const $T = tracker[4].v;
	let $U = null;
	if ($T[0] === 0) {
		const target = $T[1];
		$U = reconnect(tracker, target);
	} else {
		if (!($z(tracker[1].v)) || !($z(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$U = undefined;
	}
	$U;
	if (!($z(tracker[1].v))) {
		tracker[1].v = [  ];
	}
}
function reconnect(tracker, target) {
	if ($z(tracker[1].v) && $z(tracker[3].v)) {
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
		const $aa = reusable(held, kept, dependency[0], position);
		let $ab = null;
		if ($aa[0] === 0) {
			const index = $aa[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$ab = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ab = undefined;
		}
		$ab;
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
	const $W = identity;
	let $X = null;
	if ($W[0] === 0) {
		const wanted = $W[1];
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
		$X = [ 1 ];
	} else {
		$X = [ 1 ];
	}
	return $X;
}
function same_identity(identity, wanted) {
	const $Y = identity;
	let $Z = null;
	if ($Y[0] === 0) {
		const held = $Y[1];
		$Z = held === wanted;
	} else {
		$Z = false;
	}
	return $Z;
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
	let $az = null;
	if (!($z(tracker[2].v))) {
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
		$az = undefined;
	}
	return $az;
}
function forget_reads(tracker) {
	let $aA = null;
	if (!($z(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aA = undefined;
	}
	$aA;
	if (!($z(tracker[2].v))) {
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
		const $aJ = previous;
		let $aK = null;
		if ($aJ[0] === 0) {
			const earlier = $aJ[1];
			$aK = earlier();
		} else {
			$aK = undefined;
		}
		return $aK;
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
function $f(self, transform) {
	return [ __clone(self), transform ];
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
function $z(self) {
	return self.length === 0;
}
function $I(self) {
	const $J = self;
	return $J[0] === 1;
}
function $D(runs, body) {
	const run = renew(runs, true);
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
function $ag(self) {
	let $ai = null;
	if ($z(self)) {
		$ai = [ 1 ];
	} else {
		$ai = __list_get(self, self.length - 1);
	}
	return $ai;
}
function $y(runs, tracker, body) {
	const scope = open_run(tracker);
	const value = $D(runs, ($A, $B) => {
		return (($C) => {
			return body($A, $C, $B);
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
		tracker[0].v = answered(tracker[0].v);
		value = $y(runs, tracker, ($aw, $ax, $ay) => {
			return body($aw, $ax, $ay);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
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
function $aD(self, $aE) {
	const $aF = $aE;
	let $aG = null;
	if ($aF[0] === 0) {
		const turn = $aF[1];
		$aG = enqueue(turn, __clone(self[1].v));
	} else {
		const $aH = $ag(draining_turns.v);
		let $aI = null;
		if ($aH[0] === 0) {
			const draining = $aH[1];
			$aI = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aI = undefined;
		}
		$aG = $aI;
	}
	return $aG;
}
function $aB(self, value, $aC) {
	self[0].v = __clone(value);
	$aD(self, $aC);
}
function $aP(self, item, $aQ) {
	defer(self, () => {
		dispose(item, $aQ);
		return;
	});
	return __clone(item);
}
function $j(self, $k, $l) {
	const instance = $m(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aB(cached, pull(), $k);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $k, $l);
	return cached;
}
function $g(self, $h, $i) {
	return [ $j(self, $h, $i) ];
}
function $aY(signal, observer) {
	const cell = signal[0];
	return $q(signal, mint_subscriber(() => {
		const $aZ = [ 0, cell ];
		let $ba = null;
		if ($aZ[0] === 0) {
			const live = $aZ[1];
			$ba = observer(live.v);
		} else {
			$ba = undefined;
		}
		return $ba;
	}));
}
function $aX(self, observer, immediately) {
	const subscription = $aY(self, observer);
	if (immediately) {
		observer($o(self));
	}
	return subscription;
}
function $aW(self, observer, immediately) {
	return $aX(self[0], observer, immediately);
}
function $aV(self, observer) {
	return $aW(self, observer, true);
}
function $bb(self, transform, $bc) {
	$aB(self, transform($o(self)), $bc);
}
function $bd(self) {
	return $o(self[0]);
}
function $be(self, observer) {
	return $aX(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const owner = new2();
const count = $a(0);
const doubled = $g($f(__clone(count), (n, $c, $d, $e) => {
	return n * 2;
}), [ 1 ], [ 1 ]);
$aP(owner, $aV(__clone(doubled), (n) => {
	return console.log(n);
}), [ 1 ]);
$aB(count, 1, [ 1 ]);
$bb(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($bd(doubled));
$aP(owner, $be(__clone(count), (n) => {
	return console.log(n);
}), [ 1 ]);
$aB(count, 20, [ 1 ]);
