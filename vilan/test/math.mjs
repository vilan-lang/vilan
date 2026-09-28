function __clone(value) {
	if (Array.isArray(value)) return value.map(__clone);
	if (value instanceof Set) return new Set([ ...value ].map(__clone));
	if (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));
	return value;
}
function compare(self, b) {
	let $d = null;
	if (self < b) {
		$d = -1;
	} else {
		let $e = null;
		if (self > b) {
			$e = 1;
		} else {
			$e = 0;
		}
		$d = $e;
	}
	return $d;
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
function $a(a, b) {
	return Math.min(a, b);
}
function $c(self, b) {
	let $f = null;
	if (compare(self, b) >= 0) {
		$f = self;
	} else {
		$f = b;
	}
	return $f;
}
function $b(a, b) {
	return $c(a, b);
}
function $g(a, b) {
	let $h = null;
	if (a <= b) {
		$h = [ __clone(a), __clone(b) ];
	} else {
		$h = [ __clone(b), __clone(a) ];
	}
	return $h;
}
const PI = 3.141592653589793;
const TAU = 6.283185307179586;
const E = 2.718281828459045;
const EPSILON = Math.pow(2, 0 - 52);
const INFINITY = 1 / 0;
const NAN = 0 / 0;
console.log(PI);
console.log(TAU === PI * 2);
console.log(E > 2.718 && E < 2.719);
console.log(EPSILON === Math.pow(2, 0 - 52));
console.log(INFINITY > 0 && is_infinite(INFINITY));
console.log(is_nan(NAN));
console.log($a(3, 9));
console.log($b("ant", "bee"));
const $i = $g(9, 3);
const low = $i[0];
const high = $i[1];
console.log("" + low + " " + high);
console.log(Math.sin(0));
console.log(Math.cos(0));
console.log(Math.atan2(0, 1));
console.log(to_radians(180) === PI);
console.log(to_degrees(PI));
console.log(Math.exp(0));
console.log(Math.log(1));
console.log(Math.log2(8));
console.log(Math.log10(1000));
console.log(Math.cbrt(27));
console.log(Math.hypot(3, 4));
console.log(Math.sign(0 - 5));
console.log(fract(1.5));
console.log(lerp(0, 10, 0.5));
console.log(rem3(7.5, 2));
console.log(Number.isFinite(1.5));
console.log(Number.isFinite(INFINITY));
console.log(Math.abs(0 - 5));
console.log(Math.pow(3, 2));
console.log(Math.min(200, 90));
console.log(Math.max(7, 9));
console.log(rem2(7, 3));
console.log(rem2(0 - 7, 3));
console.log(rem4(250, 7));
console.log(rem5(9, 4));
console.log(rem(3000000000, 7));
console.log(Math.pow(2, 3));
console.log(Math.sqrt(2.25));
console.log(Math.abs(0 - 1.5));
