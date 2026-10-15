// E257 S7: a scripted DAP client over vilan-dap, in-process — the editor's
// half of a debug session, as `src/test/session.test.ts` drives it.
import { ChildProcess, spawn } from 'child_process';
import { DapMessage, VilanDebugAdapter } from '../dap/adapter';

export class DapClient {
    readonly adapter = new VilanDebugAdapter();
    readonly events: DapMessage[] = [];
    readonly output: string[] = [];
    private seq = 1;
    private readonly waiting = new Map<number, (message: DapMessage) => void>();
    private readonly watchers: {
        test: (message: DapMessage) => boolean;
        resolve: (message: DapMessage) => void;
        timer: NodeJS.Timeout;
    }[] = [];
    /// What the editor's terminal ran for a `runInTerminal` request, and what
    /// it printed.
    readonly terminal: { process?: ChildProcess; args?: string[]; output: string } = { output: '' };

    constructor() {
        this.adapter.onDidSendMessage((message) => this.receive(message));
    }

    private receive(message: DapMessage): void {
        if (process.env.VILAN_DAP_TRACE) {
            process.stderr.write(`<- ${JSON.stringify(message).slice(0, 300)}\n`);
        }
        if (message.type === 'request' && message.command === 'runInTerminal') {
            // The editor's half: run the command in a "terminal" — here a
            // process whose output the test reads — and answer its pid.
            const args: string[] = message.arguments.args;
            const child = spawn(args[0], args.slice(1), {
                cwd: message.arguments.cwd,
                env: { ...process.env, ...(message.arguments.env ?? {}) },
                stdio: ['ignore', 'pipe', 'pipe'],
            });
            child.stdout?.on('data', (chunk: Buffer) => (this.terminal.output += chunk.toString()));
            child.stderr?.on('data', (chunk: Buffer) => (this.terminal.output += chunk.toString()));
            this.terminal.process = child;
            this.terminal.args = args;
            this.adapter.handleMessage({
                seq: this.seq++,
                type: 'response',
                request_seq: message.seq,
                command: 'runInTerminal',
                success: true,
                body: { processId: child.pid },
            });
            return;
        }
        if (message.type === 'response') {
            this.waiting.get(message.request_seq ?? -1)?.(message);
            this.waiting.delete(message.request_seq ?? -1);
            return;
        }
        if (message.type === 'event') {
            if (message.event === 'output') {
                this.output.push(message.body.output);
            }
            this.events.push(message);
            for (const watcher of [...this.watchers]) {
                if (watcher.test(message)) {
                    this.watchers.splice(this.watchers.indexOf(watcher), 1);
                    watcher.resolve(message);
                }
            }
        }
    }

    /// Sends a request and answers its response's body; a failed response throws.
    async request(command: string, args?: unknown): Promise<any> {
        const seq = this.seq++;
        const response = new Promise<DapMessage>((resolve) => this.waiting.set(seq, resolve));
        this.adapter.handleMessage({ seq, type: 'request', command, arguments: args });
        const answered = await response;
        if (!answered.success) {
            throw new Error(`${command}: ${answered.message}`);
        }
        return answered.body;
    }

    /// The next event named `name` (that `test` accepts), from now on.
    next(name: string, test: (message: DapMessage) => boolean = () => true, timeout = 60000): Promise<DapMessage> {
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error(`no \`${name}\` event within ${timeout} ms`)), timeout);
            this.watchers.push({
                test: (message) => message.event === name && test(message),
                resolve: (message) => {
                    clearTimeout(timer);
                    resolve(message);
                },
                timer,
            });
        });
    }

    /// Ends the session: the adapter lets the program go, and every wait still
    /// open is dropped (a session that ended stops nowhere again).
    async close(): Promise<void> {
        await this.request('disconnect', {}).catch(() => undefined);
        for (const watcher of this.watchers.splice(0)) {
            clearTimeout(watcher.timer);
        }
        // The "terminal" outlives the session, as an editor's does: `vilan
        // run` removes its temp script once its node has gone. Stopped only
        // if it has not ended a few seconds after the adapter let go.
        const terminal = this.terminal.process;
        if (terminal && terminal.exitCode === null && terminal.signalCode === null) {
            const exited = new Promise<void>((resolve) => terminal.once('exit', () => resolve()));
            const timer = setTimeout(() => terminal.kill(), 5000);
            await exited;
            clearTimeout(timer);
        }
    }

    /// Everything the program wrote to stderr so far.
    stderr(): string {
        return this.events
            .filter((event) => event.event === 'output' && event.body.category === 'stderr')
            .map((event) => event.body.output)
            .join('');
    }

    stdout(): string {
        return this.events
            .filter((event) => event.event === 'output' && event.body.category === 'stdout')
            .map((event) => event.body.output)
            .join('');
    }
}
