// E257 S7: one session with node's inspector — the Chrome DevTools Protocol's
// request/response and events over the inspector's WebSocket.
import * as http from 'http';
import { InspectorSocket } from './websocket';

export type CdpEventListener = (method: string, params: any) => void;

export class CdpError extends Error {
    constructor(
        readonly method: string,
        message: string,
    ) {
        super(`${method}: ${message}`);
    }
}

export class CdpSession {
    private nextId = 1;
    private readonly pending = new Map<
        number,
        { method: string; resolve: (value: any) => void; reject: (error: Error) => void }
    >();
    private socket: InspectorSocket | undefined;
    private closed = false;

    private constructor(
        private readonly onEvent: CdpEventListener,
        private readonly onClose: () => void,
    ) {}

    static async connect(
        url: string,
        onEvent: CdpEventListener,
        onClose: () => void,
    ): Promise<CdpSession> {
        const session = new CdpSession(onEvent, onClose);
        session.socket = await InspectorSocket.connect(url, {
            onMessage: (text) => session.receive(text),
            onClose: () => session.finish(),
        });
        return session;
    }

    get isClosed(): boolean {
        return this.closed;
    }

    send<T = any>(method: string, params: Record<string, unknown> = {}): Promise<T> {
        if (this.closed || !this.socket) {
            return Promise.reject(new CdpError(method, 'the inspector connection is closed'));
        }
        const id = this.nextId++;
        return new Promise<T>((resolve, reject) => {
            this.pending.set(id, { method, resolve, reject });
            this.socket?.send(JSON.stringify({ id, method, params }));
        });
    }

    close(): void {
        this.socket?.close();
        this.finish();
    }

    private receive(text: string): void {
        let message: any;
        try {
            message = JSON.parse(text);
        } catch {
            return;
        }
        if (typeof message.id === 'number') {
            const waiting = this.pending.get(message.id);
            if (!waiting) {
                return;
            }
            this.pending.delete(message.id);
            if (message.error) {
                waiting.reject(new CdpError(waiting.method, message.error.message ?? 'failed'));
            } else {
                waiting.resolve(message.result ?? {});
            }
        } else if (typeof message.method === 'string') {
            this.onEvent(message.method, message.params ?? {});
        }
    }

    private finish(): void {
        if (this.closed) {
            return;
        }
        this.closed = true;
        for (const waiting of this.pending.values()) {
            waiting.reject(new CdpError(waiting.method, 'the inspector connection closed'));
        }
        this.pending.clear();
        this.onClose();
    }
}

/// The inspector's debuggable targets at `host:port` (`/json/list`), or
/// `undefined` while nothing listens there.
export function inspectorTargets(
    host: string,
    port: number,
): Promise<{ webSocketDebuggerUrl: string; id: string }[] | undefined> {
    return new Promise((resolve) => {
        const request = http.get({ host, port, path: '/json/list', timeout: 1000 }, (response) => {
            let body = '';
            response.setEncoding('utf8');
            response.on('data', (chunk: string) => (body += chunk));
            response.on('end', () => {
                try {
                    const targets = JSON.parse(body);
                    resolve(Array.isArray(targets) ? targets : undefined);
                } catch {
                    resolve(undefined);
                }
            });
        });
        request.on('timeout', () => {
            request.destroy();
            resolve(undefined);
        });
        request.on('error', () => resolve(undefined));
    });
}
