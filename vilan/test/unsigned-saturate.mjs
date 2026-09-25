function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $b = null;
	if (wrapped < 0) {
		$b = wrapped + modulus;
	} else {
		$b = wrapped;
	}
	return $b;
}
function saturate_unsigned(value) {
	const truncated = Math.trunc(value);
	let $a = null;
	if (truncated > 0) {
		$a = truncated;
	} else {
		$a = 0;
	}
	return $a;
}
function as_u53(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_usize(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_u532(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_usize2(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_u8(self) {
	const widened = Number(self);
	return Number(fold_unsigned(widened, 256));
}
function as_u16(self) {
	const widened = Number(self);
	return Number(fold_unsigned(widened, 65536));
}
function as_u32(self) {
	const widened = Number(self);
	return Number(fold_unsigned(widened, 4294967296));
}
function as_u533(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_usize3(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_u534(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_usize4(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_u535(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_usize5(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_u536(self) {
	const widened = self;
	return Number(saturate_unsigned(widened));
}
function as_usize6(self) {
	const widened = self;
	return Number(saturate_unsigned(widened));
}
function as_u537(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
function as_usize7(self) {
	const widened = Number(self);
	return Number(saturate_unsigned(widened));
}
const small = -(5);
const middle = -(300);
const word = -(1);
const wide = -(9007199254740000);
const single = -(2.5);
const double = -(7.9);
const big = -(3n);
console.log("i8 " + as_u53(small) + " " + as_usize(small));
console.log("i16 " + as_u532(middle) + " " + as_usize2(middle));
console.log("i32 " + as_u533(word) + " " + as_usize3(word));
console.log("i53 " + as_u534(wide) + " " + as_usize4(wide));
console.log("f32 " + as_u535(single) + " " + as_usize5(single));
console.log("f64 " + as_u536(double) + " " + as_usize6(double));
console.log("BigInt " + as_u537(big) + " " + as_usize7(big));
const fraction = -(0.5);
console.log("fraction " + as_u536(fraction) + " " + as_usize6(fraction));
const zero = 0;
const positive = 7.9;
const large = 9007199254740000;
console.log("zero " + as_u533(zero) + " " + as_usize3(zero));
console.log("positive " + as_u536(positive) + " " + as_usize6(positive));
console.log("large " + as_u534(large) + " " + as_usize4(large));
console.log("fold " + as_u8(word) + " " + as_u16(word) + " " + as_u32(word));
