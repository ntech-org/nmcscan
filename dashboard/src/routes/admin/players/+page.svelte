<script lang="ts">
    import { fetchWithAuth, authState } from '$lib/state.svelte';
    import { goto } from '$app/navigation';
    import MinecraftText from '$lib/components/MinecraftText.svelte';

    interface PlayerResponse {
        ip: string;
        player_name: string;
        last_seen: string;
    }

    let playerSearchQuery = $state('');
    let playerSearchResults = $state<PlayerResponse[]>([]);
    let playerSearchLoading = $state(false);
    let error = $state<string | null>(null);

    async function searchPlayers() {
        if (!authState.isAuthenticated || playerSearchQuery.length < 3) return;
        playerSearchLoading = true;
        error = null;
        try {
            const res = await fetchWithAuth(`/api/players?name=${encodeURIComponent(playerSearchQuery)}`);
            playerSearchResults = await res.json();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Player search failed';
        } finally {
            playerSearchLoading = false;
        }
    }

    function formatLastSeen(dateStr: string | null): string {
        if (!dateStr) return 'Never';
        return new Date(dateStr).toLocaleString();
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-white tracking-tight">Global Player Search</h1>
    </div>

    <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
        <p class="text-gray-400 text-sm mb-6">Track player sightings across all scanned networks.</p>
        
        <div class="flex gap-4">
            <div class="relative flex-1">
                <div class="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
                    <svg class="h-5 w-5 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"></path></svg>
                </div>
                <input
                    type="text"
                    placeholder="Enter player name (min 3 chars)..."
                    bind:value={playerSearchQuery}
                    onkeydown={(e) => e.key === 'Enter' && searchPlayers()}
                    class="w-full bg-gray-950 border border-gray-700 rounded-lg pl-11 pr-4 py-3 text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all shadow-inner"
                />
            </div>
            <button 
                onclick={searchPlayers}
                disabled={playerSearchLoading || playerSearchQuery.length < 3}
                class="px-6 py-3 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:hover:bg-blue-600 rounded-lg text-white font-medium transition-all shadow-md flex items-center gap-2"
            >
                {#if playerSearchLoading}
                    <svg class="animate-spin h-5 w-5 text-white" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
                    Searching
                {:else}
                    Search
                {/if}
            </button>
        </div>
        {#if error}
            <div class="mt-4 p-3 bg-red-500/10 border border-red-500/20 text-red-400 rounded-lg text-sm">{error}</div>
        {/if}
    </div>

    {#if playerSearchResults.length > 0}
        <div class="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden shadow-sm">
            <table class="w-full text-left border-collapse">
                <thead>
                    <tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider">
                        <th class="p-4 font-medium">Player Name</th>
                        <th class="p-4 font-medium">Server IP</th>
                        <th class="p-4 font-medium">Last Seen</th>
                        <th class="p-4 font-medium text-right">Actions</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-800/50">
                    {#each playerSearchResults as player}
                        <tr class="hover:bg-gray-800/20 transition-colors group cursor-pointer" onclick={() => goto(`/admin/servers/${player.ip}`)}>
                            <td class="p-4 font-medium text-white flex items-center gap-3">
                                <img src={`https://minotar.net/helm/${player.player_name}/32.png`} alt={player.player_name} class="w-8 h-8 rounded shadow-sm" onerror={(e) => { (e.currentTarget as HTMLImageElement).src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><rect width="24" height="24" fill="%23333"/><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="%23666"/></svg>'; }} />
                                <span class="truncate max-w-[200px]">
                                    <MinecraftText text={player.player_name} />
                                </span>
                            </td>
                            <td class="p-4 font-mono text-sm text-blue-400">{player.ip}</td>
                            <td class="p-4 text-sm text-gray-400">{formatLastSeen(player.last_seen)}</td>
                            <td class="p-4 text-right">
                                <button 
                                    class="px-3 py-1.5 bg-gray-800 group-hover:bg-gray-700 text-gray-300 rounded text-xs font-medium transition-colors"
                                >
                                    View Server
                                </button>
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    {:else if playerSearchQuery.length >= 3 && !playerSearchLoading && playerSearchResults.length === 0}
        <div class="bg-gray-900 border border-gray-800 rounded-xl p-16 text-center text-gray-500 shadow-sm">
            <svg class="w-16 h-16 mx-auto mb-4 opacity-20" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"></path></svg>
            <p>No records found for player "{playerSearchQuery}"</p>
        </div>
    {/if}
</div>
