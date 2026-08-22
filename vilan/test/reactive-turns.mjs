function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new2() {
	return [ __shared_new([  ]), __shared_new(false), __shared_new(false), __shared_new(false) ];
}
function enqueue(turn, subscribers) {
	for (const subscriber of subscribers) {
		let seen = false;
		for (const queued of turn[0].v) {
			if (queued[0] === subscriber[0]) {
				seen = true;
			}
		}
		if (!(seen)) {
			turn[0].v.push(__clone(subscriber));
		}
	}
	if (turn[2].v && !(turn[3].v) && !(turn[1].v)) {
		turn[3].v = true;
		queueMicrotask(() => {
			turn[3].v = false;
			drain(turn);
			return;
		});
	}
}
function drain(turn) {
	if (!(turn[1].v)) {
		turn[1].v = true;
		draining_turns.v.push(__clone(turn));
		let budget = 100000;
		while (!($k(turn[0].v)) && budget > 0) {
			const wave = turn[0].v;
			turn[0].v = [  ];
			for (const subscriber of wave) {
				subscriber[1]();
				budget = budget - 1;
			}
		}
		__list_pop(draining_turns.v);
		turn[1].v = false;
	}
}
function flush($p) {
	const $q = $p;
	let $r = null;
	if ($q[0] === 0) {
		const turn = $q[1];
		$r = drain(turn);
	} else {
		$r = undefined;
	}
	return $r;
}
async function tick() {

}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $c(self) {
	return self[0].v;
}
function $b(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($c(self));
		return;
	} ]);
	observer($c(self));
	return [ self[1], id, __shared_new([ 1 ]) ];
}
function $k(self) {
	return self.length === 0;
}
function $l(self) {
	return __list_get(self, self.length - 1);
}
function $g(self, $h) {
	const $i = $h;
	let $j = null;
	if ($i[0] === 0) {
		const turn = $i[1];
		$j = enqueue(turn, self[1].v);
	} else {
		const $m = $l(draining_turns.v);
		let $n = null;
		if ($m[0] === 0) {
			const draining = $m[1];
			$n = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$n = undefined;
		}
		$j = $n;
	}
	return $j;
}
function $e(self, value, $f) {
	self[0].v = value;
	$g(self, $f);
}
function $u(policy, body) {
	const fresh = new2();
	const result = body(fresh);
	drain(fresh);
	fresh[2].v = true;
	return result;
}
function $w(body, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const current = $y[1];
		$z = body(current);
	} else {
		const fresh = new2();
		const result = body(fresh);
		drain(fresh);
		fresh[2].v = true;
		$z = result;
	}
	return $z;
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const a = $a(0);
const b = $a(0);
$b(a, (value) => {
	return console.log("a -> " + value);
});
$b(b, (value) => {
	return console.log("b -> " + value);
});
const turn_a = new2();
const turn_b = new2();
(($d) => {
	$e(a, 1, [ 0, $d ]);
	return;
})(turn_a);
(($o) => {
	$e(b, 1, [ 0, $o ]);
	flush([ 0, $o ]);
	return;
})(turn_b);
console.log("mid");
(($s) => {
	return flush([ 0, $s ]);
})(turn_a);
$u([ 0 ], ($t) => {
	$e(a, 2, [ 0, $t ]);
	$e(b, 2, [ 0, $t ]);
	console.log("inside");
	return;
});
$w(($v) => {
	$e(a, 3, [ 0, $v ]);
	console.log("batched");
	return;
}, [ 1 ]);
$u([ 0 ], ($A) => {
	$w(($B) => {
		$e(a, 4, [ 0, $B ]);
		return;
	}, [ 0, $A ]);
	console.log("joined");
	return;
});
const turn_c = new2();
(($C) => {
	__task(async () => {
		await (await (tick()));
		$e(a, 5, [ 0, $C ]);
		flush([ 0, $C ]);
		return;
	}, "main");
	return;
})(turn_c);
console.log("end-sync");
