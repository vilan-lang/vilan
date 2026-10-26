// E257 S7 (debugging.md §5.3, door D1): `vilan-dap`, the `vilan` debug
// type's adapter. It speaks the Debug Adapter Protocol to the editor and the
// Chrome DevTools Protocol to node's inspector, which `vilan run
// --inspect-brk` opens over the program (S5/S6), and prints the Variables
// and Watch panes by static type: the sidecar beside the script's map says
// what each JavaScript binding is, and the walker reads it as `dbg` prints it.
// A binding the sidecar has no row for — a compiler temporary, a value a
// shared generic body holds at two types — shows as JavaScript.
//
// It runs inside the extension host (an inline adapter: no second runtime,
// nothing to install beside the vsix) and imports nothing from `vscode`, so a
// scripted session drives it under plain node (`src/test/session.test.ts`).
import { ChildProcess, spawn } from 'child_process';
import * as fs from 'fs';
import * as net from 'net';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { CdpSession, inspectorTargets } from './cdp';
import { Doc, WALKER, children, expandable, flat, layout } from './printer';
import { ResolvedFrame, Sidecar, SidecarBinding, parsePath } from './sidecar';
import { SourceMap, normalize } from './sourceMap';

/// A DAP message, loosely typed: the adapter reads what it needs.
export interface DapMessage {
    seq: number;
    type: 'request' | 'response' | 'event';
    command?: string;
    event?: string;
    arguments?: any;
    body?: any;
    request_seq?: number;
    success?: boolean;
    message?: string;
}

/// What a `vilan` launch configuration resolves to for the adapter
/// (`debug.ts`'s `resolveAdapterLaunch`).
export interface AdapterLaunchArguments {
    /// The `vilan` executable.
    vilan: string;
    program: string;
    args?: string[];
    cwd?: string;
    entry?: string;
    watch?: boolean;
    stopOnEntry?: boolean;
    env?: Record<string, string>;
    /// `internalConsole` runs the program with its output in the debug
    /// console; a terminal keeps its stdin (`scan()`).
    console?: string;
    /// The inspector's port; a free one when absent.
    port?: number;
}

export const INSPECT_HOST = '127.0.0.1';

/// What node writes to stderr about its inspector, a line at a time.
const WAITING_FOR_DISCONNECT = /^Waiting for the debugger to disconnect\.\.\.\r?$/m;
const INSPECTOR_BANNER =
    /^(?:Debugger listening on ws:\/\/\S+|For help, see: https:\/\/nodejs\.org\/en\/docs\/inspector|Debugger attached\.|Waiting for the debugger to disconnect\.\.\.)\r?\n/gm;
const THREAD_ID = 1;
/// The reactive trace's lines (`std::reactive::trace`, E257 S8), as the program
/// writes them to stderr.
const TRACE_LINE = /^\[reactive\] .*(?:\r?\n|$)/gm;

/// What an `internalConsole` launch shows of a chunk of the program's stderr:
/// node's lines about the inspector this adapter is, and the reactive trace's
/// lines (which reach the console from the inspector instead), taken out.
export function shownStderr(text: string): string {
    return text.replace(INSPECTOR_BANNER, '').replace(TRACE_LINE, '');
}

/// The command `vilan-dap` starts: `vilan run --inspect-brk` over the program,
/// `--watch` with every round held at its first line until the adapter has
/// attached and bound the breakpoints.
export function runCommand(launch: AdapterLaunchArguments, port: number): string[] {
    const run = ['run'];
    if (launch.watch) {
        run.push('--watch');
    }
    run.push(`--inspect-brk=${INSPECT_HOST}:${port}`);
    if (launch.entry) {
        run.push('--entry', launch.entry);
    }
    run.push(launch.program, ...(launch.args ?? []));
    return run;
}

interface ScriptInfo {
    id: string;
    url: string;
    map?: SourceMap;
    sidecar?: Sidecar;
}

/// What a `variablesReference` stands for while the program is paused.
type Handle =
    | {
          kind: 'scope';
          objects: string[];
          frame?: ResolvedFrame;
          sidecar?: Sidecar;
          main: boolean;
          /// The paused frame the scope is one of: where the program's own
          /// printers are looked up (E323).
          callFrameId?: string;
      }
    | { kind: 'object'; objectId: string }
    | { kind: 'doc'; doc: Doc; path?: string };

interface FrameInfo {
    callFrame: any;
    script?: ScriptInfo;
    resolved?: ResolvedFrame;
    /// A frame of an async parent (E326): a place the program will come back
    /// to, not one it stands in — its scopes are gone.
    awaiting?: boolean;
}

interface BreakpointRequest {
    id: number;
    line: number;
    condition?: string;
    /// The line it bound on, once it bound.
    bound?: number;
    cdp: string[];
}

type Listener = (message: DapMessage) => void;

export class VilanDebugAdapter {
    private seq = 1;
    private readonly listeners: Listener[] = [];
    private launch: AdapterLaunchArguments | undefined;
    private port = 0;
    private child: ChildProcess | undefined;
    /// The `vilan` process and the round's node, by pid — what a terminal
    /// launch can only learn from the program itself.
    private vilanPid: number | undefined;
    private nodePid: number | undefined;
    private cdp: CdpSession | undefined;
    private scripts = new Map<string, ScriptInfo>();
    private breakpoints = new Map<string, { path: string; requests: BreakpointRequest[] }>();
    private nextBreakpointId = 1;
    private exceptions: 'none' | 'uncaught' | 'all' = 'uncaught';
    private configured = false;
    private configuredWaiters: (() => void)[] = [];
    private round = 0;
    private paused:
        | { callFrames: any[]; asyncStackTrace?: any; reason: string; data?: any; hit: string[] }
        | undefined;
    private frames = new Map<number, FrameInfo>();
    private handles = new Map<number, Handle>();
    private nextHandle = 1;
    private stepping: { kind: 'next' | 'stepIn' | 'stepOut'; from?: string; depth: number; budget: number } | undefined;
    private pauseRequested = false;
    private terminated = false;
    private entryPending = false;
    private sources = new Map<number, string>();
    private sourceReferences = new Map<string, number>();

    /// The editor's subscription (`vscode.DebugAdapter.onDidSendMessage`).
    readonly onDidSendMessage = (listener: Listener) => {
        this.listeners.push(listener);
        return {
            dispose: () => {
                const index = this.listeners.indexOf(listener);
                if (index >= 0) {
                    this.listeners.splice(index, 1);
                }
            },
        };
    };

    handleMessage(message: DapMessage): void {
        if (message.type === 'response') {
            this.reverseResponses.get(message.request_seq ?? -1)?.(message);
            this.reverseResponses.delete(message.request_seq ?? -1);
            return;
        }
        if (message.type !== 'request' || !message.command) {
            return;
        }
        void this.dispatch(message).then(
            (body) => this.respond(message, true, body),
            (error: unknown) =>
                this.respond(message, false, undefined, error instanceof Error ? error.message : String(error)),
        );
    }

    dispose(): void {
        this.kill();
    }

    // --- The protocol ---------------------------------------------------------

    private send(message: Omit<DapMessage, 'seq'>): void {
        const sent = { ...message, seq: this.seq++ } as DapMessage;
        for (const listener of [...this.listeners]) {
            listener(sent);
        }
    }

    private respond(request: DapMessage, success: boolean, body?: unknown, message?: string): void {
        this.send({
            type: 'response',
            request_seq: request.seq,
            command: request.command,
            success,
            message,
            body,
        });
    }

    private event(event: string, body?: unknown): void {
        this.send({ type: 'event', event, body });
    }

    private readonly reverseResponses = new Map<number, (message: DapMessage) => void>();

    /// A request to the editor (`runInTerminal`), answered by its response.
    private request(command: string, args: unknown): Promise<DapMessage> {
        const seq = this.seq;
        return new Promise((resolve) => {
            this.reverseResponses.set(seq, resolve);
            this.send({ type: 'request', command, arguments: args });
        });
    }

    private output(category: string, text: string): void {
        this.event('output', { category, output: text });
    }

    private async dispatch(request: DapMessage): Promise<unknown> {
        const args = request.arguments ?? {};
        switch (request.command) {
            case 'initialize':
                return {
                    supportsConfigurationDoneRequest: true,
                    supportsEvaluateForHovers: true,
                    supportsTerminateRequest: true,
                    supportsConditionalBreakpoints: true,
                    supportsClipboardContext: true,
                    exceptionBreakpointFilters: [
                        { filter: 'uncaught', label: 'Uncaught Exceptions', default: true },
                        { filter: 'all', label: 'All Exceptions', default: false },
                    ],
                };
            case 'launch':
                await this.start(args as AdapterLaunchArguments);
                this.event('initialized');
                return undefined;
            case 'setBreakpoints':
                return { breakpoints: await this.setBreakpoints(args) };
            case 'setExceptionBreakpoints': {
                const filters: string[] = args.filters ?? [];
                this.exceptions = filters.includes('all') ? 'all' : filters.includes('uncaught') ? 'uncaught' : 'none';
                await this.cdp?.send('Debugger.setPauseOnExceptions', { state: this.exceptions }).catch(() => undefined);
                return undefined;
            }
            case 'configurationDone':
                this.configured = true;
                for (const waiter of this.configuredWaiters.splice(0)) {
                    waiter();
                }
                return undefined;
            case 'threads':
                return { threads: [{ id: THREAD_ID, name: this.launch?.watch ? `round ${this.round}` : 'main' }] };
            case 'stackTrace':
                return this.stackTrace(args);
            case 'scopes':
                return { scopes: await this.scopes(args.frameId) };
            case 'variables':
                return { variables: await this.variables(args.variablesReference) };
            case 'evaluate':
                return this.evaluate(args.expression, args.frameId, args.context);
            case 'source':
                return this.source(args);
            case 'continue':
                await this.resume('Debugger.resume');
                return { allThreadsContinued: true };
            case 'next':
                await this.step('next');
                return undefined;
            case 'stepIn':
                await this.step('stepIn');
                return undefined;
            case 'stepOut':
                await this.step('stepOut');
                return undefined;
            case 'pause':
                this.pauseRequested = true;
                await this.cdp?.send('Debugger.pause');
                return undefined;
            case 'terminate':
            case 'disconnect':
                this.kill();
                this.finish();
                return undefined;
            default:
                throw new Error(`\`${request.command}\` is not supported by the vilan debugger`);
        }
    }

    // --- Launching and attaching ----------------------------------------------

    private async start(launch: AdapterLaunchArguments): Promise<void> {
        this.launch = launch;
        this.port = launch.port ?? (await freePort());
        const command = runCommand(launch, this.port);
        const cwd = launch.cwd ?? path.dirname(path.resolve(launch.program));
        const env = { ...process.env, ...(launch.env ?? {}) };
        const consoleKind = launch.console ?? 'integratedTerminal';
        if (consoleKind === 'internalConsole') {
            const child = spawn(launch.vilan, command, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
            this.child = child;
            this.vilanPid = child.pid;
            child.stdout?.setEncoding('utf8');
            child.stderr?.setEncoding('utf8');
            child.stdout?.on('data', (text: string) => this.output('stdout', text));
            child.stderr?.on('data', (text: string) => {
                // node's own lines about the inspector this adapter is: the
                // debug console is the program's.
                if (WAITING_FOR_DISCONNECT.test(text)) {
                    // The same end once more, as node writes it: the one
                    // signal no inspector protocol detail can lose.
                    this.programEnded();
                }
                // The reactive trace's lines reach the console from the
                // inspector (`Runtime.consoleAPICalled`), as in a terminal
                // launch: once, and as the console's.
                const shown = shownStderr(text);
                if (shown) {
                    this.output('stderr', shown);
                }
            });
            const failed = new Promise<never>((_, reject) => child.once('error', reject));
            child.once('exit', (code) => {
                this.child = undefined;
                this.event('exited', { exitCode: code ?? 1 });
                this.finish();
            });
            await Promise.race([new Promise((resolve) => child.once('spawn', resolve)), failed]);
        } else {
            const response = await this.request('runInTerminal', {
                kind: consoleKind === 'externalTerminal' ? 'external' : 'integrated',
                title: 'vilan run',
                cwd,
                args: [launch.vilan, ...command],
                env: launch.env ?? {},
            });
            if (!response.success) {
                throw new Error(response.message ?? 'the editor could not start `vilan run`');
            }
            // VS Code answers with the terminal's shell, not the process it
            // runs (`shellProcessId`); `vilan run` is then learned from its
            // node at the first round.
            this.vilanPid = validPid(response.body?.processId);
        }
        void this.attachRounds();
    }

    /// Attaches to each inspector `vilan run` opens: one for a plain run, one
    /// per round under `--watch`.
    private async attachRounds(): Promise<void> {
        while (!this.terminated) {
            const targets = await inspectorTargets(INSPECT_HOST, this.port);
            const target = targets?.find((candidate) => candidate.webSocketDebuggerUrl);
            if (!target) {
                if (this.launch?.console !== 'internalConsole' && this.round > 0 && !this.launch?.watch) {
                    // A terminal launch: the run is over once its one round is.
                    this.finish();
                    return;
                }
                if (this.vilanPid !== undefined && !alive(this.vilanPid)) {
                    // `vilan run` is gone — a compile error before any
                    // inspector opened, or the end of a watch session. Only a
                    // pid the session KNOWS can be declared gone (`validPid`);
                    // an unknown one is not "dead" (E331: a `null` read as a
                    // dead process ended a watch session between rounds).
                    this.finish();
                    return;
                }
                await delay(100);
                continue;
            }
            const closed = new Promise<void>((resolve) => {
                void this.attach(target.webSocketDebuggerUrl, resolve).catch((error) => {
                    this.output('stderr', `vilan-dap: ${error instanceof Error ? error.message : String(error)}\n`);
                    resolve();
                });
            });
            await closed;
            if (!this.launch?.watch) {
                // The plain run's one program ended; `vilan run` exits with it.
                if (this.launch?.console !== 'internalConsole') {
                    this.finish();
                }
                return;
            }
            // The next round's server starts its inspector at the same port.
            await delay(100);
        }
    }

    private async attach(url: string, closed: () => void): Promise<void> {
        this.round++;
        this.scripts = new Map();
        this.contexts = new Set();
        this.mainContext = undefined;
        const initial = new Promise<void>((resolve) => (this.initialPause = resolve));
        const cdp = await CdpSession.connect(
            url,
            (method, params) => this.onCdpEvent(method, params),
            () => {
                if (this.cdp === cdp) {
                    this.cdp = undefined;
                    this.clearPause();
                }
                closed();
            },
        );
        this.cdp = cdp;
        await cdp.send('Runtime.enable');
        await cdp.send('NodeRuntime.notifyWhenWaitingForDisconnect', { enabled: true }).catch(() => undefined);
        await cdp.send('Debugger.enable');
        await cdp.send('Debugger.setAsyncCallStackDepth', { maxDepth: 32 }).catch(() => undefined);
        await cdp.send('Debugger.setPauseOnExceptions', { state: this.exceptions });
        await cdp.send('Runtime.runIfWaitingForDebugger');
        await initial;
        // Who to stop when the session ends: this round's node, and `vilan
        // run` (its parent) when the launch could not say. Asked at the first
        // line, not before it: until node has bootstrapped, `process.pid` and
        // `process.ppid` read `null` (E331).
        const pids = await cdp
            .send('Runtime.evaluate', { expression: '[process.pid, process.ppid]', returnByValue: true })
            .catch(() => undefined);
        const [nodePid, parentPid] = (pids?.result?.value as unknown[] | undefined) ?? [];
        this.nodePid = validPid(nodePid);
        this.vilanPid ??= validPid(parentPid);
        // Every breakpoint binds before the program's first line runs.
        await this.bindAll();
        if (this.round === 1 && !this.configured) {
            await new Promise<void>((resolve) => this.configuredWaiters.push(resolve));
            await this.bindAll();
        }
        if (this.round === 1 && this.launch?.stopOnEntry) {
            this.entryPending = true;
            await this.cdp?.send('Debugger.stepOver');
            return;
        }
        await this.resume('Debugger.resume', false);
    }

    private initialPause: (() => void) | undefined;
    private contexts = new Set<number>();
    private mainContext: number | undefined;

    /// The round's program has run to its end and node waits for the
    /// debugger to disconnect: disconnect. A plain run then ends with
    /// `vilan run`; if `vilan` has not exited a few seconds later, the session
    /// ends anyway rather than waiting on it.
    private programEnded(): void {
        this.cdp?.close();
        if (!this.launch?.watch) {
            setTimeout(() => {
                if (!this.finished) {
                    this.kill();
                    this.finish();
                }
            }, 5000).unref();
        }
    }

    private onCdpEvent(method: string, params: any): void {
        switch (method) {
            case 'Debugger.scriptParsed':
                this.scriptParsed(params);
                break;
            case 'Debugger.paused':
                void this.onPaused(params);
                break;
            case 'Runtime.consoleAPICalled':
                this.consoleCalled(params);
                break;
            case 'NodeRuntime.waitingForDisconnect':
                // The program ran to its end: node holds the process open
                // ("Waiting for the debugger to disconnect") until the
                // debugger lets go, so let go. Asked for at attach
                // (`NodeRuntime.notifyWhenWaitingForDisconnect`): node then
                // sends this instead of destroying the main context.
                this.programEnded();
                break;
            case 'Runtime.executionContextCreated':
                this.contexts.add(params.context?.id);
                if (params.context?.auxData?.isDefault) {
                    this.mainContext = params.context.id;
                }
                break;
            case 'Runtime.executionContextDestroyed':
                // The same end, from a node without the `NodeRuntime` domain:
                // its main context goes.
                this.contexts.delete(params.executionContextId);
                if (params.executionContextId === this.mainContext || this.contexts.size === 0) {
                    this.programEnded();
                }
                break;
            default:
                break;
        }
    }

    /// E257 S8: a line of the reactive trace, written by `console.error` (or
    /// `console.log` in a browser), is an output event in the debug console —
    /// whatever the launch did with stderr — with the write it names as its
    /// place where a script's map holds that file. The program's other
    /// console calls are its own output, which the launch already shows.
    private consoleCalled(params: any): void {
        const first = params.args?.[0];
        if (first?.type !== 'string' || typeof first.value !== 'string' || !first.value.startsWith('[reactive] ')) {
            return;
        }
        const line: string = first.value;
        const body: Record<string, unknown> = { category: 'console', output: line + '\n' };
        // The write's place when it has one, else the cell's.
        const place = /, set at (.+):(\d+):(\d+)$/.exec(line) ?? /\(made at (.+?):(\d+):(\d+)\)/.exec(line);
        const file = place ? this.sourceFile(place[1]) : undefined;
        if (place && file) {
            body.source = { name: path.basename(file), path: file };
            body.line = Number(place[2]);
            body.column = Number(place[3]);
        }
        this.event('output', body);
    }

    /// The file a location's text names (`src/main.vl`, relative to its package
    /// root): the source of a script's map whose path ends with it.
    private sourceFile(relative: string): string | undefined {
        const tail = relative.split('/').join(path.sep);
        for (const script of this.scripts.values()) {
            for (const source of script.map?.sources ?? []) {
                if (source.path && !source.ignored && (source.path === tail || source.path.endsWith(path.sep + tail))) {
                    return source.path;
                }
            }
        }
        return undefined;
    }

    private scriptParsed(params: any): void {
        const url: string = params.url ?? '';
        const script: ScriptInfo = { id: params.scriptId, url };
        this.scripts.set(params.scriptId, script);
        if (!url.startsWith('file:') || !params.sourceMapURL) {
            return;
        }
        try {
            const mapUrl = new URL(params.sourceMapURL, url);
            if (mapUrl.protocol === 'file:') {
                const mapPath = fileURLToPath(mapUrl);
                script.map = new SourceMap(JSON.parse(fs.readFileSync(mapPath, 'utf8')), mapPath);
            } else if (mapUrl.protocol === 'data:') {
                const comma = params.sourceMapURL.indexOf(',');
                const data = params.sourceMapURL.slice(comma + 1);
                const text = params.sourceMapURL.slice(0, comma).includes(';base64')
                    ? Buffer.from(data, 'base64').toString('utf8')
                    : decodeURIComponent(data);
                script.map = new SourceMap(JSON.parse(text), fileURLToPath(url));
            }
        } catch {
            script.map = undefined;
        }
        try {
            const sidecarPath = fileURLToPath(url) + '.vdbg.json';
            if (fs.existsSync(sidecarPath)) {
                script.sidecar = new Sidecar(JSON.parse(fs.readFileSync(sidecarPath, 'utf8')));
            }
        } catch (error) {
            this.output('stderr', `vilan-dap: the sidecar beside ${url} is unreadable (${error}); values show as JavaScript\n`);
        }
        if (script.map) {
            const ranges = script.map.ignoredRanges();
            if (ranges.length > 0) {
                const positions = ranges.flatMap(([start, end]) => [
                    { lineNumber: start.line, columnNumber: start.column },
                    { lineNumber: end.line, columnNumber: end.column },
                ]);
                void this.cdp?.send('Debugger.setBlackboxedRanges', { scriptId: script.id, positions }).catch(() => undefined);
            }
        }
    }

    // --- Breakpoints ------------------------------------------------------------

    private async setBreakpoints(args: any): Promise<unknown[]> {
        const file: string | undefined = args.source?.path;
        if (!file) {
            return (args.breakpoints ?? []).map(() => ({ verified: false, message: 'not a file' }));
        }
        const key = normalize(file);
        const previous = this.breakpoints.get(key);
        if (previous && this.cdp) {
            for (const request of previous.requests) {
                for (const id of request.cdp) {
                    await this.cdp.send('Debugger.removeBreakpoint', { breakpointId: id }).catch(() => undefined);
                }
            }
        }
        const requests: BreakpointRequest[] = (args.breakpoints ?? []).map((breakpoint: any) => ({
            id: this.nextBreakpointId++,
            line: breakpoint.line,
            condition: breakpoint.condition || undefined,
            cdp: [],
        }));
        this.breakpoints.set(key, { path: file, requests });
        if (this.cdp && this.paused === undefined && this.initialPause !== undefined) {
            // Still starting: `attach` binds them at the first line.
        } else if (this.cdp) {
            await this.bind(key);
        }
        return requests.map((request) => this.describe(request, file));
    }

    private describe(request: BreakpointRequest, file: string): unknown {
        return request.bound !== undefined
            ? { id: request.id, verified: true, line: request.bound, source: { path: file } }
            : { id: request.id, verified: false, line: request.line, source: { path: file } };
    }

    private async bindAll(): Promise<void> {
        for (const key of this.breakpoints.keys()) {
            await this.bind(key);
        }
    }

    /// Binds one file's breakpoints in every script that maps it, and tells
    /// the editor where each landed.
    private async bind(key: string): Promise<void> {
        const entry = this.breakpoints.get(key);
        const cdp = this.cdp;
        if (!entry || !cdp) {
            return;
        }
        for (const request of entry.requests) {
            const wasBound = request.bound;
            // Bound again (the first line, then `configurationDone`; a new
            // watch round): what this connection holds goes first.
            for (const id of request.cdp) {
                await cdp.send('Debugger.removeBreakpoint', { breakpointId: id }).catch(() => undefined);
            }
            request.cdp = [];
            request.bound = undefined;
            for (const script of this.scripts.values()) {
                const source = script.map?.sourceIndex(entry.path);
                if (!script.map || source === undefined) {
                    continue;
                }
                const found = script.map.breakpointPositions(source, request.line - 1);
                if (!found) {
                    continue;
                }
                for (const position of found.positions) {
                    try {
                        const result = await cdp.send('Debugger.setBreakpoint', {
                            location: { scriptId: script.id, lineNumber: position.line, columnNumber: position.column },
                            condition: request.condition,
                        });
                        request.cdp.push(result.breakpointId);
                        request.bound = found.line + 1;
                    } catch {
                        // A position V8 cannot break at: the others still bind.
                    }
                }
            }
            if (request.bound !== wasBound) {
                this.event('breakpoint', { reason: 'changed', breakpoint: this.describe(request, entry.path) });
            }
        }
    }

    // --- Running and stopping ---------------------------------------------------

    private async resume(method: string, report = true): Promise<void> {
        this.clearPause();
        if (report) {
            this.event('continued', { threadId: THREAD_ID, allThreadsContinued: true });
        }
        await this.cdp?.send(method).catch(() => undefined);
    }

    private async step(kind: 'next' | 'stepIn' | 'stepOut'): Promise<void> {
        const top = this.paused?.callFrames[0];
        this.stepping = {
            kind,
            from: top ? this.lineKey(top) : undefined,
            depth: this.paused?.callFrames.length ?? 0,
            budget: 200,
        };
        await this.resume(cdpStep(kind), false);
    }

    private clearPause(): void {
        this.paused = undefined;
        this.frames.clear();
        this.functions.clear();
        this.handles.clear();
        this.nextHandle = 1;
    }

    private async onPaused(params: any): Promise<void> {
        const callFrames: any[] = params.callFrames ?? [];
        if (this.initialPause) {
            const resolve = this.initialPause;
            this.initialPause = undefined;
            resolve();
            return;
        }
        const top = callFrames[0];
        if (this.entryPending) {
            if (top && !this.isUserLocation(top) && this.stepBudget()) {
                await this.cdp?.send('Debugger.stepOver').catch(() => undefined);
                return;
            }
            this.entryPending = false;
            this.stop(params, 'entry');
            return;
        }
        const stepping = this.stepping;
        const hit: string[] = params.hitBreakpoints ?? [];
        if (stepping && hit.length === 0 && params.reason !== 'exception' && params.reason !== 'promiseRejection') {
            const user = top && this.isUserLocation(top);
            const sameLine =
                user &&
                stepping.kind !== 'stepOut' &&
                callFrames.length === stepping.depth &&
                this.lineKey(top) === stepping.from;
            if ((!user || sameLine) && --stepping.budget > 0) {
                // Generated code with no `.vl` place, or the line the step
                // started on: keep going, as one vilan step.
                const next = !user && stepping.kind === 'stepOut' ? 'Debugger.stepOut' : cdpStep(stepping.kind === 'stepOut' ? 'next' : stepping.kind);
                await this.cdp?.send(next).catch(() => undefined);
                return;
            }
        }
        this.stepping = undefined;
        let reason = 'step';
        if (params.reason === 'exception' || params.reason === 'promiseRejection') {
            reason = 'exception';
        } else if (hit.length > 0) {
            reason = 'breakpoint';
        } else if (this.pauseRequested) {
            reason = 'pause';
        } else if (!stepping) {
            reason = params.reason === 'other' ? 'debugger statement' : 'pause';
        }
        this.pauseRequested = false;
        this.stop(params, reason);
    }

    private entrySteps = 0;

    private stepBudget(): boolean {
        return ++this.entrySteps < 10000;
    }

    private stop(params: any, reason: string): void {
        this.clearPause();
        this.paused = {
            callFrames: params.callFrames ?? [],
            asyncStackTrace: params.asyncStackTrace,
            reason,
            data: params.data,
            hit: params.hitBreakpoints ?? [],
        };
        const text = reason === 'exception' ? params.data?.description?.split('\n')[0] : undefined;
        this.event('stopped', {
            reason,
            threadId: THREAD_ID,
            allThreadsStopped: true,
            text,
            description: text,
        });
    }

    /// `source:line` of a frame's `.vl` place, for "same line" while stepping.
    private lineKey(callFrame: any): string | undefined {
        const script = this.scripts.get(callFrame.location?.scriptId);
        const original = script?.map?.original({
            line: callFrame.location.lineNumber,
            column: callFrame.location.columnNumber ?? 0,
        });
        return original ? `${original.source}:${original.line}` : undefined;
    }

    /// Whether a frame stands in the user's own code — mapped, and not into
    /// an ignore-listed source.
    private isUserLocation(callFrame: any): boolean {
        const script = this.scripts.get(callFrame.location?.scriptId);
        if (!script?.map) {
            return false;
        }
        const original = script.map.original({
            line: callFrame.location.lineNumber,
            column: callFrame.location.columnNumber ?? 0,
        });
        return original !== undefined && script.map.sources[original.source]?.ignored !== true;
    }

    /// Stops the program: its node first, then `vilan` — under `--watch` by
    /// its own interrupt, which removes the round's script and map before it
    /// exits; a plain `vilan run` exits by itself once its node has, and is
    /// stopped only if it outlives that.
    private kill(): void {
        if (this.killed) {
            return;
        }
        this.killed = true;
        this.terminated = true;
        this.cdp?.close();
        signal(this.nodePid, 'SIGTERM');
        const vilan = this.vilanPid;
        if (vilan !== undefined && this.launch?.watch) {
            signal(vilan, process.platform === 'win32' ? 'SIGTERM' : 'SIGINT');
        }
        const child = this.child;
        setTimeout(() => {
            signal(vilan, 'SIGTERM');
            child?.kill();
        }, 2000).unref();
    }

    private killed = false;

    private finish(): void {
        if (this.finished) {
            return;
        }
        this.finished = true;
        this.terminated = true;
        this.event('terminated');
    }

    private finished = false;

    // --- The stack ----------------------------------------------------------------

    private stackTrace(args: any): unknown {
        const callFrames = this.paused?.callFrames ?? [];
        const stackFrames = callFrames.map((callFrame, index) => this.stackFrame(index + 1, callFrame, false));
        stackFrames.push(...this.awaitingFrames(stackFrames.length + 1));
        const start = args.startFrame ?? 0;
        const levels = args.levels || stackFrames.length;
        return { stackFrames: stackFrames.slice(start, start + levels), totalFrames: stackFrames.length };
    }

    /// E326: the async parents below the stack the program stands in — node's
    /// `asyncStackTrace` (asked for at attach, `setAsyncCallStackDepth`): each
    /// segment under a label frame naming it (`await`, or the task a queued
    /// reactive wake ran in, S8), its frames mapped as the synchronous ones
    /// are. A segment that never reaches a script with a map is node's own —
    /// its module loader awaiting the program — and is left out.
    private awaitingFrames(firstId: number): unknown[] {
        const frames: unknown[] = [];
        let id = firstId;
        for (let segment = this.paused?.asyncStackTrace; segment; segment = segment.parent) {
            const callFrames: any[] = segment.callFrames ?? [];
            if (!callFrames.some((callFrame) => this.scripts.get(callFrame.scriptId)?.map)) {
                continue;
            }
            frames.push({
                id: id++,
                name: segment.description || 'async',
                line: 0,
                column: 0,
                presentationHint: 'label',
            });
            for (const callFrame of callFrames) {
                // A `Runtime.CallFrame` is flat; the paused stack's frames
                // nest their place under `location`.
                const located = {
                    functionName: callFrame.functionName,
                    url: callFrame.url,
                    location: {
                        scriptId: callFrame.scriptId,
                        lineNumber: callFrame.lineNumber,
                        columnNumber: callFrame.columnNumber,
                    },
                };
                frames.push(this.stackFrame(id++, located, true));
            }
        }
        return frames;
    }

    /// One frame of the Call Stack, mapped to its `.vl` place and named by its
    /// sidecar record where one resolves.
    private stackFrame(id: number, callFrame: any, awaiting: boolean): unknown {
        const script = this.scripts.get(callFrame.location.scriptId);
        const info: FrameInfo = { callFrame, script, awaiting };
        const generated = { line: callFrame.location.lineNumber, column: callFrame.location.columnNumber ?? 0 };
        const original = script?.map?.original(generated);
        const topLevel =
            !awaiting &&
            !callFrame.functionName &&
            (callFrame.functionLocation?.lineNumber ?? 0) === 0 &&
            (callFrame.functionLocation?.columnNumber ?? 0) === 0;
        if (script?.sidecar && script.map) {
            if (awaiting) {
                // No function start to go by: the place it stands in is.
                info.resolved = script.sidecar.resolveAt(callFrame.functionName ?? '', original);
            } else {
                const head = callFrame.functionLocation
                    ? script.map.original({
                          line: callFrame.functionLocation.lineNumber,
                          column: callFrame.functionLocation.columnNumber ?? 0,
                      })
                    : undefined;
                info.resolved = script.sidecar.resolve(callFrame.functionName ?? '', head, topLevel);
            }
        }
        this.frames.set(id, info);
        const name =
            info.resolved?.name ??
            this.mapFrameName(script, callFrame, generated, awaiting, topLevel) ??
            (callFrame.functionName || (topLevel ? 'main' : '(anonymous)'));
        if (original && script?.map) {
            const source = script.map.sources[original.source];
            return {
                id,
                name,
                source: this.dapSource(source.path, source.url, source.content, source.ignored),
                line: original.line + 1,
                column: original.column + 1,
                presentationHint: source.ignored ? 'subtle' : 'normal',
            };
        }
        const file = callFrame.url?.startsWith('file:') ? safePath(callFrame.url) : undefined;
        return {
            id,
            name,
            source: file ? { name: path.basename(file), path: file } : undefined,
            line: generated.line + 1,
            column: generated.column + 1,
            presentationHint: 'subtle',
        };
    }

    /// E336: the name of a frame the sidecar has no record for (it describes
    /// the USER's functions; std's frames are the ones that fall here), from
    /// the source map. S5 marks every function head - std's included - and
    /// carries the function's name there, so a frame whose head is marked is
    /// named by it; a head marked with no name is a closure's, spelled `a
    /// closure in <the function holding it>` as the sidecar spells a user's.
    /// V8's own `functionName` infers from the nearest assignment (a closure
    /// std passed to `with_finally` read `running_wake.v`), so it stays only
    /// for what the map does not describe: the runtime helpers, a script with
    /// no map, and an awaiting frame, which carries no function start (it is
    /// named by V8 unless that is empty, then by the function holding its
    /// place).
    private mapFrameName(
        script: ScriptInfo | undefined,
        callFrame: any,
        generated: { line: number; column: number },
        awaiting: boolean,
        topLevel: boolean,
    ): string | undefined {
        const map = script?.map;
        if (!map || topLevel) {
            return undefined;
        }
        if (awaiting) {
            if (callFrame.functionName) {
                return undefined;
            }
            const at = map.original(generated);
            return at && !map.sources[at.source]?.url.startsWith('vilan:') ? this.closureIn(map, generated) : undefined;
        }
        const location = callFrame.functionLocation;
        if (!location) {
            return undefined;
        }
        const head = { line: location.lineNumber, column: location.columnNumber ?? 0 };
        const segment = map.segmentAt(head);
        if (!segment || map.sources[segment.source]?.url.startsWith('vilan:')) {
            return undefined;
        }
        return segment.name !== undefined ? map.names[segment.name] : this.closureIn(map, head);
    }

    private closureIn(map: SourceMap, place: { line: number; column: number }): string | undefined {
        const owner = map.enclosingFunctionName(place);
        return owner === undefined ? undefined : `a closure in ${owner}`;
    }

    private dapSource(file: string | undefined, url: string, content: string | undefined, ignored: boolean): unknown {
        const hint = ignored ? 'deemphasize' : undefined;
        if (file) {
            return { name: path.basename(file), path: file, presentationHint: hint };
        }
        let reference = this.sourceReferences.get(url);
        if (reference === undefined) {
            reference = this.sourceReferences.size + 1;
            this.sourceReferences.set(url, reference);
            this.sources.set(reference, content ?? '');
        }
        return { name: url, sourceReference: reference, presentationHint: hint };
    }

    private source(args: any): unknown {
        const reference = args.sourceReference ?? args.source?.sourceReference;
        return { content: this.sources.get(reference) ?? '' };
    }

    // --- Scopes and variables ---------------------------------------------------

    private handle(handle: Handle): number {
        const id = this.nextHandle++;
        this.handles.set(id, handle);
        return id;
    }

    private async scopes(frameId: number): Promise<unknown[]> {
        const info = this.frames.get(frameId);
        if (!info || info.awaiting) {
            // A label, or an awaiting frame: V8 keeps no scopes for either.
            return [];
        }
        const chain: any[] = info.callFrame.scopeChain ?? [];
        const scopes: unknown[] = [];
        const sidecar = info.script?.sidecar;
        if (info.resolved && sidecar) {
            const own = chain.filter((scope) => scope.type === 'local' || scope.type === 'block' || scope.type === 'catch');
            const main = info.resolved.kind === 'main';
            const objects = (main ? [...own, ...chain.filter((scope) => scope.type === 'module')] : own).map(
                (scope) => scope.object.objectId,
            );
            scopes.push({
                name: 'Locals',
                presentationHint: 'locals',
                variablesReference: this.handle({
                    kind: 'scope',
                    objects,
                    frame: info.resolved,
                    sidecar,
                    main,
                    callFrameId: info.callFrame.callFrameId,
                }),
                expensive: false,
            });
            for (const scope of chain.filter((candidate) => candidate.type === 'closure')) {
                const owner = this.closureOwner(info.script, scope);
                scopes.push({
                    name: owner ? `Captured from ${owner.name}` : `Closure${scope.name ? ` (${scope.name})` : ''}`,
                    variablesReference: this.handle(
                        owner
                            ? {
                                  kind: 'scope',
                                  objects: [scope.object.objectId],
                                  frame: owner,
                                  sidecar,
                                  main: false,
                                  callFrameId: info.callFrame.callFrameId,
                              }
                            : { kind: 'object', objectId: scope.object.objectId },
                    ),
                    expensive: false,
                });
            }
            for (const scope of chain.filter((candidate) => candidate.type === 'module' || candidate.type === 'global')) {
                scopes.push({
                    name: scope.type === 'module' ? 'Module (JavaScript)' : 'Global',
                    variablesReference: this.handle({ kind: 'object', objectId: scope.object.objectId }),
                    expensive: true,
                });
            }
            return scopes;
        }
        for (const scope of chain) {
            scopes.push({
                name: scopeTitle(scope.type, scope.name),
                variablesReference: this.handle({ kind: 'object', objectId: scope.object.objectId }),
                expensive: scope.type === 'global' || scope.type === 'module',
            });
        }
        return scopes;
    }

    /// The record of the function a closure scope's variables belong to.
    private closureOwner(script: ScriptInfo | undefined, scope: any): ResolvedFrame | undefined {
        if (!script?.sidecar || !script.map || !scope.startLocation) {
            return undefined;
        }
        const head = script.map.original({
            line: scope.startLocation.lineNumber,
            column: scope.startLocation.columnNumber ?? 0,
        });
        const topLevel = scope.startLocation.lineNumber === 0 && (scope.startLocation.columnNumber ?? 0) === 0;
        return script.sidecar.resolve(scope.name ?? '', head, topLevel);
    }

    private async variables(reference: number): Promise<unknown[]> {
        const handle = this.handles.get(reference);
        if (!handle || !this.cdp) {
            return [];
        }
        switch (handle.kind) {
            case 'doc':
                return children(handle.doc).map((child) => ({
                    name: child.name,
                    value: flat(child.document),
                    variablesReference: expandable(child.document)
                        ? this.handle({
                              kind: 'doc',
                              doc: child.document,
                              path: handle.path && child.path ? handle.path + child.path : undefined,
                          })
                        : 0,
                    evaluateName: handle.path && child.path ? handle.path + child.path : undefined,
                }));
            case 'object':
                return this.javascriptProperties(handle.objectId);
            case 'scope':
                return this.scopeVariables(handle);
        }
    }

    /// A frame's bindings, vilan-shaped where the sidecar has a row and the
    /// value reads by it, JavaScript-shaped otherwise.
    private async scopeVariables(handle: Extract<Handle, { kind: 'scope' }>): Promise<unknown[]> {
        const cdp = this.cdp;
        if (!cdp) {
            return [];
        }
        // Innermost first: a shadowed name is the inner scope's.
        const properties = new Map<string, { value: any; owner: string }>();
        for (const objectId of handle.objects) {
            const result = await cdp.send('Runtime.getProperties', { objectId, ownProperties: true });
            for (const property of result.result ?? []) {
                if (!properties.has(property.name)) {
                    properties.set(property.name, { value: property.value, owner: objectId });
                }
            }
        }
        const variables: unknown[] = [];
        const shown = new Set<string>();
        for (const binding of handle.frame?.bindings ?? []) {
            const property = properties.get(binding.js);
            if (!property || shown.has(binding.js)) {
                continue;
            }
            shown.add(binding.js);
            variables.push(
                await this.bindingVariable(binding, property.value, property.owner, handle.sidecar, handle.callFrameId),
            );
        }
        if (!handle.main) {
            // Bindings the sidecar has no row for — what the compiler bound
            // for itself (`$a`) is not the user's and stays out.
            for (const [name, property] of properties) {
                if (!shown.has(name) && !name.startsWith('$')) {
                    variables.push(this.javascriptVariable(name, property.value));
                }
            }
        }
        return variables;
    }

    private async bindingVariable(
        binding: SidecarBinding,
        value: any,
        context: string,
        sidecar: Sidecar | undefined,
        callFrameId: string | undefined,
    ): Promise<unknown> {
        if (sidecar && binding.type !== undefined) {
            if ((!value || value.type === 'undefined') && sidecar.types[binding.type]?.shape !== 'void') {
                // A `let` the program has not reached yet.
                return { name: binding.name, value: '<not yet initialized>', type: sidecar.typeText(binding), variablesReference: 0 };
            }
            const document = await this.walk(value, binding.type, binding.read ?? 'value', context, sidecar, callFrameId);
            if (document !== undefined) {
                return {
                    name: binding.name,
                    value: flat(document),
                    type: sidecar.typeText(binding),
                    evaluateName: binding.name,
                    variablesReference: expandable(document)
                        ? this.handle({ kind: 'doc', doc: document, path: binding.name })
                        : 0,
                };
            }
        }
        if (binding.read === 'lazy') {
            return { name: binding.name, value: '<lazy, not forced>', variablesReference: 0 };
        }
        return { ...this.javascriptVariable(binding.name, value), evaluateName: binding.name };
    }

    /// Reads `value` (a CDP remote object) by the sidecar's type `type` in the
    /// paused program, or `undefined` where the walker refused it.
    private async walk(
        value: any,
        type: number,
        read: string,
        context: string,
        sidecar: Sidecar,
        callFrameId?: string,
    ): Promise<Doc | undefined> {
        const cdp = this.cdp;
        if (!cdp) {
            return undefined;
        }
        const table = sidecar.table(type);
        // E323: a type a written `Debug` impl prints is handed to the walker
        // with the program's own function for it — the first of the sidecar's
        // names the paused frame reaches — as an extra argument.
        const printers: string[] = [];
        const types = [];
        for (const layout of table.types) {
            let printer: string | undefined;
            for (const name of layout.debug ?? []) {
                printer = await this.programFunction(name, callFrameId);
                if (printer) {
                    break;
                }
            }
            if (printer) {
                types.push({ ...layout, fn: printers.length });
                printers.push(printer);
            } else {
                types.push(layout);
            }
        }
        try {
            const result = await cdp.send('Runtime.callFunctionOn', {
                functionDeclaration: WALKER,
                objectId: context,
                arguments: [
                    callArgument(value),
                    { value: types },
                    { value: table.root },
                    { value: read },
                    ...printers.map((objectId) => ({ objectId })),
                ],
                returnByValue: true,
                silent: true,
            });
            if (result.exceptionDetails || result.result?.value === undefined) {
                return undefined;
            }
            return result.result.value as Doc;
        } catch {
            return undefined;
        }
    }

    /// The program's function `name` as the paused frame `callFrameId` (the
    /// top frame when absent) sees it, or `undefined`: a module-scope function
    /// is reachable only where V8 context-allocated it — when some other
    /// function refers to it — so a name that does not resolve leaves the
    /// structure in charge (E323). Asked once per name per stop.
    private async programFunction(name: string, callFrameId: string | undefined): Promise<string | undefined> {
        const frame = callFrameId ?? this.paused?.callFrames[0]?.callFrameId;
        if (!frame || !this.cdp || !/^[A-Za-z_$][\w$]*$/.test(name)) {
            return undefined;
        }
        const key = `${frame}\u0000${name}`;
        if (this.functions.has(key)) {
            return this.functions.get(key) ?? undefined;
        }
        const result = await this.cdp
            .send('Debugger.evaluateOnCallFrame', { callFrameId: frame, expression: name, silent: true })
            .catch(() => undefined);
        const found = !result?.exceptionDetails && result?.result?.type === 'function' ? result.result.objectId : undefined;
        this.functions.set(key, found ?? null);
        return found;
    }

    private functions = new Map<string, string | null>();

    private async javascriptProperties(objectId: string): Promise<unknown[]> {
        const result = await this.cdp?.send('Runtime.getProperties', { objectId, ownProperties: true });
        const variables: unknown[] = [];
        for (const property of result?.result ?? []) {
            if (property.value) {
                variables.push(this.javascriptVariable(property.name, property.value));
            }
        }
        for (const property of result?.internalProperties ?? []) {
            if (property.value) {
                variables.push(this.javascriptVariable(property.name, property.value));
            }
        }
        return variables;
    }

    private javascriptVariable(name: string, value: any): { name: string; value: string; type?: string; variablesReference: number } {
        return {
            name,
            value: describe(value),
            type: value?.className ?? value?.type,
            variablesReference:
                value?.objectId && value.type === 'object' && value.subtype !== 'null'
                    ? this.handle({ kind: 'object', objectId: value.objectId })
                    : 0,
        };
    }

    // --- Watch, hover and the console --------------------------------------------

    private async evaluate(expression: string, frameId: number | undefined, context: string | undefined): Promise<unknown> {
        const cdp = this.cdp;
        if (!cdp) {
            throw new Error('the program is not running');
        }
        const info = frameId !== undefined ? this.frames.get(frameId) : undefined;
        if (info?.awaiting) {
            throw new Error('an awaiting frame keeps no values to evaluate in: pick a frame above the first label');
        }
        if (info) {
            const vilan = await this.evaluatePath(expression, info, context);
            if (vilan) {
                return vilan;
            }
        }
        const result = info
            ? await cdp.send('Debugger.evaluateOnCallFrame', {
                  callFrameId: info.callFrame.callFrameId,
                  expression,
                  silent: context !== 'repl',
              })
            : await cdp.send('Runtime.evaluate', { expression, silent: context !== 'repl' });
        if (result.exceptionDetails) {
            throw new Error(result.exceptionDetails.exception?.description?.split('\n')[0] ?? 'evaluation failed');
        }
        const variable = this.javascriptVariable(expression, result.result);
        return { result: variable.value, type: variable.type, variablesReference: variable.variablesReference };
    }

    /// `point.from.x`: a binding of the frame and the steps after it, read by
    /// the sidecar's types and printed as `dbg` prints it — on one line for
    /// the Watch pane, laid out for a hover and the console.
    private async evaluatePath(expression: string, info: FrameInfo, context: string | undefined): Promise<unknown> {
        const sidecar = info.script?.sidecar;
        const parsed = parsePath(expression);
        const cdp = this.cdp;
        if (!sidecar || !info.resolved || !parsed || !cdp) {
            return undefined;
        }
        const candidates = info.resolved.bindings.filter((binding) => binding.name === parsed.root);
        if (candidates.length === 0) {
            return undefined;
        }
        // Which of the same-named bindings the frame can see: the innermost.
        const visible = await this.visibleBinding(info, candidates.map((binding) => binding.js));
        const binding = candidates.find((candidate) => candidate.js === visible);
        if (!binding || binding.type === undefined || binding.read === 'lazy') {
            return undefined;
        }
        let access = binding.read === 'boxed' ? `${binding.js}[0]` : binding.read === 'pair' ? `${binding.js}[0][${binding.js}[1]]` : binding.js;
        let type = binding.type;
        for (const segment of parsed.segments) {
            const step = sidecar.step(type, segment);
            if (!step) {
                return undefined;
            }
            access += step.access;
            type = step.type;
        }
        const result = await cdp.send('Debugger.evaluateOnCallFrame', {
            callFrameId: info.callFrame.callFrameId,
            expression: access,
            silent: true,
        });
        if (result.exceptionDetails) {
            return undefined;
        }
        const scopeObject = info.callFrame.scopeChain?.[0]?.object?.objectId;
        if (!scopeObject) {
            return undefined;
        }
        const document = await this.walk(result.result, type, 'value', scopeObject, sidecar, info.callFrame.callFrameId);
        if (document === undefined) {
            return undefined;
        }
        const text = context === 'hover' || context === 'repl' || context === 'clipboard' ? layout(document, 0, 0) : flat(document);
        return {
            result: text,
            type: sidecar.types[type]?.text,
            variablesReference: expandable(document) ? this.handle({ kind: 'doc', doc: document, path: expression.trim() }) : 0,
        };
    }

    /// The first of `names` the frame's scope chain holds, innermost first.
    private async visibleBinding(info: FrameInfo, names: string[]): Promise<string | undefined> {
        if (names.length === 1) {
            return names[0];
        }
        for (const scope of info.callFrame.scopeChain ?? []) {
            if (scope.type === 'global') {
                continue;
            }
            const result = await this.cdp?.send('Runtime.getProperties', {
                objectId: scope.object.objectId,
                ownProperties: true,
            });
            const found = names.find((name) => (result?.result ?? []).some((property: any) => property.name === name));
            if (found) {
                return found;
            }
        }
        return undefined;
    }
}

function cdpStep(kind: 'next' | 'stepIn' | 'stepOut'): string {
    return kind === 'next' ? 'Debugger.stepOver' : kind === 'stepIn' ? 'Debugger.stepInto' : 'Debugger.stepOut';
}

/// A CDP `CallArgument` for a remote object.
function callArgument(value: any): Record<string, unknown> {
    if (!value) {
        return {};
    }
    if (value.objectId) {
        return { objectId: value.objectId };
    }
    if (value.unserializableValue !== undefined) {
        return { unserializableValue: value.unserializableValue };
    }
    if (value.type === 'undefined') {
        return {};
    }
    return { value: value.value };
}

/// A remote object as JavaScript prints it in a pane.
export function describe(value: any): string {
    if (!value) {
        return 'undefined';
    }
    switch (value.type) {
        case 'undefined':
            return 'undefined';
        case 'string':
            return JSON.stringify(value.value);
        case 'number':
        case 'boolean':
            return value.unserializableValue ?? String(value.value);
        case 'bigint':
            return value.unserializableValue ?? String(value.value);
        case 'function':
            return (value.description ?? 'function').split('\n')[0];
        case 'object':
            return value.subtype === 'null' ? 'null' : (value.description ?? value.className ?? 'Object');
        default:
            return value.description ?? String(value.value);
    }
}

function scopeTitle(type: string, name: string | undefined): string {
    const title = type.charAt(0).toUpperCase() + type.slice(1);
    return name ? `${title} (${name})` : title;
}

function safePath(url: string): string | undefined {
    try {
        return fileURLToPath(url);
    } catch {
        return undefined;
    }
}

/// Sends `name` to `pid` if it is still running.
function signal(pid: number | undefined, name: NodeJS.Signals): void {
    if (pid === undefined || !alive(pid)) {
        return;
    }
    try {
        process.kill(pid, name);
    } catch {
        // Gone in between.
    }
}

/// `value` when it is a process id, else `undefined` — never a `null` or a
/// `NaN` that `process.kill` would refuse, and so never one a liveness check
/// could read as a process that ended (E331).
export function validPid(value: unknown): number | undefined {
    return typeof value === 'number' && Number.isInteger(value) && value > 0 ? value : undefined;
}

function alive(pid: number): boolean {
    try {
        process.kill(pid, 0);
        return true;
    } catch {
        return false;
    }
}

function delay(milliseconds: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

/// A free loopback port, released for the inspector to take.
export function freePort(): Promise<number> {
    return new Promise((resolve, reject) => {
        const server = net.createServer();
        server.once('error', reject);
        server.listen(0, INSPECT_HOST, () => {
            const address = server.address();
            const port = typeof address === 'object' && address ? address.port : 9229;
            server.close(() => resolve(port));
        });
    });
}
