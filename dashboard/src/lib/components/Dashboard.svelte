<script lang="ts">
    import { onMount } from "svelte";
    import { browser } from "$app/environment";
    import { env } from "$env/dynamic/public";
    import * as Card from "$lib/components/ui/card";
    import { Badge } from "$lib/components/ui/badge";
    import { Progress } from "$lib/components/ui/progress"; // Need to add this
    import Users from "@lucide/svelte/icons/users";
    import Server from "@lucide/svelte/icons/server";
    import Globe from "@lucide/svelte/icons/globe";
    import Zap from "@lucide/svelte/icons/zap";
    import ShieldCheck from "@lucide/svelte/icons/shield-check";
    import TrendingUp from "@lucide/svelte/icons/trending-up";
    import Activity from "@lucide/svelte/icons/activity";
    import Database from "@lucide/svelte/icons/database";

    const API_BASE =
        env.PUBLIC_API_URL ||
        (browser ? window.location.origin : "http://localhost:3000");

    let stats = $state({
        total_servers: 0,
        online_servers: 0,
        total_players: 0,
        asn_hosting: 0,
        asn_residential: 0,
        asn_unknown: 0,
    });

    let loading = $state(true);

    onMount(async () => {
        try {
            const res = await fetch(`${API_BASE}/api/stats`);
            if (res.ok) {
                stats = await res.json();
            }
        } catch (e) {
            console.error("Failed to fetch stats", e);
        } finally {
            loading = false;
        }
    });

    const onlinePercentage = $derived(
        stats.total_servers > 0
            ? (stats.online_servers / stats.total_servers) * 100
            : 0,
    );
</script>

<div class="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
    <Card.Root class="py-0">
        <Card.Header
            class="flex flex-row items-center justify-between space-y-0 pb-2"
        >
            <Card.Title class="text-sm font-medium italic"
                >Total Servers</Card.Title
            >
            <Database class="h-4 w-4 text-muted-foreground" />
        </Card.Header>
        <Card.Content>
            <div class="text-2xl font-bold">
                {stats.total_servers.toLocaleString()}
            </div>
            <p class="text-xs text-muted-foreground italic">
                Indexed in database
            </p>
        </Card.Content>
    </Card.Root>

    <Card.Root class="py-0">
        <Card.Header
            class="flex flex-row items-center justify-between space-y-0 pb-2"
        >
            <Card.Title class="text-sm font-medium italic"
                >Currently Online</Card.Title
            >
            <Activity class="h-4 w-4 text-emerald-500" />
        </Card.Header>
        <Card.Content>
            <div class="text-2xl font-bold">
                {stats.online_servers.toLocaleString()}
            </div>
            <div class="flex items-center gap-2 mt-1">
                <div class="flex-1 h-1.5 bg-muted rounded-full overflow-hidden">
                    <div
                        class="h-full bg-emerald-500"
                        style="width: {onlinePercentage}%"
                    ></div>
                </div>
                <span class="text-[10px] text-muted-foreground"
                    >{onlinePercentage.toFixed(1)}%</span
                >
            </div>
        </Card.Content>
    </Card.Root>

    <Card.Root class="py-0">
        <Card.Header
            class="flex flex-row items-center justify-between space-y-0 pb-2"
        >
            <Card.Title class="text-sm font-medium italic"
                >Players Online</Card.Title
            >
            <Users class="h-4 w-4 text-blue-500" />
        </Card.Header>
        <Card.Content>
            <div class="text-2xl font-bold">
                {stats.total_players.toLocaleString()}
            </div>
            <p class="text-xs text-muted-foreground italic">
                Aggregated player count
            </p>
        </Card.Content>
    </Card.Root>

    <Card.Root class="py-0">
        <Card.Header
            class="flex flex-row items-center justify-between space-y-0 pb-2"
        >
            <Card.Title class="text-sm font-medium italic"
                >Hosting Providers</Card.Title
            >
            <Globe class="h-4 w-4 text-orange-500" />
        </Card.Header>
        <Card.Content>
            <div class="text-2xl font-bold">
                {stats.asn_hosting.toLocaleString()}
            </div>
            <p class="text-xs text-muted-foreground italic">Verified ASNs</p>
        </Card.Content>
    </Card.Root>
</div>

<div class="grid gap-4 md:grid-cols-2 lg:grid-cols-7">
    <Card.Root class="col-span-4">
        <Card.Header>
            <Card.Title>Scanning Overview</Card.Title>
            <Card.Description class="italic"
                >Real-time statistics of the scanning engine</Card.Description
            >
        </Card.Header>
        <Card.Content>
            <div
                class="h-[300px] flex items-center justify-center border-2 border-dashed rounded-lg bg-muted/10 italic text-muted-foreground"
            >
                Chart.js Integration Pending
            </div>
        </Card.Content>
    </Card.Root>

    <Card.Root class="col-span-3">
        <Card.Header>
            <Card.Title>Network Distribution</Card.Title>
            <Card.Description class="italic"
                >Provider vs Residential</Card.Description
            >
        </Card.Header>
        <Card.Content class="space-y-4">
            <div class="space-y-2">
                <div class="flex items-center justify-between text-sm italic">
                    <span>Hosting (Cloud/Data Centers)</span>
                    <span class="font-medium">{stats.asn_hosting}</span>
                </div>
                <div class="h-2 bg-muted rounded-full overflow-hidden">
                    <div
                        class="h-full bg-blue-500"
                        style="width: {(stats.asn_hosting /
                            (stats.asn_hosting +
                                stats.asn_residential +
                                stats.asn_unknown)) *
                            100 || 0}%"
                    ></div>
                </div>
            </div>
            <div class="space-y-2">
                <div class="flex items-center justify-between text-sm italic">
                    <span>Residential / ISP</span>
                    <span class="font-medium">{stats.asn_residential}</span>
                </div>
                <div class="h-2 bg-muted rounded-full overflow-hidden">
                    <div
                        class="h-full bg-orange-500"
                        style="width: {(stats.asn_residential /
                            (stats.asn_hosting +
                                stats.asn_residential +
                                stats.asn_unknown)) *
                            100 || 0}%"
                    ></div>
                </div>
            </div>
            <div class="space-y-2">
                <div class="flex items-center justify-between text-sm italic">
                    <span>Unknown / Other</span>
                    <span class="font-medium">{stats.asn_unknown}</span>
                </div>
                <div class="h-2 bg-muted rounded-full overflow-hidden">
                    <div
                        class="h-full bg-gray-400"
                        style="width: {(stats.asn_unknown /
                            (stats.asn_hosting +
                                stats.asn_residential +
                                stats.asn_unknown)) *
                            100 || 0}%"
                    ></div>
                </div>
            </div>

            <div
                class="pt-4 border-t italic text-[11px] text-muted-foreground leading-relaxed"
            >
                Our scanner prioritizes <strong>Hosting ASNs</strong> for high-frequency
                updates, while Residential ranges are scanned monthly to identify
                home-hosted community servers.
            </div>
        </Card.Content>
    </Card.Root>
</div>
