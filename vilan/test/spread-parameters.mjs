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
		while (!($q(turn[0].v)) && budget > 0) {
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
function middle(items) {
	return items[1];
}
function swap_pack(items) {
	items = __clone(items);
	items = [ 8, 9 ];
	return items[0];
}
function forward(items) {
	return items[0];
}
function outer(items) {
	return forward(items);
}
function $a(items) {
	return 1;
}
function $b(items) {
	return 1;
}
function $c(head, rest) {
	return head;
}
function $d(items) {
	return 1;
}
function $e(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $f(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $i(self) {
	return self[0].v;
}
function $j(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $q(self) {
	return self.length === 0;
}
function $r(self) {
	return __list_get(self, self.length - 1);
}
function $m(self, $n) {
	const $o = $n;
	let $p = null;
	if ($o[0] === 0) {
		const turn = $o[1];
		$p = enqueue(turn, self[1].v);
	} else {
		const $s = $r(draining_turns.v);
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
		$p = $t;
	}
	return $p;
}
function $k(self, value, $l) {
	self[0].v = value;
	$m(self, $l);
}
function $u(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($i(self));
		return;
	} ]);
	observer($i(self));
	return [ self[1], id, __shared_new([ 1 ]) ];
}
function $g(sources, $h) {
	const snapshot = () => {
		return sources.map((source) => {
			return $i(source);
		});
	};
	const derived = $j(snapshot());
	sources.map((source) => {
		return $u(source, (_) => {
			$k(derived, snapshot(), $h);
			return;
		});
	});
	return derived;
}
function $v(self) {
	return self[0].v;
}
function $x(self, $n) {
	const $y = $n;
	let $z = null;
	if ($y[0] === 0) {
		const turn = $y[1];
		$z = enqueue(turn, self[1].v);
	} else {
		const $A = $r(draining_turns.v);
		let $B = null;
		if ($A[0] === 0) {
			const draining = $A[1];
			$B = enqueue(draining, self[1].v);
		} else {
			for (const subscriber of self[1].v) {
				subscriber[1]();
			}
			$B = undefined;
		}
		$z = $B;
	}
	return $z;
}
function $w(self, value, $l) {
	self[0].v = value;
	$x(self, $l);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
console.log(middle([ 4, 5, 6 ]));
console.log($a([  ]));
console.log($b([ 1 ]));
console.log($c(2, [ 3, 4 ]));
console.log(swap_pack([ 1, 2 ]));
console.log(outer([ 3, 4 ]));
const inner = [ 10, 11 ];
console.log($d([ ...inner, 12 ]));
const count = $e(20);
const name = $f("hi");
const both = $g([ __clone(count), name ], [ 1 ]);
console.log($v(both)[0]);
$w(count, 21, [ 1 ]);
console.log($v(both)[0]);
console.log($v(both)[1]);
