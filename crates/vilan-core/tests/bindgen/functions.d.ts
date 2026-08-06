// Plain function declarations: the simplest row in the table.
declare function greet(name: string): string;
declare function log(message: string, level?: string): void;
declare function total(values: number[]): number;
declare function load(url: string): Promise<string>;
declare function every(values: string[], predicate: (value: string) => boolean): boolean;
declare function watch(handler: (value: number) => void): void;
declare function commit(handler: (value: number) => Promise<string>): void;
declare function join(...parts: string[]): string;
declare function pair(): [string, number];
declare function nothing(): void;
declare function halt(): never;
