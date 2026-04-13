<script lang="ts">
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import * as Avatar from "$lib/components/ui/avatar";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import UserPlus from "@lucide/svelte/icons/user-plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import UserMinus from "@lucide/svelte/icons/user-minus";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import { enhance } from "$app/forms";

  let { data } = $props();
  const users = $derived(data.users);
  const currentUserId = $derived((data.session?.user as any)?.id);

  function getRoleBadge(role: string) {
    switch (role) {
      case 'admin': return { label: 'Admin', variant: 'default' };
      case 'user': return { label: 'Member', variant: 'secondary' };
      case 'blocked': return { label: 'Blocked', variant: 'destructive' };
      default: return { label: 'Unknown', variant: 'outline' };
    }
  }
</script>

<div class="space-y-6">
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h2 class="text-3xl font-bold tracking-tight italic flex items-center gap-3">
        <ShieldCheck class="text-primary" />
        User Management
      </h2>
      <p class="text-muted-foreground italic text-sm">Review, authorize, and manage administrative permissions.</p>
    </div>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>Administrative Control</Card.Title>
      <Card.Description class="italic">Control access to scanner operations and sensitive data.</Card.Description>
    </Card.Header>
    <Card.Content class="p-0">
      <Table.Root>
        <Table.Header>
          <Table.Row class="bg-muted/30 hover:bg-muted/30 uppercase tracking-widest text-[10px] font-bold">
            <Table.Head class="w-16"></Table.Head>
            <Table.Head>User Identity</Table.Head>
            <Table.Head>Role</Table.Head>
            <Table.Head class="text-right pr-6">Manage</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each users as user}
            <Table.Row class="group">
              <Table.Cell>
                <Avatar.Root class="w-8 h-8">
                  <Avatar.Image src={user.image} alt={user.name} />
                  <Avatar.Fallback>{user.name?.charAt(0) || 'U'}</Avatar.Fallback>
                </Avatar.Root>
              </Table.Cell>
              <Table.Cell>
                <div class="flex flex-col">
                  <span class="font-bold text-sm">{user.name}</span>
                  <span class="text-xs text-muted-foreground italic font-mono">{user.email}</span>
                </div>
              </Table.Cell>
              <Table.Cell>
                {@const badge = getRoleBadge(user.role)}
                <Badge variant={badge.variant as any} class="text-[10px] uppercase font-bold px-2 py-0">
                  {badge.label}
                </Badge>
              </Table.Cell>
              <Table.Cell class="text-right pr-4">
                {#if user.id !== currentUserId}
                  <div class="flex items-center justify-end gap-2">
                    <form action="?/updateRole" method="POST" use:enhance>
                      <input type="hidden" name="userId" value={user.id} />
                      {#if user.role === 'admin'}
                        <input type="hidden" name="role" value="user" />
                        <Button type="submit" variant="ghost" size="sm" class="h-8 gap-2 italic hover:bg-orange-500/10 text-orange-500 hover:text-orange-500">
                          <UserMinus size={14} />
                          Demote
                        </Button>
                      {:else}
                        <input type="hidden" name="role" value="admin" />
                        <Button type="submit" variant="ghost" size="sm" class="h-8 gap-2 italic hover:bg-emerald-500/10 text-emerald-500 hover:text-emerald-500">
                          <ShieldCheck size={14} />
                          Promote
                        </Button>
                      {/if}
                    </form>
                    
                    <form action="?/updateRole" method="POST" use:enhance>
                      <input type="hidden" name="userId" value={user.id} />
                      {#if user.role === 'blocked'}
                        <input type="hidden" name="role" value="user" />
                        <Button type="submit" variant="ghost" size="sm" class="h-8 gap-2 italic">
                          <UserPlus size={14} />
                          Unblock
                        </Button>
                      {:else}
                        <input type="hidden" name="role" value="blocked" />
                        <Button type="submit" variant="ghost" size="sm" class="h-8 gap-2 italic hover:bg-destructive/10 text-destructive hover:text-destructive">
                          <ShieldX size={14} />
                          Block
                        </Button>
                      {/if}
                    </form>
                    
                    <form action="?/deleteUser" method="POST" use:enhance>
                      <input type="hidden" name="userId" value={user.id} />
                      <Button type="submit" variant="ghost" size="icon" class="h-8 w-8 text-muted-foreground hover:text-destructive transition-colors">
                        <Trash2 size={16} />
                      </Button>
                    </form>
                  </div>
                {:else}
                    <span class="text-[10px] italic text-muted-foreground pr-2">(Current User)</span>
                {/if}
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </Table.Root>
    </Card.Content>
  </Card.Root>

  <div class="bg-muted/20 border border-dashed rounded-lg p-4 flex gap-4">
    <ShieldAlert class="w-5 h-5 text-muted-foreground shrink-0 mt-0.5" />
    <div class="space-y-1">
      <h4 class="text-sm font-bold italic">Administrative Safety</h4>
      <p class="text-xs text-muted-foreground italic leading-relaxed">
        Only <strong>Admins</strong> can access this page and manage permissions. Initial admins are bootstrapped from the <code>ALLOWED_USERS</code> environment variable upon their first login. Once registered, their role is permanently stored in the database.
      </p>
    </div>
  </div>
</div>
