/**
 * Playwright micro-benchmarks — same operations as wrymium-bench for comparison.
 *
 * Measures CDP-over-WebSocket latency through Playwright's standard API.
 * Run: node benchmarks/playwright_bench.mjs
 */

import { chromium } from "playwright";
import { readFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { performance } from "perf_hooks";

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixtureUrl = (name) =>
  "file://" + resolve(__dirname, `../tests/fixtures/${name}`);

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

class Bench {
  constructor(name) {
    this.name = name;
    this.samples = [];
  }
  record(ms) {
    this.samples.push(ms);
  }
  print() {
    this.samples.sort((a, b) => a - b);
    const n = this.samples.length;
    const p = (pct) => this.samples[Math.min(Math.floor((n * pct) / 100), n - 1)];
    const avg = this.samples.reduce((a, b) => a + b, 0) / n;
    const fmt = (ms) => {
      if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
      if (ms < 1000) return `${ms.toFixed(2)}ms`;
      return `${(ms / 1000).toFixed(2)}s`;
    };
    console.log(
      `  ${this.name.padEnd(42)} n=${String(n).padEnd(5)} avg=${fmt(avg).padEnd(9)} p50=${fmt(p(50)).padEnd(9)} p95=${fmt(p(95)).padEnd(9)} p99=${fmt(p(99)).padEnd(9)} min=${fmt(this.samples[0])}`
    );
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  const page = await context.newPage();

  await page.goto(fixtureUrl("basic.html"), { waitUntil: "load" });

  console.log("\n================================================================");
  console.log("  Playwright Micro-Benchmarks (CDP over WebSocket)");
  console.log("================================================================\n");

  // Warmup
  for (let i = 0; i < 5; i++) {
    await page.evaluate("1");
  }

  // ------------------------------------------------------------------
  // 1. CDP roundtrip via page.evaluate
  // ------------------------------------------------------------------
  {
    const b = new Bench('cdp_roundtrip (evaluate "1")');
    for (let i = 0; i < 1000; i++) {
      const t = performance.now();
      await page.evaluate("1");
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 2. Screenshot (full viewport, PNG)
  // ------------------------------------------------------------------
  {
    const b = new Bench("screenshot (full viewport PNG)");
    for (let i = 0; i < 100; i++) {
      const t = performance.now();
      await page.screenshot({ type: "png" });
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 3. Screenshot (200x200 clip)
  // ------------------------------------------------------------------
  {
    const b = new Bench("screenshot (200x200 clip)");
    for (let i = 0; i < 100; i++) {
      const t = performance.now();
      await page.screenshot({
        type: "png",
        clip: { x: 0, y: 0, width: 200, height: 200 },
      });
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 4. A11y tree via raw CDP (basic.html)
  // ------------------------------------------------------------------
  {
    const cdp = await context.newCDPSession(page);
    const b = new Bench("a11y_tree CDP (basic.html)");
    for (let i = 0; i < 100; i++) {
      const t = performance.now();
      await cdp.send("Accessibility.getFullAXTree");
      b.record(performance.now() - t);
    }
    b.print();
    await cdp.detach();
  }

  // ------------------------------------------------------------------
  // 5. A11y tree via raw CDP (a11y.html)
  // ------------------------------------------------------------------
  {
    await page.goto(fixtureUrl("a11y.html"), { waitUntil: "load" });
    const cdp = await context.newCDPSession(page);
    const b = new Bench("a11y_tree CDP (a11y.html, complex)");
    for (let i = 0; i < 100; i++) {
      const t = performance.now();
      await cdp.send("Accessibility.getFullAXTree");
      b.record(performance.now() - t);
    }
    b.print();
    await cdp.detach();
    await page.goto(fixtureUrl("basic.html"), { waitUntil: "load" });
  }

  // ------------------------------------------------------------------
  // 6. DOM query: querySelector (via locator)
  // ------------------------------------------------------------------
  {
    const b = new Bench('dom_query (locator "#title")');
    for (let i = 0; i < 1000; i++) {
      const t = performance.now();
      await page.locator("#title").boundingBox();
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 7. Click
  // ------------------------------------------------------------------
  {
    const b = new Bench('click ("#test-btn")');
    for (let i = 0; i < 100; i++) {
      const t = performance.now();
      await page.click("#test-btn");
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 8. Type text
  // ------------------------------------------------------------------
  {
    await page.click("#test-input");
    const b = new Bench('type ("Hello World", each iter)');
    for (let i = 0; i < 100; i++) {
      await page.fill("#test-input", "");
      const t = performance.now();
      await page.fill("#test-input", "Hello World");
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 9. Navigate (local file)
  // ------------------------------------------------------------------
  {
    const url = fixtureUrl("basic.html");
    const b = new Bench("navigate (file:// local)");
    for (let i = 0; i < 20; i++) {
      const t = performance.now();
      await page.goto(url, { waitUntil: "load" });
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 10. Concurrent evaluates (10 parallel)
  // ------------------------------------------------------------------
  {
    const b = new Bench("concurrent (10 parallel evals)");
    for (let i = 0; i < 100; i++) {
      const t = performance.now();
      await Promise.all(
        Array.from({ length: 10 }, (_, j) => page.evaluate(`${j}`))
      );
      b.record(performance.now() - t);
    }
    b.print();
  }

  // ------------------------------------------------------------------
  // 11. Raw CDP call via CDPSession
  // ------------------------------------------------------------------
  {
    const cdp = await context.newCDPSession(page);
    const b = new Bench("raw CDP (Runtime.evaluate via session)");
    for (let i = 0; i < 1000; i++) {
      const t = performance.now();
      await cdp.send("Runtime.evaluate", {
        expression: "1",
        returnByValue: true,
      });
      b.record(performance.now() - t);
    }
    b.print();
    await cdp.detach();
  }

  console.log(
    "\n================================================================"
  );
  console.log("  Benchmarks complete.");
  console.log(
    "================================================================"
  );

  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
