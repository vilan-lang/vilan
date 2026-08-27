function __at(list, index) {
	if (index >= 0 && index < list.length) return list[index];
	throw "index out of bounds: the length is " + list.length + " but the index is " + index;
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
	await (__sleep(ms, ambient_signal($b)));
}
function ambient_signal($c) {
	const $d = $c;
	let $e = null;
	if ($d[0] === 0) {
		const n = $d[1];
		$e = [ 0, n.signal_of() ];
	} else {
		$e = [ 1 ];
	}
	return $e;
}
function doubled(self) {
	return self[0] * 2;
}
async function fetch_row($a) {
	await (sleep(0, $a));
	return [ 7, "seven" ];
}
async function fetch_list($f) {
	await (sleep(0, $f));
	return [ 10, 20, 30 ];
}
async function fetch_num($h) {
	await (sleep(0, $h));
	return 5;
}
async function fetch_maker($g) {
	await (sleep(0, $g));
	return () => {
		return 99;
	};
}
(async () => {
	console.log((await (fetch_row([ 1 ])))[0]);
	console.log((await (fetch_list([ 1 ]))).length);
	console.log((await (fetch_maker([ 1 ])))());
	console.log((await (fetch_row([ 1 ])))[1].length);
	const pending = __task(async () => {
		return await (fetch_row([ 1 ]));
	}, "main");
	console.log((await (pending))[0]);
	console.log(__at(await (fetch_list([ 1 ])), 0));
	console.log(doubled([ 21 ]));
	console.log(await (fetch_num([ 1 ])) + 1);
	const row = await (fetch_row([ 1 ]));
	console.log(row[0]);
})().catch(($i) => {
	console.error(String($i));
	process.exit(1);
});
