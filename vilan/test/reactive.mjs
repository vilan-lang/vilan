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
	return $P(self[0].v) && $P(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $O = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$O = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$O;
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
				while (!($P(turn[1].v)) && budget > 0) {
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
function dispose(self, $ad) {
	const $ae = $ad;
	let $af = null;
	if ($ae[0] === 0) {
		const established = $ae[1];
		$af = [ 0, established ];
	} else {
		$af = $Q(draining_turns.v);
	}
	const ambient = $af;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ag = [ 0, handle[0] ];
	let $ah = null;
	if ($ag[0] === 0) {
		const subscribers = $ag[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ah = undefined;
	} else {
		$ah = undefined;
	}
	$ah;
	const $ai = ambient;
	let $aj = null;
	if ($ai[0] === 0) {
		const turn = $ai[1];
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
		$aj = undefined;
	} else {
		$aj = undefined;
	}
	$aj;
	const $ak = handle[3].v;
	let $al = null;
	if ($ak[0] === 0) {
		const release = $ak[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$al = undefined;
	} else {
		$al = undefined;
	}
	return $al;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $am = null;
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
		$am = undefined;
	}
	return $am;
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
	let $D = null;
	if (is_disposed(self)) {
		$D = [ 1 ];
	} else {
		$D = self[0].v[3];
	}
	return $D;
}
function dispose2(self) {
	let $C = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $t = held[3];
		let $u = null;
		if ($t[0] === 0) {
			const nursery2 = $t[1];
			$u = nursery2.cancel();
		} else {
			$u = undefined;
		}
		$u;
		let $B = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $v = __guarded(cleanup);
				let $w = null;
				if ($v[0] === 0) {
					const message = $v[1];
					if ($x(failure)) {
						failure = [ 0, message ];
					}
					$w = undefined;
				} else {
					$w = undefined;
				}
				$w;
			}
			const $z = failure;
			let $A = null;
			if ($z[0] === 0) {
				const message2 = $z[1];
				$A = (() => {
					throw message2;
				})();
			} else {
				$A = undefined;
			}
			$B = $A;
		}
		$C = $B;
	}
	return $C;
}
function register_with_owner(subscription, $X, $Y) {
	const $Z = $Y;
	let $aa = null;
	if ($Z[0] === 0) {
		const owner2 = $Z[1];
		$aa = $ab(owner2, subscription, $X);
	} else {
		$aa = __clone(subscription);
	}
	return $aa;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $V = previous;
		let $W = null;
		if ($V[0] === 0) {
			const earlier = $V[1];
			$W = earlier();
		} else {
			$W = undefined;
		}
		return $W;
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
function $e(self, transform) {
	return [ __clone(self), transform ];
}
function $n(self) {
	return __clone(self[0].v);
}
function $p(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $o(self, subscriber) {
	return $p(self, subscriber);
}
function $m(self) {
	return [ () => {
		return $n(self);
	}, (subscriber) => {
		return $o(self, subscriber);
	}, () => {
		return;
	} ];
}
function $x(self) {
	const $y = self;
	return $y[0] === 1;
}
function $s(runs, body) {
	const run = renew(runs, true);
	const $E = nursery(run);
	let $F = null;
	if ($E[0] === 0) {
		const nursery2 = $E[1];
		$F = (($G) => {
			return (($H) => {
				return body($G, $H);
			})(nursery2);
		})(run);
	} else {
		$F = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $F;
}
function $l(self) {
	const transform = self[1];
	const upstream = $m(__clone(self[0]));
	const pull = upstream[0];
	const runs = new2();
	return [ () => {
		const value = pull();
		return $s(runs, ($q, $r) => {
			return transform(value, $q, $r);
		});
	}, upstream[1], () => {
		release_runs(runs);
		upstream[2]();
		return;
	} ];
}
function $P(self) {
	return self.length === 0;
}
function $Q(self) {
	let $S = null;
	if ($P(self)) {
		$S = [ 1 ];
	} else {
		$S = __list_get(self, self.length - 1);
	}
	return $S;
}
function $K(self, $L) {
	const $M = $L;
	let $N = null;
	if ($M[0] === 0) {
		const turn = $M[1];
		$N = enqueue(turn, self[1].v);
	} else {
		const $T = $Q(draining_turns.v);
		let $U = null;
		if ($T[0] === 0) {
			const draining = $T[1];
			$U = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$U = undefined;
		}
		$N = $U;
	}
	return $N;
}
function $I(self, value, $J) {
	self[0].v = __clone(value);
	$K(self, $J);
}
function $ab(self, item, $ac) {
	defer(self, () => {
		dispose(item, $ac);
		return;
	});
	return __clone(item);
}
function $i(self, $j, $k) {
	const instance = $l(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$I(cached, pull(), $j);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $j, $k);
	return cached;
}
function $f(self, $g, $h) {
	return [ $i(self, $g, $h) ];
}
function $aq(signal, observer) {
	const cell = signal[0];
	return $p(signal, mint_subscriber(() => {
		const $ar = [ 0, cell ];
		let $as = null;
		if ($ar[0] === 0) {
			const live = $ar[1];
			$as = observer(live.v);
		} else {
			$as = undefined;
		}
		return $as;
	}));
}
function $ap(self, observer, immediately) {
	const subscription = $aq(self, observer);
	if (immediately) {
		observer($n(self));
	}
	return subscription;
}
function $ao(self, observer, immediately) {
	return $ap(self[0], observer, immediately);
}
function $an(self, observer) {
	return $ao(self, observer, true);
}
function $at(self, transform, $au) {
	$I(self, transform($n(self)), $au);
}
function $av(self) {
	return $n(self[0]);
}
function $aw(self, observer) {
	return $ap(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const no_cleanups = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const owner = new2();
const count = $a(0);
const doubled = $f($e(__clone(count), (n, $c, $d) => {
	return n * 2;
}), [ 1 ], [ 1 ]);
$ab(owner, $an(__clone(doubled), (n) => {
	return console.log(n);
}), [ 1 ]);
$I(count, 1, [ 1 ]);
$at(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($av(doubled));
$ab(owner, $aw(__clone(count), (n) => {
	return console.log(n);
}), [ 1 ]);
$I(count, 20, [ 1 ]);
