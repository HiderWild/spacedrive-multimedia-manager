import {readdirSync, readFileSync, statSync} from 'node:fs';
import {join} from 'node:path';
import {describe, expect, test} from 'bun:test';

function sourceFiles(root: string, extensions: readonly string[]): string[] {
	return readdirSync(root).flatMap((name) => {
		const path = join(root, name);
		if (statSync(path).isDirectory()) {
			return sourceFiles(path, extensions);
		}
		return extensions.some((extension) => path.endsWith(extension))
			? [path]
			: [];
	});
}

function productionSource(
	root: string,
	extensions: readonly string[]
): string[] {
	return sourceFiles(root, extensions).filter(
		(path) =>
			!path.includes('__tests__') &&
			!path.endsWith('.test.ts') &&
			!path.endsWith('.test.tsx')
	);
}

function readSources(
	roots: readonly string[],
	extensions: readonly string[]
): string {
	return roots
		.flatMap((root) => productionSource(root, extensions))
		.map((path) => readFileSync(path, 'utf8'))
		.join('\n');
}

describe('recursive organize retirement contract', () => {
	test('new organize route uses generated DTOs without unsafe response casts', () => {
		const root = 'packages/interface/src/routes/organize';
		const source = productionSource(root, ['.ts', '.tsx'])
			.map((path) => readFileSync(path, 'utf8'))
			.join('\n');

		expect(source).not.toMatch(/\bas\s+any\b/);
		expect(source).not.toMatch(
			/interface\s+Organize(TaskSummary|ItemView|CommitPlanOutput|SelectionInput)\b/
		);
	});

	test('active JSON persistence and organize ViewMode entry are retired', () => {
		const source = readSources(
			[
				'packages/interface/src',
				'apps/tauri/src',
				'apps/tauri/src-tauri/src'
			],
			['.ts', '.tsx', '.rs']
		);

		expect(source).not.toContain('load_organize_state');
		expect(source).not.toContain('save_organize_state');
		expect(source).not.toContain('delete_organize_state');
		expect(source).not.toMatch(/viewMode\s*===\s*["']organize["']/);
		expect(source).not.toMatch(/case\s+["']organize["']/);
		expect(source).not.toMatch(/id:\s*["']organize["']/);
	});

	test('legacy migration boundary retains list, read, and archive only', () => {
		const source = readFileSync(
			'apps/tauri/src-tauri/src/legacy_organize.rs',
			'utf8'
		);

		expect(source).toContain('list_legacy_organize_states');
		expect(source).toContain('read_legacy_organize_state');
		expect(source).toContain('archive_legacy_organize_state');
		expect(source).not.toContain('save_organize_state');
		expect(source).not.toContain('delete_organize_state');
	});

	test('old organize route is removed without deleting shared QuickPreview', () => {
		expect(() =>
			statSync('packages/interface/src/routes/explorer/organize')
		).toThrow();
		expect(
			statSync(
				'packages/interface/src/components/QuickPreview'
			).isDirectory()
		).toBe(true);
		expect(
			statSync('packages/interface/src/routes/organize').isDirectory()
		).toBe(true);
	});

	test('task view state is persisted per tab and task while selection stays transient', () => {
		const source = readFileSync(
			'packages/interface/src/components/TabManager/TabManagerContext.tsx',
			'utf8'
		);

		expect(source).toContain('organizeStates');
		expect(source).toContain('getOrganizeState');
		expect(source).toContain('updateOrganizeState');
		expect(source).toContain('localStorage.setItem(STORAGE_KEY');
		expect(source).toContain('Per-tab selection state (ephemeral');
	});
});
