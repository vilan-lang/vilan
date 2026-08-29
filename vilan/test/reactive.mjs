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
		while (!($l(turn[0].v)) && budget > 0) {
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
function dispose(self, $w) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $x = $w;
	let $y = null;
	if ($x[0] === 0) {
		const turn = $x[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$y = undefined;
	} else {
		$y = undefined;
	}
	$y;
	const $z = self[2].v;
	let $A = null;
	if ($z[0] === 0) {
		const release = $z[1];
		self[2].v = [ 1 ];
		release();
		$A = undefined;
	} else {
		$A = undefined;
	}
	return $A;
}
function new2() {
	return [ __shared_new([  ]) ];
}
function register_with_owner(subscription, $q, $r) {
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const owner2 = $s[1];
		$t = $u(owner2, subscription, $q);
	} else {
		$t = __clone(subscription);
	}
	return $t;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $e(self) {
	return self[0].v;
}
function $l(self) {
	return self.length === 0;
}
function $m(self) {
	return __list_get(self, self.length - 1);
}
function $h(self, $i) {
	const $j = $i;
	let $k = null;
	if ($j[0] === 0) {
		const turn = $j[1];
		$k = enqueue(turn, self[1].v);
	} else {
		const $n = $m(draining_turns.v);
		let $o = null;
		if ($n[0] === 0) {
			const draining = $n[1];
			$o = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$o = undefined;
		}
		$k = $o;
	}
	return $k;
}
function $f(self, value, $g) {
	self[0].v = value;
	$h(self, $g);
}
function $p(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $u(self, item, $v) {
	self[0].v.push(() => {
		dispose(item, $v);
		return;
	});
	return __clone(item);
}
function $b(self, transform, $c, $d) {
	const derived = $a(transform($e(self)));
	register_with_owner($p(self, (value) => {
		$f(derived, transform(value), $c);
		return;
	}), $c, $d);
	return derived;
}
function $B(self, observer) {
	const subscription = $p(self, observer);
	observer($e(self));
	return subscription;
}
function $C(self, transform, $D) {
	$f(self, transform($e(self)), $D);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $b(count, (n) => {
	return n * 2;
}, [ 1 ], [ 1 ]);
$u(owner, $B(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$f(count, 1, [ 1 ]);
$C(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($e(doubled));
$u(owner, $B(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$f(count, 20, [ 1 ]);
