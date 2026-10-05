function diff(self, other) {
	let $a = null;
	if (self > other) {
		$a = self - other;
	} else {
		$a = other - self;
	}
	return $a;
}
function clamp(self, min, max) {
	return Math.max(Math.min(self, max), min);
}
function is_even(self) {
	return (self & 1) === 0;
}
function is_odd(self) {
	return (self & 1) === 1;
}
function is_even2(self) {
	return (self & 1) >>> 0 === 0;
}
function is_odd2(self) {
	return (self & 1) >>> 0 === 1;
}
function clamp2(self, min, max) {
	return Math.max(Math.min(self, max), min);
}
function as_f32(self) {
	const widened = self;
	return Number(widened);
}
function parity() {
	console.log(is_even(6));
	console.log(is_odd(6));
	console.log(is_odd(7));
	console.log(is_even(0 - 3));
	console.log(is_even2(8));
	console.log(is_odd2(9));
}
function clamp3(self, min, max) {
	return Math.max(Math.min(self, max), min);
}
parity();
const n = -(5);
console.log(String(Math.abs(n)));
console.log(String(diff(n, 3)));
const b = 2;
console.log(String(Math.pow(b, 10)));
console.log(String(Math.min(b, 7)));
console.log(String(Math.max(b, 7)));
const x = 16;
console.log(String(Math.sqrt(x)));
const y = 3.7;
console.log(String(Math.floor(y)));
console.log(String(Math.ceil(y)));
console.log(String(Math.round(y)));
console.log(String(Math.min(y, 2)));
console.log(String(Math.max(y, 10)));
console.log(String(clamp3(b, 3, 7)));
console.log(String(clamp(y, 0, 3)));
console.log(String(clamp(y, 4, 10)));
console.log(String(clamp(y, 0, 10)));
console.log(String(clamp2(as_f32(16), as_f32(0), as_f32(4))));
