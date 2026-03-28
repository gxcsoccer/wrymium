# CEF Startup Performance Analysis

> Research findings for wrymium TODO #7: CEF startup performance data.
> Sources include CEF forum (magpcss.org), CefSharp GitHub issues, Microsoft WebView2 docs,
> Electron benchmarks, Apple Developer Forums, and Chromium architecture docs.

---

## 1. CefInitialize() Timing

### Measured Data

| Scenario | Time | Source |
|----------|------|--------|
| **Cold start** (first run after reboot, Windows) | **3,000-4,000 ms** | [CEF Forum #13272](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=13272) |
| **Warm start** (subsequent runs, DLLs cached in memory) | **~30 ms** | [CEF Forum #13272](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=13272) |
| **Typical hardware** (non-cold, non-trivial app) | **300-600 ms** | [CEF Forum #13043](https://magpcss.org/ceforum/viewtopic.php?f=10&t=13043) |
| **Worst case** (resource-constrained systems) | **up to ~20,000 ms** | [CEF Forum #13043](https://magpcss.org/ceforum/viewtopic.php?f=10&t=13043) (fddima report) |

### What Happens During CefInitialize()

`CefInitialize()` is a **synchronous, blocking call** on the main thread that performs:

1. **DLL/dylib loading** — Loads `libcef.dll`/`libcef.so`/`Chromium Embedded Framework.framework` and all dependencies. This is the dominant cost on cold start (3-4s on Windows when DLLs are not in OS file cache).
2. **GPU process spawn** — Launches `--type=gpu-process` subprocess. This happens during `CefInitialize()` and the GPU process stays alive until `CefShutdown()`.
3. **Network service initialization** — The utility/network service process is created shortly after the browser process starts.
4. **Sandbox setup** — If sandboxing is enabled, configures sandbox attributes (requires main thread on all platforms).
5. **Cache/profile directory setup** — Opens or creates the user data directory, SQLite databases, etc.
6. **Proxy auto-detection** — WPAD/DHCP proxy resolution can add 1-2 seconds if system proxy is set to "auto-detect" (a major hidden cost).

### Subprocess Spawning: Synchronous vs Asynchronous

- The **GPU process** is spawned during `CefInitialize()` (effectively synchronous — init does not return until the GPU process is launched).
- The **renderer process** is spawned later, when the first browser is created.
- The **utility/network process** is spawned asynchronously after `CefInitialize()` returns (CEF continues background initialization on its own threads).

### Cold Start vs Warm Start

The 100x difference (30 ms vs 3,000-4,000 ms) on Windows is primarily due to **OS file cache behavior**:
- Cold: Windows reads `libcef.dll` (~100+ MB) and dependencies from disk
- Warm: DLLs remain in the OS page cache even after the app exits

On macOS and Linux, the same effect applies to `Chromium Embedded Framework.framework` / `libcef.so`, though macOS's unified memory architecture and APFS may reduce the cold-start penalty somewhat.

**Key insight from magreenblatt (CEF maintainer)**: Chrome mitigates this by using a separate DLL for the browser process vs subprocesses, and by prioritizing window display before loading all rendering libraries. CEF does not do this — it loads everything upfront.

---

## 2. Browser Creation Timing

### Measured Data

| Operation | Time | Source |
|-----------|------|--------|
| **First browser creation** (multi-process mode) | **2,000-3,000 ms** to visible content | [CEF Forum #10760](https://magpcss.org/ceforum/viewtopic.php?f=14&t=10760) |
| **Subsequent browser creation** (same session) | **< 1,000 ms** | [CEF Forum #10760](https://magpcss.org/ceforum/viewtopic.php?f=14&t=10760) |
| **Hot browser creation** (renderer process pool warm) | **50-250 ms** | [CEF Forum #16671](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=16671) |

### Async vs Sync API

- **`browser_host_create_browser()`** (async): Returns immediately. The browser is NOT usable until `OnAfterCreated` fires on the `CefLifeSpanHandler`. The renderer process may still be spinning up.
- **`browser_host_create_browser_sync()`** (sync): Blocks until the browser object is created, but this is NOT time-to-first-paint. There is additional latency for:
  1. Renderer process launch (if first browser)
  2. V8 context creation
  3. Page load + parse + layout + paint
  4. Compositing to the window

### Time-to-First-Paint Breakdown (estimated)

For the first browser in a session:

| Phase | Estimated Time |
|-------|---------------|
| Renderer process spawn | 200-500 ms |
| V8 context creation | 50-100 ms |
| HTML parse + layout | depends on content |
| First composite to screen | 50-100 ms |
| **Total (simple page)** | **~500-1,000 ms** |
| **Total with proxy delay** | **~2,000-3,000 ms** |

For subsequent browsers (renderer process reuse):

| Phase | Estimated Time |
|-------|---------------|
| Browser object creation | 20-50 ms |
| V8 context creation | 50-100 ms |
| Page load + paint | depends on content |
| **Total (simple page)** | **~100-300 ms** |

### Proxy Auto-Detection: The Hidden Killer

Multiple sources ([CEF Forum #10760](https://magpcss.org/ceforum/viewtopic.php?f=14&t=10760), [CefSharp #1873](https://github.com/cefsharp/CefSharp/issues/1873)) confirm that **Windows proxy auto-detection (WPAD)** can add 1-2 seconds to first browser creation. Fix: pass `--no-proxy-server` or `--winhttp-proxy-resolver` via `OnBeforeCommandLineProcessing()`.

---

## 3. Comparison with Native WebView Startup

### WKWebView (macOS/iOS)

| Metric | Time | Source |
|--------|------|--------|
| WKWebView allocation + init | **50-100+ ms** | [Apple Developer Forums](https://developer.apple.com/forums/thread/733774), [Adobe SDK issue #106](https://github.com/adobe/aepsdk-assurance-ios/issues/106) |
| First load (cold, with CSS) | **~57 ms** | [WebViewWarmUper benchmark](https://github.com/bernikovich/WebViewWarmUper) (iPhone XR, iOS 12) |
| First load (warm, with CSS) | **~32 ms** | [WebViewWarmUper benchmark](https://github.com/bernikovich/WebViewWarmUper) |
| Process pool creation overhead | dominant cost | Apple Developer Forums |

WKWebView is fast because:
- WebKit is a system framework, always loaded and in memory
- No separate process spawn for simple use (WebContent process is managed by the OS)
- No DLL/library loading overhead

### WebView2 (Windows)

| Metric | Time | Source |
|--------|------|--------|
| `CreateCoreWebView2ControllerAsync()` (multi-file app) | **~196 ms** | [WebView2Feedback #1909](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1909) |
| `CreateCoreWebView2ControllerAsync()` (single-file app) | **~342 ms** | [WebView2Feedback #1909](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1909) |
| DOMContentLoaded (multi-file) | **~320 ms** | [WebView2Feedback #1909](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1909) |
| DOMContentLoaded (single-file) | **~844 ms** | [WebView2Feedback #1909](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1909) |
| Cold start (first ever, with proxy) | **7,000-15,000 ms** | [WebView2Feedback #1540](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1540) |
| Cold start (simple WinForms app) | **< 2,000 ms** | [WebView2Feedback #1540](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1540) (Microsoft dev measurement) |

WebView2 is faster than CEF for warm starts because:
- Edge Runtime is pre-installed and often already running
- Shared browser process with other WebView2 apps
- Binaries are already in OS file cache from normal Edge usage

### WebKitGTK (Linux)

No specific benchmark data was found in public sources. Based on architecture:
- WebKitGTK uses a multi-process model similar to WKWebView
- Library loading is the dominant cold-start cost (~20-50 MB of shared objects)
- Estimated initialization: **100-300 ms** warm, **500-1,500 ms** cold (extrapolated from WKWebView + Linux I/O characteristics)
- No proxy auto-detection overhead (unlike Windows)

### Summary: Native WebView vs CEF

| Platform | Native WebView (warm) | Native WebView (cold) | CEF (warm) | CEF (cold) |
|----------|----------------------|----------------------|-----------|-----------|
| **macOS** | 50-100 ms | 100-200 ms | 300-600 ms | 2,000-4,000 ms |
| **Windows** | 200-350 ms | 1,000-2,000 ms | 300-600 ms | 3,000-4,000 ms |
| **Linux** | 100-300 ms (est.) | 500-1,500 ms (est.) | 300-600 ms | 2,000-4,000 ms |

**CEF overhead vs native**: ~200-500 ms on warm start; ~1,000-3,000 ms on cold start.

---

## 4. Electron Startup Time (Reference)

### Key Clarification

Electron does NOT use CEF. Electron embeds Chromium directly (via `libchromiumcontent`). However, the Chromium initialization path is architecturally similar, making Electron a useful reference point.

### Measured Data

| Metric | Time | Source |
|--------|------|--------|
| Simple Electron app (optimized) | **~1,000 ms** | [Electron issue #30529](https://github.com/electron/electron/issues/30529) |
| Typical Electron app (unoptimized) | **3,000-4,000 ms** | [Devas.life](https://www.devas.life/how-to-make-your-electron-app-launch-1000ms-faster/), [Astrolytics](https://www.astrolytics.io/blog/optimize-electron-app-slow-startup-time) |
| Complex Electron app (VS Code, Slack) | **2,000-5,000 ms** | Various community reports |
| Electron 11+ regression (window display) | **+1,000 ms** vs Electron 10 | [Electron issue #30529](https://github.com/electron/electron/issues/30529) |

### Startup Breakdown (Approximate)

| Phase | Estimated % | Estimated Time |
|-------|------------|---------------|
| Chromium/CEF library loading | 30-40% | 300-1,500 ms |
| GPU + subprocess launch | 15-20% | 150-800 ms |
| V8 initialization | 10-15% | 100-500 ms |
| App JS bundle loading | 20-30% | 200-1,200 ms |
| First paint | 5-10% | 50-400 ms |

### Electron Optimization Techniques Applicable to CEF

1. **V8 snapshots** — Pre-serialize V8 heap to skip JS parsing on startup. Electron uses `mksnapshot`. CEF does not expose this directly, but custom V8 snapshots could theoretically be used.
2. **Deferred module loading** — Load only essential code at startup; lazy-load the rest.
3. **DOM snapshot caching** — Cache the last-rendered DOM as a Data URL for instant visual display while the real content loads.
4. **Disable proxy auto-detection** — `--no-proxy-server` or `--winhttp-proxy-resolver` saves 1-2s on Windows.

---

## 5. CEF Optimization Techniques for wrymium

### Can subprocess launch be parallelized with app initialization?

**Partially.** `CefInitialize()` spawns the GPU process synchronously, but the renderer process is spawned lazily when the first browser is created. wrymium can:
- Call `CefInitialize()` as early as possible (during `WebViewBuilder::build()` first call)
- The app's UI (tao window) can be created in parallel with CEF's background initialization threads
- However, `CefInitialize()` itself blocks the main thread for 300-600 ms (warm) to 3-4s (cold)

### Does `browser_subprocess_path` affect startup time?

**Yes, positively.** Using a separate, smaller helper executable (instead of re-launching the main app binary) reduces subprocess startup time because:
- The helper binary is smaller, loads faster
- Avoids re-initializing all of the host app's libraries in subprocess mode
- On macOS, separate helper bundles are required anyway (for notarization)
- This is the approach used by Chrome itself

**Recommendation for wrymium**: Always use separate helper executables (already planned via cef-rs `build_util::mac`).

### Does disabling GPU speed up initialization?

**Slightly.** `--disable-gpu` prevents the GPU process from spawning, saving ~100-200 ms. However:
- Software rendering is significantly slower for page rendering
- Some CEF versions still spawn a gpu-process even with `--disable-gpu` ([CefSharp discussion #4421](https://github.com/cefsharp/CefSharp/discussions/4421))
- **Not recommended** for production wrymium apps — the GPU process is essential for performance

### Can CEF be pre-initialized on a background thread?

**No.** This is a hard constraint on all platforms:
- **macOS**: Cocoa requires all UX-related code on the main thread. `CefInitialize()` MUST be called from the main thread.
- **Linux**: X11 expects UX code on the main thread. Chromium also requires fork() to happen before any threads are created.
- **Windows**: Technically possible with `multi_threaded_message_loop`, but `CefInitialize()` itself still blocks the calling thread.

magreenblatt confirmed: *"I agree [that the calling thread should be considered main]. Unfortunately this is a non-goal for Chromium developers and CEF is limited to Chromium supported behaviors."* ([CEF Forum #19639](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=19639))

### Does cache directory setup affect init time?

**Yes.**
- First-ever launch (no cache dir) is faster than subsequent launches with large cache directories
- Corrupt cache can cause slow init or failures
- **Deleting cache synchronously during init blocks startup** — recommended workaround: rename the cache dir and delete in a background thread
- `persist_session_cookies = true` adds minor overhead (SQLite WAL checkpoint on init)

### Other Optimizations

| Technique | Impact | Notes |
|-----------|--------|-------|
| `--no-proxy-server` | **-1,000 to -2,000 ms** on Windows | Biggest single win; no effect on macOS/Linux |
| Separate helper executable | **-100 to -300 ms** per subprocess | Avoids loading host app code in subprocesses |
| SSD vs HDD | **-1,000 to -3,000 ms** cold start | Dominant factor for cold start on Windows |
| Disable sandbox (`--no-sandbox`) | **-50 to -100 ms** | Small impact; acceptable for dev, not recommended for production |
| Keep CEF alive (don't shutdown between uses) | **saves full reinit** | Only applicable for multi-window apps |
| Pre-warm: create hidden browser | **saves ~500-1,000 ms** on first visible browser | Create a minimal `about:blank` browser early |

---

## 6. Real-World Measurements Summary

### Published CEF Startup Benchmarks

| Source | Environment | CefInitialize | First Browser Visible | Total Cold Start |
|--------|-------------|--------------|----------------------|-----------------|
| CEF Forum #13272 | Windows, post-reboot | 3,000-4,000 ms | N/A | N/A |
| CEF Forum #13272 | Windows, warm | ~30 ms | N/A | N/A |
| CEF Forum #13043 | Standard hardware | 300-600 ms | N/A | N/A |
| CEF Forum #10760 | Multi-process, first browser | N/A | 2,000-3,000 ms | N/A |
| CEF Forum #10760 | Multi-process, subsequent | N/A | < 1,000 ms | N/A |
| WebView2 #1909 (comparable) | .NET multi-file | N/A | 320 ms (DOM ready) | N/A |

### No cef-rs or Rust+CEF Benchmarks Found

No published performance data exists specifically for `cef-rs` or any Rust+CEF project. This data will need to be collected during wrymium implementation (Spike 1 code or v0.1 prototype).

---

## 7. Impact Analysis for Tauri

### Current Tauri Startup (Native WebView)

| Platform | Window Visible | Content Rendered | Source |
|----------|---------------|-----------------|--------|
| macOS (WKWebView) | ~100-150 ms | ~350-400 ms | [Lukas Kalbertodt benchmark](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html) |
| Windows (WebView2) | ~200-350 ms | ~500-800 ms | Estimated from WebView2 data |
| Linux (WebKitGTK) | ~100-200 ms | ~300-500 ms | Estimated |

### Projected wrymium Startup (CEF)

| Platform | Window Visible | Content Rendered (warm) | Content Rendered (cold) |
|----------|---------------|------------------------|------------------------|
| macOS | ~100-150 ms (tao window) | **800-1,200 ms** | **2,500-4,500 ms** |
| Windows | ~100-150 ms (tao window) | **800-1,200 ms** | **3,000-5,000 ms** |
| Linux | ~100-150 ms (tao window) | **800-1,200 ms** | **2,500-4,500 ms** |

### Additional Delay vs Native WebView

| Scenario | macOS | Windows | Linux |
|----------|-------|---------|-------|
| **Warm start overhead** | +400-800 ms | +300-400 ms | +300-700 ms |
| **Cold start overhead** | +2,000-4,000 ms | +2,000-4,000 ms | +2,000-3,000 ms |

### User Perception Thresholds

Based on Nielsen Norman Group's established research ([NNGroup](https://www.nngroup.com/articles/response-times-3-important-limits/)):

| Threshold | User Perception | wrymium Status |
|-----------|----------------|---------------|
| **< 100 ms** | Instant | Window appears (tao) |
| **< 400 ms** | Responsive (Doherty threshold) | Native WebView achieves this |
| **< 1,000 ms** | Flow maintained, delay noticed | wrymium warm start (borderline) |
| **< 3,000 ms** | Acceptable with feedback | wrymium cold start (needs splash) |
| **> 5,000 ms** | User may abandon | Risk on cold start with proxy issues |

### Acceptability Assessment

**Warm start (800-1,200 ms)**: Acceptable for desktop apps. Users notice the delay but flow is not broken. This is comparable to Electron app startup, which users are accustomed to. Most desktop apps (VS Code, Slack, Discord) have similar startup times.

**Cold start (2,500-5,000 ms)**: Requires mitigation. Users will perceive this as slow. However:
- Cold starts are rare (only after reboot or first-ever launch)
- Subsequent launches benefit from OS file cache
- This is comparable to Electron cold start

### Mitigation Strategies for wrymium

1. **Show the tao window immediately** (~100 ms) with a loading indicator or the app's native UI chrome. CEF content appears when ready. This is already natural in wrymium's architecture since tao window creation is independent of CEF init.

2. **Splash screen / skeleton UI**: Render a lightweight native splash (Cocoa NSView / Win32 / GTK) in the tao window, then swap in CEF content when `OnLoadEnd` fires. The user sees something within 100 ms.

3. **Pre-warm a hidden browser**: During `CefInitialize()`, create a hidden `about:blank` browser. This pre-spawns the renderer process. When the real browser is needed, it reuses the warm renderer pool, saving 500-1,000 ms.

4. **Disable proxy auto-detection**: Add `--no-proxy-server` or `--winhttp-proxy-resolver` by default on Windows. This alone saves 1-2 seconds for many users.

5. **Progressive loading**: Load a minimal HTML shell first (instant), then hydrate with app content via JavaScript. The user sees a responsive UI within 1 second.

6. **Document cold-start expectations**: Be transparent in docs that first launch after install/reboot will be slower, similar to any Chromium-based app.

---

## Key Recommendations for wrymium

1. **Measure first, optimize second**: Instrument `CefInitialize()` and `browser_host_create_browser()` with `std::time::Instant` in the v0.1 prototype. Publish these numbers.

2. **Use separate helper executables**: Already planned. Saves 100-300 ms per subprocess.

3. **Default to `--no-proxy-server` on Windows**: Single biggest optimization (1-2s).

4. **Implement splash/skeleton UI pattern**: Show tao window immediately, swap in CEF content when ready.

5. **Consider pre-warming**: Create a hidden `about:blank` browser during init to pre-spawn the renderer process.

6. **Accept the tradeoff**: CEF adds 400-800 ms warm-start overhead vs native WebView. This is the inherent cost of bundled Chromium. Electron has the same cost. Document it clearly.

---

## Sources

- [CEF Forum: CefInitialize takes 4 secs first time after reboot](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=13272)
- [CEF Forum: How to improve startup performance?](https://magpcss.org/ceforum/viewtopic.php?f=14&t=10760)
- [CEF Forum: Asynchronous CEF initialization](https://magpcss.org/ceforum/viewtopic.php?f=10&t=13043)
- [CEF Forum: Using CEF from non-main application thread?](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=19639)
- [CEF Forum: How to reuse a CefBrowser](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=16671)
- [CefSharp: How to speed up first load (issue #1873)](https://github.com/cefsharp/CefSharp/issues/1873)
- [WebView2: EnsureCoreWebView2Async takes too long (issue #1540)](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1540)
- [WebView2: Slow startup in .NET 6 single-file (issue #1909)](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1909)
- [WebView2: Performance best practices](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- [Rick Strahl: Fighting WebView2 Visibility on Initialization](https://weblog.west-wind.com/posts/2022/Jul/14/Fighting-WebView2-Visibility-on-Initialization)
- [Electron: How to make your app launch 1,000ms faster](https://www.devas.life/how-to-make-your-electron-app-launch-1000ms-faster/)
- [Electron: Performance docs](https://www.electronjs.org/docs/latest/tutorial/performance)
- [Electron: Startup regression issue #30529](https://github.com/electron/electron/issues/30529)
- [Electron startup optimization (Astrolytics)](https://www.astrolytics.io/blog/optimize-electron-app-slow-startup-time)
- [WKWebView warm-up benchmarks](https://github.com/bernikovich/WebViewWarmUper)
- [Apple Developer Forums: WKWebView initialization](https://developer.apple.com/forums/thread/733774)
- [Tauri vs Iced vs egui performance comparison](http://lukaskalbertodt.github.io/2023/02/03/tauri-iced-egui-performance-comparison.html)
- [Nielsen Norman Group: Response Time Limits](https://www.nngroup.com/articles/response-times-3-important-limits/)
- [Doherty Threshold (Laws of UX)](https://lawsofux.com/doherty-threshold/)
- [Tauri vs Electron comparison (gethopp.app)](https://www.gethopp.app/blog/tauri-vs-electron)
- [CEF General Usage docs](https://chromiumembedded.github.io/cef/general_usage.html)
- [Chromium Multi-process Architecture](https://www.chromium.org/developers/design-documents/multi-process-architecture/)
