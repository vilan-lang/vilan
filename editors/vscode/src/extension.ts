import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
    workspace,
    window,
    commands,
    CancellationToken,
    CodeAction,
    CodeActionKind,
    Disposable,
    FileSystemWatcher,
    LogOutputChannel,
    ExtensionContext,
    Range,
    Selection,
    StatusBarAlignment,
    StatusBarItem,
    TextDocument,
    TextDocumentChangeEvent,
    TextEdit,
    TextEditor,
} from 'vscode';
import {
    DidChangeConfigurationNotification,
    ExecuteCommandRequest,
    LanguageClient,
    LanguageClientOptions,
    MessageSignature,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let outputChannel: LogOutputChannel | undefined;

// --- E106: session instrumentation ---------------------------------------
//
// The owner reports the language server "slowing down quite a bit" over a
// working session, and the decisive datapoint is that `Vilan: Restart Language
// Server` does NOT fix it while restarting VS Code does. That datapoint only
// means something once the restart command is known to really replace the
// server, so that was checked first: `startClient` stops the old client and
// constructs a NEW `LanguageClient`, and `vscode-languageclient`'s node layer
// spawns a fresh process for it (and SIGTERMs the old one two seconds later if
// it has not exited). So a restart genuinely resets everything living inside the
// server process — which is what moves the suspicion to state that OUTLIVES the
// client, here in the extension host.
//
// One such leak is fixed below (see `sharedFileWatcher`). The counters here are
// how the next one gets attributed rather than guessed at: every request is
// timed at the client edge, so "slow" can be read as a number per method, and
// the per-restart bookkeeping is logged instead of being silently swallowed.
//
// Everything is logged to the extension's own output channel (Vilan Language
// Server), and `Vilan: Show Language Server Status` dumps the tally on demand.

/// A single request round trip is called slow past this, and says so in the
/// output channel with its method and duration. Chosen well above an ordinary
/// completion or hover on a large file, so a quiet session logs nothing.
const SLOW_REQUEST_MS = 400;

/// Round-trip timings for one method, as measured at the client edge — so it
/// includes transport and the extension host's own scheduling, which is what
/// the user actually waits for.
interface RequestStat {
    count: number;
    totalMilliseconds: number;
    maxMilliseconds: number;
}

const requestStats = new Map<string, RequestStat>();

/// How many times a server process has been started this session. A session
/// with more than one has exercised the restart path, which is the path that
/// used to leak a file watcher every time.
let serverStarts = 0;

/// When this extension host activated — the span every count below covers.
const sessionStarted = Date.now();

/// The ONE `**/*.vl` watcher this session owns.
///
/// This used to be `workspace.createFileSystemWatcher('**/*.vl')` written inline
/// in the client options, evaluated afresh on every `startClient` call. The
/// client does not own a watcher handed to it that way: `FileSystemWatcherFeature`
/// hooks `onDidCreate`/`onDidChange`/`onDidDelete` listeners and, on stop,
/// disposes THE LISTENERS — the `FileSystemWatcher` itself is the caller's to
/// dispose, and nothing disposed it. So every restart (and every `vilan.server.path`
/// or `vilan.stdPath` change) left one more workspace-recursive watcher running
/// in the extension host, for the lifetime of the window.
///
/// That is exactly the shape of the owner's datapoint: restarting the server
/// could not clear it — restarting the server was what CREATED it — and only
/// reloading the window did. Created once and registered with the extension's
/// subscriptions, it is now reused by every client: the stop disposes the old
/// client's listeners and the new client hooks its own.
let fileWatcher: FileSystemWatcher | undefined;

function sharedFileWatcher(context: ExtensionContext): FileSystemWatcher {
    if (fileWatcher === undefined) {
        // Sources AND manifests: the server acts on both
        // (`did_change_watched_files`, E127) — a `vilan.toml` arriving with a
        // checkout re-colors every package file under it, and the server can
        // only know if the event reaches it. One watcher, one glob, so the
        // one-per-session accounting below still reads `1`.
        fileWatcher = workspace.createFileSystemWatcher('**/{*.vl,vilan.toml}');
        context.subscriptions.push(fileWatcher);
    }
    return fileWatcher;
}

/// The method name of a request, whichever spelling the client used.
function methodOf(type: string | MessageSignature): string {
    return typeof type === 'string' ? type : type.method;
}

/// Fold one round trip into the tally, and name it in the output channel when it
/// crosses `SLOW_REQUEST_MS`.
function recordRequest(method: string, milliseconds: number): void {
    const stat = requestStats.get(method) ?? {
        count: 0,
        totalMilliseconds: 0,
        maxMilliseconds: 0,
    };
    stat.count += 1;
    stat.totalMilliseconds += milliseconds;
    stat.maxMilliseconds = Math.max(stat.maxMilliseconds, milliseconds);
    requestStats.set(method, stat);
    if (milliseconds >= SLOW_REQUEST_MS) {
        outputChannel?.warn(
            `slow request: ${method} took ${milliseconds} ms ` +
                `(request ${stat.count} of this method; slowest so far ${stat.maxMilliseconds} ms)`,
        );
    }
}

/// The session tally, as lines. Ordered by total time spent, which is the order
/// that answers "what is the session actually waiting on".
function sessionStatusLines(context: ExtensionContext): string[] {
    const minutes = ((Date.now() - sessionStarted) / 60000).toFixed(1);
    const lines = [
        `session age: ${minutes} min`,
        `server starts this session: ${serverStarts}`,
        `client attached: ${client !== undefined}`,
        `file watchers created: ${fileWatcher === undefined ? 0 : 1} (one per session, never per restart)`,
        `extension subscriptions: ${context.subscriptions.length}`,
        'requests (count / mean ms / max ms), slowest total first:',
    ];
    const ordered = [...requestStats.entries()].sort(
        (left, right) => right[1].totalMilliseconds - left[1].totalMilliseconds,
    );
    if (ordered.length === 0) {
        lines.push('  (none yet)');
    }
    for (const [method, stat] of ordered) {
        const mean = (stat.totalMilliseconds / stat.count).toFixed(1);
        lines.push(`  ${method}: ${stat.count} / ${mean} / ${stat.maxMilliseconds}`);
    }
    return lines;
}

/// E174: the server's own `workspace/executeCommand`, spelled exactly as
/// `vilan-lsp`'s `LOG_SESSION_SUMMARY` declares it. The two spellings are gated
/// against each other by `book_sync`, which reads this file.
const LOG_SESSION_SUMMARY = 'vilan.logSessionSummary';

/// Ask the server to put ITS session summary on the channel now (E174).
///
/// E166 put the memory numbers — resident size, the heap in-use/retained-free
/// split, `programs=` — on that summary, and until this the only way to see
/// them was to wait for the server's 500-request tick: the status command
/// printed the CLIENT's tally and opened the channel the server's page would
/// eventually arrive on. They are read when a session starts feeling slow,
/// which is the moment the user runs the command.
///
/// The server logs the page rather than returning it, and its log goes to this
/// same output channel (the client is constructed with it), so the two tallies
/// land one after the other under one palette entry.
async function requestServerSessionSummary(): Promise<void> {
    if (!client) {
        outputChannel?.info('  server session summary: no client attached');
        return;
    }
    try {
        await client.sendRequest(ExecuteCommandRequest.type, { command: LOG_SESSION_SUMMARY });
    } catch (error) {
        outputChannel?.warn(
            `the server did not answer ${LOG_SESSION_SUMMARY} ` +
                `(${error instanceof Error ? error.message : String(error)}); ` +
                'its summary still arrives on its own every 500 requests',
        );
    }
}

/// Windows executables carry a `.exe` suffix and nothing else does, so every use
/// below collapses to exactly the pre-Windows expression on Linux and macOS
/// (`windows-support.md` §7). Without it the four discovery candidates never
/// match on Windows and the documented "developer builds outrank the release
/// install" ordering silently never holds.
const executableSuffix = process.platform === 'win32' ? '.exe' : '';
const serverBinary = `vilan-lsp${executableSuffix}`;

/// The install location the toolchain manages, as the user would type it.
const installDirectory = process.platform === 'win32' ? '%USERPROFILE%\\.vilan\\bin' : '~/.vilan/bin';

/// The feature settings the server reads (gating inlay hints / semantic tokens,
/// and — for WO-3 — completion's call style). Sent as `initializationOptions` at
/// startup and re-sent via `workspace/didChangeConfiguration` on change. Shaped
/// as the server's `Config::from_settings` expects. (`organizeImports.onSave` is
/// handled entirely on the client, so it is not included here.)
function readFeatureConfig(): object {
    const config = workspace.getConfiguration('vilan');
    return {
        inlayHints: { enabled: config.get<boolean>('inlayHints.enabled', true) },
        semanticTokens: { enabled: config.get<boolean>('semanticTokens.enabled', true) },
        completion: { functionCall: config.get<string>('completion.functionCall', 'full') },
        // E222: not the setting — whether the override is INSTALLED. The two
        // differ when another extension owns `type`, and then the server must
        // keep placing the `>` itself.
        autoClosing: { generics: typeOverride !== undefined },
    };
}

// --- E222: a generic `<` pairs, and its `>` types over ------------------------
//
// The server has placed a generic `<`'s `>` since E202 (`onTypeFormatting`),
// and that half cannot be finished from the server: VS Code types OVER a
// closing character only when it auto-inserted that character itself, and an
// edit cannot move the caret, so typing the `>` of `List<i32>` by hand gave
// `List<i32>>`. A static `autoClosingPairs` entry would buy the overtype and
// grow a `>` in every `a < b` — its `notIn` filter sees only string, comment
// and regex scopes, and the token left of the caret is `Other` for `List<` and
// for `a <` alike (editor-39 pinned exactly that).
//
// So the extension takes the keystroke. `type` is overridden, and the override
// is thin: every character is typed by VS Code's own `default:type` exactly as
// before, and only two are then looked at — a `<` asks the server
// `vilan/opensAGenericList` and, on a yes, places the `>` itself; a `>` typed
// onto a `>` this override placed moves the caret past it instead (the map of
// placed closers below IS the overtype). It declares itself to the server in
// `initializationOptions` (`autoClosing.generics`, via `readFeatureConfig`), and
// the server then stops answering `<` through `onTypeFormatting` — a client
// that never declares it keeps that answer.
//
// It is a SETTING (`vilan.autoClosing.generics`, on by default) because only
// one extension can own `type`: Vim emulation (VSCodeVim) takes it for its own
// modes. With the setting off — or when another extension already owns the
// command and registering fails — the override is absent, the server is told
// so, and the `<` behaves as it did before this.

/// E222: the server's custom request, spelled exactly as `vilan-lsp`'s
/// `OPENS_A_GENERIC_LIST` declares it; `book_sync` gates the two spellings.
const OPENS_A_GENERIC_LIST = 'vilan/opensAGenericList';

/// How long a `<` waits for the server before it gives up on its `>`. Every
/// keystroke after it queues behind the answer (typed text must never reorder),
/// so the wait is bounded well under a typing interval; an answer that misses
/// it costs one `>` the author types themselves, which is today's behaviour.
const GENERIC_ANSWER_TIMEOUT_MS = 150;

/// The characters a `>` may be placed before — the language configuration's own
/// `autoCloseBefore`, which is where VS Code lets every OTHER pair fire, so a `<`
/// typed in front of a word (`List|i32`) does not become `List<>i32`.
/// `vscode_extension.rs` holds the two strings equal.
const AUTO_CLOSE_BEFORE = ';:.,=}])> \n\t';

/// The installed override, or `undefined` while the setting is off or another
/// extension owns `type`.
let typeOverride: Disposable | undefined;

/// Every keystroke's turn, in order: a `<` awaiting the server holds the
/// characters typed after it, so they land after its `>` rather than before.
let typing: Promise<void> = Promise.resolve();

/// The `>`s this override placed and may type over, as document offsets per URI.
/// Kept current through every edit, and forgotten once the caret leaves their
/// line — VS Code's own overtype forgets an auto-closed character the same way.
const placedClosers = new Map<string, number[]>();

/// Install or remove the override to match `vilan.autoClosing.generics`.
/// Returns whether the installed state changed, which is when the server must
/// be told.
function syncTypeOverride(): boolean {
    const wanted = workspace.getConfiguration('vilan').get<boolean>('autoClosing.generics', true);
    if (wanted && typeOverride === undefined) {
        try {
            typeOverride = commands.registerCommand('type', typeThrough);
            return true;
        } catch (error) {
            outputChannel?.warn(
                'vilan.autoClosing.generics: another extension already owns the `type` command ' +
                    '(Vim emulation does), so a generic `<` is paired by the server alone and ' +
                    `its \`>\` is not typed over (${error instanceof Error ? error.message : String(error)})`,
            );
            return false;
        }
    }
    if (!wanted && typeOverride !== undefined) {
        typeOverride.dispose();
        typeOverride = undefined;
        placedClosers.clear();
        return true;
    }
    return false;
}

/// The override itself: queue the keystroke behind the one before it.
function typeThrough(args: { text: string }): Promise<void> {
    const turn = typing.then(() => typeOne(args));
    typing = turn.catch(() => undefined);
    return turn;
}

/// Whether a keystroke in `editor` is one this override pairs at all: a vilan
/// document with one caret and nothing selected. A selection is VS Code's to
/// surround (`<>` is a surrounding pair), and several carets are left to it too.
function pairsIn(editor: TextEditor | undefined): editor is TextEditor {
    return (
        editor !== undefined &&
        editor.document.languageId === 'vilan' &&
        editor.selections.length === 1 &&
        editor.selection.isEmpty
    );
}

async function typeOne(args: { text: string }): Promise<void> {
    const editor = window.activeTextEditor;
    if (args.text === '>' && pairsIn(editor) && typeOverClosing(editor)) {
        return;
    }
    await commands.executeCommand('default:type', args);
    if (args.text === '<' && pairsIn(editor)) {
        await closeGenericList(editor);
    }
}

/// A `>` typed onto one this override placed: move past it, insert nothing.
function typeOverClosing(editor: TextEditor): boolean {
    const document = editor.document;
    const closers = placedClosers.get(document.uri.toString());
    const caret = editor.selection.active;
    const offset = document.offsetAt(caret);
    const index = closers?.indexOf(offset) ?? -1;
    if (closers === undefined || index < 0) {
        return false;
    }
    closers.splice(index, 1);
    if (document.getText(new Range(caret, caret.translate(0, 1))) !== '>') {
        return false;
    }
    const past = caret.translate(0, 1);
    editor.selection = new Selection(past, past);
    return true;
}

/// The `<` has been typed; ask the server whether it opens a generic list, and
/// if so place its `>` without moving the caret.
async function closeGenericList(editor: TextEditor): Promise<void> {
    if (!client) {
        return;
    }
    const document = editor.document;
    const caret = editor.selection.active;
    const following = document.lineAt(caret.line).text.charAt(caret.character);
    if (following !== '' && !AUTO_CLOSE_BEFORE.includes(following)) {
        return;
    }
    const version = document.version;
    let opens = false;
    try {
        opens = await Promise.race([
            client.sendRequest<boolean>(
                OPENS_A_GENERIC_LIST,
                client.code2ProtocolConverter.asTextDocumentPositionParams(document, caret),
            ),
            new Promise<boolean>((resolve) =>
                setTimeout(() => resolve(false), GENERIC_ANSWER_TIMEOUT_MS),
            ),
        ]);
    } catch {
        return;
    }
    // The answer is about the text the question was asked of; anything that
    // moved since (the buffer, the caret, the focus) makes it stale.
    if (
        !opens ||
        document.version !== version ||
        window.activeTextEditor !== editor ||
        !editor.selection.active.isEqual(caret)
    ) {
        return;
    }
    // Merged into the `<`'s own undo step: one undo takes back `<>`.
    const placed = await editor.edit((builder) => builder.insert(caret, '>'), {
        undoStopBefore: false,
        undoStopAfter: false,
    });
    if (!placed) {
        return;
    }
    editor.selection = new Selection(caret, caret);
    const key = document.uri.toString();
    const closers = placedClosers.get(key) ?? [];
    closers.push(document.offsetAt(caret));
    placedClosers.set(key, closers);
}

// --- F27 R1/R6: the platform a file is analyzed under -------------------------
//
// The platform decides which `std` twin a file's types come from, so a file
// analyzed under the wrong one is full of errors about members that "do not
// exist". The overlay note on such an error says why the file is where it is;
// this says it BEFORE any error: `analyzed as: browser — declared` in the
// status bar, the full reason in its tooltip, for the vilan file in front of
// the author. The server answers from its last analysis
// (`vilan/analysisPlatform`), so the line follows the file as it is edited —
// type `[platform("browser")] mod self;` at the top and it turns to `declared`.

/// The server's status request, spelled exactly as `vilan-lsp`'s
/// `ANALYSIS_PLATFORM` declares it; `book_sync` gates the two spellings.
const ANALYSIS_PLATFORM = 'vilan/analysisPlatform';

/// How long after an edit the line asks again — past the server's own
/// analysis debounce, so it asks about the analysis the edit produced.
const PLATFORM_REFRESH_MS = 600;

let platformStatus: StatusBarItem | undefined;
let platformRefresh: ReturnType<typeof setTimeout> | undefined;

interface AnalysisPlatform {
    platform: string;
    kind: string | null;
    reason: string | null;
}

/// Ask the server about the active editor's file and show the answer, or hide
/// the line for anything that is not a vilan file.
async function refreshPlatformStatus(): Promise<void> {
    const editor = window.activeTextEditor;
    if (!platformStatus) {
        return;
    }
    if (!client || !editor || editor.document.languageId !== 'vilan') {
        platformStatus.hide();
        return;
    }
    let answer: AnalysisPlatform | null = null;
    try {
        answer = await client.sendRequest<AnalysisPlatform | null>(ANALYSIS_PLATFORM, {
            uri: editor.document.uri.toString(),
        });
    } catch {
        answer = null;
    }
    if (!answer || window.activeTextEditor !== editor) {
        platformStatus.hide();
        return;
    }
    platformStatus.text = answer.kind
        ? `analyzed as: ${answer.platform} — ${answer.kind}`
        : `analyzed as: ${answer.platform}`;
    platformStatus.tooltip = answer.reason
        ? `This file is analyzed under ${answer.platform}: ${answer.reason}`
        : `This file is analyzed under ${answer.platform}`;
    platformStatus.show();
}

/// Ask again once the edits have settled into an analysis.
function schedulePlatformRefresh(): void {
    if (platformRefresh !== undefined) {
        clearTimeout(platformRefresh);
    }
    platformRefresh = setTimeout(() => {
        platformRefresh = undefined;
        void refreshPlatformStatus();
    }, PLATFORM_REFRESH_MS);
}

/// Keep every placed `>` at its character through edits; one an edit replaces
/// is gone.
function trackClosers(changed: TextDocumentChangeEvent): void {
    const key = changed.document.uri.toString();
    const closers = placedClosers.get(key);
    if (closers === undefined) {
        return;
    }
    for (const change of changed.contentChanges) {
        const start = change.rangeOffset;
        const end = start + change.rangeLength;
        const delta = change.text.length - change.rangeLength;
        for (let index = closers.length - 1; index >= 0; index--) {
            if (closers[index] >= end) {
                closers[index] += delta;
            } else if (closers[index] >= start) {
                closers.splice(index, 1);
            }
        }
    }
    if (closers.length === 0) {
        placedClosers.delete(key);
    }
}

/// Forget the placed `>`s once the caret leaves their line.
function forgetDistantClosers(editor: TextEditor): void {
    const key = editor.document.uri.toString();
    const closers = placedClosers.get(key);
    if (closers === undefined) {
        return;
    }
    const line = editor.selection.active.line;
    const kept = closers.filter((offset) => editor.document.positionAt(offset).line === line);
    if (kept.length === 0) {
        placedClosers.delete(key);
    } else {
        placedClosers.set(key, kept);
    }
}

/// Resolve the language-server binary. An explicit `vilan.server.path` setting
/// wins; otherwise look for a binary built in-repo (the extension lives at
/// `<repo>/editors/vscode`, so the cargo target dir is two levels up), one
/// installed by `cargo install` (`~/.cargo/bin`), or the release toolchain the
/// install script manages (`~/.vilan/bin` — kept current by `vilan upgrade`),
/// and fall back to `vilan-lsp` on PATH. Developer builds outrank the release
/// install on purpose.
function resolveServerPath(context: ExtensionContext, configured: string): string {
    // Both spellings of the default are the sentinel: a Windows user who writes
    // the setting out as `vilan-lsp.exe` means "find it for me", not "this exact
    // relative file", and must not bypass discovery.
    if (configured && configured !== 'vilan-lsp' && configured !== serverBinary) {
        return configured;
    }
    const repoRoot = path.resolve(context.extensionPath, '..', '..');
    const candidates = [
        path.join(repoRoot, 'target', 'release', serverBinary),
        path.join(repoRoot, 'target', 'debug', serverBinary),
        path.join(os.homedir(), '.cargo', 'bin', serverBinary),
        path.join(os.homedir(), '.vilan', 'bin', serverBinary),
    ];
    for (const candidate of candidates) {
        if (fs.existsSync(candidate)) {
            return candidate;
        }
    }
    return serverBinary;
}

/// The spelling of `command` that is actually on disk: as written, or with the
/// platform's executable suffix. A Windows `vilan.server.path` is as likely to
/// name `C:\…\vilan-lsp` as `C:\…\vilan-lsp.exe`, and only one of them is a file
/// — accepting either beats rejecting a correct setting before the spawn. Off
/// Windows the suffix is empty, so this is the single `existsSync` it was.
function existingExecutable(command: string): string | undefined {
    for (const candidate of [command, `${command}${executableSuffix}`]) {
        if (fs.existsSync(candidate)) {
            return candidate;
        }
    }
    return undefined;
}

/// A clear, actionable error when the server can't be launched — instead of the
/// raw `spawn vilan-lsp ENOENT` buried in the output channel.
function reportMissingServer(command: string): void {
    const message =
        `Vilan: couldn't start the language server (\`${command}\`). ` +
        `Install the toolchain (the install script puts \`${serverBinary}\` in \`${installDirectory}\`), ` +
        'or build it with `cargo build --release -p vilan-lsp` and set ' +
        `\`vilan.server.path\` to the binary — or put \`${serverBinary}\` on your PATH.`;
    window.showErrorMessage(message, 'Open Settings').then((choice) => {
        if (choice === 'Open Settings') {
            commands.executeCommand('workbench.action.openSettings', 'vilan.server.path');
        }
    });
}

/// (Re)start the language client from current settings — used on activation and
/// by the `vilan.restartServer` command, so a rebuilt server (or a changed
/// `vilan.server.path` / `vilan.stdPath`) is picked up without reloading the
/// window. Replaces any running client; reuses one output channel.
async function startClient(context: ExtensionContext): Promise<void> {
    if (client) {
        // E106: the outcome of the stop used to be swallowed whole
        // (`.catch(() => undefined)`), so a server that refused to shut down —
        // `stop()` rejects after its two-second grace, and the old process is
        // only SIGTERMed two seconds after that — looked exactly like a clean
        // restart. Naming it is the difference between "the restart didn't
        // help" and "the restart didn't happen".
        const stopping = Date.now();
        try {
            await client.stop();
            outputChannel?.info(`previous server stopped in ${Date.now() - stopping} ms`);
        } catch (error) {
            outputChannel?.warn(
                `previous server did not stop cleanly after ${Date.now() - stopping} ms ` +
                    `(${error instanceof Error ? error.message : String(error)}); ` +
                    'the client library terminates the orphan shortly after',
            );
        }
        client = undefined;
    }

    const config = workspace.getConfiguration('vilan');
    const requested = resolveServerPath(context, config.get<string>('server.path') || 'vilan-lsp');
    const stdPath = config.get<string>('stdPath') || '';

    // A configured/in-repo path that doesn't exist is a clear misconfiguration —
    // report it up front rather than letting the spawn fail opaquely. (A bare
    // `vilan-lsp` is a PATH lookup, so it's checked by the `start()` failure.)
    const command = path.isAbsolute(requested) ? existingExecutable(requested) : requested;
    if (command === undefined) {
        reportMissingServer(requested);
        return;
    }

    const env = { ...process.env };
    if (stdPath) {
        env.VILAN_STD = stdPath;
    }

    const run = { command, transport: TransportKind.stdio, options: { env } };
    const serverOptions: ServerOptions = { run, debug: run };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'vilan' },
            // The manifest, by PATH rather than language id: `vilan.toml` is
            // `toml` only if the user has a TOML extension installed, and
            // `plaintext` otherwise — matching on the name catches both. The
            // server routes these to its manifest handler (completion only);
            // they never reach the vilan pipeline.
            { scheme: 'file', pattern: '**/vilan.toml' },
        ],
        synchronize: {
            // One watcher for the whole session, not one per client (E106).
            fileEvents: sharedFileWatcher(context),
        },
        // Seed the server's feature settings; later changes go via
        // `workspace/didChangeConfiguration` (see `activate`).
        initializationOptions: readFeatureConfig(),
        outputChannel,
        // E106: time every round trip at the client edge. One general hook
        // covers every request type, so a method added later is measured
        // without being remembered here.
        middleware: {
            sendRequest: async <P, R>(
                type: string | MessageSignature,
                param: P | undefined,
                token: CancellationToken | undefined,
                next: (
                    type: string | MessageSignature,
                    param?: P,
                    token?: CancellationToken,
                ) => Promise<R>,
            ): Promise<R> => {
                const started = Date.now();
                try {
                    return await next(type, param, token);
                } finally {
                    recordRequest(methodOf(type), Date.now() - started);
                }
            },
        },
    };

    serverStarts += 1;
    outputChannel?.info(`starting language server #${serverStarts} from ${command}`);
    client = new LanguageClient('vilan', 'Vilan Language Server', serverOptions, clientOptions);
    try {
        await client.start();
    } catch (error) {
        client = undefined;
        reportMissingServer(command);
        console.error('vilan-lsp failed to start:', error);
    }
}

/// The Organize Imports text edits the server offers for `document`, or `[]`.
/// Backs `vilan.organizeImports.onSave` by requesting the server's OWN
/// `source.organizeImports` action — so on-save organizing is byte-identical to
/// invoking it from the Source Action menu (the "editor and fmt can never
/// disagree" chain extends to the save hook).
async function organizeImportsEdits(document: TextDocument): Promise<TextEdit[]> {
    if (!client) {
        return [];
    }
    const wholeFile = new Range(0, 0, document.lineCount, 0);
    const actions = await commands.executeCommand<CodeAction[]>(
        'vscode.executeCodeActionProvider',
        document.uri,
        wholeFile,
        CodeActionKind.SourceOrganizeImports.value,
    );
    const organize = actions?.find((action) =>
        action.kind?.contains(CodeActionKind.SourceOrganizeImports),
    );
    return organize?.edit?.get(document.uri) ?? [];
}

export function activate(context: ExtensionContext): void {
    outputChannel = window.createOutputChannel('Vilan Language Server', { log: true });
    context.subscriptions.push(outputChannel);

    // E222: installed BEFORE the client starts, so its first
    // `initializationOptions` already say whether the `<` is ours.
    syncTypeOverride();
    context.subscriptions.push(
        { dispose: () => typeOverride?.dispose() },
        workspace.onDidChangeTextDocument(trackClosers),
        window.onDidChangeTextEditorSelection((event) => forgetDistantClosers(event.textEditor)),
        workspace.onDidCloseTextDocument((document) =>
            placedClosers.delete(document.uri.toString()),
        ),
    );

    // F27 R1/R6: the platform status line.
    platformStatus = window.createStatusBarItem(StatusBarAlignment.Right, 100);
    platformStatus.name = 'Vilan analysis platform';
    context.subscriptions.push(
        platformStatus,
        { dispose: () => platformRefresh !== undefined && clearTimeout(platformRefresh) },
        window.onDidChangeActiveTextEditor(() => void refreshPlatformStatus()),
        workspace.onDidChangeTextDocument((event) => {
            if (event.document === window.activeTextEditor?.document) {
                schedulePlatformRefresh();
            }
        }),
    );

    void startClient(context).then(() => schedulePlatformRefresh());

    context.subscriptions.push(
        commands.registerCommand('vilan.restartServer', async () => {
            // E106: the profile as it stood BEFORE the restart is the evidence a
            // "restarting didn't help" report needs — after the restart it is
            // gone. Logged here rather than remembered by the user.
            outputChannel?.info(
                ['session status before restart:', ...sessionStatusLines(context)].join('\n  '),
            );
            await startClient(context);
            if (client) {
                window.showInformationMessage('Vilan: language server restarted.');
            }
        }),
    );

    // E106: the tally on demand, for the moment the session starts feeling slow.
    context.subscriptions.push(
        commands.registerCommand('vilan.showServerStatus', async () => {
            outputChannel?.show(true);
            outputChannel?.info(
                ['language server session status:', ...sessionStatusLines(context)].join('\n  '),
            );
            // E174: and the SERVER's own page, asked for rather than waited
            // for — the client's tally says what this half spent, and E166's
            // memory numbers are on the other half's.
            await requestServerSessionSummary();
        }),
    );

    // Live setting changes. A server-path / std-path change needs a restart to
    // take effect; a feature toggle is pushed to the running server, which reads
    // its config per request (no re-registration needed).
    context.subscriptions.push(
        workspace.onDidChangeConfiguration(async (event) => {
            if (
                event.affectsConfiguration('vilan.server.path') ||
                event.affectsConfiguration('vilan.stdPath')
            ) {
                await startClient(context);
                return;
            }
            // E222: the override follows its setting live, and the server
            // hears about it only when the installed state actually moved.
            const overrideMoved =
                event.affectsConfiguration('vilan.autoClosing') && syncTypeOverride();
            if (
                client &&
                (overrideMoved ||
                    event.affectsConfiguration('vilan.inlayHints') ||
                    event.affectsConfiguration('vilan.semanticTokens') ||
                    event.affectsConfiguration('vilan.completion'))
            ) {
                client.sendNotification(DidChangeConfigurationNotification.type, {
                    settings: { vilan: readFeatureConfig() },
                });
            }
        }),
    );

    // `vilan.organizeImports.onSave`: run the server's Organize Imports action
    // before a save writes the file. This is the extension's own hook rather
    // than mutating the user's `editor.codeActionsOnSave` — it leaves that config
    // untouched (respecting it), and because organizing is a fixed point, a user
    // who has ALSO listed `source.organizeImports` there gets no double effect.
    context.subscriptions.push(
        workspace.onWillSaveTextDocument((event) => {
            if (event.document.languageId !== 'vilan' || !client) {
                return;
            }
            const enabled = workspace
                .getConfiguration('vilan', event.document)
                .get<boolean>('organizeImports.onSave', false);
            if (enabled) {
                event.waitUntil(organizeImportsEdits(event.document));
            }
        }),
    );
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
