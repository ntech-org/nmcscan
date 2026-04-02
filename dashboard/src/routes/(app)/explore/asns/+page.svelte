<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchWithAuth } from '$lib/state.svelte';
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import Network from "@lucide/svelte/icons/network";
  import RefreshCcw from "@lucide/svelte/icons/refresh-ccw";
  import Globe2 from "@lucide/svelte/icons/globe-2";
  import ExternalLink from "@lucide/svelte/icons/external-link";

  interface Asn {
    asn: string;
    org: string;
    category: string;
    country: string | null;
    server_count: number;
    tags: string[];
  }

  let asns = $state<Asn[]>([]);
  let total = $state(0);
  let page = $state(0);
  let limit = $state(50);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function loadAsns() {
    loading = true;
    try {
      const res = await fetchWithAuth(`/api/asns?page=${page}&limit=${limit}`);
      const data = await res.json();
      asns = data.items;
      total = data.total;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load ASNs';
    } finally {
      loading = false;
    }
  }

  function nextPage() {
    if ((page + 1) * limit < total) {
      page += 1;
      loadAsns();
    }
  }

  function prevPage() {
    if (page > 0) {
      page -= 1;
      loadAsns();
    }
  }

  onMount(() => {
    loadAsns();
  });
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h2 class="text-3xl font-bold tracking-tight italic flex items-center gap-3">
        <Network class="text-primary" />
        Network Topology
      </h2>
      <p class="text-muted-foreground italic text-sm">Explore the infrastructure of discovered Minecraft networks.</p>
    </div>
    <div class="flex items-center gap-2">
      <div class="flex items-center gap-1 bg-muted/30 px-3 py-1.5 rounded-full border border-muted text-xs font-mono italic mr-2 shadow-sm">
        <span class="text-primary font-bold">{page * limit + 1}-{Math.min((page + 1) * limit, total)}</span>
        <span class="text-muted-foreground">of</span>
        <span class="text-foreground font-bold">{total.toLocaleString()}</span>
      </div>
      <Button 
        variant="outline" 
        size="icon" 
        onclick={loadAsns} 
        disabled={loading}
        class="rounded-full h-10 w-10 shadow-sm"
      >
        <RefreshCcw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
      </Button>
    </div>
  </div>

  {#if error}
    <div class="p-4 bg-destructive/10 border border-destructive/20 text-destructive rounded-xl italic text-sm">
      {error}
    </div>
  {/if}

  <Card.Root class="bg-card shadow-lg border-muted overflow-hidden !py-0">
    <Card.Content class="p-0">
      <Table.Root>
        <Table.Header>
          <Table.Row class="bg-muted/30 hover:bg-muted/30 uppercase tracking-widest text-[10px] font-bold">
            <Table.Head>ASN</Table.Head>
            <Table.Head>Organization</Table.Head>
            <Table.Head>Classification</Table.Head>
            <Table.Head class="text-center">Servers</Table.Head>
            <Table.Head class="text-right pr-6">Region</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#if loading}
            <Table.Row>
              <Table.Cell colspan={5} class="h-24 text-center italic text-muted-foreground">
                Mapping network infrastructure...
              </Table.Cell>
            </Table.Row>
          {:else if asns.length === 0}
            <Table.Row>
              <Table.Cell colspan={5} class="h-24 text-center italic text-muted-foreground">
                No network data available.
              </Table.Cell>
            </Table.Row>
          {/if}
          
          {#each asns as asn}
            <Table.Row class="group">
              <Table.Cell class="font-mono text-sm font-semibold text-primary">
                <a href={`/explore/servers?asn=${asn.asn}`} class="hover:underline">
                  {asn.asn}
                </a>
              </Table.Cell>
              <Table.Cell>
                <div class="flex flex-col gap-1">
                  <span class="font-bold text-sm tracking-tight">{asn.org}</span>
                  <div class="flex flex-wrap gap-1">
                    {#each asn.tags as tag}
                      <Badge variant="outline" class="text-[9px] px-1.5 py-0 uppercase font-bold tracking-tighter italic">
                        {tag}
                      </Badge>
                    {/each}
                  </div>
                </div>
              </Table.Cell>
              <Table.Cell>
                <Badge variant={asn.category === 'Hosting' ? 'default' : 'outline'} class="text-[10px] uppercase font-bold py-0 italic">
                  {asn.category}
                </Badge>
              </Table.Cell>
              <Table.Cell class="text-center font-mono text-sm">
                {asn.server_count.toLocaleString()}
              </Table.Cell>
              <Table.Cell class="text-right pr-6">
                {#if asn.country}
                  <div class="flex items-center justify-end gap-2">
                    <span class="text-xs font-bold uppercase text-muted-foreground">{asn.country}</span>
                    <img 
                      src={`https://flagcdn.com/24x18/${asn.country.toLowerCase()}.png`} 
                      alt={asn.country} 
                      class="rounded shadow-sm opacity-80" 
                    />
                  </div>
                {:else}
                  <Globe2 class="w-4 h-4 ml-auto text-muted-foreground/30" />
                {/if}
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </Table.Root>
    </Card.Content>
  </Card.Root>

  <div class="flex items-center justify-center gap-4">
    <Button variant="outline" size="sm" onclick={prevPage} disabled={page === 0 || loading}>
      Previous
    </Button>
    <div class="text-xs font-bold italic text-muted-foreground uppercase tracking-wider">
      Page {page + 1}
    </div>
    <Button variant="outline" size="sm" onclick={nextPage} disabled={(page + 1) * limit >= total || loading}>
      Next
    </Button>
  </div>
</div>
