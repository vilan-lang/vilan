function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function compare(self, b) {
	let $a = null;
	if (self < b) {
		$a = -1;
	} else {
		let $b = null;
		if (self > b) {
			$b = 1;
		} else {
			$b = 0;
		}
		$a = $b;
	}
	return $a;
}
function rem(self, m) {
	return self - Math.trunc(self / m) * m;
}
function rem2(self, m) {
	return self - Math.trunc(self / m) * m;
}
function fract(self) {
	return self - Math.trunc(self);
}
function lerp(self, to, t) {
	return self + (to - self) * t;
}
function to_radians(self) {
	return self * (PI / 180);
}
function to_degrees(self) {
	return self * (180 / PI);
}
function rem3(self, m) {
	return self % m;
}
function is_nan(self) {
	return self !== self;
}
function is_infinite(self) {
	return !(Number.isFinite(self)) && !(is_nan(self));
}
function rem4(self, m) {
	return self - Math.trunc(self / m) * m;
}
function rem5(self, m) {
	return self - Math.trunc(self / m) * m;
}
function min(a, b) {
	return Math.min(a, b);
}
function max(self, b) {
	let $c = null;
	if (compare(self, b) >= 0) {
		$c = self;
	} else {
		$c = b;
	}
	return $c;
}
function max2(a, b) {
	return max(a, b);
}
function minmax(a, b) {
	let $d = null;
	if (a <= b) {
		$d = [ __clone(a), __clone(b) ];
	} else {
		$d = [ __clone(b), __clone(a) ];
	}
	return $d;
}
const PI = 3.141592653589793;
const TAU = 6.283185307179586;
const E = 2.718281828459045;
const EPSILON = Math.pow(2, 0 - 52);
const INFINITY = 1 / 0;
const NAN = 0 / 0;
console.log(String(PI));
console.log(TAU === PI * 2);
console.log(E > 2.718 && E < 2.719);
console.log(EPSILON === Math.pow(2, 0 - 52));
console.log(INFINITY > 0 && is_infinite(INFINITY));
console.log(is_nan(NAN));
console.log(String(min(3, 9)));
console.log(max2("ant", "bee"));
const $e = minmax(9, 3);
const low = $e[0];
const high = $e[1];
console.log("" + low + " " + high);
console.log(String(Math.sin(0)));
console.log(String(Math.cos(0)));
console.log(String(Math.atan2(0, 1)));
console.log(to_radians(180) === PI);
console.log(String(to_degrees(PI)));
console.log(String(Math.exp(0)));
console.log(String(Math.log(1)));
console.log(String(Math.log2(8)));
console.log(String(Math.log10(1000)));
console.log(String(Math.cbrt(27)));
console.log(String(Math.hypot(3, 4)));
console.log(String(Math.sign(0 - 5)));
console.log(String(fract(1.5)));
console.log(String(lerp(0, 10, 0.5)));
console.log(String(rem3(7.5, 2)));
console.log(Number.isFinite(1.5));
console.log(Number.isFinite(INFINITY));
console.log(String(Math.abs(0 - 5)));
console.log(String(Math.pow(3, 2)));
console.log(String(Math.min(200, 90)));
console.log(String(Math.max(7, 9)));
console.log(String(rem2(7, 3)));
console.log(String(rem2(0 - 7, 3)));
console.log(String(rem4(250, 7)));
console.log(String(rem5(9, 4)));
console.log(String(rem(3000000000, 7)));
console.log(String(Math.pow(2, 3)));
console.log(String(Math.sqrt(2.25)));
console.log(String(Math.abs(0 - 1.5)));
