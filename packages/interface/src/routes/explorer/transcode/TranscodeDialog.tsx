import {useLibraryMutation} from '@sd/ts-client';
import type {
	File,
	HwAccel,
	TranscodeCodec,
	TranscodeContainer,
	TranscodeInput
} from '@sd/ts-client';
import {
	Dialog,
	dialogManager,
	Input,
	Label,
	Select,
	SelectOption,
	Switch,
	toast,
	useDialog
} from '@spacedrive/primitives';
import {useState} from 'react';
import {useForm} from 'react-hook-form';

// UI-only selection state. These are not backend types: they map onto the
// numeric `max_dimension` / `crf` / `bitrate_kbps` fields of `TranscodeInput`.
type ResolutionMode = 'keep' | '3840' | '2560' | '1920' | '1280' | '854';
type QualityMode = 'crf' | 'bitrate';

const CODEC_LABELS: Record<TranscodeCodec, string> = {
	h264: 'H.264 / AVC (most compatible)',
	hevc: 'H.265 / HEVC (smaller files)',
	vp9: 'VP9 (royalty-free)',
	av1: 'AV1 (best compression)'
};

const CONTAINER_LABELS: Record<TranscodeContainer, string> = {
	mp4: 'MP4',
	mkv: 'MKV',
	webm: 'WebM'
};

const HW_ACCEL_LABELS: Record<HwAccel, string> = {
	auto: 'Auto (best available)',
	none: 'CPU only',
	nvenc: 'NVIDIA (NVENC)',
	qsv: 'Intel QuickSync',
	amf: 'AMD (AMF)',
	videotoolbox: 'Apple VideoToolbox'
};

const RESOLUTION_LABELS: Record<ResolutionMode, string> = {
	keep: 'Keep source',
	'3840': '4K (3840px)',
	'2560': '1440p (2560px)',
	'1920': '1080p (1920px)',
	'1280': '720p (1280px)',
	'854': '480p (854px)'
};

const PRESET_OPTIONS = [
	'ultrafast',
	'superfast',
	'veryfast',
	'faster',
	'fast',
	'medium',
	'slow',
	'slower',
	'veryslow'
] as const;
const CODEC_VALUES = Object.keys(CODEC_LABELS) as TranscodeCodec[];
const CONTAINER_VALUES = Object.keys(CONTAINER_LABELS) as TranscodeContainer[];
const HW_ACCEL_VALUES = Object.keys(HW_ACCEL_LABELS) as HwAccel[];
const RESOLUTION_VALUES = Object.keys(RESOLUTION_LABELS) as ResolutionMode[];
const QUALITY_MODE_VALUES = [
	'crf',
	'bitrate'
] as const satisfies readonly QualityMode[];

function isSelectValue<T extends string>(
	values: readonly T[],
	value: string
): value is T {
	return values.includes(value as T);
}

interface TranscodeDialogProps {
	id: number;
	files: File[];
}

function TranscodeDialog(props: TranscodeDialogProps) {
	const dialog = useDialog(props);
	const form = useForm();

	const [codec, setCodec] = useState<TranscodeCodec>('h264');
	const [container, setContainer] = useState<TranscodeContainer>('mp4');
	const [resolution, setResolution] = useState<ResolutionMode>('keep');
	const [qualityMode, setQualityMode] = useState<QualityMode>('crf');
	const [crf, setCrf] = useState(23);
	const [bitrateKbps, setBitrateKbps] = useState(8000);
	const [preset, setPreset] = useState<string>('medium');
	const [hwAccel, setHwAccel] = useState<HwAccel>('auto');
	const [force, setForce] = useState(false);

	const transcode = useLibraryMutation('media.transcode');
	const handleCodecChange = (value: string) => {
		if (isSelectValue(CODEC_VALUES, value)) {
			setCodec(value);
		}
	};
	const handleContainerChange = (value: string) => {
		if (isSelectValue(CONTAINER_VALUES, value)) {
			setContainer(value);
		}
	};
	const handleResolutionChange = (value: string) => {
		if (isSelectValue(RESOLUTION_VALUES, value)) {
			setResolution(value);
		}
	};
	const handleQualityModeChange = (value: string) => {
		if (isSelectValue(QUALITY_MODE_VALUES, value)) {
			setQualityMode(value);
		}
	};
	const handleHwAccelChange = (value: string) => {
		if (isSelectValue(HW_ACCEL_VALUES, value)) {
			setHwAccel(value);
		}
	};

	const onSubmit = form.handleSubmit(async () => {
		if (props.files.length === 0) return;

		try {
			await Promise.all(
				props.files.map((file) => {
					const input: TranscodeInput = {
						entry_uuid: file.id,
						codec,
						container,
						max_dimension:
							resolution === 'keep' ? null : Number(resolution),
						crf: qualityMode === 'crf' ? crf : null,
						bitrate_kbps:
							qualityMode === 'bitrate' ? bitrateKbps : null,
						preset,
						hw_accel: hwAccel,
						force
					};
					return transcode.mutateAsync(input);
				})
			);

			toast.success({
				title: 'Transcode started',
				body: `Transcoding ${props.files.length} ${
					props.files.length === 1 ? 'video' : 'videos'
				} to ${codec.toUpperCase()}`
			});
			dialog.state.open = false;
		} catch (err) {
			toast.error({
				title: 'Transcode failed',
				body: String(err)
			});
		}
	});

	return (
		<Dialog
			form={form}
			dialog={dialog}
			title="Transcode Video"
			description={`${props.files.length} ${
				props.files.length === 1 ? 'video' : 'videos'
			} selected`}
			onSubmit={onSubmit}
			ctaLabel="Transcode"
			loading={transcode.isPending}
		>
			<div className="space-y-4">
				<div>
					<Label>Codec</Label>
					<Select value={codec} onChange={handleCodecChange}>
						{CODEC_VALUES.map((value) => (
							<SelectOption key={value} value={value}>
								{CODEC_LABELS[value]}
							</SelectOption>
						))}
					</Select>
				</div>

				<div>
					<Label>Container</Label>
					<Select value={container} onChange={handleContainerChange}>
						{CONTAINER_VALUES.map((value) => (
							<SelectOption key={value} value={value}>
								{CONTAINER_LABELS[value]}
							</SelectOption>
						))}
					</Select>
				</div>

				<div>
					<Label>Resolution</Label>
					<Select
						value={resolution}
						onChange={handleResolutionChange}
					>
						{RESOLUTION_VALUES.map((value) => (
							<SelectOption key={value} value={value}>
								{RESOLUTION_LABELS[value]}
							</SelectOption>
						))}
					</Select>
				</div>

				<div>
					<Label>Quality</Label>
					<Select
						value={qualityMode}
						onChange={handleQualityModeChange}
					>
						<SelectOption value="crf">
							Constant quality (CRF)
						</SelectOption>
						<SelectOption value="bitrate">
							Target bitrate
						</SelectOption>
					</Select>
					{qualityMode === 'crf' ? (
						<Input
							type="number"
							min={0}
							max={51}
							value={crf}
							onChange={(e) =>
								setCrf(Number(e.currentTarget.value))
							}
							className="mt-2"
							inputElementClassName="w-full"
						/>
					) : (
						<Input
							type="number"
							min={100}
							value={bitrateKbps}
							onChange={(e) =>
								setBitrateKbps(Number(e.currentTarget.value))
							}
							className="mt-2"
							inputElementClassName="w-full"
						/>
					)}
				</div>

				<div>
					<Label>Encoder preset</Label>
					<Select value={preset} onChange={setPreset}>
						{PRESET_OPTIONS.map((value) => (
							<SelectOption key={value} value={value}>
								{value}
							</SelectOption>
						))}
					</Select>
				</div>

				<div>
					<Label>Hardware acceleration</Label>
					<Select value={hwAccel} onChange={handleHwAccelChange}>
						{HW_ACCEL_VALUES.map((value) => (
							<SelectOption key={value} value={value}>
								{HW_ACCEL_LABELS[value]}
							</SelectOption>
						))}
					</Select>
				</div>

				<div className="flex items-center justify-between">
					<Label>Re-encode if output exists</Label>
					<Switch checked={force} onCheckedChange={setForce} />
				</div>
			</div>
		</Dialog>
	);
}

/**
 * Opens the transcode configuration dialog for the given video files.
 *
 * Call this from a click handler. The caller is responsible for filtering the
 * selection down to video entries before opening.
 */
export function openTranscodeDialog(files: File[]) {
	return dialogManager.create((props) => (
		<TranscodeDialog {...props} files={files} />
	));
}
