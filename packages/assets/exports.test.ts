import { expect, test } from "bun:test";

test("resolves the extensionless bearded icon helper through package exports", () => {
	expect(import.meta.resolve("@sd/assets/svgs/ext/Extras/urls")).toBe(
		new URL("./svgs/ext/Extras/urls.ts", import.meta.url).href,
	);
});
