(function () {
  function byId(id) {
    return document.getElementById(id);
  }

  function setMessage(text) {
    byId("message").textContent = text || "";
  }

  function tr(key, vars) {
    return window.CrabUi?.t ? window.CrabUi.t(key, vars) : key;
  }

  function getInvoke() {
    const tauri = window.__TAURI__;
    if (!tauri?.core?.invoke) {
      throw new Error("Tauri API not available — restart the desktop app.");
    }
    return tauri.core.invoke;
  }

  function shouldShowHome(_status) {
    return true;
  }

  function setView(mode, options = {}) {
    const home = mode === "home";
    byId("view-home").hidden = !home;
    byId("view-setup").hidden = home;
    byId("welcome-header").classList.toggle("is-home", home);

    const backHome = byId("setup-back-home");
    if (backHome) {
      backHome.hidden = options.hideBackHome ?? false;
    }

    if (home) {
      byId("welcome-title").textContent = tr("welcome.title.home");
      byId("welcome-lead").hidden = true;
    } else {
      byId("welcome-title").textContent = tr("welcome.title.setup");
      const lead = byId("welcome-lead");
      lead.hidden = false;
      lead.textContent = tr("welcome.lead.setup");
    }
  }

  function formatListenUrl(bridge) {
    if (bridge.admin_url) {
      return bridge.admin_url.replace(/\/admin\/?$/, "");
    }
    return `http://${bridge.bind_addr}`;
  }

  function applyBridgeUi(bridge) {
    const running = bridge.status === "running";
    const pill = byId("home-bridge-status");
    pill.textContent = bridge.status;
    pill.className = "status-pill " + (running ? "ok" : bridge.status === "error" ? "fail" : "");

    const headline = byId("home-headline");
    if (running) {
      headline.hidden = true;
    } else {
      headline.hidden = false;
      headline.textContent =
        bridge.status === "error"
          ? tr("welcome.msg.bridge_error")
          : tr("welcome.msg.bridge_stopped");
    }
    const subtitle = byId("home-subtitle");
    if (running) {
      subtitle.hidden = true;
    } else {
      subtitle.hidden = false;
      subtitle.textContent = bridge.last_error
        ? bridge.last_error
        : tr("welcome.msg.start_help");
    }

    byId("home-bridge-start").disabled = running;
    byId("home-bridge-stop").disabled = !running;
    byId("home-bridge-admin").disabled = !running;
    byId("home-bind").textContent = formatListenUrl(bridge);
  }

  async function refreshHome(invoke) {
    setMessage("");
    const bridge = await invoke("bridge_status");
    applyBridgeUi(bridge);

    const provider = await invoke("provider_config_get", { slug: null });
    const active = provider.providers.find((p) => p.is_active);
    byId("home-provider").textContent = active
      ? `${active.label} (${active.slug})`
      : tr("welcome.meta.not_configured");
    const version = await invoke("app_version");
    byId("home-version").textContent = version ? `v${version}` : "—";
    byId("home-hint").textContent = await invoke("codex_usage_hint");

    return bridge;
  }

  async function initWelcome() {
    const invoke = getInvoke();
    let providerConfig = null;

    function initProviderConfig() {
      if (!providerConfig) {
        providerConfig = window.CrabProviderConfig.create(invoke, { setMessage });
      }
      return providerConfig;
    }

    async function refreshSetup() {
      const cfg = initProviderConfig();
      await cfg.refresh();

      const status = await invoke("onboarding_status");
      const bridge = await invoke("bridge_status");
      const bridgeReady = bridge.status === "running";
      byId("setup-status").textContent = status.bridge_config_exists
        ? tr("welcome.msg.bridge_config_ready")
        : bridgeReady
          ? tr("welcome.msg.bridge_builtin_ready")
          : tr("welcome.msg.bridge_config_not_ready");
      byId("setup-status").className =
        "status-line" + (status.bridge_config_exists || bridgeReady ? " ok" : "");

      setMessage("");

      return status;
    }

    async function applyViewFromStatus() {
      const status = await invoke("onboarding_status");
      if (shouldShowHome(status)) {
        setView("home");
        await refreshHome(invoke);
      } else {
        setView("setup", { hideBackHome: true });
        await refreshSetup();
      }
    }

    byId("home-bridge-start").addEventListener("click", async () => {
      try {
        const bridge = await invoke("bridge_start");
        applyBridgeUi(bridge);
        setMessage("");
      } catch (err) {
        setMessage(String(err));
        await refreshHome(invoke);
      }
    });

    byId("home-bridge-stop").addEventListener("click", async () => {
      try {
        const bridge = await invoke("bridge_stop");
        applyBridgeUi(bridge);
        setMessage(tr("welcome.msg.bridge_stopped_toast"));
      } catch (err) {
        setMessage(String(err));
        await refreshHome(invoke);
      }
    });

    byId("home-bridge-admin").addEventListener("click", async () => {
      try {
        await invoke("bridge_open_admin");
      } catch (err) {
        setMessage(String(err));
      }
    });

    byId("home-show-wizard").addEventListener("click", async () => {
      setView("setup");
      await refreshSetup();
    });

    byId("setup-back-home").addEventListener("click", async () => {
      setView("home");
      await refreshHome(invoke);
    });

    byId("set-codex-provider").addEventListener("click", async () => {
      try {
        await initProviderConfig().applyProvider();
      } catch (err) {
        setMessage(String(err));
      }
    });

    window.addEventListener("focus", () => {
      if (!byId("view-home").hidden) {
        refreshHome(invoke).catch((err) => setMessage(String(err)));
      }
    });

    window.addEventListener("crabbridge:prefs-changed", async () => {
      window.CrabUi?.applyI18n?.();
      const status = await invoke("onboarding_status");
      if (shouldShowHome(status)) {
        setView("home");
        await refreshHome(invoke);
      } else {
        setView("setup", { hideBackHome: !status.onboarding_complete });
        await refreshSetup();
      }
    });

    window.CrabUi?.init?.();
    await applyViewFromStatus();

    const { listen } = window.__TAURI__.event;
    await listen("bridge-status-changed", (event) => {
      if (!byId("view-home").hidden) {
        applyBridgeUi(event.payload);
      }
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => {
      initWelcome().catch((err) => setMessage(String(err)));
    });
  } else {
    initWelcome().catch((err) => setMessage(String(err)));
  }
})();
