# Spike 1: tao + CEF external_message_pump Coexistence on macOS

> Technical spike to verify that tao's event loop and CEF's `external_message_pump` can coexist on the same main thread on macOS.

---

## 1. Research Findings

### 1.1 How tao's Event Loop Works on macOS

tao's macOS event loop is implemented across three key files in `src/platform_impl/macos/`:

**event_loop.rs** -- `EventLoop::run()` calls `run_return()`, which ultimately calls `[NSApp run]` via `msg_send!`. This means tao delegates to Apple's standard `NSApplication` run loop, which internally uses `CFRunLoop` on the main thread.

**observer.rs** -- tao installs two `CFRunLoopObserver` instances on the main thread's `CFRunLoop`:

```
setup_control_flow_observers() installs:

  Observer 1 (priority = CFIndex::MIN, highest priority):
    - Watches: kCFRunLoopEntry | kCFRunLoopAfterWaiting
    - Callback: control_flow_begin_handler
    - Action: calls AppState::wakeup() on kCFRunLoopAfterWaiting

  Observer 2 (priority = CFIndex::MAX, lowest priority):
    - Watches: kCFRunLoopExit | kCFRunLoopBeforeWaiting
    - Callback: control_flow_end_handler
    - Action: calls AppState::cleared() on kCFRunLoopBeforeWaiting
```

Both observers are added to `kCFRunLoopCommonModes`.

**EventLoopWaker** -- tao also installs a `CFRunLoopTimer` on the main run loop for its own waking purposes:

```rust
// Created with:
//   fire date: f64::MAX (far future = dormant)
//   interval:  0.000_000_1 (0.1 microsecond)
//   mode:      kCFRunLoopCommonModes
//
// Controlled via:
//   waker.stop()      -> sets next fire date to f64::MAX (never)
//   waker.start()     -> sets next fire date to f64::MIN (immediate)
//   waker.start_at(t) -> sets next fire date to specific time
```

The timer callback (`wakeup_main_loop`) is intentionally a no-op -- its sole purpose is to wake the CFRunLoop from its blocking wait.

**EventLoopProxy** -- The proxy uses a `CFRunLoopSourceRef` to wake the main thread:

```rust
// Proxy::send_event():
CFRunLoopSourceSignal(self.source);
CFRunLoopWakeUp(CFRunLoopGetMain());
```

This is Send + Sync (when T: Send), making it safe to call from CEF's background threads.

**Key insight**: Because tao uses `[NSApp run]` which is built on `CFRunLoop`, we can add our own `CFRunLoopTimer` or `CFRunLoopObserver` to the same run loop. CFRunLoop supports multiple timers and observers coexisting -- this is by design.

### 1.2 CEF external_message_pump on macOS

**CefDoMessageLoopWork()** performs a single iteration of CEF message loop processing. On macOS, with `external_message_pump = true`, it does NOT call `[NSApp nextEventMatchingMask:]` to pull OS events (that is the non-external-pump behavior of `MessagePumpNSApplication`). Instead, it only processes internal Chromium task queues.

This is critical: without `external_message_pump`, `CefDoMessageLoopWork()` calls through to `MessagePumpNSApplication` which actively pulls OS events from the NS event queue. This causes event-stealing conflicts with the host app (documented in the SDL keyboard event loss issue). With `external_message_pump = true`, this problem is eliminated.

**OnScheduleMessagePumpWork(delay_ms)** is called from ANY thread when CEF has pending work. The contract:
- `delay_ms <= 0`: call `CefDoMessageLoopWork()` on the main thread "reasonably soon"
- `delay_ms > 0`: schedule a call after `delay_ms` milliseconds, cancelling any pending scheduled call
- Must result in `CefDoMessageLoopWork()` being called on the main (UI) thread

**CEF's own reference implementation** (in `main_message_loop_external_pump_mac.mm`) uses:
- `NSTimer` for delayed scheduling
- `performSelector:onThread:withObject:waitUntilDone:NO` to dispatch from background threads to the main thread
- A `KillTimer()` / `SetTimer()` pattern for rescheduling
- A maximum timer delay capped at ~33ms (30fps) to ensure continuous rendering updates

**CefAppProtocol requirement**: All CEF client apps on macOS "must subclass NSApplication and implement CefAppProtocol". This protocol requires `isHandlingSendEvent` / `setHandlingSendEvent:` methods on the NSApplication. This is a potential conflict since tao creates its own NSApplication instance. However, with `external_message_pump = true`, CEF does not call `[NSApp sendEvent:]` itself, so this may be relaxable. This needs empirical verification.

### 1.3 CefInitialize Lifecycle

CEF's reference `cefclient_mac.mm` shows this order:
1. Create `NSAutoreleasePool`
2. Initialize the `NSApplication` singleton (ClientApplication)
3. **`CefInitialize()`** -- before any message loop runs
4. Create message loop (external pump or standard)
5. Enter message loop (`[NSApp run]` or equivalent)

In the Tauri lifecycle:
1. `EventLoop::new()` -- creates `NSApplication` singleton (tao creates it here)
2. `WebViewBuilder::build()` -- **this is where wrymium gets called**
3. `EventLoop::run()` -- calls `[NSApp run]`, never returns

Therefore, `CefInitialize()` CAN be called during `WebViewBuilder::build()` because `NSApplication` already exists (created in step 1) but the run loop has not started yet (step 3). This is exactly the right timing.

### 1.4 CFRunLoopTimer vs CFRunLoopObserver for CEF Integration

**Option A: CFRunLoopTimer** (RECOMMENDED)
- Add a repeating `CFRunLoopTimer` to `CFRunLoopGetMain()` with `kCFRunLoopCommonModes`
- Fire at ~33ms intervals (30fps baseline)
- Timer callback calls `CefDoMessageLoopWork()`
- `OnScheduleMessagePumpWork(delay_ms)` adjusts the timer's next fire date via `CFRunLoopTimerSetNextFireDate()`
- This is analogous to tao's own `EventLoopWaker` pattern

Advantages:
- Precise control over timing
- Can be rescheduled dynamically via `OnScheduleMessagePumpWork`
- No conflict with tao's observers (they use different priorities)
- Timer callbacks execute on the main thread, which is exactly what CEF requires

**Option B: CFRunLoopObserver (kCFRunLoopBeforeWaiting)**
- Install an observer at a priority between tao's two observers
- Call `CefDoMessageLoopWork()` before the run loop goes to sleep

Disadvantages:
- Only fires when the run loop is about to sleep -- misses cases where CEF needs work while the run loop is actively processing events
- Cannot honor `delay_ms` scheduling from `OnScheduleMessagePumpWork`
- Could delay the run loop's sleep, causing performance issues

**Verdict**: CFRunLoopTimer is the correct approach. It mirrors what tao itself does and what CEF's reference implementation does.

### 1.5 Threading Analysis

All participants operate on the main thread:
- tao's `[NSApp run]` and CFRunLoop: main thread
- tao's CFRunLoopObserver callbacks: main thread (they fire within CFRunLoop)
- tao's CFRunLoopTimer callback: main thread
- Our CFRunLoopTimer callback: main thread (same CFRunLoop)
- `CefDoMessageLoopWork()`: must be called on main thread (satisfied)

The only cross-thread interaction is `OnScheduleMessagePumpWork()`, which CEF calls from any thread. Our implementation must safely dispatch to the main thread. Options:
- `CFRunLoopTimerSetNextFireDate()` -- this is thread-safe for CFRunLoopTimer
- `dispatch_async(dispatch_get_main_queue(), ...)` -- standard GCD approach
- `CFRunLoopSourceSignal()` + `CFRunLoopWakeUp()` -- what tao's proxy does

`CFRunLoopTimerSetNextFireDate()` is documented as thread-safe by Apple, making it the simplest option.

### 1.6 CefAppProtocol Concern

tao creates its own NSApplication class. CEF requires the NSApplication to conform to `CefAppProtocol` (which requires `isHandlingSendEvent` / `setHandlingSendEvent:`). With `external_message_pump = true`, CEF should not be calling `[NSApp sendEvent:]` itself, so the protocol check may not be hit at runtime. However, some internal Chromium code paths check `[NSApp conformsToProtocol:@protocol(CrAppProtocol)]`.

Mitigation strategies (to be tested in the spike):
1. **Runtime swizzle**: Add the protocol methods to tao's NSApplication class at runtime using `class_addMethod` + `class_addProtocol`
2. **Verify it is not needed**: With external_message_pump, CEF may not check this
3. **Pre-create NSApplication**: If wrymium initializes before tao, create a custom NSApplication subclass first

### 1.7 Prior Art

- **Sokol + CEF** (https://github.com/floooh/sokol/issues/737): Resolved by using `external_message_pump = true` and calling `CefDoMessageLoopWork()` from the frame callback
- **SDL + CEF** keyboard event loss: Caused by BOTH frameworks pulling events from the NS event queue. Fixed by routing SDL event processing through `[SDLApplication sentEvent:]` instead of competing for raw events. With `external_message_pump = true` this problem does not arise because CEF stops pulling from the OS event queue.
- **CEFPython** (https://github.com/cztomczak/cefpython/issues/246): Detailed implementation of external message pump, confirming the timer + OnScheduleMessagePumpWork pattern works

---

## 2. Spike Plan

### 2.1 Goal

Build a minimal Rust program that:
1. Creates a tao window
2. Initializes CEF with `external_message_pump = true`
3. Creates a CEF browser as a child of the tao window
4. Loads `https://example.com`
5. The page renders and the tao event loop remains responsive (window can be moved, resized, closed)

### 2.2 Project Structure

```
spike-1-message-loop/
  Cargo.toml
  src/
    main.rs              # Entry point, tao event loop
    cef_glue.rs          # CefInitialize, CefShutdown, browser creation
    message_pump.rs      # CFRunLoopTimer + OnScheduleMessagePumpWork
    cef_app_protocol.rs  # Runtime protocol injection (if needed)
```

### 2.3 Dependencies

```toml
[package]
name = "spike-1-message-loop"
version = "0.0.1"
edition = "2021"

[dependencies]
tao = "0.33"                       # Latest tao with macOS support
cef = { git = "https://github.com/aspect-build/aspect-cef", branch = "main" }
# OR if cef-rs from tauri-apps:
# cef = { git = "https://github.com/nicehash/aspect-cef" }
core-foundation = "0.10"           # CFRunLoop bindings
core-foundation-sys = "0.8"       # CFRunLoop raw types
objc2 = "0.6"                     # For runtime protocol injection
raw-window-handle = "0.6"         # Window handle extraction
```

Note: If `cef-rs` from tauri-apps is not publicly buildable, fall back to `cef-dll-sys` + raw FFI calls for the spike.

### 2.4 Core Architecture

```
                    ┌──────────────────────────────────────────┐
                    │           Main Thread (CFRunLoop)         │
                    │                                          │
                    │  ┌─────────────┐  ┌──────────────────┐  │
                    │  │ tao's       │  │ Our              │  │
                    │  │ observers + │  │ CFRunLoopTimer    │  │
                    │  │ timer       │  │ (30fps baseline)  │  │
                    │  │             │  │                   │  │
                    │  │ handles:    │  │ fires:            │  │
                    │  │ - window    │  │ CefDoMessageLoop  │  │
                    │  │   events    │  │ Work()            │  │
                    │  │ - user      │  │                   │  │
                    │  │   events    │  │ rescheduled by:   │  │
                    │  │ - redraws   │  │ OnScheduleMessage │  │
                    │  └─────────────┘  │ PumpWork()        │  │
                    │                   └──────────────────┘  │
                    │                                          │
                    │  [NSApp run] drives CFRunLoop             │
                    └──────────────────────────────────────────┘
                                       ▲
                                       │ CFRunLoopTimerSetNextFireDate()
                                       │ (thread-safe)
                    ┌──────────────────┘
                    │
          ┌─────────────────┐
          │ CEF IO/UI thread │
          │                  │
          │ OnScheduleMsg    │
          │ PumpWork(delay)  │
          └──────────────────┘
```

### 2.5 Code Sketch: message_pump.rs

```rust
use core_foundation::runloop::*;
use core_foundation_sys::runloop::*;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

/// Global timer reference -- only accessed to reschedule
static mut CEF_TIMER: CFRunLoopTimerRef = std::ptr::null_mut();
static INIT: Once = Once::new();

/// Baseline interval: ~33ms (30fps)
const BASELINE_INTERVAL_SEC: f64 = 1.0 / 30.0;

/// Called once during CefInitialize setup, BEFORE tao's run() starts.
/// Also safe to call after run() starts -- CFRunLoopAddTimer is fine
/// on an already-running run loop.
pub fn install_cef_timer() {
    unsafe {
        INIT.call_once(|| {
            let timer = CFRunLoopTimerCreate(
                std::ptr::null_mut(),       // allocator
                CFAbsoluteTimeGetCurrent(), // first fire: now
                BASELINE_INTERVAL_SEC,      // interval: 33ms
                0,                          // flags
                0,                          // order
                cef_timer_callback,         // callback
                std::ptr::null_mut(),       // context
            );
            CFRunLoopAddTimer(
                CFRunLoopGetMain(),
                timer,
                kCFRunLoopCommonModes,
            );
            CEF_TIMER = timer;
        });
    }
}

/// Timer callback -- runs on the main thread within CFRunLoop
extern "C" fn cef_timer_callback(
    _timer: CFRunLoopTimerRef,
    _info: *mut c_void,
) {
    // Safety: CefDoMessageLoopWork() must be called on the main thread.
    // This callback fires on the main thread because the timer is on
    // CFRunLoopGetMain().
    unsafe {
        cef_sys::cef_do_message_loop_work();
    }
}

/// Called from CefBrowserProcessHandler::OnScheduleMessagePumpWork()
/// from ANY thread. Must reschedule the timer.
pub fn on_schedule_message_pump_work(delay_ms: i64) {
    unsafe {
        if CEF_TIMER.is_null() {
            return;
        }
        let fire_date = if delay_ms <= 0 {
            // Fire as soon as possible
            CFAbsoluteTimeGetCurrent()
        } else {
            // Fire after delay_ms milliseconds
            CFAbsoluteTimeGetCurrent() + (delay_ms as f64 / 1000.0)
        };
        // CFRunLoopTimerSetNextFireDate is thread-safe
        CFRunLoopTimerSetNextFireDate(CEF_TIMER, fire_date);
    }
}

/// Cleanup -- call during CefShutdown
pub fn remove_cef_timer() {
    unsafe {
        if !CEF_TIMER.is_null() {
            CFRunLoopTimerInvalidate(CEF_TIMER);
            CEF_TIMER = std::ptr::null_mut();
        }
    }
}
```

### 2.6 Code Sketch: main.rs

```rust
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;

mod cef_glue;
mod message_pump;

fn main() {
    // Step 1: Create tao event loop (creates NSApplication)
    let event_loop = EventLoop::new();

    // Step 2: Create tao window
    let window = WindowBuilder::new()
        .with_title("Spike 1: tao + CEF")
        .with_inner_size(tao::dpi::LogicalSize::new(1024.0, 768.0))
        .build(&event_loop)
        .expect("failed to create window");

    // Step 3: Initialize CEF (before event loop starts!)
    //   - Sets external_message_pump = true
    //   - Installs CFRunLoopTimer via message_pump::install_cef_timer()
    //   - CefBrowserProcessHandler stores reference for
    //     OnScheduleMessagePumpWork callback
    let _cef_context = cef_glue::initialize_cef();

    // Step 4: Create browser as child of tao window
    let raw_handle = window.raw_window_handle(); // RawWindowHandle::AppKit
    cef_glue::create_browser(raw_handle, "https://example.com");

    // Step 5: Run tao event loop (calls [NSApp run], never returns)
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Shutdown CEF before exiting
                cef_glue::shutdown_cef();
                message_pump::remove_cef_timer();
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // Notify CEF of resize
                cef_glue::notify_resize(size);
            }
            _ => {}
        }
    });
}
```

### 2.7 Code Sketch: cef_glue.rs (abbreviated)

```rust
use std::ffi::CString;

pub fn initialize_cef() -> CefContext {
    let mut settings = cef::CefSettings::default();
    settings.external_message_pump = true;
    settings.multi_threaded_message_loop = false;
    // Optional: settings.windowless_rendering_enabled = false;
    // (we use windowed mode, CEF renders into a child NSView)

    let app = MyCefApp::new(); // implements CefBrowserProcessHandler

    let main_args = cef::CefMainArgs::new(); // argc/argv from std::env

    let result = cef::initialize(&main_args, &settings, Some(&app), None);
    assert!(result, "CefInitialize failed");

    // Install the CFRunLoopTimer AFTER CefInitialize succeeds
    super::message_pump::install_cef_timer();

    CefContext { app }
}

pub fn create_browser(window_handle: RawWindowHandle, url: &str) {
    let mut window_info = cef::CefWindowInfo::default();

    // Extract NSView from window handle
    if let RawWindowHandle::AppKit(handle) = window_handle {
        // set_as_child configures CEF to create a child NSView
        // within the given parent NSView
        window_info.set_as_child(handle.ns_view.as_ptr() as *mut _);
    }

    let browser_settings = cef::CefBrowserSettings::default();
    let url = CString::new(url).unwrap();

    cef::browser_host_create_browser(
        &window_info,
        /* client */ &MyCefClient::new(),
        &url,
        &browser_settings,
        /* extra_info */ None,
        /* request_context */ None,
    );
}

/// CefBrowserProcessHandler implementation
impl CefBrowserProcessHandler for MyCefApp {
    fn on_schedule_message_pump_work(&self, delay_ms: i64) {
        // Dispatch to our CFRunLoopTimer rescheduler
        // This is called from ANY thread -- the timer reschedule
        // is thread-safe
        super::message_pump::on_schedule_message_pump_work(delay_ms);
    }
}
```

### 2.8 CefAppProtocol Injection (if needed)

```rust
// cef_app_protocol.rs
// Only needed if CEF checks [NSApp conformsToProtocol:@protocol(CrAppProtocol)]
// at runtime even with external_message_pump = true.

use objc2::runtime::{AnyClass, Sel, Bool};

/// Inject CefAppProtocol conformance into tao's NSApplication class
/// at runtime using Objective-C runtime APIs.
pub unsafe fn inject_cef_app_protocol() {
    use std::ffi::CStr;

    let app_class = objc2::msg_send![
        objc2::class!(NSApplication),
        class
    ];

    // Add isHandlingSendEvent method
    let sel_is_handling = Sel::register(
        CStr::from_bytes_with_nul(b"isHandlingSendEvent\0").unwrap()
    );
    // Always return NO -- with external_message_pump, CEF doesn't
    // call sendEvent, so this is never meaningfully queried
    extern "C" fn is_handling_send_event(
        _this: &objc2::runtime::AnyObject,
        _sel: Sel,
    ) -> Bool {
        Bool::NO
    }
    objc2::runtime::class_addMethod(
        app_class,
        sel_is_handling,
        is_handling_send_event as _,
        "B@:".as_ptr() as _,
    );

    // Add setHandlingSendEvent: method
    let sel_set_handling = Sel::register(
        CStr::from_bytes_with_nul(b"setHandlingSendEvent:\0").unwrap()
    );
    extern "C" fn set_handling_send_event(
        _this: &objc2::runtime::AnyObject,
        _sel: Sel,
        _handling: Bool,
    ) {
        // No-op with external_message_pump
    }
    objc2::runtime::class_addMethod(
        app_class,
        sel_set_handling,
        set_handling_send_event as _,
        "v@:B".as_ptr() as _,
    );
}
```

---

## 3. Success Criteria

| # | Criterion | How to verify |
|---|-----------|---------------|
| 1 | tao window opens and is responsive | Move, resize, minimize the window |
| 2 | CEF browser renders inside the tao window | `https://example.com` content is visible |
| 3 | No event loop stalls | Window events are processed promptly (< 16ms latency) |
| 4 | No keyboard/mouse event loss | Type in a CEF text input, verify all characters arrive |
| 5 | `OnScheduleMessagePumpWork` fires | Add logging, verify it is called and triggers timer rescheduling |
| 6 | Clean shutdown | Close window -> CefShutdown -> process exits with code 0 |
| 7 | No crashes or assertions | No `conformsToProtocol` failures or reentrancy crashes |

### Bonus measurements:
- `CefInitialize()` wall time (expected: 200-500ms)
- First paint latency (CefInitialize -> page visible)
- CPU usage at idle (should be < 5% with external_message_pump)

---

## 4. Known Risks

### Risk 1: CefAppProtocol check (MEDIUM)
**What**: CEF may check at runtime that `[NSApp conformsToProtocol:@protocol(CrAppProtocol)]` and crash/fail if it does not.
**Mitigation**: Runtime method injection (section 2.8). If that fails, we may need to create our own NSApplication subclass BEFORE tao creates one -- which may require a tao fork or using `EventLoopBuilder` with a custom NSApplication class.
**Fallback**: File an issue on tao to expose NSApplication class customization.

### Risk 2: tao creates NSApplication too early (LOW)
**What**: If `EventLoop::new()` creates the NSApplication singleton, we cannot substitute our own subclass later.
**Mitigation**: Verified that `EventLoop::new()` does create NSApplication. But with `external_message_pump = true`, CEF should not need a custom NSApplication subclass -- it only needs the protocol for `sendEvent` reentrancy tracking, which is irrelevant when CEF does not call `sendEvent`. To be empirically verified.

### Risk 3: CFRunLoopTimer reentrancy (LOW)
**What**: Could `CefDoMessageLoopWork()` called from our timer callback interfere with tao's observer callbacks?
**Mitigation**: CFRunLoop processes timers and observers in a well-defined order within each iteration. Our timer fires between observer calls, not inside them. `CefDoMessageLoopWork()` with `external_message_pump = true` only processes internal Chromium tasks, not OS events, so there is no reentrancy risk with NSApplication event handling.

### Risk 4: CEF subprocess helper bundle (MEDIUM)
**What**: CEF on macOS requires a helper app bundle for renderer/GPU/utility subprocesses. The spike must package this correctly or CEF will fail to spawn subprocesses.
**Mitigation**: Use `cef-rs`'s build infrastructure which handles helper bundle generation. For the spike, can also use `--single-process` mode (not for production, but sufficient to validate message loop coexistence).

### Risk 5: cef-rs build complexity (MEDIUM)
**What**: `cef-rs` requires CMake, CEF binary download, and specific directory structure. May be difficult to get building for a spike.
**Mitigation**: If `cef-rs` is too complex, fall back to raw `cef-dll-sys` FFI calls. The message pump integration only needs `cef_initialize()`, `cef_do_message_loop_work()`, `cef_shutdown()`, and `cef_browser_host_create_browser()` -- all C functions.

### Risk 6: Window handle extraction (LOW)
**What**: Need to extract the NSView from tao's window to pass to CEF's `WindowInfo::set_as_child()`.
**Mitigation**: tao implements `HasRawWindowHandle` which provides `RawWindowHandle::AppKit { ns_view }`. This is the standard mechanism.

---

## 5. Decision Record

Based on this research, the recommended approach for wrymium's message loop integration on macOS is:

1. **CefInitialize** during `WebViewBuilder::build()` (after tao creates the window, before `run()`)
2. **CFRunLoopTimer** at 30fps baseline, added to `CFRunLoopGetMain()` with `kCFRunLoopCommonModes`
3. **OnScheduleMessagePumpWork** reschedules the timer via `CFRunLoopTimerSetNextFireDate()` (thread-safe)
4. **external_message_pump = true** to avoid OS event queue conflicts
5. **Runtime protocol injection** for CefAppProtocol if needed (verify empirically first)

This approach:
- Requires NO changes to tao
- Requires NO changes to tauri-runtime-wry (the timer is installed by wrymium internally)
- Is invisible to the caller -- wrymium handles all message pump integration in its `build()` and `Drop` implementations
- Follows the same patterns that tao itself uses (CFRunLoopTimer on the main run loop)

---

## Sources

- [tao macOS observer.rs](https://github.com/tauri-apps/tao/blob/dev/src/platform_impl/macos/observer.rs) -- CFRunLoopObserver and EventLoopWaker implementation
- [tao macOS event_loop.rs](https://github.com/tauri-apps/tao/blob/dev/src/platform_impl/macos/event_loop.rs) -- EventLoop::run() and Proxy implementation
- [CEF cef_app.h](https://github.com/chromiumembedded/cef/blob/master/include/cef_app.h) -- CefInitialize, CefDoMessageLoopWork documentation
- [CEF cef_browser_process_handler.h](https://github.com/chromiumembedded/cef/blob/master/include/cef_browser_process_handler.h) -- OnScheduleMessagePumpWork
- [CEF cef_application_mac.h](https://github.com/chromiumembedded/cef/blob/master/include/cef_application_mac.h) -- CefAppProtocol requirements
- [CEF Issue #1805: External message pump](https://bitbucket.org/chromiumembedded/cef/issues/1805/improve-support-for-a-host-owned-message) -- Design of external_message_pump feature
- [CEF Issue #2968: External message pump docs](https://bitbucket.org/chromiumembedded/cef/issues/2968/documentation-of-external-message-pump) -- Clarification that CefDoMessageLoopWork must also be called periodically
- [cefclient macOS entry point](https://github.com/Mojang/cef/blob/master/tests/cefclient/cefclient_mac.mm) -- Reference implementation showing CefInitialize before message loop
- [Sokol + CEF macOS issue](https://github.com/floooh/sokol/issues/737) -- Prior art: external_message_pump resolves coexistence
- [CEF Forum: Mac keyboard event loss](https://www.magpcss.org/ceforum/viewtopic.php?f=6&t=11141) -- Why external_message_pump is needed
- [CEFPython external message pump](https://github.com/cztomczak/cefpython/issues/246) -- Detailed implementation reference
- [BriskBard: Tuning external message pump performance](https://www.briskbard.com/forum/viewtopic.php?t=275) -- Performance tuning guidance
- [tao EventLoopProxy docs](https://docs.rs/tao/latest/tao/event_loop/struct.EventLoopProxy.html) -- Send + Sync proxy for cross-thread events
