<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { browser } from '$app/environment';
	import {
		Chart,
		Title,
		Tooltip,
		Legend,
		LineElement,
		LinearScale,
		PointElement,
		CategoryScale,
		LineController,
		Filler
	} from 'chart.js';

	Chart.register(
		Title,
		Tooltip,
		Legend,
		LineElement,
		LinearScale,
		PointElement,
		CategoryScale,
		LineController,
		Filler
	);

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

	interface PlayerResponse {
		ip: string;
		player_name: string;
		last_seen: string;
	}

	interface HistoryResponse {
		timestamp: string;
		players_online: number;
	}

	// State
	let apiKey = $state(browser ? localStorage.getItem('nmcscan_api_key') || '' : '');
	let isAuthenticated = $state(false);
	
	let stats = $state<Stats | null>(null);
	let servers = $state<Server[]>([]);
	let asns = $state<Asn[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let selectedTab = $state<'overview' | 'servers' | 'asns' | 'players'>('overview');

	// Search and Filtering
	let searchQuery = $state('');
	let serverStatusFilter = $state('all');
	
	// Player Search
	let playerSearchQuery = $state('');
	let playerSearchResults = $state<PlayerResponse[]>([]);
	let playerSearchLoading = $state(false);

	// Server History
	let selectedServerHistory = $state<{ip: string, data: HistoryResponse[]} | null>(null);
	let historyLoading = $state(false);
	let chartCanvas = $state<HTMLCanvasElement | null>(null);
	let chart: Chart | null = null;

	let testScanning = $state(false);
	let testScanResult = $state<{ status: string; servers_added: number } | null>(null);
	let testScanRegion = $state('quick');

	let refreshInterval: ReturnType<typeof setInterval>;

	const API_BASE = browser ? '' : 'http://localhost:3000';

	async function fetchWithAuth(url: string, options: RequestInit = {}) {
		const headers = new Headers(options.headers);
		if (apiKey) {
			headers.set('X-API-Key', apiKey);
		}
		const res = await fetch(url, { ...options, headers });
		if (res.status === 401) {
			isAuthenticated = false;
			throw new Error('Unauthorized: Invalid API Key');
		}
		if (!res.ok) throw new Error(`HTTP Error: ${res.status}`);
		return res;
	}

	async function login() {
		try {
			error = null;
			loading = true;
			if (browser) localStorage.setItem('nmcscan_api_key', apiKey);
			
			// Test auth by fetching stats
			const res = await fetchWithAuth(`${API_BASE}/api/stats`);
			stats = await res.json();
			isAuthenticated = true;
			loadData();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Login failed';
		} finally {
			loading = false;
		}
	}

	function logout() {
		apiKey = '';
		isAuthenticated = false;
		if (browser) localStorage.removeItem('nmcscan_api_key');
		clearInterval(refreshInterval);
	}

	async function loadData() {
		if (!isAuthenticated) return;
		try {
			const [statsRes, asnsRes] = await Promise.all([
				fetchWithAuth(`${API_BASE}/api/stats`),
				fetchWithAuth(`${API_BASE}/api/asns`),
			]);
			stats = await statsRes.json();
			asns = await asnsRes.json();
			
			await searchServers();
		} catch (e) {
			if (e instanceof Error && e.message.includes('Unauthorized')) {
				isAuthenticated = false;
			}
			console.error("Data load error:", e);
		}
	}

	async function searchServers() {
		if (!isAuthenticated) return;
		loading = true;
		try {
			let url = `${API_BASE}/api/servers?limit=100`;
			if (searchQuery) url += `&search=${encodeURIComponent(searchQuery)}`;
			if (serverStatusFilter !== 'all') url += `&status=${serverStatusFilter}`;

			const res = await fetchWithAuth(url);
			servers = await res.json();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Search failed';
		} finally {
			loading = false;
		}
	}

	async function searchPlayers() {
		if (!isAuthenticated || playerSearchQuery.length < 3) return;
		playerSearchLoading = true;
		try {
			const res = await fetchWithAuth(`${API_BASE}/api/players?name=${encodeURIComponent(playerSearchQuery)}`);
			playerSearchResults = await res.json();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Player search failed';
		} finally {
			playerSearchLoading = false;
		}
	}

	async function fetchServerHistory(ip: string) {
		historyLoading = true;
		selectedServerHistory = null;
		try {
			const res = await fetchWithAuth(`${API_BASE}/api/server/${ip}/history`);
			const data = await res.json();
			selectedServerHistory = { ip, data: data.reverse() }; // Reverse to chronological order
		} catch (e) {
			error = e instanceof Error ? e.message : 'History fetch failed';
		} finally {
			historyLoading = false;
		}
	}

	onMount(() => {
		if (apiKey) {
			login(); // Auto-login if key exists
		}
		refreshInterval = setInterval(() => {
			if (isAuthenticated && selectedTab !== 'players') loadData();
		}, 30000);

		return () => clearInterval(refreshInterval);
	});

	$effect(() => {
		if (selectedServerHistory && chartCanvas) {
			if (chart) chart.destroy();
			
			const labels = selectedServerHistory.data.map(d => 
				new Date(d.timestamp).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit', month: 'short', day: 'numeric'})
			);
			const values = selectedServerHistory.data.map(d => d.players_online);

			chart = new Chart(chartCanvas, {
				type: 'line',
				data: {
					labels,
					datasets: [{
						label: 'Players Online',
						data: values,
						borderColor: '#3b82f6',
						backgroundColor: 'rgba(59, 130, 246, 0.1)',
						tension: 0.4,
						fill: true,
						pointRadius: 3,
						pointBackgroundColor: '#3b82f6',
						borderWidth: 2
					}]
				},
				options: {
					responsive: true,
					maintainAspectRatio: false,
					scales: {
						y: { 
							beginAtZero: true,
							grid: { color: 'rgba(255, 255, 255, 0.05)' },
							ticks: { color: '#9ca3af' }
						},
						x: {
							grid: { display: false },
							ticks: { color: '#9ca3af', maxRotation: 45, minRotation: 45 }
						}
					},
					plugins: {
						legend: { display: false },
						tooltip: {
							backgroundColor: 'rgba(17, 24, 39, 0.9)',
							titleColor: '#fff',
							bodyColor: '#60a5fa',
							borderColor: 'rgba(55, 65, 81, 1)',
							borderWidth: 1,
							padding: 12,
							displayColors: false
						}
					},
					interaction: { mode: 'index', intersect: false }
				}
			});
		}
	});

	function formatLastSeen(dateStr: string | null): string {
		if (!dateStr) return 'Never';
		return new Date(dateStr).toLocaleString();
	}

	async function triggerTestScan() {
		testScanning = true;
		testScanResult = null;
		try {
			const payload: any = {};
			if (testScanRegion === 'quick') payload.quick = true;
			else if (testScanRegion !== 'default') payload.region = testScanRegion;

			const res = await fetchWithAuth('/api/scan/test', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(payload)
			});
			testScanResult = await res.json();
			setTimeout(() => loadData(), 2000);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to trigger test scan';
		} finally {
			testScanning = false;
		}
	}

	function getStatusColor(status: string): string {
		switch (status) {
			case 'online': return 'text-green-400 bg-green-400/10 border-green-500/20';
			case 'offline': return 'text-red-400 bg-red-400/10 border-red-500/20';
			default: return 'text-gray-400 bg-gray-400/10 border-gray-500/20';
		}
	}

	function getCategoryColor(category: string): string {
		if (category.includes('Hosting')) return 'text-blue-400 bg-blue-400/10 border-blue-500/20';
		if (category.includes('Residential')) return 'text-orange-400 bg-orange-400/10 border-orange-500/20';
		return 'text-gray-400 bg-gray-400/10 border-gray-500/20';
	}
	
	// Run search when query changes but debounced
	let searchTimeout: ReturnType<typeof setTimeout>;
	function onServerSearchInput() {
		clearTimeout(searchTimeout);
		searchTimeout = setTimeout(searchServers, 500);
	}
</script>

<svelte:head>
	<title>NMCScan Dashboard</title>
</svelte:head>

<div class="min-h-screen bg-[#0b0f19] text-gray-200 font-sans selection:bg-blue-500/30">
	{#if !isAuthenticated}
		<div class="flex items-center justify-center min-h-screen bg-[#0b0f19] bg-[radial-gradient(ellipse_at_center,_var(--tw-gradient-stops))] from-blue-900/20 via-[#0b0f19] to-[#0b0f19]">
			<div class="bg-gray-900/80 backdrop-blur-xl border border-gray-800 p-8 rounded-2xl shadow-2xl w-full max-w-md">
				<div class="text-center mb-8">
					<div class="inline-flex items-center justify-center w-16 h-16 rounded-full bg-blue-500/10 text-blue-400 mb-4 shadow-[0_0_30px_rgba(59,130,246,0.3)]">
						<svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"></path></svg>
					</div>
					<h1 class="text-3xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-cyan-300">NMCScan</h1>
					<p class="text-gray-400 mt-2">Secure access required</p>
				</div>
				
				<form onsubmit={(e) => { e.preventDefault(); login(); }} class="space-y-4">
					<div>
						<label for="apiKey" class="block text-sm font-medium text-gray-400 mb-1">API Key</label>
						<input
							type="password"
							id="apiKey"
							bind:value={apiKey}
							class="w-full bg-gray-950/50 border border-gray-700 rounded-lg px-4 py-3 text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all"
							placeholder="Enter your secure key"
							required
						/>
					</div>
					{#if error}
						<div class="text-red-400 text-sm p-3 bg-red-400/10 rounded-lg border border-red-400/20">{error}</div>
					{/if}
					<button
						type="submit"
						disabled={loading}
						class="w-full bg-blue-600 hover:bg-blue-500 disabled:bg-blue-800 disabled:opacity-50 text-white font-medium py-3 rounded-lg transition-all shadow-lg hover:shadow-blue-500/25 active:scale-[0.98]"
					>
						{loading ? 'Authenticating...' : 'Access Dashboard'}
					</button>
				</form>
			</div>
		</div>
	{:else}
		<!-- Header -->
		<header class="bg-gray-900/50 backdrop-blur-md border-b border-gray-800 sticky top-0 z-40">
			<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
				<div class="flex items-center justify-between h-16">
					<div class="flex items-center gap-3">
						<div class="w-8 h-8 rounded bg-gradient-to-br from-blue-500 to-cyan-400 flex items-center justify-center shadow-lg shadow-blue-500/30">
							<svg class="w-5 h-5 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path></svg>
						</div>
						<h1 class="text-xl font-bold tracking-tight text-white">NMCScan</h1>
					</div>
					
					<div class="flex items-center gap-6">
						<div class="flex items-center gap-2">
							<span class="relative flex h-3 w-3">
							  <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
							  <span class="relative inline-flex rounded-full h-3 w-3 bg-green-500"></span>
							</span>
							<span class="text-sm font-medium text-gray-300">Live</span>
						</div>
						<button onclick={logout} class="text-gray-400 hover:text-white transition-colors text-sm font-medium px-3 py-1.5 rounded-md hover:bg-gray-800">
							Logout
						</button>
					</div>
				</div>
			</div>
			
			<!-- Navigation Tabs -->
			<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
				<div class="flex space-x-1">
					{#each ['overview', 'servers', 'players', 'asns'] as tab}
						<button
							class="px-4 py-3 text-sm font-medium transition-all relative {selectedTab === tab ? 'text-blue-400' : 'text-gray-400 hover:text-gray-200 hover:bg-gray-800/50 rounded-t-lg'}"
							onclick={() => selectedTab = tab as any}
						>
							{tab.charAt(0).toUpperCase() + tab.slice(1)}
							{#if selectedTab === tab}
								<span class="absolute bottom-0 left-0 w-full h-0.5 bg-blue-500 rounded-t-full shadow-[0_-2px_10px_rgba(59,130,246,0.5)]"></span>
							{/if}
						</button>
					{/each}
				</div>
			</div>
		</header>
		
		<!-- Main Content -->
		<main class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
			{#if selectedTab === 'overview'}
				<div class="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
					<!-- Stats Grid -->
					<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
						<div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
							<div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
								<svg class="w-12 h-12 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"></path></svg>
							</div>
							<div class="text-gray-400 font-medium text-sm">Discovered Servers</div>
							<div class="text-4xl font-bold text-white mt-2 tracking-tight">{stats?.total_servers?.toLocaleString() ?? 0}</div>
						</div>
						<div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
							<div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
								<svg class="w-12 h-12 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5.121 17.804A13.937 13.937 0 0112 16c2.5 0 4.847.655 6.879 1.804M15 10a3 3 0 11-6 0 3 3 0 016 0zm6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>
							</div>
							<div class="text-gray-400 font-medium text-sm">Online Right Now</div>
							<div class="text-4xl font-bold text-green-400 mt-2 tracking-tight">{stats?.online_servers?.toLocaleString() ?? 0}</div>
						</div>
						<div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
							<div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
								<svg class="w-12 h-12 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"></path></svg>
							</div>
							<div class="text-gray-400 font-medium text-sm">Players Online</div>
							<div class="text-4xl font-bold text-purple-400 mt-2 tracking-tight">{stats?.total_players?.toLocaleString() ?? 0}</div>
						</div>
						<div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
							<div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
								<svg class="w-12 h-12 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"></path></svg>
							</div>
							<div class="text-gray-400 font-medium text-sm">Hosting ASNs Tracked</div>
							<div class="text-4xl font-bold text-orange-400 mt-2 tracking-tight">{stats?.asn_hosting?.toLocaleString() ?? 0}</div>
						</div>
					</div>

					<!-- Test Scan Panel -->
					<div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
						<div class="flex items-center justify-between flex-wrap gap-4">
							<div>
								<h2 class="text-lg font-semibold text-white">Manual Scanner Override</h2>
								<p class="text-gray-400 text-sm mt-1">Force an immediate scan on specific network segments.</p>
							</div>
							<div class="flex items-center gap-3">
								<select
									class="bg-gray-950 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
									bind:value={testScanRegion}
								>
									<option value="quick">Quick Test (10 servers)</option>
									<option value="default">All Known Servers (50)</option>
									<option value="us">US Servers</option>
									<option value="eu">EU Servers</option>
								</select>
								<button
									class="px-5 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded-lg text-sm font-medium transition-all shadow-md active:scale-95 flex items-center gap-2"
									onclick={triggerTestScan}
									disabled={testScanning}
								>
									{#if testScanning}
										<svg class="animate-spin h-4 w-4 text-white" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
										Deploying...
									{:else}
										<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path></svg>
										Execute Scan
									{/if}
								</button>
							</div>
						</div>
						{#if testScanResult}
							<div class="mt-4 p-3 bg-green-500/10 border border-green-500/20 text-green-400 rounded-lg text-sm flex items-center gap-2">
								<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
								Successfully dispatched tasks for {testScanResult.servers_added} servers.
							</div>
						{/if}
					</div>
				</div>
			{/if}

			{#if selectedTab === 'servers'}
				<div class="animate-in fade-in slide-in-from-bottom-4 duration-500">
					<!-- Filter/Search Bar -->
					<div class="bg-gray-900 border border-gray-800 rounded-t-xl p-4 flex flex-col sm:flex-row gap-4 justify-between items-center">
						<div class="relative w-full sm:w-96">
							<div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
								<svg class="h-5 w-5 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"></path></svg>
							</div>
							<input
								type="text"
								placeholder="Search IP, MOTD, or Version..."
								bind:value={searchQuery}
								oninput={onServerSearchInput}
								class="w-full bg-gray-950 border border-gray-700 rounded-lg pl-10 pr-4 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all"
							/>
						</div>
						<div class="flex gap-2 w-full sm:w-auto">
							<select
								bind:value={serverStatusFilter}
								onchange={searchServers}
								class="w-full sm:w-auto bg-gray-950 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
							>
								<option value="all">All Statuses</option>
								<option value="online">Online Only</option>
								<option value="offline">Offline Only</option>
							</select>
							<button onclick={searchServers} class="p-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 transition-colors" aria-label="Refresh">
								<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
							</button>
						</div>
					</div>

					<div class="bg-gray-900 border-x border-b border-gray-800 rounded-b-xl overflow-hidden">
						<div class="overflow-x-auto">
							<table class="w-full text-left border-collapse">
								<thead>
									<tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider">
										<th class="p-4 font-medium">Server Address</th>
										<th class="p-4 font-medium">Status</th>
										<th class="p-4 font-medium">Players</th>
										<th class="p-4 font-medium">Version</th>
										<th class="p-4 font-medium">MOTD</th>
										<th class="p-4 font-medium text-right">Actions</th>
									</tr>
								</thead>
								<tbody class="divide-y divide-gray-800/50">
									{#if loading && servers.length === 0}
										<tr><td colspan="6" class="p-8 text-center text-gray-500">Searching the subnet...</td></tr>
									{:else if servers.length === 0}
										<tr><td colspan="6" class="p-8 text-center text-gray-500">No servers found matching criteria.</td></tr>
									{/if}
									{#each servers as server}
										<tr class="hover:bg-gray-800/20 transition-colors group">
											<td class="p-4">
												<div class="font-mono text-sm text-gray-200">{server.ip}</div>
												<div class="text-xs text-gray-500">Port {server.port}</div>
											</td>
											<td class="p-4">
												<span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border {getStatusColor(server.status)}">
													<span class="w-1.5 h-1.5 rounded-full mr-1.5 {server.status === 'online' ? 'bg-green-400' : 'bg-gray-400'}"></span>
													{server.status}
												</span>
											</td>
											<td class="p-4 text-sm">
												<div class="flex items-center gap-2">
													<div class="w-16 h-1.5 bg-gray-800 rounded-full overflow-hidden">
														<div 
															class="h-full bg-blue-500 rounded-full" 
															style="width: {server.players_max > 0 ? Math.min((server.players_online / server.players_max) * 100, 100) : 0}%"
														></div>
													</div>
													<span class="{server.players_online > 0 ? 'text-white' : 'text-gray-500'} font-medium">{server.players_online}</span>
													<span class="text-gray-600">/ {server.players_max}</span>
												</div>
											</td>
											<td class="p-4">
												<span class="inline-block px-2 py-1 bg-gray-800 text-gray-300 rounded text-xs truncate max-w-[120px]" title={server.version || 'Unknown'}>
													{server.version || 'Unknown'}
												</span>
											</td>
											<td class="p-4 text-sm text-gray-400 max-w-xs truncate" title={server.motd || ''}>
												{server.motd || '-'}
											</td>
											<td class="p-4 text-right">
												<button 
													onclick={() => fetchServerHistory(server.ip)}
													class="opacity-0 group-hover:opacity-100 transition-opacity px-3 py-1.5 bg-blue-600/10 hover:bg-blue-600/20 text-blue-400 rounded text-xs font-medium border border-blue-500/20"
												>
													View History
												</button>
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					</div>
				</div>
			{/if}

			{#if selectedTab === 'players'}
				<div class="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
					<div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
						<h2 class="text-xl font-bold text-white mb-2">Global Player Search</h2>
						<p class="text-gray-400 text-sm mb-6">Track player sightings across all scanned networks.</p>
						
						<div class="flex gap-4">
							<div class="relative flex-1">
								<div class="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
									<svg class="h-5 w-5 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"></path></svg>
								</div>
								<input
									type="text"
									placeholder="Enter player name (min 3 chars)..."
									bind:value={playerSearchQuery}
									onkeydown={(e) => e.key === 'Enter' && searchPlayers()}
									class="w-full bg-gray-950 border border-gray-700 rounded-lg pl-11 pr-4 py-3 text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all shadow-inner"
								/>
							</div>
							<button 
								onclick={searchPlayers}
								disabled={playerSearchLoading || playerSearchQuery.length < 3}
								class="px-6 py-3 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:hover:bg-blue-600 rounded-lg text-white font-medium transition-all shadow-md flex items-center gap-2"
							>
								{#if playerSearchLoading}
									<svg class="animate-spin h-5 w-5 text-white" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
									Searching
								{:else}
									Search
								{/if}
							</button>
						</div>
					</div>

					{#if playerSearchResults.length > 0}
						<div class="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden">
							<table class="w-full text-left border-collapse">
								<thead>
									<tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider">
										<th class="p-4 font-medium">Player Name</th>
										<th class="p-4 font-medium">Server IP</th>
										<th class="p-4 font-medium">Last Seen</th>
										<th class="p-4 font-medium text-right">Actions</th>
									</tr>
								</thead>
								<tbody class="divide-y divide-gray-800/50">
									{#each playerSearchResults as player}
										<tr class="hover:bg-gray-800/20 transition-colors">
											<td class="p-4 font-medium text-white flex items-center gap-3">
												<img src={`https://minotar.net/helm/${player.player_name}/32.png`} alt={player.player_name} class="w-8 h-8 rounded" onerror={(e) => { (e.currentTarget as HTMLImageElement).src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><rect width="24" height="24" fill="%23333"/><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="%23666"/></svg>'; }} />
												{player.player_name}
											</td>
											<td class="p-4 font-mono text-sm text-blue-400">{player.ip}</td>
											<td class="p-4 text-sm text-gray-400">{formatLastSeen(player.last_seen)}</td>
											<td class="p-4 text-right">
												<button 
													onclick={() => { searchQuery = player.ip; selectedTab = 'servers'; searchServers(); }}
													class="px-3 py-1.5 bg-gray-800 hover:bg-gray-700 text-gray-300 rounded text-xs font-medium transition-colors"
												>
													View Server
												</button>
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					{:else if playerSearchQuery.length >= 3 && !playerSearchLoading && playerSearchResults.length === 0}
						<div class="bg-gray-900 border border-gray-800 rounded-xl p-12 text-center text-gray-500">
							<svg class="w-16 h-16 mx-auto mb-4 opacity-20" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"></path></svg>
							<p>No records found for player "{playerSearchQuery}"</p>
						</div>
					{/if}
				</div>
			{/if}

			{#if selectedTab === 'asns'}
				<div class="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden animate-in fade-in slide-in-from-bottom-4 duration-500">
					<div class="p-4 border-b border-gray-800 bg-gray-950/30 flex justify-between items-center">
						<h2 class="text-lg font-semibold text-white">Network Topology Map</h2>
					</div>
					<div class="overflow-x-auto">
						<table class="w-full text-left border-collapse">
							<thead>
								<tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider">
									<th class="p-4 font-medium">ASN</th>
									<th class="p-4 font-medium">Organization</th>
									<th class="p-4 font-medium">Classification</th>
									<th class="p-4 font-medium text-center">Country</th>
								</tr>
							</thead>
							<tbody class="divide-y divide-gray-800/50">
								{#each asns as asn}
									<tr class="hover:bg-gray-800/20 transition-colors">
										<td class="p-4 font-mono text-sm text-blue-400">{asn.asn}</td>
										<td class="p-4 text-sm text-gray-200">{asn.org}</td>
										<td class="p-4">
											<span class="inline-flex items-center px-2.5 py-0.5 rounded border {getCategoryColor(asn.category)} text-xs font-medium">
												{asn.category}
											</span>
										</td>
										<td class="p-4 text-center">
											{#if asn.country}
												<img src={`https://flagcdn.com/24x18/${asn.country.toLowerCase()}.png`} alt={asn.country} class="inline-block rounded shadow-sm opacity-80" />
											{:else}
												<span class="text-gray-500">-</span>
											{/if}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			{/if}
		</main>
	{/if}

	<!-- History Modal -->
	{#if selectedServerHistory}
		<div class="fixed inset-0 bg-black/80 backdrop-blur-sm z-50 flex items-center justify-center p-4 animate-in fade-in duration-200">
			<div class="bg-gray-900 border border-gray-800 rounded-2xl w-full max-w-4xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh]">
				<div class="p-5 border-b border-gray-800 flex justify-between items-center bg-gray-950/50">
					<div>
						<h3 class="text-lg font-bold text-white">Telemetry Data</h3>
						<p class="text-sm text-gray-400 font-mono mt-1">{selectedServerHistory.ip}</p>
					</div>
					<button 
						onclick={() => selectedServerHistory = null}
						class="p-2 text-gray-400 hover:text-white hover:bg-gray-800 rounded-lg transition-colors"
						aria-label="Close"
					>
						<svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg>
					</button>
				</div>
				<div class="p-6 flex-1 overflow-y-auto">
					{#if historyLoading}
						<div class="flex justify-center py-20">
							<svg class="animate-spin h-8 w-8 text-blue-500" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
						</div>
					{:else if selectedServerHistory.data.length > 0}
						<div class="h-80 w-full">
							<canvas bind:this={chartCanvas}></canvas>
						</div>
					{:else}
						<div class="text-center py-20 text-gray-500">
							Insufficient historical data available for this target.
						</div>
					{/if}
				</div>
			</div>
		</div>
	{/if}
</div>
