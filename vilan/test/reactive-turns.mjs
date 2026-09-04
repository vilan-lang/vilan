function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __list_get(list, index) {
	return index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];
}
function __list_pop(list) {
	return list.length === 0 ? [ 1 ] : [ 0, list.pop() ];
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
function hash(self) {
	return __hash(self);
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(new Map()), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		const key = hash(subscriber[0]);
		if (!(turn[1].v.has(key))) {
			turn[1].v.set(key, true);
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[3].v && !(turn[4].v) && !(turn[2].v)) {
		turn[4].v = true;
		queueMicrotask(() => {
			turn[4].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[2].v)) {
		turn[2].v = true;
		draining_turns.v.push(__clone(turn));
		let budget = 100000;
		while (!($m(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			turn[1].v = new Map();
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[2].v = false;
	}
}
function flush($r) {
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const turn = $s[1];
		$t = drain(turn);
	} else {
		$t = undefined;
	}
	return $t;
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
function $d(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $e(self) {
	return self[0].v;
}
function $c(self, observer) {
	const subscription = $d(self, observer);
	observer($e(self));
	return subscription;
}
function $m(self) {
	return self.length === 0;
}
function $n(self) {
	return __list_get(self, self.length - 1);
}
function $i(self, $j) {
	const $k = $j;
	let $l = null;
	if ($k[0] === 0) {
		const turn = $k[1];
		$l = enqueue(turn, self[1].v);
	} else {
		const $o = $n(draining_turns.v);
		let $p = null;
		if ($o[0] === 0) {
			const draining = $o[1];
			$p = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$p = undefined;
		}
		$l = $p;
	}
	return $l;
}
function $g(self, value, $h) {
	self[0].v = value;
	$i(self, $h);
}
function $w(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[3].v = true;
	return result;
}
function $y(body, $z) {
	const $A = $z;
	let $B = null;
	if ($A[0] === 0) {
		const current = $A[1];
		$B = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[3].v = true;
		$B = result;
	}
	return $B;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const a = $a(0);
const b = $a(0);
$c(a, (value) => {
	return console.log("a -> " + value);
});
$c(b, (value) => {
	return console.log("b -> " + value);
});
const turn_a = new2();
const turn_b = new2();
(($f) => {
	$g(a, 1, [ 0, $f ]);
	return;
})(turn_a);
(($q) => {
	$g(b, 1, [ 0, $q ]);
	flush([ 0, $q ]);
	return;
})(turn_b);
console.log("mid");
(($u) => {
	return flush([ 0, $u ]);
})(turn_a);
$w([ 0 ], ($v) => {
	$g(a, 2, [ 0, $v ]);
	$g(b, 2, [ 0, $v ]);
	console.log("inside");
	return;
});
$y(($x) => {
	$g(a, 3, [ 0, $x ]);
	console.log("batched");
	return;
}, [ 1 ]);
$w([ 0 ], ($C) => {
	$y(($D) => {
		$g(a, 4, [ 0, $D ]);
		return;
	}, [ 0, $C ]);
	console.log("joined");
	return;
});
const turn_c = new2();
(($E) => {
	__task(async () => {
		await (await (tick()));
		$g(a, 5, [ 0, $E ]);
		flush([ 0, $E ]);
		return;
	}, "main");
	return;
})(turn_c);
console.log("end-sync");
