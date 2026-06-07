import {Check, Copy} from '@phosphor-icons/react';
import {useEffect, useMemo, useRef, useState} from 'react';

export function OrganizeDebugPanel(props: {
	title: string;
	payload: unknown;
}) {
	const textAreaRef = useRef<HTMLTextAreaElement>(null);
	const [copied, setCopied] = useState(false);
	const debugJson = useMemo(
		() => JSON.stringify(props.payload, null, 2),
		[props.payload]
	);

	useEffect(() => {
		if (!copied) {
			return;
		}

		const timeout = window.setTimeout(() => setCopied(false), 1500);
		return () => window.clearTimeout(timeout);
	}, [copied]);

	const handleCopy = async () => {
		textAreaRef.current?.focus();
		textAreaRef.current?.select();

		try {
			if (navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(debugJson);
			} else {
				document.execCommand('copy');
			}
			setCopied(true);
		} catch (error) {
			console.error('Failed to copy organize debug payload:', error);
		}
	};

	return (
		<div className="border-app-line bg-app-box/70 rounded-xl border px-2 py-1.5">
			<div className="flex items-center justify-between gap-2">
				<div className="text-ink text-[10px] font-semibold uppercase tracking-[0.12em]">
					{props.title}
				</div>
				<button
					type="button"
					onClick={handleCopy}
					className="text-ink-dull hover:text-ink hover:bg-app-box flex items-center gap-1 rounded-md px-1.5 py-1 text-[10px] transition-colors"
					title={copied ? 'Copied' : 'Copy debug payload'}
				>
					{copied ? <Check size={12} weight="bold" /> : <Copy size={12} weight="bold" />}
					<span>{copied ? 'Copied' : 'Copy'}</span>
				</button>
			</div>
			<textarea
				ref={textAreaRef}
				readOnly
				spellCheck={false}
				value={debugJson}
				className="text-ink-dull mt-1 h-40 w-full resize-none overflow-auto rounded-lg bg-transparent font-mono text-[10px] leading-4 outline-none"
			/>
		</div>
	);
}
