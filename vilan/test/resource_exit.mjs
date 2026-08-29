function drop(self) {
	console.log(self[0]);
}
function $b($c) {
	drop($c);
}
let $a = undefined;
const guard = [ "teardown" ];
$b(guard);
console.log("body");
$a = 7;
process.exit($a);
