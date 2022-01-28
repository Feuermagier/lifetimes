import * as vscode from 'vscode';
import { setupGutters } from './gutters';

export function activate(context: vscode.ExtensionContext) {
	setupGutters(context);
}