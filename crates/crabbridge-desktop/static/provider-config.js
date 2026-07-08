(function () {
  function byId(id) {
    return document.getElementById(id);
  }

  function tr(key, vars) {
    return window.CrabUi?.t ? window.CrabUi.t(key, vars) : key;
  }

  function renderProviderForm(selected) {
    byId("provider-base-url").value = selected.base_url || "";
    byId("provider-default-url").textContent = tr("provider.default_url", {
      value: selected.default_base_url
    });
    byId("provider-env-key").textContent = selected.env_key;

    const keyInput = byId("provider-api-key");
    const keyHint = byId("provider-key-hint");
    if (selected.api_key_available && selected.api_key_masked) {
      keyInput.value = selected.api_key_masked;
      keyInput.readOnly = true;
      keyInput.classList.add("masked");
      keyInput.type = "text";
      keyHint.textContent =
        selected.api_key_source === "env"
          ? tr("provider.key.from_env")
          : tr("provider.key.from_keychain");
    } else {
      keyInput.value = "";
      keyInput.readOnly = false;
      keyInput.classList.remove("masked");
      keyInput.type = "password";
      keyInput.placeholder = tr("provider.key.placeholder", { env_key: selected.env_key });
      keyHint.textContent = tr("provider.key.empty_hint");
    }

    const defaultHint = byId("provider-default-hint");
    if (defaultHint) {
      defaultHint.textContent = selected.is_active
        ? tr("provider.hint.current", { label: selected.label })
        : tr("provider.hint.switch", { label: selected.label });
    }
  }

  function createProviderConfig(invoke, options) {
    const setMessage = options?.setMessage || (() => {});
    let providerSnapshot = null;
    let selectedSlug = null;

    function renderProviderList(snapshot) {
      providerSnapshot = snapshot;
      selectedSlug = snapshot.selected.slug;
      const container = byId("provider-list");
      container.innerHTML = "";
      for (const item of snapshot.providers) {
        const btn = document.createElement("button");
        btn.type = "button";
        const isSelected = item.slug === selectedSlug;
        btn.className =
          "provider-chip" +
          (isSelected ? " selected" : "") +
          (item.is_active ? " current" : "") +
          (item.configured ? " configured" : "");
        btn.setAttribute("aria-pressed", isSelected ? "true" : "false");
        btn.textContent = item.is_active
          ? tr("provider.chip.current", { label: item.label })
          : item.label;
        btn.addEventListener("click", async () => {
          const next = await invoke("provider_config_get", { slug: item.slug });
          renderProviderList(next);
        });
        container.appendChild(btn);
      }
      renderProviderForm(snapshot.selected);
    }

    async function refresh() {
      const snapshot = await invoke("provider_config_get", { slug: selectedSlug });
      renderProviderList(snapshot);
      return snapshot;
    }

    async function applyProvider() {
      const selected = providerSnapshot?.selected;
      if (!selected) return null;
      const apiKeyInput = byId("provider-api-key");
      const request = {
        slug: selected.slug,
        baseUrl: byId("provider-base-url").value.trim(),
        setActive: true,
        apiKey: apiKeyInput.readOnly ? null : apiKeyInput.value.trim() || null,
      };
      const snapshot = await invoke("provider_config_save", { request });
      renderProviderList(snapshot);
      setMessage("");
      return snapshot;
    }

    return {
      refresh,
      applyProvider,
      getSnapshot: () => providerSnapshot,
    };
  }

  window.CrabProviderConfig = { create: createProviderConfig };
})();
