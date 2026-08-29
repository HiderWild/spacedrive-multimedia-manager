/// <reference types="vite/client" />

declare module '*.png' {
	const src: string;
	export default src;
}

declare module '*.jpg' {
	const src: string;
	export default src;
}

declare module '*.jpeg' {
	const src: string;
	export default src;
}

declare module '*.gif' {
	const src: string;
	export default src;
}

declare module '*.mp3' {
	const src: string;
	export default src;
}

declare module '*.ogg' {
	const src: string;
	export default src;
}

declare module '*.mp4' {
	const src: string;
	export default src;
}

declare module '*.svg' {
	const src: string;
	export default src;
	const ReactComponent: React.FC<React.SVGProps<SVGSVGElement>>;
	export {ReactComponent};
}

declare module '@sd/assets/icons/*.png' {
	const src: string;
	export default src;
}

declare module '@sd/assets/icons/*.jpg' {
	const src: string;
	export default src;
}

declare module '@sd/assets/sounds/*.mp3' {
	const src: string;
	export default src;
}

declare module 'maplibre-gl/dist/maplibre-gl.css';
