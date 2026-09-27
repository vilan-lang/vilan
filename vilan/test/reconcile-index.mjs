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
function __shared_new(value) {
	return { v: value };
}
function hash(self) {
	return __hash(self);
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $n = null;
	if (wrapped < 0) {
		$n = wrapped + modulus;
	} else {
		$n = wrapped;
	}
	return $n;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $o = null;
	if (wrapped >= half) {
		$o = wrapped - modulus;
	} else {
		$o = wrapped;
	}
	return $o;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function next_random(bound) {
	const state = seed.v * 16807 % 2147483647;
	seed.v = state;
	return as_i32(state % as_i53(bound));
}
function render_step(step) {
	const $l = step;
	let $m = null;
	if ($l[0] === 0) {
		const index = $l[1];
		$m = "K" + index + " ";
	} else if ($l[0] === 1) {
		const index2 = $l[1];
		$m = "R" + index2 + " ";
	} else {
		$m = "F ";
	}
	return $m;
}
function render(plan) {
	let out = "";
	for (const step of plan[0]) {
		out = out + render_step(step);
	}
	out = out + "| ";
	for (const index of plan[1]) {
		out = out + ("" + index + " ");
	}
	return out;
}
function compare_int(label, old4, new_items) {
	const indexed = $a(old4, old4, new_items, (item) => {
		return item;
	}, (before, after) => {
		return before === after;
	});
	const scanned = $j(old4, old4, new_items, (item) => {
		return item;
	}, (before, after) => {
		return before === after;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw "" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]";
		})();
	}
}
function eq(self, other) {
	return self[0] === other[0];
}
function hash2(self) {
	return hash(self[0]);
}
function eq2(self, other) {
	return self[0] === other[0] && self[1] === other[1];
}
function hash3(self) {
	return hash(self[0]);
}
function compare_tags(label, old4, new_items) {
	const indexed = $p(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	const scanned = $y(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw "" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]";
		})();
	}
}
function compare_pairs(label, old4, new_items) {
	const indexed = $A(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	const scanned = $J(old4, old4, new_items, (item) => {
		return __clone(item);
	}, (_before, _after) => {
		return true;
	});
	cases.v = cases.v + 1;
	if (render(indexed) !== render(scanned)) {
		(() => {
			throw "" + label + ": indexed=[" + render(indexed) + "] scanned=[" + render(scanned) + "]";
		})();
	}
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
function $j(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		let step = [ 2 ];
		let index = 0;
		while (index < old_keys.length) {
			if (!(__at(claimed, index)) && __at(old_keys, index) === item_key) {
				__at_put(claimed, index, true);
				let $k = null;
				if (same(__at(old_items, index), item)) {
					$k = [ 0, index ];
				} else {
					$k = [ 1, index ];
				}
				step = $k;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function $p(old_keys, old_items, items, key_of, same) {
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
		const canonical = hash2(__at(old_keys, build));
		__at_put(next_same, build, __map_get(first, canonical));
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash2(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $q = head;
			let $r = null;
			if ($q[0] === 0) {
				const at = $q[1];
				if (__at(claimed, at)) {
					head = __at(next_same, at);
				} else {
					advancing = false;
				}
				$r = undefined;
			} else {
				$r = advancing = false;
			}
			$r;
		}
		const $s = head;
		let $t = null;
		if ($s[0] === 0) {
			const at2 = $s[1];
			$t = first.set(canonical2, at2);
		} else {
			$t = first.delete(canonical2);
		}
		$t;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $u = walk;
			let $v = null;
			if ($u[0] === 0) {
				const at3 = $u[1];
				if (!(__at(claimed, at3)) && eq(__at(old_keys, at3), item_key)) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3);
				}
				$v = undefined;
			} else {
				$v = walking = false;
			}
			$v;
		}
		let step = [ 2 ];
		const $w = found;
		if ($w[0] === 0) {
			__at_put(claimed, $w[1], true);
			let $x = null;
			if (same(__at(old_items, $w[1]), item)) {
				$x = [ 0, $w[1] ];
			} else {
				$x = [ 1, $w[1] ];
			}
			step = $x;
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
function $y(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		let step = [ 2 ];
		let index = 0;
		while (index < old_keys.length) {
			if (!(__at(claimed, index)) && eq(__at(old_keys, index), item_key)) {
				__at_put(claimed, index, true);
				let $z = null;
				if (same(__at(old_items, index), item)) {
					$z = [ 0, index ];
				} else {
					$z = [ 1, index ];
				}
				step = $z;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
function $A(old_keys, old_items, items, key_of, same) {
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
		const canonical = hash3(__at(old_keys, build));
		__at_put(next_same, build, __map_get(first, canonical));
		first.set(canonical, build);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		const canonical2 = hash3(item_key);
		let head = __map_get(first, canonical2);
		let advancing = true;
		while (advancing) {
			const $B = head;
			let $C = null;
			if ($B[0] === 0) {
				const at = $B[1];
				if (__at(claimed, at)) {
					head = __at(next_same, at);
				} else {
					advancing = false;
				}
				$C = undefined;
			} else {
				$C = advancing = false;
			}
			$C;
		}
		const $D = head;
		let $E = null;
		if ($D[0] === 0) {
			const at2 = $D[1];
			$E = first.set(canonical2, at2);
		} else {
			$E = first.delete(canonical2);
		}
		$E;
		let found = [ 1 ];
		let walk = head;
		let walking = true;
		while (walking) {
			const $F = walk;
			let $G = null;
			if ($F[0] === 0) {
				const at3 = $F[1];
				if (!(__at(claimed, at3)) && eq2(__at(old_keys, at3), item_key)) {
					found = [ 0, at3 ];
					walking = false;
				} else {
					walk = __at(next_same, at3);
				}
				$G = undefined;
			} else {
				$G = walking = false;
			}
			$G;
		}
		let step = [ 2 ];
		const $H = found;
		if ($H[0] === 0) {
			__at_put(claimed, $H[1], true);
			let $I = null;
			if (same(__at(old_items, $H[1]), item)) {
				$I = [ 0, $H[1] ];
			} else {
				$I = [ 1, $H[1] ];
			}
			step = $I;
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
function $J(old_keys, old_items, items, key_of, same) {
	let claimed = [  ];
	for (const _ of old_keys) {
		claimed.push(false);
	}
	let steps = [  ];
	for (const item of items) {
		const item_key = key_of(item);
		let step = [ 2 ];
		let index = 0;
		while (index < old_keys.length) {
			if (!(__at(claimed, index)) && eq2(__at(old_keys, index), item_key)) {
				__at_put(claimed, index, true);
				let $K = null;
				if (same(__at(old_items, index), item)) {
					$K = [ 0, index ];
				} else {
					$K = [ 1, index ];
				}
				step = $K;
				break;
			}
			index = index + 1;
		}
		steps.push(step);
	}
	let removed = [  ];
	let index2 = 0;
	while (index2 < old_keys.length) {
		if (!(__at(claimed, index2))) {
			removed.push(index2);
		}
		index2 = index2 + 1;
	}
	return [ steps, removed ];
}
const seed = __shared_new(7);
const cases = __shared_new(0);
compare_int("empty->empty", [  ], [  ]);
compare_int("empty->three", [  ], [ 1, 2, 3 ]);
compare_int("three->empty", [ 1, 2, 3 ], [  ]);
compare_int("append", [ 1, 2, 3 ], [ 1, 2, 3, 4 ]);
compare_int("prepend", [ 1, 2, 3 ], [ 0, 1, 2, 3 ]);
compare_int("remove-front", [ 1, 2, 3 ], [ 2, 3 ]);
compare_int("remove-back", [ 1, 2, 3 ], [ 1, 2 ]);
compare_int("remove-middle", [ 1, 2, 3 ], [ 1, 3 ]);
compare_int("reverse", [ 1, 2, 3, 4 ], [ 4, 3, 2, 1 ]);
compare_int("swap", [ 1, 2, 3 ], [ 1, 3, 2 ]);
compare_int("dup-old", [ 1, 1, 2 ], [ 1, 2 ]);
compare_int("dup-new", [ 1, 2 ], [ 1, 1, 2 ]);
compare_int("dup-both", [ 1, 1, 2, 2 ], [ 2, 1, 2, 1 ]);
compare_int("all-fresh", [ 1, 2, 3 ], [ 7, 8, 9 ]);
compare_int("dup-only", [ 5, 5, 5 ], [ 5, 5, 5, 5 ]);
let round = 0;
while (round < 600) {
	const old_length = next_random(9);
	const new_length = next_random(9);
	const alphabet = 1 + next_random(5);
	let old = [  ];
	let fill = 0;
	while (fill < old_length) {
		old.push(next_random(alphabet));
		fill = fill + 1;
	}
	let fresh = [  ];
	fill = 0;
	while (fill < new_length) {
		fresh.push(next_random(alphabet));
		fill = fill + 1;
	}
	compare_int("random " + round, old, fresh);
	round = round + 1;
}
round = 0;
while (round < 400) {
	const old_length2 = next_random(7);
	const new_length2 = next_random(7);
	let old2 = [  ];
	let fill2 = 0;
	while (fill2 < old_length2) {
		old2.push([ next_random(3), next_random(50) ]);
		fill2 = fill2 + 1;
	}
	let fresh2 = [  ];
	fill2 = 0;
	while (fill2 < new_length2) {
		fresh2.push([ next_random(3), next_random(50) ]);
		fill2 = fill2 + 1;
	}
	compare_tags("tags " + round, old2, fresh2);
	round = round + 1;
}
round = 0;
while (round < 400) {
	const old_length3 = next_random(7);
	const new_length3 = next_random(7);
	let old3 = [  ];
	let fill3 = 0;
	while (fill3 < old_length3) {
		old3.push([ next_random(3), next_random(4) ]);
		fill3 = fill3 + 1;
	}
	let fresh3 = [  ];
	fill3 = 0;
	while (fill3 < new_length3) {
		fresh3.push([ next_random(3), next_random(4) ]);
		fill3 = fill3 + 1;
	}
	compare_pairs("pairs " + round, old3, fresh3);
	round = round + 1;
}
console.log("cases=" + cases.v + " differences=0");
