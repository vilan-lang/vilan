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
		let $af = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$af = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1)[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber));
		}
		$af;
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
function dispose(self, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const established = $N[1];
		$O = [ 0, established ];
	} else {
		$O = $P(draining_turns.v);
	}
	const ambient = $O;
	release_under(self, ambient);
}
function release_under(handle, ambient) {
	handle[2].v = false;
	const $S = [ 0, handle[0] ];
	let $T = null;
	if ($S[0] === 0) {
		const subscribers = $S[1];
		let kept = [  ];
		for (const subscriber of subscribers.v) {
			if (subscriber[0] !== handle[1]) {
				kept.push(__clone(subscriber));
			}
		}
		subscribers.v = kept;
		$T = undefined;
	} else {
		$T = undefined;
	}
	$T;
	const $U = ambient;
	let $V = null;
	if ($U[0] === 0) {
		const turn = $U[1];
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
		$V = undefined;
	} else {
		$V = undefined;
	}
	$V;
	const $W = handle[3].v;
	let $X = null;
	if ($W[0] === 0) {
		const release = $W[1];
		handle[3].v = [ 1 ];
		releasing_turns.v.push(ambient);
		__with_finally(release, () => {
			__list_pop(releasing_turns.v);
			return;
		});
		$X = undefined;
	} else {
		$X = undefined;
	}
	return $X;
}
function new2() {
	return [ __shared_new([ 0, no_cleanups, false, [ 1 ] ]), 0 ];
}
function is_disposed(self) {
	return self[0].v[0] !== self[1];
}
function defer(self, cleanup) {
	let $Y = null;
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
		$Y = undefined;
	}
	return $Y;
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
	let $v = null;
	if (is_disposed(self)) {
		$v = [ 1 ];
	} else {
		$v = self[0].v[3];
	}
	return $v;
}
function dispose2(self) {
	let $u = null;
	if (!(is_disposed(self))) {
		const held = self[0].v;
		self[0].v = [ self[1] + 1, no_cleanups, false, [ 1 ] ];
		const $l = held[3];
		let $m = null;
		if ($l[0] === 0) {
			const nursery2 = $l[1];
			$m = nursery2.cancel();
		} else {
			$m = undefined;
		}
		$m;
		let $t = null;
		if (held[2]) {
			let failure = [ 1 ];
			for (const cleanup of held[1].v) {
				const $n = __guarded(cleanup);
				let $o = null;
				if ($n[0] === 0) {
					const message = $n[1];
					if ($p(failure)) {
						failure = [ 0, message ];
					}
					$o = undefined;
				} else {
					$o = undefined;
				}
				$o;
			}
			const $r = failure;
			let $s = null;
			if ($r[0] === 0) {
				const message2 = $r[1];
				$s = (() => {
					throw message2;
				})();
			} else {
				$s = undefined;
			}
			$t = $s;
		}
		$u = $t;
	}
	return $u;
}
function get_owner($J) {
	return $J;
}
function release_runs(runs) {
	dispose2([ runs[0], runs[0].v[0] ]);
}
function also_releasing(handle, release) {
	const previous = handle[3].v;
	handle[3].v = [ 0, () => {
		release();
		const $H = previous;
		let $I = null;
		if ($H[0] === 0) {
			const earlier = $H[1];
			$I = earlier();
		} else {
			$I = undefined;
		}
		return $I;
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
function $p(self) {
	const $q = self;
	return $q[0] === 1;
}
function $k(runs, body) {
	const run = renew(runs, true);
	const $w = nursery(run);
	let $x = null;
	if ($w[0] === 0) {
		const nursery2 = $w[1];
		$x = (($y) => {
			return (($z) => {
				return body($y, $z);
			})(nursery2);
		})(run);
	} else {
		$x = (() => {
			throw "a renewed run carries its nursery";
		})();
	}
	return $x;
}
function $F(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $C(signal, observer) {
	const cell = signal[0];
	return $F(signal, mint_subscriber(() => {
		const $D = [ 0, cell ];
		let $E = null;
		if ($D[0] === 0) {
			const live = $D[1];
			$E = observer(live.v);
		} else {
			$E = undefined;
		}
		return $E;
	}));
}
function $G(self) {
	return __clone(self[0].v);
}
function $B(self, observer, immediately) {
	const subscription = $C(self, observer);
	if (immediately) {
		observer($G(self));
	}
	return subscription;
}
function $A(self, observer) {
	return $B(self, observer, true);
}
function $Q(self) {
	return self.length === 0;
}
function $P(self) {
	let $R = null;
	if ($Q(self)) {
		$R = [ 1 ];
	} else {
		$R = __list_get(self, self.length - 1);
	}
	return $R;
}
function $K(self, item, $L) {
	defer(self, () => {
		dispose(item, $L);
		return;
	});
	return __clone(item);
}
function $f(self, body, $g, $h) {
	const runs = new2();
	const subscription = $A(self, (value) => {
		return $k(runs, ($i, $j) => {
			return body(value, $i, $j);
		});
	});
	also_releasing(subscription, () => {
		return release_runs(runs);
	});
	$K(get_owner($h), subscription, $g);
}
function $ab(self, $ac) {
	const $ad = $ac;
	let $ae = null;
	if ($ad[0] === 0) {
		const turn = $ad[1];
		$ae = enqueue(turn, self[1].v);
	} else {
		const $ah = $P(draining_turns.v);
		let $ai = null;
		if ($ah[0] === 0) {
			const draining = $ah[1];
			$ai = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$ai = undefined;
		}
		$ae = $ai;
	}
	return $ae;
}
function $Z(self, value, $aa) {
	self[0].v = __clone(value);
	$ab(self, $aa);
}
function $as(owner2, body) {
	return body(owner2);
}
function $aw(body) {
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
	$f(__clone(count), (value, $d, $e) => {
		return console.log("seen " + value);
	}, [ 1 ], $c);
	return;
})(owner);
$Z(count, 2, [ 1 ]);
dispose2(owner);
$Z(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($aj) => {
	(($ak) => {
		$f(__clone(count), (value, $al, $am) => {
			return console.log("inner " + value);
		}, [ 1 ], $ak);
		return;
	})(inner);
	$f(__clone(count), (value, $an, $ao) => {
		return console.log("outer " + value);
	}, [ 1 ], $aj);
	return;
})(outer);
$Z(count, 4, [ 1 ]);
dispose2(inner);
$Z(count, 5, [ 1 ]);
dispose2(outer);
$Z(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$as(wrapped, ($ap) => {
	$f(__clone(count), (value, $aq, $ar) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $ap);
	return;
});
$Z(count, 7, [ 1 ]);
dispose2(wrapped);
$Z(count, 8, [ 1 ]);
console.log("fin");
const $ax = $aw(($at) => {
	$f(__clone(count), (value, $au, $av) => {
		return console.log("comp " + value);
	}, [ 1 ], $at);
	return "built";
});
const label = $ax[0];
const scope = $ax[1];
console.log(label);
$Z(count, 9, [ 1 ]);
dispose2(scope);
$Z(count, 10, [ 1 ]);
console.log("post");
