<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";

  type Entry = { id:string; sourceLang:string; targetLang:string; sourceText:string; translation:string; provider:string; status:string; updatedAt:string };
  type SyncSettings = { serverUrl:string; workspaceId:string; deviceId:string; hasToken:boolean; cursor:string|null; pending:number; failed:number; lastError:string|null; conflicts:number };
  type SyncResult = { pushed:number; pulled:number; pending:number; cursor:string|null };
  let { onclose }: { onclose: () => void } = $props();
  let entries = $state<Entry[]>([]);
  let query = $state("");
  let message = $state("");
  let syncSettings = $state<SyncSettings>({ serverUrl:"", workspaceId:"", deviceId:"", hasToken:false, cursor:null, pending:0, failed:0, lastError:null, conflicts:0 });
  let syncToken = $state("");
  let syncBusy = $state(false);
  let showSyncSettings = $state(false);
  let filtered = $derived(entries.filter((entry) => `${entry.sourceText}\n${entry.translation}\n${entry.provider}\n${entry.status}`.toLowerCase().includes(query.trim().toLowerCase())));

  async function load() {
    try {
      [entries, syncSettings] = await Promise.all([
        invoke<Entry[]>("list_translation_memory_entries"),
        invoke<SyncSettings>("get_translation_sync_settings"),
      ]);
    }
    catch (reason) { message = String(reason); }
  }
  async function remove(id: string) {
    try { entries = await invoke<Entry[]>("delete_translation_memory_entry", { id }); }
    catch (reason) { message = String(reason); }
  }
  async function review(id: string, status: "suggested"|"approved"|"rejected") {
    try {
      entries = await invoke<Entry[]>("update_translation_memory_status", { id, status });
      syncSettings.pending += 1;
      message = `Translation Memory entry marked ${status}.`;
    }
    catch (reason) { message = String(reason); }
  }
  async function importCsv() {
    const path = await open({ multiple:false, filters:[{ name:"CSV", extensions:["csv"] }] });
    if (typeof path !== "string") return;
    try { entries = await invoke<Entry[]>("import_translation_memory_csv", { path }); message = `Imported successfully (${entries.length} total).`; }
    catch (reason) { message = String(reason); }
  }
  async function exportCsv() {
    const path = await save({ defaultPath:"translation-memory.csv", filters:[{ name:"CSV", extensions:["csv"] }] });
    if (!path) return;
    try { const count = await invoke<number>("export_translation_memory_csv", { path }); message = `Exported ${count} entries.`; }
    catch (reason) { message = String(reason); }
  }
  async function saveSyncSettings() {
    syncBusy = true; message = "";
    try {
      syncSettings = await invoke<SyncSettings>("save_translation_sync_settings", {
        config: { serverUrl:syncSettings.serverUrl, workspaceId:syncSettings.workspaceId, deviceId:syncSettings.deviceId },
        token: syncToken || null,
      });
      syncToken = ""; message = "Enterprise sync settings saved.";
    } catch (reason) { message = String(reason); }
    finally {
      try { syncSettings = await invoke<SyncSettings>("get_translation_sync_settings"); } catch {}
      syncBusy = false;
    }
  }
  async function testSyncConnection() {
    syncBusy = true; message = "";
    try { message = await invoke<string>("test_translation_sync_connection"); }
    catch (reason) { message = String(reason); }
    finally { syncBusy = false; }
  }
  async function synchronize() {
    syncBusy = true; message = "";
    try {
      const result = await invoke<SyncResult>("perform_translation_sync");
      syncSettings.pending = result.pending; syncSettings.cursor = result.cursor;
      entries = await invoke<Entry[]>("list_translation_memory_entries");
      message = `Sync completed: ${result.pushed} pushed, ${result.pulled} pulled, ${result.pending} pending.`;
    } catch (reason) { message = String(reason); }
    finally {
      try { syncSettings = await invoke<SyncSettings>("get_translation_sync_settings"); } catch {}
      syncBusy = false;
    }
  }
  void load();
</script>

<div class="overlay" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="tm-title" tabindex="-1" onclick={(event) => event.stopPropagation()} onkeydown={() => {}}>
    <header>
      <div><h2 id="tm-title">Translation Memory</h2><p>Shared automatically by text and Excel translation.</p></div>
      <div class="actions"><button onclick={() => showSyncSettings = !showSyncSettings}>Sync settings</button><button class="sync" onclick={synchronize} disabled={syncBusy || !syncSettings.serverUrl}>Sync now ({syncSettings.pending})</button><button onclick={importCsv}>Import CSV</button><button onclick={exportCsv}>Export CSV</button><button class="close" onclick={onclose} aria-label="Close">×</button></div>
    </header>
    <main>
      {#if showSyncSettings}
        <section class="sync-settings">
          <div><strong>Enterprise synchronization</strong><span>HTTPS push/pull API; credentials stay in the OS credential store.</span><span>{syncSettings.pending} pending · {syncSettings.failed} failed · {syncSettings.conflicts} conflicts</span>{#if syncSettings.lastError}<span class="sync-error">Last error: {syncSettings.lastError}</span>{/if}</div>
          <label>Server URL<input bind:value={syncSettings.serverUrl} placeholder="https://translate.example.com" /></label>
          <label>Workspace ID<input bind:value={syncSettings.workspaceId} placeholder="sabitech-vn" /></label>
          <label>Device ID<input bind:value={syncSettings.deviceId} /></label>
          <label>API token<input type="password" bind:value={syncToken} placeholder={syncSettings.hasToken ? "Stored securely; enter to replace" : "Enterprise API token"} autocomplete="off" /></label>
          <div class="sync-actions"><span>{syncSettings.cursor ? `Cursor: ${syncSettings.cursor}` : "Not synchronized yet"}</span><button onclick={testSyncConnection} disabled={syncBusy || !syncSettings.hasToken}>Test</button><button class="primary" onclick={saveSyncSettings} disabled={syncBusy}>Save</button></div>
        </section>
      {/if}
      <div class="toolbar"><input bind:value={query} placeholder="Search source, translation, or provider..." /><span>{filtered.length} / {entries.length}</span></div>
      {#if message}<p class="message">{message}</p>{/if}
      <div class="list">
        {#each filtered as entry}
          <article>
            <div class="meta"><span>{entry.sourceLang} → {entry.targetLang}</span><span>{entry.provider}</span><strong class:approved={entry.status === "approved"} class:rejected={entry.status === "rejected"}>{entry.status}</strong></div>
            <div class="pair"><div>{entry.sourceText}</div><div>{entry.translation}</div></div>
            <div class="review-actions"><button onclick={() => review(entry.id, "approved")} disabled={entry.status === "approved"}>Approve</button><button onclick={() => review(entry.id, "rejected")} disabled={entry.status === "rejected"}>Reject</button><button onclick={() => remove(entry.id)}>Delete</button></div>
          </article>
        {/each}
        {#if !filtered.length}<p class="empty">No matching translation memory entries.</p>{/if}
      </div>
    </main>
  </div>
</div>

<style>
  .overlay{position:fixed;inset:0;z-index:1600;display:grid;place-items:center;padding:12px;background:rgba(0,0,0,.55)}
  .dialog{width:min(860px,calc(100vw - 24px));max-height:calc(100vh - 24px);display:flex;flex-direction:column;color:var(--fg);background:var(--bg3);border:1px solid var(--border);border-top:3px solid var(--accent);border-radius:8px}
  header{display:flex;justify-content:space-between;align-items:center;gap:12px;padding:13px 16px;border-bottom:1px solid var(--border)}h2,p{margin:0}h2{font-size:15px}header p{margin-top:3px;color:var(--fg2);font-size:10px}.actions{display:flex;gap:6px}
  button,input{height:28px;border:1px solid var(--border);border-radius:3px;background:var(--combo-bg);color:var(--fg);font:inherit;font-size:11px;padding:0 8px}button{cursor:pointer}.close{border:0;background:transparent;font-size:22px}.sync,.primary{color:var(--bg);border-color:var(--accent);background:var(--accent)}button:disabled{opacity:.55;cursor:default}
  main{min-height:0;padding:14px;overflow:auto}.toolbar{display:flex;align-items:center;gap:10px}.toolbar input{flex:1}.toolbar span,.meta,.empty{color:var(--fg2);font-size:10px}.message{margin-top:8px;color:var(--accent);font-size:11px}.list{display:grid;gap:6px;margin-top:12px}
  .sync-settings{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin-bottom:13px;padding:12px;border:1px solid var(--border);border-radius:5px;background:var(--bg2)}.sync-settings>div:first-child{grid-column:1/-1}.sync-settings strong,.sync-settings span{display:block}.sync-settings strong{font-size:12px}.sync-settings span{margin-top:3px;color:var(--fg2);font-size:10px}.sync-settings label{display:grid;gap:4px;color:var(--fg2);font-size:10px}.sync-actions{grid-column:1/-1;display:flex;align-items:center;justify-content:flex-end;gap:6px}.sync-actions span{margin-right:auto}
  .sync-settings .sync-error{color:var(--error)}
  article{display:grid;grid-template-columns:110px 1fr auto;align-items:center;gap:9px;padding:9px;background:var(--bg2);border:1px solid var(--border);border-radius:4px}.meta{display:grid;gap:3px}.meta strong{width:max-content;padding:2px 5px;border-radius:8px;color:var(--warning);background:color-mix(in srgb,var(--warning) 14%,transparent);font-size:9px;text-transform:uppercase}.meta strong.approved{color:var(--accent);background:color-mix(in srgb,var(--accent) 14%,transparent)}.meta strong.rejected{color:var(--error);background:color-mix(in srgb,var(--error) 14%,transparent)}.pair{display:grid;grid-template-columns:1fr 1fr;gap:12px;font-size:12px}.pair div{white-space:pre-wrap;overflow-wrap:anywhere}.review-actions{display:flex;gap:4px}.review-actions button{padding:0 6px}
  @media(max-width:620px){article{grid-template-columns:1fr auto}.meta{grid-column:1}.pair{grid-column:1/-1;grid-template-columns:1fr}.actions button:not(.close):not(.sync){display:none}.sync-settings{grid-template-columns:1fr}.sync-settings>*{grid-column:1!important}}
</style>
