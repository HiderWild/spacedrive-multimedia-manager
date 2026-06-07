import {describe, expect, test} from 'bun:test';
import {createLineBuffer} from '../../src/transport';

describe('createLineBuffer', () => {
	test('emits complete lines across split string and binary chunks', () => {
		const lines: string[] = [];
		const buffer = createLineBuffer((line) => lines.push(line));

		buffer.push('{"first":1');
		expect(lines).toEqual([]);

		buffer.push('}\n{"second":2}\n');
		expect(lines).toEqual(['{"first":1}', '{"second":2}']);

		buffer.push(new TextEncoder().encode('{"third":3'));
		expect(lines).toEqual(['{"first":1}', '{"second":2}']);

		buffer.push(new TextEncoder().encode('}\n'));
		expect(lines).toEqual(['{"first":1}', '{"second":2}', '{"third":3}']);
	});

	test('keeps a trailing partial line buffered until it is completed', () => {
		const lines: string[] = [];
		const buffer = createLineBuffer((line) => lines.push(line));

		buffer.push('partial');
		buffer.push('-line');
		expect(lines).toEqual([]);

		buffer.push('\n');
		expect(lines).toEqual(['partial-line']);
	});
});
