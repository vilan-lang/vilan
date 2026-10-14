// E257 S6 (debugging.md §5.2): the `vilan` debug type and the "Debug" code
// lens on `fun main`.
//
// A `vilan` launch configuration is not a debug adapter of its own. It
// RESOLVES to VS Code's JavaScript debugger (`type: node`), which already
// speaks node's inspector and reads source maps — and `vilan run` writes the
// map beside the script it hands node (E257 S5). Two shapes:
//
// - a plain run: `vilan run --inspect-brk=127.0.0.1:<port> <program>`, the
//   JavaScript debugger attaching at that port (`attachSimplePort`) before
//   the first line runs, so every breakpoint is bound when the program starts;
// - `watch: true`: `vilan run --watch <program>`, the JavaScript debugger
//   attaching to every node process `vilan` starts (its child-process attach),
//   so the server a watch round restarts is debugged again each round.
//
// Either way the breakpoints are set in `.vl` files and the stops are shown in
// them; std's frames are skipped (the map lists std in `ignoreList`, and
// `skipFiles` names the std cache as well), as are the runtime helpers'.
//
// No `vscode` import: configurations in, configurations out, so `npm test`
// pins the resolution under plain node (`src/test/debug.test.ts`).

/// What a user writes in `launch.json` (the `debuggers` contribution's
/// `configurationAttributes` in `package.json` documents each).
export interface VilanLaunchConfiguration {
    type: string;
    request: string;
    name?: string;
    /// The `.vl` file or project directory `vilan run` is given.
    program?: string;
    /// Arguments for the program (`process::args()`).
    args?: string[];
    cwd?: string;
    /// `--entry`: which node leg of a workspace runs.
    entry?: string;
    /// `vilan run --watch`: rebuild and restart on every change, re-attaching.
    watch?: boolean;
    /// Pause on the program's first line.
    stopOnEntry?: boolean;
    /// The `vilan` executable (default: found as the language server is).
    vilanPath?: string;
    env?: Record<string, string>;
    console?: string;
    [key: string]: unknown;
}

/// What the resolution needs from the editor, gathered by `extension.ts`.
export interface LaunchContext {
    /// The `vilan` executable to run.
    vilan: string;
    /// A free local port for the inspector (the plain run's).
    port: number;
    /// Directories whose files the debugger never steps into: the std cache,
    /// and a configured `vilan.stdPath`.
    stdDirectories: string[];
    /// The active editor's file, when it is a `.vl` file.
    activeFile?: string;
    /// The workspace folder the configuration belongs to.
    workspaceFolder?: string;
}

/// The JavaScript debugger's launch configuration a `vilan` one resolves to,
/// or the reason it cannot.
export type Resolved =
    | { ok: true; configuration: Record<string, unknown> }
    | { ok: false; message: string };

/// The address the plain run's inspector listens on: loopback only, as node's
/// own default.
export const INSPECT_HOST = '127.0.0.1';

/// Resolves a `vilan` launch configuration (an empty one — F5 with no
/// `launch.json` — debugs the active `.vl` file).
export function resolveLaunch(
    configuration: VilanLaunchConfiguration,
    context: LaunchContext,
): Resolved {
    if (configuration.request !== 'launch') {
        return {
            ok: false,
            message:
                'A `vilan` debug configuration launches its program (`"request": "launch"`); ' +
                'to attach to a running `vilan run --inspect`, use a `node` attach configuration.',
        };
    }
    const program = configuration.program || context.activeFile || context.workspaceFolder;
    if (!program) {
        return {
            ok: false,
            message:
                'Nothing to debug: open a `.vl` file, or set `program` in the launch configuration.',
        };
    }
    const watch = configuration.watch === true;
    const run: string[] = ['run'];
    if (watch) {
        run.push('--watch');
    } else {
        run.push(`--inspect-brk=${INSPECT_HOST}:${context.port}`);
    }
    if (configuration.entry) {
        run.push('--entry', configuration.entry);
    }
    run.push(program, ...(configuration.args ?? []));
    const skipFiles = [
        '<node_internals>/**',
        ...context.stdDirectories.map((directory) => `${trimSlash(directory)}/**`),
    ];
    const resolved: Record<string, unknown> = {
        type: 'node',
        request: 'launch',
        name: configuration.name ?? 'Debug vilan program',
        runtimeExecutable: configuration.vilanPath || context.vilan,
        runtimeArgs: run,
        cwd: configuration.cwd ?? context.workspaceFolder,
        env: configuration.env,
        // The program's stdin is the terminal's (`scan()`), as under `vilan run`.
        console: configuration.console ?? 'integratedTerminal',
        sourceMaps: true,
        // `vilan run` hands node a script in the temp directory, its map
        // beside it: maps are read wherever the script is, not only in the
        // workspace (the JavaScript debugger's default).
        resolveSourceMapLocations: null,
        skipFiles,
        stopOnEntry: configuration.stopOnEntry === true,
    };
    if (watch) {
        // Each round's server is a new node process under `vilan`.
        resolved.autoAttachChildProcesses = true;
    } else {
        // `--inspect-brk` holds the program before its first line until the
        // debugger has attached here and bound the breakpoints.
        resolved.attachSimplePort = context.port;
        resolved.continueOnAttach = configuration.stopOnEntry !== true;
    }
    return { ok: true, configuration: resolved };
}

function trimSlash(directory: string): string {
    return directory.replace(/[\\/]+$/, '');
}

/// Where the `vilan` executable may be, in the order the language server's
/// own discovery asks (`resolveServerPath`): an in-repo build, `cargo
/// install`, the release toolchain, and then the PATH. A configured server
/// path that names a file is asked first — the CLI is installed beside it.
export function vilanCandidates(options: {
    serverPath?: string;
    repoRoot: string;
    home: string;
    suffix: string;
    join: (...parts: string[]) => string;
    dirname: (path: string) => string;
}): string[] {
    const binary = `vilan${options.suffix}`;
    const candidates: string[] = [];
    const server = options.serverPath;
    if (server && server !== 'vilan-lsp' && server !== `vilan-lsp${options.suffix}`) {
        candidates.push(options.join(options.dirname(server), binary));
    }
    candidates.push(
        options.join(options.repoRoot, 'target', 'release', binary),
        options.join(options.repoRoot, 'target', 'debug', binary),
        options.join(options.home, '.cargo', 'bin', binary),
        options.join(options.home, '.vilan', 'bin', binary),
    );
    return candidates;
}

/// The 0-based lines that declare a program's `fun main` — where the "Debug"
/// code lens goes. A top-level declaration only (no indentation: a `main`
/// inside an `impl` is a method, not a program's), with or without `async`.
export function mainFunctionLines(text: string): number[] {
    const lines: number[] = [];
    const declaration = /^(?:pub\s+)?(?:async\s+)?fun\s+main\s*\(/;
    text.split(/\r?\n/).forEach((line, index) => {
        if (declaration.test(line)) {
            lines.push(index);
        }
    });
    return lines;
}

/// The configuration the code lens starts: the file it sits in.
export function mainLensConfiguration(file: string): VilanLaunchConfiguration & { name: string } {
    return {
        type: 'vilan',
        request: 'launch',
        name: 'Debug main',
        program: file,
    };
}
