<script lang="ts">
    import { onMount } from "svelte";
    import { fetchWithAuth } from "$lib/state.svelte";
    import { Separator } from "$lib/components/ui/separator";
    import { Badge } from "$lib/components/ui/badge";

    interface Stats {
        total_servers: number;
        online_servers: number;
        total_players: number;
        asn_hosting: number;
        asn_residential: number;
        asn_unknown: number;
    }

    interface CategoryProgress {
        category: string;
        total_ranges: number;
        scanned_ranges: number;
        total_epochs: number;
        cycle_progress_pct: number;
    }

    interface ScanProgress {
        categories: CategoryProgress[];
        queues: { ready: number; total: number; discovery: number };
    }

    interface LoginQueueStatus {
        running: boolean;
        total_attempts: number;
        success: number;
        premium: number;
    }

    let stats = $state<Stats | null>(null);
    let scanProgress = $state<ScanProgress | null>(null);
    let loginStatus = $state<LoginQueueStatus | null>(null);
    let error = $state<string | null>(null);

    async function loadData() {
        try {
            const [statsRes, progressRes, loginRes] = await Promise.all([
                fetchWithAuth("/api/stats"),
                fetchWithAuth("/api/scan/progress"),
                fetchWithAuth("/api/login-queue/status"),
            ]);
            stats = await statsRes.json();
            scanProgress = await progressRes.json();
            loginStatus = await loginRes.json();
        } catch (e) {
            console.error("Data load error:", e);
        }
    }

    function formatNum(n: number | undefined | null): string {
        return (n ?? 0).toLocaleString();
    }

    let refreshInterval: ReturnType<typeof setInterval>;
    onMount(() => {
        loadData();
        refreshInterval = setInterval(loadData, 30000);
        return () => clearInterval(refreshInterval);
    });
</script>

<!-- Hero Stats -->
<div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-6">
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(stats?.total_servers)}
        </div>
        <div class="text-xs text-muted-foreground">Discovered</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight text-emerald-500">
            {formatNum(stats?.online_servers)}
        </div>
        <div class="text-xs text-muted-foreground">Online</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(stats?.total_players)}
        </div>
        <div class="text-xs text-muted-foreground">Players</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(stats?.asn_hosting)}
        </div>
        <div class="text-xs text-muted-foreground">Hosting ASNs</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(stats?.asn_residential)}
        </div>
        <div class="text-xs text-muted-foreground">Residential</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(scanProgress?.queues?.ready)}
        </div>
        <div class="text-xs text-muted-foreground">Ready Queue</div>
    </div>
</div>

<Separator />

<!-- Simplified Discovery Progress -->
<div class="space-y-4">
    <h2 class="text-sm font-semibold tracking-tight">Discovery Progress</h2>
    {#if scanProgress?.categories?.length}
        <div class="flex gap-6">
            {#each scanProgress.categories as cat}
                {@const avgEpoch = cat.total_ranges > 0 ? Math.round(cat.total_epochs / cat.total_ranges) : 0}
                <div class="flex-1 space-y-2">
                    <div class="flex items-center justify-between text-sm">
                        <span class="capitalize text-muted-foreground">{cat.category}</span>
                        <span class="font-mono tabular-nums">
                            {cat.cycle_progress_pct.toFixed(0)}% (Epoch {avgEpoch})
                        </span>
                    </div>
                    <div class="h-2 bg-muted rounded-full overflow-hidden">
                        <div 
                            class="h-full bg-amber-500 transition-all duration-500" 
                            style="width: {cat.cycle_progress_pct}%"
                        ></div>
                    </div>
                </div>
            {/each}
        </div>
    {:else}
        <p class="text-xs text-muted-foreground">Loading...</p>
    {/if}
</div>

<Separator />

<!-- Simplified Queues + Login -->
<div class="grid grid-cols-1 md:grid-cols-2 gap-8">
    <!-- Queue -->
    <div class="space-y-2">
        <h2 class="text-sm font-semibold tracking-tight">Queues</h2>
        {#if scanProgress?.queues}
            <div class="flex gap-6 text-sm">
                <div>
                    <span class="text-muted-foreground">Ready</span>
                    <span class="ml-2 font-mono">{formatNum(scanProgress.queues.ready)}</span>
                </div>
                <div>
                    <span class="text-muted-foreground">Total</span>
                    <span class="ml-2 font-mono">{formatNum(scanProgress.queues.total)}</span>
                </div>
            </div>
        {/if}
    </div>

    <!-- Login Queue -->
    <div class="space-y-2">
        <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold tracking-tight">Login Queue</h2>
            {#if loginStatus?.running}
                <Badge variant="outline" class="text-[10px] gap-1 px-1.5 py-0">
                    <span class="w-1 h-1 rounded-full bg-emerald-500 animate-pulse"></span>
                    Active
                </Badge>
            {/if}
        </div>
        {#if loginStatus}
            <div class="flex gap-6 text-sm">
                <div>
                    <span class="text-muted-foreground">Cracked</span>
                    <span class="ml-2 font-mono text-emerald-500">{formatNum(loginStatus.success)}</span>
                </div>
                <div>
                    <span class="text-muted-foreground">Premium</span>
                    <span class="ml-2 font-mono">{formatNum(loginStatus.premium)}</span>
                </div>
                <div>
                    <span class="text-muted-foreground">Total</span>
                    <span class="ml-2 font-mono">{formatNum(loginStatus.total_attempts)}</span>
                </div>
            </div>
        {:else}
            <p class="text-xs text-muted-foreground">Loading...</p>
        {/if}
    </div>
</div>