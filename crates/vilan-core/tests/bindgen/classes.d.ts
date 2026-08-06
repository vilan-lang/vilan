// Classes: constructors, statics, instance methods, and readonly properties.
declare class Widget {
    constructor(title: string, open?: boolean);
    static create(title: string): Widget;
    static readonly version: string;
    readonly id: string;
    label: string;
    render(): void;
    resize(width: number, height: number): boolean;
    load(url: string): Promise<string>;
}
