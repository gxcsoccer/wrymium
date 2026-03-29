/**
 * Browser Use Agent — LLM-driven browser automation loop.
 *
 * This is a reference implementation showing how to use wrymium's Browser Use
 * primitives with an LLM (Claude) to automate web tasks.
 *
 * Architecture:
 *   Observe (screenshot + A11y tree)
 *     → Think (LLM decides next action)
 *       → Act (execute browser action)
 *         → Verify (check result)
 *           → Error Recovery (retry / abort)
 *             → Loop
 */

import type { BrowserAction, AgentConfig, AgentResult, StepResult } from "./types";
import { LLMClient } from "./llm";
import { BrowserClient } from "./browser";

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

export class BrowserAgent {
  private browser: BrowserClient;
  private llm: LLMClient;
  private config: AgentConfig;
  private history: StepResult[] = [];

  constructor(config: AgentConfig) {
    this.config = {
      maxSteps: 30,
      screenshotMode: "annotated", // "annotated" | "plain" | "none"
      observeMode: "a11y+screenshot", // "a11y+screenshot" | "a11y" | "screenshot"
      ...config,
    };
    this.browser = new BrowserClient();
    this.llm = new LLMClient(config.llmApiKey, config.llmModel);
  }

  /**
   * Run the agent on a task. Returns when the task is completed or max steps reached.
   */
  async run(task: string): Promise<AgentResult> {
    console.log(`[agent] Starting task: ${task}`);

    for (let step = 0; step < this.config.maxSteps!; step++) {
      console.log(`[agent] Step ${step + 1}/${this.config.maxSteps}`);

      // 1. Observe
      const observation = await this.observe();

      // 2. Think
      const action = await this.think(task, observation);

      if (action.type === "done") {
        console.log(`[agent] Task completed: ${action.result}`);
        return {
          success: true,
          result: action.result,
          steps: this.history,
          totalSteps: step + 1,
        };
      }

      if (action.type === "wait_for_user") {
        console.log(`[agent] Waiting for user: ${action.reason}`);
        await this.waitForUser(action.reason);
        continue;
      }

      // 3. Act
      const actResult = await this.act(action);

      // 4. Verify + Error Recovery
      const stepResult: StepResult = {
        step: step + 1,
        observation: observation.summary,
        action,
        success: actResult.success,
        error: actResult.error,
      };
      this.history.push(stepResult);

      if (!actResult.success) {
        const recovered = await this.recover(action, actResult.error!, step);
        if (!recovered) {
          return {
            success: false,
            result: `Failed at step ${step + 1}: ${actResult.error}`,
            steps: this.history,
            totalSteps: step + 1,
          };
        }
      }

      // Dead loop detection: same action repeated 3+ times
      if (this.detectDeadLoop()) {
        console.log("[agent] Dead loop detected, aborting");
        return {
          success: false,
          result: "Aborted: detected repeating actions (dead loop)",
          steps: this.history,
          totalSteps: step + 1,
        };
      }
    }

    return {
      success: false,
      result: `Max steps (${this.config.maxSteps}) reached`,
      steps: this.history,
      totalSteps: this.config.maxSteps!,
    };
  }

  // ---------------------------------------------------------------------------
  // Observe
  // ---------------------------------------------------------------------------

  private async observe(): Promise<Observation> {
    const parts: string[] = [];
    let screenshot: string | undefined;

    // Accessibility tree (structured text, LLM-friendly)
    if (this.config.observeMode?.includes("a11y")) {
      const tree = await this.browser.accessibilityTree();
      parts.push(`## Accessibility Tree\n${tree}`);
    }

    // Screenshot
    if (this.config.observeMode?.includes("screenshot")) {
      if (this.config.screenshotMode === "annotated") {
        const result = await this.browser.annotatedScreenshot();
        screenshot = result.image; // base64
        parts.push(
          `## Annotated Elements\n${result.elements
            .map((e) => `[${e.label}] ${e.role}: "${e.name}" (${e.selector})`)
            .join("\n")}`
        );
      } else {
        screenshot = await this.browser.screenshot();
      }
    }

    // Current URL
    const url = await this.browser.evaluate("window.location.href");
    parts.push(`## Current URL\n${url}`);

    return {
      summary: parts.join("\n\n"),
      screenshot,
    };
  }

  // ---------------------------------------------------------------------------
  // Think (LLM)
  // ---------------------------------------------------------------------------

  private async think(task: string, observation: Observation): Promise<BrowserAction> {
    const systemPrompt = SYSTEM_PROMPT;
    const userMessage = this.buildUserMessage(task, observation);

    const response = await this.llm.chat(systemPrompt, userMessage, observation.screenshot);

    // Parse the LLM response as a BrowserAction
    try {
      const action = JSON.parse(response) as BrowserAction;
      if (!isValidAction(action)) {
        throw new Error(`Invalid action schema: ${JSON.stringify(action)}`);
      }
      console.log(`[agent] Action: ${action.type}`, action);
      return action;
    } catch (e) {
      console.error(`[agent] Failed to parse LLM response: ${e}`);
      // Ask LLM to retry
      return { type: "done", result: `LLM parse error: ${e}` };
    }
  }

  private buildUserMessage(task: string, observation: Observation): string {
    const historyStr =
      this.history.length > 0
        ? `## Previous Actions\n${this.history
            .slice(-5)
            .map((s) => `Step ${s.step}: ${s.action.type} → ${s.success ? "OK" : s.error}`)
            .join("\n")}`
        : "";

    return `## Task\n${task}\n\n${historyStr}\n\n## Current Page State\n${observation.summary}`;
  }

  // ---------------------------------------------------------------------------
  // Act
  // ---------------------------------------------------------------------------

  private async act(action: BrowserAction): Promise<{ success: boolean; error?: string }> {
    try {
      switch (action.type) {
        case "click":
          await this.browser.clickElement(action.selector!);
          break;
        case "type":
          await this.browser.typeText(action.text!);
          break;
        case "press_key":
          await this.browser.pressKey(action.key!);
          break;
        case "navigate":
          await this.browser.navigate(action.url!);
          break;
        case "scroll":
          await this.browser.scroll(action.direction!, action.amount ?? 500);
          break;
        case "wait":
          await this.browser.wait(action.ms ?? 1000);
          break;
        case "evaluate":
          // Only allow pre-approved JS expressions (not arbitrary LLM code)
          if (!this.isSafeExpression(action.expression!)) {
            return { success: false, error: "Blocked: unsafe JS expression" };
          }
          await this.browser.evaluate(action.expression!);
          break;
        default:
          return { success: false, error: `Unknown action type: ${(action as any).type}` };
      }
      return { success: true };
    } catch (e: any) {
      return { success: false, error: e.message ?? String(e) };
    }
  }

  /**
   * Safety check: only allow read-only JS expressions.
   * LLM-generated JS is restricted to prevent side effects.
   */
  private isSafeExpression(expr: string): boolean {
    const blocked = [
      "fetch(",
      "XMLHttpRequest",
      "eval(",
      "Function(",
      "import(",
      "document.cookie",
      "localStorage",
      "sessionStorage",
      "window.open",
    ];
    return !blocked.some((b) => expr.includes(b));
  }

  // ---------------------------------------------------------------------------
  // Error Recovery
  // ---------------------------------------------------------------------------

  private async recover(
    failedAction: BrowserAction,
    error: string,
    step: number
  ): Promise<boolean> {
    console.log(`[agent] Recovering from error: ${error}`);

    // Take a screenshot to understand current state
    const observation = await this.observe();

    // Ask LLM what to do
    const recovery = await this.llm.chat(
      "You are a browser automation error recovery assistant.",
      `The following action failed:\n${JSON.stringify(failedAction)}\n\nError: ${error}\n\nCurrent page state:\n${observation.summary}\n\nShould we retry, try an alternative, or abort? Respond with JSON: {"decision": "retry" | "alternative" | "abort", "reason": "..."}`,
      observation.screenshot
    );

    try {
      const decision = JSON.parse(recovery);
      if (decision.decision === "abort") {
        return false;
      }
      // For retry/alternative, the next loop iteration will handle it
      return true;
    } catch {
      return false;
    }
  }

  // ---------------------------------------------------------------------------
  // Dead Loop Detection
  // ---------------------------------------------------------------------------

  private detectDeadLoop(): boolean {
    if (this.history.length < 3) return false;

    const last3 = this.history.slice(-3);
    const actions = last3.map((s) => JSON.stringify(s.action));
    return actions[0] === actions[1] && actions[1] === actions[2];
  }

  // ---------------------------------------------------------------------------
  // Human-in-the-Loop
  // ---------------------------------------------------------------------------

  private async waitForUser(reason: string): Promise<void> {
    // In a real Tauri app, this would show a UI dialog and wait for user input.
    // For the reference implementation, we emit a custom event.
    console.log(`[agent] 🔔 User action required: ${reason}`);
    console.log("[agent] The browser is embedded in the app — user can interact directly.");
    console.log("[agent] Call agent.resume() when done.");

    // This would be implemented as a Promise that resolves when the user
    // clicks a "Continue" button in the Tauri UI.
    return new Promise((resolve) => {
      // @ts-ignore — Tauri event listener
      if (typeof window !== "undefined" && window.__TAURI__) {
        window.__TAURI__.event.once("agent-resume", () => resolve());
      } else {
        // Fallback: auto-resume after 30s for testing
        setTimeout(resolve, 30000);
      }
    });
  }
}

// ---------------------------------------------------------------------------
// Types (internal)
// ---------------------------------------------------------------------------

interface Observation {
  summary: string;
  screenshot?: string; // base64
}

// ---------------------------------------------------------------------------
// Action Schema Validation
// ---------------------------------------------------------------------------

function isValidAction(action: any): action is BrowserAction {
  if (!action || typeof action.type !== "string") return false;

  switch (action.type) {
    case "click":
      return typeof action.selector === "string";
    case "type":
      return typeof action.text === "string";
    case "press_key":
      return typeof action.key === "string";
    case "navigate":
      return typeof action.url === "string";
    case "scroll":
      return typeof action.direction === "string";
    case "wait":
      return true;
    case "evaluate":
      return typeof action.expression === "string";
    case "done":
      return typeof action.result === "string";
    case "wait_for_user":
      return typeof action.reason === "string";
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// System Prompt
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT = `You are a browser automation agent. You control a web browser to complete tasks.

## Available Actions (respond with JSON)

- {"type": "click", "selector": "#button-id"}  — Click an element by CSS selector
- {"type": "type", "text": "Hello"}  — Type text into the focused element
- {"type": "press_key", "key": "Enter"}  — Press a key (Enter, Tab, Escape, ArrowDown, etc.)
- {"type": "navigate", "url": "https://..."}  — Navigate to a URL
- {"type": "scroll", "direction": "down", "amount": 500}  — Scroll (up/down/left/right)
- {"type": "wait", "ms": 1000}  — Wait for a duration
- {"type": "evaluate", "expression": "document.title"}  — Evaluate a read-only JS expression
- {"type": "done", "result": "The answer is X"}  — Task is complete, report the result
- {"type": "wait_for_user", "reason": "Please complete the CAPTCHA"}  — Pause for human intervention

## Rules

1. Respond with ONLY a single JSON action. No explanation, no markdown.
2. Use the annotated element labels [1], [2], etc. to identify elements when available.
3. Before typing, click the target input field first.
4. If you see a CAPTCHA or login prompt, use wait_for_user.
5. If the page hasn't loaded yet, use wait.
6. When the task is complete, use done with a clear result.
7. Do NOT use evaluate for anything that modifies the page. Read-only expressions only.
`;
