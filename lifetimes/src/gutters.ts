import path = require('path');
import * as vscode from 'vscode';
import { Path, Point, SVGBuilder } from './svg';

export function setupGutters(context: vscode.ExtensionContext) {
    let gutters = new Gutters();

    if (vscode.window.activeTextEditor) {
        gutters.updateGutters(vscode.window.activeTextEditor);
    }

    context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
        if (editor) {
            gutters.updateGutters(editor);
        }
    }));
}

class Gutters {
    private decoration: vscode.TextEditorDecorationType;

    constructor() {
        this.decoration = vscode.window.createTextEditorDecorationType({
            gutterIconPath: createIcon(),
            gutterIconSize: 'auto'
        });
        console.log(path.join(__dirname, '..', 'images', 'check.svg'));
    }

    public updateGutters(editor: vscode.TextEditor) {
        console.log("Updating gutters");
        editor.setDecorations(this.decoration, [new vscode.Range(0, 0, 1, 0)]);
        createIcon();
    }
}

function createIcon(): vscode.Uri {
    /*
    let image = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <path d="M30,76q6-14,13-26q6-12,14-23q8-12,13-17q3-4,6-6q1-1,5-2q8-1,12-1q1,0,1,1q0,1-1,2q-13,11-27,33q-14,21-24,44q-4,9-5,11q-1,2-9,2q-5,0-6-1q-1-1-5-6q-5-8-12-15q-3-4-3-6q0-2,4-5q3-2,6-2q3,0,8,3q5,4,10,14z" fill="green"/>
    </svg>`;
    */
    return new SVGBuilder()
        .path(new Path(Point.origin())
            .moveTo(new Point(55, 0))
            .lineTo(new Point(55, 100))
            .lineTo(new Point(45, 100))
            .lineTo(new Point(45, 0))
            .close(), 
            new vscode.Color(1, 0, 0, 1))
        .finish()
        .createUri();
}