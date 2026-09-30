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
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ak = null;
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
		$ak = undefined;
	}
	return $ak;
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
	let $Z = null;
	if (is_disposed(self)) {
		$Z = [ 1 ];
	} else {
		$Z = self[0].v[3];
	}
	return $Z;
}
function dispose2(self) {
	let $Y = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $P = held[3];
		let $Q = null;
		if ($P[0] === 0) {
			const nursery2 = $P[1];
			$Q = nursery2.cancel();
		} else {
			$Q = undefined;
		}
		$Q;
		let $X = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $R = __guarded(cleanup);
				let $S = null;
				if ($R[0] === 0) {
					const message = $R[1];
					if ($T(failure)) {
						failure = [ 0, message ];
					}
					$S = undefined;
				} else {
					$S = undefined;
				}
				$S;
			}
			const $V = failure;
			let $W = null;
			if ($V[0] === 0) {
				const message2 = $V[1];
				$W = (() => {
					throw message2;
				})();
			} else {
				$W = undefined;
			}
			$X = $W;
		}
		$Y = $X;
	}
	return $Y;
}
function get_owner($ah) {
	return $ah;
}
function register_with_owner(subscription, $aR, $aS) {
	const $aT = $aS;
	let $aU = null;
	if ($aT[0] === 0) {
		const owner = $aT[1];
		$aU = $ai(owner, subscription, $aR);
	} else {
		$aU = __clone(subscription);
	}
	return $aU;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $af = previous;
		let $ag = null;
		if ($af[0] === 0) {
			const earlier = $af[1];
			$ag = earlier();
		} else {
			$ag = undefined;
		}
		return $ag;
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
function $T(self) {
	const $U = self;
	return $U[0] === 1;
}
function $O(runs, body) {
	const run = renew(runs, true);
	const $aa = nursery(run);
	let $ab = null;
	if ($aa[0] === 0) {
		const nursery2 = $aa[1];
		$ab = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$ab = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $ab;
}
function $ai(self, item, $aj) {
	defer(self, () => {
		dispose(item, $aj);
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
	$ai(get_owner($L), subscription, $K);
}
function $al(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $ap(self) {
	return $i(self[0]);
}
function $ar(self, subscriber) {
	return $h(self, subscriber);
}
function $aq(self, subscriber) {
	return $ar(self[0], subscriber);
}
function $ao(self, observer, immediately) {
	const subscription = $aq(self, mint_subscriber(() => {
		return observer($ap(self));
	}));
	if (immediately) {
		observer($ap(self));
	}
	return subscription;
}
function $an(self, observer) {
	return $ao(self, observer, true);
}
function $as(self, observer) {
	return $ao(self, observer, false);
}
function $aw(self, transform) {
	return [ __clone(self), transform ];
}
function $aE(self) {
	return [ () => {
		return $ap(self);
	}, (subscriber) => {
		return $aq(self, subscriber);
	}, () => {
		return;
	} ];
}
function $aH(runs, body) {
	const run = renew(runs, true);
	const $aI = nursery(run);
	let $aJ = null;
	if ($aI[0] === 0) {
		const nursery2 = $aI[1];
		$aJ = (($ac) => {
			return (($ad) => {
				return body($ac, $ad);
			})(nursery2);
		})(run);
	} else {
		$aJ = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $aJ;
}
function $aD(self) {
	const transform = self[1];
	const upstream = $aE(__clone(self[0]));
	const pull = upstream[0];
	const runs = new2();
	return [ () => {
		const value = pull();
		return $aH(runs, ($aF, $aG) => {
			return transform(value, $aF, $aG);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $aM(self, $n) {
	const $aN = $n;
	let $aO = null;
	if ($aN[0] === 0) {
		const turn = $aN[1];
		$aO = enqueue(turn, self[1].v);
	} else {
		const $aP = $s(draining_turns.v);
		let $aQ = null;
		if ($aP[0] === 0) {
			const draining = $aP[1];
			$aQ = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$aQ = undefined;
		}
		$aO = $aQ;
	}
	return $aO;
}
function $aL(self, value, $l) {
	self[0].v = __clone(value);
	$aM(self, $l);
}
function $aA(self, $aB, $aC) {
	const instance = $aD(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$aL(cached, pull(), $aB);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $aB, $aC);
	return cached;
}
function $ax(self, $ay, $az) {
	return [ $aA(self, $ay, $az) ];
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
const no_cleanups = __shared_new([  ]);
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
const $am = $al(($G) => {
	$J(__clone(count), (value, $H, $I) => {
		return console.log("effect_on_change " + value);
	}, [ 1 ], $G);
	return;
});
const _built = $am[0];
const scope = $am[1];
$k(count, 4, [ 1 ]);
dispose2(scope);
$k(count, 5, [ 1 ]);
const stored = [ $a(10) ];
const eagerly = $an(__clone(stored), (value) => {
	return console.log("eager " + value);
});
$k(stored[0], 11, [ 1 ]);
dispose(eagerly, [ 1 ]);
const watched = $as(__clone(stored), (value) => {
	return console.log("stored " + value);
});
$k(stored[0], 12, [ 1 ]);
dispose(watched, [ 1 ]);
const $aW = $al(($at) => {
	return $ax($aw(__clone(stored), (value, $au, $av) => {
		return "n=" + value;
	}), [ 1 ], [ 0, $at ]);
});
const labelled = $aW[0];
const sealed = $aW[1];
console.log($aX(labelled));
const shown = $aZ(labelled, (value) => {
	return console.log("label " + value);
});
$k(stored[0], 13, [ 1 ]);
dispose(shown, [ 1 ]);
dispose2(sealed);
