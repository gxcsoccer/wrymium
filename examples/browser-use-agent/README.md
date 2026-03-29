# Browser Use Agent — Reference Implementation

LLM-driven browser automation using wrymium's Browser Use primitives.

## Architecture

```
Observe (screenshot + A11y tree)
  → Think (Claude decides next action)
    → Act (execute via Tauri commands → wrymium → CEF)
      → Verify (check result, detect CAPTCHA)
        → Error Recovery (retry / abort / ask user)
          → Loop until done or max steps
```

## Files

| File | Description |
|------|-------------|
| `src/agent.ts` | Core agent loop with error recovery and dead-loop detection |
| `src/browser.ts` | Browser client wrapping Tauri invoke calls |
| `src/llm.ts` | Claude API client with vision support |
| `src/types.ts` | Action schema and result types |
| `src/index.ts` | Entry point and demo |

## Action Schema

The LLM responds with JSON matching one of these types:

```typescript
{ type: "click", selector: "#button" }
{ type: "type", text: "Hello World" }
{ type: "press_key", key: "Enter" }
{ type: "navigate", url: "https://..." }
{ type: "scroll", direction: "down", amount: 500 }
{ type: "wait", ms: 1000 }
{ type: "evaluate", expression: "document.title" }
{ type: "done", result: "The answer is X" }
{ type: "wait_for_user", reason: "Complete the CAPTCHA" }
```

## Human-in-the-Loop

When the agent encounters CAPTCHAs or sensitive operations, it pauses and
lets the user interact directly with the embedded browser. The user clicks
"Continue" when done, and the agent resumes.

## Safety

- `evaluate` is restricted to read-only expressions (no fetch, eval, cookies, localStorage)
- LLM cannot execute arbitrary JS — all actions go through the predefined schema
- Navigation can be filtered via `with_navigation_filter` on the framework level

## Usage

```typescript
import { BrowserAgent } from "./src/agent";

const agent = new BrowserAgent({
  llmApiKey: "sk-ant-...",
  maxSteps: 20,
});

const result = await agent.run("Find the top story on Hacker News");
console.log(result.result);
```
