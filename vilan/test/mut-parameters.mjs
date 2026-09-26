function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $a = null;
	if (wrapped < 0) {
		$a = wrapped + modulus;
	} else {
		$a = wrapped;
	}
	return $a;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $b = null;
	if (wrapped >= half) {
		$b = wrapped - modulus;
	} else {
		$b = wrapped;
	}
	return $b;
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function bump(x) {
	x = x + 1;
	return x;
}
function grow(xs) {
	xs = __clone(xs);
	xs.push(9);
	return as_i32(xs.length);
}
function poke(x) {
	x = [ x ];
	const cell = [ x, 0 ];
	cell[0][cell[1]] = 5;
	return x[0];
}
function with_x(self, value) {
	self = __clone(self);
	self[0] = value;
	return __clone(self);
}
console.log(bump(1));
let list = [ 1, 2 ];
console.log(grow(list));
console.log(list.length);
console.log(poke(1));
const original = [ 0 ];
const moved = with_x(original, 9);
console.log(moved[0]);
console.log(original[0]);
const shrink = (v) => {
	v = v - 1;
	return v;
};
console.log(shrink(3));
