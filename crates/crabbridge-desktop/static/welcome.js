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

  function shouldShowHome(status) {
    return status.onboarding_complete && status.bridge_config_exists;
  }

  function setView(mode) {
    const home = mode === "home";
    byId("view-home").hidden = !home;
    byId("view-setup").hidden = home;
    byId("welcome-header").classList.toggle("is-home", home);

    if (home) {
      byId("welcome-title").textContent = tr("welcome.title.home");
      byId("welcome-lead").textContent = tr("welcome.lead.home");
    } else {
      byId("welcome-title").textContent = tr("welcome.title.setup");
      byId("welcome-lead").textContent = tr("welcome.lead.setup");
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

    byId("home-headline").textContent = running
      ? tr("welcome.msg.bridge_running")
      : bridge.status === "error"
        ? tr("welcome.msg.bridge_error")
        : tr("welcome.msg.bridge_stopped");
    byId("home-subtitle").textContent = running
      ? tr("welcome.home.connect_at", { url: formatListenUrl(bridge) })
      : bridge.last_error
        ? bridge.last_error
        : tr("welcome.msg.start_help");

    byId("home-bridge-start").disabled = running;
    byId("home-bridge-stop").disabled = !running;
    byId("home-bridge-admin").disabled = !running;
    byId("home-bind").textContent = formatListenUrl(bridge);
  }

  async function refreshHome(invoke) {
    const bridge = await invoke("bridge_status");
    applyBridgeUi(bridge);

    const provider = await invoke("provider_config_get", { slug: null });
    const active = provider.providers.find((p) => p.is_active);
    byId("home-provider").textContent = active
      ? `${active.label} (${active.slug})`
      : tr("welcome.meta.not_configured");
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
      byId("setup-status").textContent = status.bridge_config_exists
        ? tr("welcome.msg.bridge_config_ready")
        : tr("welcome.msg.bridge_config_not_ready");
      byId("setup-status").className =
        "status-line" + (status.bridge_config_exists ? " ok" : "");

      const active = (await invoke("provider_config_get", { slug: null })).providers.find(
        (p) => p.is_active
      );
      if (!active?.configured && !status.bridge_config_exists) {
        setMessage(tr("welcome.msg.save_provider_first"));
      } else {
        setMessage("");
      }

      return status;
    }

    async function applyViewFromStatus() {
      const status = await invoke("onboarding_status");
      if (shouldShowHome(status)) {
        setView("home");
        await refreshHome(invoke);
      } else {
        setView("setup");
        await refreshSetup();
      }
    }

    byId("home-bridge-start").addEventListener("click", async () => {
      try {
        const bridge = await invoke("bridge_start");
        applyBridgeUi(bridge);
        setMessage(tr("welcome.msg.bridge_started"));
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

    byId("complete-setup").addEventListener("click", async () => {
      setMessage(tr("welcome.msg.applying"));
      try {
        await initProviderConfig().save();
        await invoke("onboarding_run_setup");
        await invoke("onboarding_finish");
        setMessage(tr("welcome.msg.applied"));
        await applyViewFromStatus();
      } catch (err) {
        setMessage(String(err));
      }
    });

    window.addEventListener("focus", () => {
      if (!byId("view-home").hidden) {
        refreshHome(invoke).catch((err) => setMessage(String(err)));
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
