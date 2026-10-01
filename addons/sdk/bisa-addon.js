/*!
 * bisa-addon — the one door an addon has into Bisa.
 *
 * An addon runs in a sandboxed frame: an opaque origin, a policy that lets it
 * fetch nothing, no token, no file, no program. What it may do it asks the
 * desktop for through `postMessage`, and the desktop answers only what the
 * person granted. This file is that conversation, as promises on
 * `window.bisa`. It is served by the platform itself at every addon's root —
 * `<script src="bisa-addon.js"></script>` — so a bundle never ships it.
 *
 * Protocol, version 1 (docs/reference/addon-api.md):
 *   addon → host   { v: 1, kind: "bisa:hello" }
 *                  { v: 1, kind: "bisa:call", id, method, params }
 *   host → addon   { v: 1, kind: "bisa:ready", addon, granted, locale, theme, window }
 *                  { v: 1, kind: "bisa:reply", id, ok: true, result }
 *                  { v: 1, kind: "bisa:reply", id, ok: false, error: { code, message } }
 *                  { v: 1, kind: "bisa:event", topic, payload }
 *
 * Only the frame's own parent is listened to, and only after it said
 * `bisa:ready`; a call made before that waits for it. A call nobody answers
 * within ten seconds fails with the code `unavailable`.
 */
(function (global) {
  "use strict";

  var VERSION = 1;
  var CALL_TIMEOUT_MS = 10000;

  /** Every method the host knows, and the shape the sugar below wears. */
  var METHODS = [
    "platform.info",
    "theme.get",
    "system.load.subscribe",
    "system.load.unsubscribe",
    "workspace.summary",
    "notify.show",
    "clipboard.write",
    "storage.get",
    "storage.set",
    "storage.remove",
    "storage.keys",
    "network.fetch",
    "url.open",
    "navigate",
    "window.resize",
    "window.close",
    "window.setTitle",
  ];

  /** The topics the host pushes. */
  var EVENTS = ["theme", "system.load", "workspace.summary", "visibility"];

  var parent = global.parent;
  var hello = null;
  var readyResolvers = [];
  var pending = {};
  var listeners = {};
  var seq = 0;

  function AddonError(code, message) {
    var e = new Error(message || code);
    e.name = "AddonError";
    e.code = code;
    return e;
  }

  function nextId() {
    seq += 1;
    return "c" + seq.toString(36) + "-" + Math.random().toString(36).slice(2, 10);
  }

  function post(message) {
    if (!parent || parent === global) return;
    parent.postMessage(message, "*");
  }

  function whenReady() {
    if (hello) return Promise.resolve(hello);
    return new Promise(function (resolve) {
      readyResolvers.push(resolve);
    });
  }

  function call(method, params) {
    if (METHODS.indexOf(method) === -1) {
      return Promise.reject(AddonError("unknown_method", method + " is not a method the platform offers"));
    }
    var safeParams = params && typeof params === "object" ? params : {};
    return whenReady().then(function () {
      return new Promise(function (resolve, reject) {
        var id = nextId();
        var timer = global.setTimeout(function () {
          delete pending[id];
          reject(AddonError("unavailable", "the platform did not answer " + method));
        }, CALL_TIMEOUT_MS);
        pending[id] = { resolve: resolve, reject: reject, timer: timer };
        post({ v: VERSION, kind: "bisa:call", id: id, method: method, params: safeParams });
      });
    });
  }

  function on(topic, fn) {
    if (EVENTS.indexOf(topic) === -1 || typeof fn !== "function") return function () {};
    (listeners[topic] = listeners[topic] || []).push(fn);
    return function () {
      off(topic, fn);
    };
  }

  function off(topic, fn) {
    var list = listeners[topic];
    if (!list) return;
    var at = list.indexOf(fn);
    if (at !== -1) list.splice(at, 1);
  }

  function emit(topic, payload) {
    var list = listeners[topic];
    if (!list) return;
    list.slice().forEach(function (fn) {
      try {
        fn(payload);
      } catch (_e) {
        /* a listener's fault stays the listener's */
      }
    });
  }

  function onMessage(event) {
    // The frame's own parent, and nobody else — whatever the origin says.
    if (!parent || event.source !== parent) return;
    var m = event.data;
    if (!m || typeof m !== "object" || m.v !== VERSION || typeof m.kind !== "string") return;
    if (m.kind === "bisa:ready") {
      hello = {
        addon: m.addon || {},
        granted: Array.isArray(m.granted) ? m.granted.slice() : [],
        locale: typeof m.locale === "string" ? m.locale : "en",
        theme: m.theme && typeof m.theme === "object" ? m.theme : { scheme: "light" },
        window: m.window && typeof m.window === "object" ? m.window : {},
      };
      var waiting = readyResolvers;
      readyResolvers = [];
      waiting.forEach(function (resolve) {
        resolve(hello);
      });
      return;
    }
    if (!hello) return;
    if (m.kind === "bisa:reply" && typeof m.id === "string") {
      var call_ = pending[m.id];
      if (!call_) return;
      delete pending[m.id];
      global.clearTimeout(call_.timer);
      if (m.ok) call_.resolve(m.result);
      else {
        var err = m.error && typeof m.error === "object" ? m.error : {};
        call_.reject(AddonError(typeof err.code === "string" ? err.code : "refused", err.message));
      }
      return;
    }
    if (m.kind === "bisa:event" && typeof m.topic === "string" && EVENTS.indexOf(m.topic) !== -1) {
      emit(m.topic, m.payload);
    }
  }

  function granted(word) {
    return !!hello && hello.granted.indexOf(word) !== -1;
  }

  var bisa = {
    version: VERSION,
    ready: whenReady,
    call: call,
    on: on,
    off: off,
    granted: granted,
    methods: function () {
      return METHODS.slice();
    },
    events: function () {
      return EVENTS.slice();
    },
    platform: {
      info: function () {
        return call("platform.info", {});
      },
    },
    theme: {
      get: function () {
        return call("theme.get", {});
      },
    },
    system: {
      load: {
        subscribe: function () {
          return call("system.load.subscribe", {});
        },
        unsubscribe: function () {
          return call("system.load.unsubscribe", {});
        },
      },
    },
    workspace: {
      summary: function () {
        return call("workspace.summary", {});
      },
    },
    notify: {
      show: function (opts) {
        return call("notify.show", opts);
      },
    },
    clipboard: {
      write: function (opts) {
        return call("clipboard.write", typeof opts === "string" ? { text: opts } : opts);
      },
    },
    storage: {
      get: function (key) {
        return call("storage.get", { key: key });
      },
      set: function (key, value) {
        return call("storage.set", { key: key, value: value });
      },
      remove: function (key) {
        return call("storage.remove", { key: key });
      },
      keys: function () {
        return call("storage.keys", {});
      },
    },
    network: {
      fetch: function (opts) {
        return call("network.fetch", typeof opts === "string" ? { url: opts } : opts);
      },
    },
    url: {
      open: function (opts) {
        return call("url.open", typeof opts === "string" ? { url: opts } : opts);
      },
    },
    navigate: function (opts) {
      return call("navigate", typeof opts === "string" ? { route: opts } : opts);
    },
    window: {
      resize: function (opts) {
        return call("window.resize", opts);
      },
      close: function () {
        return call("window.close", {});
      },
      setTitle: function (title) {
        return call("window.setTitle", { title: title });
      },
    },
  };

  global.addEventListener("message", onMessage);
  global.bisa = bisa;
  post({ v: VERSION, kind: "bisa:hello" });
})(typeof window !== "undefined" ? window : this);
