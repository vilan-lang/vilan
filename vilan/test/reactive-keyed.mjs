function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __at_put(list, index, value) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
}
function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function __hash(value) {
	return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
}
function __map_get(map, key) {
	return map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];
}
function hash(self) {
	return __hash(self);
}
function show(plan) {
	let out = "";
	for (const step of plan[0]) {
		const $j = step;
		let $k = null;
		if ($j[0] === 0) {
			const index = $j[1];
			$k = "K" + index + " ";
		} else if ($j[0] === 1) {
			const index2 = $j[1];
			$k = "R" + index2 + " ";
		} else {
			$k = "F ";
		}
		const rendered = $k;
		out = out + rendered;
	}
	out = out + "| removed:";
	for (const index3 of plan[1]) {
		out = out + (" " + index3);
	}
	console.log(out);
}
function $a(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	const held = old_keys.length;
	let first = new Map();
	let next_same = [  ];
	for (const _ of old_keys) {
		next_same.push([ 1 ]);
	}
	let build = held;
	while (build > 0) {
		build = build - 1;
		const canonical = hash(__at(old_keys, build));
		__at_put(next_same, build, __map_get(first, canonical));
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $b = head;
			let $c = null;
			if ($b[0] === 0) {
				const at = $b[1];
				if (__at(claimed, at)) {
					head = __at(next_same, at);
				} else {
					advancing = false;
				}
				$c = undefined;
			} else {
				$c = advancing = false;
			}
			$c;
		}
		const $d = head;
		let $e = null;
		if ($d[0] === 0) {
			const at2 = $d[1];
			$e = first.set(canonical2, at2);
		} else {
			$e = first.delete(canonical2);
		}
		$e;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $f = walk;
			let $g = null;
			if ($f[0] === 0) {
				const at3 = $f[1];
				if (!(__at(claimed, at3)) && __at(old_keys, at3) === item_key) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3);
				}
				$g = undefined;
			} else {
				$g = walking = false;
			}
			$g;
		}
		let step = [ 2 ];
		const $h = found;
		if ($h[0] === 0) {
			__at_put(claimed, $h[1], true);
			let $i = null;
			if (same(__at(old_items, $h[1]), item)) {
				$i = [ 0, $h[1] ];
			} else {
				$i = [ 1, $h[1] ];
			}
			step = $i;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index = 0;
	while (index < held) {
		if (!(__at(claimed, index))) {
			removed.push(index);
		}
		index = index + 1;
	}
	return [ steps, removed ];
}
show($a([ 1, 2, 3 ], [ 10, 20, 30 ], [ 30, 10, 20 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show($a([ 1, 2 ], [ 10, 20 ], [ 10, 21, 35 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show($a([ 1, 2, 3 ], [ 10, 20, 30 ], [ 30 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show($a([ 1 ], [ 10 ], [ 10, 10 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show($a([  ], [  ], [ 10 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show($a([ 1 ], [ 10 ], [  ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show($a([ 1, 2 ], [ 10, 20 ], [ 10, 21, 35 ], (item) => {
	return Math.trunc(item / 10);
}, (_a, _b) => {
	return true;
}));
