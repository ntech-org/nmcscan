<script lang="ts">
  import { onMount } from "svelte";
  import { signIn } from "@auth/sveltekit/client";
  import { fetchWithAuth } from "$lib/state.svelte";
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { Input } from "$lib/components/ui/input";
  import { Separator } from "$lib/components/ui/separator";
  import User from "@lucide/svelte/icons/user";
  import Link2 from "@lucide/svelte/icons/link-2";
  import Unlink from "@lucide/svelte/icons/unlink";
  import AlertCircle from "@lucide/svelte/icons/alert-circle";
  import Key from "@lucide/svelte/icons/key";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import { enhance } from "$app/forms";

  let { data } = $props();
  const session = $derived(data.session);
  const linkedAccounts = $derived(data.accounts);

  const availableProviders = [
    { id: 'discord', name: 'Discord', icon: 'i-simple-icons-discord' },
    { id: 'github', name: 'GitHub', icon: 'i-simple-icons-github' }
  ];

  function isLinked(providerId: string) {
    return linkedAccounts.some((a: any) => a.provider === providerId);
  }

  // API Key management
  let apiKeys = $state<Array<{id: number, name: string, key: string | null, created_at: string, last_used_at: string | null}>>([]);
  let newKeyName = $state("");
  let creatingKey = $state(false);
  let createdKey = $state<string | null>(null);
  let copied = $state(false);
  let deletingId = $state<number | null>(null);

  async function loadKeys() {
    try {
      const res = await fetchWithAuth("/api/keys");
      apiKeys = await res.json();
    } catch (e) {
      console.error("Failed to load API keys:", e);
    }
  }

  async function createKey() {
    if (!newKeyName.trim()) return;
    creatingKey = true;
    try {
      const res = await fetchWithAuth("/api/keys", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: newKeyName.trim() }),
      });
      const data = await res.json();
      createdKey = data.key;
      newKeyName = "";
      await loadKeys();
    } catch (e) {
      console.error("Failed to create API key:", e);
    } finally {
      creatingKey = false;
    }
  }

  function copyKey() {
    if (!createdKey) return;
    navigator.clipboard.writeText(createdKey);
    copied = true;
    setTimeout(() => copied = false, 2000);
  }

  async function revokeKey(id: number) {
    if (!confirm("Revoke this API key? This cannot be undone.")) return;
    deletingId = id;
    try {
      await fetchWithAuth(`/api/keys/${id}`, { method: "DELETE" });
      await loadKeys();
    } catch (e) {
      console.error("Failed to revoke API key:", e);
    } finally {
      deletingId = null;
    }
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString("en-US", {
      year: "numeric", month: "short", day: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
  }

  onMount(() => {
    loadKeys();
  });
</script>

<div class="max-w-4xl mx-auto space-y-8">
  <div class="space-y-1">
    <h2 class="text-3xl font-bold tracking-tight italic flex items-center gap-3">
      <User class="text-primary" />
      Account Settings
    </h2>
    <p class="text-muted-foreground italic text-sm">Manage your profile and connected authentication methods.</p>
  </div>

  <div class="grid gap-6 md:grid-cols-2">
    <Card.Root>
      <Card.Header>
        <Card.Title>Profile Information</Card.Title>
        <Card.Description class="italic">Your primary identity on NMCScan</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-4">
        <div class="flex items-center gap-4 p-4 rounded-lg bg-muted/30 border border-dashed">
          <img src={session?.user?.image} alt="" class="w-16 h-16 rounded-full border-2 border-primary/20 shadow-sm" />
          <div class="flex-1">
            <h3 class="font-bold text-lg">{session?.user?.name}</h3>
            <p class="text-sm text-muted-foreground italic">{session?.user?.email}</p>
          </div>
        </div>
        <p class="text-[11px] text-muted-foreground italic leading-relaxed">
          Profile information is automatically synchronized from your primary authentication provider.
        </p>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title>Connected Accounts</Card.Title>
        <Card.Description class="italic">Link multiple providers to your account</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-4">
        <div class="space-y-3">
          {#each availableProviders as provider}
            <div class="flex items-center justify-between p-3 rounded-md border bg-card/50">
              <div class="flex items-center gap-3">
                <span class="{provider.icon} w-5 h-5 text-muted-foreground"></span>
                <span class="font-medium text-sm">{provider.name}</span>
                {#if isLinked(provider.id)}
                  <Badge variant="default" class="bg-emerald-500/10 text-emerald-500 border-emerald-500/20 text-[10px] uppercase font-bold py-0">Linked</Badge>
                {/if}
              </div>
              
              {#if isLinked(provider.id)}
                <form action="?/unlink" method="POST" use:enhance>
                  <input type="hidden" name="accountId" value={linkedAccounts.find((a: any) => a.provider === provider.id).id} />
                  <Button type="submit" variant="ghost" size="sm" class="h-8 text-destructive hover:text-destructive hover:bg-destructive/10 gap-2">
                    <Unlink size={14} />
                    Unlink
                  </Button>
                </form>
              {:else}
                <Button variant="outline" size="sm" class="h-8 gap-2 italic" onclick={() => signIn(provider.id)}>
                  <Link2 size={14} />
                  Connect
                </Button>
              {/if}
            </div>
          {/each}
        </div>

        <div class="p-3 rounded-md bg-orange-500/5 border border-orange-500/20 flex gap-3">
          <AlertCircle class="w-4 h-4 text-orange-500 shrink-0 mt-0.5" />
          <p class="text-[10px] text-orange-500 italic leading-normal">
            Unlinking an account will prevent you from using that method to log in. Ensure you have at least one active connection or you may lose access.
          </p>
        </div>
      </Card.Content>
    </Card.Root>
  </div>

  <Separator />

  <!-- API Keys Section -->
  <Card.Root>
    <Card.Header>
      <Card.Title class="flex items-center gap-2">
        <Key class="h-5 w-5 text-primary" />
        API Keys
      </Card.Title>
      <Card.Description class="italic">Generate keys for external tools and scripts</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-4">
      <!-- Create form -->
      <div class="flex gap-2">
        <Input placeholder="Key name (e.g., My CLI Tool)" bind:value={newKeyName} class="h-9" />
        <Button onclick={createKey} disabled={creatingKey || !newKeyName.trim()} class="h-9 gap-2 shrink-0">
          <Plus size={14} />
          Create
        </Button>
      </div>

      <!-- Created key display (shown once) -->
      {#if createdKey}
        <div class="p-3 rounded-md bg-emerald-500/5 border border-emerald-500/20 space-y-2">
          <div class="flex items-center gap-3">
            <code class="flex-1 text-xs font-mono break-all select-all">{createdKey}</code>
            <Button variant="ghost" size="icon" onclick={copyKey} class="h-8 w-8 shrink-0">
              {#if copied}
                <Check size={14} class="text-emerald-500" />
              {:else}
                <Copy size={14} />
              {/if}
            </Button>
          </div>
          <p class="text-[10px] text-orange-500 italic">Copy this key now. It will not be shown again.</p>
        </div>
      {/if}

      <!-- Keys table -->
      {#if apiKeys.length > 0}
        <Table.Root>
          <Table.Header>
            <Table.Row class="text-[10px] uppercase tracking-wider font-bold text-muted-foreground">
              <Table.Head>Name</Table.Head>
              <Table.Head>Created</Table.Head>
              <Table.Head>Last Used</Table.Head>
              <Table.Head class="w-16"></Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each apiKeys as key (key.id)}
              <Table.Row>
                <Table.Cell class="font-medium text-sm">{key.name}</Table.Cell>
                <Table.Cell class="text-xs text-muted-foreground">{formatDate(key.created_at)}</Table.Cell>
                <Table.Cell class="text-xs text-muted-foreground">
                  {key.last_used_at ? formatDate(key.last_used_at) : "Never"}
                </Table.Cell>
                <Table.Cell>
                  <Button
                    variant="ghost"
                    size="icon"
                    class="h-8 w-8 text-destructive hover:text-destructive hover:bg-destructive/10"
                    onclick={() => revokeKey(key.id)}
                    disabled={deletingId === key.id}
                  >
                    <Trash2 size={14} />
                  </Button>
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </Table.Root>
      {:else}
        <p class="text-sm text-muted-foreground italic text-center py-4">No API keys yet.</p>
      {/if}

      <div class="p-3 rounded-md bg-muted/30 border border-dashed flex gap-3">
        <Key class="w-4 h-4 text-muted-foreground shrink-0 mt-0.5" />
        <p class="text-[10px] text-muted-foreground italic leading-normal">
          Use API keys with the <code class="bg-muted px-1 rounded">X-API-Key</code> header to access the NMCScan REST API from external tools.
          See the API documentation for available endpoints.
        </p>
      </div>
    </Card.Content>
  </Card.Root>
</div>
