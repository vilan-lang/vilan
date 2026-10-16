// E257 S7: the smallest WebSocket client node's inspector needs — text
// frames over one HTTP upgrade, masked as RFC 6455 requires of a client.
//
// The extension host's node is VS Code's, and the oldest one the extension
// supports (VS Code 1.91, node 20) has no global `WebSocket`; a dependency
// for this would be the vsix's first runtime one beyond the language client.
// The inspector speaks text frames of JSON, answers pings, and closes when
// the program exits: that is all this implements.
import * as crypto from 'crypto';
import * as http from 'http';
import type { Socket } from 'net';

const GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

export interface WebSocketHandlers {
    onMessage: (text: string) => void;
    onClose: () => void;
}

export class InspectorSocket {
    private buffer: Buffer = Buffer.alloc(0);
    private fragments: Buffer[] = [];
    private closed = false;

    private constructor(
        private readonly socket: Socket,
        private readonly handlers: WebSocketHandlers,
    ) {
        socket.on('data', (chunk: Buffer) => this.receive(chunk));
        socket.on('close', () => this.finish());
        socket.on('error', () => this.finish());
    }

    /// Opens `url` (`ws://127.0.0.1:9229/<id>`).
    static connect(url: string, handlers: WebSocketHandlers): Promise<InspectorSocket> {
        const target = new URL(url);
        const key = crypto.randomBytes(16).toString('base64');
        return new Promise((resolve, reject) => {
            const request = http.request({
                host: target.hostname,
                port: target.port,
                path: target.pathname + target.search,
                headers: {
                    Connection: 'Upgrade',
                    Upgrade: 'websocket',
                    'Sec-WebSocket-Version': '13',
                    'Sec-WebSocket-Key': key,
                },
            });
            request.once('upgrade', (response, socket, head) => {
                const expected = crypto
                    .createHash('sha1')
                    .update(key + GUID)
                    .digest('base64');
                if (response.headers['sec-websocket-accept'] !== expected) {
                    socket.destroy();
                    reject(new Error('the inspector refused the WebSocket handshake'));
                    return;
                }
                socket.setNoDelay(true);
                const connection = new InspectorSocket(socket, handlers);
                if (head.length > 0) {
                    connection.receive(head);
                }
                resolve(connection);
            });
            request.once('response', (response) => {
                reject(new Error(`the inspector answered ${response.statusCode} to the upgrade`));
            });
            request.once('error', reject);
            request.end();
        });
    }

    send(text: string): void {
        if (!this.closed) {
            this.socket.write(frame(0x1, Buffer.from(text, 'utf8')));
        }
    }

    close(): void {
        if (!this.closed) {
            this.socket.write(frame(0x8, Buffer.alloc(0)));
            this.socket.end();
        }
        this.finish();
    }

    private finish(): void {
        if (this.closed) {
            return;
        }
        this.closed = true;
        this.socket.destroy();
        this.handlers.onClose();
    }

    private receive(chunk: Buffer): void {
        this.buffer = this.buffer.length === 0 ? chunk : Buffer.concat([this.buffer, chunk]);
        for (;;) {
            const parsed = parseFrame(this.buffer);
            if (!parsed) {
                return;
            }
            this.buffer = this.buffer.subarray(parsed.length);
            switch (parsed.opcode) {
                case 0x0:
                case 0x1:
                case 0x2:
                    this.fragments.push(parsed.payload);
                    if (parsed.final) {
                        const text = Buffer.concat(this.fragments).toString('utf8');
                        this.fragments = [];
                        this.handlers.onMessage(text);
                    }
                    break;
                case 0x8:
                    this.close();
                    return;
                case 0x9:
                    this.socket.write(frame(0xa, parsed.payload));
                    break;
                default:
                    break;
            }
        }
    }
}

/// One masked client frame.
function frame(opcode: number, payload: Buffer): Buffer {
    const mask = crypto.randomBytes(4);
    let header: Buffer;
    if (payload.length < 126) {
        header = Buffer.from([0x80 | opcode, 0x80 | payload.length]);
    } else if (payload.length < 0x10000) {
        header = Buffer.alloc(4);
        header[0] = 0x80 | opcode;
        header[1] = 0x80 | 126;
        header.writeUInt16BE(payload.length, 2);
    } else {
        header = Buffer.alloc(10);
        header[0] = 0x80 | opcode;
        header[1] = 0x80 | 127;
        header.writeBigUInt64BE(BigInt(payload.length), 2);
    }
    const masked = Buffer.alloc(payload.length);
    for (let index = 0; index < payload.length; index++) {
        masked[index] = payload[index] ^ mask[index % 4];
    }
    return Buffer.concat([header, mask, masked]);
}

interface ParsedFrame {
    final: boolean;
    opcode: number;
    payload: Buffer;
    length: number;
}

/// The frame at the start of `buffer`, or `undefined` until all of it arrived.
function parseFrame(buffer: Buffer): ParsedFrame | undefined {
    if (buffer.length < 2) {
        return undefined;
    }
    const final = (buffer[0] & 0x80) !== 0;
    const opcode = buffer[0] & 0x0f;
    const masked = (buffer[1] & 0x80) !== 0;
    let length = buffer[1] & 0x7f;
    let offset = 2;
    if (length === 126) {
        if (buffer.length < 4) {
            return undefined;
        }
        length = buffer.readUInt16BE(2);
        offset = 4;
    } else if (length === 127) {
        if (buffer.length < 10) {
            return undefined;
        }
        length = Number(buffer.readBigUInt64BE(2));
        offset = 10;
    }
    const maskOffset = offset;
    if (masked) {
        offset += 4;
    }
    if (buffer.length < offset + length) {
        return undefined;
    }
    let payload = buffer.subarray(offset, offset + length);
    if (masked) {
        const mask = buffer.subarray(maskOffset, maskOffset + 4);
        const unmasked = Buffer.alloc(length);
        for (let index = 0; index < length; index++) {
            unmasked[index] = payload[index] ^ mask[index % 4];
        }
        payload = unmasked;
    }
    return { final, opcode, payload, length: offset + length };
}
