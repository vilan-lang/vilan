function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __at_put(list, index, value, location) {
	if (index >= 0 && index < list.length) return list[index] = value;
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
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
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function hash(self) {
	return __hash(self);
}
function show(plan) {
	let out = "";
	for (const step of plan[0]) {
		const $i = step;
		let $j = null;
		if ($i[0] === 0) {
			const index = $i[1];
			$j = "K" + index + " ";
		} else if ($i[0] === 1) {
			const index2 = $i[1];
			$j = "R" + index2 + " ";
		} else {
			$j = "F ";
		}
		const rendered = $j;
		out = out + rendered;
	}
	out = out + "| removed:";
	for (const index3 of plan[1]) {
		out = out + (" " + index3);
	}
	console.log(out);
}
function reconcile(old_keys, old_items, items, key_of, same) {
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
		const canonical = hash(__at(old_keys, build, "std/src/reactive.vl:3683:19"));
		__at_put(next_same, build, __map_get(first, canonical), "std/src/reactive.vl:3684:3");
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $a = head;
			let $b = null;
			if ($a[0] === 0) {
				const at = $a[1];
				if (__at(claimed, at, "std/src/reactive.vl:3700:9")) {
					head = __at(next_same, at, "std/src/reactive.vl:3701:14");
				} else {
					advancing = false;
				}
				$b = undefined;
			} else {
				$b = advancing = false;
			}
			$b;
		}
		const $c = head;
		let $d = null;
		if ($c[0] === 0) {
			const at2 = $c[1];
			$d = first.set(canonical2, at2);
		} else {
			$d = first.delete(canonical2);
		}
		$d;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $e = walk;
			let $f = null;
			if ($e[0] === 0) {
				const at3 = $e[1];
				if (!(__at(claimed, at3, "std/src/reactive.vl:3719:10")) && __at(old_keys, at3, "std/src/reactive.vl:3719:25") === item_key) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3, "std/src/reactive.vl:3726:14");
				}
				$f = undefined;
			} else {
				$f = walking = false;
			}
			$f;
		}
		let step = [ 2 ];
		const $g = found;
		if ($g[0] === 0) {
			__at_put(claimed, $g[1], true, "std/src/reactive.vl:3734:4");
			let $h = null;
			if (same(__at(old_items, $g[1], "std/src/reactive.vl:3735:19"), item)) {
				$h = [ 0, $g[1] ];
			} else {
				$h = [ 1, $g[1] ];
			}
			step = $h;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index = 0;
	while (index < held) {
		if (!(__at(claimed, index, "std/src/reactive.vl:3742:7"))) {
			removed.push(index);
		}
		index = index + 1;
	}
	return [ steps, removed ];
}
show(reconcile([ 1, 2, 3 ], [ 10, 20, 30 ], [ 30, 10, 20 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show(reconcile([ 1, 2 ], [ 10, 20 ], [ 10, 21, 35 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show(reconcile([ 1, 2, 3 ], [ 10, 20, 30 ], [ 30 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show(reconcile([ 1 ], [ 10 ], [ 10, 10 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show(reconcile([  ], [  ], [ 10 ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show(reconcile([ 1 ], [ 10 ], [  ], (item) => {
	return Math.trunc(item / 10);
}, (a, b) => {
	return a === b;
}));
show(reconcile([ 1, 2 ], [ 10, 20 ], [ 10, 21, 35 ], (item) => {
	return Math.trunc(item / 10);
}, (_a, _b) => {
	return true;
}));
