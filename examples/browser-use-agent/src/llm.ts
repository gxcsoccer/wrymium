/**
 * LLM client — talks to Claude API for the "Think" step.
 *
 * Uses the Anthropic Messages API with vision support for screenshots.
 */

const DEFAULT_MODEL = "claude-sonnet-4-20250514";
const API_URL = "https://api.anthropic.com/v1/messages";

export class LLMClient {
  private apiKey: string;
  private model: string;

  constructor(apiKey: string, model?: string) {
    this.apiKey = apiKey;
    this.model = model ?? DEFAULT_MODEL;
  }

  /**
   * Send a chat message to Claude. Optionally include a screenshot (base64 PNG).
   * Returns the text content of the response.
   */
  async chat(systemPrompt: string, userMessage: string, screenshot?: string): Promise<string> {
    const content: MessageContent[] = [];

    // Add screenshot as image if provided
    if (screenshot) {
      content.push({
        type: "image",
        source: {
          type: "base64",
          media_type: "image/png",
          data: screenshot,
        },
      });
    }

    // Add text message
    content.push({ type: "text", text: userMessage });

    const body = {
      model: this.model,
      max_tokens: 1024,
      system: systemPrompt,
      messages: [{ role: "user", content }],
    };

    const response = await fetch(API_URL, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": this.apiKey,
        "anthropic-version": "2023-06-01",
      },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Claude API error (${response.status}): ${error}`);
    }

    const data = await response.json();

    // Extract text from response
    const textBlock = data.content?.find((c: any) => c.type === "text");
    if (!textBlock?.text) {
      throw new Error("No text in Claude response");
    }

    return textBlock.text;
  }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type MessageContent =
  | { type: "text"; text: string }
  | {
      type: "image";
      source: { type: "base64"; media_type: string; data: string };
    };
