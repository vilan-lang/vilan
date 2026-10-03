// E247 (R-a, ruled 2026-10-03): the status bar item and the menu it opens.
//
// The item reads `vilan 0.43.0` — the server's version — and a click opens a
// quick pick of three kinds of row: READ-ONLY facts about the file in front of
// the author (the platform it is analyzed under and why, the entry whose world
// it is served from — M104 —, the server's version and commit, and the last
// analysis' work counts, COUNTS rather than milliseconds, by the M106 ruling);
// TOGGLES for the feature switches the server reads per request (each flips its
// setting, which the extension already pushes to the running server live); and
// ACTIONS (restart, stop or start — E248 —, the status page, the output
// channel).
//
// Everything that DECIDES something lives here, with no `vscode` import, so
// `npm test` runs it under plain node — `versions.ts`'s pattern (E229). The
// wiring is `extension.ts`'s.

/// The server's answer to `vilan/analysisPlatform` for the active file, as the
/// menu reads it. Every field past `platform` is optional: an older server
/// answers the first three only, and the menu leaves the rest out.
export interface FileStatus {
    platform: string;
    kind: string | null;
    reason: string | null;
    /// M104: the canonical path of the entry whose world serves the file;
    /// `null` when the file is its own entry.
    world?: string | null;
    /// The last analysis' size, in counts.
    work?: { files: number; entities: number; impls: number } | null;
}

/// One feature switch the menu can flip: its setting key under `vilan.`, its
/// label, and its current value.
export interface FeatureToggle {
    key: string;
    label: string;
    value: boolean;
}

/// The settings a toggle row flips — the feature switches the server (or the
/// extension) reads live, inlay hints first (the owner's order). Every key is
/// declared in `package.json`; `npm test` holds the two lists together.
export const TOGGLES: ReadonlyArray<{ key: string; label: string }> = [
    { key: 'inlayHints.enabled', label: 'Inlay hints' },
    { key: 'inlayHints.abbreviate', label: 'Abbreviate hinted types (~Trait)' },
    { key: 'semanticTokens.enabled', label: 'Semantic highlighting' },
    { key: 'autoClosing.generics', label: 'Pair a generic < with its >' },
    { key: 'organizeImports.onSave', label: 'Organize Imports on save' },
];

/// The commands an action row runs — each declared in `package.json`.
export const ACTIONS = {
    restart: 'vilan.restartServer',
    stop: 'vilan.stopServer',
    start: 'vilan.startServer',
    status: 'vilan.showServerStatus',
    output: 'vilan.showOutput',
} as const;

/// Everything the menu is built from.
export interface MenuState {
    /// Whether a server is running (`false` after `Stop`, or a failed start).
    running: boolean;
    /// The server's `serverInfo.version`, as reported (`0.43.0 (fe092e8d1)`).
    serverVersion: string | undefined;
    /// The active vilan file's status, when there is one and the server
    /// answered.
    file: FileStatus | undefined;
    toggles: FeatureToggle[];
}

/// One row of the quick pick. `info` rows do nothing when chosen; a `toggle`
/// flips `setting`; an `action` runs `command`. `separator` rows are the
/// quick pick's own section headings.
export type MenuRow =
    | { kind: 'separator'; label: string }
    | { kind: 'info'; label: string; detail?: string }
    | { kind: 'toggle'; label: string; setting: string; value: boolean }
    | { kind: 'action'; label: string; command: string };

/// What the status bar item reads: `vilan 0.43.0`, `vilan` before the server
/// has said its version, and `vilan (stopped)` when no server runs.
export function statusText(running: boolean, serverVersion: string | undefined): string {
    if (!running) {
        return 'vilan (stopped)';
    }
    const version = serverVersion ? parseVersionOnly(serverVersion) : undefined;
    return version ? `vilan ${version}` : 'vilan';
}

/// The `analyzed as` line the status bar used to show, now the tooltip's first
/// line and the menu's first row.
export function platformLine(file: FileStatus): string {
    return file.kind
        ? `analyzed as: ${file.platform} — ${file.kind}`
        : `analyzed as: ${file.platform}`;
}

/// The tooltip: the platform and its whole reason, as the status line's
/// tooltip always said it, and how to open the menu.
export function statusTooltip(running: boolean, file: FileStatus | undefined): string {
    if (!running) {
        return 'The Vilan language server is stopped — click to start it, or for the menu';
    }
    const lines: string[] = [];
    if (file) {
        lines.push(
            file.reason
                ? `This file is analyzed under ${file.platform}: ${file.reason}`
                : `This file is analyzed under ${file.platform}`,
        );
    }
    lines.push('Click for the Vilan menu');
    return lines.join('\n');
}

/// The menu, top to bottom: the facts, the switches, the actions.
export function buildMenu(state: MenuState): MenuRow[] {
    const rows: MenuRow[] = [{ kind: 'separator', label: 'This file' }];
    if (!state.running) {
        rows.push({ kind: 'info', label: 'The language server is stopped' });
    } else if (state.file) {
        const file = state.file;
        rows.push({
            kind: 'info',
            label: platformLine(file),
            detail: file.reason ?? undefined,
        });
        if (file.world !== undefined) {
            rows.push({
                kind: 'info',
                label:
                    file.world === null
                        ? 'analyzed as its own entry'
                        : `analyzed in the world of ${baseName(file.world)}`,
                detail: file.world ?? undefined,
            });
        }
        if (file.work) {
            rows.push({
                kind: 'info',
                label: `last analysis: ${count(file.work.files, 'file')} · ${count(file.work.entities, 'entity', 'entities')} · ${count(file.work.impls, 'impl')}`,
                detail: 'work counts, not milliseconds',
            });
        }
    } else {
        rows.push({ kind: 'info', label: 'no analysis of this file yet' });
    }
    if (state.running && state.serverVersion) {
        rows.push({ kind: 'info', label: `server ${state.serverVersion}` });
    }
    rows.push({ kind: 'separator', label: 'Features' });
    for (const toggle of state.toggles) {
        rows.push({
            kind: 'toggle',
            label: `${toggle.value ? '$(check)' : '$(circle-large-outline)'} ${toggle.label}`,
            setting: toggle.key,
            value: toggle.value,
        });
    }
    rows.push({ kind: 'separator', label: 'Server' });
    if (state.running) {
        rows.push({ kind: 'action', label: '$(debug-restart) Restart Language Server', command: ACTIONS.restart });
        rows.push({ kind: 'action', label: '$(debug-stop) Stop Language Server', command: ACTIONS.stop });
    } else {
        rows.push({ kind: 'action', label: '$(debug-start) Start Language Server', command: ACTIONS.start });
    }
    rows.push({ kind: 'action', label: '$(pulse) Show Language Server Status', command: ACTIONS.status });
    rows.push({ kind: 'action', label: '$(output) Open the Output Channel', command: ACTIONS.output });
    return rows;
}

/// Where a toggle writes: the WORKSPACE setting when one is set there (so the
/// flip changes what is actually in force), else the USER setting.
export function toggleTarget(inspected: {
    workspaceValue?: unknown;
    workspaceFolderValue?: unknown;
}): 'workspace' | 'global' {
    return inspected.workspaceValue !== undefined || inspected.workspaceFolderValue !== undefined
        ? 'workspace'
        : 'global';
}

function parseVersionOnly(server: string): string {
    const match = /^(.*?)\s*\([^()]*\)\s*$/.exec(server.trim());
    return (match ? match[1] : server).trim();
}

function baseName(path: string): string {
    const parts = path.split(/[\\/]/);
    return parts[parts.length - 1] || path;
}

function count(value: number, one: string, many = `${one}s`): string {
    return `${value.toLocaleString('en-US')} ${value === 1 ? one : many}`;
}
