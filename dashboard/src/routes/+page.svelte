<script lang="ts">
  import Dashboard from "$lib/components/Dashboard.svelte";
  import Landing from "$lib/components/Landing.svelte";

  let { data } = $props();
  const session = $derived(data.session);
</script>

<svelte:head>
  <title>NMCScan - {session ? 'Dashboard' : 'Research Project'}</title>
</svelte:head>

{#if session}
  <Dashboard />
{:else}
  <Landing />
{/if}
