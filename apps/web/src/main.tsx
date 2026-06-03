import React from "react";
import ReactDOM from "react-dom/client";
import { PlatformProvider, Shell, initI18n } from "@sd/interface";
import { SpacedriveClient, HttpTransport } from "@sd/ts-client";
import { platform } from "./platform";
import "./index.css";
import "@sd/interface/styles.css";

// Initialize i18n with localStorage persistence
initI18n({
	persistence: {
		getLanguage: () => localStorage.getItem('sd-language'),
		setLanguage: (lang) => localStorage.setItem('sd-language', lang),
	},
});

// Talk to sd-server's /rpc endpoint on the same origin the page was loaded from.
// This works both standalone (browser → sd-server) and embedded inside an iframe.
const client = new SpacedriveClient(new HttpTransport());

function App() {
	return (
		<PlatformProvider platform={platform}>
			<Shell client={client} />
		</PlatformProvider>
	);
}

ReactDOM.createRoot(document.getElementById("root")!).render(
	<React.StrictMode>
		<App />
	</React.StrictMode>
);