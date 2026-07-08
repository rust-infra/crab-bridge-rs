(function () {
  const LANG_KEY = "crabbridge_lang";
  const THEME_KEY = "crabbridge_theme";

  const dict = {
    en: {
      "settings.title": "Settings",
      "settings.lead": "App preferences only. Provider and bridge setup are in Setup Wizard.",
      "settings.back_home": "Back to Home",
      "settings.language": "Language",
      "settings.appearance": "Appearance",
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
      "settings.msg.lang_updated": "Language updated",
      "settings.msg.autostart_on": "Launch at login enabled",
      "settings.msg.autostart_off": "Launch at login disabled",
      "welcome.title.setup": "What should we connect?",
      "welcome.lead.setup": "Optional: choose a Codex provider, custom base URL, or store API keys in keychain.",
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
      "welcome.quick.desc": "DeepSeek and Kimi work out of the box. Customize base URL or store an API key here if you want.",
      "welcome.quick.base_url": "Base URL",
      "welcome.quick.base_url_placeholder": "https://api.deepseek.com/v1",
      "welcome.quick.api_key": "API Key",
      "welcome.quick.set_codex": "Set as Codex Provider",
      "welcome.msg.bridge_running": "Bridge is running",
      "welcome.msg.bridge_stopped": "Bridge is stopped",
      "welcome.msg.bridge_error": "Bridge error",
      "welcome.msg.bridge_started": "Bridge started",
      "welcome.msg.bridge_stopped_toast": "Bridge stopped",
      "welcome.msg.start_help": "Start the bridge, then open Codex in your terminal.",
      "welcome.msg.bridge_config_ready": "Bridge config ready",
      "welcome.msg.bridge_config_not_ready": "Bridge starting…",
      "welcome.msg.bridge_builtin_ready": "Bridge running with built-in DeepSeek and Kimi",
      "provider.default_url": "Default: {value}",
      "provider.key.from_env": "Loaded from shell environment — Codex uses the same variable.",
      "provider.key.from_keychain": "Loaded from keychain fallback.",
      "provider.key.empty_hint": "Leave empty to keep using shell env only.",
      "provider.key.placeholder": "Export {env_key} in shell, or paste here to store in keychain",
      "provider.key.placeholder_generic": "From shell env, or paste here",
      "provider.hint.current": "{label} is the current provider for Codex.",
      "provider.hint.switch": "Set as Codex Provider to switch to {label}.",
      "provider.chip.current": "{label} · Current",
      "welcome.meta.not_configured": "Not configured",
      "welcome.setup.wizard_only": "Setup Wizard",
      "welcome.setup.back_home": "Back Home",
      "welcome.home.connect_at": "Codex can connect at {url}"
    },
    zh: {
      "settings.title": "设置",
      "settings.lead": "这里只放应用偏好；Provider 和 Bridge 配置在 Setup Wizard 中。",
      "settings.back_home": "返回首页",
      "settings.language": "语言",
      "settings.appearance": "外观",
      "settings.theme": "主题",
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
      "settings.msg.lang_updated": "语言已更新",
      "settings.msg.autostart_on": "已开启登录自动启动",
      "settings.msg.autostart_off": "已关闭登录自动启动",
      "welcome.title.setup": "需要连接什么？",
      "welcome.lead.setup": "可选：选择 Codex Provider、自定义 Base URL，或将 API Key 存入钥匙串。",
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
      "welcome.quick.desc": "DeepSeek 和 Kimi 开箱即用。如需自定义 Base URL 或保存 API Key，可在此配置。",
      "welcome.quick.base_url": "Base URL",
      "welcome.quick.base_url_placeholder": "https://api.deepseek.com/v1",
      "welcome.quick.api_key": "API Key",
      "welcome.quick.set_codex": "设为 Codex Provider",
      "welcome.msg.bridge_running": "Bridge 运行中",
      "welcome.msg.bridge_stopped": "Bridge 已停止",
      "welcome.msg.bridge_error": "Bridge 异常",
      "welcome.msg.bridge_started": "Bridge 已启动",
      "welcome.msg.bridge_stopped_toast": "Bridge 已停止",
      "welcome.msg.start_help": "请先启动 Bridge，再在终端里使用 Codex。",
      "welcome.msg.bridge_config_ready": "Bridge 配置已就绪",
      "welcome.msg.bridge_config_not_ready": "Bridge 启动中…",
      "welcome.msg.bridge_builtin_ready": "Bridge 已运行（内置 DeepSeek 与 Kimi）",
      "provider.default_url": "默认值：{value}",
      "provider.key.from_env": "已从 shell 环境变量读取，Codex 会使用同一变量。",
      "provider.key.from_keychain": "已从钥匙串读取（回退）。",
      "provider.key.empty_hint": "留空则继续使用 shell 环境变量。",
      "provider.key.placeholder": "在 shell 中导出 {env_key}，或粘贴到这里存入钥匙串",
      "provider.key.placeholder_generic": "从 shell 环境读取，或在此粘贴",
      "provider.hint.current": "{label} 是 Codex 当前 Provider。",
      "provider.hint.switch": "点击「设为 Codex Provider」切换到 {label}。",
      "provider.chip.current": "{label} · 当前",
      "welcome.meta.not_configured": "未配置",
      "welcome.setup.wizard_only": "设置向导",
      "welcome.setup.back_home": "返回首页",
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
    document.documentElement.lang = getLang() === "zh" ? "zh-CN" : "en";
    root.querySelectorAll("[data-i18n]:not([data-i18n-skip])").forEach((el) => {
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
