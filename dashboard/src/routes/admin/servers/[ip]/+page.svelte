<script lang="ts">
    import { page } from '$app/stores';
    import { onMount, untrack } from 'svelte';
    import { fetchWithAuth, authState } from '$lib/state.svelte';
    import MinecraftText from '$lib/components/MinecraftText.svelte';
    import {
        Chart,
        Title,
        Tooltip,
        Legend,
        LineElement,
        LinearScale,
        PointElement,
        CategoryScale,
        LineController,
        Filler
    } from 'chart.js';

    Chart.register(
        Title,
        Tooltip,
        Legend,
        LineElement,
        LinearScale,
        PointElement,
        CategoryScale,
        LineController,
        Filler
    );

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
        whitelist_prob: number;
        asn?: string | null;
        country?: string | null;
    }

    interface HistoryResponse {
        timestamp: string;
        players_online: number;
    }

    interface PlayerResponse {
        player_name: string;
        player_uuid: string;
        last_seen: string;
    }

    let ip = $derived($page.params.ip);
    let server = $state<Server | null>(null);
    let history = $state<HistoryResponse[]>([]);
    let players = $state<PlayerResponse[]>([]);
    
    let loading = $state(true);
    let error = $state<string | null>(null);

    let chartCanvas = $state<HTMLCanvasElement | null>(null);
    let chart: Chart | null = null;

    async function loadServerData() {
        if (!authState.isAuthenticated || !ip) return;
        loading = true;
        try {
            const [serverRes, historyRes, playersRes] = await Promise.all([
                fetchWithAuth(`/api/server/${ip}`),
                fetchWithAuth(`/api/server/${ip}/history`),
                fetchWithAuth(`/api/server/${ip}/players`)
            ]);
            
            server = await serverRes.json();
            const rawHistory = await historyRes.json();
            history = rawHistory.reverse(); // chronological order
            players = await playersRes.json();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to load server data';
        } finally {
            loading = false;
        }
    }

    onMount(() => {
        loadServerData();
        return () => {
            if (chart) chart.destroy();
        };
    });

    $effect(() => {
        if (history.length > 0 && chartCanvas) {
            if (chart) chart.destroy();
            
            untrack(() => {
                const labels = history.map(d => 
                    new Date(d.timestamp).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit', month: 'short', day: 'numeric'})
                );
                const values = history.map(d => d.players_online);

                chart = new Chart(chartCanvas!, {
                    type: 'line',
                    data: {
                        labels,
                        datasets: [{
                            label: 'Players Online',
                            data: values,
                            borderColor: '#3b82f6',
                            backgroundColor: 'rgba(59, 130, 246, 0.1)',
                            tension: 0.4,
                            fill: true,
                            pointRadius: 3,
                            pointBackgroundColor: '#3b82f6',
                            borderWidth: 2
                        }]
                    },
                    options: {
                        responsive: true,
                        maintainAspectRatio: false,
                        scales: {
                            y: { 
                                beginAtZero: true,
                                grid: { color: 'rgba(255, 255, 255, 0.05)' },
                                ticks: { color: '#9ca3af' }
                            },
                            x: {
                                grid: { display: false },
                                ticks: { color: '#9ca3af', maxRotation: 45, minRotation: 45 }
                            }
                        },
                        plugins: {
                            legend: { display: false },
                            tooltip: {
                                backgroundColor: 'rgba(17, 24, 39, 0.9)',
                                titleColor: '#fff',
                                bodyColor: '#60a5fa',
                                borderColor: 'rgba(55, 65, 81, 1)',
                                borderWidth: 1,
                                padding: 12,
                                displayColors: false
                            }
                        },
                        interaction: { mode: 'index', intersect: false }
                    }
                });
            });
        }
    });

    function getStatusColor(status: string | undefined): string {
        switch (status) {
            case 'online': return 'text-green-400 bg-green-400/10 border-green-500/20';
            case 'offline': return 'text-red-400 bg-red-400/10 border-red-500/20';
            default: return 'text-gray-400 bg-gray-400/10 border-gray-500/20';
        }
    }
</script>

<div class="space-y-6">
    <div class="flex items-center gap-4">
        <a href="/admin/servers" aria-label="Back to servers list" class="p-2 bg-gray-800 hover:bg-gray-700 rounded-lg text-gray-300 transition-colors">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"></path></svg>
        </a>
        <h1 class="text-2xl font-bold text-white tracking-tight flex items-center gap-3">
            {ip}
            {#if server}
                <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border {getStatusColor(server.status)}">
                    {server.status}
                </span>
            {/if}
        </h1>
    </div>

    {#if loading}
        <div class="flex justify-center py-20">
            <svg class="animate-spin h-8 w-8 text-blue-500" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
        </div>
    {:else if error}
        <div class="p-6 bg-red-500/10 border border-red-500/20 text-red-400 rounded-xl">
            {error}
        </div>
    {:else if server}
        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <!-- Server Details Card -->
            <div class="lg:col-span-1 space-y-6">
                <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
                    <h2 class="text-lg font-semibold text-white mb-4">Server Details</h2>
                    <dl class="space-y-4 text-sm">
                        <div class="flex justify-between pb-3 border-b border-gray-800/50">
                            <dt class="text-gray-500">Port</dt>
                            <dd class="text-gray-200 font-mono">{server.port}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-gray-800/50">
                            <dt class="text-gray-500">Version</dt>
                            <dd class="text-gray-200">{server.version || 'Unknown'}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-gray-800/50">
                            <dt class="text-gray-500">Players</dt>
                            <dd class="text-gray-200">{server.players_online} / {server.players_max}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-gray-800/50">
                            <dt class="text-gray-500">Whitelist Prob.</dt>
                            <dd class="text-gray-200">{(server.whitelist_prob * 100).toFixed(1)}%</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-gray-800/50">
                            <dt class="text-gray-500">Last Seen</dt>
                            <dd class="text-gray-200">{server.last_seen ? new Date(server.last_seen).toLocaleString() : 'Never'}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-gray-800/50">
                            <dt class="text-gray-500">Network (ASN)</dt>
                            <dd class="text-gray-200 text-right truncate max-w-[150px]" title={server.asn || ''}>{server.asn || 'Unknown'}</dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-gray-500">Country</dt>
                            <dd class="text-gray-200">
                                {#if server.country}
                                    <div class="flex items-center gap-2">
                                        <span>{server.country}</span>
                                        <img src={`https://flagcdn.com/20x15/${server.country.toLowerCase()}.png`} alt={server.country} class="rounded shadow-sm" />
                                    </div>
                                {:else}
                                    Unknown
                                {/if}
                            </dd>
                        </div>
                    </dl>
                </div>
                
                <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
                    <h2 class="text-lg font-semibold text-white mb-2">Message of the Day</h2>
                    <div class="p-4 bg-black/50 border border-gray-800 rounded-lg font-mono text-sm break-words">
                        <MinecraftText text={server.motd || 'No MOTD provided'} />
                    </div>
                </div>
            </div>

            <!-- Main Panel -->
            <div class="lg:col-span-2 space-y-6">
                <!-- Chart -->
                <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
                    <h2 class="text-lg font-semibold text-white mb-4">Player Activity</h2>
                    {#if history.length > 0}
                        <div class="h-64 w-full">
                            <canvas bind:this={chartCanvas}></canvas>
                        </div>
                    {:else}
                        <div class="h-64 w-full flex items-center justify-center text-gray-500 border border-dashed border-gray-800 rounded-lg">
                            No historical data available.
                        </div>
                    {/if}
                </div>

                <!-- Players List -->
                <div class="bg-gray-900 border border-gray-800 rounded-xl shadow-sm overflow-hidden">
                    <div class="p-4 border-b border-gray-800 bg-gray-950/30 flex items-center justify-between">
                        <h2 class="text-lg font-semibold text-white">Known Players ({players.length})</h2>
                    </div>
                    <div class="max-h-[400px] overflow-y-auto">
                        <table class="w-full text-left border-collapse">
                            <thead class="sticky top-0 bg-gray-950/90 backdrop-blur-md z-10 border-b border-gray-800">
                                <tr class="text-gray-400 text-xs uppercase tracking-wider">
                                    <th class="p-4 font-medium">Player</th>
                                    <th class="p-4 font-medium">UUID</th>
                                    <th class="p-4 font-medium text-right">Last Seen</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-gray-800/50">
                                {#if players.length === 0}
                                    <tr><td colspan="3" class="p-8 text-center text-gray-500">No players recorded for this server.</td></tr>
                                {/if}
                                {#each players as player}
                                    <tr class="hover:bg-gray-800/20 transition-colors">
                                        <td class="p-4">
                                            <div class="flex items-center gap-3">
                                                <img src={`https://minotar.net/helm/${player.player_name}/32.png`} alt={player.player_name} class="w-8 h-8 rounded" onerror={(e) => { (e.currentTarget as HTMLImageElement).src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><rect width="24" height="24" fill="%23333"/><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="%23666"/></svg>'; }} />
                                                <span class="font-medium text-white truncate max-w-[200px]">
                                                    <MinecraftText text={player.player_name} />
                                                </span>
                                            </div>
                                        </td>
                                        <td class="p-4 font-mono text-xs text-gray-500">
                                            {player.player_uuid}
                                        </td>
                                        <td class="p-4 text-sm text-gray-400 text-right">
                                            {new Date(player.last_seen).toLocaleString()}
                                        </td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    {/if}
</div>
