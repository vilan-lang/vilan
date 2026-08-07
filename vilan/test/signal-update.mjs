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
function hash(self) {
	return __hash(self);
}
function new2() {
	const table = new Map();
	return [ table ];
}
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
}
function new3() {
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
		while (!($h(turn[0].v)) && budget > 0) {
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
function dispose(self, $H) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $I = $H;
	let $J = null;
	if ($I[0] === 0) {
		const turn = $I[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$J = undefined;
	} else {
		$J = undefined;
	}
	return $J;
}
function new4() {
	return [ __shared_new([  ]) ];
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $h(self) {
	return self.length === 0;
}
function $i(self) {
	return __list_get(self, self.length - 1);
}
function $d(self, $e) {
	const $f = $e;
	let $g = null;
	if ($f[0] === 0) {
		const turn = $f[1];
		$g = enqueue(turn, self[1].v);
	} else {
		const $j = $i(draining_turns.v);
		let $k = null;
		if ($j[0] === 0) {
			const draining = $j[1];
			$k = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$k = undefined;
		}
		$g = $k;
	}
	return $g;
}
function $b(self, mutate, $c) {
	mutate(self[0].v);
	$d(self, $c);
}
function $l(self) {
	return self[0].v;
}
function $m(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $n(self, key, value) {
	self[0].set(hash(key), [ __clone(key), __clone(value) ]);
}
function $p(self, $e) {
	const $q = $e;
	let $r = null;
	if ($q[0] === 0) {
		const turn = $q[1];
		$r = enqueue(turn, self[1].v);
	} else {
		const $s = $i(draining_turns.v);
		let $t = null;
		if ($s[0] === 0) {
			const draining = $s[1];
			$t = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$t = undefined;
		}
		$r = $t;
	}
	return $r;
}
function $o(self, mutate, $c) {
	mutate(self[0].v);
	$p(self, $c);
}
function $u(self) {
	return self[0].v;
}
function $v(self) {
	return self[0].size;
}
function $w(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $y(self, $e) {
	const $z = $e;
	let $A = null;
	if ($z[0] === 0) {
		const turn = $z[1];
		$A = enqueue(turn, self[1].v);
	} else {
		const $B = $i(draining_turns.v);
		let $C = null;
		if ($B[0] === 0) {
			const draining = $B[1];
			$C = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$C = undefined;
		}
		$A = $C;
	}
	return $A;
}
function $x(self, mutate, $c) {
	mutate([ self[0], "v" ]);
	$y(self, $c);
}
function $D(self) {
	return self[0].v;
}
function $E(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($l(self));
		return;
	} ]);
	observer($l(self));
	return [ self[1], id ];
}
function $F(self, item, $G) {
	self[0].v.push(() => {
		dispose(item, $G);
		return;
	});
	return __clone(item);
}
function $L(body, $M) {
	const $N = $M;
	let $O = null;
	if ($N[0] === 0) {
		const current = $N[1];
		$O = body(current);
	} else {
		const fresh = new3();
		const result = body(fresh);
		drain(fresh);
		fresh[2].v = true;
		$O = result;
	}
	return $O;
}
function $R(self, value, $S) {
	self[0].v = value;
	$y(self, $S);
}
function $P(self, transform, $Q) {
	$R(self, transform($D(self)), $Q);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const owner = new4();
const todos = $a([ 1, 2 ]);
$b(todos, (list) => {
	list.push(5);
	return;
}, [ 1 ]);
console.log($l(todos).length);
const scores = $m(new2());
$o(scores, (entries) => {
	$n(entries, "a", 1);
	$n(entries, "b", 2);
	return;
}, [ 1 ]);
console.log($v($u(scores)));
const count = $w(1);
$x(count, (value) => {
	value[0][value[1]] = value[0][value[1]] + 10;
	return;
}, [ 1 ]);
console.log($D(count));
const watched = $a([ 0 ]);
$F(owner, $E(watched, (list) => {
	return console.log("len " + list.length);
}), [ 1 ]);
$b(watched, (list) => {
	list.push(1);
	list.push(2);
	return;
}, [ 1 ]);
$b(watched, (list) => {
	return;
}, [ 1 ]);
console.log("---");
$L(($K) => {
	$b(watched, (list) => {
		list.push(3);
		return;
	}, [ 0, $K ]);
	$b(watched, (list) => {
		list.push(4);
		return;
	}, [ 0, $K ]);
	console.log("inside");
	return;
}, [ 1 ]);
$b(todos, (list) => {
	list.push(6);
	console.log("reentrant " + $l(todos).length);
	return;
}, [ 1 ]);
$P(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($D(count));
