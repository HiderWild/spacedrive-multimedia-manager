/*
 * Bearded icon SVG URLs for use in interface package (web/desktop only).
 * Auto-generated mapping.
 *
 * NOTE: This file uses import.meta.glob which is Vite-specific and not compatible
 * with React Native. Mobile apps should not import this file.
 */

type ViteGlob = <T>(
	pattern: string,
	options: {eager: true; query: string; import: 'default'}
) => Record<string, T>;

// Bun can import this helper for pure UI tests without providing Vite's glob API.
const glob = (import.meta as ImportMeta & {glob?: ViteGlob}).glob;
const modules =
	glob?.<string>('./*.svg', {
		eager: true,
		query: '?url',
		import: 'default'
	}) ?? {};

// Create a clean mapping: filename -> URL
export const beardedIconUrls: Record<string, string> = {};

Object.keys(modules).forEach((path) => {
	// Extract filename without path and extension
	// "./typescript.svg" -> "typescript"
	const name = path.replace('./', '').replace('.svg', '');
	beardedIconUrls[name] = modules[path];
});
