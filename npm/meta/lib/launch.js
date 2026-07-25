// The only part of vilan that ships as JavaScript: it finds the binary npm
// installed for this machine and hands the process over to it.
//
// Deliberately not a shim (proposal/distribution.md §2 — "no shim logic beyond
// resolution"). It does not read, rewrite or interpret an argument, does not
// touch the streams, and adds no behaviour of its own: `vilan` installed from
// npm must be indistinguishable from `vilan` unpacked from a release archive,
// down to the bytes on stdout and the exit code. Every line below is either
// resolution, hand-off, or an error message for the two ways resolution can
// fail.

"use strict";

const { spawnSync } = require("node:child_process");
const os = require("node:os");

// node's own platform+arch spelling → the package carrying that build. These
// five keys are exactly the release matrix's five targets; the release
// workflow maps the rust triples onto these names, and
// crates/vilan-cli/tests/npm_stub.rs pins the two lists against each other.
const PACKAGES = {
    "linux-x64": "@vilan-lang/linux-x64",
    "linux-arm64": "@vilan-lang/linux-arm64",
    "darwin-x64": "@vilan-lang/darwin-x64",
    "darwin-arm64": "@vilan-lang/darwin-arm64",
    "win32-x64": "@vilan-lang/win32-x64",
};

const RELEASES = "https://github.com/vilan-lang/vilan/releases";

// The absolute path of `name`'s executable inside this machine's platform
// package. Throws with the whole story when there is none — the two failure
// classes are genuinely different problems, so they get different messages:
// an unsupported platform can only be solved by a source build, a missing
// package by reinstalling.
function binaryPath(name) {
    const key = `${process.platform}-${process.arch}`;
    const packageName = PACKAGES[key];
    if (packageName === undefined) {
        throw new Error(
            `no prebuilt binary for ${key}; see ${RELEASES}\n` +
                `  prebuilt binaries exist for: ${Object.keys(PACKAGES).join(", ")}`,
        );
    }
    const suffix = process.platform === "win32" ? ".exe" : "";
    try {
        // No `exports` field in the platform packages, so the subpath resolves
        // directly — and require.resolve only locates the file, it never loads
        // it.
        return require.resolve(`${packageName}/bin/${name}${suffix}`);
    } catch (error) {
        if (error.code !== "MODULE_NOT_FOUND") {
            throw error;
        }
        throw new Error(
            `no prebuilt binary for ${key}: the package ${packageName} is not installed.\n` +
                "  It is an optional dependency of @vilan-lang/vilan — reinstall with\n" +
                "  `npm install -g @vilan-lang/vilan` (and without --omit=optional),\n" +
                `  or take a release archive from ${RELEASES}`,
        );
    }
}

// Run `name` with this process's arguments and streams; answer the exit code
// the caller should exit with.
function launch(name) {
    let binary;
    try {
        binary = binaryPath(name);
    } catch (error) {
        process.stderr.write(`${name}: ${error.message}\n`);
        return 1;
    }

    const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
    if (result.error) {
        process.stderr.write(`${name}: cannot run ${binary}: ${result.error.message}\n`);
        return 1;
    }
    if (result.status !== null) {
        return result.status;
    }

    // Killed by a signal: report it the way a shell reports a signalled child,
    // 128 + the signal number, so `$?` means the same thing whether vilan was
    // started through npm or directly (a Ctrl-C'd `vilan run` is the everyday
    // case).
    const number = os.constants.signals[result.signal];
    if (number === undefined) {
        process.stderr.write(`${name}: killed by ${result.signal}\n`);
        return 1;
    }
    return 128 + number;
}

module.exports = { PACKAGES, binaryPath, launch };
