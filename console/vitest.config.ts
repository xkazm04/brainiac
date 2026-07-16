import { defineConfig } from "vitest/config";
import path from "node:path";

export default defineConfig({
  test: {
    environment: "node",
    // `app/` is included because the console's modules live there, and so does
    // their pure logic (archive-data's as-of arithmetic, for one). While this
    // said `src` only, a test file under app/ was silently skipped — it did not
    // fail, it simply never ran, and the suite stayed green while proving
    // nothing.
    include: ["src/**/*.test.ts", "app/**/*.test.ts"],
  },
  resolve: {
    alias: {
      // Server components guard — inert under vitest's node environment.
      "server-only": path.resolve(__dirname, "src/test/server-only-stub.ts"),
      "@": path.resolve(__dirname, "src"),
    },
  },
});
