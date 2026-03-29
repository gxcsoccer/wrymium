/**
 * Browser client — wraps Tauri invoke calls to wrymium's Browser Use primitives.
 *
 * Each method corresponds to a Phase 3 Tauri command that dispatches
 * to the CEF UI thread via the event loop message pump.
 */

import type { AnnotatedScreenshotResult } from "./types";

// Tauri invoke — available when running inside a Tauri app
declare function __TAURI_INVOKE__(cmd: string, args?: Record<string, unknown>): Promise<any>;

function invoke(cmd: string, args?: Record<string, unknown>): Promise<any> {
  if (typeof __TAURI_INVOKE__ !== "undefined") {
    return __TAURI_INVOKE__(cmd, args);
  }
  // Fallback for non-Tauri environments (testing)
  throw new Error(`Tauri invoke not available. Cannot call: ${cmd}`);
}

export class BrowserClient {
  // --- Navigation ---

  async navigate(url: string): Promise<void> {
    await invoke("browser_navigate", { url, wait: true });
  }

  // --- Page Perception ---

  /** Take a screenshot, returns base64 PNG. */
  async screenshot(): Promise<string> {
    return invoke("browser_screenshot");
  }

  /** Get the accessibility tree as a formatted string. */
  async accessibilityTree(): Promise<string> {
    return invoke("browser_accessibility_tree");
  }

  /** Execute JS and return the result. */
  async evaluate(expression: string): Promise<any> {
    const json = await invoke("browser_evaluate", { expr: expression });
    try {
      return JSON.parse(json);
    } catch {
      return json;
    }
  }

  /** Take an annotated screenshot with element labels. */
  async annotatedScreenshot(): Promise<AnnotatedScreenshotResult> {
    return invoke("browser_annotated_screenshot");
  }

  // --- Input ---

  /** Click an element by CSS selector. */
  async clickElement(selector: string): Promise<void> {
    await invoke("browser_click_element", { selector });
  }

  /** Type text into the focused element. */
  async typeText(text: string): Promise<void> {
    await invoke("browser_type", { text });
  }

  /** Press a special key. */
  async pressKey(key: string): Promise<void> {
    await invoke("browser_press_key", { key });
  }

  /** Scroll the page. */
  async scroll(direction: string, amount: number): Promise<void> {
    const dx = direction === "left" ? -amount : direction === "right" ? amount : 0;
    const dy = direction === "up" ? -amount : direction === "down" ? amount : 0;
    await invoke("browser_scroll", { x: 0, y: 0, dx, dy });
  }

  // --- Wait ---

  async wait(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms));
  }

  async waitForSelector(selector: string, timeoutMs = 10000): Promise<void> {
    await invoke("browser_wait_for_selector", { selector, timeoutMs });
  }

  async waitForNetworkIdle(idleMs = 500, timeoutMs = 30000): Promise<void> {
    await invoke("browser_wait_for_network_idle", { idleMs, timeoutMs });
  }
}
