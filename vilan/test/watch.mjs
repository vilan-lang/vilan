class __Watcher {
	constructor(fsPromises, nodePath, root, recursive, intervalMs) {
		this.fs = fsPromises;
		this.nodePath = nodePath;
		this.root = nodePath.join(root, ".");
		this.recursive = recursive;
		this.intervalMs = intervalMs;
		this.previous = new Map();
		this.queue = [];
		this.waiters = [];
		this.stopped = false;
		this.id = null;
	}
	__key(path) {
		return this.nodePath.sep === "/" ? path : path.split(this.nodePath.sep).join("/");
	}
	async __stat(path) {
		try {
			return await this.fs.stat(path);
		} catch (error) {
			if (error && (error.code === "ENOENT" || error.code === "ENOTDIR")) return null;
			throw error;
		}
	}
	async __snapshot() {
		const seen = new Map();
		const rootStat = await this.__stat(this.root);
		if (rootStat === null) return seen;
		seen.set(this.__key(this.root), { mtime: rootStat.mtimeMs, size: rootStat.size, dir: rootStat.isDirectory() });
		if (!rootStat.isDirectory()) return seen;
		let names;
		try {
			names = await this.fs.readdir(this.root, { recursive: this.recursive });
		} catch (error) {
			if (error && error.code === "ENOENT") return seen;
			throw error;
		}
		for (const name of names) {
			const full = this.nodePath.join(this.root, name);
			const entry = await this.__stat(full);
			if (entry !== null) seen.set(this.__key(full), { mtime: entry.mtimeMs, size: entry.size, dir: entry.isDirectory() });
		}
		return seen;
	}
	__diff(current) {
		const changes = [];
		for (const [ path, now ] of current) {
			const before = this.previous.get(path);
			if (before === undefined) changes.push({ path: path, kind: [ 0 ] });
			else if (!now.dir && (before.mtime !== now.mtime || before.size !== now.size)) changes.push({ path: path, kind: [ 1 ] });
		}
		for (const path of this.previous.keys()) {
			if (!current.has(path)) changes.push({ path: path, kind: [ 2 ] });
		}
		changes.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
		return changes;
	}
	__arm() {
		if (this.stopped) return;
		this.id = setTimeout(() => this.__tick(), this.intervalMs);
	}
	async __tick() {
		this.id = null;
		if (this.stopped) return;
		let current;
		try {
			current = await this.__snapshot();
		} catch (error) {
			console.error("vilan: a filesystem watch poll failed:", error);
			this.__arm();
			return;
		}
		if (this.stopped) return;
		for (const change of this.__diff(current)) this.queue.push(change);
		this.previous = current;
		while (this.queue.length > 0 && this.waiters.length > 0) this.waiters.shift().resolve(this.queue.shift());
		this.__arm();
	}
	next_change(signal) {
		if (this.queue.length > 0) return Promise.resolve(this.queue.shift());
		const sig = signal && signal[0] === 0 ? signal[1] : undefined;
		return new Promise((resolve, reject) => {
			if (this.stopped) {
				reject("the watcher was dropped while a change was awaited");
				return;
			}
			if (sig && sig.aborted) {
				reject(sig.reason);
				return;
			}
			const waiter = { resolve: resolve, reject: reject };
			this.waiters.push(waiter);
			if (sig) sig.addEventListener("abort", () => {
				const parked = this.waiters.indexOf(waiter);
				if (parked >= 0) this.waiters.splice(parked, 1);
				reject(sig.reason);
			}, { once: true });
		});
	}
	stop() {
		if (this.stopped) return;
		this.stopped = true;
		if (this.id !== null) clearTimeout(this.id);
		this.id = null;
		const waiters = this.waiters;
		this.waiters = [];
		for (const waiter of waiters) waiter.reject("the watcher was dropped while a change was awaited");
	}
}
async function __fs_watch(root, recursive, intervalMs) {
	const fsPromises = await import("node:fs/promises");
	const nodePath = await import("node:path");
	const watcher = new __Watcher(fsPromises, nodePath, root, recursive, intervalMs);
	watcher.previous = await watcher.__snapshot();
	watcher.__arm();
	return watcher;
}
function __fs_watch_stop(watcher) {
	watcher.stop();
}
async function watch(path) {
	return await (__fs_watch(path, false, 300));
}
async function watch_all(path) {
	return await (__fs_watch(path, true, 300));
}
async function next(self, $a) {
	const raw = await (self.next_change(ambient_signal($a)));
	return [ raw.path, raw.kind ];
}
function drop(self) {
	__fs_watch_stop(self);
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
function describe(change) {
	const $e = change[1];
	let $f = null;
	if ($e[0] === 0) {
		$f = "created " + change[0];
	} else if ($e[0] === 1) {
		$f = "modified " + change[0];
	} else {
		$f = "removed " + change[0];
	}
	return $f;
}
function $g($h) {
	drop($h);
}
(async () => {
	let flat = await (watch("watch-corpus"));
	try {
		console.log(describe(await (next(flat, [ 1 ]))));
		$g(flat);
		flat = null;
	} finally {
		if (flat !== null) {
			$g(flat);
		}
	}
	const deep = await (watch_all("watch-corpus"));
	try {
		console.log(describe(await (next(deep, [ 1 ]))));
	} finally {
		$g(deep);
	}
})().catch(($i) => {
	console.error(String($i));
	process.exit(1);
});
