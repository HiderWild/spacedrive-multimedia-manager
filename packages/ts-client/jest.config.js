module.exports = {
	preset: 'ts-jest',
	testEnvironment: 'jsdom',
	roots: ['<rootDir>/src'],
	testMatch: ['**/__tests__/**/*.test.ts', '**/__tests__/**/*.test.tsx'],
	moduleNameMapper: {
		'^@/(.*)$': '<rootDir>/src/$1',
		'^@sd/ts-client$': '<rootDir>/src/__tests__/support/ts-client-interface-shim.ts',
		'^@sd/ts-client/(.*)$': '<rootDir>/src/$1',
	},
	setupFilesAfterEnv: ['<rootDir>/src/__tests__/setup.ts'],
	collectCoverageFrom: [
		'src/**/*.{ts,tsx}',
		'!src/**/*.d.ts',
		'!src/**/__tests__/**',
		'!src/generated/**',
	],
	globals: {
		'ts-jest': {
			diagnostics: false,
			isolatedModules: true,
			tsconfig: {
				jsx: 'react-jsx',
				baseUrl: '.',
				paths: {
					'@sd/ts-client': ['src/__tests__/support/ts-client-interface-shim.ts'],
					'@sd/ts-client/*': ['src/*'],
				},
			},
		},
	},
};

