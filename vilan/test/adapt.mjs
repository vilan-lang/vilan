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
	let $h = null;
	if (wrapped < 0) {
		$h = wrapped + modulus;
	} else {
		$h = wrapped;
	}
	return $h;
}
function fold_signed(value, modulus, half) {
	const wrapped = fold_unsigned(value, modulus);
	let $i = null;
	if (wrapped >= half) {
		$i = wrapped - modulus;
	} else {
		$i = wrapped;
	}
	return $i;
}
function as_i32(self) {
	const widened = Number(self);
	return Number(fold_signed(widened, 4294967296, 2147483648));
}
async function sleep(ms, $a) {
	await (__sleep(ms, ambient_signal($a)));
}
function ambient_signal($b) {
	const $c = $b;
	let $d = null;
	if ($c[0] === 0) {
		const n = $c[1];
		$d = [ 0, n.signal_of() ];
	} else {
		$d = [ 1 ];
	}
	return $d;
}
function run(f) {
	return f() + 100;
}
async function $e(self, fn) {
	let result = [  ];
	for (const item of [ ...self ]) {
		result.push(await (fn(item)));
	}
	return result;
}
function $f(self, fn) {
	let result = [  ];
	for (const item of self) {
		result.push(fn(item));
	}
	return result;
}
async function $g(f) {
	return await (f()) + 100;
}
async function $j(urls, f) {
	return await ($e(urls, f));
}
(async () => {
	const urls = [ "ab", "cdef" ];
	const ids = await ($e(urls, async (url) => {
		const length = url.length;
		await (sleep(1, [ 1 ]));
		return length;
	}));
	console.log(ids);
	console.log($f(urls, (url) => {
		return url.length;
	}));
	console.log(await ($g(async () => {
		await (sleep(1, [ 1 ]));
		return 7;
	})));
	console.log(run(() => {
		return 1;
	}));
	console.log(await ($j(urls, async (url) => {
		await (sleep(1, [ 1 ]));
		return as_i32(url.length) + 10;
	})));
})().catch(($l) => {
	console.error(String($l));
	process.exit(1);
});
