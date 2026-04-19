import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
	reactStrictMode: true,
	turbopack: {
		root: import.meta.dirname,
	},
	async redirects() {
		return [
			{
				source: '/',
				destination: '/overview/introduction',
				permanent: false,
			},
		];
	},
	async rewrites() {
		return [
			{
				source: '/:path*.mdx',
				destination: '/llms.mdx/docs/:path*',
			},
		];
	},
};

export default withMDX(config);
