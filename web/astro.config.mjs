// @ts-check
import node from "@astrojs/node";
import tailwind from "@astrojs/tailwind";
import { defineConfig } from "astro/config";

// https://astro.build/config
export default defineConfig({
	integrations: [tailwind({ applyBaseStyles: false })],
	output: "server",
	adapter: node({ mode: "standalone" }),
});
