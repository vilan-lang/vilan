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
		while (!($x(turn[0].v)) && budget > 0) {
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
function dispose(self, $m) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $n = $m;
	let $o = null;
	if ($n[0] === 0) {
		const turn = $n[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		turn[1].v.delete(hash(self[1]));
		$o = undefined;
	} else {
		$o = undefined;
	}
	$o;
	const $p = self[2].v;
	let $q = null;
	if ($p[0] === 0) {
		const release = $p[1];
		self[2].v = [ 1 ];
		release();
		$q = undefined;
	} else {
		$q = undefined;
	}
	return $q;
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
function get_owner($g) {
	return $g;
}
function $b(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $a(value) {
	return $b(value);
}
function $i(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $j(self) {
	return self[0].v;
}
function $h(self, observer) {
	const subscription = $i(self, observer);
	observer($j(self));
	return subscription;
}
function $k(self, item, $l) {
	self[0].v.push(() => {
		dispose(item, $l);
		return;
	});
	return __clone(item);
}
function $d(self, observer, $e, $f) {
	$k(get_owner($f), $h(self, observer), $e);
}
function $x(self) {
	return self.length === 0;
}
function $y(self) {
	return __list_get(self, self.length - 1);
}
function $t(self, $u) {
	const $v = $u;
	let $w = null;
	if ($v[0] === 0) {
		const turn = $v[1];
		$w = enqueue(turn, self[1].v);
	} else {
		const $z = $y(draining_turns.v);
		let $A = null;
		if ($z[0] === 0) {
			const draining = $z[1];
			$A = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$A = undefined;
		}
		$w = $A;
	}
	return $w;
}
function $r(self, value, $s) {
	self[0].v = value;
	$t(self, $s);
}
function $E(owner2, body) {
	return body(owner2);
}
function $G(body) {
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
$r(count, 2, [ 1 ]);
dispose2(owner);
$r(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($B) => {
	(($C) => {
		$d(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $C);
		return;
	})(inner);
	$d(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $B);
	return;
})(outer);
$r(count, 4, [ 1 ]);
dispose2(inner);
$r(count, 5, [ 1 ]);
dispose2(outer);
$r(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$E(wrapped, ($D) => {
	$d(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $D);
	return;
});
$r(count, 7, [ 1 ]);
dispose2(wrapped);
$r(count, 8, [ 1 ]);
console.log("fin");
const $H = $G(($F) => {
	$d(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $F);
	return "built";
});
const label = $H[0];
const scope = $H[1];
console.log(label);
$r(count, 9, [ 1 ]);
dispose2(scope);
$r(count, 10, [ 1 ]);
console.log("post");
