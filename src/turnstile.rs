//! Cloudflare Turnstile for the Better Lyrics API, completed the same way the browser
//! extension does: the API's own challenge page runs in a real browser engine
//! (WebView2) and its widget reports the token. The widget uses
//! `appearance: interaction-only`, so it normally passes invisibly; if Cloudflare wants
//! an interaction, the window is shown so the user can complete it. Nothing here
//! solves or forges the challenge.
//!
//! Like the sign-in window, this runs in a child process (`ytm-rs --turnstile <dir>`)
//! because tao and winit can't share a process.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

use crate::login::BuildIn;

pub const FLAG: &str = "--turnstile";
const CHALLENGE: &str = "https://lyrics.api.dacubeking.com/challenge";
/// Give up if nothing happens (e.g. the user ignores a shown challenge).
const GIVE_UP_AFTER: Duration = Duration::from_secs(120);

/// The challenge page posts to `window.parent`; at top level that is the page itself,
/// so forward those messages to the native side. Also detect when Cloudflare Turnstile
/// transitions to interactive mode (so the window is only shown if interaction is needed).
const BRIDGE: &str = r#"
window.addEventListener('message', function (e) {
  if (e.data && typeof e.data.type === 'string' && e.data.type.indexOf('turnstile-') === 0) {
    window.ipc.postMessage(JSON.stringify(e.data));
  }
});

(function() {
  function applyTheme() {
    if (document.body) {
      document.body.style.backgroundColor = '#1E1F22';
      document.body.style.color = '#FFFFFF';
    }
  }
  if (document.body) {
    applyTheme();
  } else {
    window.addEventListener('DOMContentLoaded', applyTheme);
  }

  function notifyInteractive() {
    window.ipc.postMessage(JSON.stringify({ type: 'turnstile-interactive' }));
  }

  window.onTurnstileBeforeInteractive = notifyInteractive;

  function checkInteractive() {
    var el = document.querySelector('.cf-turnstile');
    if (el && !el.getAttribute('data-before-interactive-callback')) {
      el.setAttribute('data-before-interactive-callback', 'onTurnstileBeforeInteractive');
    }
    var iframes = document.querySelectorAll('iframe');
    for (var i = 0; i < iframes.length; i++) {
      var f = iframes[i];
      var rect = f.getBoundingClientRect();
      var style = window.getComputedStyle(f);
      if (style.display !== 'none' && style.visibility !== 'hidden' && rect.height > 35 && rect.width > 80) {
        notifyInteractive();
        return;
      }
    }
  }
  window.checkInteractive = checkInteractive;

  var _ts = undefined;
  try {
    Object.defineProperty(window, 'turnstile', {
      configurable: true,
      enumerable: true,
      get: function() { return _ts; },
      set: function(val) {
        _ts = val;
        if (val && typeof val.render === 'function') {
          var origRender = val.render;
          val.render = function(container, options) {
            if (options && typeof options === 'object') {
              var origBefore = options['before-interactive-callback'];
              options['before-interactive-callback'] = function() {
                notifyInteractive();
                if (typeof origBefore === 'function') origBefore();
              };
            }
            return origRender.call(val, container, options);
          };
        }
      }
    });
  } catch (e) {}

  if (typeof MutationObserver !== 'undefined') {
    var observer = new MutationObserver(function() {
      checkInteractive();
    });
    if (document.documentElement) {
      observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true });
    } else {
      window.addEventListener('DOMContentLoaded', function() {
        observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true });
      });
    }
  }

  setInterval(checkInteractive, 300);
  window.addEventListener('DOMContentLoaded', checkInteractive);
})();
"#;

/// Run the challenge in a child process; returns the Turnstile token.
pub fn obtain(profile_dir: PathBuf) -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    crate::proxy::pass_to(&mut cmd);
    let out = cmd
        .arg(FLAG)
        .arg(&profile_dir)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().last().unwrap_or_default();
    if let Some(t) = line.strip_prefix("token ") {
        Ok(t.to_owned())
    } else if let Some(e) = line.strip_prefix("failed ") {
        anyhow::bail!("lyrics verification failed: {e}")
    } else {
        anyhow::bail!("lyrics verification window exited ({})", out.status)
    }
}

/// Entry point of the child process.
pub fn child_main(profile_dir: PathBuf) {
    match run(profile_dir) {
        Ok(token) => println!("token {token}"),
        Err(e) => println!("failed {e:#}"),
    }
}

fn run(profile_dir: PathBuf) -> anyhow::Result<String> {
    let mut event_loop = EventLoopBuilder::<String>::with_user_event().build();
    // macOS: no Dock icon for a window that's usually never shown.
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let window = WindowBuilder::new()
        .with_title("Verifying lyrics access")
        .with_inner_size(tao::dpi::LogicalSize::new(360.0, 160.0))
        .with_theme(Some(tao::window::Theme::Dark))
        .with_resizable(false)
        .with_visible(false)
        .build(&event_loop)?;

    if let Some(monitor) = window.primary_monitor().or_else(|| window.current_monitor()) {
        let screen_size = monitor.size();
        let scale = monitor.scale_factor();
        let win_w = (360.0 * scale) as i32;
        let win_h = (160.0 * scale) as i32;
        let x = (screen_size.width as i32 - win_w) / 2;
        let y = (screen_size.height as i32 - win_h) / 2;
        window.set_outer_position(tao::dpi::PhysicalPosition::new(x.max(0), y.max(0)));
    }

    let proxy = event_loop.create_proxy();
    let mut context = WebContext::new(Some(profile_dir));
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_background_color((0x1E, 0x1F, 0x22, 255));
    let webview = crate::proxy::webview(builder, false)
        .with_initialization_script(BRIDGE)
        .with_ipc_handler(move |req| {
            let _ = proxy.send_event(req.body().clone());
        })
        .with_url(CHALLENGE)
        .build_in(&window)?;

    let started = Instant::now();
    let mut result: anyhow::Result<String> = Err(anyhow::anyhow!("closed before verification finished"));
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(500));
        match event {
            Event::UserEvent(msg) => {
                let v: serde_json::Value = serde_json::from_str(&msg).unwrap_or_default();
                match v["type"].as_str() {
                    Some("turnstile-token") => {
                        result = v["token"]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| anyhow::anyhow!("empty token"));
                        *flow = ControlFlow::Exit;
                    }
                    Some("turnstile-interactive") => {
                        // Cloudflare requires an interactive check (e.g. checkbox);
                        // reveal the widget for the user to complete.
                        if !window.is_visible() {
                            window.set_inner_size(tao::dpi::LogicalSize::new(360.0, 160.0));
                            window.set_visible(true);
                            window.set_focus();
                        }
                    }
                    Some("turnstile-error") => {
                        result = Err(anyhow::anyhow!("challenge error {}", v["error"]));
                        *flow = ControlFlow::Exit;
                    }
                    Some("turnstile-timeout") => {
                        result = Err(anyhow::anyhow!("challenge timed out"));
                        *flow = ControlFlow::Exit;
                    }
                    // Expired before use: ask the widget for a fresh one.
                    Some("turnstile-expired") => {
                        let _ = webview.evaluate_script("if (window.turnstile) turnstile.reset();");
                    }
                    _ => {}
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let waited = started.elapsed();
                if waited > GIVE_UP_AFTER {
                    result = Err(anyhow::anyhow!("timed out waiting for verification"));
                    *flow = ControlFlow::Exit;
                } else if waited > Duration::from_secs(20) && !window.is_visible() {
                    let _ = webview.evaluate_script("if (typeof checkInteractive === 'function') checkInteractive();");
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            _ => {}
        }
    });
    drop(webview);
    result
}
