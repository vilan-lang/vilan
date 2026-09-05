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
function fresh_id() {
	const id = next_subscriber_id.v;
	next_subscriber_id.v = id + 1;
	return id;
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
		while (!($A(turn[0].v)) && budget > 0) {
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
function dispose(self, $o) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $p = $o;
	let $q = null;
	if ($p[0] === 0) {
		const turn = $p[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$q = undefined;
	} else {
		$q = undefined;
	}
	$q;
	const $r = self[2].v;
	let $s = null;
	if ($r[0] === 0) {
		const release = $r[1];
		self[2].v = [ 1 ];
		release();
		$s = undefined;
	} else {
		$s = undefined;
	}
	return $s;
}
function new2() {
	return [ __shared_new([  ]) ];
}
function dispose2(self) {
	for (const cleanup of self[0].v) {
		cleanup();
	}
	self[0].v = [  ];
}
function get_owner($j) {
	return $j;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $l(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $k(self, observer) {
	return $l(self, observer);
}
function $m(self, item, $n) {
	self[0].v.push(() => {
		dispose(item, $n);
		return;
	});
	return __clone(item);
}
function $g(self, observer, $h, $i) {
	$m(get_owner($i), $k(self, observer), $h);
}
function $t(self) {
	return self[0].v;
}
function $d(self, observer, $e, $f) {
	$g(self, observer, $e, $f);
	observer($t(self));
}
function $A(self) {
	return self.length === 0;
}
function $B(self) {
	return __list_get(self, self.length - 1);
}
function $w(self, $x) {
	const $y = $x;
	let $z = null;
	if ($y[0] === 0) {
		const turn = $y[1];
		$z = enqueue(turn, self[1].v);
	} else {
		const $C = $B(draining_turns.v);
		let $D = null;
		if ($C[0] === 0) {
			const draining = $C[1];
			$D = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$D = undefined;
		}
		$z = $D;
	}
	return $z;
}
function $u(self, value, $v) {
	self[0].v = value;
	$w(self, $v);
}
function $H(owner2, body) {
	return body(owner2);
}
function $J(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const count = $a(1);
const owner = new2();
(($c) => {
	$d(count, (value) => {
		return console.log("seen " + value);
	}, [ 1 ], $c);
	return;
})(owner);
$u(count, 2, [ 1 ]);
dispose2(owner);
$u(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($E) => {
	(($F) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $F);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $E);
	return;
})(outer);
$u(count, 4, [ 1 ]);
dispose2(inner);
$u(count, 5, [ 1 ]);
dispose2(outer);
$u(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$H(wrapped, ($G) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $G);
	return;
});
$u(count, 7, [ 1 ]);
dispose2(wrapped);
$u(count, 8, [ 1 ]);
console.log("fin");
const $K = $J(($I) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $I);
	return "built";
});
const label = $K[0];
const scope = $K[1];
console.log(label);
$u(count, 9, [ 1 ]);
dispose2(scope);
$u(count, 10, [ 1 ]);
console.log("post");
