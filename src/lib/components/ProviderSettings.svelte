<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Provider = {
    id: string;
    kind: string;
    name: string;
    enabled: boolean;
    model: string;
    baseUrl: string;
    hasApiKey: boolean;
    requiresApiKey: boolean;
    implemented: boolean;
    timeoutSeconds: number;
    retries: number;
    concurrency: number;
  };
  type Policy = { useDictionary:boolean; useTranslationMemory:boolean; useCache:boolean; saveTranslationMemory:boolean; cacheTtlSeconds:number };
  type AgentCliProfile = {
    id: string;
    name: string;
    executable: string;
    arguments: string;
    installed: boolean;
    description: string;
  };

  let { onclose }: { onclose: () => void } = $props();
  let providers = $state<Provider[]>([]);
  let savedProviderIds = $state<string[]>([]);
  let agentCliProfiles = $state<AgentCliProfile[]>([]);
  let fallbackIds = $state<string[]>([]);
  let policy = $state<Policy>({ useDictionary:true, useTranslationMemory:true, useCache:true, saveTranslationMemory:true, cacheTtlSeconds:900 });
  let selectedId = $state("google");
  let apiKey = $state("");
  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let loaded = $state(false);
  let agentProfileOverride = $state<string | null>(null);
  let selected = $derived(providers.find((provider) => provider.id === selectedId));
  let selectedIsAgentCli = $derived(selected?.kind === "agent_cli");
  let selectedAgentProfileId = $derived(
    agentProfileOverride ?? (selectedIsAgentCli
      ? agentCliProfiles.find((profile) => profile.arguments === selected?.baseUrl)?.id ?? "custom"
      : "custom"),
  );
  let selectedAgentProfile = $derived(agentCliProfiles.find((profile) => profile.id === selectedAgentProfileId));

  async function load() {
    try {
      [providers, fallbackIds, policy, agentCliProfiles] = await Promise.all([
        invoke<Provider[]>("get_translation_provider_settings"),
        invoke<string[]>("get_translation_fallback"),
        invoke<Policy>("get_translation_policy"),
        invoke<AgentCliProfile[]>("get_agent_cli_profiles"),
      ]);
      savedProviderIds = providers.map((provider) => provider.id);
      selectedId = providers.find((provider) => provider.enabled)?.id ?? providers[0]?.id ?? "google";
    } catch (reason) {
      error = `Unable to load provider settings: ${reason}`;
    } finally {
      loaded = true;
    }
  }

  function update(field: "name" | "enabled" | "model" | "baseUrl" | "timeoutSeconds" | "retries" | "concurrency", value: boolean | string | number) {
    if (!selected) return;
    if (selectedIsAgentCli && (field === "model" || field === "baseUrl")) {
      agentProfileOverride = "custom";
    }
    providers = providers.map((provider) =>
      provider.id === selected.id ? { ...provider, [field]: value } : provider,
    );
  }

  function uniqueAgentName(base: string, currentId = "") {
    const used = new Set(providers.filter((provider) => provider.id !== currentId).map((provider) => provider.name.toLowerCase()));
    if (!used.has(base.toLowerCase())) return base;
    let suffix = 2;
    while (used.has(`${base} ${suffix}`.toLowerCase())) suffix += 1;
    return `${base} ${suffix}`;
  }

  function addAgentCli() {
    const profile = agentCliProfiles.find((candidate) => candidate.installed) ?? agentCliProfiles[0];
    const id = nextDynamicId("agent_cli");
    const name = uniqueAgentName(profile?.name ?? "Custom Agent CLI");
    providers = [...providers, {
      id,
      kind: "agent_cli",
      name,
      enabled: true,
      model: profile?.executable ?? "",
      baseUrl: profile?.arguments ?? "",
      hasApiKey: false,
      requiresApiKey: false,
      implemented: true,
      timeoutSeconds: 60,
      retries: 2,
      concurrency: 4,
    }];
    selectedId = id;
    agentProfileOverride = profile?.id ?? "custom";
    apiKey = "";
    message = "New Agent CLI draft. Choose a profile or Custom, then save.";
    error = "";
  }

  function nextDynamicId(prefix: "agent_cli" | "custom") {
    const seed = Date.now();
    let suffix = 0;
    let id = `${prefix}:${seed}`;
    while (providers.some((provider) => provider.id === id)) {
      suffix += 1;
      id = `${prefix}:${seed}:${suffix}`;
    }
    return id;
  }

  function addCustomProvider() {
    const id = nextDynamicId("custom");
    providers = [...providers, {
      id,
      kind: "custom",
      name: uniqueAgentName("Custom Provider"),
      enabled: true,
      model: "",
      baseUrl: "",
      hasApiKey: false,
      requiresApiKey: false,
      implemented: true,
      timeoutSeconds: 60,
      retries: 2,
      concurrency: 4,
    }];
    selectedId = id;
    agentProfileOverride = null;
    apiKey = "";
    message = "New OpenAI-compatible provider draft. Configure it, then save.";
    error = "";
  }

  function applyAgentProfile(id: string) {
    if (!selected || !selectedIsAgentCli) return;
    agentProfileOverride = id;
    if (id === "custom") return;
    const profile = agentCliProfiles.find((candidate) => candidate.id === id);
    if (!profile) return;
    const knownProfileName = agentCliProfiles.some((candidate) => candidate.name === selected.name);
    providers = providers.map((provider) =>
      provider.id === selected.id
        ? {
            ...provider,
            name: knownProfileName ? uniqueAgentName(profile.name, provider.id) : provider.name,
            model: profile.executable,
            baseUrl: profile.arguments,
          }
        : provider,
    );
    message = "";
    error = "";
  }

  function selectProvider(id: string) {
    selectedId = id;
    agentProfileOverride = null;
    apiKey = "";
    message = "";
    error = "";
  }

  async function save() {
    if (!selected) return;
    busy = true;
    error = "";
    message = "";
    try {
      const saved = await invoke<Provider[]>("save_translation_provider_settings", {
        config: {
          id: selected.id,
          name: selected.name,
          enabled: selected.enabled,
          model: selected.model,
          baseUrl: selected.baseUrl,
          timeoutSeconds: selected.timeoutSeconds,
          retries: selected.retries,
          concurrency: selected.concurrency,
        },
        apiKey: (selected.requiresApiKey || selected.kind === "custom") && apiKey.trim() ? apiKey : null,
      });
      const drafts = providers.filter((provider) => !savedProviderIds.includes(provider.id) && provider.id !== selected.id);
      providers = [...saved, ...drafts];
      savedProviderIds = saved.map((provider) => provider.id);
      fallbackIds = await invoke<string[]>("save_translation_fallback", { ids: fallbackIds });
      policy = await invoke<Policy>("save_translation_policy", { policy });
      apiKey = "";
      message = "Provider and fallback settings saved.";
    } catch (reason) {
      error = `Unable to save provider settings: ${reason}`;
    } finally {
      busy = false;
    }
  }

  async function testConnection() {
    if (!selected) return;
    busy = true;
    error = "";
    message = "";
    try {
      message = await invoke<string>("test_translation_provider", {
        config: {
          id: selected.id,
          name: selected.name,
          enabled: selected.enabled,
          model: selected.model,
          baseUrl: selected.baseUrl,
          timeoutSeconds: selected.timeoutSeconds,
          retries: selected.retries,
          concurrency: selected.concurrency,
        },
        apiKey: (selected.requiresApiKey || selected.kind === "custom") && apiKey.trim() ? apiKey : null,
      });
    } catch (reason) {
      error = `Connection test failed: ${reason}`;
    } finally {
      busy = false;
    }
  }

  async function removeProvider() {
    if (!selected || selected.kind === "google" || !confirm(`Remove ${selected.name}?`)) return;
    const id = selected.id;
    busy = true;
    error = "";
    message = "";
    try {
      if (savedProviderIds.includes(id)) {
        const saved = await invoke<Provider[]>("delete_translation_provider", { id });
        const drafts = providers.filter((provider) => !savedProviderIds.includes(provider.id) && provider.id !== id);
        providers = [...saved, ...drafts];
        savedProviderIds = saved.map((provider) => provider.id);
      } else {
        providers = providers.filter((provider) => provider.id !== id);
      }
      fallbackIds = fallbackIds.filter((item) => item !== id);
      selectedId = providers[0]?.id ?? "google";
      agentProfileOverride = null;
      message = "Provider removed.";
    } catch (reason) {
      error = `Unable to remove provider: ${reason}`;
    } finally {
      busy = false;
    }
  }

  async function clearKey() {
    if (!selected) return;
    busy = true;
    try {
      providers = await invoke<Provider[]>("clear_translation_provider_key", { id: selected.id });
      apiKey = "";
      message = "API key removed.";
      error = "";
    } catch (reason) {
      error = `Unable to remove API key: ${reason}`;
    } finally {
      busy = false;
    }
  }

  function toggleFallback(id: string, enabled: boolean) {
    fallbackIds = enabled ? [...fallbackIds, id] : fallbackIds.filter((item) => item !== id);
  }

  function moveFallback(id: string, offset: number) {
    const index = fallbackIds.indexOf(id);
    const next = index + offset;
    if (index < 0 || next < 0 || next >= fallbackIds.length) return;
    const reordered = [...fallbackIds];
    [reordered[index], reordered[next]] = [reordered[next], reordered[index]];
    fallbackIds = reordered;
  }

  void load();
</script>

<div class="overlay" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="provider-title" tabindex="-1" onclick={(event) => event.stopPropagation()} onkeydown={() => {}}>
    <header>
      <div>
        <h2 id="provider-title">Translation Providers</h2>
        <p>API keys are stored in the operating system credential store.</p>
      </div>
      <button class="close" onclick={onclose} aria-label="Close">×</button>
    </header>

    {#if !loaded}
      <div class="loading">Loading providers...</div>
    {:else}
      <div class="body">
        <nav aria-label="Translation providers">
          <div class="provider-items">
            {#each providers as provider}
              <button class:active={provider.id === selectedId} onclick={() => selectProvider(provider.id)}>
                <span>{provider.name}</span>
                <small class:ready={provider.implemented}>{provider.implemented ? (provider.enabled ? "Enabled" : "Ready") : "Unavailable"}</small>
              </button>
            {/each}
          </div>
          <div class="add-actions">
            <button class="add-provider" onclick={addCustomProvider}><span>＋ Add Custom Provider</span></button>
            <button class="add-agent" onclick={addAgentCli}><span>＋ Add Agent CLI</span></button>
          </div>
        </nav>

        {#if selected}
          <div class="form">
            <div class="form-title">
              <h3>{selected.name}</h3>
              {#if selected.kind !== "google"}<button class="remove-agent" onclick={removeProvider} disabled={busy}>Remove</button>{/if}
            </div>
            {#if selected.kind === "google"}
              <p class="hint">Built-in Google Translate engine. No API key is required.</p>
            {:else if selectedIsAgentCli}
              <p class="hint">Run a supported AI agent CLI in its safe, non-interactive profile, or use a custom command.</p>
              <label>Display name<input value={selected.name} oninput={(event) => update("name", event.currentTarget.value)} placeholder="Example: Kiro Translator" /></label>
              <label>CLI profile
                <select value={selectedAgentProfileId} onchange={(event) => applyAgentProfile(event.currentTarget.value)}>
                  {#each agentCliProfiles as profile}
                    <option value={profile.id}>{profile.name} — {profile.installed ? "Installed" : "Not detected"}</option>
                  {/each}
                  <option value="custom">Custom executable</option>
                </select>
              </label>
              {#if selectedAgentProfile}
                <div class="profile-info" class:installed={selectedAgentProfile.installed}>
                  <strong>{selectedAgentProfile.installed ? "Detected" : "Not detected in PATH"}</strong>
                  <span>{selectedAgentProfile.description}</span>
                </div>
              {/if}
              <label>Executable<input value={selected.model} oninput={(event) => update("model", event.currentTarget.value)} placeholder="codex, claude, gemini, or full path" /></label>
              <label>Arguments (one per line)<textarea value={selected.baseUrl} oninput={(event) => update("baseUrl", event.currentTarget.value)} placeholder={"exec\n-\n--color\nnever"}></textarea></label>
              <p class="hint cli-hint">Profiles use stdin where supported and parse each CLI's final output. For Custom, use {"{prompt}"} inside one argument or omit it to send the prompt through stdin.</p>
            {:else if selected.kind === "custom"}
              <p class="hint">Connect any OpenAI-compatible chat-completions provider. API key is optional.</p>
              <label>Display name<input value={selected.name} oninput={(event) => update("name", event.currentTarget.value)} placeholder="Example: Company Translator" /></label>
              <label>Model<input value={selected.model} oninput={(event) => update("model", event.currentTarget.value)} placeholder="Provider model" /></label>
              <label>Base URL<input value={selected.baseUrl} oninput={(event) => update("baseUrl", event.currentTarget.value)} placeholder="https://api.example.com/v1" /></label>
              <label>API key (optional)<input type="password" bind:value={apiKey} placeholder={selected.hasApiKey ? "Stored securely; enter a new key to replace" : "Optional API key"} autocomplete="off" /></label>
              <div class="key-row">
                <span>{selected.hasApiKey ? "API key stored securely." : "No API key stored."}</span>
                {#if selected.hasApiKey}<button onclick={clearKey} disabled={busy}>Remove key</button>{/if}
              </div>
            {:else}
              <label>Model<input value={selected.model} oninput={(event) => update("model", event.currentTarget.value)} placeholder={selected.id === "deepl" ? "Not required" : "Provider model"} /></label>
              <label>Base URL<input value={selected.baseUrl} oninput={(event) => update("baseUrl", event.currentTarget.value)} placeholder="Provider endpoint" /></label>
              <label>API key<input type="password" bind:value={apiKey} placeholder={selected.hasApiKey ? "Stored securely; enter a new key to replace" : "Enter API key"} autocomplete="off" /></label>
              <div class="key-row">
                <span>{selected.hasApiKey ? "API key stored securely." : selected.id === "local" ? "API key is optional for Local AI." : "No API key stored."}</span>
                {#if selected.hasApiKey}<button onclick={clearKey} disabled={busy}>Remove key</button>{/if}
              </div>
            {/if}

            <label class="toggle"><input type="checkbox" checked={selected.enabled} onchange={(event) => update("enabled", event.currentTarget.checked)} /> Enable this provider</label>
            <div class="limits">
              <label>Timeout (seconds)<input type="number" min="5" max="600" value={selected.timeoutSeconds} oninput={(event) => update("timeoutSeconds", Number(event.currentTarget.value))} /></label>
              <label>Retries<input type="number" min="0" max="10" value={selected.retries} oninput={(event) => update("retries", Number(event.currentTarget.value))} /></label>
              <label>Excel concurrency<input type="number" min="1" max="32" value={selected.concurrency} oninput={(event) => update("concurrency", Number(event.currentTarget.value))} /></label>
            </div>

            <section class="fallback">
              <h4>Fallback order</h4>
              <p>If the selected engine fails, enabled providers below are tried from top to bottom.</p>
              {#each providers.filter((provider) => provider.implemented) as provider}
                {@const position = fallbackIds.indexOf(provider.id)}
                <div class="fallback-row">
                  <label><input type="checkbox" checked={position >= 0} onchange={(event) => toggleFallback(provider.id, event.currentTarget.checked)} /> {provider.name}</label>
                  {#if position >= 0}
                    <span>#{position + 1}</span>
                    <button title="Move up" onclick={() => moveFallback(provider.id, -1)} disabled={position === 0}>↑</button>
                    <button title="Move down" onclick={() => moveFallback(provider.id, 1)} disabled={position === fallbackIds.length - 1}>↓</button>
                  {/if}
                </div>
              {/each}
            </section>

            <section class="fallback policy">
              <h4>Shared translation policy</h4>
              <p>These options apply to both the Translate tab and Excel files.</p>
              <label><input type="checkbox" bind:checked={policy.useDictionary} /> Use approved Dictionary first</label>
              <label><input type="checkbox" bind:checked={policy.useTranslationMemory} /> Reuse Translation Memory</label>
              <label><input type="checkbox" bind:checked={policy.useCache} /> Use in-session cache</label>
              <label class="ttl">Cache TTL (seconds)<input type="number" min="1" max="86400" bind:value={policy.cacheTtlSeconds} /></label>
              <label><input type="checkbox" bind:checked={policy.saveTranslationMemory} /> Save new provider translations to Memory</label>
            </section>

            <footer>
              <div class="footer-message" aria-live="polite">
                {#if message}<p class="success">{message}</p>{/if}
                {#if error}<p class="error">{error}</p>{/if}
              </div>
              <div class="footer-actions">
                <button onclick={testConnection} disabled={busy || !selected.implemented}>Test connection</button>
                <button onclick={onclose}>Close</button>
                <button class="primary" onclick={save} disabled={busy}>{busy ? "Working..." : "Save"}</button>
              </div>
            </footer>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay{position:fixed;inset:0;z-index:1600;display:grid;place-items:center;padding:12px;background:rgba(0,0,0,.55)}
  .dialog{width:min(760px,calc(100vw - 24px));max-height:min(680px,calc(100vh - 24px));display:flex;flex-direction:column;overflow:hidden;color:var(--fg);background:var(--bg3);border:1px solid var(--border);border-top:3px solid var(--accent);border-radius:8px;box-shadow:0 16px 48px rgba(0,0,0,.48)}
  header{display:flex;justify-content:space-between;gap:12px;padding:14px 16px;border-bottom:1px solid var(--border)}
  h2,h3,h4,p{margin:0}h2{font-size:15px}h4{font-size:12px}header p,.hint{margin-top:4px;color:var(--fg2);font-size:11px}
  .close{border:0;background:transparent;color:var(--fg2);font-size:22px}
  .body{display:grid;grid-template-columns:185px 1fr;min-height:390px;overflow:hidden}
  nav{display:flex;min-height:0;flex-direction:column;padding:6px;background:var(--bg2);border-right:1px solid var(--border)}.provider-items{min-height:0;flex:1;overflow-y:auto}
  nav button{display:flex;justify-content:space-between;width:100%;padding:8px;border:0;border-radius:4px;color:var(--fg2);background:transparent;font:inherit;font-size:12px;cursor:pointer}
  nav button.active,nav button:hover{color:var(--fg);background:var(--btn-hover)}nav small{color:var(--warning);font-size:10px}nav small.ready{color:var(--accent)}
  .add-actions{display:grid;flex:0 0 auto;gap:5px;margin-top:6px}.add-actions button{justify-content:center;border:1px dashed var(--border);color:var(--accent)}
  .form{display:flex;min-height:0;flex-direction:column;padding:16px;overflow:auto}.form>label:not(.toggle){display:grid;gap:5px;margin-top:13px;color:var(--fg2);font-size:11px}.form-title{display:flex;align-items:center;justify-content:space-between;gap:10px}.remove-agent{padding:4px 8px;color:var(--error);background:transparent;border:1px solid color-mix(in srgb,var(--error) 55%,var(--border));border-radius:3px;font:inherit;font-size:10px;cursor:pointer}
  input:not([type="checkbox"]),textarea,select{width:100%;box-sizing:border-box;color:var(--fg);background:var(--combo-bg);border:1px solid var(--border);border-radius:3px}
  input:not([type="checkbox"]){height:29px;padding:0 8px}textarea{min-height:92px;padding:7px 8px;resize:vertical;font:11px ui-monospace,SFMono-Regular,Consolas,monospace}.cli-hint{margin-top:7px}
  select{height:29px;padding:0 7px}.profile-info{display:grid;gap:3px;margin-top:8px;padding:8px;color:var(--fg2);background:var(--bg2);border:1px solid var(--border);border-radius:4px;font-size:10px}.profile-info strong{color:var(--warning)}.profile-info.installed strong{color:var(--accent)}
  .toggle{display:flex;align-items:center;gap:7px;margin-top:17px;font-size:12px}.key-row{display:flex;align-items:center;justify-content:space-between;margin-top:6px;color:var(--fg2);font-size:10px}
  .limits{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-top:13px}.limits label,.ttl{display:grid;gap:5px;color:var(--fg2);font-size:10px}.ttl{grid-template-columns:1fr 100px;align-items:center}.ttl input{height:26px!important}
  .key-row button,footer button,.fallback-row button{height:28px;padding:0 9px;border:1px solid var(--border);border-radius:3px;color:var(--fg);background:var(--bg2);font:inherit;font-size:11px;cursor:pointer}
  .fallback{margin-top:18px;padding:12px;background:var(--bg2);border:1px solid var(--border);border-radius:5px}.fallback>p{margin:4px 0 8px;color:var(--fg2);font-size:10px}
  .fallback-row{display:flex;align-items:center;gap:5px;min-height:30px}.fallback-row label{flex:1;font-size:11px}.fallback-row span{width:25px;color:var(--fg2);font-size:10px}.fallback-row button{width:26px;padding:0}
  .policy{display:grid;gap:7px}.policy p{margin-bottom:2px}.policy label{font-size:11px}
  .success,.error{margin:0;font-size:11px}.success{color:var(--accent)}.error{color:var(--error)}footer{position:sticky;z-index:2;bottom:-16px;display:flex;align-items:center;justify-content:space-between;gap:12px;margin:20px -16px -16px;padding:12px 16px;background:var(--bg3);border-top:1px solid var(--border)}.footer-message{min-width:0;flex:1}.footer-actions{display:flex;flex:0 0 auto;gap:7px}.primary{color:var(--bg)!important;border-color:var(--accent)!important;background:var(--accent)!important}.loading{padding:30px;color:var(--fg2)}button:disabled{opacity:.55;cursor:default}
  @media(max-width:560px){.body{grid-template-columns:1fr}nav{flex-direction:row;border-right:0;border-bottom:1px solid var(--border);overflow-x:auto}.provider-items{display:flex;overflow:visible}nav button{min-width:max-content}.add-actions{display:flex;margin:0 0 0 6px}}
</style>
