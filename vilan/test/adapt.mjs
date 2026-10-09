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
function fold_unsigned(value, modulus) {
	const truncated = Math.trunc(value);
	const wrapped = truncated % modulus;
	let $f = null;
	if (wrapped < 0) {
		$f = wrapped + modulus;
	} else {
		$f = wrapped;
	}
	return $f;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $g = null;
	if (wrapped >= half) {
		$g = wrapped - modulus;
	} else {
		$g = wrapped;
	}
	return $g;
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
async function sleep(ms, $a) {
	await (__sleep(clamp_delay(ms), ambient_signal($a)));
}
function clamp_delay(ms) {
	let $b = null;
	if (ms < 0) {
		$b = 0;
	} else {
		$b = ms;
	}
	return $b;
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
function run(f) {
	return f() + 100;
}
async function map(self, fn) {
	let result = [  ];
	for (const item of [ ...self ]) {
		result.push(await (fn(item)));
	}
	return result;
}
function map2(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
async function run2(f) {
	return await (f()) + 100;
}
async function helper(urls, f) {
	return await (map(urls, f));
}
(async () => {
	const urls = [ "ab", "cdef" ];
	const ids = await (map(urls, async (url) => {
		const length = url.length;
		await (sleep(1, [ 1 ]));
		return length;
	}));
	console.log(ids);
	console.log(map2(urls, (url) => {
		return url.length;
	}));
	console.log(String(await (run2(async () => {
		await (sleep(1, [ 1 ]));
		return 7;
	}))));
	console.log(String(run(() => {
		return 1;
	})));
	console.log(await (helper(urls, async (url) => {
		await (sleep(1, [ 1 ]));
		return as_i32(url.length) + 10;
	})));
})().catch(($h) => {
	console.error(String($h));
	process.exit(1);
});
