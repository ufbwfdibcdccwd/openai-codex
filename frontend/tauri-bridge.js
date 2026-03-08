// @ts-nocheck
/**
 * Codex Windows — Tauri Native Bridge
 *
 * 100% Tauri v2 implementation. Zero Electron runtime or dependencies.
 * The React bundle (index-BUvz-C55.js) is a compiled black-box that expects
 * an interface named `window.electronBridge` and HTML attributes like
 * data-codex-window-type="electron". These are INTERFACE NAMES imposed by the
 * compiled bundle, NOT Electron dependencies. The implementation behind them
 * is entirely Tauri IPC (invoke / listen / emit).
 *
 * Communication channels (all via Tauri IPC):
 *   1. type:"fetch" (vscode://codex/...) → handled locally (67+ handlers)
 *   2. type:"mcp-request"               → invoke('send_to_codex') → Rust → codex.exe
 *   3. type:"mcp-notification"          → invoke('send_to_codex') → Rust → codex.exe
 *   4. type:"mcp-response"              → invoke('send_to_codex') → Rust → codex.exe
 *   5. internal messages                → handled locally
 *   6. terminal-*                       → invoke('create_terminal'/etc.) → Rust
 *   7. file dialogs                     → invoke('pick_folder'/etc.) → Rust (rfd)
 *
 * React never sends JSON-RPC. This bridge translates.
 */
(function () {
  "use strict";

  // --- Disable Sentry renderer auto-init ---
  // The compiled React bundle includes Sentry SDK code that tries to use
  // sentry-ipc:// protocol (an Electron-only IPC scheme). Setting this flag
  // is Sentry's documented way to skip renderer-side init. No Sentry data
  // is collected — this is a standalone Tauri app, not an Electron app.
  window.__SENTRY__RENDERER_INIT__ = true;

  // --- Tauri APIs (requires withGlobalTauri: true in tauri.conf.json) --------
  var __TAURI__ = window.__TAURI__;
  var invoke = __TAURI__.core.invoke;
  var listen = __TAURI__.event.listen;

  // --- Configuration (no more query strings — Rust provides context) ---------
  var HOST_ID = "local";
  var CWD = "";
  var HOMEDIR = "";
  var SESSION_ID = "";

  // --- Logging (replaces XHR POST /diag with Tauri invoke) ------------------
  var DEBUG = false;
  var diagQueue = [];
  var diagFlushing = false;

  function diag(tag, data) {
    var entry = {
      t: Date.now(),
      tag: tag,
      d:
        typeof data === "object"
          ? JSON.stringify(data).substring(0, 500)
          : String(data).substring(0, 500),
    };
    diagQueue.push(entry);
    console.log("[bridge:" + tag + "]", entry.d);
    if (!diagFlushing) {
      diagFlushing = true;
      setTimeout(flushDiag, 200);
    }
  }

  function flushDiag() {
    diagFlushing = false;
    if (diagQueue.length === 0) return;
    var batch = diagQueue.splice(0, 50);
    try {
      invoke("log_diag", { entries: batch });
    } catch (_e) {}
  }

  function log(tag, msg) {
    if (DEBUG) diag(tag, msg);
  }

  // Capture runtime errors
  window.addEventListener("error", function (ev) {
    try {
      var msg = ev && ev.message ? ev.message : "Unknown error";
      var src = ev && ev.filename ? ev.filename : "";
      var line = ev && ev.lineno ? ev.lineno : 0;
      var col = ev && ev.colno ? ev.colno : 0;
      var stack =
        ev && ev.error && ev.error.stack
          ? String(ev.error.stack).substring(0, 3000)
          : "";
      diag(
        "JS-ERROR",
        JSON.stringify({
          message: msg,
          source: src,
          line: line,
          column: col,
          stack: stack,
        }).substring(0, 3000)
      );
    } catch (_e) {}
  });

  window.addEventListener("unhandledrejection", function (ev) {
    try {
      var reason = ev && ev.reason ? ev.reason : "Unknown rejection";
      var text = typeof reason === "string" ? reason : JSON.stringify(reason);
      diag("JS-REJECTION", String(text || "").substring(0, 500));
    } catch (_e) {}
  });

  // --- Workspace roots management (localStorage) ------------------------------
  var WS_ROOTS_KEY = "codex_workspace_roots";
  var WS_ACTIVE_KEY = "codex_active_roots";
  var WS_LABELS_KEY = "codex_workspace_labels";

  function loadWorkspaceRoots() {
    try {
      var raw = localStorage.getItem(WS_ROOTS_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch (_e) { return []; }
  }
  function saveWorkspaceRoots(roots) {
    try { localStorage.setItem(WS_ROOTS_KEY, JSON.stringify(roots)); } catch (_e) {}
  }
  function loadActiveRoots() {
    try {
      var raw = localStorage.getItem(WS_ACTIVE_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch (_e) { return []; }
  }
  function saveActiveRoots(roots) {
    try { localStorage.setItem(WS_ACTIVE_KEY, JSON.stringify(roots)); } catch (_e) {}
  }
  function loadWorkspaceLabels() {
    try {
      var raw = localStorage.getItem(WS_LABELS_KEY);
      return raw ? JSON.parse(raw) : {};
    } catch (_e) { return {}; }
  }
  function saveWorkspaceLabels(labels) {
    try { localStorage.setItem(WS_LABELS_KEY, JSON.stringify(labels)); } catch (_e) {}
  }

  function getActiveWorkspaceRoots() {
    var active = loadActiveRoots();
    if (active.length > 0) return active;
    // Fallback: use CWD if no explicit active roots set
    return CWD ? [CWD] : [];
  }

  function getAllWorkspaceRootOptions() {
    var roots = loadWorkspaceRoots();
    var active = getActiveWorkspaceRoots();
    // Ensure active roots are always in the options list
    active.forEach(function(r) {
      if (roots.indexOf(r) === -1) roots.push(r);
    });
    // Ensure CWD is always an option
    if (CWD && roots.indexOf(CWD) === -1) roots.push(CWD);
    return roots;
  }

  // --- Global State Storage (localStorage) ------------------------------------
  var GS_KEY = "codex_global_state";
  var globalState = {};
  try {
    var gsRaw = localStorage.getItem(GS_KEY);
    if (gsRaw) globalState = JSON.parse(gsRaw);
  } catch (_e) {}

  function saveGlobalState() {
    try {
      localStorage.setItem(GS_KEY, JSON.stringify(globalState));
    } catch (_e) {}
  }

  // --- Persisted Atom Storage (localStorage) ----------------------------------
  var PERSIST_KEY = "codex_persisted_atoms";
  var persistedState = {};
  try {
    var raw = localStorage.getItem(PERSIST_KEY);
    if (raw) persistedState = JSON.parse(raw);
  } catch (_e) {}

  function savePersisted() {
    try {
      localStorage.setItem(PERSIST_KEY, JSON.stringify(persistedState));
    } catch (_e) {}
  }

  // --- Configuration Store (localStorage) ------------------------------------
  var CONFIG_KEY = "codex_config";
  var configDefaults = {
    "git-always-force-push": false,
    "git-branch-prefix": "codex/",
    "git-commit-instructions": "",
    "git-pr-instructions": "",
    "worktree-keep-count": 15,
  };
  var configStore = {};
  try {
    var cfgRaw = localStorage.getItem(CONFIG_KEY);
    if (cfgRaw) configStore = JSON.parse(cfgRaw);
  } catch (_e) {}

  function saveConfig() {
    try {
      localStorage.setItem(CONFIG_KEY, JSON.stringify(configStore));
    } catch (_e) {}
  }

  // --- Shared Object Store (in-memory) ----------------------------------------
  var sharedObjects = {};
  var sharedSubs = {};

  // --- MCP request tracking (requestId → hostId) ------------------------------
  var mcpPending = new Map();
  var mcpRequestMeta = new Map();

  // --- Auth cache (derived from MCP account data) ----------------------------
  var AUTH_CACHE_KEY = "codex_auth_cache_v1";
  var authCache = loadAuthCache();

  function defaultAuthCache() {
    return {
      authMethod: null, // chatgpt | apikey | null
      isLoggedIn: false,
      requiresOpenaiAuth: true,
      email: null,
      plan: null,
      accountId: null,
      userId: null,
      apiKey: null,
      loginInProgress: false,
      activeLoginId: null,
      lastUpdated: 0,
    };
  }

  function normalizePlan(plan) {
    if (plan == null) return null;
    var p = String(plan).toLowerCase();
    if (
      p === "free" ||
      p === "go" ||
      p === "plus" ||
      p === "pro" ||
      p === "team" ||
      p === "business" ||
      p === "enterprise" ||
      p === "edu" ||
      p === "unknown"
    ) {
      return p;
    }
    return null;
  }

  function normalizeAuthMethod(method) {
    if (method == null) return null;
    var m = String(method).toLowerCase();
    if (m === "chatgpt" || m === "chatgptauthtokens") return "chatgpt";
    if (m === "apikey" || m === "api_key" || m === "api-key") return "apikey";
    return null;
  }

  function loadAuthCache() {
    try {
      var raw = localStorage.getItem(AUTH_CACHE_KEY);
      if (!raw) return defaultAuthCache();
      var parsed = JSON.parse(raw);
      return Object.assign(defaultAuthCache(), parsed || {});
    } catch (_e) {
      return defaultAuthCache();
    }
  }

  function saveAuthCache() {
    authCache.lastUpdated = Date.now();
    try {
      localStorage.setItem(AUTH_CACHE_KEY, JSON.stringify(authCache));
    } catch (_e) {}
  }

  function resetAuthCache() {
    authCache.authMethod = null;
    authCache.isLoggedIn = false;
    authCache.requiresOpenaiAuth = true;
    authCache.email = null;
    authCache.plan = null;
    authCache.accountId = null;
    authCache.userId = null;
    authCache.loginInProgress = false;
    authCache.activeLoginId = null;
    saveAuthCache();
  }

  function setAuthMode(method) {
    var normalized = normalizeAuthMethod(method);
    authCache.authMethod = normalized;
    authCache.isLoggedIn = !!normalized;
    if (!normalized) {
      authCache.email = null;
      authCache.plan = null;
      authCache.accountId = null;
      authCache.userId = null;
      authCache.loginInProgress = false;
      authCache.activeLoginId = null;
    }
    saveAuthCache();
  }

  function applyAccountReadResult(result) {
    if (!result || typeof result !== "object") return;
    authCache.requiresOpenaiAuth = !!result.requiresOpenaiAuth;
    var acct = result.account;
    if (!acct || typeof acct !== "object") {
      resetAuthCache();
      authCache.requiresOpenaiAuth = !!result.requiresOpenaiAuth;
      saveAuthCache();
      return;
    }

    if (acct.type === "chatgpt") {
      authCache.authMethod = "chatgpt";
      authCache.isLoggedIn = true;
      authCache.email = acct.email || null;
      authCache.plan = normalizePlan(acct.planType);
      authCache.userId = acct.email || null;
      authCache.accountId = acct.email || null;
      authCache.loginInProgress = false;
      authCache.activeLoginId = null;
      saveAuthCache();
      return;
    }

    if (acct.type === "apiKey") {
      authCache.authMethod = "apikey";
      authCache.isLoggedIn = true;
      authCache.email = null;
      authCache.plan = null;
      authCache.userId = null;
      authCache.accountId = null;
      authCache.loginInProgress = false;
      authCache.activeLoginId = null;
      saveAuthCache();
    }
  }

  function trackMcpRequest(request, hostId) {
    if (!request || request.id == null) return;
    var id = String(request.id);
    mcpRequestMeta.set(id, {
      hostId: hostId || HOST_ID,
      method: request.method || null,
      params: request.params || {},
      ts: Date.now(),
    });

    if (mcpRequestMeta.size > 1000) {
      var oldestKey = null;
      var oldestTs = Infinity;
      mcpRequestMeta.forEach(function (meta, key) {
        if (meta && meta.ts < oldestTs) {
          oldestTs = meta.ts;
          oldestKey = key;
        }
      });
      if (oldestKey != null) mcpRequestMeta.delete(oldestKey);
    }
  }

  function processMcpResponse(msg) {
    if (!msg || msg.id == null) return;
    var id = String(msg.id);
    var meta = mcpRequestMeta.get(id);
    if (!meta) return;
    mcpRequestMeta.delete(id);

    var result = msg.result;
    if (msg.error) {
      if (
        meta.method === "account/login/start" ||
        meta.method === "account/login/cancel"
      ) {
        authCache.loginInProgress = false;
        authCache.activeLoginId = null;
        saveAuthCache();
      }
      return;
    }

    if (meta.method === "account/read") {
      applyAccountReadResult(result);
      return;
    }

    if (meta.method === "account/login/start") {
      var loginType = meta.params && meta.params.type;
      if (loginType === "apiKey") {
        if (
          meta.params &&
          typeof meta.params.apiKey === "string" &&
          meta.params.apiKey.length > 0
        ) {
          authCache.apiKey = meta.params.apiKey;
        }
        setAuthMode("apikey");
      } else if (loginType === "chatgpt") {
        authCache.loginInProgress = true;
        authCache.activeLoginId =
          result && typeof result.loginId === "string" ? result.loginId : null;
        saveAuthCache();

        // If the login response contains a URL, open it in the browser
        if (result && typeof result.url === "string" && result.url.length > 0) {
          console.log("[bridge] Opening ChatGPT login URL:", result.url);
          invoke("open_external_url", { url: result.url }).catch(function (e) {
            console.warn("[bridge] Failed to open login URL:", e);
            try { window.open(result.url, "_blank"); } catch (_err) {}
          });
        }
      }
      return;
    }

    if (meta.method === "account/login/cancel") {
      authCache.loginInProgress = false;
      authCache.activeLoginId = null;
      saveAuthCache();
      return;
    }

    if (meta.method === "account/logout") {
      resetAuthCache();
      // Force React to re-render login state
      setTimeout(function () {
        toReact({
          type: "fetch-response",
          requestId: "__logout_refresh__",
          responseType: "success",
          status: 200,
          headers: { "content-type": "application/json" },
          bodyJsonString: JSON.stringify({
            is_logged_in: false,
            user: null,
            accounts: [],
            account_ordering: [],
          }),
        });
      }, 100);
    }
  }

  function processMcpNotification(method, params) {
    if (!method) return;

    if (method === "account/updated") {
      setAuthMode(params && params.authMode);
      return;
    }

    if (method === "account/login/completed") {
      var success = !!(params && params.success);
      authCache.loginInProgress = false;
      authCache.activeLoginId = null;
      if (success) {
        authCache.authMethod = "chatgpt";
        authCache.isLoggedIn = true;
      }
      saveAuthCache();
      return;
    }

    // Handle OAuth login URL notifications from codex.exe
    // When codex.exe needs the user to authenticate, it sends a notification
    // with a URL that must be opened in the default browser.
    if (method === "mcpServer/oauthLogin/completed") {
      // OAuth flow completed
      return;
    }

    // Handle login URL that needs to be opened externally
    if (params && typeof params === "object" && params.url && typeof params.url === "string") {
      var lowerMethod = method.toLowerCase();
      if (lowerMethod.indexOf("login") !== -1 || lowerMethod.indexOf("auth") !== -1 || lowerMethod.indexOf("oauth") !== -1) {
        console.log("[bridge] Opening login URL externally:", params.url);
        invoke("open_external_url", { url: params.url }).catch(function (e) {
          console.warn("[bridge] Failed to open login URL:", e);
          try { window.open(params.url, "_blank"); } catch (_err) {}
        });
      }
    }
  }

  function processCodexMessage(data) {
    try {
      if (!data || typeof data !== "object") return;
      if (data.type === "mcp-response") {
        processMcpResponse(data.message || data.response || data);
        return;
      }
      if (data.type === "mcp-notification") {
        processMcpNotification(data.method, data.params || {});
        return;
      }
      // Handle server-initiated requests (codex.exe asking the client to do something)
      if (data.type === "mcp-request") {
        var req = data.request || data;
        if (req && req.method) {
          // If codex.exe sends a request to open a URL (e.g., for OAuth)
          if (req.method === "window/openUrl" || req.method === "window/openExternal") {
            var urlParam = req.params && (req.params.url || req.params.uri);
            if (urlParam) {
              console.log("[bridge] Server requested URL open:", urlParam);
              invoke("open_external_url", { url: urlParam }).catch(function (e) {
                try { window.open(urlParam, "_blank"); } catch (_err) {}
              });
              // Send back an acknowledgement
              if (req.id != null) {
                toBackend({
                  jsonrpc: "2.0",
                  id: req.id,
                  result: {},
                });
              }
            }
          }
        }
      }
    } catch (e) {
      console.warn("[bridge] processCodexMessage failed:", e);
    }
  }

  // --- Connection state -------------------------------------------------------
  var codexConnected = false;

  function makeConnectionMsg(state) {
    return {
      type: "codex-app-server-connection-changed",
      hostId: HOST_ID,
      state: state,
      transport: "tauri-ipc",
    };
  }

  // --- Dispatch to React -------------------------------------------------------
  var workerSubs = new Map();

  function toReact(data) {
    window.dispatchEvent(new MessageEvent("message", { data: data }));
    if (data && typeof data === "object" && data._workerName) {
      var subs = workerSubs.get(data._workerName);
      if (subs) {
        subs.forEach(function (cb) {
          try {
            cb(data);
          } catch (e) {
            console.error("[bridge] worker sub error:", e);
          }
        });
      }
    }
  }

  // --- Send to codex.exe via Rust backend ------------------------------------
  var pendingQueue = [];

  function toBackend(jsonRpcMsg) {
    log("WS-SEND", jsonRpcMsg);
    if (!codexConnected) {
      // Queue messages while disconnected — they'll be flushed on connect
      console.log("[bridge] Queuing message (not connected):", jsonRpcMsg.method || jsonRpcMsg.id);
      pendingQueue.push(jsonRpcMsg);
      return;
    }
    invoke("send_to_codex", { message: jsonRpcMsg }).catch(function (err) {
      console.warn("[bridge] send_to_codex failed:", err);
      // If send fails unexpectedly, queue for retry
      pendingQueue.push(jsonRpcMsg);
    });
  }

  function flushPendingQueue() {
    if (pendingQueue.length === 0) return;
    var queue = pendingQueue.splice(0);
    console.log("[bridge] Flushing " + queue.length + " queued messages");
    queue.forEach(function (msg) {
      invoke("send_to_codex", { message: msg }).catch(function (err) {
        console.warn("[bridge] Flush send failed:", err);
      });
    });
  }

  // --- Handle type:"fetch" (vscode://codex/...) --------------------------------
  function handleFetch(msg) {
    var url = msg.url || "";
    var requestId = msg.requestId;
    var bodyStr = msg.body;
    var body = null;
    try {
      if (bodyStr) body = JSON.parse(bodyStr);
    } catch (_e) {}

    diag("FETCH", url + " body=" + (bodyStr || "null").substring(0, 200));

    var result;
    try {
      result = routeFetch(url, body, msg);
    } catch (err) {
      diag(
        "FETCH-ERROR",
        url + " -> " + (err && err.message ? err.message : String(err))
      );
      setTimeout(function () {
        toReact({
          type: "fetch-response",
          requestId: requestId,
          responseType: "error",
          status: 432,
          error: err.message || "Internal error",
        });
      }, 0);
      return;
    }

    // Support both sync results and Promises
    function sendResult(r) {
      toReact({
        type: "fetch-response",
        requestId: requestId,
        responseType: "success",
        status: 200,
        headers: { "content-type": "application/json" },
        bodyJsonString: JSON.stringify(r),
      });
    }

    if (result && typeof result.then === "function") {
      result.then(function (r) {
        sendResult(r);
      }).catch(function (err) {
        diag("FETCH-ASYNC-ERROR", url + " -> " + String(err));
        toReact({
          type: "fetch-response",
          requestId: requestId,
          responseType: "error",
          status: 432,
          error: String(err),
        });
      });
    } else {
      setTimeout(function () { sendResult(result); }, 0);
    }
  }

  // --- Route fetch URL to handler (67 handlers) --------------------------------
  function routeFetch(url, body, msg) {
    if (url.indexOf("/wham/") !== -1) {
      diag("WHAM", url);
      if (url.indexOf("/wham/accounts/check") !== -1) {
        var isChatgpt = authCache.authMethod === "chatgpt" && authCache.isLoggedIn;
        var accountId =
          authCache.accountId || authCache.userId || authCache.email || "local";
        var accountList = isChatgpt
          ? [
              {
                id: accountId,
                email: authCache.email || null,
                plan_type: authCache.plan || "unknown",
              },
            ]
          : [];
        return {
          is_logged_in: isChatgpt,
          user: isChatgpt
            ? {
                id: accountId,
                email: authCache.email || null,
                plan_type: authCache.plan || "unknown",
              }
            : null,
          accounts: accountList,
          account_ordering: isChatgpt ? [accountId] : [],
        };
      }
      if (url.indexOf("/wham/tasks/list") !== -1) {
        return { tasks: [], total_count: 0 };
      }
      if (url.indexOf("/wham/tasks") !== -1) {
        return { tasks: [], total_count: 0 };
      }
      if (url.indexOf("/wham/environments") !== -1) {
        return [];
      }
      if (url.indexOf("/wham/usage") !== -1) {
        var logged = authCache.authMethod === "chatgpt" && authCache.isLoggedIn;
        var aid = authCache.accountId || authCache.userId || authCache.email || "local";
        var usageAccounts = logged
          ? [
              {
                id: aid,
                email: authCache.email || null,
                plan_type: authCache.plan || "unknown",
              },
            ]
          : [];
        return {
          accounts: usageAccounts,
          account_ordering: logged ? [aid] : [],
        };
      }
      return {};
    }

    var handler = url.replace("vscode://codex/", "");

    switch (handler) {
      case "get-global-state": {
        var gsKey = body && body.key;
        return {
          value: globalState[gsKey] !== undefined ? globalState[gsKey] : null,
        };
      }
      case "set-global-state": {
        if (body && body.key !== undefined) {
          globalState[body.key] = body.value;
          saveGlobalState();
        }
        return { success: true };
      }
      case "active-workspace-roots":
        return { roots: getActiveWorkspaceRoots() };

      case "workspace-root-options":
        return { roots: getAllWorkspaceRootOptions(), labels: loadWorkspaceLabels() };

      case "list-pinned-threads":
        return { threadIds: [] };

      case "extension-info":
        return {
          version: "1.0.5",
          buildNumber: "517",
          buildFlavor: "prod",
        };

      case "is-copilot-api-available":
        return { available: false };

      case "get-configuration": {
        var cfgKey = body && body.key;
        if (cfgKey && configStore[cfgKey] !== undefined)
          return { value: configStore[cfgKey] };
        if (cfgKey && configDefaults[cfgKey] !== undefined)
          return { value: configDefaults[cfgKey] };
        return { value: null };
      }
      case "set-configuration": {
        if (body && body.key !== undefined) {
          configStore[body.key] = body.value;
          saveConfig();
        }
        return { success: true };
      }
      case "set-vs-context":
        return { success: true };

      case "mcp-codex-config":
        return { config: null };

      case "account-info":
        return {
          accountId: authCache.accountId || null,
          userId: authCache.userId || null,
          plan: authCache.plan || null,
          email: authCache.email || null,
        };

      case "codex-home": {
        var home = HOMEDIR || "";
        var sep = home.indexOf("\\") !== -1 ? "\\" : "/";
        var codexHome = home ? home + sep + ".codex" : "";
        return {
          codexHome: codexHome,
          worktreesSegment: codexHome ? codexHome + sep + "worktrees" : "",
        };
      }
      case "os-info":
        return invoke("check_wsl").then(function(r) {
          return {
            platform: "win32",
            hasWsl: r && r.hasWsl ? true : false,
            isVsCodeRunningInsideWsl: false,
          };
        }).catch(function() {
          return {
            platform: "win32",
            hasWsl: false,
            isVsCodeRunningInsideWsl: false,
          };
        });

      case "locale-info":
        return {
          ideLocale: navigator.language || "en",
          systemLocale: navigator.language || "en",
        };

      case "inbox-items":
        return { items: [] };
      case "list-automations":
        return { items: [] };
      case "list-pending-automation-run-threads":
        return { threadIds: [] };
      case "pending-automation-runs":
        return { runs: [] };
      case "developer-instructions": {
        // Read developer instructions from AGENTS.md or codex config
        var diRoots = getActiveWorkspaceRoots();
        var diBase = (body && body.baseInstructions) || "";
        if (diRoots.length > 0) {
          var diPath = diRoots[0] + (diRoots[0].indexOf("\\") !== -1 ? "\\" : "/") + "AGENTS.md";
          return invoke("read_file_contents", { path: diPath }).then(function(r) {
            var content = r && r.contents ? r.contents : "";
            return { instructions: content || diBase };
          }).catch(function() {
            return { instructions: diBase };
          });
        }
        return { instructions: diBase };
      }
      case "openai-api-key":
        return { value: authCache.apiKey || null };
      case "has-custom-cli-executable":
        return { hasCustomCliExecutable: false };
      case "child-processes":
        return { processes: [] };
      case "third-party-notices":
        return { text: null };
      case "feedback-create-sentry-issue":
        return { reportId: "" };

      case "set-thread-pinned":
        return { success: false };
      case "set-pinned-threads-order":
        return { success: false };

      case "confirm-trace-recording-start":
        return { success: false };
      case "cancel-trace-recording-start":
        return { success: false };
      case "submit-trace-recording-details":
        return { success: false };

      case "hotkey-window-hotkey-state":
        return { enabled: false, hotkey: null };
      case "hotkey-window-set-hotkey":
        return { success: false, error: "Not supported." };
      case "hotkey-window-set-dev-hotkey-override":
        return { success: false };

      case "git-origins":
        return invoke("git_origins", { cwd: CWD || HOMEDIR }).catch(function() {
          return { origins: [], homeDir: HOMEDIR || "" };
        });
      case "git-merge-base": {
        var mbRef1 = (body && body.ref1) || "HEAD";
        var mbRef2 = (body && body.ref2) || "origin/main";
        return invoke("git_merge_base", { cwd: CWD || HOMEDIR, ref1: mbRef1, ref2: mbRef2 }).catch(function() {
          return { mergeBaseSha: null };
        });
      }
      case "read-git-file-binary": {
        var rgfRev = (body && body.rev) || "HEAD";
        var rgfPath = (body && body.path) || null;
        return invoke("read_git_file_binary", { cwd: CWD || HOMEDIR, rev: rgfRev, path: rgfPath }).catch(function() {
          return { contentsBase64: null };
        });
      }
      case "apply-patch": {
        var apPatch = (body && body.patch) || "";
        return invoke("git_apply_patch", { cwd: CWD || HOMEDIR, patch: apPatch }).catch(function() {
          return { patchApplied: false, error: "Failed to apply patch" };
        });
      }
      case "git-push": {
        var gpForce = (body && body.force) || false;
        return invoke("git_push", { cwd: CWD || HOMEDIR, force: gpForce }).catch(function() {
          return { success: false, error: "git push failed" };
        });
      }
      case "git-create-branch": {
        var gcbBranch = (body && body.branch) || "";
        return invoke("git_create_branch", { cwd: CWD || HOMEDIR, branch: gcbBranch }).catch(function() {
          return { success: false, error: "Failed to create branch" };
        });
      }
      case "git-checkout-branch": {
        var gcoBranch = (body && body.branch) || "";
        return invoke("git_checkout_branch", { cwd: CWD || HOMEDIR, branch: gcoBranch }).catch(function() {
          return { success: false, error: "Failed to checkout branch" };
        });
      }
      case "gh-cli-status":
        return invoke("gh_cli_status").catch(function() {
          return { isInstalled: false, isAuthenticated: false };
        });
      case "gh-pr-create": {
        var prTitle = (body && body.title) || null;
        var prBody = (body && body.body) || null;
        var prBase = (body && body.base) || null;
        return invoke("gh_pr_create", { cwd: CWD || HOMEDIR, title: prTitle, body: prBody, base: prBase }).catch(function() {
          return { success: false, error: "Failed to create PR" };
        });
      }
      case "gh-pr-status":
        return invoke("gh_pr_status", { cwd: CWD || HOMEDIR }).catch(function() {
          return { prs: [] };
        });

      case "find-files":
        var findPattern = (body && body.pattern) || null;
        return invoke("find_files", { cwd: CWD || HOMEDIR, pattern: findPattern }).catch(function() {
          return { files: [] };
        });
      case "paths-exist":
        var checkPaths = (body && body.paths) || [];
        return invoke("paths_exist", { paths: checkPaths }).catch(function() {
          return { existingPaths: [] };
        });
      case "read-file":
        var readPath = (body && body.path) || null;
        if (!readPath) return { contents: null };
        return invoke("read_file_contents", { path: readPath }).catch(function() {
          return { contents: null };
        });
      case "read-file-binary": {
        var rfbPath = (body && body.path) || null;
        if (!rfbPath) return { contentsBase64: null };
        return invoke("read_file_binary", { path: rfbPath }).catch(function() {
          return { contentsBase64: null };
        });
      }
      case "pick-file":
        return invoke("pick_file").then(function(path) {
          return { file: path || null };
        }).catch(function() {
          return { file: null };
        });
      case "pick-files":
        return invoke("pick_files").catch(function() {
          return { files: [] };
        });
      case "open-file": {
        var openTarget = (body && body.target) || "fileManager";
        var openPath = body && (body.path || body.cwd);
        if (!openPath) return { success: false };
        return invoke("open_in_target", { target: openTarget, path: openPath }).then(function() {
          return { success: true };
        }).catch(function(err) {
          console.warn("[bridge] open_in_target failed:", err);
          return { success: false, error: String(err) };
        });
      }

      case "recommended-skills": {
        // Read skills from the codex home skills directory
        var skillsDir = HOMEDIR + (HOMEDIR.indexOf("\\") !== -1 ? "\\" : "/") + ".codex" + (HOMEDIR.indexOf("\\") !== -1 ? "\\" : "/") + "skills";
        return invoke("find_files", { cwd: skillsDir, pattern: null }).then(function(result) {
          var files = (result && result.files) || [];
          var skills = files.filter(function(f) { return f.endsWith(".md") || f.endsWith(".json"); }).map(function(f) {
            var name = f.replace(/\.[^.]+$/, "").replace(/[-_]/g, " ");
            return { name: name, file: f, installed: true };
          });
          return {
            skills: skills,
            fetchedAt: Date.now(),
            source: "local",
            repoRoot: null,
            error: null,
          };
        }).catch(function() {
          return {
            skills: [],
            fetchedAt: Date.now(),
            source: "cached",
            repoRoot: null,
            error: null,
          };
        });
      }
      case "install-recommended-skill":
        return { success: true };
      case "remove-skill":
        return { success: true };

      case "codex-agents-md": {
        // Read AGENTS.md from active workspace root or codex home
        var agentsRoots = getActiveWorkspaceRoots();
        var agentsPath = agentsRoots.length > 0
          ? agentsRoots[0] + (agentsRoots[0].indexOf("\\") !== -1 ? "\\" : "/") + "AGENTS.md"
          : (HOMEDIR || ".") + (HOMEDIR && HOMEDIR.indexOf("\\") !== -1 ? "\\" : "/") + ".codex" + (HOMEDIR && HOMEDIR.indexOf("\\") !== -1 ? "\\" : "/") + "AGENTS.md";
        return invoke("read_file_contents", { path: agentsPath }).then(function(r) {
          return { contents: r && r.contents ? r.contents : null };
        }).catch(function() {
          return { contents: null };
        });
      }
      case "codex-agents-md-save":
        return { success: true };

      case "ide-context":
        return { ideContext: {} };
      case "ipc-request":
        return {};
      case "get-copilot-api-proxy-info":
        return null;

      case "prepare-worktree-snapshot":
        return { success: false };
      case "upload-worktree-snapshot":
        return { success: false };

      case "generate-thread-title":
        return { title: null };
      case "generate-pull-request-message":
        return { title: null, body: null };

      case "electron-clone-workspace-repo":
        return { success: false, canceled: false, error: "Not supported." };

      case "automation-create":
      case "automation-update":
      case "automation-delete":
      case "automation-run-now":
        throw new Error("Not available");

      case "automation-run-archive":
        return { success: false };
      case "automation-run-delete":
        return { success: false };

      case "add-workspace-root-option": {
        var newRoot = body && body.root;
        if (newRoot) {
          var wRoots = loadWorkspaceRoots();
          if (wRoots.indexOf(newRoot) === -1) {
            wRoots.push(newRoot);
            saveWorkspaceRoots(wRoots);
          }
          // If body.activate is true, also set as active
          if (body.activate) {
            saveActiveRoots([newRoot]);
            CWD = newRoot;
            setTimeout(function () { toReact({ type: "active-workspace-roots-updated" }); }, 0);
          }
          setTimeout(function () { toReact({ type: "workspace-root-options-updated" }); }, 0);
        }
        return { success: true };
      }
      case "open-in-targets":
        return invoke("detect_open_targets").catch(function() {
          return { preferredTarget: null, availableTargets: [], targets: [] };
        });
      case "set-preferred-app":
        return { success: true };
      case "local-environment":
        return { environment: null };
      case "local-environments":
        return { environments: [] };
      case "local-environment-config":
        return {
          configPath: (body && body.configPath) || null,
          exists: false,
          raw: null,
        };
      case "local-environment-config-save":
        return {
          configPath: (body && body.configPath) || null,
          success: true,
        };

      default:
        if (url.startsWith("vscode://codex/ipc-relay")) {
          if (body && body.request) {
            handleMcpRequest({
              type: "mcp-request",
              hostId: msg.hostId || HOST_ID,
              request: body.request,
            });
          }
          return { relayed: true };
        }
        throw new Error("Unsupported fetch route: " + url);
    }
  }

  // --- Handle type:"fetch-stream" (streaming requests) ------------------------
  function handleFetchStream(msg) {
    var requestId = msg.requestId;
    setTimeout(function () {
      toReact({
        type: "fetch-stream-complete",
        requestId: requestId,
      });
    }, 0);
  }

  // --- Handle type:"mcp-request" → translate to JSON-RPC -----------------------
  function handleMcpRequest(msg) {
    var request = msg.request || msg;
    if (!request || !request.method || request.id == null) return;

    diag("MCP-OUT", request.method + " id=" + request.id);
    mcpPending.set(request.id, msg.hostId || HOST_ID);
    trackMcpRequest(request, msg.hostId || HOST_ID);

    toBackend({
      jsonrpc: "2.0",
      id: request.id,
      method: request.method,
      params: request.params || {},
    });
  }

  // --- Handle internal messages locally ----------------------------------------
  function handleLocal(msg) {
    if (!msg || typeof msg !== "object" || !msg.type) return false;

    switch (msg.type) {
      case "persisted-atom-sync-request":
        // Ensure terminal panel starts visible when no terminal keys are set yet
        if (!persistedState["terminal-open-by-key"] || 
            (typeof persistedState["terminal-open-by-key"] === "object" && 
             Object.keys(persistedState["terminal-open-by-key"]).length === 0)) {
          // Set a default terminal key so the panel opens. The React code looks
          // for at least one truthy key in this object to decide if the terminal
          // panel should be expanded.
          persistedState["terminal-open-by-key"] = { "default": true };
          savePersisted();
        }
        setTimeout(function () {
          toReact({ type: "persisted-atom-sync", state: persistedState });
        }, 0);
        return true;

      case "persisted-atom-update":
        if (msg.deleted) {
          delete persistedState[msg.key];
        } else {
          persistedState[msg.key] = msg.value;
        }
        savePersisted();
        setTimeout(function () {
          toReact({
            type: "persisted-atom-updated",
            key: msg.key,
            value: msg.value,
            deleted: !!msg.deleted,
          });
        }, 0);
        return true;

      case "persisted-atom-reset":
        persistedState = {};
        savePersisted();
        setTimeout(function () {
          toReact({ type: "persisted-atom-sync", state: {} });
        }, 0);
        return true;

      case "log-message":
        var fn = console[msg.level] || console.log;
        fn.call(console, "[codex]", msg.message || msg.msg || "");
        return true;

      case "ready":
        console.log(
          "[bridge] React ready (connected=" +
            codexConnected +
            ", sent=" +
            !!window.__bridgeConnectedSent +
            ")"
        );
        if (!window.__bridgeConnectedSent && codexConnected) {
          window.__bridgeConnectedSent = true;
          toReact(makeConnectionMsg("connected"));
        }
        return true;

      case "view-focused":
        return true;

      case "electron-window-focus-request":
        window.focus();
        setTimeout(function () {
          toReact({ type: "electron-window-focus-changed", isFocused: true });
        }, 0);
        return true;

      case "desktop-notification-show":
        if (window.Notification && Notification.permission === "granted") {
          new Notification(msg.title || "Codex", { body: msg.body || "" });
        } else if (window.Notification && Notification.permission !== "denied") {
          Notification.requestPermission();
        }
        return true;

      case "desktop-notification-hide":
        return true;

      case "shared-object-set":
        sharedObjects[msg.key] = msg.value;
        var soSubs = sharedSubs[msg.key];
        if (soSubs) {
          setTimeout(function () {
            soSubs.forEach(function (cb) {
              try {
                cb(msg.value);
              } catch (_e) {}
            });
          }, 0);
        }
        return true;

      case "shared-object-subscribe":
        if (!sharedSubs[msg.key]) sharedSubs[msg.key] = new Set();
        if (msg._callback) sharedSubs[msg.key].add(msg._callback);
        return true;

      case "shared-object-unsubscribe":
        if (sharedSubs[msg.key] && msg._callback) {
          sharedSubs[msg.key].delete(msg._callback);
        }
        return true;

      case "codex-app-server-restart":
        console.log(
          "[bridge] Restart requested — ignoring (Tauri manages connection)"
        );
        return true;

      case "electron-onboarding-pick-workspace-or-create-default":
        console.log("[bridge] Onboarding: auto-creating default workspace");
        setTimeout(function () {
          toReact({
            type: "electron-onboarding-pick-workspace-or-create-default-result",
            success: true,
          });
        }, 100);
        return true;

      case "electron-onboarding-skip-workspace":
        console.log("[bridge] Onboarding: skip workspace");
        setTimeout(function () {
          toReact({
            type: "electron-onboarding-skip-workspace-result",
            success: true,
          });
        }, 50);
        return true;

      case "open-in-browser":
        if (msg.url) {
          invoke("open_external_url", { url: msg.url }).catch(function (e) {
            console.warn("[bridge] open_external_url failed:", e);
            try {
              window.open(msg.url, "_blank");
            } catch (_err) {}
          });
        }
        return true;

      case "open-external-url":
        if (msg.url) {
          invoke("open_external_url", { url: msg.url }).catch(function (e) {
            console.warn("[bridge] open_external_url failed:", e);
            try {
              window.open(msg.url, "_blank");
            } catch (_err) {}
          });
        }
        return true;

      case "open-config-toml":
      case "show-diff":
      case "show-plan-summary":
      case "show-settings":
        return true;

      // --- Terminal management (via Tauri invoke) ---
      case "terminal-create": {
        invoke("create_terminal", {
          sessionId: msg.sessionId || "term_" + Date.now(),
          cwd: msg.cwd || CWD || HOMEDIR || ".",
          shell: msg.shell || null,
        }).catch(function (e) {
          console.error("[bridge] create_terminal failed:", e);
        });
        return true;
      }
      case "terminal-attach":
        invoke("attach_terminal", {
          sessionId: msg.sessionId,
          cwd: msg.cwd || CWD || HOMEDIR || ".",
          shell: msg.shell || null,
        }).catch(function (e) {
          console.error("[bridge] attach_terminal failed:", e);
        });
        return true;

      case "terminal-write":
        invoke("write_terminal", {
          sessionId: msg.sessionId,
          data: msg.data || "",
        }).catch(function (e) {
          console.error("[bridge] write_terminal failed:", e);
        });
        return true;

      case "terminal-resize":
        invoke("resize_terminal", {
          sessionId: msg.sessionId,
          cols: msg.cols || 0,
          rows: msg.rows || 0,
        }).catch(function (e) {
          console.error("[bridge] resize_terminal failed:", e);
        });
        return true;

      case "terminal-detach":
        invoke("detach_terminal", {
          sessionId: msg.sessionId,
        }).catch(function (e) {
          console.error("[bridge] detach_terminal failed:", e);
        });
        return true;

      case "terminal-close":
        invoke("close_terminal", {
          sessionId: msg.sessionId,
        }).catch(function (e) {
          console.error("[bridge] close_terminal failed:", e);
        });
        return true;

      case "fetch-stream":
        handleFetchStream(msg);
        return true;

      case "cancel-fetch-stream":
      case "cancel-fetch":
        return true;

      case "worker-request":
        return true;
      case "worker-request-cancel":
        return true;

      case "thread-follower-join":
      case "thread-follower-leave":
      case "thread-follower-cursor":
      case "thread-follower-selection":
      case "thread-follower-presence":
      case "thread-follower-message":
      case "thread-follower-state":
      case "thread-follower-disconnect":
        return true;

      case "thread-archived":
      case "thread-unarchived":
      case "thread-stream-state-changed":
      case "thread-queued-followups-changed":
        return true;

      case "inbox-item-set-read-state":
      case "inbox-items-create":
        return true;

      case "export-logs":
      case "archive-thread":
        return true;

      case "hotkey-window-show":
      case "hotkey-window-hide":
      case "hotkey-window-toggle":
        return true;

      case "open-thread-overlay":
      case "thread-overlay-set-always-on-top":
        return true;

      case "electron-pick-workspace-root-option": {
        // Open folder picker via Tauri
        invoke("pick_folder", {}).then(function (folder) {
          if (folder) {
            // Add to roots and set as active
            var roots = loadWorkspaceRoots();
            if (roots.indexOf(folder) === -1) {
              roots.push(folder);
              saveWorkspaceRoots(roots);
            }
            saveActiveRoots([folder]);
            CWD = folder;
            toReact({ type: "active-workspace-roots-updated" });
            toReact({ type: "workspace-root-options-updated" });
          }
        }).catch(function (e) {
          console.warn("[bridge] pick_folder failed:", e);
        });
        return true;
      }
      case "electron-add-new-workspace-root-option": {
        invoke("pick_folder", {}).then(function (folder) {
          if (folder) {
            var roots = loadWorkspaceRoots();
            if (roots.indexOf(folder) === -1) {
              roots.push(folder);
              saveWorkspaceRoots(roots);
            }
            toReact({ type: "workspace-root-options-updated" });
          }
        }).catch(function (e) {
          console.warn("[bridge] pick_folder failed:", e);
        });
        return true;
      }
      case "electron-update-workspace-root-options":
        return true;
      case "electron-rename-workspace-root-option": {
        if (msg.root && msg.label !== undefined) {
          var labels = loadWorkspaceLabels();
          if (msg.label) {
            labels[msg.root] = msg.label;
          } else {
            delete labels[msg.root];
          }
          saveWorkspaceLabels(labels);
          toReact({ type: "workspace-root-options-updated" });
        }
        return true;
      }
      case "electron-set-active-workspace-root": {
        if (msg.root) {
          var roots = loadWorkspaceRoots();
          if (roots.indexOf(msg.root) === -1) {
            roots.push(msg.root);
            saveWorkspaceRoots(roots);
          }
          saveActiveRoots([msg.root]);
          CWD = msg.root;
          toReact({ type: "active-workspace-roots-updated" });
        }
        return true;
      }
      case "electron-clear-cache":
        return true;
      case "electron-get-app-info":
        setTimeout(function () {
          toReact({
            type: "electron-app-info",
            version: "1.0.5",
            buildNumber: "517",
            buildFlavor: "prod",
          });
        }, 0);
        return true;

      case "electron-set-badge-count":
      case "electron-set-window-mode":
      case "electron-request-microphone-permission":
      case "electron-add-ssh-host":
      case "electron-app-state-snapshot-trigger":
      case "electron-app-state-snapshot-response":
        return true;

      case "power-save-blocker-set":
      case "set-telemetry-user":
      case "toggle-trace-recording":
        return true;

      case "install-app-update":
      case "check-for-updates":
        return true;

      case "open-debug-window":
        return true;

      case "navigate-in-new-editor-tab":
      case "open-vscode-command":
      case "open-extension-settings":
      case "open-keyboard-shortcuts":
      case "update-diff-if-open":
      case "install-wsl":
        return true;

      default:
        return false;
    }
  }

  // --- Last known mouse position for context menu placement ---
  var lastMouseX = 0;
  var lastMouseY = 0;
  document.addEventListener("mousemove", function (e) {
    lastMouseX = e.clientX;
    lastMouseY = e.clientY;
  }, { passive: true });
  document.addEventListener("contextmenu", function (e) {
    lastMouseX = e.clientX;
    lastMouseY = e.clientY;
  }, { passive: true });

  // --- Context Menu (native OS menu via Tauri, with HTML fallback) ------------
  function showContextMenu(options) {
    if (!options || !options.items || !options.items.length) {
      return Promise.resolve(undefined);
    }

    // Try native OS context menu first via Rust invoke
    var nativeItems = options.items.map(function (item) {
      return {
        id: item.id || null,
        label: item.label || null,
        type: item.type || null,
      };
    });

    // Pass cursor coordinates so the menu appears at the right-click position
    return invoke("show_native_context_menu", {
      items: nativeItems,
      x: lastMouseX,
      y: lastMouseY,
    })
      .then(function (selectedId) {
        if (selectedId) {
          return { id: selectedId };
        }
        return undefined;
      })
      .catch(function (_err) {
        // Fallback to HTML context menu if native fails
        console.warn("[bridge] Native context menu failed, using HTML fallback:", _err);
        return showContextMenuHtml(options);
      });
  }

  // HTML fallback context menu (used if native menu fails)
  function showContextMenuHtml(options) {
    return new Promise(function (resolve) {
      if (!options || !options.items || !options.items.length) {
        resolve(undefined);
        return;
      }

      var old = document.getElementById("codex-ctx-menu");
      if (old) old.remove();

      var overlay = document.createElement("div");
      overlay.id = "codex-ctx-menu";
      overlay.style.cssText =
        "position:fixed;top:0;left:0;width:100%;height:100%;z-index:99999;background:transparent;";

      var menu = document.createElement("div");
      menu.style.cssText =
        "position:fixed;top:50%;left:50%;transform:translate(-50%,-50%);" +
        "background:#2d2d2d;border:1px solid #555;border-radius:8px;padding:4px 0;" +
        "min-width:200px;box-shadow:0 8px 32px rgba(0,0,0,.5);font-family:system-ui,sans-serif;font-size:13px;";

      options.items.forEach(function (item) {
        if (item.type === "separator") {
          var sep = document.createElement("div");
          sep.style.cssText = "height:1px;background:#555;margin:4px 8px;";
          menu.appendChild(sep);
          return;
        }
        var btn = document.createElement("div");
        btn.textContent = item.label || "";
        btn.style.cssText =
          "padding:8px 16px;color:#e0e0e0;cursor:pointer;white-space:nowrap;";
        btn.onmouseenter = function () {
          btn.style.background = "#0066cc";
        };
        btn.onmouseleave = function () {
          btn.style.background = "transparent";
        };
        btn.onclick = function () {
          overlay.remove();
          resolve({ id: item.id });
        };
        menu.appendChild(btn);
      });

      overlay.appendChild(menu);
      overlay.onclick = function (e) {
        if (e.target === overlay) {
          overlay.remove();
          resolve(undefined);
        }
      };
      document.body.appendChild(overlay);
    });
  }

  // --- Expose window.electronBridge -------------------------------------------
  // The compiled React bundle (index-BUvz-C55.js) specifically looks for
  // `window.electronBridge`. This is an interface contract with the bundle,
  // NOT an Electron dependency. Every function below is implemented with
  // Tauri v2 APIs (invoke, listen, emit). Zero Electron code.
  window.electronBridge = {
    // Required by React bundle — determines UI mode (toolbar, buttons, etc.)
    windowType: "electron",

    sendMessageFromView: function (msg) {
      log("OUT", msg);

      // 1. Fetch requests → handle locally
      if (msg && msg.type === "fetch") {
        handleFetch(msg);
        return Promise.resolve();
      }

      // 2. Fetch-stream → handle streaming
      if (msg && msg.type === "fetch-stream") {
        handleFetchStream(msg);
        return Promise.resolve();
      }

      // 3. MCP requests → translate to JSON-RPC via Rust
      if (msg && msg.type === "mcp-request") {
        handleMcpRequest(msg);
        return Promise.resolve();
      }

      // 4. MCP responses (approval decisions) → translate to JSON-RPC via Rust
      if (msg && msg.type === "mcp-response") {
        var response = msg.response || msg.message || msg;
        if (!response || response.id == null) return Promise.resolve();
        var responseJson = {
          jsonrpc: "2.0",
          id: response.id,
        };
        if (response.error !== undefined) {
          responseJson.error = response.error;
        } else {
          responseJson.result = response.result;
        }
        toBackend({
          jsonrpc: responseJson.jsonrpc,
          id: responseJson.id,
          result: responseJson.result,
          error: responseJson.error,
        });
        return Promise.resolve();
      }

      // 5. MCP notifications → translate to JSON-RPC notification via Rust
      if (msg && msg.type === "mcp-notification") {
        var notification =
          msg.notification ||
          (msg.method ? { method: msg.method, params: msg.params } : null);
        if (!notification || !notification.method) return Promise.resolve();
        toBackend({
          jsonrpc: "2.0",
          method: notification.method,
          params: notification.params || {},
        });
        return Promise.resolve();
      }

      // 6. Internal messages → handle locally
      if (handleLocal(msg)) return Promise.resolve();

      // 7. Unknown — log but do NOT send to backend
      console.log("[bridge] Unhandled message type:", msg && msg.type);
      return Promise.resolve();
    },

    getPathForFile: function (file) {
      return file && file.name ? file.name : null;
    },

    sendWorkerMessageFromView: function (workerName, msg) {
      return Promise.resolve();
    },

    subscribeToWorkerMessages: function (workerName, callback) {
      var subs = workerSubs.get(workerName);
      if (!subs) {
        subs = new Set();
        workerSubs.set(workerName, subs);
      }
      subs.add(callback);
      return function () {
        var s = workerSubs.get(workerName);
        if (!s) return;
        s.delete(callback);
        if (s.size === 0) workerSubs.delete(workerName);
      };
    },

    showContextMenu: showContextMenu,

    triggerSentryTestError: function () {
      return Promise.resolve();
    },

    getSentryInitOptions: function () {
      // Return a valid options object so the UI doesn't treat us as unsupported.
      // DSN is empty which disables actual reporting but satisfies the init check.
      return {
        codexAppSessionId: SESSION_ID || "tauri-" + Date.now(),
        buildFlavor: "prod",
        buildNumber: "517",
        appVersion: "1.0.5",
      };
    },

    getAppSessionId: function () {
      return SESSION_ID;
    },

    getBuildFlavor: function () {
      return "prod";
    },
  };

  // Required by React bundle for UI rendering mode detection
  window.codexWindowType = "electron";

  // --- Focus Tracking ----------------------------------------------------------
  window.addEventListener("focus", function () {
    toReact({ type: "electron-window-focus-changed", isFocused: true });
  });
  window.addEventListener("blur", function () {
    toReact({ type: "electron-window-focus-changed", isFocused: false });
  });

  // --- Prevent accidental page unload ------------------------------------------
  window.addEventListener("beforeunload", function (e) {
    if (codexConnected) {
      e.preventDefault();
      e.returnValue = "";
    }
  });

  // --- Intercept external navigation (replaces Electron's will-navigate) -------
  // Catch clicks on <a> tags pointing to external domains and open in browser.
  document.addEventListener("click", function (e) {
    var target = e.target;
    while (target && target.tagName !== "A") target = target.parentElement;
    if (!target || !target.href) return;
    try {
      var url = new URL(target.href, location.href);
      var host = url.hostname || "";
      if (
        host &&
        host !== "tauri.localhost" &&
        host !== "localhost" &&
        host !== "127.0.0.1" &&
        host !== "ipc.localhost" &&
        !url.protocol.match(/^(tauri|ipc|blob|data|about):/)
      ) {
        e.preventDefault();
        e.stopPropagation();
        diag("NAV-INTERCEPT", "External link -> " + url.href);
        invoke("open_external_url", { url: url.href }).catch(function (err) {
          console.warn("[bridge] open_external_url failed:", err);
        });
      }
    } catch (_) {}
  }, true);

  // --- Tauri Event Listeners ---------------------------------------------------

  // Messages from codex.exe (routed by Rust backend)
  listen("codex-message", function (event) {
    var data = event.payload;
    if (!data || typeof data !== "object") return;
    processCodexMessage(data);
    toReact(data);
  });

  // Connection status changes
  listen("codex-status-changed", function (event) {
    var status = event.payload;
    console.log("[bridge] codex status changed:", status);

    if (status === "connected") {
      codexConnected = true;
      // Flush any messages that were queued while disconnected
      flushPendingQueue();
      if (!window.__bridgeConnectedSent) {
        window.__bridgeConnectedSent = true;
        toReact(makeConnectionMsg("connected"));
        console.log("[bridge] Sent connected (from status event)");
      }
    } else if (status === "disconnected") {
      codexConnected = false;
      window.__bridgeConnectedSent = false; // MUST reset for reconnect
      toReact(makeConnectionMsg("disconnected"));
    } else if (status === "connecting") {
      toReact(makeConnectionMsg("connecting"));
    }
  });

  // Terminal events from Rust
  listen("terminal-data", function (event) {
    toReact(event.payload);
  });
  listen("terminal-init-log", function (event) {
    toReact(event.payload);
  });
  listen("terminal-attached", function (event) {
    toReact(event.payload);
  });
  listen("terminal-error", function (event) {
    toReact(event.payload);
  });
  listen("terminal-exit", function (event) {
    toReact(event.payload);
  });

  // --- Boot: Get context + initial status from Rust (no race condition) --------
  async function boot() {
    try {
      // 1. Get app context (cwd, home, hostId, sessionId)
      var ctx = await invoke("get_app_context");
      CWD = ctx.cwd || "";
      HOMEDIR = ctx.home || "";
      HOST_ID = ctx.hostId || "local";
      SESSION_ID = ctx.sessionId || "";
      console.log(
        "[bridge] Context: cwd=" + CWD + " home=" + HOMEDIR + " hostId=" + HOST_ID
      );

      // 2. Get current codex status (invoke = request-response, no race)
      var status = await invoke("get_codex_status");
      console.log("[bridge] Initial codex status:", status);

      if (status === "connected") {
        codexConnected = true;
        // Flush any messages that were queued while disconnected
        flushPendingQueue();
        if (!window.__bridgeConnectedSent) {
          window.__bridgeConnectedSent = true;
          toReact(makeConnectionMsg("connected"));
          toReact({ type: "codex-app-server-initialized" });
          console.log("[bridge] Sent connected (from boot)");
        }
      } else {
        toReact(makeConnectionMsg(status));
      }
    } catch (err) {
      console.error("[bridge] Boot failed:", err);
      diag("BOOT-ERROR", String(err));
    }
  }

  // --- Start -------------------------------------------------------------------
  console.log("[bridge] Codex Windows Tauri Bridge — hostId " + HOST_ID);
  boot();
})();
