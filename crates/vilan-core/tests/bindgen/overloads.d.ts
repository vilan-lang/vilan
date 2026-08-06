// Overloads: vilan has exactly one signature per name (§3.10).
declare function parse(source: string): string;
declare function parse(source: string, strict: boolean): string;
declare function parse(source: number): string;

interface Painter {
    draw(): void;
    draw(target: string): void;
    draw(target: string, alpha: number): void;
}
