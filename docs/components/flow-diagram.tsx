type FlowStep = {
	title: string;
	description?: string;
	items?: string[];
	metrics?: Record<string, string>;
};

export function FlowDiagram({ steps = [] }: { steps?: FlowStep[] }) {
	return (
		<div className="my-8 space-y-0">
			{steps.map((step, index) => (
				<div key={step.title} className="flex flex-col items-center">
					<div className="w-full max-w-2xl rounded-lg border border-[#36A3FF]/20 bg-gradient-to-br from-[#36A3FF]/5 to-transparent p-6 shadow-sm backdrop-blur-sm transition-all hover:border-[#36A3FF]/40 hover:shadow-md">
						<div className="flex items-start gap-4">
							<div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[#36A3FF]/20 text-sm font-semibold text-[#36A3FF]">
								{index + 1}
							</div>
							<div className="flex-1">
								<h3 className="mt-0 text-base font-semibold text-fd-foreground">
									{step.title}
								</h3>
								{step.description && (
									<p className="mt-1 text-sm leading-relaxed text-fd-muted-foreground">
										{step.description}
									</p>
								)}
								{step.items && step.items.length > 0 && (
									<div className="mt-3 flex flex-wrap gap-2">
										{step.items.map((item) => (
											<span
												key={item}
												className="rounded-md border border-[#36A3FF]/30 bg-[#36A3FF]/10 px-2.5 py-1 text-xs text-fd-muted-foreground"
											>
												{item}
											</span>
										))}
									</div>
								)}
								{step.metrics && (
									<div className="mt-3 flex flex-wrap gap-3 text-xs">
										{Object.entries(step.metrics).map(([key, value]) => (
											<div
												key={key}
												className="rounded-md bg-fd-muted/40 px-2.5 py-1.5 font-mono"
											>
												<span className="text-fd-muted-foreground">
													{key}:
												</span>{' '}
												<span className="font-semibold text-[#36A3FF]">
													{value}
												</span>
											</div>
										))}
									</div>
								)}
							</div>
						</div>
					</div>
					{index < steps.length - 1 && (
						<div className="relative flex h-12 w-0.5 items-center justify-center">
							<div className="h-full w-full bg-gradient-to-b from-[#36A3FF]/40 via-[#36A3FF]/20 to-[#36A3FF]/40" />
							<div className="absolute h-2 w-2 rounded-full bg-[#36A3FF]/60" />
						</div>
					)}
				</div>
			))}
		</div>
	);
}
