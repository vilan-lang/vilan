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
		while (!($v(turn[0].v)) && budget > 0) {
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
function dispose(self, $k) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(__clone(subscriber));
		}
	}
	self[0].v = kept;
	const $l = $k;
	let $m = null;
	if ($l[0] === 0) {
		const turn = $l[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(__clone(subscriber2));
			}
		}
		turn[0].v = kept_pending;
		$m = undefined;
	} else {
		$m = undefined;
	}
	$m;
	const $n = self[2].v;
	let $o = null;
	if ($n[0] === 0) {
		const release = $n[1];
		self[2].v = [ 1 ];
		release();
		$o = undefined;
	} else {
		$o = undefined;
	}
	return $o;
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
function $h(self) {
	return self[0].v;
}
function $g(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($h(self));
		return;
	} ]);
	observer($h(self));
	return [ self[1], id, __shared_new([ 1 ]) ];
}
function $i(self, item, $j) {
	self[0].v.push(() => {
		dispose(item, $j);
		return;
	});
	return __clone(item);
}
function $c(self, observer, $d, $e) {
	$i(get_owner($e), $g(self, observer), $d);
}
function $v(self) {
	return self.length === 0;
}
function $w(self) {
	return __list_get(self, self.length - 1);
}
function $r(self, $s) {
	const $t = $s;
	let $u = null;
	if ($t[0] === 0) {
		const turn = $t[1];
		$u = enqueue(turn, self[1].v);
	} else {
		const $x = $w(draining_turns.v);
		let $y = null;
		if ($x[0] === 0) {
			const draining = $x[1];
			$y = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$y = undefined;
		}
		$u = $y;
	}
	return $u;
}
function $p(self, value, $q) {
	self[0].v = value;
	$r(self, $q);
}
function $C(owner2, body) {
	return body(owner2);
}
function $E(body) {
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
$p(count, 2, [ 1 ]);
dispose2(owner);
$p(count, 3, [ 1 ]);
console.log("done");
const outer = new2();
const inner = new2();
(($z) => {
	(($A) => {
		$c(count, (value) => {
			return console.log("inner " + value);
		}, [ 1 ], $A);
		return;
	})(inner);
	$c(count, (value) => {
		return console.log("outer " + value);
	}, [ 1 ], $z);
	return;
})(outer);
$p(count, 4, [ 1 ]);
dispose2(inner);
$p(count, 5, [ 1 ]);
dispose2(outer);
$p(count, 6, [ 1 ]);
console.log("end");
const wrapped = new2();
$C(wrapped, ($B) => {
	$c(count, (value) => {
		return console.log("wrapped " + value);
	}, [ 1 ], $B);
	return;
});
$p(count, 7, [ 1 ]);
dispose2(wrapped);
$p(count, 8, [ 1 ]);
console.log("fin");
const $F = $E(($D) => {
	$c(count, (value) => {
		return console.log("comp " + value);
	}, [ 1 ], $D);
	return "built";
});
const label = $F[0];
const scope = $F[1];
console.log(label);
$p(count, 9, [ 1 ]);
dispose2(scope);
$p(count, 10, [ 1 ]);
console.log("post");
