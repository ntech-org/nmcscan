<script lang="ts">
    import { onMount } from "svelte";
    import { fetchWithAuth } from "$lib/state.svelte";
    import * as Card from "$lib/components/ui/card";
    import * as Table from "$lib/components/ui/table";
    import { Button } from "$lib/components/ui/button";
    import { Input } from "$lib/components/ui/input";
    import { Badge } from "$lib/components/ui/badge";
    import ShieldAlert from "@lucide/svelte/icons/shield-alert";
    import RefreshCcw from "@lucide/svelte/icons/refresh-ccw";
    import Plus from "@lucide/svelte/icons/plus";
    import Trash2 from "@lucide/svelte/icons/trash-2";
    import ShieldX from "@lucide/svelte/icons/shield-x";

    interface ExcludeEntry {
        network: string;
        comment: string | null;
    }

    let exclusions = $state<ExcludeEntry[]>([]);
    let total = $state(0);
    let page = $state(0);
    let limit = $state(50);
    let newExcludeNetwork = $state("");
    let newExcludeComment = $state("");
    let excludeSubmitting = $state(false);
    let loading = $state(true);
    let error = $state<string | null>(null);

    async function loadExclusions() {
        loading = true;
        try {
            const res = await fetchWithAuth(
                `/api/exclude?page=${page}&limit=${limit}`,
            );
            const data = await res.json();
            exclusions = data.items;
            total = data.total;
        } catch (e) {
            error =
                e instanceof Error ? e.message : "Failed to load exclusions";
        } finally {
            loading = false;
        }
    }

    function nextPage() {
        if ((page + 1) * limit < total) {
            page += 1;
            loadExclusions();
        }
    }

    function prevPage() {
        if (page > 0) {
            page -= 1;
            loadExclusions();
        }
    }

    async function addExclusion() {
        if (!newExcludeNetwork) return;
        excludeSubmitting = true;
        error = null;
        try {
            await fetchWithAuth("/api/exclude", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    network: newExcludeNetwork,
                    comment: newExcludeComment || null,
                }),
            });
            newExcludeNetwork = "";
            newExcludeComment = "";
            page = 0; // Go to first page to see the new exclusion
            await loadExclusions();
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to add exclusion";
        } finally {
            excludeSubmitting = false;
        }
    }

    onMount(() => {
        loadExclusions();
    });
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <div class="space-y-1">
            <h2
                class="text-3xl font-bold tracking-tight italic flex items-center gap-3"
            >
                <ShieldAlert class="text-destructive" />
                Safety Exclusions
            </h2>
            <p class="text-muted-foreground italic text-sm">
                Restrict scanner access to specific networks or critical
                infrastructure.
            </p>
        </div>
        <div class="flex items-center gap-2">
            <div
                class="flex items-center gap-1 bg-muted/30 px-3 py-1.5 rounded-full border border-muted text-xs font-mono italic mr-2 shadow-sm"
            >
                <span class="text-primary font-bold"
                    >{page * limit + 1}-{Math.min(
                        (page + 1) * limit,
                        total,
                    )}</span
                >
                <span class="text-muted-foreground">of</span>
                <span class="text-foreground font-bold"
                    >{total.toLocaleString()}</span
                >
            </div>
            <Button
                variant="outline"
                size="icon"
                onclick={loadExclusions}
                disabled={loading}
                class="rounded-full h-10 w-10 shadow-sm"
            >
                <RefreshCcw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
            </Button>
        </div>
    </div>

    {#if error}
        <div
            class="p-4 bg-destructive/10 border border-destructive/20 text-destructive rounded-xl italic text-sm"
        >
            {error}
        </div>
    {/if}

    <Card.Root class="bg-card shadow-lg border-muted overflow-hidden py-0!">
        <Card.Header class="p-6">
            <Card.Title>Add New Exclusion</Card.Title>
            <Card.Description class="italic"
                >The scanner will immediately stop targeting these ranges.</Card.Description
            >
            <form
                onsubmit={(e) => {
                    e.preventDefault();
                    addExclusion();
                }}
                class="flex flex-col md:flex-row gap-4 mt-4"
            >
                <div class="flex-1">
                    <Input
                        placeholder="IP Address or CIDR (e.g. 1.2.3.4/24)"
                        class="h-10 italic"
                        bind:value={newExcludeNetwork}
                        required
                    />
                </div>
                <div class="flex-1">
                    <Input
                        placeholder="Reason for exclusion..."
                        class="h-10 italic"
                        bind:value={newExcludeComment}
                    />
                </div>
                <Button
                    type="submit"
                    variant="destructive"
                    disabled={excludeSubmitting || !newExcludeNetwork}
                    class="h-10 gap-2 font-bold uppercase text-[10px] tracking-widest shadow-md"
                >
                    {#if excludeSubmitting}
                        <RefreshCcw class="h-3 w-3 animate-spin" />
                    {:else}
                        <Plus class="h-3 w-3" />
                    {/if}
                    Apply Block
                </Button>
            </form>
        </Card.Header>
        <Card.Content class="p-0 border-t">
            <Table.Root>
                <Table.Header>
                    <Table.Row
                        class="bg-muted/30 hover:bg-muted/30 uppercase tracking-widest text-[10px] font-bold"
                    >
                        <Table.Head>Network Range</Table.Head>
                        <Table.Head>Status</Table.Head>
                        <Table.Head>Comment / Origin</Table.Head>
                        <Table.Head class="text-right pr-6">Action</Table.Head>
                    </Table.Row>
                </Table.Header>
                <Table.Body>
                    {#if loading}
                        <Table.Row>
                            <Table.Cell
                                colspan={4}
                                class="h-24 text-center italic text-muted-foreground"
                            >
                                Syncing exclusion list from scanner...
                            </Table.Cell>
                        </Table.Row>
                    {:else if exclusions.length === 0}
                        <Table.Row>
                            <Table.Cell
                                colspan={4}
                                class="h-24 text-center italic text-muted-foreground"
                            >
                                No active network exclusions found.
                            </Table.Cell>
                        </Table.Row>
                    {:else}
                        {#each exclusions as entry}
                            <Table.Row class="group italic">
                                <Table.Cell
                                    class="font-mono text-sm font-semibold text-destructive"
                                >
                                    {entry.network}
                                </Table.Cell>
                                <Table.Cell>
                                    <Badge
                                        variant="destructive"
                                        class="text-[9px] px-1.5 py-0 uppercase font-bold tracking-tighter"
                                    >
                                        BLOCKED
                                    </Badge>
                                </Table.Cell>
                                <Table.Cell
                                    class="text-sm text-muted-foreground truncate max-w-md"
                                >
                                    {entry.comment || "No reason provided"}
                                </Table.Cell>
                                <Table.Cell class="text-right pr-6">
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        class="h-8 w-8 rounded-full text-muted-foreground/50 hover:text-destructive transition-all"
                                    >
                                        <ShieldX class="h-4 w-4" />
                                    </Button>
                                </Table.Cell>
                            </Table.Row>
                        {/each}
                    {/if}
                </Table.Body>
            </Table.Root>
        </Card.Content>
    </Card.Root>

    <div class="flex items-center justify-center gap-4">
        <Button
            variant="outline"
            size="sm"
            onclick={prevPage}
            disabled={page === 0 || loading}
        >
            Previous
        </Button>
        <div
            class="text-xs font-bold italic text-muted-foreground uppercase tracking-wider"
        >
            Page {page + 1}
        </div>
        <Button
            variant="outline"
            size="sm"
            onclick={nextPage}
            disabled={(page + 1) * limit >= total || loading}
        >
            Next
        </Button>
    </div>
</div>
