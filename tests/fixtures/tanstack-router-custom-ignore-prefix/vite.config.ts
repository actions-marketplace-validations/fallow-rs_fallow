import { defineConfig } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";

export default defineConfig({
  plugins: [
    tanstackStart({
      router: {
        routeFileIgnorePrefix: "__excluded__",
      },
    }),
  ],
});
