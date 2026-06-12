// Replaces window.Notification so WhatsApp's notifications become native ones
// delivered by Rust over IPC (spec §3.2). Runs before any page script.
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

  // IPC bootstrap may land after us — queue until it exists.
  var queue = [];
  function send(cmd, args) {
    if (!invoke(cmd, args) && queue.length < 50) queue.push([cmd, args]);
  }
  var drain = setInterval(function () {
    while (queue.length && invoke(queue[0][0], queue[0][1])) queue.shift();
    if (!queue.length && invoke("debug_log", { message: "notification-shim: IPC ready" })) {
      clearInterval(drain);
    }
  }, 100);

  function FakeNotification(title, opts) {
    opts = opts || {};
    send("deliver_notification", {
      title: String(title || ""),
      body: String(opts.body || ""),
    });
  }
  FakeNotification.permission = "granted";
  FakeNotification.requestPermission = function (cb) {
    var p = Promise.resolve("granted");
    if (cb) p.then(cb);
    return p;
  };
  // Instance API surface WhatsApp may touch — all no-ops.
  FakeNotification.prototype.close = function () {};
  FakeNotification.prototype.addEventListener = function () {};
  FakeNotification.prototype.removeEventListener = function () {};

  Object.defineProperty(window, "Notification", {
    value: FakeNotification,
    writable: true,
    enumerable: false,
    configurable: true,
  });

  send("debug_log", { message: "notification-shim: installed" });
})();
