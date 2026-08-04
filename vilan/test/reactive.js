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
			turn[0].v.push(subscriber);
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
		draining_turns.v.push(turn);
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
function dispose(self, $r) {
	let kept = [  ];
	for (const subscriber of self[0].v) {
		if (subscriber[0] !== self[1]) {
			kept.push(subscriber);
		}
	}
	self[0].v = kept;
	const $s = $r;
	let $t = null;
	if ($s[0] === 0) {
		const turn = $s[1];
		let kept_pending = [  ];
		for (const subscriber2 of turn[0].v) {
			if (subscriber2[0] !== self[1]) {
				kept_pending.push(subscriber2);
			}
		}
		turn[0].v = kept_pending;
		$t = undefined;
	} else {
		$t = undefined;
	}
	return $t;
}
function new2() {
	return [ __shared_new([  ]) ];
}
function $a(value) {
	let subscribers = [  ];
	return [ __shared_new(value), __shared_new(subscribers) ];
}
function $d(self) {
	return self[0].v;
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
function $b(self, transform, $c) {
	const derived = $a(transform($d(self)));
	self[1].v.push([ fresh_id(), () => {
		$e(derived, transform($d(self)), $c);
		return;
	} ]);
	return derived;
}
function $o(self, observer) {
	const id = fresh_id();
	self[1].v.push([ id, () => {
		observer($d(self));
		return;
	} ]);
	observer($d(self));
	return [ self[1], id ];
}
function $p(self, item, $q) {
	self[0].v.push(() => {
		dispose(item, $q);
		return;
	});
	return item;
}
function $u(self, transform, $v) {
	$e(self, transform($d(self)), $v);
}
const next_subscriber_id = __shared_new(0);
const draining_turns = __shared_new([  ]);
const owner = new2();
const count = $a(0);
const doubled = $b(count, (n) => {
	return n * 2;
}, [ 1 ]);
$p(owner, $o(doubled, (n) => {
	return console.log(n);
}), [ 1 ]);
$e(count, 1, [ 1 ]);
$u(count, (n) => {
	return n + 4;
}, [ 1 ]);
console.log($d(doubled));
$p(owner, $o(count, (n) => {
	return console.log(n);
}), [ 1 ]);
$e(count, 20, [ 1 ]);
