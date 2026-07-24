<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type Provider = {
    id: string;
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

  let { onclose }: { onclose: () => void } = $props();
  let providers = $state<Provider[]>([]);
  let fallbackIds = $state<string[]>([]);
  let policy = $state<Policy>({ useDictionary:true, useTranslationMemory:true, useCache:true, saveTranslationMemory:true, cacheTtlSeconds:900 });
  let selectedId = $state("google");
  let apiKey = $state("");
  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let loaded = $state(false);
  let selected = $derived(providers.find((provider) => provider.id === selectedId));

  async function load() {
    try {
      [providers, fallbackIds, policy] = await Promise.all([
        invoke<Provider[]>("get_translation_provider_settings"),
        invoke<string[]>("get_translation_fallback"),
        invoke<Policy>("get_translation_policy"),
      ]);
      selectedId = providers.find((provider) => provider.enabled)?.id ?? providers[0]?.id ?? "google";
    } catch (reason) {
      error = `Unable to load provider settings: ${reason}`;
    } finally {
      loaded = true;
    }
  }

  function update(field: "enabled" | "model" | "baseUrl" | "timeoutSeconds" | "retries" | "concurrency", value: boolean | string | number) {
    if (!selected) return;
    providers = providers.map((provider) =>
      provider.id === selected.id ? { ...provider, [field]: value } : provider,
    );
  }

  async function save() {
    if (!selected) return;
    busy = true;
    error = "";
    message = "";
    try {
      providers = await invoke<Provider[]>("save_translation_provider_settings", {
        config: {
          id: selected.id,
          enabled: selected.enabled,
          model: selected.model,
          baseUrl: selected.baseUrl,
          timeoutSeconds: selected.timeoutSeconds,
          retries: selected.retries,
          concurrency: selected.concurrency,
        },
        apiKey: selected.requiresApiKey ? apiKey : null,
      });
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
      message = await invoke<string>("test_translation_provider", { engine: selected.name });
    } catch (reason) {
      error = `Connection test failed: ${reason}`;
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
          {#each providers as provider}
            <button class:active={provider.id === selectedId} onclick={() => { selectedId = provider.id; apiKey = ""; message = ""; error = ""; }}>
              <span>{provider.name}</span>
              <small class:ready={provider.implemented}>{provider.implemented ? (provider.enabled ? "Enabled" : "Ready") : "Unavailable"}</small>
            </button>
          {/each}
        </nav>

        {#if selected}
          <div class="form">
            <h3>{selected.name}</h3>
            {#if selected.id === "google"}
              <p class="hint">Built-in Google Translate engine. No API key is required.</p>
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

            {#if message}<p class="success">{message}</p>{/if}
            {#if error}<p class="error">{error}</p>{/if}
            <footer>
              <button onclick={testConnection} disabled={busy || !selected.implemented}>Test connection</button>
              <button onclick={onclose}>Close</button>
              <button class="primary" onclick={save} disabled={busy}>{busy ? "Working..." : "Save"}</button>
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
  nav{padding:6px;overflow-y:auto;background:var(--bg2);border-right:1px solid var(--border)}
  nav button{display:flex;justify-content:space-between;width:100%;padding:8px;border:0;border-radius:4px;color:var(--fg2);background:transparent;font:inherit;font-size:12px;cursor:pointer}
  nav button.active,nav button:hover{color:var(--fg);background:var(--btn-hover)}nav small{color:var(--warning);font-size:10px}nav small.ready{color:var(--accent)}
  .form{padding:16px;overflow:auto}.form>label:not(.toggle){display:grid;gap:5px;margin-top:13px;color:var(--fg2);font-size:11px}
  input:not([type="checkbox"]){width:100%;height:29px;box-sizing:border-box;padding:0 8px;color:var(--fg);background:var(--combo-bg);border:1px solid var(--border);border-radius:3px}
  .toggle{display:flex;align-items:center;gap:7px;margin-top:17px;font-size:12px}.key-row{display:flex;align-items:center;justify-content:space-between;margin-top:6px;color:var(--fg2);font-size:10px}
  .limits{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-top:13px}.limits label,.ttl{display:grid;gap:5px;color:var(--fg2);font-size:10px}.ttl{grid-template-columns:1fr 100px;align-items:center}.ttl input{height:26px!important}
  .key-row button,footer button,.fallback-row button{height:28px;padding:0 9px;border:1px solid var(--border);border-radius:3px;color:var(--fg);background:var(--bg2);font:inherit;font-size:11px;cursor:pointer}
  .fallback{margin-top:18px;padding:12px;background:var(--bg2);border:1px solid var(--border);border-radius:5px}.fallback>p{margin:4px 0 8px;color:var(--fg2);font-size:10px}
  .fallback-row{display:flex;align-items:center;gap:5px;min-height:30px}.fallback-row label{flex:1;font-size:11px}.fallback-row span{width:25px;color:var(--fg2);font-size:10px}.fallback-row button{width:26px;padding:0}
  .policy{display:grid;gap:7px}.policy p{margin-bottom:2px}.policy label{font-size:11px}
  .success,.error{margin-top:10px;font-size:11px}.success{color:var(--accent)}.error{color:var(--error)}footer{display:flex;justify-content:flex-end;gap:7px;margin-top:20px}.primary{color:var(--bg)!important;border-color:var(--accent)!important;background:var(--accent)!important}.loading{padding:30px;color:var(--fg2)}button:disabled{opacity:.55;cursor:default}
  @media(max-width:560px){.body{grid-template-columns:1fr}nav{display:flex;border-right:0;border-bottom:1px solid var(--border);overflow-x:auto}nav button{min-width:max-content}}
</style>
