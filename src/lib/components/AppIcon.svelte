<script lang="ts">
  let { name, size = 14 }: { name: string; size?: number } = $props();
</script>

<svg class="app-icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === "translate"}
    <path d="M4 5h7M7.5 3v2M5 9c2.8-1 4.5-3 5.5-6M6 6c1 2 2.3 3.4 4 4.3"/><path d="m13 20 4-10 4 10M14.5 16h5"/>
  {:else if name === "diff"}
    <path d="M6 3v12a4 4 0 0 0 4 4h1M18 21V9a4 4 0 0 0-4-4h-1"/><path d="m3 6 3-3 3 3M15 18l3 3 3-3"/>
  {:else if name === "notes"}
    <path d="M5 3h11l3 3v15H5z"/><path d="M16 3v4h4M8 11h8M8 15h8"/>
  {:else if name === "layout"}
    <rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/>
  {:else if name === "wrap"}
    <path d="M4 6h12a4 4 0 1 1 0 8h-5M4 10h9M8 17l3-3-3-3"/>
  {:else if name === "detail"}
    <path d="M4 6h10M4 12h7M4 18h5"/><circle cx="16" cy="15" r="4"/><path d="m19 18 2 2"/>
  {:else if name === "actions"}
    <path d="M4 4v16M20 4v16M8 8h8M13 5l3 3-3 3M16 16H8M11 13l-3 3 3 3"/>
  {:else if name === "save"}
    <path d="M5 3h12l3 3v15H4V4z"/><path d="M8 3v6h8V3M8 21v-7h8v7"/>
  {:else if name === "add"}
    <circle cx="12" cy="12" r="9"/><path d="M12 8v8M8 12h8"/>
  {:else if name === "delete"}
    <path d="M4 7h16M9 7V4h6v3M7 7l1 14h8l1-14M10 11v6M14 11v6"/>
  {:else if name === "search"}
    <circle cx="11" cy="11" r="7"/><path d="m16 16 5 5"/>
  {:else if name === "search-left"}
    <rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/><circle cx="7.5" cy="10.5" r="2.5"/><path d="m9.3 12.3 2.2 2.2"/>
  {:else if name === "search-right"}
    <rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/><circle cx="16.5" cy="10.5" r="2.5"/><path d="m18.3 12.3 2.2 2.2"/>
  {:else if name === "copy"}
    <rect x="8" y="8" width="11" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h2"/>
  {:else if name === "clear"}
    <path d="m6 6 12 12M18 6 6 18"/>
  {:else if name === "swap"}
    <path d="M4 7h13M14 4l3 3-3 3M20 17H7M10 14l-3 3 3 3"/>
  {:else if name === "legacy"}
    <path d="M5 6h14M5 12h14M5 18h14"/><path d="m7 4-2 2 2 2M17 10l2 2-2 2M7 16l-2 2 2 2"/>
  {:else if name === "advanced"}
    <circle cx="6" cy="6" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="18" r="2"/><path d="M8 6h8M6 8v8M18 8v8M8 18h8M8 7.5l8 9"/>
  {:else if name === "expand"}
    <path d="M8 3H3v5M16 3h5v5M8 21H3v-5M16 21h5v-5"/><path d="m3 8 5-5M21 8l-5-5M3 16l5 5M21 16l-5 5"/>
  {:else if name === "preview"}
    <path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z"/><circle cx="12" cy="12" r="2.5"/>
  {:else if name === "word"}
    <path d="M4 18 9 6l5 12M6 14h6M14 18l3-8 3 8M15.5 15h3"/>
  {:else if name === "whitespace"}
    <path d="M9 5h8M13 5v14M9 5a4 4 0 1 0 0 8h4"/>
  {:else if name === "ignore-whitespace"}
    <path d="M9 5h8M13 5v14M9 5a4 4 0 1 0 0 8h4"/><path d="M4 4l16 16"/>
  {:else if name === "window"}
    <rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 9h18"/>
  {:else if name === "compact"}
    <path d="m8 3-4 4 4 4M16 3l4 4-4 4M8 21l-4-4 4-4M16 21l4-4-4-4"/>
  {:else if name === "pin"}
    <path d="m14 4 6 6-4 1-4 5-1 5-2-7-6-2 5-2 4-5z"/>
  {:else if name === "sun"}
    <circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.42 1.42M17.65 17.65l1.42 1.42M2 12h2M20 12h2M4.93 19.07l1.42-1.42M17.65 6.35l1.42-1.42"/>
  {:else if name === "moon"}
    <path d="M20 15.5A8.5 8.5 0 0 1 8.5 4 8.5 8.5 0 1 0 20 15.5z"/>
  {:else if name === "help"}
    <circle cx="12" cy="12" r="9"/><path d="M9.7 9a2.4 2.4 0 1 1 3.6 2.1c-.9.5-1.3 1.1-1.3 2.1M12 17h.01"/>
  {:else if name === "info"}
    <circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/>
  {/if}
</svg>

<style>
  .app-icon { display: block; flex: 0 0 auto; }
</style>
