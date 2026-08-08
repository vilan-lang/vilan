function to_string(self) {
	return "Point";
}
function $a(value) {
	return to_string(value);
}
const who = "world";
const text = "hello " + who + "\n" + who + " leads\n" + "\n" + "    indented " + who + " deeper";
console.log(text);
const raw = "literal {braces}, no \\n escape, path C:\\dir " + who;
console.log(raw);
const quoted = "say \"\" and \"" + who + "\"";
console.log(quoted);
const nested = "" + ("{not a hole}" + who);
console.log(nested);
const empty = "";
console.log("[" + empty + "]" + "!");
console.log($a([ 1, 2 ]));
