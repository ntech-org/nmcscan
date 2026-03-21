<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	
	// Types
	interface Stats {
		total_servers: number;
		online_servers: number;
		total_players: number;
		asn_hosting: number;
		asn_residential: number;
		asn_unknown: number;
	}
	
	interface Server {
		ip: string;
		port: number;
		status: string;
		players_online: number;
		players_max: number;
		motd: string | null;
		version: string | null;
		priority: number;
		last_seen: string | null;
		consecutive_failures: number;
	}
	
	interface Asn {
		asn: string;
		org: string;
		category: string;
		country: string | null;
		server_count: number;
	}
	
	// Reactive state using Svelte 5 runes
	let stats = $state<Stats | null>(null);
	let servers = $state<Server[]>([]);
	let asns = $state<Asn[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let selectedTab = $state<'overview' | 'servers' | 'asns'>('overview');
	
	// Test scan state
	let testScanning = $state(false);
	let testScanResult = $state<{status: string, servers_added: number} | null>(null);
	let testScanRegion = $state('quick');
	
	// Fetch data from API
	async function loadData() {
		try {
			const API_BASE = browser ? '' : 'http://localhost:3000';
			
			const [statsRes, serversRes, asnsRes] = await Promise.all([
				fetch(`${API_BASE}/api/stats`),
				fetch(`${API_BASE}/api/servers?limit=100`),
				fetch(`${API_BASE}/api/asns`)
			]);
			
			if (!statsRes.ok) throw new Error('Failed to fetch stats');
			if (!serversRes.ok) throw new Error('Failed to fetch servers');
			if (!asnsRes.ok) throw new Error('Failed to fetch ASNs');
			
			stats = await statsRes.json();
			servers = await serversRes.json();
			asns = await asnsRes.json();
			
			loading = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
			loading = false;
		}
	}
	
	// Auto-refresh every 30 seconds
	let refreshInterval: ReturnType<typeof setInterval>;
	
	onMount(() => {
		loadData();
		refreshInterval = setInterval(loadData, 30000);
		
		return () => {
			clearInterval(refreshInterval);
		};
	});
	
	// Format last seen time
	function formatLastSeen(dateStr: string | null): string {
		if (!dateStr) return 'Never';
		const date = new Date(dateStr);
		return date.toLocaleString();
	}
	
	// Trigger test scan
	async function triggerTestScan() {
		testScanning = true;
		testScanResult = null;
		
		try {
			const payload: any = {};
			if (testScanRegion === 'quick') {
				payload.quick = true;
			} else if (testScanRegion !== 'default') {
				payload.region = testScanRegion;
			}
			
			const res = await fetch('/api/scan/test', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(payload)
			});
			
			if (!res.ok) throw new Error('Failed to trigger test scan');
			
			testScanResult = await res.json();
			
			// Reload data after a short delay
			setTimeout(() => loadData(), 2000);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to trigger test scan';
		} finally {
			testScanning = false;
		}
	}
	
	// Get status color
	function getStatusColor(status: string): string {
		switch (status) {
			case 'online': return 'text-green-400 bg-green-400/10';
			case 'offline': return 'text-red-400 bg-red-400/10';
			default: return 'text-gray-400 bg-gray-400/10';
		}
	}
	
	// Get category color
	function getCategoryColor(category: string): string {
		if (category.includes('Hosting')) return 'text-blue-400 bg-blue-400/10';
		if (category.includes('Residential')) return 'text-orange-400 bg-orange-400/10';
		return 'text-gray-400 bg-gray-400/10';
	}
	
	// Calculate ASN distribution for chart
	function getAsnDistribution() {
		if (!stats) return { hosting: 0, residential: 0, unknown: 0 };
		return {
			hosting: stats.asn_hosting,
			residential: stats.asn_residential,
			unknown: stats.asn_unknown
		};
	}
</script>

<svelte:head>
	<title>NMCScan Dashboard</title>
</svelte:head>

<div class="min-h-screen bg-gray-900">
	<!-- Header -->
	<header class="bg-gray-800 border-b border-gray-700">
		<div class="max-w-7xl mx-auto px-4 py-4">
			<div class="flex items-center justify-between">
				<h1 class="text-2xl font-bold text-blue-400">🎮 NMCScan Dashboard</h1>
				<div class="flex items-center gap-4">
					{#if loading}
						<span class="text-gray-400 animate-pulse">Loading...</span>
					{:else if error}
						<span class="text-red-400">Error: {error}</span>
					{:else}
						<span class="text-green-400">● Live</span>
					{/if}
				</div>
			</div>
			
			<!-- Test Scan Controls -->
			<div class="mt-4 flex items-center gap-3 flex-wrap">
				<span class="text-gray-400 text-sm">Test Mode:</span>
				<select
					class="bg-gray-700 border border-gray-600 rounded px-3 py-1 text-sm text-gray-200 focus:border-blue-400 focus:outline-none"
					bind:value={testScanRegion}
				>
					<option value="quick">Quick Test (10 servers)</option>
					<option value="default">All Known Servers (50)</option>
					<option value="us">US Servers</option>
					<option value="eu">EU Servers</option>
					<option value="uk">UK Servers</option>
					<option value="au">AU Servers</option>
					<option value="br">BR Servers</option>
					<option value="asia">Asia Servers</option>
				</select>
				<button
					class="px-4 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-600 rounded text-sm font-medium transition-colors flex items-center gap-2"
					onclick={triggerTestScan}
					disabled={testScanning}
				>
					{#if testScanning}
						<span class="animate-spin">⏳</span>
						Scanning...
					{:else}
						<span>🧪</span>
						Run Test Scan
					{/if}
				</button>
				{#if testScanResult}
					<span class="text-green-400 text-sm">
						✅ Added {testScanResult.servers_added} servers
					</span>
				{/if}
			</div>
		</div>
	</header>
	
	<!-- Navigation Tabs -->
	<nav class="bg-gray-800/50 border-b border-gray-700">
		<div class="max-w-7xl mx-auto px-4">
			<div class="flex gap-4">
				<button
					class="px-4 py-3 font-medium transition-colors {selectedTab === 'overview' ? 'text-blue-400 border-b-2 border-blue-400' : 'text-gray-400 hover:text-gray-200'}"
					onclick={() => selectedTab = 'overview'}
				>
					Overview
				</button>
				<button
					class="px-4 py-3 font-medium transition-colors {selectedTab === 'servers' ? 'text-blue-400 border-b-2 border-blue-400' : 'text-gray-400 hover:text-gray-200'}"
					onclick={() => selectedTab = 'servers'}
				>
					Servers
				</button>
				<button
					class="px-4 py-3 font-medium transition-colors {selectedTab === 'asns' ? 'text-blue-400 border-b-2 border-blue-400' : 'text-gray-400 hover:text-gray-200'}"
					onclick={() => selectedTab = 'asns'}
				>
					ASNs
				</button>
			</div>
		</div>
	</nav>
	
	<!-- Main Content -->
	<main class="max-w-7xl mx-auto px-4 py-6">
		{#if loading}
			<div class="flex items-center justify-center py-20">
				<div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-400"></div>
			</div>
		{:else if error}
			<div class="bg-red-400/10 border border-red-400 text-red-400 px-4 py-3 rounded">
				{error}
			</div>
		{:else}
			<!-- Overview Tab -->
			{#if selectedTab === 'overview'}
				<div class="space-y-6">
					<!-- Stats Cards -->
					<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
						<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
							<div class="text-gray-400 text-sm">Total Servers</div>
							<div class="text-3xl font-bold text-blue-400 mt-1">{stats?.total_servers ?? 0}</div>
						</div>
						<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
							<div class="text-gray-400 text-sm">Online Servers</div>
							<div class="text-3xl font-bold text-green-400 mt-1">{stats?.online_servers ?? 0}</div>
						</div>
						<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
							<div class="text-gray-400 text-sm">Total Players</div>
							<div class="text-3xl font-bold text-purple-400 mt-1">{stats?.total_players ?? 0}</div>
						</div>
						<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
							<div class="text-gray-400 text-sm">Hosting ASNs</div>
							<div class="text-3xl font-bold text-orange-400 mt-1">{stats?.asn_hosting ?? 0}</div>
						</div>
					</div>
					
					<!-- ASN Distribution -->
					<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
						<h2 class="text-xl font-semibold mb-4">ASN Distribution</h2>
						<div class="grid grid-cols-3 gap-4">
							<div class="text-center">
								<div class="text-2xl font-bold text-blue-400">{stats?.asn_hosting ?? 0}</div>
								<div class="text-gray-400 text-sm mt-1">Hosting</div>
							</div>
							<div class="text-center">
								<div class="text-2xl font-bold text-orange-400">{stats?.asn_residential ?? 0}</div>
								<div class="text-gray-400 text-sm mt-1">Residential</div>
							</div>
							<div class="text-center">
								<div class="text-2xl font-bold text-gray-400">{stats?.asn_unknown ?? 0}</div>
								<div class="text-gray-400 text-sm mt-1">Unknown</div>
							</div>
						</div>
					</div>
					
					<!-- Recent Servers -->
					<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
						<h2 class="text-xl font-semibold mb-4">Recent Servers</h2>
						<div class="overflow-x-auto">
							<table class="w-full">
								<thead>
									<tr class="text-left text-gray-400 text-sm border-b border-gray-700">
										<th class="pb-3 font-medium">IP Address</th>
										<th class="pb-3 font-medium">Status</th>
										<th class="pb-3 font-medium">Players</th>
										<th class="pb-3 font-medium">MOTD</th>
										<th class="pb-3 font-medium">Last Seen</th>
									</tr>
								</thead>
								<tbody>
									{#each servers.slice(0, 10) as server}
										<tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
											<td class="py-3 font-mono text-sm">{server.ip}:{server.port}</td>
											<td class="py-3">
												<span class="px-2 py-1 rounded text-xs font-medium {getStatusColor(server.status)}">
													{server.status}
												</span>
											</td>
											<td class="py-3">{server.players_online}/{server.players_max}</td>
											<td class="py-3 text-gray-400 text-sm truncate max-w-xs">{server.motd ?? '-'}</td>
											<td class="py-3 text-gray-400 text-sm">{formatLastSeen(server.last_seen)}</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					</div>
				</div>
			{/if}
			
			<!-- Servers Tab -->
			{#if selectedTab === 'servers'}
				<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
					<h2 class="text-xl font-semibold mb-4">All Servers</h2>
					<div class="overflow-x-auto">
						<table class="w-full">
							<thead>
								<tr class="text-left text-gray-400 text-sm border-b border-gray-700">
									<th class="pb-3 font-medium">IP Address</th>
									<th class="pb-3 font-medium">Status</th>
									<th class="pb-3 font-medium">Players</th>
									<th class="pb-3 font-medium">MOTD</th>
									<th class="pb-3 font-medium">Version</th>
									<th class="pb-3 font-medium">Last Seen</th>
								</tr>
							</thead>
							<tbody>
								{#each servers as server}
									<tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
										<td class="py-3 font-mono text-sm">{server.ip}:{server.port}</td>
										<td class="py-3">
											<span class="px-2 py-1 rounded text-xs font-medium {getStatusColor(server.status)}">
												{server.status}
											</span>
										</td>
										<td class="py-3">{server.players_online}/{server.players_max}</td>
										<td class="py-3 text-gray-400 text-sm truncate max-w-xs">{server.motd ?? '-'}</td>
										<td class="py-3 text-gray-400 text-sm">{server.version ?? '-'}</td>
										<td class="py-3 text-gray-400 text-sm">{formatLastSeen(server.last_seen)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			{/if}
			
			<!-- ASNs Tab -->
			{#if selectedTab === 'asns'}
				<div class="bg-gray-800 rounded-lg p-6 border border-gray-700">
					<h2 class="text-xl font-semibold mb-4">Autonomous Systems</h2>
					<div class="overflow-x-auto">
						<table class="w-full">
							<thead>
								<tr class="text-left text-gray-400 text-sm border-b border-gray-700">
									<th class="pb-3 font-medium">ASN</th>
									<th class="pb-3 font-medium">Organization</th>
									<th class="pb-3 font-medium">Category</th>
									<th class="pb-3 font-medium">Country</th>
									<th class="pb-3 font-medium">Server Count</th>
								</tr>
							</thead>
							<tbody>
								{#each asns as asn}
									<tr class="border-b border-gray-700/50 hover:bg-gray-700/30">
										<td class="py-3 font-mono text-sm text-blue-400">{asn.asn}</td>
										<td class="py-3">{asn.org}</td>
										<td class="py-3">
											<span class="px-2 py-1 rounded text-xs font-medium {getCategoryColor(asn.category)}">
												{asn.category}
											</span>
										</td>
										<td class="py-3 text-gray-400">{asn.country ?? '-'}</td>
										<td class="py-3">{asn.server_count}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			{/if}
		{/if}
	</main>
	
	<!-- Footer -->
	<footer class="border-t border-gray-700 mt-8">
		<div class="max-w-7xl mx-auto px-4 py-4 text-center text-gray-400 text-sm">
			NMCScan - High-performance Minecraft Server Scanner
		</div>
	</footer>
</div>
