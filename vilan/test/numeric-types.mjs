function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $d = null;
	if (wrapped < 0) {
		$d = wrapped + modulus;
	} else {
		$d = wrapped;
	}
	return $d;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $e = null;
	if (wrapped >= half) {
		$e = wrapped - modulus;
	} else {
		$e = wrapped;
	}
	return $e;
}
function max_value() {
	return 127;
}
function min_value() {
	return -(128);
}
function max_value2() {
	return 255;
}
function min_value2() {
	return 0;
}
function max_value3() {
	return 32767;
}
function min_value3() {
	return -(32768);
}
function max_value4() {
	return 65535;
}
function max_value5() {
	return 2147483647;
}
function min_value4() {
	return -(2147483648);
}
function as_i8(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 256, 128));
}
function as_u8(self) {
	const widened = Number(self);
	return Number(fold_unsigned(widened, 256));
}
function as_u16(self) {
	const widened = Number(self);
	return Number(fold_unsigned(widened, 65536));
}
function max_value6() {
	return 4294967295;
}
function max_value7() {
	return 9007199254740992;
}
function min_value5() {
	return -(9007199254740992);
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function max_value8() {
	return 9007199254740992;
}
function min_value6() {
	return 0;
}
function as_i53(self) {
	const widened = Number(self);
	return Number(Math.trunc(widened));
}
function as_i322(self) {
	const widened = self;
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
function div(self, b) {
	return Math.trunc(self / b);
}
function to_json(self) {
	return "{\"kind\":" + JSON.stringify(self[0]) + "," + "\"sequence\":" + JSON.stringify(self[1]) + "," + "\"stamp\":" + JSON.stringify(self[2]) + "}";
}
function $a(value, divisor) {
	return div(value, divisor);
}
function $b(value, divisor) {
	return Math.trunc(value / divisor);
}
function $c(value, divisor) {
	return Math.trunc(value / divisor);
}
const byte = 0xFF;
const short = 60000;
const wide = 9007199254740992;
const ratio = 2.5;
console.log(byte);
console.log(short);
console.log(wide);
console.log(ratio);
console.log(Math.trunc(7 / 2));
console.log(Math.trunc(-(7) / 2));
console.log(Math.trunc(7 / 2));
console.log(Math.trunc(100 / 3));
console.log(7.0 / 2.0);
console.log(7n / 2n);
let counter = 9;
counter = Math.trunc(counter / 2);
console.log(counter);
console.log($a(100, 8));
console.log($b(7, 2));
console.log($c(9, 4));
console.log(as_u8(300));
console.log(as_u8(-(1)));
console.log(as_i8(130));
console.log(as_i322(3.9));
console.log(as_i322(-(3.9)));
console.log(as_u16(70000));
console.log(Number(byte) + 0.25);
console.log(as_i32(wide));
console.log(as_i53(2.5));
const doubled = 100 + 100;
console.log(doubled);
console.log(100 * 3);
console.log(5 < 6);
console.log(max_value());
console.log(min_value());
console.log(max_value2());
console.log(min_value2());
console.log(max_value3());
console.log(min_value3());
console.log(max_value4());
console.log(max_value5());
console.log(min_value4());
console.log(max_value6());
console.log(max_value7());
console.log(min_value5());
console.log(max_value8());
console.log(min_value6());
console.log(JSON.stringify(200));
const packet = [ 7, 300, 5 ];
console.log(to_json(packet));
