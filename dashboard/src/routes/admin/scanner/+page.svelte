<script lang="ts">
    import { onMount } from "svelte";
    import { fetchWithAuth } from "$lib/state.svelte";
    import { Separator } from "$lib/components/ui/separator";
    import { Badge } from "$lib/components/ui/badge";
    import { Button } from "$lib/components/ui/button";

    interface ScanProgress {
        categories: {
            category: string;
            total_ranges: number;
            scanned_ranges: number;
            total_epochs: number;
            cycle_progress_pct: number;
            first_loop_pct: number;
        }[];
        queues: { ready: number; discovery: number };
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
    let testScanResult = $state<{
        status: string;
        servers_added: number;
    } | null>(null);
    let testScanRegion = $state("quick");
    let error = $state<string | null>(null);

    // Reset state
    let resetting = $state(false);
    let resetSuccess = $state(false);
    let resetError = $state<string | null>(null);

    async function loadData() {
        try {
            const [progressRes, loginRes] = await Promise.all([
                fetchWithAuth("/api/scan/progress"),
                fetchWithAuth("/api/login-queue/status"),
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
            if (testScanRegion === "quick") payload.quick = true;
            else if (testScanRegion !== "default") payload.region = testScanRegion;

            const res = await fetchWithAuth("/api/scan/test", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload),
            });
            testScanResult = await res.json();
            setTimeout(() => loadData(), 2000);
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to trigger test scan";
        } finally {
            testScanning = false;
        }
    }

    async function resetProgress() {
        resetting = true;
        resetSuccess = false;
        resetError = null;
        try {
            const res = await fetchWithAuth("/api/scan/reset-progress", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ reset_failures: false }),
            });
            if (res.ok) {
                const data = await res.json();
                resetSuccess = true;
                setTimeout(() => loadData(), 2000);
            } else {
                resetError = "Failed to reset progress";
            }
        } catch (e) {
            resetError = e instanceof Error ? e.message : "Failed to reset progress";
        } finally {
            resetting = false;
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
    <!-- Top Actions Row -->
    <div class="flex items-center justify-between">
        <div class="space-y-1">
            <h2 class="text-sm font-semibold tracking-tight">
                Test Scan Dispatcher
            </h2>
            <p class="text-xs text-muted-foreground">
                Deploy a test scan to validate scanner functionality.
            </p>
        </div>
        <div class="flex items-center gap-3">
            <select
                class="h-9 px-3 bg-background border border-input rounded-md text-sm focus:ring-1 focus:ring-ring outline-none"
                bind:value={testScanRegion}
            >
                <option value="quick">Quick (10)</option>
                <option value="default">Default (50)</option>
                <option value="us">US</option>
                <option value="eu">EU</option>
            </select>
            <Button onclick={triggerTestScan} disabled={testScanning} size="sm">
                {#if testScanning}
                    Deploying...
                {:else}
                    Execute Scan
                {/if}
            </Button>
        </div>
    </div>

    {#if testScanResult}
        <p class="text-xs text-emerald-500">Dispatched {testScanResult.servers_added} servers</p>
    {/if}
    {#if error}
        <p class="text-xs text-destructive">{error}</p>
    {/if}

    <Separator />

    <!-- Simplified Scan Progress -->
    <div class="space-y-4">
        <div class="flex items-center justify-between">
            <h2 class="text-sm font-semibold tracking-tight">Discovery Progress</h2>
            <Button variant="outline" size="sm" onclick={resetProgress} disabled={resetting}>
                {#if resetting}
                    Resetting...
                {:else}
                    Reset Progress
                {/if}
            </Button>
        </div>
        
        {#if resetSuccess}
            <p class="text-xs text-emerald-500">Progress reset successfully</p>
        {/if}
        {#if resetError}
            <p class="text-xs text-destructive">{resetError}</p>
        {/if}

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

    <!-- Simplified Login Queue -->
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
            <div class="flex gap-8 text-sm">
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

    <Separator />

    <!-- Queue Status -->
    <div class="space-y-2">
        <h2 class="text-sm font-semibold tracking-tight">Queue Status</h2>
        {#if scanProgress?.queues}
            <div class="flex gap-6 text-sm">
                <div>
                    <span class="text-muted-foreground">Ready</span>
                    <span class="ml-2 font-mono">{formatNum(scanProgress.queues.ready)}</span>
                </div>
                <div>
                    <span class="text-muted-foreground">Discovery</span>
                    <span class="ml-2 font-mono">{formatNum(scanProgress.queues.discovery)}</span>
                </div>
            </div>
        {:else}
            <p class="text-xs text-muted-foreground">Loading...</p>
        {/if}
    </div>
</div>