<script lang="ts">
  import { signIn } from "@auth/sveltekit/client";
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import User from "@lucide/svelte/icons/user";
  import Link2 from "@lucide/svelte/icons/link-2";
  import Unlink from "@lucide/svelte/icons/unlink";
  import AlertCircle from "@lucide/svelte/icons/alert-circle";
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
</div>
