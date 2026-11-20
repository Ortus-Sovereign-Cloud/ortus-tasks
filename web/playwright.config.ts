import { defineConfig } from "@playwright/test";

// The tests drive a real program with the core and this crate's apps
// (examples/dev.rs), on SQLite in a throwaway directory.
const port = Number(process.env.ORTUS_E2E_PORT ?? 7986);
const binary = process.env.ORTUS_E2E_BINARY ?? "../target/debug/examples/dev";

export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [["list"]],
  globalSetup: "./e2e/global-setup.ts",
  use: { baseURL: `http://localhost:${port}`, viewport: { width: 1440, height: 900 }, colorScheme: "dark", timezoneId: "UTC", storageState: "test-results/owner.json" },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
  webServer: [
    {
      command: `rm -rf test-results/data && ${binary}`,
      env: {
        ORTUS_ROLES: "web,worker",
        ORTUS_WEB_PORT: String(port),
        ORTUS_DATA_DIR: "test-results/data",
        ORTUS_SETUP_TOKEN: "e2e-setup-token",
        ORTUS_PUBLIC_URL: `http://localhost:${port}`,
      },
      url: `http://127.0.0.1:${port}/api/v1/health`,
      reuseExistingServer: false,
      timeout: 30_000,
    },
  ],
});
