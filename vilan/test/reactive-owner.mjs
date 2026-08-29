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
		while (!($w(turn[0].v)) && budget > 0) {
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
function dispose(self, $l) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $m = $l;
	let $n = null;
	if ($m[0] === 0) {
		const turn = $m[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$n = undefined;
	} else {
		$n = undefined;
	}
	$n;
	const $o = self[2].v;
	let $p = null;
	if ($o[0] === 0) {
		const release = $o[1];
		self[2].v = [ 1 ];
		release();
		$p = undefined;
	} else {
		$p = undefined;
	}
	return $p;
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
function get_owner($f) {
	return $f;
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $h(signal, observer) {
	const id = fresh_id();
	const cell = signal[0];
	signal[1].v.push([ id, () => {
		observer(cell.v);
		return;
	} ]);
	return [ signal[1], id, __shared_new([ 1 ]) ];
}
function $i(self) {
	return self[0].v;
}
function $g(self, observer) {
	const subscription = $h(self, observer);
	observer($i(self));
	return subscription;
}
function $j(self, item, $k) {
	self[0].v.push(() => {
		dispose(item, $k);
		return;
	});
	return __clone(item);
}
function $c(self, observer, $d, $e) {
	$j(get_owner($e), $g(self, observer), $d);
}
function $w(self) {
	return self.length === 0;
}
function $x(self) {
	return __list_get(self, self.length - 1);
}
function $s(self, $t) {
	const $u = $t;
	let $v = null;
	if ($u[0] === 0) {
		const turn = $u[1];
		$v = enqueue(turn, self[1].v);
	} else {
		const $y = $x(draining_turns.v);
		let $z = null;
		if ($y[0] === 0) {
			const draining = $y[1];
			$z = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$z = undefined;
		}
		$v = $z;
	}
	return $v;
}
function $q(self, value, $r) {
	self[0].v = value;
	$s(self, $r);
}
function $D(owner2, body) {
	return body(owner2);
}
function $F(body) {
	const scope2 = new2();
	const result = body(scope2);
	return [ result, scope2 ];
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const count = $a(1);
const owner = new2();
(($b) => {
	$c(count, (value) => {
		return console.log("seen " + value);
	}, [ 1 ], $b);
	return;
})(owner);
$q(count, 2, [ 1 ]);
dispose2(owner);
$q(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($A) => {
	(($B) => {
		$c(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $B);
		return;
	})(inner);
	$c(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $A);
	return;
})(outer);
$q(count, 4, [ 1 ]);
dispose2(inner);
$q(count, 5, [ 1 ]);
dispose2(outer);
$q(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$D(wrapped, ($C) => {
	$c(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $C);
	return;
});
$q(count, 7, [ 1 ]);
dispose2(wrapped);
$q(count, 8, [ 1 ]);
console.log("fin");
const $G = $F(($E) => {
	$c(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $E);
	return "built";
});
const label = $G[0];
const scope = $G[1];
console.log(label);
$q(count, 9, [ 1 ]);
dispose2(scope);
$q(count, 10, [ 1 ]);
console.log("post");
