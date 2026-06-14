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

  // Smart local timestamp: "14:32" today, else "Jun 13 14:32".
  function fmtTs(d) {
    var now = new Date();
    var sameDay =
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate();
    var time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (sameDay) return time;
    var date = d.toLocaleDateString([], { month: "short", day: "numeric" });
    return date + " " + time;
  }

  function FakeNotification(title, opts) {
    opts = opts || {};
    send("deliver_notification", {
      title: String(title || ""),
      body: String(opts.body || ""),
      ts: fmtTs(new Date()),
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
