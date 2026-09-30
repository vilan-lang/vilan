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
function subscriber_of(notify, derived) {
	return [ fresh_id(), notify, __shared_new(true), derived ];
}
function is_quiescent(self) {
	return $S(self[0].v) && $S(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $R = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$R = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$R;
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
				while (!($S(turn[1].v)) && budget > 0) {
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
	const $P = turn;
	let $Q = null;
	if ($P[0] === 0) {
		const ambient = $P[1];
		$Q = enqueue(ambient, [ reissued(subscriber) ]);
	} else {
		const $W = $T(draining_turns.v);
		let $X = null;
		if ($W[0] === 0) {
			const draining = $W[1];
			$X = enqueue(draining, [ reissued(subscriber) ]);
		} else {
			if (subscriber[2].v) {
				subscriber[1]();
			}
			$X = undefined;
		}
		$Q = $X;
	}
	return $Q;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
function wake(subscriber) {
	defer_subscriber([ 1 ], subscriber);
}
function retiring(subscriber, release) {
	return detached(subscriber[0], subscriber[2], release);
}
function detached(id, live, release) {
	let subscribers = [  ];
	const empty = __shared_new(subscribers);
	return [ empty, id, live, __shared_new([ 0, release ]) ];
}
function dispose(self, $aD) {
	const $aE = $aD;
	let $aF = null;
	if ($aE[0] === 0) {
		const established = $aE[1];
		$aF = [ 0, established ];
	} else {
		$aF = $T(draining_turns.v);
	}
	const ambient = $aF;
	release_under(self, ambient);
}
function detach(handle) {
	const $ad = $T(releasing_turns.v);
	let $ae = null;
	if ($ad[0] === 0) {
		const at_release = $ad[1];
		$ae = at_release;
	} else {
		$ae = $T(draining_turns.v);
	}
	const turn = $ae;
	release_under(handle, turn);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $af = [ 0, handle[0] ];
	let $ag = null;
	if ($af[0] === 0) {
		const subscribers = $af[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ag = undefined;
	} else {
		$ag = undefined;
	}
	$ag;
	const $ah = ambient;
	let $ai = null;
	if ($ah[0] === 0) {
		const turn = $ah[1];
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
		$ai = undefined;
	} else {
		$ai = undefined;
	}
	$ai;
	const $aj = handle[3].v;
	let $ak = null;
	if ($aj[0] === 0) {
		const release = $aj[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$ak = undefined;
	} else {
		$ak = undefined;
	}
	return $ak;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $aG = null;
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
		$aG = undefined;
	}
	return $aG;
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
function register_with_owner(subscription, $ax, $ay) {
	const $az = $ay;
	let $aA = null;
	if ($az[0] === 0) {
		const owner = $az[1];
		$aA = $aB(owner, subscription, $ax);
	} else {
		$aA = __clone(subscription);
	}
	return $aA;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $av = previous;
		let $aw = null;
		if ($av[0] === 0) {
			const earlier = $av[1];
			$aw = earlier();
		} else {
			$aw = undefined;
		}
		return $aw;
	} ];
}
function relay_to(subscriber) {
	return subscriber_of(() => {
		return wake(subscriber);
	}, true);
}
function detach_held(handle) {
	const $Y = handle.v;
	let $Z = null;
	if ($Y[0] === 0) {
		const held = $Y[1];
		$Z = detach(held);
	} else {
		$Z = undefined;
	}
	$Z;
	handle.v = [ 1 ];
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
function $c(value) {
	return $b(value);
}
function $h(self, select) {
	return [ __clone(self), select ];
}
function $q(self) {
	return __clone(self[0].v);
}
function $s(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $r(self, subscriber) {
	return $s(self, subscriber);
}
function $p(self) {
	return [ () => {
		return $q(self);
	}, (subscriber) => {
		return $r(self, subscriber);
	}, () => {
		return;
	} ];
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
function $N(self, subscriber) {
	return $s(self, subscriber);
}
function $L(self) {
	return [ () => {
		return $q(self);
	}, (subscriber) => {
		return $N(self, subscriber);
	}, () => {
		return;
	} ];
}
function $S(self) {
	return self.length === 0;
}
function $T(self) {
	let $V = null;
	if ($S(self)) {
		$V = [ 1 ];
	} else {
		$V = __list_get(self, self.length - 1);
	}
	return $V;
}
function $o(self) {
	const select = self[1];
	const runs = new2();
	const outer2 = $p(__clone(self[0]));
	const outer_pull = outer2[0];
	const followed = __shared_new($L($v(runs, ($t, $u) => {
		return select(outer_pull(), $t, $u);
	})));
	const followed_handle = __shared_new([ 1 ]);
	return [ () => {
		return followed.v[0]();
	}, (subscriber) => {
		followed_handle.v = [ 0, followed.v[1](relay_to(subscriber)) ];
		const outer_handle = outer2[1](subscriber_of(() => {
			detach_held(followed_handle);
			followed.v[2]();
			const next = $L($v(runs, ($al, $am) => {
				return select(outer_pull(), $al, $am);
			}));
			followed_handle.v = [ 0, next[1](relay_to(subscriber)) ];
			followed.v = next;
			wake(subscriber);
			return;
		}, true));
		return retiring(subscriber, () => {
			detach_held(followed_handle);
			detach(outer_handle);
			return;
		});
	}, () => {
		release_runs(runs);
		followed.v[2]();
		outer2[2]();
		return;
	} ];
}
function $ap(self, $aq) {
	const $ar = $aq;
	let $as = null;
	if ($ar[0] === 0) {
		const turn = $ar[1];
		$as = enqueue(turn, self[1].v);
	} else {
		const $at = $T(draining_turns.v);
		let $au = null;
		if ($at[0] === 0) {
			const draining = $at[1];
			$au = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$au = undefined;
		}
		$as = $au;
	}
	return $as;
}
function $an(self, value, $ao) {
	self[0].v = __clone(value);
	$ap(self, $ao);
}
function $aB(self, item, $aC) {
	defer(self, () => {
		dispose(item, $aC);
		return;
	});
	return __clone(item);
}
function $l(self, $m, $n) {
	const instance = $o(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$an(cached, pull(), $m);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $m, $n);
	return cached;
}
function $i(self, $j, $k) {
	return [ $l(self, $j, $k) ];
}
function $aH(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
function $aJ(self) {
	return $q(self[0]);
}
function $aK(self, value, $ao) {
	self[0].v = __clone(value);
	$ap(self, $ao);
}
function $aT(self, transform) {
	return [ __clone(self), transform ];
}
function $aY(self, subscriber) {
	return $s(self[0], subscriber);
}
function $aX(self) {
	return [ () => {
		return $aJ(self);
	}, (subscriber) => {
		return $aY(self, subscriber);
	}, () => {
		return;
	} ];
}
function $bb(runs, body) {
	const run = renew(runs, true);
	const $bc = nursery(run);
	let $bd = null;
	if ($bc[0] === 0) {
		const nursery2 = $bc[1];
		$bd = (($J) => {
			return (($K) => {
				return body($J, $K);
			})(nursery2);
		})(run);
	} else {
		$bd = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $bd;
}
function $aW(self) {
	const transform = self[1];
	const upstream = $aX(__clone(self[0]));
	const pull = upstream[0];
	const runs = new2();
	return [ () => {
		const value = pull();
		return $bb(runs, ($aZ, $ba) => {
			return transform(value, $aZ, $ba);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $aV(self, $m, $n) {
	const instance = $aW(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$an(cached, pull(), $m);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $m, $n);
	return cached;
}
function $aU(self, $j, $k) {
	return [ $aV(self, $j, $k) ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const first = $a(1);
const second = $a(10);
const outer = $c(first);
const $aI = $aH(($e) => {
	return $i($h(__clone(outer), (inner, $f, $g) => {
		return __clone(inner);
	}), [ 1 ], [ 0, $e ]);
});
const joined = $aI[0];
const scope = $aI[1];
console.log($aJ(joined));
$an(first, 2, [ 1 ]);
console.log($aJ(joined));
$aK(outer, second, [ 1 ]);
console.log($aJ(joined));
$an(first, 99, [ 1 ]);
console.log($aJ(joined));
$an(second, 11, [ 1 ]);
console.log($aJ(joined));
const $be = $aH(($aQ) => {
	return $aU($aT(__clone(joined), (value, $aR, $aS) => {
		return value * 2;
	}), [ 1 ], [ 0, $aQ ]);
});
const doubled = $be[0];
const stacked = $be[1];
$an(second, 21, [ 1 ]);
console.log($aJ(doubled));
dispose2(stacked);
dispose2(scope);
