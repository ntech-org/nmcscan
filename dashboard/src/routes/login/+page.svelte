<script lang="ts">
  import { signIn } from "@auth/sveltekit/client";
  import * as Card from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";

  let loading = $state("");

  async function handleSignIn(provider: string) {
    loading = provider;
    await signIn(provider, { callbackUrl: '/admin' });
  }
</script>

<div class="flex items-center justify-center min-h-screen bg-muted/40 p-4">
  <Card.Root class="w-full max-w-sm">
    <Card.Header class="space-y-1 text-center">
      <Card.Title class="text-2xl font-bold tracking-tight">NMCScan Dashboard</Card.Title>
      <Card.Description>Sign in to access the Minecraft scanner</Card.Description>
    </Card.Header>
    <Card.Content class="grid gap-4">
      <Button 
        variant="outline" 
        class="w-full gap-2" 
        onclick={() => handleSignIn('discord')}
        disabled={loading !== ""}
      >
        <span class="i-simple-icons-discord w-5 h-5"></span>
        {loading === 'discord' ? "Connecting..." : "Continue with Discord"}
      </Button>
      <Button 
        variant="outline" 
        class="w-full gap-2" 
        onclick={() => handleSignIn('github')}
        disabled={loading !== ""}
      >
        <span class="i-simple-icons-github w-5 h-5"></span>
        {loading === 'github' ? "Connecting..." : "Continue with GitHub"}
      </Button>
    </Card.Content>
    <Card.Footer>
      <p class="text-xs text-center text-muted-foreground w-full">
        Private dashboard. Unauthorized access is logged.
      </p>
    </Card.Footer>
  </Card.Root>
</div>
