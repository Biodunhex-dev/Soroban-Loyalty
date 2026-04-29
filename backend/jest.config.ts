import type { Config } from "jest";

const config: Config = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/__tests__/**/*.test.ts"],
  collectCoverageFrom: ["src/**/*.ts", "!src/index.ts"],
  setupFiles: ["<rootDir>/src/__tests__/setup.ts"],
  moduleNameMapper: {
    "^.*/soroban$": "<rootDir>/src/__mocks__/soroban.ts",
  },
};

export default config;
