<script lang="ts">
    import { onMount, untrack } from 'svelte';
    import { fetchWithAuth, authState } from '$lib/state.svelte';
    import { goto } from '$app/navigation';
    
    interface Server {
        ip: string;
        port: number;
        status: string;
        players_online: number;
        players_max: number;
        motd: string | null;
        version: string | null;
        priority: number;
        last_seen: string | null;
        consecutive_failures: number;
        asn?: string | null;
        country?: string | null;
    }

    let servers = $state<Server[]>([]);
    let loading = $state(true);
    let loadingMore = $state(false);
    let hasMore = $state(true);
    let error = $state<string | null>(null);

    // Filters
    let searchQuery = $state('');
    let statusFilter = $state('all');
    let minPlayers = $state<number | null>(null);
    let maxPlayers = $state<number | null>(null);
    let versionFilter = $state('');
    let whitelistProbMin = $state<number | null>(null);
    let asnCategory = $state('all');
    let countryFilter = $state('');
    let sortBy = $state('players');
    let sortOrder = $state('desc');
    
    let searchTimeout: ReturnType<typeof setTimeout>;

    async function searchServers(append = false) {
        if (!authState.isAuthenticated) return;
        
        if (append) loadingMore = true;
        else {
            loading = true;
            servers = [];
        }
        
        error = null;
        try {
            let url = `/api/servers?limit=50`;
            if (searchQuery) url += `&search=${encodeURIComponent(searchQuery)}`;
            if (statusFilter !== 'all') url += `&status=${statusFilter}`;
            if (minPlayers !== null && minPlayers !== undefined) url += `&min_players=${minPlayers}`;
            if (maxPlayers !== null && maxPlayers !== undefined) url += `&max_players=${maxPlayers}`;
            if (versionFilter) url += `&version=${encodeURIComponent(versionFilter)}`;
            if (whitelistProbMin !== null && whitelistProbMin !== undefined) url += `&whitelist_prob_min=${whitelistProbMin}`;
            if (asnCategory !== 'all') url += `&asn_category=${encodeURIComponent(asnCategory)}`;
            if (countryFilter) url += `&country=${encodeURIComponent(countryFilter)}`;
            
            url += `&sort_by=${sortBy}&sort_order=${sortOrder}`;

            if (append && servers.length > 0) {
                const last = servers[servers.length - 1];
                url += `&cursor_ip=${encodeURIComponent(last.ip)}`;
                if (sortBy === 'players') url += `&cursor_players=${last.players_online}`;
                if (sortBy === 'last_seen' && last.last_seen) url += `&cursor_last_seen=${encodeURIComponent(last.last_seen)}`;
            }

            const res = await fetchWithAuth(url);
            const newServers: Server[] = await res.json();
            
            if (append) {
                servers = [...servers, ...newServers];
            } else {
                servers = newServers;
            }
            
            hasMore = newServers.length === 50;
        } catch (e) {
            error = e instanceof Error ? e.message : 'Search failed';
        } finally {
            loading = false;
            loadingMore = false;
        }
    }

    function onFilterChange() {
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(() => {
            untrack(() => searchServers());
        }, 500);
    }

    function getStatusColor(status: string): string {
        switch (status) {
            case 'online': return 'text-green-400 bg-green-400/10 border-green-500/20';
            case 'offline': return 'text-red-400 bg-red-400/10 border-red-500/20';
            default: return 'text-gray-400 bg-gray-400/10 border-gray-500/20';
        }
    }

    onMount(() => {
        searchServers();
    });
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-white tracking-tight">Server Directory</h1>
        <button onclick={() => searchServers()} aria-label="Refresh Servers" class="p-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 transition-colors shadow-sm">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
        </button>
    </div>

    <!-- Advanced Filters -->
    <div class="bg-gray-900 border border-gray-800 rounded-xl p-5 shadow-sm space-y-4">
        <div class="flex flex-col sm:flex-row gap-4">
            <div class="relative flex-1">
                <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                    <svg class="h-5 w-5 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"></path></svg>
                </div>
                <input
                    aria-label="Search Query"
                    type="text"
                    placeholder="Search IP, MOTD, or Version..."
                    bind:value={searchQuery}
                    oninput={onFilterChange}
                    class="w-full bg-gray-950 border border-gray-700 rounded-lg pl-10 pr-4 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all"
                />
            </div>
            <select
                aria-label="Status Filter"
                bind:value={statusFilter}
                onchange={onFilterChange}
                class="w-full sm:w-40 bg-gray-950 border border-gray-700 rounded-lg px-3 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
            >
                <option value="all">Any Status</option>
                <option value="online">Online</option>
                <option value="offline">Offline</option>
            </select>
            <select
                aria-label="ASN Category Filter"
                bind:value={asnCategory}
                onchange={onFilterChange}
                class="w-full sm:w-48 bg-gray-950 border border-gray-700 rounded-lg px-3 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
            >
                <option value="all">Any Network</option>
                <option value="hosting">Hosting</option>
                <option value="residential">Residential</option>
                <option value="unknown">Unknown</option>
            </select>
            <div class="flex gap-2 w-full sm:w-auto">
                <select
                    aria-label="Sort By"
                    bind:value={sortBy}
                    onchange={onFilterChange}
                    class="w-full sm:w-32 bg-gray-950 border border-gray-700 rounded-lg px-3 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
                >
                    <option value="players">Players</option>
                    <option value="last_seen">Last Seen</option>
                    <option value="ip">IP Address</option>
                </select>
                <select
                    aria-label="Sort Order"
                    bind:value={sortOrder}
                    onchange={onFilterChange}
                    class="w-full sm:w-24 bg-gray-950 border border-gray-700 rounded-lg px-3 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
                >
                    <option value="desc">DESC</option>
                    <option value="asc">ASC</option>
                </select>
            </div>
        </div>
        
        <!-- Deep Filters -->
        <div class="grid grid-cols-2 md:grid-cols-5 gap-4 pt-4 border-t border-gray-800">
            <div>
                <label for="minPlayers" class="block text-xs font-medium text-gray-500 uppercase mb-1">Min Players</label>
                <input id="minPlayers" type="number" bind:value={minPlayers} oninput={onFilterChange} placeholder="0" class="w-full bg-gray-950 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:border-blue-500 outline-none" />
            </div>
            <div>
                <label for="maxPlayers" class="block text-xs font-medium text-gray-500 uppercase mb-1">Max Players</label>
                <input id="maxPlayers" type="number" bind:value={maxPlayers} oninput={onFilterChange} placeholder="&infin;" class="w-full bg-gray-950 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:border-blue-500 outline-none" />
            </div>
            <div>
                <label for="versionFilter" class="block text-xs font-medium text-gray-500 uppercase mb-1">Version Match</label>
                <input id="versionFilter" type="text" bind:value={versionFilter} oninput={onFilterChange} placeholder="e.g. 1.20" class="w-full bg-gray-950 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:border-blue-500 outline-none" />
            </div>
            <div>
                <label for="countryFilter" class="block text-xs font-medium text-gray-500 uppercase mb-1">Country (ISO)</label>
                <input id="countryFilter" type="text" bind:value={countryFilter} oninput={onFilterChange} placeholder="US, DE, etc." class="w-full bg-gray-950 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:border-blue-500 outline-none" />
            </div>
            <div>
                <label for="whitelistProbMin" class="block text-xs font-medium text-gray-500 uppercase mb-1">Min Whitelist %</label>
                <input id="whitelistProbMin" type="number" step="0.1" bind:value={whitelistProbMin} oninput={onFilterChange} placeholder="0.0 - 1.0" class="w-full bg-gray-950 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:border-blue-500 outline-none" />
            </div>
        </div>
    </div>

    <!-- Results Table -->
    <div class="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden shadow-sm">
        <div class="overflow-x-auto">
            <table class="w-full text-left border-collapse">
                <thead>
                    <tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider">
                        <th class="p-4 font-medium">Server Address</th>
                        <th class="p-4 font-medium">Status</th>
                        <th class="p-4 font-medium">Players</th>
                        <th class="p-4 font-medium">Version</th>
                        <th class="p-4 font-medium">Network</th>
                        <th class="p-4 font-medium text-right">Actions</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-800/50">
                    {#if loading && servers.length === 0}
                        <tr><td colspan="6" class="p-8 text-center text-gray-500"><svg class="animate-spin h-6 w-6 mx-auto text-blue-500" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg></td></tr>
                    {:else if servers.length === 0}
                        <tr><td colspan="6" class="p-12 text-center text-gray-500">No servers found matching criteria.</td></tr>
                    {/if}
                    {#each servers as server}
                        <tr class="hover:bg-gray-800/40 transition-colors group cursor-pointer" onclick={() => goto(`/admin/servers/${server.ip}`)}>
                            <td class="p-4">
                                <div class="font-mono text-sm text-gray-200">{server.ip}</div>
                                <div class="text-xs text-gray-500">Port {server.port}</div>
                            </td>
                            <td class="p-4">
                                <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border {getStatusColor(server.status)}">
                                    <span class="w-1.5 h-1.5 rounded-full mr-1.5 {server.status === 'online' ? 'bg-green-400' : 'bg-gray-400'}"></span>
                                    {server.status}
                                </span>
                            </td>
                            <td class="p-4 text-sm">
                                <div class="flex items-center gap-2">
                                    <div class="w-16 h-1.5 bg-gray-800 rounded-full overflow-hidden">
                                        <div 
                                            class="h-full bg-blue-500 rounded-full" 
                                            style="width: {server.players_max > 0 ? Math.min((server.players_online / server.players_max) * 100, 100) : 0}%"
                                        ></div>
                                    </div>
                                    <span class="{server.players_online > 0 ? 'text-white' : 'text-gray-500'} font-medium">{server.players_online}</span>
                                    <span class="text-gray-600">/ {server.players_max}</span>
                                </div>
                            </td>
                            <td class="p-4">
                                <span class="inline-block px-2 py-1 bg-gray-800 text-gray-300 rounded text-xs truncate max-w-[120px]" title={server.version || 'Unknown'}>
                                    {server.version || 'Unknown'}
                                </span>
                            </td>
                            <td class="p-4 text-sm text-gray-400">
                                <div class="flex items-center gap-2">
                                    {#if server.country}
                                        <img src={`https://flagcdn.com/16x12/${server.country.toLowerCase()}.png`} alt={server.country} class="rounded shadow-sm opacity-80" />
                                    {/if}
                                    <span class="truncate max-w-[100px]" title={server.asn || ''}>{server.asn || '-'}</span>
                                </div>
                            </td>
                            <td class="p-4 text-right">
                                <button 
                                    class="px-3 py-1.5 bg-blue-600/10 hover:bg-blue-600/20 text-blue-400 rounded text-xs font-medium border border-blue-500/20 transition-all shadow-sm"
                                >
                                    Details
                                </button>
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        
        {#if hasMore && servers.length > 0}
            <div class="p-6 border-t border-gray-800 flex justify-center">
                <button 
                    onclick={() => searchServers(true)}
                    disabled={loadingMore}
                    class="px-8 py-2.5 bg-gray-800 hover:bg-gray-700 disabled:opacity-50 text-gray-200 rounded-lg text-sm font-medium transition-all border border-gray-700 flex items-center gap-2 shadow-sm"
                >
                    {#if loadingMore}
                        <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
                        Loading...
                    {:else}
                        Load More Results
                    {/if}
                </button>
            </div>
        {/if}
    </div>
</div>
