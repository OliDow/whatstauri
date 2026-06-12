// Ships the raw document title to Rust (parsing happens Rust-side, spec §3.3)
// and holds a WebLock so WebKitGTK doesn't over-throttle the hidden page (§6.5).
(function () {
  "use strict";

  // Only the top frame talks to Rust; iframe copies exit immediately.
  try { if (window.top !== window.self) return; } catch (e) { return; }

  function invoke(cmd, args) {
    var i = window.__TAURI_INTERNALS__;
    if (i && typeof i.invoke === "function") {
      i.invoke(cmd, args).catch(function () {});
      return true;
    }
    return false;
  }

  // Throttling insurance: a held lock keeps the page schedulable while hidden.
  if (navigator.locks && navigator.locks.request) {
    navigator.locks.request("whatstauri-keepalive", function () {
      return new Promise(function () {}); // held forever
    }).catch(function () {});
  }

  var lastSent = null;
  function tick() {
    var t = document.title;
    if (t !== lastSent && invoke("report_title", { title: String(t) })) {
      lastSent = t;
    }
  }
  setInterval(tick, 1000); // polling beats MutationObserver here: <title> may not exist yet at init-script time

  var announce = setInterval(function () {
    if (invoke("debug_log", { message: "title-watcher: attached" })) clearInterval(announce);
  }, 100);
})();
