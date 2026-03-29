/**
 * Type definitions for the Browser Use Agent.
 */

// ---------------------------------------------------------------------------
// Action types — the schema LLM must follow
// ---------------------------------------------------------------------------

export type BrowserAction =
  | { type: "click"; selector: string }
  | { type: "type"; text: string }
  | { type: "press_key"; key: string }
  | { type: "navigate"; url: string }
  | { type: "scroll"; direction: "up" | "down" | "left" | "right"; amount?: number }
  | { type: "wait"; ms?: number }
  | { type: "evaluate"; expression: string }
  | { type: "done"; result: string }
  | { type: "wait_for_user"; reason: string };

// ---------------------------------------------------------------------------
// Agent configuration
// ---------------------------------------------------------------------------

export interface AgentConfig {
  /** Anthropic API key for Claude. */
  llmApiKey: string;
  /** Model to use (default: "claude-sonnet-4-20250514"). */
  llmModel?: string;
  /** Maximum number of agent loop steps (default: 30). */
  maxSteps?: number;
  /** Screenshot mode: "annotated" overlays element labels (default). */
  screenshotMode?: "annotated" | "plain" | "none";
  /** What to include in observations (default: "a11y+screenshot"). */
  observeMode?: "a11y+screenshot" | "a11y" | "screenshot";
}

// ---------------------------------------------------------------------------
// Agent result
// ---------------------------------------------------------------------------

export interface AgentResult {
  /** Whether the task was completed successfully. */
  success: boolean;
  /** The result or error message. */
  result: string;
  /** History of all steps taken. */
  steps: StepResult[];
  /** Total number of steps executed. */
  totalSteps: number;
}

export interface StepResult {
  step: number;
  observation: string;
  action: BrowserAction;
  success: boolean;
  error?: string;
}

// ---------------------------------------------------------------------------
// Browser types (from Tauri commands)
// ---------------------------------------------------------------------------

export interface AnnotatedScreenshotResult {
  image: string; // base64 PNG
  elements: AnnotatedElement[];
}

export interface AnnotatedElement {
  label: number;
  role: string;
  name: string;
  selector: string;
  bounds: { x: number; y: number; width: number; height: number };
}
