function accumulate(i) {
	return i * 10;
}
function one() {
	return 1;
}
function two() {
	return 2;
}
console.log(String(two()));
console.log(String(one()));
const __s2_m0 = (i) => {
	return accumulate(i);
};
console.log(String(0 + __s2_m0(0) + __s2_m0(1) + __s2_m0(2) + __s2_m0(3)));
const __s3_m0 = (i) => {
	return i + 100;
};
console.log(String(0 + __s3_m0(0) + __s3_m0(1) + __s3_m0(2)));
