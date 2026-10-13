// E257 S6: the `vilan` debug type's registration — the configuration provider
// that resolves a `vilan` launch configuration to the JavaScript debugger's
// (`debug.ts` holds the resolution itself, pinned under plain node), and the
// "Debug" code lens on a program's `fun main`.
import * as fs from 'fs';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
import {
    CodeLens,
    CodeLensProvider,
    DebugConfiguration,
    DebugConfigurationProvider,
    ExtensionContext,
    Range,
    TextDocument,
    Uri,
    WorkspaceFolder,
    commands,
    debug,
    languages,
    window,
    workspace,
} from 'vscode';
import {
    VilanLaunchConfiguration,
    mainFunctionLines,
    mainLensConfiguration,
    resolveLaunch,
    vilanCandidates,
} from './debug';

const executableSuffix = process.platform === 'win32' ? '.exe' : '';

export function registerDebugging(context: ExtensionContext): void {
    context.subscriptions.push(
        debug.registerDebugConfigurationProvider('vilan', new VilanConfigurationProvider(context)),
        languages.registerCodeLensProvider(
            { language: 'vilan', scheme: 'file' },
            new MainLensProvider(),
        ),
        commands.registerCommand('vilan.debugMain', (uri: Uri) =>
            debug.startDebugging(workspace.getWorkspaceFolder(uri), mainLensConfiguration(uri.fsPath)),
        ),
    );
}

class VilanConfigurationProvider implements DebugConfigurationProvider {
    constructor(private readonly context: ExtensionContext) {}

    async resolveDebugConfiguration(
        folder: WorkspaceFolder | undefined,
        configuration: DebugConfiguration,
    ): Promise<DebugConfiguration | undefined> {
        // F5 with no `launch.json`: an empty configuration, which debugs the
        // active `.vl` file.
        const launch: VilanLaunchConfiguration =
            !configuration.type && !configuration.request
                ? { type: 'vilan', request: 'launch', name: 'Debug vilan program' }
                : (configuration as VilanLaunchConfiguration);
        const resolved = resolveLaunch(launch, {
            vilan: this.vilanExecutable(),
            port: await freePort(),
            stdDirectories: stdDirectories(),
            activeFile: activeVilanFile(),
            workspaceFolder: folder?.uri.fsPath,
        });
        if (!resolved.ok) {
            void window.showErrorMessage(`Vilan: ${resolved.message}`);
            return undefined;
        }
        return resolved.configuration as DebugConfiguration;
    }

    private vilanExecutable(): string {
        const candidates = vilanCandidates({
            serverPath: workspace.getConfiguration('vilan').get<string>('server.path'),
            repoRoot: path.resolve(this.context.extensionPath, '..', '..'),
            home: os.homedir(),
            suffix: executableSuffix,
            join: path.join,
            dirname: path.dirname,
        });
        return candidates.find((candidate) => fs.existsSync(candidate)) ?? `vilan${executableSuffix}`;
    }
}

class MainLensProvider implements CodeLensProvider {
    provideCodeLenses(document: TextDocument): CodeLens[] {
        return mainFunctionLines(document.getText()).map(
            (line) =>
                new CodeLens(new Range(line, 0, line, 0), {
                    title: 'Debug',
                    tooltip: 'Run this program under the debugger (vilan run --inspect-brk)',
                    command: 'vilan.debugMain',
                    arguments: [document.uri],
                }),
        );
    }
}

function activeVilanFile(): string | undefined {
    const document = window.activeTextEditor?.document;
    if (document?.languageId === 'vilan' && document.uri.scheme === 'file') {
        return document.uri.fsPath;
    }
    return undefined;
}

/// The directories a debugger never steps into: the std cache the toolchain
/// materializes std into, and a configured `vilan.stdPath`.
function stdDirectories(): string[] {
    const directories = [path.join(os.homedir(), '.vilan', 'std-cache')];
    const configured = workspace.getConfiguration('vilan').get<string>('stdPath');
    if (configured) {
        directories.push(configured);
    }
    return directories;
}

/// A free loopback port for the inspector: one the OS hands out, released
/// for `node --inspect-brk` to take.
function freePort(): Promise<number> {
    return new Promise((resolve, reject) => {
        const server = net.createServer();
        server.once('error', reject);
        server.listen(0, '127.0.0.1', () => {
            const address = server.address();
            const port = typeof address === 'object' && address ? address.port : 9229;
            server.close(() => resolve(port));
        });
    });
}
