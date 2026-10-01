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
		let $ab = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$ab = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$ab;
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
	const $Z = turn;
	let $aa = null;
	if ($Z[0] === 0) {
		const ambient = $Z[1];
		$aa = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ag = $ad(draining_turns.v);
		let $ah = null;
		if ($ag[0] === 0) {
			const draining = $ag[1];
			$ah = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ah = undefined;
		}
		$aa = $ah;
	}
	return $aa;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aV) {
	const $aW = $aV;
	let $aX = null;
	if ($aW[0] === 0) {
		const established = $aW[1];
		$aX = [ 0, established ];
	} else {
		$aX = $ad(draining_turns.v);
	}
	const ambient = $aX;
	release_under(self, ambient);
}
function detach(handle) {
	const $al = $ad(releasing_turns.v);
	let $am = null;
	if ($al[0] === 0) {
		const at_release = $al[1];
		$am = at_release;
	} else {
		$am = $ad(draining_turns.v);
	}
	const turn = $am;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $an = [ 0, handle[0] ];
	let $ao = null;
	if ($an[0] === 0) {
		const subscribers = $an[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ao = undefined;
	} else {
		$ao = undefined;
	}
	$ao;
	const $ap = ambient;
	let $aq = null;
	if ($ap[0] === 0) {
		const turn = $ap[1];
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
		$aq = undefined;
	} else {
		$aq = undefined;
	}
	$aq;
	const $ar = handle[3].v;
	let $as = null;
	if ($ar[0] === 0) {
		const release = $ar[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$as = undefined;
	} else {
		$as = undefined;
	}
	return $as;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aY = null;
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
		$aY = undefined;
	}
	return $aY;
}
function renew(self) {
	const $w = self[0].v[3];
	let $x = null;
	if ($w[0] === 0) {
		const nursery2 = __clone($w[1]);
		let $y = null;
		if (has_spawned(nursery2)) {
			$y = [ 1 ];
		} else {
			$y = [ 0, nursery2 ];
		}
		$x = $y;
	} else {
		$x = [ 1 ];
	}
	const carried = $x;
	advance([ self[0], self[0].v[0] ], carried);
	if ($B(self[0].v[3])) {
		run_nurseries_allocated_count.v = run_nurseries_allocated_count.v + 1;
		self[0].v[3] = [ 0, detached_nursery() ];
	}
	return [ self[0], self[0].v[0] ];
}
function nursery(self) {
	let $L = null;
	if (is_disposed(self)) {
		$L = [ 1 ];
	} else {
		$L = self[0].v[3];
	}
	return $L;
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
function get_owner($aS) {
	return $aS;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function new_tracker() {
	return [ __shared_new([ 0, false, false, false, false ]), __shared_new([  ]), __shared_new([  ]), __shared_new([  ]), __shared_new([ 1 ]) ];
}
function has_run(tracker) {
	return tracker[0].v[0] > 0;
}
function open_run(tracker) {
	const epoch = tracker[0].v[0] + 1;
	tracker[0].v[0] = epoch;
	tracker[0].v[1] = true;
	tracker[0].v[2] = false;
	if (!($r(tracker[1].v))) {
		tracker[1].v = [  ];
	}
	return [ __clone(tracker), epoch ];
}
function close_run(tracker) {
	tracker[0].v[1] = false;
	const $Q = tracker[4].v;
	let $R = null;
	if ($Q[0] === 0) {
		const target = $Q[1];
		$R = reconnect(tracker, target);
	} else {
		if (!($r(tracker[1].v)) || !($r(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$R = undefined;
	}
	$R;
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
	tracker[0].v[3] = true;
	let position = 0;
	for (const dependency of reading) {
		const $X = reusable(held, kept, dependency[0], position);
		let $Y = null;
		if ($X[0] === 0) {
			const index = $X[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$Y = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$Y = undefined;
		}
		$Y;
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
	const $T = identity;
	let $U = null;
	if ($T[0] === 0) {
		const wanted = $T[1];
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
		$U = [ 1 ];
	} else {
		$U = [ 1 ];
	}
	return $U;
}
function same_identity(identity, wanted) {
	const $V = identity;
	let $W = null;
	if ($V[0] === 0) {
		const held = $V[1];
		$W = held === wanted;
	} else {
		$W = false;
	}
	return $W;
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
	let $aJ = null;
	if (!($r(tracker[2].v))) {
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
		$aJ = undefined;
	}
	return $aJ;
}
function forget_reads(tracker) {
	let $aP = null;
	if (!($r(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aP = undefined;
	}
	$aP;
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
		const $aQ = previous;
		let $aR = null;
		if ($aQ[0] === 0) {
			const earlier = $aQ[1];
			$aR = earlier();
		} else {
			$aR = undefined;
		}
		return $aR;
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
	const $M = nursery(run);
	let $N = null;
	if ($M[0] === 0) {
		const nursery2 = $M[1];
		$N = (($O) => {
			return (($P) => {
				return body($O, $P);
			})(nursery2);
		})(run);
	} else {
		$N = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $N;
}
function $ad(self) {
	let $af = null;
	if ($r(self)) {
		$af = [ 1 ];
	} else {
		$af = __list_get(self, self.length - 1);
	}
	return $af;
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
		tracker[0].v[4] = false;
		value = $q(runs, tracker, ($at, $au, $av) => {
			return body($at, $au, $av);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v[4] = false;
	}
	return value;
}
function $aB(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $ay(signal, observer) {
	const cell = signal[0];
	return $aB(signal, mint_subscriber(() => {
		const $az = [ 0, cell ];
		let $aA = null;
		if ($az[0] === 0) {
			const live = $az[1];
			$aA = observer(live.v);
		} else {
			$aA = undefined;
		}
		return $aA;
	}));
}
function $aC(self) {
	return __clone(self[0].v);
}
function $ax(self, observer, immediately) {
	const subscription = $ay(self, observer);
	if (immediately) {
		observer($aC(self));
	}
	return subscription;
}
function $aw(self, observer) {
	return $ax(self, observer, true);
}
function $aT(self, item, $aU) {
	defer(self, () => {
		dispose(item, $aU);
		return;
	});
	return __clone(item);
}
function $g(self, body, $h, $i) {
	const runs = new2();
	const tracker = new_tracker();
	const latest = __shared_new([ 1 ]);
	const subscription = $aw(self, (value2) => {
		latest.v = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$m(runs, tracker, ($j, $k, $l) => {
				return body(value2, $j, $k, $l);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aI = null;
		if (tracker[0].v[2]) {
			const $aD = latest.v;
			let $aE = null;
			if ($aD[0] === 0) {
				const value2 = $aD[1];
				$aE = $m(runs, tracker, ($aF, $aG, $aH) => {
					return body(value2, $aF, $aG, $aH);
				});
			} else {
				$aE = undefined;
			}
			$aI = $aE;
		}
		return $aI;
	}, false);
	attach_tracker(tracker, rerun);
	const $aK = latest.v;
	let $aL = null;
	if ($aK[0] === 0) {
		const value = $aK[1];
		$aL = $m(runs, tracker, ($aM, $aN, $aO) => {
			return body(value, $aM, $aN, $aO);
		});
	} else {
		$aL = undefined;
	}
	$aL;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$aT(get_owner($i), subscription, $h);
}
function $bb(self, $bc) {
	const $bd = $bc;
	let $be = null;
	if ($bd[0] === 0) {
		const turn = $bd[1];
		$be = enqueue(turn, __clone(self[1].v));
	} else {
		const $bf = $ad(draining_turns.v);
		let $bg = null;
		if ($bf[0] === 0) {
			const draining = $bf[1];
			$bg = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bg = undefined;
		}
		$be = $bg;
	}
	return $be;
}
function $aZ(self, value, $ba) {
	self[0].v = __clone(value);
	$bb(self, $ba);
}
function $bt(owner2, body) {
	return body(owner2);
}
function $by(body) {
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
$aZ(count, 2, [ 1 ]);
dispose2(owner);
$aZ(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($bh) => {
	(($bi) => {
		$g(__clone(count), (value, $bj, $bk, $bl) => {
			return console.log("inner " + value);
		}, [ 1 ], $bi);
		return;
	})(inner);
	$g(__clone(count), (value, $bm, $bn, $bo) => {
		return console.log("outer " + value);
	}, [ 1 ], $bh);
	return;
})(outer);
$aZ(count, 4, [ 1 ]);
dispose2(inner);
$aZ(count, 5, [ 1 ]);
dispose2(outer);
$aZ(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$bt(wrapped, ($bp) => {
	$g(__clone(count), (value, $bq, $br, $bs) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $bp);
	return;
});
$aZ(count, 7, [ 1 ]);
dispose2(wrapped);
$aZ(count, 8, [ 1 ]);
console.log("fin");
const $bz = $by(($bu) => {
	$g(__clone(count), (value, $bv, $bw, $bx) => {
		return console.log("comp " + value);
	}, [ 1 ], $bu);
	return "built";
});
const label = $bz[0];
const scope = $bz[1];
console.log(label);
$aZ(count, 9, [ 1 ]);
dispose2(scope);
$aZ(count, 10, [ 1 ]);
console.log("post");
