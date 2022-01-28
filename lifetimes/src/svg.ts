import * as vscode from 'vscode';

const sideLength = 100;

export class SVG {
    private svg: string;

    public constructor(svg: string) {
        this.svg = svg;
    }

    public createUri(): vscode.Uri {
        let encoded = Buffer.from(this.svg, 'binary').toString('base64');
        let uri = "data:image/svg+xml;base64," + encoded;
        return vscode.Uri.parse(uri);
    }
}

export class SVGBuilder {
    private svg: string;

    public constructor() {
        this.svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ' + sideLength + ' ' + sideLength + '">';
    }

    public circle(center: Point, radius: number, fill: vscode.Color): SVGBuilder {
        this.svg += '<circle cx="' + center.x + '" cy="' + center.y + '" r="' + radius + '" fill="' + this.createRGB(fill) + '"/>';
        return this;
    }

    public path(path: Path, fill: vscode.Color): SVGBuilder {
        this.svg += '<path d="' + path.path + '" fill="' + this.createRGB(fill) + '"/>';
        return this;
    }

    public finish(): SVG {
        console.log(this.svg + '</svg>');
        return new SVG(this.svg + '</svg>');
    }

    private createRGB(color: vscode.Color): string {
        return 'rgb(' + Math.round(color.red * 255) + ',' + Math.round(color.green * 255) + ',' + Math.round(color.blue * 255) + ')';
    }
}

export class Path {
    public path: string;
    private position: Point = Point.origin();

    public constructor(start: Point) {
        this.path = "";
        this.moveTo(start);
    }

    public moveTo(target: Point): Path {
        this.position = target;
        this.opToPosition("M");
        return this;
    }

    public move(offset: Point): Path {
        this.position = this.position.add(offset);
        this.opToPosition("M");
        return this;
    }

    public lineTo(target: Point): Path {
        this.position = target;
        this.opToPosition("L");
        return this;
    }

    public line(offset: Point): Path {
        this.position = this.position.add(offset);
        this.opToPosition("L");
        return this;
    }

    public close(): Path {
        this.path += "Z ";
        return this;
    }

    private opToPosition(op: string) {
        this.path += op + " " + this.position.x + " " + this.position.y + " ";
    }
}

export class Point {
    public readonly x: number;
    public readonly y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    public add(other: Point): Point {
        return new Point(this.x + other.x, this.y + other.y);
    }

    public static origin(): Point {
        return new Point(0, 0);
    }

    public static center(): Point {
        return new Point(sideLength / 2, sideLength / 2);
    }
}