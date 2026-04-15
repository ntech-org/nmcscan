<script lang="ts">
    import { onMount } from "svelte";
    import { fetchWithAuth } from "$lib/state.svelte";
    import { Separator } from "$lib/components/ui/separator";
    import { Progress } from "$lib/components/ui/progress";
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
        first_loop_pct: number;
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
        whitelist: number;
        banned: number;
        rejected: number;
        unreachable: number;
        timeout: number;
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
        <div class="text-xs text-muted-foreground">Discovered Servers</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight text-emerald-500">
            {formatNum(stats?.online_servers)}
        </div>
        <div class="text-xs text-muted-foreground">Online Now</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(stats?.total_players)}
        </div>
        <div class="text-xs text-muted-foreground">Players Online</div>
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
        <div class="text-xs text-muted-foreground">Residential ASNs</div>
    </div>
    <div class="space-y-1">
        <div class="text-3xl font-bold tracking-tight">
            {formatNum(scanProgress?.queues?.discovery)}
        </div>
        <div class="text-xs text-muted-foreground">Discovery Queue</div>
    </div>
</div>

<Separator />

<!-- Scan Progress -->
<div class="space-y-4">
    <div class="flex items-center gap-2">
        <h2 class="text-sm font-semibold tracking-tight">
            Continuous Discovery Scan
        </h2>
        <div class="group relative">
            <svg
                class="w-3.5 h-3.5 text-muted-foreground cursor-help"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <circle cx="12" cy="12" r="10" />
                <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
                <circle cx="12" cy="17" r="0.5" fill="currentColor" />
            </svg>
            <div
                class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-72 p-3 bg-popover text-popover-foreground rounded-lg shadow-lg border text-xs opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none group-hover:pointer-events-auto z-50"
            >
                Discovery scanning is continuous and cyclical. Each category
                scans all IP ranges in a cycle, then resets and starts the next
                epoch with a fresh shuffle. Progress shows completion of the <strong
                    >current cycle</strong
                >, not overall scanning — it will reset periodically.
            </div>
        </div>
    </div>
    {#if scanProgress?.categories?.length}
        <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
            {#each scanProgress.categories as cat}
                {@const avgEpoch =
                    cat.total_ranges > 0
                        ? Math.round(cat.total_epochs / cat.total_ranges)
                        : 0}
                {@const cyclingSoon = cat.cycle_progress_pct >= 90}
                <div class="space-y-2">
                    <div
                        class="flex items-center justify-between text-xs gap-2"
                    >
                        <span class="capitalize text-muted-foreground"
                            >{cat.category}</span
                        >
                        <div class="flex items-center gap-1.5">
                            {#if cyclingSoon}
                                <span
                                    class="text-[9px] px-1.5 py-0 rounded-full bg-amber-500/10 text-amber-500 border border-amber-500/20 font-medium"
                                >
                                    Epoch #{avgEpoch + 1} cycling soon
                                </span>
                            {:else}
                                <span class="text-muted-foreground font-mono"
                                    >Epoch #{avgEpoch}</span
                                >
                            {/if}
                            <span class="font-mono tabular-nums"
                                >Cycle: {cat.cycle_progress_pct.toFixed(
                                    1,
                                )}%</span
                            >
                        </div>
                    </div>
                    <Progress
                        value={cat.cycle_progress_pct}
                        max={100}
                        class="h-1.5"
                    />
                    <div class="text-[10px] text-muted-foreground">
                        {formatNum(cat.scanned_ranges)} / {formatNum(
                            cat.total_ranges,
                        )} ranges scanned this cycle
                    </div>
                    <div class="space-y-1.5 mt-2">
                        <div
                            class="flex items-center justify-between text-[10px]"
                        >
                            <span class="text-muted-foreground"
                                >First loop completion</span
                            >
                            <span
                                class="font-mono tabular-nums text-emerald-500"
                                >{cat.first_loop_pct.toFixed(1)}%</span
                            >
                        </div>
                        <Progress
                            value={cat.first_loop_pct}
                            max={100}
                            class="h-1"
                        />
                        <div class="text-[9px] text-muted-foreground">
                            Ranges ever scanned at least once (this resets on
                            ASN database changes)
                        </div>
                    </div>
                </div>
            {/each}
        </div>
    {:else}
        <p class="text-xs text-muted-foreground">Loading...</p>
    {/if}
</div>

<Separator />

<!-- Queues + Login Queue -->
<div class="grid grid-cols-1 md:grid-cols-2 gap-8">
    <!-- Queue sizes -->
    <div class="space-y-3">
        <h2 class="text-sm font-semibold tracking-tight">Queue Sizes</h2>
        {#if scanProgress?.queues}
            <div class="grid grid-cols-3 gap-x-6 gap-y-2 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Ready</span>
                    <span class="font-mono"
                        >{formatNum(scanProgress.queues.ready)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Total</span>
                    <span class="font-mono"
                        >{formatNum(scanProgress.queues.total)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Discovery</span>
                    <span class="font-mono"
                        >{formatNum(scanProgress.queues.discovery)}</span
                    >
                </div>
            </div>
        {/if}
    </div>

    <!-- Login queue -->
    <div class="space-y-3">
        <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold tracking-tight">Login Queue</h2>
            {#if loginStatus?.running}
                <Badge variant="outline" class="text-[10px] gap-1 px-1.5 py-0">
                    <span
                        class="w-1 h-1 rounded-full bg-emerald-500 animate-pulse"
                    ></span>
                    Active
                </Badge>
            {/if}
        </div>
        {#if loginStatus}
            <div class="grid grid-cols-2 gap-x-6 gap-y-2 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Cracked</span>
                    <span class="font-mono text-emerald-500"
                        >{formatNum(loginStatus.success)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Premium</span>
                    <span class="font-mono"
                        >{formatNum(loginStatus.premium)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Whitelisted</span>
                    <span class="font-mono"
                        >{formatNum(loginStatus.whitelist)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Banned</span>
                    <span class="font-mono"
                        >{formatNum(loginStatus.banned)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Total</span>
                    <span class="font-mono"
                        >{formatNum(loginStatus.total_attempts)}</span
                    >
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Unreachable</span>
                    <span class="font-mono"
                        >{formatNum(loginStatus.unreachable)}</span
                    >
                </div>
            </div>
        {:else}
            <p class="text-xs text-muted-foreground">Loading...</p>
        {/if}
    </div>
</div>
