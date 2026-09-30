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
	return $r(self[0].v) && $r(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $W = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$W = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$W;
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
	const $U = turn;
	let $V = null;
	if ($U[0] === 0) {
		const ambient = $U[1];
		$V = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $ab = $Y(draining_turns.v);
		let $ac = null;
		if ($ab[0] === 0) {
			const draining = $ab[1];
			$ac = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$ac = undefined;
		}
		$V = $ac;
	}
	return $V;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function dispose(self, $aQ) {
	const $aR = $aQ;
	let $aS = null;
	if ($aR[0] === 0) {
		const established = $aR[1];
		$aS = [ 0, established ];
	} else {
		$aS = $Y(draining_turns.v);
	}
	const ambient = $aS;
	release_under(self, ambient);
}
function detach(handle) {
	const $ag = $Y(releasing_turns.v);
	let $ah = null;
	if ($ag[0] === 0) {
		const at_release = $ag[1];
		$ah = at_release;
	} else {
		$ah = $Y(draining_turns.v);
	}
	const turn = $ah;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ai = [ 0, handle[0] ];
	let $aj = null;
	if ($ai[0] === 0) {
		const subscribers = $ai[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$aj = undefined;
	} else {
		$aj = undefined;
	}
	$aj;
	const $ak = ambient;
	let $al = null;
	if ($ak[0] === 0) {
		const turn = $ak[1];
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
		$al = undefined;
	} else {
		$al = undefined;
	}
	$al;
	const $am = handle[3].v;
	let $an = null;
	if ($am[0] === 0) {
		const release = $am[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$an = undefined;
	} else {
		$an = undefined;
	}
	return $an;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aT = null;
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
		$aT = undefined;
	}
	return $aT;
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
	let $G = null;
	if (is_disposed(self)) {
		$G = [ 1 ];
	} else {
		$G = self[0].v[3];
	}
	return $G;
}
function dispose2(self) {
	let $F = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $w = held[3];
		let $x = null;
		if ($w[0] === 0) {
			const nursery2 = $w[1];
			$x = nursery2.cancel();
		} else {
			$x = undefined;
		}
		$x;
		let $E = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $y = __guarded(cleanup);
				let $z = null;
				if ($y[0] === 0) {
					const message = $y[1];
					if ($A(failure)) {
						failure = [ 0, message ];
					}
					$z = undefined;
				} else {
					$z = undefined;
				}
				$z;
			}
			const $C = failure;
			let $D = null;
			if ($C[0] === 0) {
				const message2 = $C[1];
				$D = (() => {
					throw message2;
				})();
			} else {
				$D = undefined;
			}
			$E = $D;
		}
		$F = $E;
	}
	return $F;
}
function get_owner($aN) {
	return $aN;
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
	const $L = tracker[4].v;
	let $M = null;
	if ($L[0] === 0) {
		const target = $L[1];
		$M = reconnect(tracker, target);
	} else {
		if (!($r(tracker[1].v)) || !($r(tracker[2].v))) {
			tracker[2].v = __clone(tracker[1].v);
		}
		$M = undefined;
	}
	$M;
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
		const $S = reusable(held, kept, dependency[0], position);
		let $T = null;
		if ($S[0] === 0) {
			const index = $S[1];
			__at_put(kept, index, true);
			next.push(__clone(__at(held, index)));
			$T = undefined;
		} else {
			next.push([ dependency[0], dependency[1](relay_for(tracker, target)) ]);
			$T = undefined;
		}
		$T;
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
	const $O = identity;
	let $P = null;
	if ($O[0] === 0) {
		const wanted = $O[1];
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
		$P = [ 1 ];
	} else {
		$P = [ 1 ];
	}
	return $P;
}
function same_identity(identity, wanted) {
	const $Q = identity;
	let $R = null;
	if ($Q[0] === 0) {
		const held = $Q[1];
		$R = held === wanted;
	} else {
		$R = false;
	}
	return $R;
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
	let $aE = null;
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
		$aE = undefined;
	}
	return $aE;
}
function forget_reads(tracker) {
	let $aK = null;
	if (!($r(tracker[3].v))) {
		const edges = tracker[3].v;
		tracker[3].v = [  ];
		for (const edge of edges) {
			detach(edge[1]);
		}
		$aK = undefined;
	}
	$aK;
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
		const $aL = previous;
		let $aM = null;
		if ($aL[0] === 0) {
			const earlier = $aL[1];
			$aM = earlier();
		} else {
			$aM = undefined;
		}
		return $aM;
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
function $r(self) {
	return self.length === 0;
}
function $A(self) {
	const $B = self;
	return $B[0] === 1;
}
function $v(runs, body) {
	const run = renew(runs, true);
	const $H = nursery(run);
	let $I = null;
	if ($H[0] === 0) {
		const nursery2 = $H[1];
		$I = (($J) => {
			return (($K) => {
				return body($J, $K);
			})(nursery2);
		})(run);
	} else {
		$I = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $I;
}
function $Y(self) {
	let $aa = null;
	if ($r(self)) {
		$aa = [ 1 ];
	} else {
		$aa = __list_get(self, self.length - 1);
	}
	return $aa;
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
		value = $q(runs, tracker, ($ao, $ap, $aq) => {
			return body($ao, $ap, $aq);
		});
		rounds = rounds + 1;
	}
	if (tracker[0].v[4]) {
		tracker[0].v = answered(tracker[0].v);
	}
	return value;
}
function $aw(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $at(signal, observer) {
	const cell = signal[0];
	return $aw(signal, mint_subscriber(() => {
		const $au = [ 0, cell ];
		let $av = null;
		if ($au[0] === 0) {
			const live = $au[1];
			$av = observer(live.v);
		} else {
			$av = undefined;
		}
		return $av;
	}));
}
function $ax(self) {
	return __clone(self[0].v);
}
function $as(self, observer, immediately) {
	const subscription = $at(self, observer);
	if (immediately) {
		observer($ax(self));
	}
	return subscription;
}
function $ar(self, observer) {
	return $as(self, observer, true);
}
function $aO(self, item, $aP) {
	defer(self, () => {
		dispose(item, $aP);
		return;
	});
	return __clone(item);
}
function $g(self, body, $h, $i) {
	const runs = new2();
	const tracker = new_tracker();
	const latest = __shared_new([ 1 ]);
	const subscription = $ar(self, (value2) => {
		latest.v = [ 0, __clone(value2) ];
		if (has_run(tracker)) {
			$m(runs, tracker, ($j, $k, $l) => {
				return body(value2, $j, $k, $l);
			});
		}
		return;
	});
	const rerun = subscriber_of(() => {
		let $aD = null;
		if (tracker[0].v[2]) {
			const $ay = latest.v;
			let $az = null;
			if ($ay[0] === 0) {
				const value2 = $ay[1];
				$az = $m(runs, tracker, ($aA, $aB, $aC) => {
					return body(value2, $aA, $aB, $aC);
				});
			} else {
				$az = undefined;
			}
			$aD = $az;
		}
		return $aD;
	}, false);
	attach_tracker(tracker, rerun);
	const $aF = latest.v;
	let $aG = null;
	if ($aF[0] === 0) {
		const value = $aF[1];
		$aG = $m(runs, tracker, ($aH, $aI, $aJ) => {
			return body(value, $aH, $aI, $aJ);
		});
	} else {
		$aG = undefined;
	}
	$aG;
	also_releasing(subscription, () => {
		detach_tracker(tracker);
		release_runs(runs);
		return;
	});
	$aO(get_owner($i), subscription, $h);
}
function $aW(self, $aX) {
	const $aY = $aX;
	let $aZ = null;
	if ($aY[0] === 0) {
		const turn = $aY[1];
		$aZ = enqueue(turn, __clone(self[1].v));
	} else {
		const $ba = $Y(draining_turns.v);
		let $bb = null;
		if ($ba[0] === 0) {
			const draining = $ba[1];
			$bb = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$bb = undefined;
		}
		$aZ = $bb;
	}
	return $aZ;
}
function $aU(self, value, $aV) {
	self[0].v = __clone(value);
	$aW(self, $aV);
}
function $bo(owner2, body) {
	return body(owner2);
}
function $bt(body) {
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
const count = $a(1);
const owner = new2();
(($c) => {
	$g(__clone(count), (value, $d, $e, $f) => {
		return console.log("seen " + value);
	}, [ 1 ], $c);
	return;
})(owner);
$aU(count, 2, [ 1 ]);
dispose2(owner);
$aU(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($bc) => {
	(($bd) => {
		$g(__clone(count), (value, $be, $bf, $bg) => {
			return console.log("inner " + value);
		}, [ 1 ], $bd);
		return;
	})(inner);
	$g(__clone(count), (value, $bh, $bi, $bj) => {
		return console.log("outer " + value);
	}, [ 1 ], $bc);
	return;
})(outer);
$aU(count, 4, [ 1 ]);
dispose2(inner);
$aU(count, 5, [ 1 ]);
dispose2(outer);
$aU(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$bo(wrapped, ($bk) => {
	$g(__clone(count), (value, $bl, $bm, $bn) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $bk);
	return;
});
$aU(count, 7, [ 1 ]);
dispose2(wrapped);
$aU(count, 8, [ 1 ]);
console.log("fin");
const $bu = $bt(($bp) => {
	$g(__clone(count), (value, $bq, $br, $bs) => {
		return console.log("comp " + value);
	}, [ 1 ], $bp);
	return "built";
});
const label = $bu[0];
const scope = $bu[1];
console.log(label);
$aU(count, 9, [ 1 ]);
dispose2(scope);
$aU(count, 10, [ 1 ]);
console.log("post");
