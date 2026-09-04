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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
		while (!($n(turn[0].v)) && budget > 0) {
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
function dispose(self, $z) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $A = $z;
	let $B = null;
	if ($A[0] === 0) {
		const turn = $A[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$B = undefined;
	} else {
		$B = undefined;
	}
	$B;
	const $C = self[2].v;
	let $D = null;
	if ($C[0] === 0) {
		const release = $C[1];
		self[2].v = [ 1 ];
		release();
		$D = undefined;
	} else {
		$D = undefined;
	}
	return $D;
}
function new2() {
	return [ __shared_new([  ]) ];
}
function register_with_owner(subscription, $t, $u) {
	const $v = $u;
	let $w = null;
	if ($v[0] === 0) {
		const owner2 = $v[1];
		$w = $x(owner2, subscription, $t);
	} else {
		$w = __clone(subscription);
	}
	return $w;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $f(self) {
	return self[0].v;
}
function $n(self) {
	return self.length === 0;
}
function $o(self) {
	return __list_get(self, self.length - 1);
}
function $j(self, $k) {
	const $l = $k;
	let $m = null;
	if ($l[0] === 0) {
		const turn = $l[1];
		$m = enqueue(turn, self[1].v);
	} else {
		const $p = $o(draining_turns.v);
		let $q = null;
		if ($p[0] === 0) {
			const draining = $p[1];
			$q = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$q = undefined;
		}
		$m = $q;
	}
	return $m;
}
function $h(self, value, $i) {
	self[0].v = value;
	$j(self, $i);
}
function $s(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $r(self, observer) {
	return $s(self, observer);
}
function $x(self, item, $y) {
	self[0].v.push(() => {
		dispose(item, $y);
		return;
	});
	return __clone(item);
}
function $c(self, transform, $d, $e) {
	const derived = $b(transform($f(self)));
	register_with_owner($r(self, (value) => {
		$h(derived, transform(value), $d);
		return;
	}), $d, $e);
	return derived;
}
function $E(self, observer) {
	const subscription = $s(self, observer);
	observer($f(self));
	return subscription;
}
function $G(self, $k) {
	const $H = $k;
	let $I = null;
	if ($H[0] === 0) {
		const turn = $H[1];
		$I = enqueue(turn, self[1].v);
	} else {
		const $J = $o(draining_turns.v);
		let $K = null;
		if ($J[0] === 0) {
			const draining = $J[1];
			$K = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$K = undefined;
		}
		$I = $K;
	}
	return $I;
}
function $F(self, value, $i) {
	self[0].v = value;
	$G(self, $i);
}
function $L(self, transform, $M) {
	$F(self, transform($f(self)), $M);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $c(count, (n) => {
	return n * 2;
}, [ 1 ], [ 1 ]);
$x(owner, $E(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$F(count, 1, [ 1 ]);
$L(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($f(doubled));
$x(owner, $E(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$F(count, 20, [ 1 ]);
