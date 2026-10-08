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
function halve(value, divisor) {
	return div(value, divisor);
}
function halve2(value, divisor) {
	return Math.trunc(value / divisor);
}
const byte = 0xFF;
const short = 60000;
const wide = 9007199254740992;
const ratio = 2.5;
console.log(String(byte));
console.log(String(short));
console.log(String(wide));
console.log(String(ratio));
console.log(String(Math.trunc(7 / 2)));
console.log(String(Math.trunc(-(7) / 2)));
console.log(String(Math.trunc(7 / 2)));
console.log(String(Math.trunc(100 / 3)));
console.log(String(7.0 / 2.0));
console.log(7n / 2n);
let counter = 9;
counter = Math.trunc(counter / 2);
console.log(String(counter));
console.log(String(halve(100, 8)));
console.log(String(halve2(7, 2)));
console.log(String(halve2(9, 4)));
console.log(String(as_u8(300)));
console.log(String(as_u8(-(1))));
console.log(String(as_i8(130)));
console.log(String(as_i322(3.9)));
console.log(String(as_i322(-(3.9))));
console.log(String(as_u16(70000)));
console.log(String(Number(byte) + 0.25));
console.log(String(as_i32(wide)));
console.log(String(as_i53(2.5)));
const doubled = 100 + 100;
console.log(String(doubled));
console.log(String(100 * 3));
console.log(5 < 6);
console.log(String(max_value()));
console.log(String(min_value()));
console.log(String(max_value2()));
console.log(String(min_value2()));
console.log(String(max_value3()));
console.log(String(min_value3()));
console.log(String(max_value4()));
console.log(String(max_value5()));
console.log(String(min_value4()));
console.log(String(max_value6()));
console.log(String(max_value7()));
console.log(String(min_value5()));
console.log(String(max_value8()));
console.log(String(min_value6()));
console.log(JSON.stringify(200));
const packet = [ 7, 300, 5 ];
console.log(to_json(packet));
