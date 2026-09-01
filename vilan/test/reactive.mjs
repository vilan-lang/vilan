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
		while (!($m(turn[0].v)) && budget > 0) {
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
function dispose(self, $x) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const turn = $y[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$z = undefined;
	} else {
		$z = undefined;
	}
	$z;
	const $A = self[2].v;
	let $B = null;
	if ($A[0] === 0) {
		const release = $A[1];
		self[2].v = [ 1 ];
		release();
		$B = undefined;
	} else {
		$B = undefined;
	}
	return $B;
}
function new2() {
	return [ __shared_new([  ]) ];
}
function register_with_owner(subscription, $r, $s) {
	const $t = $s;
	let $u = null;
	if ($t[0] === 0) {
		const owner2 = $t[1];
		$u = $v(owner2, subscription, $r);
	} else {
		$u = __clone(subscription);
	}
	return $u;
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
function $q(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $v(self, item, $w) {
	self[0].v.push(() => {
		dispose(item, $w);
		return;
	});
	return __clone(item);
}
function $c(self, transform, $d, $e) {
	const derived = $b(transform($f(self)));
	register_with_owner($q(self, (value) => {
		$g(derived, transform(value), $d);
		return;
	}), $d, $e);
	return derived;
}
function $C(self, observer) {
	const subscription = $q(self, observer);
	observer($f(self));
	return subscription;
}
function $D(self, transform, $E) {
	$g(self, transform($f(self)), $E);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $c(count, (n) => {
	return n * 2;
}, [ 1 ], [ 1 ]);
$v(owner, $C(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$g(count, 1, [ 1 ]);
$D(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($f(doubled));
$v(owner, $C(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$g(count, 20, [ 1 ]);
