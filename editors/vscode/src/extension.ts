import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
    workspace,
    window,
    commands,
    CodeAction,
    CodeActionKind,
    LogOutputChannel,
    ExtensionContext,
    Range,
    TextDocument,
    TextEdit,
} from 'vscode';
import {
    DidChangeConfigurationNotification,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let outputChannel: LogOutputChannel | undefined;

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
    };
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
        await client.stop().catch(() => undefined);
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
            fileEvents: workspace.createFileSystemWatcher('**/*.vl'),
        },
        // Seed the server's feature settings; later changes go via
        // `workspace/didChangeConfiguration` (see `activate`).
        initializationOptions: readFeatureConfig(),
        outputChannel,
    };

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

    void startClient(context);

    context.subscriptions.push(
        commands.registerCommand('vilan.restartServer', async () => {
            await startClient(context);
            if (client) {
                window.showInformationMessage('Vilan: language server restarted.');
            }
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
            if (
                client &&
                (event.affectsConfiguration('vilan.inlayHints') ||
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
