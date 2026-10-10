function __at(list, index, location) {
	if (index >= 0 && index < list.length) return list[index];
	throw __panic("index out of bounds: the length is " + list.length + " but the index is " + index, location);
}
function __panic(message, location) {
	const error = new Error(message);
	error.name = "panicked at " + location;
	Object.defineProperty(error, "location", { value: location });
	if (Error.captureStackTrace) Error.captureStackTrace(error, __panic);
	return error;
}
function __sleep(ms, signal) {
	const sig = signal && signal[0] === 0 ? signal[1] : undefined;
	return new Promise((resolve, reject) => {
		if (sig && sig.aborted) {
			reject(sig.reason);
			return;
		}
		const timer = setTimeout(() => resolve(), ms);
		if (sig) sig.addEventListener("abort", () => {
			clearTimeout(timer);
			reject(sig.reason);
		}, { once: true });
	});
}
class __Task {
	constructor(run, origin, nursery) {
		this.origin = origin;
		this.observed = false;
		this.nursery = nursery;
		this.owned = !!nursery;
		this.rejected = false;
		this.error = undefined;
		this.promise = run();
		this.promise.then(null, (error) => {
			this.rejected = true;
			this.error = error;
			if (this.owned && !__nursery_is_cancel(error)) this.nursery.__fail(this);
			if (!this.observed && !this.owned) {
				globalThis.setTimeout(() => {
					if (!this.observed) console.error("unhandled task error (spawned in " + this.origin + "): " + String(error));
				}, 0);
			}
		});
		if (nursery) nursery.children.push(this);
	}
	then(onFulfilled, onRejected) {
		this.observed = true;
		return this.promise.then(onFulfilled, onRejected);
	}
}
function __task(run, origin, nursery) {
	return new __Task(run, origin, nursery);
}
async function sleep(ms, $b) {
	await (__sleep(clamp_delay(ms), ambient_signal($b)));
}
function clamp_delay(ms) {
	let $c = null;
	if (ms < 0) {
		$c = 0;
	} else {
		$c = ms;
	}
	return $c;
}
function ambient_signal($d) {
	const $e = $d;
	let $f = null;
	if ($e[0] === 0) {
		const n = $e[1];
		$f = [ 0, n.signal_of() ];
	} else {
		$f = [ 1 ];
	}
	return $f;
}
function doubled(self) {
	return self[0] * 2;
}
async function fetch_row($a) {
	await (sleep(0, $a));
	return [ 7, "seven" ];
}
async function fetch_list($g) {
	await (sleep(0, $g));
	return [ 10, 20, 30 ];
}
async function fetch_num($i) {
	await (sleep(0, $i));
	return 5;
}
async function fetch_maker($h) {
	await (sleep(0, $h));
	return () => {
		return 99;
	};
}
(async () => {
	console.log(String((await (fetch_row([ 1 ])))[0]));
	console.log(String((await (fetch_list([ 1 ]))).length));
	console.log(String((await (fetch_maker([ 1 ])))()));
	console.log(String((await (fetch_row([ 1 ])))[1].length));
	const pending = __task(async () => {
		return await (fetch_row([ 1 ]));
	}, "main");
	console.log(String((await (pending))[0]));
	console.log(String(__at(await (fetch_list([ 1 ])), 0, "await-postfix.vl:67:8")));
	console.log(String(doubled([ 21 ])));
	console.log(String(await (fetch_num([ 1 ])) + 1));
	const row = await (fetch_row([ 1 ]));
	console.log(String(row[0]));
})().catch(($j) => {
	console.error(String($j));
	process.exit(1);
});
