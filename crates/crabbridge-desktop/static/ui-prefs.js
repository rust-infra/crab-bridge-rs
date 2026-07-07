(function () {
  const LANG_KEY = "crabbridge_lang";
  const THEME_KEY = "crabbridge_theme";

  const dict = {
    en: {
      "settings.title": "Settings",
      "settings.lead": "App preferences only. Provider and bridge setup are in Setup Wizard.",
      "settings.back_home": "Back to Home",
      "settings.language": "Language",
      "settings.theme": "Theme",
      "settings.theme_note": "Theme and language apply instantly across open windows.",
      "settings.startup": "Startup",
      "settings.launch_login": "Launch CrabBridge at login",
      "settings.logs": "Logs",
      "settings.logs_refresh": "Refresh",
      "settings.logs_reveal": "Reveal Directory",
      "settings.logs_empty": "(no log lines yet)",
      "settings.theme.system": "Follow system",
      "settings.theme.light": "Light",
      "settings.theme.dark": "Dark",
      "settings.lang.en": "English",
      "settings.lang.zh": "简体中文",
      "settings.msg.theme_updated": "Appearance updated",
      "settings.msg.autostart_on": "Launch at login enabled",
      "settings.msg.autostart_off": "Launch at login disabled",
      "welcome.title.setup": "What should we connect?",
      "welcome.lead.setup": "Set up CrabBridge to route Codex through DeepSeek or Kimi.",
      "welcome.title.home": "CrabBridge",
      "welcome.lead.home": "Your local bridge for Codex. Start Codex in your usual terminal.",
      "welcome.bridge.start": "Start Bridge",
      "welcome.bridge.stop": "Stop",
      "welcome.bridge.admin": "Admin",
      "welcome.setup.current": "Current setup",
      "welcome.setup.provider": "Current provider",
      "welcome.setup.bridge": "Bridge",
      "welcome.setup.wizard": "Setup Wizard",
      "welcome.quick.title": "Quick Setup",
      "welcome.quick.desc": "Set Base URL and API key, then CrabBridge configures Codex and starts the bridge automatically.",
      "welcome.quick.base_url": "Base URL",
      "welcome.quick.api_key": "API Key",
      "welcome.quick.save_start": "Save & Start",
      "welcome.msg.bridge_running": "Bridge is running",
      "welcome.msg.bridge_stopped": "Bridge is stopped",
      "welcome.msg.bridge_error": "Bridge error",
      "welcome.msg.bridge_started": "Bridge started",
      "welcome.msg.bridge_stopped_toast": "Bridge stopped",
      "welcome.msg.start_help": "Start the bridge, then open Codex in your terminal.",
      "welcome.msg.applied": "All set — open Codex in your terminal.",
      "welcome.msg.applying": "Applying provider and starting bridge…",
      "welcome.msg.save_provider_first": "Save a provider before running setup.",
      "welcome.msg.bridge_config_ready": "Bridge config ready",
      "welcome.msg.bridge_config_not_ready": "Setup not run yet",
      "provider.default_url": "Default: {value}",
      "provider.key.from_env": "Loaded from shell environment — Codex uses the same variable.",
      "provider.key.from_keychain": "Loaded from keychain fallback.",
      "provider.key.empty_hint": "Leave empty to keep using shell env only.",
      "provider.key.placeholder": "Export {env_key} in shell, or paste here to store in keychain",
      "provider.hint.current": "{label} is the current provider for Codex.",
      "provider.hint.switch": "Save to set {label} as the current provider.",
      "provider.chip.current": "{label} · Current",
      "provider.msg.saved_current": "Saved {label} as current provider",
      "provider.msg.saved_started": "Saved {label} and started bridge — see Home for status",
      "provider.msg.saved": "Provider saved",
      "welcome.meta.not_configured": "Not configured",
      "welcome.setup.wizard_only": "Setup Wizard",
      "welcome.home.connect_at": "Codex can connect at {url}"
    },
    zh: {
      "settings.title": "设置",
      "settings.lead": "这里只放应用偏好；Provider 和 Bridge 配置在 Setup Wizard 中。",
      "settings.back_home": "返回首页",
      "settings.language": "语言",
      "settings.theme": "外观",
      "settings.theme_note": "主题和语言会立即应用到所有已打开的窗口。",
      "settings.startup": "启动",
      "settings.launch_login": "登录时自动启动 CrabBridge",
      "settings.logs": "日志",
      "settings.logs_refresh": "刷新",
      "settings.logs_reveal": "打开目录",
      "settings.logs_empty": "（暂无日志）",
      "settings.theme.system": "跟随系统",
      "settings.theme.light": "浅色",
      "settings.theme.dark": "深色",
      "settings.lang.en": "English",
      "settings.lang.zh": "简体中文",
      "settings.msg.theme_updated": "外观已更新",
      "settings.msg.autostart_on": "已开启登录自动启动",
      "settings.msg.autostart_off": "已关闭登录自动启动",
      "welcome.title.setup": "需要连接什么？",
      "welcome.lead.setup": "将 Codex 通过 CrabBridge 路由到 DeepSeek 或 Kimi。",
      "welcome.title.home": "CrabBridge",
      "welcome.lead.home": "本地 Codex 网桥。请在常用终端中启动 Codex。",
      "welcome.bridge.start": "启动 Bridge",
      "welcome.bridge.stop": "停止",
      "welcome.bridge.admin": "管理页",
      "welcome.setup.current": "当前配置",
      "welcome.setup.provider": "当前 Provider",
      "welcome.setup.bridge": "Bridge",
      "welcome.setup.wizard": "设置向导",
      "welcome.quick.title": "快速设置",
      "welcome.quick.desc": "设置 Base URL 和 API Key 后，CrabBridge 会自动配置 Codex 并启动 Bridge。",
      "welcome.quick.base_url": "Base URL",
      "welcome.quick.api_key": "API Key",
      "welcome.quick.save_start": "保存并启动",
      "welcome.msg.bridge_running": "Bridge 运行中",
      "welcome.msg.bridge_stopped": "Bridge 已停止",
      "welcome.msg.bridge_error": "Bridge 异常",
      "welcome.msg.bridge_started": "Bridge 已启动",
      "welcome.msg.bridge_stopped_toast": "Bridge 已停止",
      "welcome.msg.start_help": "请先启动 Bridge，再在终端里使用 Codex。",
      "welcome.msg.applied": "设置完成，可在终端中使用 Codex。",
      "welcome.msg.applying": "正在应用配置并启动 Bridge…",
      "welcome.msg.save_provider_first": "请先保存 Provider 再执行配置。",
      "welcome.msg.bridge_config_ready": "Bridge 配置已就绪",
      "welcome.msg.bridge_config_not_ready": "尚未执行配置",
      "provider.default_url": "默认值：{value}",
      "provider.key.from_env": "已从 shell 环境变量读取，Codex 会使用同一变量。",
      "provider.key.from_keychain": "已从钥匙串读取（回退）。",
      "provider.key.empty_hint": "留空则继续使用 shell 环境变量。",
      "provider.key.placeholder": "在 shell 中导出 {env_key}，或粘贴到这里存入钥匙串",
      "provider.hint.current": "{label} 是 Codex 当前 Provider。",
      "provider.hint.switch": "保存后将 {label} 设为当前 Provider。",
      "provider.chip.current": "{label} · 当前",
      "provider.msg.saved_current": "已将 {label} 设为当前 Provider",
      "provider.msg.saved_started": "已保存 {label} 并启动 Bridge（状态见首页）",
      "provider.msg.saved": "Provider 已保存",
      "welcome.meta.not_configured": "未配置",
      "welcome.setup.wizard_only": "设置向导",
      "welcome.home.connect_at": "Codex 可连接到 {url}"
    }
  };

  function getLang() {
    return localStorage.getItem(LANG_KEY) || "en";
  }

  function getTheme() {
    return localStorage.getItem(THEME_KEY) || "system";
  }

  function interpolate(template, vars) {
    if (!vars) return template;
    return template.replace(/\{(\w+)\}/g, (_, key) => String(vars[key] ?? ""));
  }

  function t(key, vars) {
    const lang = getLang();
    const locale = dict[lang] || dict.en;
    const fallback = dict.en[key] || key;
    return interpolate(locale[key] || fallback, vars);
  }

  function resolveTheme(theme) {
    if (theme === "system") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    return theme;
  }

  function applyTheme(theme) {
    const resolved = resolveTheme(theme);
    document.documentElement.setAttribute("data-theme", resolved);
  }

  function applyI18n(root = document) {
    root.querySelectorAll("[data-i18n]").forEach((el) => {
      el.textContent = t(el.dataset.i18n);
    });
    root.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
      el.setAttribute("placeholder", t(el.dataset.i18nPlaceholder));
    });
  }

  function refresh() {
    applyTheme(getTheme());
    applyI18n();
  }

  function emitChange() {
    window.dispatchEvent(
      new CustomEvent("crabbridge:prefs-changed", {
        detail: { lang: getLang(), theme: getTheme() }
      })
    );
    const emit = window.__TAURI__?.event?.emit;
    if (emit) {
      emit("appearance-changed", { lang: getLang(), theme: getTheme() }).catch(() => {});
    }
  }

  function watchSystemTheme() {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => {
      if (getTheme() === "system") {
        applyTheme("system");
      }
    };
    if (typeof media.addEventListener === "function") {
      media.addEventListener("change", onChange);
    } else if (typeof media.addListener === "function") {
      media.addListener(onChange);
    }
  }

  function bindSyncListeners() {
    window.addEventListener("storage", (event) => {
      if (event.key === LANG_KEY || event.key === THEME_KEY) {
        refresh();
      }
    });
    window.addEventListener("focus", refresh);
    const listen = window.__TAURI__?.event?.listen;
    if (listen) {
      listen("appearance-changed", refresh).catch(() => {});
    }
  }

  function setLang(lang) {
    localStorage.setItem(LANG_KEY, lang);
    applyI18n();
    emitChange();
  }

  function setTheme(theme) {
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme);
    emitChange();
  }

  function init() {
    refresh();
    watchSystemTheme();
    bindSyncListeners();
  }

  window.CrabUi = {
    t,
    init,
    refresh,
    applyI18n,
    getLang,
    getTheme,
    setLang,
    setTheme
  };
})();
