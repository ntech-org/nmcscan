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
        server_type: string;
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
        asn_org?: string | null;
        asn_tags?: string[];
        favicon?: string | null;
        brand?: string | null;
        login_obstacle?: string | null;
        last_login_at?: string | null;
        flags?: string[];
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
        if (!ip) return;
        loading = true;
        try {
            const [serverRes, historyRes, playersRes] = await Promise.all([
                fetchWithAuth(`/api/server/${ip}`),
                fetchWithAuth(`/api/server/${ip}/history`),
                fetchWithAuth(`/api/server/${ip}/players`)
            ]);
            
            server = await serverRes.json();
            history = await historyRes.json(); // Backend already returns chronological order
            players = await playersRes.json();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to load server data';
        } finally {
            loading = false;
        }
    }

    $effect(() => {
        // Automatically reload when IP changes
        if (ip) {
            untrack(() => loadServerData());
        }
    });

    onMount(() => {
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
            case 'online': return 'text-emerald-500 bg-emerald-500/10 border-emerald-500/20';
            case 'offline': return 'text-destructive bg-destructive/10 border-destructive/20';
            default: return 'text-muted-foreground bg-muted border-border';
        }
    }

    function getTagColor(tag: string): string {
        const t = tag.toLowerCase();
        if (t.includes('ddos')) return 'text-destructive bg-destructive/10 border-destructive/20';
        if (t.includes('cloud')) return 'text-purple-500 bg-purple-500/10 border-purple-500/20';
        if (t.includes('cdn')) return 'text-teal-500 bg-teal-500/10 border-teal-500/20';
        if (t.includes('vpn') || t.includes('proxy')) return 'text-yellow-500 bg-yellow-500/10 border-yellow-500/20';
        if (t.includes('defense') || t.includes('security')) return 'text-destructive bg-destructive/10 border-destructive/20';
        return 'text-muted-foreground bg-muted border-border';
    }
</script>

<div class="space-y-6">
    <div class="flex items-center gap-4">
        <button onclick={() => window.history.back()} aria-label="Go back" class="p-2 bg-muted hover:bg-muted/80 rounded-lg text-muted-foreground transition-colors">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18"></path></svg>
        </button>
        <div class="flex items-center gap-3">
            {#if server?.favicon}
                <img src={server.favicon} alt="" class="w-10 h-10 rounded-lg shadow-sm rendering-pixelated" />
            {:else}
                <div class="w-10 h-10 rounded-lg bg-muted border border-border flex items-center justify-center text-muted-foreground">
                    <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9"></path></svg>
                </div>
            {/if}
            <h1 class="text-2xl font-bold text-foreground tracking-tight flex items-center gap-3">
                {ip}
                {#if server}
                    <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border {getStatusColor(server.status)}">
                        {server.status}
                    </span>
                {/if}
            </h1>
        </div>
    </div>

    {#if loading}
        <div class="flex justify-center py-20">
            <svg class="animate-spin h-8 w-8 text-primary" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
        </div>
    {:else if error}
        <div class="p-6 bg-destructive/10 border border-destructive/20 text-destructive rounded-xl">
            {error}
        </div>
    {:else if server}
        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
            <!-- Server Details Card -->
            <div class="lg:col-span-1 space-y-6">
                <div class="bg-card border border-border rounded-xl p-6 shadow-sm">
                    <h2 class="text-lg font-semibold text-foreground mb-4">Server Details</h2>
                    <dl class="space-y-4 text-sm">
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Port</dt>
                            <dd class="text-foreground font-mono">{server.port}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Type</dt>
                            <dd class="text-foreground uppercase font-bold text-[10px] tracking-widest">{server.server_type}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Software</dt>
                            <dd class="text-foreground text-right">
                                <div>{server.brand || 'Vanilla'}</div>
                                <div class="text-[10px] text-muted-foreground italic truncate max-w-[150px]">{server.version || 'Unknown'}</div>
                            </dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Players</dt>
                            <dd class="text-foreground">{server.players_online} / {server.players_max}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Priority</dt>
                            <dd class="text-foreground">
                                <span class="px-2 py-0.5 rounded bg-primary/10 text-primary border border-primary/20 text-[10px] font-bold">
                                    Tier {server.priority}
                                </span>
                            </dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Scan Status</dt>
                            <dd class="text-foreground">
                                {#if server.consecutive_failures > 0}
                                    <span class="text-destructive font-medium">{server.consecutive_failures} Failures</span>
                                {:else}
                                    <span class="text-emerald-500 font-medium">Healthy</span>
                                {/if}
                            </dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Whitelist Prob.</dt>
                            <dd class="text-foreground">{(server.whitelist_prob * 100).toFixed(1)}%</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Last Seen</dt>
                            <dd class="text-foreground">{server.last_seen ? new Date(server.last_seen).toLocaleString() : 'Never'}</dd>
                        </div>
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Network (ASN)</dt>
                            <dd class="text-foreground text-right">
                                {#if server.asn}
                                    <div class="flex flex-col items-end gap-1">
                                        <a href={`/explore/servers?asn=${server.asn}`} class="text-primary hover:text-primary/80 hover:underline font-mono">
                                            {server.asn}
                                        </a>
                                        {#if server.asn_org}
                                            <span class="text-xs text-muted-foreground truncate max-w-[180px]" title={server.asn_org}>{server.asn_org}</span>
                                        {/if}
                                        {#if server.asn_tags && server.asn_tags.length > 0}
                                            <div class="flex flex-wrap justify-end gap-1 mt-0.5">
                                                {#each server.asn_tags as tag}
                                                    <span class="px-1.5 py-0.5 rounded-md border {getTagColor(tag)} text-[9px] font-bold uppercase tracking-tight">
                                                        {tag}
                                                    </span>
                                                {/each}
                                            </div>
                                        {/if}
                                    </div>
                                {:else}
                                    Unknown
                                {/if}
                            </dd>
                        </div>
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">Country</dt>
                            <dd class="text-foreground">
                                {#if server.country}
                                    <a href={`/explore/servers?country=${server.country}`} class="flex items-center gap-2 hover:text-primary transition-colors">
                                        <span>{server.country}</span>
                                        <img src={`https://flagcdn.com/20x15/${server.country.toLowerCase()}.png`} alt={server.country} class="rounded shadow-sm" />
                                    </a>
                                {:else}
                                    Unknown
                                {/if}
                            </dd>
                        </div>
                        {#if server.login_obstacle}
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Login Status</dt>
                            <dd>
                                <span class="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider
                                    {server.login_obstacle === 'success' ? 'bg-emerald-500/10 text-emerald-500 border border-emerald-500/20' : ''}
                                    {server.login_obstacle === 'premium' ? 'bg-blue-500/10 text-blue-500 border border-blue-500/20' : ''}
                                    {server.login_obstacle === 'whitelist' ? 'bg-yellow-500/10 text-yellow-500 border border-yellow-500/20' : ''}
                                    {server.login_obstacle === 'banned' ? 'bg-destructive/10 text-destructive border border-destructive/20' : ''}
                                    {!['success','premium','whitelist','banned'].includes(server.login_obstacle) ? 'bg-muted text-muted-foreground border border-border' : ''}
                                ">
                                    {server.login_obstacle === 'success' ? 'Cracked' : ''}
                                    {server.login_obstacle === 'premium' ? 'Premium Only' : ''}
                                    {server.login_obstacle === 'whitelist' ? 'Whitelisted' : ''}
                                    {server.login_obstacle === 'banned' ? 'Banned' : ''}
                                    {!['success','premium','whitelist','banned'].includes(server.login_obstacle) ? server.login_obstacle : ''}
                                </span>
                            </dd>
                        </div>
                        {/if}
                        {#if server.last_login_at}
                        <div class="flex justify-between pb-3 border-b border-border/50">
                            <dt class="text-muted-foreground">Last Login Test</dt>
                            <dd class="text-foreground text-sm">{new Date(server.last_login_at).toLocaleString()}</dd>
                        </div>
                        {/if}
                        {#if server.flags && server.flags.length > 0}
                        <div class="flex justify-between">
                            <dt class="text-muted-foreground">Flags</dt>
                            <dd class="flex flex-wrap justify-end gap-1">
                                {#each server.flags as flag}
                                    <span class="px-1.5 py-0.5 rounded bg-muted text-muted-foreground text-[9px] font-bold uppercase tracking-tight border border-border">
                                        {flag}
                                    </span>
                                {/each}
                            </dd>
                        </div>
                        {/if}
                    </dl>
                </div>
                
                <div class="bg-card border border-border rounded-xl p-6 shadow-sm">
                    <h2 class="text-lg font-semibold text-foreground mb-2">Message of the Day</h2>
                    <div class="p-4 bg-muted/50 border border-border rounded-lg font-mono text-sm break-words">
                        <MinecraftText text={server.motd || 'No MOTD provided'} />
                    </div>
                </div>
            </div>

            <!-- Main Panel -->
            <div class="lg:col-span-2 space-y-6">
                <!-- Chart -->
                <div class="bg-card border border-border rounded-xl p-6 shadow-sm">
                    <h2 class="text-lg font-semibold text-foreground mb-4">Player Activity</h2>
                    {#if history.length > 0}
                        <div class="h-64 w-full">
                            <canvas bind:this={chartCanvas}></canvas>
                        </div>
                    {:else}
                        <div class="h-64 w-full flex items-center justify-center text-muted-foreground border border-dashed border-border rounded-lg">
                            No historical data available.
                        </div>
                    {/if}
                </div>

                <!-- Players List -->
                <div class="bg-card border border-border rounded-xl shadow-sm overflow-hidden">
                    <div class="p-4 border-b border-border bg-muted/20 flex items-center justify-between">
                        <h2 class="text-lg font-semibold text-foreground">Known Players ({players.length})</h2>
                    </div>
                    <div class="max-h-[400px] overflow-y-auto">
                        <table class="w-full text-left border-collapse">
                            <thead class="sticky top-0 bg-background/90 backdrop-blur-md z-10 border-b border-border">
                                <tr class="text-muted-foreground text-xs uppercase tracking-wider">
                                    <th class="p-4 font-medium">Player</th>
                                    <th class="p-4 font-medium">UUID</th>
                                    <th class="p-4 font-medium text-right">Last Seen</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-border/50">
                                {#if players.length === 0}
                                    <tr><td colspan="3" class="p-8 text-center text-muted-foreground">No players recorded for this server.</td></tr>
                                {/if}
                                {#each players as player}
                                    <tr class="hover:bg-muted/20 transition-colors">
                                        <td class="p-4">
                                            <div class="flex items-center gap-3">
                                                <img src={`https://minotar.net/helm/${player.player_name}/32.png`} alt={player.player_name} class="w-8 h-8 rounded" onerror={(e) => { (e.currentTarget as HTMLImageElement).src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><rect width="24" height="24" fill="%23333"/><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="%23666"/></svg>'; }} />
                                                <span class="font-medium text-foreground truncate max-w-[200px]">
                                                    <MinecraftText text={player.player_name} />
                                                </span>
                                            </div>
                                        </td>
                                        <td class="p-4 font-mono text-xs text-muted-foreground">
                                            {player.player_uuid}
                                        </td>
                                        <td class="p-4 text-sm text-muted-foreground text-right">
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
