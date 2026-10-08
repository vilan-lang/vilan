function debug(self) {
	const escaped = self.replaceAll("\\", "\\\\").replaceAll("\"", "\\\"").replaceAll("\n", "\\n").replaceAll("\t", "\\t").replaceAll("\r", "\\r").replaceAll("\u{0}", "\\0");
	return "\"" + escaped + "\"";
}
function debug2(self) {
	return "Point { x = " + JSON.stringify(self[0]) + ", " + "y = " + JSON.stringify(self[1]) + " }";
}
function debug3(self) {
	return "Tagged { label = " + debug(self[0]) + ", " + "at = " + debug2(self[1]) + ", " + "on = " + JSON.stringify(self[2]) + " }";
}
function debug4(self) {
	return "Empty";
}
console.log(debug2([ 1, 2 ]));
const t = [ "hi", [ 3, 4 ], true ];
console.log(debug3(t));
console.log(debug4([  ]));
