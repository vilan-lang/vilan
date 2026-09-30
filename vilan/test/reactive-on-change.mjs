function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
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
		let $q = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$q = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$q;
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
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function dispose(self, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const established = $y[1];
		$z = [ 0, established ];
	} else {
		$z = $s(draining_turns.v);
	}
	const ambient = $z;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $A = [ 0, handle[0] ];
	let $B = null;
	if ($A[0] === 0) {
		const subscribers = $A[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$B = undefined;
	} else {
		$B = undefined;
	}
	$B;
	const $C = ambient;
	let $D = null;
	if ($C[0] === 0) {
		const turn = $C[1];
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
		$D = undefined;
	} else {
		$D = undefined;
	}
	$D;
	const $E = handle[3].v;
	let $F = null;
	if ($E[0] === 0) {
		const release = $E[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$F = undefined;
	} else {
		$F = undefined;
	}
	return $F;
}
function new2() {
	return [ __shared_new([ 0, [ 1 ], [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $an = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const $al = self[0].v[1];
		let $am = null;
		if ($al[0] === 0) {
			const list = $al[1];
			$am = list.v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = [ 0, __shared_new([ cleanup ]) ];
			$am = undefined;
		}
		$an = $am;
	}
	return $an;
}
function renew(self, with_nursery) {
	const live = [ self[0], self[0].v[0] ];
	dispose2(live);
	const next = self[0].v[0];
	if (with_nursery) {
		self[0].v[2] = [ 0, detached_nursery() ];
	}
	return [ self[0], next ];
}
function nursery(self) {
	let $aa = null;
	if (is_disposed(self)) {
		$aa = [ 1 ];
	} else {
		$aa = self[0].v[2];
	}
	return $aa;
}
function dispose2(self) {
	let $Z = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, [ 1 ], [ 1 ] ];
		const $P = held[2];
		let $Q = null;
		if ($P[0] === 0) {
			const nursery2 = $P[1];
			$Q = nursery2.cancel();
		} else {
			$Q = undefined;
		}
		$Q;
		const $R = held[1];
		let $S = null;
		if ($R[0] === 0) {
			const cleanups = $R[1];
			let failure = [ 1 ];
			for (const cleanup of cleanups.v) {
				const $T = __guarded(cleanup);
				let $U = null;
				if ($T[0] === 0) {
					const message = $T[1];
					if ($V(failure)) {
						failure = [ 0, message ];
					}
					$U = undefined;
				} else {
					$U = undefined;
				}
				$U;
			}
			const $X = failure;
			let $Y = null;
			if ($X[0] === 0) {
				const message2 = $X[1];
				$Y = (() => {
					throw message2;
				})();
			} else {
				$Y = undefined;
			}
			$S = $Y;
		} else {
			$S = undefined;
		}
		$Z = $S;
	}
	return $Z;
}
function get_owner($ai) {
	return $ai;
}
function register_with_owner(subscription, $aT, $aU) {
	const $aV = $aU;
	let $aW = null;
	if ($aV[0] === 0) {
		const owner = $aV[1];
		$aW = $aj(owner, subscription, $aT);
	} else {
		$aW = __clone(subscription);
	}
	return $aW;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $ag = previous;
		let $ah = null;
		if ($ag[0] === 0) {
			const earlier = $ag[1];
			$ah = earlier();
		} else {
			$ah = undefined;
		}
		return $ah;
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
function $h(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $e(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $f = [ 0, cell ];
		let $g = null;
		if ($f[0] === 0) {
			const live = $f[1];
			$g = observer(live.v);
		} else {
			$g = undefined;
		}
		return $g;
	}));
}
function $i(self) {
	return __clone(self[0].v);
}
function $d(self, observer, immediately) {
	const subscription = $e(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $c(self, observer) {
	return $d(self, observer, true);
}
function $j(self, observer) {
	return $d(self, observer, false);
}
function $r(self) {
	return self.length === 0;
}
function $s(self) {
	let $u = null;
	if ($r(self)) {
		$u = [ 1 ];
	} else {
		$u = __list_get(self, self.length - 1);
	}
	return $u;
}
function $m(self, $n) {
	const $o = $n;
	let $p = null;
	if ($o[0] === 0) {
		const turn = $o[1];
		$p = enqueue(turn, self[1].v);
	} else {
		const $v = $s(draining_turns.v);
		let $w = null;
		if ($v[0] === 0) {
			const draining = $v[1];
			$w = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$w = undefined;
		}
		$p = $w;
	}
	return $p;
}
function $k(self, value, $l) {
	self[0].v = __clone(value);
	$m(self, $l);
}
function $V(self) {
	const $W = self;
	return $W[0] === 1;
}
function $O(runs, body) {
	const run = renew(runs, true);
	const $ab = nursery(run);
	let $ac = null;
	if ($ab[0] === 0) {
		const nursery2 = $ab[1];
		$ac = (($ad) => {
			return (($ae) => {
				return body($ad, $ae);
			})(nursery2);
		})(run);
	} else {
		$ac = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $ac;
}
function $aj(self, item, $ak) {
	defer(self, () => {
		dispose(item, $ak);
		return;
	});
	return __clone(item);
}
function $J(self, body, $K, $L) {
	const runs = new2();
	const subscription = $j(self, (value) => {
		return $O(runs, ($M, $N) => {
			return body(value, $M, $N);
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	$aj(get_owner($L), subscription, $K);
}
function $ao(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $as(self) {
	return $i(self[0]);
}
function $au(self, subscriber) {
	return $h(self, subscriber);
}
function $at(self, subscriber) {
	return $au(self[0], subscriber);
}
function $ar(self, observer, immediately) {
	const subscription = $at(self, mint_subscriber(() => {
		return observer($as(self));
	}));
	if (immediately) {
		observer($as(self));
	}
	return subscription;
}
function $aq(self, observer) {
	return $ar(self, observer, true);
}
function $av(self, observer) {
	return $ar(self, observer, false);
}
function $ay(self, transform) {
	return [ __clone(self), transform ];
}
function $aG(self) {
	return [ () => {
		return $as(self);
	}, (subscriber) => {
		return $at(self, subscriber);
	}, () => {
		return;
	} ];
}
function $aJ(runs, body) {
	const run = renew(runs, true);
	const $aK = nursery(run);
	let $aL = null;
	if ($aK[0] === 0) {
		const nursery2 = $aK[1];
		$aL = (($ad) => {
			return (($ae) => {
				return body($ad, $ae);
			})(nursery2);
		})(run);
	} else {
		$aL = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $aL;
}
function $aF(self) {
	const transform = self[1];
	const upstream = $aG(__clone(self[0]));
	const pull = upstream[0];
	const runs = new2();
	return [ () => {
		const value = pull();
		return $aJ(runs, ($aH, $aI) => {
			return transform(value, $aH, $aI);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $aO(self, $n) {
	const $aP = $n;
	let $aQ = null;
	if ($aP[0] === 0) {
		const turn = $aP[1];
		$aQ = enqueue(turn, self[1].v);
	} else {
		const $aR = $s(draining_turns.v);
		let $aS = null;
		if ($aR[0] === 0) {
			const draining = $aR[1];
			$aS = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aS = undefined;
		}
		$aQ = $aS;
	}
	return $aQ;
}
function $aN(self, value, $l) {
	self[0].v = __clone(value);
	$aO(self, $l);
}
function $aC(self, $aD, $aE) {
	const instance = $aF(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aN(cached, pull(), $aD);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aD, $aE);
	return cached;
}
function $az(self, $aA, $aB) {
	return [ $aC(self, $aA, $aB) ];
}
function $aX(self) {
	return $i(self[0]);
}
function $bc(signal, observer) {
	const cell = signal[0];
	return $h(signal, mint_subscriber(() => {
		const $bd = [ 0, cell ];
		let $be = null;
		if ($bd[0] === 0) {
			const live = $bd[1];
			$be = observer(live.v);
		} else {
			$be = undefined;
		}
		return $be;
	}));
}
function $bb(self, observer, immediately) {
	const subscription = $bc(self, observer);
	if (immediately) {
		observer($i(self));
	}
	return subscription;
}
function $ba(self, observer, immediately) {
	return $bb(self[0], observer, immediately);
}
function $aZ(self, observer) {
	return $ba(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const count = $a(1);
const eager = $c(__clone(count), (value) => {
	return console.log("sub " + value);
});
const quiet = $j(__clone(count), (value) => {
	return console.log("on_change " + value);
});
console.log("attached");
$k(count, 2, [ 1 ]);
dispose(eager, [ 1 ]);
dispose(quiet, [ 1 ]);
$k(count, 3, [ 1 ]);
const $ap = $ao(($G) => {
	$J(__clone(count), (value, $H, $I) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $G);
	return;
});
const _built = $ap[0];
const scope = $ap[1];
$k(count, 4, [ 1 ]);
dispose2(scope);
$k(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $aq(__clone(stored), (value) => {
	return console.log("eager " + value);
});
$k(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $av(__clone(stored), (value) => {
	return console.log("stored " + value);
});
$k(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const labelled = $az($ay(__clone(stored), (value, $aw, $ax) => {
	return "n=" + value;
}), [ 1 ], [ 1 ]);
console.log($aX(labelled));
const shown = $aZ(labelled, (value) => {
	return console.log("label " + value);
});
$k(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
