import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';

export const gitConfig = {
	user: 'spacedriveapp',
	repo: 'spacedrive',
	branch: 'main',
	docsPath: 'docs/content/docs',
};

export function baseOptions(): BaseLayoutProps {
	return {
		nav: {
			title: 'Spacedrive',
		},
		githubUrl: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
	};
}
