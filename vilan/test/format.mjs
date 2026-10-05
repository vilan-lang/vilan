function to_string(self) {
	return self;
}
function to_string2(self) {
	return "" + self;
}
function to_string3(self) {
	return "" + self;
}
function to_string4(self) {
	return "Id { n = " + to_string2(self[0]) + " }";
}
function to_string5(self) {
	return "Point { x = " + to_string2(self[0]) + ", y = " + to_string2(self[1]) + " }";
}
function format(value) {
	return to_string4(value);
}
function format2(value) {
	return to_string5(value);
}
function format3(value) {
	return to_string2(value);
}
function format4(value) {
	return to_string(value);
}
function format5(value) {
	return to_string3(value);
}
const id = [ 0 ];
console.log(id);
console.log(format(id));
console.log(format2([ 1, 2 ]));
console.log(format3(42));
console.log(format4("hi"));
console.log(format5(true));
