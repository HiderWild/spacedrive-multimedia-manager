/**
 * Test setup for bun component tests.
 * Registers Happy DOM globals so @testing-library/react can render components.
 */
import { GlobalRegistrator } from "@happy-dom/global-registrator";

GlobalRegistrator.register();
