function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __insert_at(list, index, value, location) {
	if (index >= 0 && index < list.length) return void list.splice(index, 0, value);
	if (index === list.length) return void list.push(value);
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __shared_new(value) {
	return { v: value };
}
class __Task {
	constructor(run, origin, nursery) {
		this.origin = origin;
		this.observed = false;
		this.nursery = nursery;
		this.owned = !!nursery;
		this.rejected = false;
		this.error = undefined;
		this.promise = run();
		this.promise.then(null, (error) => {
			this.rejected = true;
			this.error = error;
			if (this.owned && !__nursery_is_cancel(error)) this.nursery.__fail(this);
			if (!this.observed && !this.owned) {
				globalThis.setTimeout(() => {
					if (!this.observed) console.error("unhandled task error (spawned in " + this.origin + "): " + String(error));
				}, 0);
			}
		});
		if (nursery) nursery.children.push(this);
	}
	then(onFulfilled, onRejected) {
		this.observed = true;
		return this.promise.then(onFulfilled, onRejected);
	}
}
function __task(run, origin, nursery) {
	return new __Task(run, origin, nursery);
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
function new2() {
	return [ __shared_new([  ]), __shared_new([  ]), __shared_new(new Map()), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function is_quiescent(self) {
	return $t(self[0].v) && $t(self[1].v);
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		let $s = null;
		if (subscriber[3]) {
			if (!(turn[3].v.has(key))) {
				turn[3].v.set(key, true);
				turn[1].v.push(__clone(subscriber));
			}
			$s = undefined;
		} else if (!(turn[2].v.has(key))) {
			turn[2].v.set(key, true);
			let index = turn[0].v.length;
			while (index > 0 && __at(turn[0].v, index - 1, "std/src/reactive.vl:414:21")[0] > subscriber[0]) {
				index = index - 1;
			}
			__insert_at(turn[0].v, index, __clone(subscriber), "std/src/reactive.vl:417:25");
		}
		$s;
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
				while (!($t(turn[1].v)) && budget > 0) {
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
function flush($A) {
	const $B = $A;
	let $C = null;
	if ($B[0] === 0) {
		const turn = $B[1];
		$C = drain(turn);
	} else {
		$C = undefined;
	}
	return $C;
}
function reissued(subscriber) {
	return [ subscriber[0], subscriber[1], subscriber[2], subscriber[3] ];
}
async function tick() {

}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $i(signal, subscriber) {
	const handle = [ signal[1], subscriber[0], subscriber[2], __shared_new([ 1 ]) ];
	signal[1].v.push(reissued(subscriber));
	return handle;
}
function $f(signal, observer) {
	const cell = signal[0];
	return $i(signal, mint_subscriber(() => {
		const $g = [ 0, cell ];
		let $h = null;
		if ($g[0] === 0) {
			const live = $g[1];
			$h = observer(live.v);
		} else {
			$h = undefined;
		}
		return $h;
	}));
}
function $j(self) {
	return __clone(self[0].v);
}
function $e(self, observer, immediately) {
	const subscription = $f(self, observer);
	if (immediately) {
		observer($j(self));
	}
	return subscription;
}
function $d(self, observer) {
	return $e(self, (value) => {
		return (() => {
			return observer(value, [ 1 ]);
		})();
	}, true);
}
function $t(self) {
	return self.length === 0;
}
function $u(self) {
	let $w = null;
	if ($t(self)) {
		$w = [ 1 ];
	} else {
		$w = __list_get(self, self.length - 1);
	}
	return $w;
}
function $o(self, $p) {
	const $q = $p;
	let $r = null;
	if ($q[0] === 0) {
		const turn = $q[1];
		$r = enqueue(turn, __clone(self[1].v));
	} else {
		const $x = $u(draining_turns.v);
		let $y = null;
		if ($x[0] === 0) {
			const draining = $x[1];
			$y = enqueue(draining, __clone(self[1].v));
		} else {
			for (const subscriber of __clone(self[1].v)) {
				if (subscriber[2].v) {
					subscriber[1]();
				}
			}
			$y = undefined;
		}
		$r = $y;
	}
	return $r;
}
function $m(self, value, $n) {
	self[0].v = __clone(value);
	$o(self, $n);
}
function $F(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[5].v = true;
	return result;
}
function $H(body, $I) {
	const $J = $I;
	let $K = null;
	if ($J[0] === 0) {
		const current = $J[1];
		$K = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[5].v = true;
		$K = result;
	}
	return $K;
}
const minting_derivation = __shared_new(false);
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const a = $a(0);
const b = $a(0);
$d(__clone(a), (value, $c) => {
	return console.log("a -> " + value);
});
$d(__clone(b), (value, $k) => {
	return console.log("b -> " + value);
});
const turn_a = new2();
const turn_b = new2();
(($l) => {
	$m(a, 1, [ 0, $l ]);
	return;
})(turn_a);
(($z) => {
	$m(b, 1, [ 0, $z ]);
	flush([ 0, $z ]);
	return;
})(turn_b);
console.log("mid");
(($D) => {
	return flush([ 0, $D ]);
})(turn_a);
$F([ 0 ], ($E) => {
	$m(a, 2, [ 0, $E ]);
	$m(b, 2, [ 0, $E ]);
	console.log("inside");
	return;
});
$H(($G) => {
	$m(a, 3, [ 0, $G ]);
	console.log("batched");
	return;
}, [ 1 ]);
$F([ 0 ], ($L) => {
	$H(($M) => {
		$m(a, 4, [ 0, $M ]);
		return;
	}, [ 0, $L ]);
	console.log("joined");
	return;
});
const turn_c = new2();
(($N) => {
	__task(async () => {
		await (await (tick()));
		$m(a, 5, [ 0, $N ]);
		flush([ 0, $N ]);
		return;
	}, "main");
	return;
})(turn_c);
console.log("end-sync");
