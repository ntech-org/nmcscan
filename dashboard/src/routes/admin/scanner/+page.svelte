<script lang="ts">
    import { onMount } from 'svelte';
    import { fetchWithAuth } from '$lib/state.svelte';
    import { Separator } from '$lib/components/ui/separator';
    import { Progress } from '$lib/components/ui/progress';
    import { Badge } from '$lib/components/ui/badge';

    interface ScanProgress {
        categories: {
            category: string;
            total_ranges: number;
            scanned_ranges: number;
            total_epochs: number;
            cycle_progress_pct: number;
            first_loop_pct: number;
        }[];
        queues: { hot: number; warm: number; cold: number; discovery: number };
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

    let scanProgress = $state<ScanProgress | null>(null);
    let loginStatus = $state<LoginQueueStatus | null>(null);
    let testScanning = $state(false);
    let testScanResult = $state<{ status: string; servers_added: number } | null>(null);
    let testScanRegion = $state('quick');
    let error = $state<string | null>(null);

    async function loadData() {
        try {
            const [progressRes, loginRes] = await Promise.all([
                fetchWithAuth('/api/scan/progress'),
                fetchWithAuth('/api/login-queue/status'),
            ]);
            scanProgress = await progressRes.json();
            loginStatus = await loginRes.json();
        } catch (e) {
            console.error("Data load error:", e);
        }
    }

    async function triggerTestScan() {
        testScanning = true;
        testScanResult = null;
        try {
            const payload: any = {};
            if (testScanRegion === 'quick') payload.quick = true;
            else if (testScanRegion !== 'default') payload.region = testScanRegion;

            const res = await fetchWithAuth('/api/scan/test', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload)
            });
            testScanResult = await res.json();
            setTimeout(() => loadData(), 2000);
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to trigger test scan';
        } finally {
            testScanning = false;
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

<!-- Scanner Control -->
<div class="space-y-6">
    <div class="space-y-3">
        <h2 class="text-sm font-semibold tracking-tight">Test Scan Dispatcher</h2>
        <p class="text-xs text-muted-foreground">Deploy a test scan to known Minecraft servers. Used for validating scanner functionality.</p>
        <div class="flex items-center gap-3 flex-wrap">
            <select
                class="h-9 px-3 bg-background border border-input rounded-md text-sm focus:ring-1 focus:ring-ring outline-none"
                bind:value={testScanRegion}
            >
                <option value="quick">Quick Test (10 servers)</option>
                <option value="default">All Known (50 servers)</option>
                <option value="us">US Servers</option>
                <option value="eu">EU Servers</option>
            </select>
            <button
                class="inline-flex items-center gap-2 h-9 px-4 bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 rounded-md text-sm font-medium transition-colors"
                onclick={triggerTestScan}
                disabled={testScanning}
            >
                {#if testScanning}
                    <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
                    Deploying...
                {:else}
                    Execute Scan
                {/if}
            </button>
            {#if testScanResult}
                <span class="text-xs text-emerald-500">
                    Dispatched {testScanResult.servers_added} servers
                </span>
            {/if}
            {#if error}
                <span class="text-xs text-destructive">{error}</span>
            {/if}
        </div>
    </div>

    <Separator />

    <!-- Live Queue Sizes -->
    <div class="space-y-3">
        <h2 class="text-sm font-semibold tracking-tight">Live Queue Sizes</h2>
        {#if scanProgress?.queues}
            <div class="grid grid-cols-2 md:grid-cols-4 gap-6 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Hot</span>
                    <span class="font-mono">{formatNum(scanProgress.queues.hot)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Warm</span>
                    <span class="font-mono">{formatNum(scanProgress.queues.warm)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Cold</span>
                    <span class="font-mono">{formatNum(scanProgress.queues.cold)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Discovery</span>
                    <span class="font-mono">{formatNum(scanProgress.queues.discovery)}</span>
                </div>
            </div>
        {:else}
            <p class="text-xs text-muted-foreground">Loading...</p>
        {/if}
    </div>

    <Separator />

    <!-- Login Queue Status -->
    <div class="space-y-3">
        <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold tracking-tight">Login Queue</h2>
            {#if loginStatus?.running}
                <Badge variant="outline" class="text-[10px] gap-1 px-1.5 py-0">
                    <span class="w-1 h-1 rounded-full bg-emerald-500 animate-pulse"></span>
                    Active
                </Badge>
            {:else}
                <Badge variant="outline" class="text-[10px] gap-1 px-1.5 py-0">
                    <span class="w-1 h-1 rounded-full bg-muted-foreground/50"></span>
                    Inactive
                </Badge>
            {/if}
        </div>
        {#if loginStatus}
            <div class="grid grid-cols-2 md:grid-cols-4 gap-6 text-sm">
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Cracked</span>
                    <span class="font-mono text-emerald-500">{formatNum(loginStatus.success)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Premium</span>
                    <span class="font-mono">{formatNum(loginStatus.premium)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Whitelisted</span>
                    <span class="font-mono">{formatNum(loginStatus.whitelist)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Banned</span>
                    <span class="font-mono">{formatNum(loginStatus.banned)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Total Attempts</span>
                    <span class="font-mono">{formatNum(loginStatus.total_attempts)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Rejected</span>
                    <span class="font-mono">{formatNum(loginStatus.rejected)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Timeout</span>
                    <span class="font-mono">{formatNum(loginStatus.timeout)}</span>
                </div>
                <div class="flex justify-between">
                    <span class="text-muted-foreground">Unreachable</span>
                    <span class="font-mono">{formatNum(loginStatus.unreachable)}</span>
                </div>
            </div>
        {:else}
            <p class="text-xs text-muted-foreground">Loading...</p>
        {/if}
    </div>

    <Separator />

    <!-- Scan Progress -->
    <div class="space-y-4">
        <h2 class="text-sm font-semibold tracking-tight">Scan Progress</h2>
        {#if scanProgress?.categories?.length}
            <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                {#each scanProgress.categories as cat}
                    {@const avgEpoch = cat.total_ranges > 0 ? Math.round(cat.total_epochs / cat.total_ranges) : 0}
                    <div class="space-y-2">
                        <div class="flex items-center justify-between text-xs gap-2">
                            <span class="capitalize text-muted-foreground">{cat.category}</span>
                            <span class="font-mono tabular-nums">Epoch #{avgEpoch} — {cat.cycle_progress_pct.toFixed(1)}%</span>
                        </div>
                        <Progress value={cat.cycle_progress_pct} max={100} class="h-1.5" />
                        <div class="text-[10px] text-muted-foreground">
                            {formatNum(cat.scanned_ranges)} / {formatNum(cat.total_ranges)} ranges
                        </div>
                    </div>
                {/each}
            </div>
        {:else}
            <p class="text-xs text-muted-foreground">Loading...</p>
        {/if}
    </div>
</div>
