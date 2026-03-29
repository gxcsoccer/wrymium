/**
 * Browser Use Agent — entry point.
 *
 * Usage (in a Tauri app frontend):
 *
 *   import { BrowserAgent } from "./agent";
 *
 *   const agent = new BrowserAgent({
 *     llmApiKey: "sk-ant-...",
 *     llmModel: "claude-sonnet-4-20250514",
 *   });
 *
 *   const result = await agent.run("Search Google for 'wrymium' and tell me what it is");
 *   console.log(result);
 */

export { BrowserAgent } from "./agent";
export { BrowserClient } from "./browser";
export { LLMClient } from "./llm";
export type { AgentConfig, AgentResult, BrowserAction, StepResult } from "./types";

// ---------------------------------------------------------------------------
// Demo (runs when loaded directly)
// ---------------------------------------------------------------------------

async function demo() {
  const { BrowserAgent } = await import("./agent");

  // Get API key from environment or prompt
  const apiKey = getApiKey();
  if (!apiKey) {
    console.error("Set ANTHROPIC_API_KEY or pass it in AgentConfig");
    return;
  }

  const agent = new BrowserAgent({
    llmApiKey: apiKey,
    maxSteps: 20,
    observeMode: "a11y+screenshot",
    screenshotMode: "annotated",
  });

  // Example tasks (uncomment one):
  const task = "Navigate to https://news.ycombinator.com and tell me the title of the #1 story";
  // const task = "Go to https://github.com/anthropics and tell me how many public repos they have";
  // const task = "Search Google for 'wrymium browser' and summarize the first result";

  console.log("Starting agent...");
  const result = await agent.run(task);

  console.log("\n=== Agent Result ===");
  console.log(`Success: ${result.success}`);
  console.log(`Result: ${result.result}`);
  console.log(`Steps: ${result.totalSteps}`);
  console.log("\nStep History:");
  for (const step of result.steps) {
    const status = step.success ? "✓" : "✗";
    console.log(`  ${status} Step ${step.step}: ${step.action.type} ${step.error ?? ""}`);
  }
}

function getApiKey(): string | undefined {
  // In Tauri app: could read from settings or environment
  if (typeof process !== "undefined" && process.env?.ANTHROPIC_API_KEY) {
    return process.env.ANTHROPIC_API_KEY;
  }
  // In browser: check window
  if (typeof window !== "undefined" && (window as any).__ANTHROPIC_API_KEY__) {
    return (window as any).__ANTHROPIC_API_KEY__;
  }
  return undefined;
}

// Auto-run demo if loaded directly
if (typeof window !== "undefined" && (window as any).__RUN_AGENT_DEMO__) {
  demo().catch(console.error);
}
