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
	return $z(self[0].v) && $z(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $aj = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$aj = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$aj;
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
	const $ah = turn;
	let $ai = null;
	if ($ah[0] === 0) {
		const ambient = $ah[1];
		$ai = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ao = $al(draining_turns.v);
		let $ap = null;
		if ($ao[0] === 0) {
			const draining = $ao[1];
			$ap = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ap = undefined;
		}
		$ai = $ap;
	}
	return $ai;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aW) {
	const $aX = $aW;
	let $aY = null;
	if ($aX[0] === 0) {
		const established = $aX[1];
		$aY = [ 0, established ];
	} else {
		$aY = $al(draining_turns.v);
	}
	const ambient = $aY;
	release_under(self, ambient);
}
function detach(handle) {
	const $at = $al(releasing_turns.v);
	let $au = null;
	if ($at[0] === 0) {
		const at_release = $at[1];
		$au = at_release;
	} else {
		$au = $al(draining_turns.v);
	}
	const turn = $au;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $av = [ 0, handle[0] ];
	let $aw = null;
	if ($av[0] === 0) {
		const subscribers = $av[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aw = undefined;
	} else {
		$aw = undefined;
	}
	$aw;
	const $ax = ambient;
	let $ay = null;
	if ($ax[0] === 0) {
		const turn = $ax[1];
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
		$ay = undefined;
	} else {
		$ay = undefined;
	}
	$ay;
	const $az = handle[3].v;
	let $aA = null;
	if ($az[0] === 0) {
		const release = $az[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$aA = undefined;
	} else {
		$aA = undefined;
	}
	return $aA;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aZ = null;
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
		$aZ = undefined;
	}
	return $aZ;
}
function renew(self) {
	const $E = self[0].v[3];
	let $F = null;
	if ($E[0] === 0) {
		const nursery2 = __clone($E[1]);
		let $G = null;
		if (has_spawned(nursery2)) {
			$G = [ 1 ];
		} else {
			$G = [ 0, nursery2 ];
		}
		$F = $G;
	} else {
		$F = [ 1 ];
	}
	const carried = $F;
	advance([ self[0], self[0].v[0] ], carried);
	if ($J(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $T = null;
	if (is_disposed(self)) {
		$T = [ 1 ];
	} else {
		$T = self[0].v[3];
	}
	return $T;
}
function dispose2(self) {
	advance(self, [ 1 ]);
}
function advance(self, carried) {
	let $S = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, carried ];
		const $H = held[3];
		let $I = null;
		if ($H[0] === 0) {
			const nursery2 = $H[1];
			if ($J(carried)) {
				nursery2.cancel();
			}
			$I = undefined;
		} else {
			$I = undefined;
		}
		$I;
		let $R = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $L = __guarded(cleanup);
				let $M = null;
				if ($L[0] === 0) {
					const message = $L[1];
					if ($J(failure)) {
						failure = [ 0, message ];
					}
					$M = undefined;
				} else {
					$M = undefined;
				}
				$M;
			}
			const $P = failure;
			let $Q = null;
			if ($P[0] === 0) {
				const message2 = $P[1];
				$Q = (() => {
					throw message2;
				})();
			} else {
				$Q = undefined;
			}
			$R = $Q;
		}
		$S = $R;
	}
	return $S;
}
function register_with_owner(subscription, $aQ, $aR) {
	const $aS = $aR;
	let $aT = null;
	if ($aS[0] === 0) {
		const owner2 = $aS[1];
		$aT = $aU(owner2, subscription, $aQ);
	} else {
		$aT = __clone(subscription);
	}
	return $aT;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false ]), __shared_new([  ]), __shared_new([  ]), __shared_new([  ]), __shared_new([ 1 ]) ];
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	if (!($z(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $Y = tracker[4].v;
	let $Z = null;
	if ($Y[0] === 0) {
		const target = $Y[1];
		$Z = reconnect(tracker, target);
	} else {
		if (!($z(tracker[1].v)) || !($z(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$Z = undefined;
	}
	$Z;
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
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $af = reusable(held, kept, dependency[0], position);
		let $ag = null;
		if ($af[0] === 0) {
			const index = $af[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$ag = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$ag = undefined;
		}
		$ag;
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
	tracker[0].v[3] = false;
}
function reusable(held, kept, identity, position) {
	const $ab = identity;
	let $ac = null;
	if ($ab[0] === 0) {
		const wanted = $ab[1];
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
		$ac = [ 1 ];
	} else {
		$ac = [ 1 ];
	}
	return $ac;
}
function same_identity(identity, wanted) {
	const $ad = identity;
	let $ae = null;
	if ($ad[0] === 0) {
		const held = $ad[1];
		$ae = held === wanted;
	} else {
		$ae = false;
	}
	return $ae;
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
	tracker[4].v = [ 0, __clone(target) ];
	let $aE = null;
	if (!($z(tracker[2].v))) {
		const read = tracker[2].v;
		tracker[2].v = [  ];
		tracker[0].v[3] = true;
		let edges = [  ];
		for (const dependency of read) {
			edges.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
		}
		tracker[3].v = edges;
		tracker[0].v[3] = false;
		if (tracker[0].v[4]) {
			tracker[0].v[4] = false;
			wake(target);
		}
		$aE = undefined;
	}
	return $aE;
}
function forget_reads(tracker) {
	let $aF = null;
	if (!($z(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aF = undefined;
	}
	$aF;
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
		const $aO = previous;
		let $aP = null;
		if ($aO[0] === 0) {
			const earlier = $aO[1];
			$aP = earlier();
		} else {
			$aP = undefined;
		}
		return $aP;
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
function $z(self) {
	return self.length === 0;
}
function $J(self) {
	const $K = self;
	return $K[0] === 1;
}
function $D(runs, body) {
	const run = renew(runs);
	const $U = nursery(run);
	let $V = null;
	if ($U[0] === 0) {
		const nursery2 = $U[1];
		$V = (($W) => {
			return (($X) => {
				return body($W, $X);
			})(nursery2);
		})(run);
	} else {
		$V = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $V;
}
function $al(self) {
	let $an = null;
	if ($z(self)) {
		$an = [ 1 ];
	} else {
		$an = __list_get(self, self.length - 1);
	}
	return $an;
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
		tracker[0].v[4] = false;
		value = $y(runs, tracker, ($aB, $aC, $aD) => {
			return body($aB, $aC, $aD);
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
function $aI(self, $aJ) {
	const $aK = $aJ;
	let $aL = null;
	if ($aK[0] === 0) {
		const turn = $aK[1];
		$aL = enqueue(turn, __clone(self[1].v));
	} else {
		const $aM = $al(draining_turns.v);
		let $aN = null;
		if ($aM[0] === 0) {
			const draining = $aM[1];
			$aN = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aN = undefined;
		}
		$aL = $aN;
	}
	return $aL;
}
function $aG(self, value, $aH) {
	self[0].v = __clone(value);
	$aI(self, $aH);
}
function $aU(self, item, $aV) {
	defer(self, () => {
		dispose(item, $aV);
		return;
	});
	return __clone(item);
}
function $j(self, $k, $l) {
	const instance = $m(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aG(cached, pull(), $k);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $k, $l);
	return cached;
}
function $g(self, $h, $i) {
	return [ $j(self, $h, $i) ];
}
function $bd(signal, observer) {
	const cell = signal[0];
	return $q(signal, mint_subscriber(() => {
		const $be = [ 0, cell ];
		let $bf = null;
		if ($be[0] === 0) {
			const live = $be[1];
			$bf = observer(live.v);
		} else {
			$bf = undefined;
		}
		return $bf;
	}));
}
function $bc(self, observer, immediately) {
	const subscription = $bd(self, observer);
	if (immediately) {
		observer($o(self));
	}
	return subscription;
}
function $bb(self, observer, immediately) {
	return $bc(self[0], observer, immediately);
}
function $ba(self, observer) {
	return $bb(self, observer, true);
}
function $bg(self, transform, $bh) {
	$aG(self, transform($o(self)), $bh);
}
function $bi(self) {
	return $o(self[0]);
}
function $bj(self, observer) {
	return $bc(self, observer, true);
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
$aU(owner, $ba(__clone(doubled), (n) => {
	return console.log(n);
}), [ 1 ]);
$aG(count, 1, [ 1 ]);
$bg(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($bi(doubled));
$aU(owner, $bj(__clone(count), (n) => {
	return console.log(n);
}), [ 1 ]);
$aG(count, 20, [ 1 ]);
