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
		const $g = step;
		let $h = null;
		if ($g[0] === 0) {
			const index = $g[1];
			$h = "K" + index + " ";
		} else if ($g[0] === 1) {
			const index2 = $g[1];
			$h = "R" + index2 + " ";
		} else {
			$h = "F ";
		}
		const rendered = $h;
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
		next_same.push(-(1));
	}
	let build = held - 1;
	while (build >= 0) {
		const canonical = hash(__at(old_keys, build));
		const $b = __map_get(first, canonical);
		let $c = null;
		if ($b[0] === 0) {
			const after = $b[1];
			$c = __at_put(next_same, build, after);
		} else {
			$c = undefined;
		}
		$c;
		first.set(canonical, build);
		build = build - 1;
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash(item_key);
		const $d = __map_get(first, canonical2);
		let $e = null;
		if ($d[0] === 0) {
			const at = $d[1];
			$e = at;
		} else {
			$e = -(1);
		}
		let head = $e;
		while (head >= 0) {
			if (!(__at(claimed, head))) {
				break;
			}
			head = __at(next_same, head);
		}
		first.set(canonical2, head);
		let found = -(1);
		let walk = head;
		while (walk >= 0) {
			if (!(__at(claimed, walk)) && __at(old_keys, walk) === item_key) {
				found = walk;
				break;
			}
			walk = __at(next_same, walk);
		}
		let step = [ 2 ];
		if (found >= 0) {
			__at_put(claimed, found, true);
			let $f = null;
			if (same(__at(old_items, found), item)) {
				$f = [ 0, found ];
			} else {
				$f = [ 1, found ];
			}
			step = $f;
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
