function to_string(self) {
	return "" + self;
}
function to_string2(self) {
	return "x=" + format(self[0]) + ", " + "y=" + format(self[1]);
}
function to_string3(self) {
	return "width=" + format(self[0]) + ", " + "height=" + format(self[1]);
}
function format(value) {
	return to_string(value);
}
function format2(value) {
	return to_string2(value);
}
function format3(value) {
	return to_string3(value);
}
console.log(format2([ 1, 2 ]));
console.log(format3([ 3, 4 ]));
