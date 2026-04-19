import { source } from '@/lib/source';

export const revalidate = false;

export async function GET() {
	const lines: string[] = ['# Spacedrive Documentation', ''];
	for (const page of source.getPages()) {
		const description = page.data.description
			? `: ${page.data.description}`
			: '';
		lines.push(`- [${page.data.title}](${page.url})${description}`);
	}
	return new Response(lines.join('\n'));
}
