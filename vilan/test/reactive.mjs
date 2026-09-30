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
	return $Q(self[0].v) && $Q(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $P = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$P = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$P;
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
				while (!($Q(turn[1].v)) && budget > 0) {
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
function dispose(self, $ae) {
	const $af = $ae;
	let $ag = null;
	if ($af[0] === 0) {
		const established = $af[1];
		$ag = [ 0, established ];
	} else {
		$ag = $R(draining_turns.v);
	}
	const ambient = $ag;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $ah = [ 0, handle[0] ];
	let $ai = null;
	if ($ah[0] === 0) {
		const subscribers = $ah[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$ai = undefined;
	} else {
		$ai = undefined;
	}
	$ai;
	const $aj = ambient;
	let $ak = null;
	if ($aj[0] === 0) {
		const turn = $aj[1];
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
		$ak = undefined;
	} else {
		$ak = undefined;
	}
	$ak;
	const $al = handle[3].v;
	let $am = null;
	if ($al[0] === 0) {
		const release = $al[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$am = undefined;
	} else {
		$am = undefined;
	}
	return $am;
}
function new2() {
	return [ __shared_new([ 0, [ 1 ], [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $ap = null;
	if (is_disposed(self)) {
		cleanup();
	} else {
		const $an = self[0].v[1];
		let $ao = null;
		if ($an[0] === 0) {
			const list = $an[1];
			$ao = list.v.push(cleanup);
		} else {
			owner_lists_allocated_count.v = owner_lists_allocated_count.v + 1;
			self[0].v[1] = [ 0, __shared_new([ cleanup ]) ];
			$ao = undefined;
		}
		$ap = $ao;
	}
	return $ap;
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
	let $E = null;
	if (is_disposed(self)) {
		$E = [ 1 ];
	} else {
		$E = self[0].v[2];
	}
	return $E;
}
function dispose2(self) {
	let $D = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, [ 1 ], [ 1 ] ];
		const $t = held[2];
		let $u = null;
		if ($t[0] === 0) {
			const nursery2 = $t[1];
			$u = nursery2.cancel();
		} else {
			$u = undefined;
		}
		$u;
		const $v = held[1];
		let $w = null;
		if ($v[0] === 0) {
			const cleanups = $v[1];
			let failure = [ 1 ];
			for (const cleanup of cleanups.v) {
				const $x = __guarded(cleanup);
				let $y = null;
				if ($x[0] === 0) {
					const message = $x[1];
					if ($z(failure)) {
						failure = [ 0, message ];
					}
					$y = undefined;
				} else {
					$y = undefined;
				}
				$y;
			}
			const $B = failure;
			let $C = null;
			if ($B[0] === 0) {
				const message2 = $B[1];
				$C = (() => {
					throw message2;
				})();
			} else {
				$C = undefined;
			}
			$w = $C;
		} else {
			$w = undefined;
		}
		$D = $w;
	}
	return $D;
}
function register_with_owner(subscription, $Y, $Z) {
	const $aa = $Z;
	let $ab = null;
	if ($aa[0] === 0) {
		const owner2 = $aa[1];
		$ab = $ac(owner2, subscription, $Y);
	} else {
		$ab = __clone(subscription);
	}
	return $ab;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $W = previous;
		let $X = null;
		if ($W[0] === 0) {
			const earlier = $W[1];
			$X = earlier();
		} else {
			$X = undefined;
		}
		return $X;
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
function $z(self) {
	const $A = self;
	return $A[0] === 1;
}
function $s(runs, body) {
	const run = renew(runs, true);
	const $F = nursery(run);
	let $G = null;
	if ($F[0] === 0) {
		const nursery2 = $F[1];
		$G = (($H) => {
			return (($I) => {
				return body($H, $I);
			})(nursery2);
		})(run);
	} else {
		$G = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $G;
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
function $Q(self) {
	return self.length === 0;
}
function $R(self) {
	let $T = null;
	if ($Q(self)) {
		$T = [ 1 ];
	} else {
		$T = __list_get(self, self.length - 1);
	}
	return $T;
}
function $L(self, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const turn = $N[1];
		$O = enqueue(turn, self[1].v);
	} else {
		const $U = $R(draining_turns.v);
		let $V = null;
		if ($U[0] === 0) {
			const draining = $U[1];
			$V = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$V = undefined;
		}
		$O = $V;
	}
	return $O;
}
function $J(self, value, $K) {
	self[0].v = __clone(value);
	$L(self, $K);
}
function $ac(self, item, $ad) {
	defer(self, () => {
		dispose(item, $ad);
		return;
	});
	return __clone(item);
}
function $i(self, $j, $k) {
	const instance = $l(self);
	const pull = instance[0];
	const cached = $b(pull());
	const refreshed = instance[1](subscriber_of(() => {
		$J(cached, pull(), $j);
		return;
	}, true));
	also_releasing(refreshed, instance[2]);
	register_with_owner(refreshed, $j, $k);
	return cached;
}
function $f(self, $g, $h) {
	return [ $i(self, $g, $h) ];
}
function $at(signal, observer) {
	const cell = signal[0];
	return $p(signal, mint_subscriber(() => {
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
function $as(self, observer, immediately) {
	const subscription = $at(self, observer);
	if (immediately) {
		observer($n(self));
	}
	return subscription;
}
function $ar(self, observer, immediately) {
	return $as(self[0], observer, immediately);
}
function $aq(self, observer) {
	return $ar(self, observer, true);
}
function $aw(self, transform, $ax) {
	$J(self, transform($n(self)), $ax);
}
function $ay(self) {
	return $n(self[0]);
}
function $az(self, observer) {
	return $as(self, observer, true);
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const releasing_turns = __shared_new([  ]);
const owner_lists_allocated_count = __shared_new(0);
const owner = new2();
const count = $a(0);
const doubled = $f($e(__clone(count), (n, $c, $d) => {
	return n * 2;
}), [ 1 ], [ 1 ]);
$ac(owner, $aq(__clone(doubled), (n) => {
	return console.log(n);
}), [ 1 ]);
$J(count, 1, [ 1 ]);
$aw(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($ay(doubled));
$ac(owner, $az(__clone(count), (n) => {
	return console.log(n);
}), [ 1 ]);
$J(count, 20, [ 1 ]);
